//! L'image décodée : une poignée vers une surface qui vit sur le GPU.
//!
//! Décision D5 : le chemin normal ne ramène JAMAIS de pixels en mémoire
//! centrale. `ImageDecodee` ne porte qu'un pointeur de périphérique ; le rendu
//! (tâche 4, crate `sky-rendu`) le consomme tel quel. La seule fonction qui
//! copie est [`ImageDecodee::copier_vers_memoire_centrale`], réservée aux tests
//! et aux mesures.

use std::rc::Rc;

use cudarc::driver::sys::{cuMemcpy2D_v2, CUmemorytype, CUresult, CUDA_MEMCPY2D};

use crate::decodeur::SessionNvdec;

/// Poignée vers une surface NVDEC mappée, en YUV 4:4:4 8 bits.
///
/// C'est de la plomberie opaque : un pointeur de périphérique CUDA et le pas
/// de ligne, tels que `cuvidMapVideoFrame64` les a rendus. Les trois plans se
/// suivent verticalement dans la même allocation :
///
/// - Y à `pointeur`,
/// - U à `pointeur + pas × hauteur_surface`,
/// - V à `pointeur + 2 × pas × hauteur_surface`.
///
/// L'espacement des plans est `hauteur_surface`, qui est la hauteur de la
/// **surface de sortie** — celle que la session a demandée dans `ulTargetHeight`
/// — et **non** la hauteur codée du flux. Les deux diffèrent : mesuré le
/// 30/09/2026 en 1920×1080, où la hauteur codée vaut 1088 alors que la surface
/// mappée en fait 1080.
///
/// Ce n'est pas un détail, et ce n'est pas ce que suggère l'échantillon
/// `NvDecoder` de NVIDIA, qui calcule son offset de chrominance depuis
/// `coded_height` — mais lui met aussi `ulTargetHeight` à la taille codée. Ici la
/// cible est la taille d'affichage, et `cuvidMapVideoFrame64` rend la surface de
/// sortie, donc c'est la cible qui commande. Le vérifier a coûté une mesure :
/// avec 1088, `cuMemcpy2D` échoue sur le troisième plan (débordement) ; avec
/// 1080, l'aller-retour 1080 est exact au pixel. Voir
/// `tests/aller_retour.rs`, qui tient cette preuve.
///
/// Aucun octet de pixel ne traverse cette frontière : seul le GPU sait lire ce
/// pointeur.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceCuda {
    /// Pointeur de périphérique CUDA vers la première ligne du plan Y.
    pub pointeur: u64,
    /// Pas de ligne en octets, commun aux trois plans.
    pub pas: u32,
    /// Nombre de lignes séparant le début d'un plan du début du suivant : la
    /// hauteur de la surface de sortie mappée. Voir la note ci-dessus — ce n'est
    /// pas la hauteur codée du flux.
    pub hauteur_surface: u32,
}

/// Une image sortie du décodeur, encore sur le GPU.
///
/// Sa destruction démappe la surface : garder une image en vie immobilise une
/// surface de sortie de NVDEC, il faut donc la libérer vite.
pub struct ImageDecodee {
    pub largeur: u32,
    pub hauteur: u32,
    pub horodatage_ms: u64,
    surface: SurfaceCuda,
    hauteur_codee: u32,
    /// La session qui a produit la surface, gardée en vie pour que le
    /// démappage ait lieu même si l'image survit au [`crate::Decodeur`].
    session: Rc<SessionNvdec>,
}

impl ImageDecodee {
    pub(crate) fn nouvelle(
        session: Rc<SessionNvdec>,
        largeur: u32,
        hauteur: u32,
        hauteur_codee: u32,
        horodatage_ms: u64,
        surface: SurfaceCuda,
    ) -> Self {
        Self {
            largeur,
            hauteur,
            horodatage_ms,
            surface,
            hauteur_codee,
            session,
        }
    }

    /// La hauteur **codée** du flux, telle que NVDEC l'a annoncée.
    ///
    /// Rien du décodage ni du rendu n'en dépend : elle est exposée pour être
    /// observable. C'est la seule valeur qui vienne vraiment du flux et non de
    /// nos propres choix, donc la seule par laquelle un test peut prouver que la
    /// géométrie qu'il examine distingue réellement la hauteur codée de la
    /// hauteur d'affichage — et non comparer deux valeurs que ce crate a posées
    /// lui-même. Voir `tests/aller_retour.rs`.
    pub fn hauteur_codee(&self) -> u32 {
        self.hauteur_codee
    }

    /// La surface GPU de cette image.
    ///
    /// Accesseur et non champ : la tâche 4 vit dans un autre crate
    /// (`sky-rendu`) et ne peut pas atteindre un membre `pub(crate)`. Ce qui en
    /// sort reste une poignée de surface, jamais des octets (décision D5).
    pub fn surface(&self) -> SurfaceCuda {
        self.surface
    }

    /// RÉSERVÉ AUX TESTS ET AUX MESURES : copie la surface vers la mémoire
    /// centrale et convertit le YUV 4:4:4 en RGB.
    ///
    /// 11 059 200 octets par appel en 2560×1440. Le chemin normal ne copie
    /// jamais vers la mémoire centrale : la surface va directement au rendu.
    pub fn copier_vers_memoire_centrale(&self) -> anyhow::Result<Vec<u8>> {
        let largeur = self.largeur as usize;
        let hauteur = self.hauteur as usize;
        let plan = largeur * hauteur;

        // `cuMemcpy2D_v2` lit le contexte courant du fil : le reposer avant. Voir
        // `SessionNvdec::rendre_contexte_courant` pour pourquoi `!Send` n'y suffit
        // pas.
        self.session.rendre_contexte_courant()?;

        // Un plan à la fois, et non une copie de `3 × hauteur` lignes : les plans
        // sont espacés de `hauteur_surface` lignes, qui dépasse `hauteur` dès que
        // la résolution n'est pas alignée. Une copie unique confondrait les deux.
        let mut yuv = vec![0u8; plan * 3];
        let saut_de_plan = self.surface.pas as u64 * u64::from(self.surface.hauteur_surface);
        for numero in 0..3u64 {
            let copie = CUDA_MEMCPY2D {
                srcXInBytes: 0,
                srcY: 0,
                srcMemoryType: CUmemorytype::CU_MEMORYTYPE_DEVICE,
                srcHost: std::ptr::null(),
                srcDevice: self.surface.pointeur + numero * saut_de_plan,
                srcArray: std::ptr::null_mut(),
                srcPitch: self.surface.pas as usize,
                dstXInBytes: 0,
                dstY: 0,
                dstMemoryType: CUmemorytype::CU_MEMORYTYPE_HOST,
                dstHost: yuv[numero as usize * plan..].as_mut_ptr().cast(),
                dstDevice: 0,
                dstArray: std::ptr::null_mut(),
                dstPitch: largeur,
                WidthInBytes: largeur,
                Height: hauteur,
            };
            let code = unsafe { cuMemcpy2D_v2(&copie) };
            if code != CUresult::CUDA_SUCCESS {
                anyhow::bail!(
                    "cuMemcpy2D a échoué sur le plan {numero} (code {})",
                    code as i32
                );
            }
        }

        let (plan_y, reste) = yuv.split_at(plan);
        let (plan_u, plan_v) = reste.split_at(plan);

        let mut rgb = vec![0u8; plan * 3];
        for i in 0..plan {
            let y = plan_y[i] as f64;
            let u = plan_u[i] as f64;
            let v = plan_v[i] as f64;

            // BT.601 PLEINE ÉCHELLE, et non BT.709. Mesuré le 27/09/2026 sur le
            // flux du jalon 0 : BT.601 pleine échelle donne 89,78 dB contre la
            // référence, BT.709 plafonne à 36 dB. Si quelqu'un « corrige » vers
            // BT.709 parce que c'est ce qu'on attend d'un flux HD, le test de
            // référence rougira — c'est voulu.
            let r = y + 1.402 * (v - 128.0);
            let g = y - 0.344_136 * (u - 128.0) - 0.714_136 * (v - 128.0);
            let b = y + 1.772 * (u - 128.0);

            rgb[i * 3] = arrondir_octet(r);
            rgb[i * 3 + 1] = arrondir_octet(g);
            rgb[i * 3 + 2] = arrondir_octet(b);
        }
        Ok(rgb)
    }
}

impl Drop for ImageDecodee {
    fn drop(&mut self) {
        // Même exigence de contexte courant que pour la copie, et même
        // impossibilité de signaler : un contexte qu'on n'arrive pas à reposer
        // rendra de toute façon le démappage inopérant.
        let _ = self.session.rendre_contexte_courant();
        // Rien à faire du code de retour : on ne peut pas signaler d'erreur
        // depuis un `Drop`, et un démappage raté ne se répare pas.
        unsafe {
            (self.session.api.cuvid_unmap_video_frame64)(
                self.session.decodeur(),
                self.surface.pointeur,
            )
        };
    }
}

/// Arrondit au plus proche, puis convertit en octet.
///
/// Pas de `.clamp` : depuis Rust 1.45, un cast de flottant vers entier **sature**
/// au lieu d'être indéfini, donc le bornage est déjà fait — un `.clamp` serait du
/// code mort, et un code mort qu'un test aurait l'air de prouver. C'est bien un
/// bornage dont on a besoin : la matrice BT.601 sort de l'intervalle `0..=255`
/// sur des couples YUV que rien n'interdit au flux de contenir.
fn arrondir_octet(valeur: f64) -> u8 {
    valeur.round() as u8
}

#[cfg(test)]
mod tests {
    use super::arrondir_octet;

    #[test]
    fn l_arrondi_va_au_plus_proche() {
        // La seule assertion qui discrimine : sans `.round()`, le cast tronque et
        // 127,5 donnerait 127.
        assert_eq!(arrondir_octet(127.5), 128);
        assert_eq!(arrondir_octet(127.4), 127);
    }

    #[test]
    fn le_cast_sature_hors_intervalle_et_c_est_ce_qui_borne() {
        // Épingle le comportement de Rust sur lequel `arrondir_octet` s'appuie à
        // la place d'un `.clamp`. Ce test ne prouve pas une garde qu'on aurait
        // écrite : il prouve celle du langage, dont on dépend.
        assert_eq!(arrondir_octet(-42.0), 0);
        assert_eq!(arrondir_octet(300.0), 255);
    }
}
