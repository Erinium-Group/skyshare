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
/// suivent verticalement dans la même allocation, chacun de `hauteur` lignes
/// de `pas` octets : Y à `pointeur`, U à `pointeur + pas × hauteur`, V à
/// `pointeur + 2 × pas × hauteur`.
///
/// Aucun octet de pixel ne traverse cette frontière : seul le GPU sait lire ce
/// pointeur.
#[derive(Debug, Clone, Copy)]
pub struct SurfaceCuda {
    /// Pointeur de périphérique CUDA vers la première ligne du plan Y.
    pub pointeur: u64,
    /// Pas de ligne en octets, commun aux trois plans.
    pub pas: u32,
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
    /// La session qui a produit la surface, gardée en vie pour que le
    /// démappage ait lieu même si l'image survit au [`crate::Decodeur`].
    session: Rc<SessionNvdec>,
}

impl ImageDecodee {
    pub(crate) fn nouvelle(
        session: Rc<SessionNvdec>,
        largeur: u32,
        hauteur: u32,
        horodatage_ms: u64,
        surface: SurfaceCuda,
    ) -> Self {
        Self {
            largeur,
            hauteur,
            horodatage_ms,
            surface,
            session,
        }
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

        // Les trois plans se suivent verticalement avec le même pas : une seule
        // copie 2D de `3 × hauteur` lignes les ramène tous, déjà compactés.
        let mut yuv = vec![0u8; plan * 3];
        let copie = CUDA_MEMCPY2D {
            srcXInBytes: 0,
            srcY: 0,
            srcMemoryType: CUmemorytype::CU_MEMORYTYPE_DEVICE,
            srcHost: std::ptr::null(),
            srcDevice: self.surface.pointeur,
            srcArray: std::ptr::null_mut(),
            srcPitch: self.surface.pas as usize,
            dstXInBytes: 0,
            dstY: 0,
            dstMemoryType: CUmemorytype::CU_MEMORYTYPE_HOST,
            dstHost: yuv.as_mut_ptr().cast(),
            dstDevice: 0,
            dstArray: std::ptr::null_mut(),
            dstPitch: largeur,
            WidthInBytes: largeur,
            Height: hauteur * 3,
        };
        let code = unsafe { cuMemcpy2D_v2(&copie) };
        if code != CUresult::CUDA_SUCCESS {
            anyhow::bail!("cuMemcpy2D a échoué (code {})", code as i32);
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

/// Arrondit au plus proche et borne à l'intervalle d'un octet.
fn arrondir_octet(valeur: f64) -> u8 {
    valeur.round().clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::arrondir_octet;

    #[test]
    fn la_conversion_borne_les_debordements_des_deux_cotes() {
        assert_eq!(arrondir_octet(-42.0), 0);
        assert_eq!(arrondir_octet(300.0), 255);
        assert_eq!(arrondir_octet(127.5), 128);
    }
}
