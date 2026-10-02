//! La session de décodage : pousser des unités d'accès HEVC 4:4:4 dans NVDEC et
//! en ressortir des images qui vivent sur le GPU.
//!
//! NVDEC ne se pilote pas par appels directs mais par un analyseur syntaxique
//! (`cuvidCreateVideoParser`) qui rappelle trois fonctions C : à la découverte
//! de la séquence, à chaque image à décoder, à chaque image à afficher. Ces
//! rappels traversent la frontière FFI et doivent déposer leurs résultats
//! quelque part que [`Decodeur::decoder`] puisse relire ; c'est le rôle de
//! [`EtatPartage`], dont l'adresse est confiée à NVDEC comme `pUserData`.

use std::collections::VecDeque;
use std::ffi::{c_int, c_ulong, c_void};
use std::rc::Rc;
use std::sync::Arc;

use cudarc::driver::sys::CUresult;
use cudarc::driver::CudaContext;
use nvidia_video_codec_sdk::sys::cuviddec::{
    cudaVideoChromaFormat, cudaVideoCodec, cudaVideoCreateFlags, cudaVideoDeinterlaceMode,
    cudaVideoSurfaceFormat, CUvideodecoder, CUVIDDECODECREATEINFO, CUVIDPICPARAMS, CUVIDPROCPARAMS,
};
use nvidia_video_codec_sdk::sys::nvcuvid::{
    CUvideopacketflags, CUvideoparser, CUVIDEOFORMAT, CUVIDPARSERDISPINFO, CUVIDPARSERPARAMS,
    CUVIDSOURCEDATAPACKET,
};

use crate::capacites::{sonder_materiel, ErreurDecodeur};
use crate::image::{ImageDecodee, SurfaceCuda};
use crate::nvcuvid_sys::NvcuvidApi;

/// Nombre de surfaces de sortie mappables simultanément.
///
/// Quatre, et pas deux, parce que l'épuisement ne se signale pas : `cuviddec.h`
/// (« cuvidDecodePicture may block the calling thread if there are too many
/// pictures pending in the decode queue ») prévient qu'on n'obtiendrait pas une
/// erreur mais un **blocage du fil appelant** — et ce fil est celui du rendu,
/// donc une interface figée sans message. Le flux de données recommandé par ce
/// même en-tête mappe `N-4`, soit quatre images en vol ; c'est aussi ce que
/// demande le double tampon d'un afficheur, qui présente l'image *n* pendant
/// qu'il décode *n+1*. Chaque surface coûte environ 11 Mo de mémoire vidéo.
const SURFACES_DE_SORTIE: u32 = 4;

/// La DLL NVDEC, le contexte CUDA et le décodeur matériel.
///
/// Partagée par `Rc` avec chaque [`ImageDecodee`] : une image démappe sa
/// surface à sa destruction, donc le décodeur doit lui survivre — même si le
/// [`Decodeur`] a déjà disparu.
pub(crate) struct SessionNvdec {
    pub(crate) api: NvcuvidApi,
    /// Le contexte CUDA dans lequel NVDEC décode. Gardé nommé, et non `_`, parce
    /// qu'on s'en sert : voir [`SessionNvdec::rendre_contexte_courant`].
    pub(crate) contexte: Arc<CudaContext>,
    /// Nul jusqu'au rappel de séquence, qui seul connaît la taille codée.
    /// `Cell` parce que ce rappel ne reçoit qu'un accès partagé à la session.
    decodeur: std::cell::Cell<CUvideodecoder>,
}

impl SessionNvdec {
    pub(crate) fn decodeur(&self) -> CUvideodecoder {
        self.decodeur.get()
    }

    /// Rend le contexte de cette session courant sur le fil appelant.
    ///
    /// À appeler avant tout appel qui lit le contexte courant : NVDEC et
    /// `cuMemcpy2D`. `Decodeur` est `!Send`, donc il ne change pas de fil — mais
    /// cela ne dit rien du **contexte courant de ce fil**, que n'importe quel
    /// autre code du même fil peut remplacer par un `cuCtxSetCurrent`. C'est
    /// pourquoi on le repose ici plutôt que de s'appuyer sur `!Send`.
    /// `bind_to_thread` ne fait rien quand le contexte est déjà courant.
    ///
    /// Sa propre variante d'erreur, et non `SessionRefusee` : ce dernier annonce
    /// une session refusée par le pilote, avec un code de retour de NVDEC. Ici
    /// aucune session n'est en cause et le code vient de CUDA. Deux causes
    /// peuvent mener au même message pour l'utilisateur ; l'inverse — un seul
    /// message pour deux causes qu'il décrit mal — est un faux diagnostic.
    pub(crate) fn rendre_contexte_courant(&self) -> Result<(), ErreurDecodeur> {
        self.contexte
            .bind_to_thread()
            .map_err(|e| ErreurDecodeur::ContexteCuda(e.to_string()))
    }
}

impl Drop for SessionNvdec {
    fn drop(&mut self) {
        let decodeur = self.decodeur.get();
        if !decodeur.is_null() {
            unsafe { (self.api.cuvid_destroy_decoder)(decodeur) };
        }
    }
}

/// Ce que les rappels de NVDEC déposent, et que [`Decodeur::decoder`] relit.
struct EtatPartage {
    session: Rc<SessionNvdec>,
    /// Taille maximale annoncée par l'appelant à la création, reportée dans
    /// `ulMaxWidth`/`ulMaxHeight`. À ne pas confondre avec la taille maximale
    /// de la carte, que [`sonder_materiel`] a déjà vérifiée.
    largeur_annoncee: u32,
    hauteur_annoncee: u32,
    /// Taille de la zone affichée, connue seulement au rappel de séquence.
    largeur_affichee: u32,
    hauteur_affichee: u32,
    /// Hauteur de la surface de sortie, c'est-à-dire `ulTargetHeight` : le
    /// nombre de lignes qui séparent le début d'un plan du début du suivant.
    ///
    /// Renseignée depuis la MÊME variable que `ulTargetHeight` au rappel de
    /// séquence, et c'est délibéré : les deux doivent rester égales, sans quoi
    /// la lecture des plans de chrominance se décale. Ce n'est pas `coded_height`
    /// — mesuré, voir la note de [`SurfaceCuda`].
    hauteur_surface: u32,
    /// `coded_height` du flux, telle que le rappel de séquence l'annonce.
    ///
    /// Le décodage ne s'en sert pas : elle est là pour être **observable**. Sans
    /// elle, un test ne peut pas vérifier que la géométrie qu'il a choisie
    /// distingue réellement la hauteur codée de la hauteur d'affichage, et se
    /// croirait probant en comparant deux valeurs que notre propre code a
    /// posées.
    hauteur_codee: u32,
    /// Nombre de surfaces de décodage retenu, redonné à chaque rappel de
    /// séquence : NVDEC interprète cette valeur de retour comme la taille de
    /// son pool, et un 0 signifierait « échec ».
    surfaces_de_decodage: u32,
    /// Images mappées, dans l'ordre d'affichage.
    file: VecDeque<ImageDecodee>,
    /// Un rappel ne peut rendre qu'un booléen à NVDEC : la vraie cause est
    /// déposée ici, et [`Decodeur::decoder`] la relève.
    erreur: Option<ErreurDecodeur>,
}

/// Une session de décodage HEVC 4:4:4 sur NVDEC.
///
/// Contient un pointeur brut vers son état partagé : le type n'est donc ni
/// `Send` ni `Sync`, ce qui est exactement la contrainte de CUDA — le contexte
/// est lié au fil qui l'a créé.
pub struct Decodeur {
    session: Rc<SessionNvdec>,
    parseur: CUvideoparser,
    /// Possédé (issu de `Box::into_raw`) et libéré par [`Drop`]. Un pointeur
    /// brut plutôt qu'une `Box` : NVDEC en garde une copie comme `pUserData`,
    /// et deux provenances concurrentes sur la même adresse seraient un abus
    /// d'aliasing. Tout accès passe par ce pointeur, jamais par une `Box`.
    etat: *mut EtatPartage,
}

impl Decodeur {
    /// Ouvre une session de décodage pour des images d'au plus `largeur` par
    /// `hauteur`.
    pub fn nouveau(largeur: u32, hauteur: u32) -> Result<Self, ErreurDecodeur> {
        // Refuse d'emblée une machine sans décodeur 4:4:4, et vérifie que la
        // taille demandée tient dans ce que la carte sait décoder.
        let capacites = sonder_materiel()?;
        if largeur > capacites.largeur_max || hauteur > capacites.hauteur_max {
            // Sa propre variante, et non `QuatreQuatreQuatreNonPris` : ce dernier
            // annoncerait à une carte parfaitement capable qu'elle ne sait pas
            // recevoir, alors que seule la résolution est en cause. Ce projet n'a
            // aucun autre moyen de diagnostic qu'un message juste.
            return Err(ErreurDecodeur::ResolutionTropGrande {
                largeur,
                hauteur,
                maximum: (capacites.largeur_max, capacites.hauteur_max),
            });
        }

        // `CudaContext::new` lie le contexte au fil courant. Ce lien n'est pas
        // acquis pour la suite — voir `SessionNvdec::rendre_contexte_courant`, que
        // `decoder` rappelle à chaque fois.
        let contexte =
            CudaContext::new(0).map_err(|e| ErreurDecodeur::AucuneCarteNvidia(e.to_string()))?;
        let api =
            NvcuvidApi::load().map_err(|e| ErreurDecodeur::AucuneCarteNvidia(e.to_string()))?;

        let session = Rc::new(SessionNvdec {
            api,
            contexte,
            decodeur: std::cell::Cell::new(std::ptr::null_mut()),
        });

        let etat = Box::into_raw(Box::new(EtatPartage {
            session: Rc::clone(&session),
            largeur_annoncee: largeur,
            hauteur_annoncee: hauteur,
            largeur_affichee: 0,
            hauteur_affichee: 0,
            hauteur_surface: 0,
            hauteur_codee: 0,
            surfaces_de_decodage: 1,
            file: VecDeque::new(),
            erreur: None,
        }));

        let mut parametres: CUVIDPARSERPARAMS = unsafe { std::mem::zeroed() };
        parametres.CodecType = cudaVideoCodec::cudaVideoCodec_HEVC;
        // Une seule surface annoncée ici : le rappel de séquence rend le nombre
        // réellement voulu, et NVDEC le retient (c'est le protocole du SDK).
        parametres.ulMaxNumDecodeSurfaces = 1;
        // Zéro image de retard d'affichage : le partage d'écran veut la latence
        // la plus basse, et le flux du jalon 0 n'a pas d'images bidirectionnelles
        // à réordonner.
        parametres.ulMaxDisplayDelay = 0;
        parametres.pUserData = etat.cast::<c_void>();
        parametres.pfnSequenceCallback = Some(rappel_sequence);
        parametres.pfnDecodePicture = Some(rappel_decodage);
        parametres.pfnDisplayPicture = Some(rappel_affichage);

        let mut parseur: CUvideoparser = std::ptr::null_mut();
        let code =
            unsafe { (session.api.cuvid_create_video_parser)(&mut parseur, &mut parametres) };
        if code != CUresult::CUDA_SUCCESS {
            // Reprendre la `Box` : sans elle, l'état fuirait.
            drop(unsafe { Box::from_raw(etat) });
            return Err(ErreurDecodeur::SessionRefusee(code as i32));
        }

        Ok(Self {
            session,
            parseur,
            etat,
        })
    }

    /// Pousse une unité d'accès et rend l'image que le rappel d'affichage a
    /// déposée, s'il y en a une.
    ///
    /// **`unite` doit être une unité d'accès ENTIÈRE** — une image et une seule,
    /// éventuellement précédée de ses en-têtes de séquence. C'est ce que
    /// l'appelant réel reçoit : l'hôte écrit une unité NVENC entière par
    /// `PeerLink::ecrire_image`, et `str0m` ne rend un `MediaData` qu'une fois
    /// tous les paquets RTP d'une même image arrivés, sans trou. Un appelant qui
    /// pousserait des NAL isolés (une tranche sur deux d'une image) ferait
    /// décoder une image incomplète.
    ///
    /// L'image rendue est celle de l'unité poussée, au même appel. C'est le rôle
    /// de `CUVID_PKT_ENDOFPICTURE` (`nvcuvid.h` : « the packet contains exactly
    /// one frame ») : sans lui, l'analyseur ne sait qu'une image est complète
    /// qu'en voyant le début de la suivante, et rend l'image k−1 quand on pousse
    /// k — mesuré, `tests/aller_retour.rs`. Le spectateur décide d'afficher
    /// selon l'unité qu'il vient de pousser : avec une image de retard, il
    /// afficherait à la reprise après une perte la dernière image décodée sur des
    /// références perdues.
    ///
    /// `Ok(None)` n'est pas une erreur : le décodeur avale les en-têtes
    /// VPS/SPS/PPS sans rendre d'image.
    pub fn decoder(
        &mut self,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> Result<Option<ImageDecodee>, ErreurDecodeur> {
        let mut paquet: CUVIDSOURCEDATAPACKET = unsafe { std::mem::zeroed() };
        paquet.flags = (CUvideopacketflags::CUVID_PKT_TIMESTAMP as c_ulong)
            | (CUvideopacketflags::CUVID_PKT_ENDOFPICTURE as c_ulong);
        paquet.payload_size = unite.len() as c_ulong;
        paquet.payload = unite.as_ptr();
        paquet.timestamp = horodatage_ms as i64;

        // NVDEC lit le contexte courant du fil, y compris depuis les rappels
        // (`cuvidDecodePicture`, `cuvidMapVideoFrame64`) : on le repose avant.
        self.session.rendre_contexte_courant()?;

        // Aucune référence Rust vers `*self.etat` n'est vivante pendant cet
        // appel : les rappels en fabriquent une, et ils sont seuls à le faire.
        let code = unsafe { (self.session.api.cuvid_parse_video_data)(self.parseur, &mut paquet) };

        let etat = unsafe { &mut *self.etat };
        // La cause déposée par un rappel est plus précise que l'échec générique
        // de l'analyseur : on la relève d'abord.
        if let Some(erreur) = etat.erreur.take() {
            return Err(erreur);
        }
        if code != CUresult::CUDA_SUCCESS {
            return Err(ErreurDecodeur::SessionRefusee(code as i32));
        }
        Ok(etat.file.pop_front())
    }
}

impl Drop for Decodeur {
    fn drop(&mut self) {
        // L'analyseur d'abord : il pourrait encore rappeler nos fonctions, donc
        // toucher à `*self.etat`.
        if !self.parseur.is_null() {
            unsafe { (self.session.api.cuvid_destroy_video_parser)(self.parseur) };
        }
        // Les images encore en file démappent leurs surfaces ici ; le décodeur
        // leur survit par le `Rc` que chacune détient.
        drop(unsafe { Box::from_raw(self.etat) });
    }
}

/// Le verdict de la décision D8, séparé de l'appel matériel pour être prouvable
/// sans GPU — exactement comme `Capacites::depuis_brut` à la tâche 1.
///
/// On refuse plutôt que de dégrader en silence : un flux 4:2:0 ou 10 bits serait
/// décodable, mais pas dans le format que le rendu attend, et convertir sans le
/// dire produirait des couleurs fausses que personne ne saurait diagnostiquer.
fn verifier_format_de_sequence(format: &CUVIDEOFORMAT) -> Result<(), ErreurDecodeur> {
    if format.chroma_format != cudaVideoChromaFormat::cudaVideoChromaFormat_444
        || format.bit_depth_luma_minus8 != 0
    {
        return Err(ErreurDecodeur::QuatreQuatreQuatreNonPris);
    }
    Ok(())
}

/// Rappel de séquence : NVDEC a lu les en-têtes et annonce le format.
///
/// Valeur de retour attendue par le SDK : 0 pour échouer, sinon le nombre de
/// surfaces de décodage que l'analyseur doit réserver.
unsafe extern "C" fn rappel_sequence(donnees: *mut c_void, format: *mut CUVIDEOFORMAT) -> c_int {
    let etat = &mut *(donnees as *mut EtatPartage);
    let format = &*format;

    // Le décodeur existe déjà : une séquence répétée (nouvelle image clé) ne
    // doit pas en ouvrir un second.
    if !etat.session.decodeur().is_null() {
        return etat.surfaces_de_decodage as c_int;
    }

    if let Err(refus) = verifier_format_de_sequence(format) {
        etat.erreur = Some(refus);
        return 0;
    }

    let largeur = (format.display_area.right - format.display_area.left).max(0) as u32;
    let hauteur = (format.display_area.bottom - format.display_area.top).max(0) as u32;
    etat.largeur_affichee = largeur;
    etat.hauteur_affichee = hauteur;
    // La cible de sortie, seule et unique source des deux valeurs qui doivent
    // rester égales : `ulTargetHeight` ci-dessous et l'espacement des plans que
    // lit `SurfaceCuda`. Changer l'une sans l'autre décale la chrominance.
    let hauteur_cible = hauteur;
    etat.hauteur_surface = hauteur_cible;
    etat.hauteur_codee = format.coded_height;
    etat.surfaces_de_decodage = u32::from(format.min_num_decode_surfaces).max(1);

    let mut creation: CUVIDDECODECREATEINFO = std::mem::zeroed();
    creation.ulWidth = format.coded_width as c_ulong;
    creation.ulHeight = format.coded_height as c_ulong;
    creation.ulNumDecodeSurfaces = etat.surfaces_de_decodage as c_ulong;
    creation.CodecType = format.codec;
    creation.ChromaFormat = format.chroma_format;
    creation.ulCreationFlags = cudaVideoCreateFlags::cudaVideoCreate_PreferCUVID as c_ulong;
    creation.bitDepthMinus8 = c_ulong::from(format.bit_depth_luma_minus8);
    // `ulMaxWidth` doit couvrir la taille codée : un flux plus grand que ce que
    // l'appelant a annoncé ne se décodera pas, mais ne doit pas rendre l'appel
    // incohérent.
    creation.ulMaxWidth = etat.largeur_annoncee.max(format.coded_width) as c_ulong;
    creation.ulMaxHeight = etat.hauteur_annoncee.max(format.coded_height) as c_ulong;
    creation.display_area.left = format.display_area.left as i16;
    creation.display_area.top = format.display_area.top as i16;
    creation.display_area.right = format.display_area.right as i16;
    creation.display_area.bottom = format.display_area.bottom as i16;
    creation.OutputFormat = cudaVideoSurfaceFormat::cudaVideoSurfaceFormat_YUV444;
    creation.DeinterlaceMode = cudaVideoDeinterlaceMode::cudaVideoDeinterlaceMode_Weave;
    creation.ulTargetWidth = largeur as c_ulong;
    creation.ulTargetHeight = hauteur_cible as c_ulong;
    creation.ulNumOutputSurfaces = SURFACES_DE_SORTIE as c_ulong;
    // `vidLock` nul : un seul fil pilote cette session.
    creation.vidLock = std::ptr::null_mut();

    let mut decodeur: CUvideodecoder = std::ptr::null_mut();
    let code = (etat.session.api.cuvid_create_decoder)(&mut decodeur, &mut creation);
    if code != CUresult::CUDA_SUCCESS {
        etat.erreur = Some(ErreurDecodeur::SessionRefusee(code as i32));
        return 0;
    }
    etat.session.decodeur.set(decodeur);
    etat.surfaces_de_decodage as c_int
}

/// Rappel de décodage : NVDEC a une image complète à faire décoder.
unsafe extern "C" fn rappel_decodage(donnees: *mut c_void, params: *mut CUVIDPICPARAMS) -> c_int {
    let etat = &mut *(donnees as *mut EtatPartage);
    let code = (etat.session.api.cuvid_decode_picture)(etat.session.decodeur(), params);
    if code != CUresult::CUDA_SUCCESS {
        etat.erreur = Some(ErreurDecodeur::SessionRefusee(code as i32));
        return 0;
    }
    1
}

/// Rappel d'affichage : une image est prête, dans l'ordre d'affichage.
///
/// On la mappe ici, et non plus tard : une fois ce rappel rendu, NVDEC
/// considère la surface de décodage réutilisable, et seule une image mappée est
/// verrouillée. Le démappage a lieu à la destruction de l'[`ImageDecodee`].
unsafe extern "C" fn rappel_affichage(
    donnees: *mut c_void,
    info: *mut CUVIDPARSERDISPINFO,
) -> c_int {
    let etat = &mut *(donnees as *mut EtatPartage);
    let info = &*info;

    let mut parametres: CUVIDPROCPARAMS = std::mem::zeroed();
    parametres.progressive_frame = info.progressive_frame;
    parametres.top_field_first = info.top_field_first;
    parametres.second_field = 0;
    parametres.unpaired_field = c_int::from(info.repeat_first_field < 0);

    let mut pointeur: u64 = 0;
    let mut pas: u32 = 0;
    let code = (etat.session.api.cuvid_map_video_frame64)(
        etat.session.decodeur(),
        info.picture_index,
        &mut pointeur,
        &mut pas,
        &mut parametres,
    );
    if code != CUresult::CUDA_SUCCESS {
        etat.erreur = Some(ErreurDecodeur::SessionRefusee(code as i32));
        return 0;
    }

    etat.file.push_back(ImageDecodee::nouvelle(
        Rc::clone(&etat.session),
        etat.largeur_affichee,
        etat.hauteur_affichee,
        etat.hauteur_codee,
        info.timestamp as u64,
        SurfaceCuda {
            pointeur,
            pas,
            hauteur_surface: etat.hauteur_surface,
        },
    ));
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique un format de séquence conforme ; chaque test n'en change qu'un
    /// champ. Même motif que `brut_capable` de `capacites.rs`.
    fn format_conforme() -> CUVIDEOFORMAT {
        let mut format: CUVIDEOFORMAT = unsafe { std::mem::zeroed() };
        format.codec = cudaVideoCodec::cudaVideoCodec_HEVC;
        format.chroma_format = cudaVideoChromaFormat::cudaVideoChromaFormat_444;
        format.bit_depth_luma_minus8 = 0;
        format.coded_width = 2560;
        format.coded_height = 1440;
        format
    }

    #[test]
    fn un_flux_444_8_bits_est_accepte() {
        verifier_format_de_sequence(&format_conforme()).expect("doit être accepté");
    }

    #[test]
    fn un_flux_420_est_refuse_au_lieu_d_etre_degrade() {
        // Décision D8 : NVDEC saurait le décoder, et c'est précisément le piège —
        // le rendu attend de la chrominance pleine résolution. Refuser, jamais
        // convertir en silence.
        let mut format = format_conforme();
        format.chroma_format = cudaVideoChromaFormat::cudaVideoChromaFormat_420;
        let refus = verifier_format_de_sequence(&format).expect_err("doit être refusé");
        assert!(matches!(refus, ErreurDecodeur::QuatreQuatreQuatreNonPris));
    }

    #[test]
    fn un_flux_10_bits_est_refuse_au_lieu_d_etre_degrade() {
        let mut format = format_conforme();
        format.bit_depth_luma_minus8 = 2;
        let refus = verifier_format_de_sequence(&format).expect_err("doit être refusé");
        assert!(matches!(refus, ErreurDecodeur::QuatreQuatreQuatreNonPris));
    }
}
