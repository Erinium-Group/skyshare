//! Décodage Media Foundation : HEVC Main 4:2:0 et H.264, en matériel.
//!
//! Recette mesurée par la sonde AMD du 02/10/2026
//! (`spike/mesures/sonde-amd/source/src/mf.rs`, rapports à côté). Trois faits
//! en fondent la forme :
//! - aucun décodeur n'est énuméré avec `MFT_ENUM_FLAG_HARDWARE`, ni sur AMD ni
//!   sur NVIDIA : le « matériel » est un MFT SYNCHRONE de Microsoft qui fait son
//!   DXVA en interne quand on lui confie un gestionnaire D3D11 ;
//! - avec ce gestionnaire et `MF_LOW_LATENCY = 1`, il rend 600 images sur 600 et
//!   en retient au plus une ; SANS gestionnaire (logiciel), le même réglage perd
//!   toutes les images après la 120e en HEVC. D'où : jamais de logiciel ;
//! - l'image sort dans une tranche d'un TABLEAU de textures NV12 lié au seul
//!   décodeur : non lisible par un nuanceur, elle se copie (`sky-rendu`).
//!
//! L'ouverture ([`DecodeurMf::nouveau`]) sert aussi à la sonde de décodage, et
//! il n'en existe qu'une. Le décodage ([`DecodeurMf::decoder`]) rend une
//! [`ImageMf`] par unité d'accès poussée, au même appel.

use std::mem::ManuallyDrop;

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, ID3D11VideoDevice, D3D11_BOX, D3D11_CPU_ACCESS_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11_VIDEO_DECODER_DESC,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

use crate::ErreurDecodeur;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecMf {
    Hevc,
    H264,
}

impl CodecMf {
    fn sous_type(self) -> GUID {
        match self {
            CodecMf::Hevc => MFVideoFormat_HEVC,
            CodecMf::H264 => MFVideoFormat_H264,
        }
    }

    /// Profil DXVA, d'après `dxva.h` du Windows SDK 10.0.26100.0 (table de la
    /// sonde, `dxva.rs`, lignes « HEVC Main (8 bits 4:2:0) » et « H.264 VLD
    /// (sans FGT) »).
    fn profil_dxva(self) -> GUID {
        match self {
            // D3D11_DECODER_PROFILE_HEVC_VLD_MAIN
            CodecMf::Hevc => GUID::from_values(
                0x5b11d51b,
                0x2f4c,
                0x4452,
                [0xbc, 0xc3, 0x09, 0xf2, 0xa1, 0x16, 0x0c, 0xc0],
            ),
            // D3D11_DECODER_PROFILE_H264_VLD_NOFGT
            CodecMf::H264 => GUID::from_values(
                0x1b81be68,
                0xa0c7,
                0x11d3,
                [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5],
            ),
        }
    }
}

/// COM et Media Foundation tenus pour la durée de vie d'un décodeur.
///
/// `CoInitializeEx` peut rendre `S_FALSE` (déjà initialisé sur ce fil) : un
/// succès qu'il faut équilibrer par `CoUninitialize` comme les autres. Un fil
/// déjà en mode STA (`RPC_E_CHANGED_MODE`) n'empêche pas Media Foundation : on
/// n'équilibre alors rien.
pub(crate) struct Plateforme {
    com_a_liberer: bool,
}

impl Plateforme {
    pub(crate) fn demarrer() -> Result<Self, ErreurDecodeur> {
        let com_a_liberer = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        // `MFStartup` et `MFShutdown` se comptent : un `MFShutdown` sans
        // `MFStartup` réussi arrêterait Media Foundation sous les pieds d'un
        // autre utilisateur du processus. La `Plateforme` (dont le `Drop`
        // appelle `MFShutdown`) n'existe donc qu'APRÈS un démarrage réussi ;
        // avant, seul COM est à rendre.
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            if com_a_liberer {
                unsafe { CoUninitialize() };
            }
            return Err(mf("MFStartup", e));
        }
        Ok(Self { com_a_liberer })
    }
}

impl Drop for Plateforme {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            if self.com_a_liberer {
                CoUninitialize();
            }
        }
    }
}

pub(crate) fn mf(etape: &str, e: windows::core::Error) -> ErreurDecodeur {
    ErreurDecodeur::MediaFoundation(format!("{etape} : {e}"))
}

/// DXVA sait-il décoder ce codec à cette taille, en NV12, sur cet adaptateur ?
/// La question est posée à la puce, pas au MFT : elle répond même si
/// l'extension HEVC manque.
pub(crate) fn dxva_accepte(
    appareil: &ID3D11Device,
    codec: CodecMf,
    largeur: u32,
    hauteur: u32,
) -> bool {
    let Ok(video) = appareil.cast::<ID3D11VideoDevice>() else {
        return false;
    };
    let desc = D3D11_VIDEO_DECODER_DESC {
        Guid: codec.profil_dxva(),
        SampleWidth: largeur,
        SampleHeight: hauteur,
        OutputFormat: DXGI_FORMAT_NV12,
    };
    matches!(unsafe { video.GetVideoDecoderConfigCount(&desc) }, Ok(n) if n > 0)
}

/// Les décodeurs Media Foundation de ce codec, meilleur d'abord.
///
/// JAMAIS `MFT_ENUM_FLAG_HARDWARE` : il ne rend aucun décodeur, ni sur AMD ni
/// sur NVIDIA (mesuré, spec §2). Le matériel passe par les MFT synchrones.
fn enumerer(codec: CodecMf) -> Result<Vec<IMFActivate>, ErreurDecodeur> {
    let info = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: codec.sous_type(),
    };
    let drapeaux = MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER;
    let mut tableau: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut n = 0u32;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_DECODER,
            drapeaux,
            Some(&info),
            None,
            &mut tableau,
            &mut n,
        )
    }
    .map_err(|e| mf("MFTEnumEx", e))?;
    let mut activations = Vec::new();
    if !tableau.is_null() {
        let elements = unsafe { std::slice::from_raw_parts_mut(tableau, n as usize) };
        activations.extend(elements.iter_mut().filter_map(Option::take));
        unsafe { CoTaskMemFree(Some(tableau as _)) };
    }
    Ok(activations)
}

/// Les trois preuves de la spec (§4) : la puce accepte la taille (DXVA), un
/// MFT existe pour le codec, et il accepte le périphérique. La création
/// complète du décodeur (`DecodeurMf::nouveau`) est la preuve la plus forte :
/// elle est faite et défaite ici.
pub fn mf_sait_decoder(
    appareil: &ID3D11Device,
    codec: CodecMf,
    largeur: u32,
    hauteur: u32,
) -> bool {
    DecodeurMf::nouveau(codec, appareil, largeur, hauteur).is_ok()
}

/// `MF_LOW_LATENCY` à l'ouverture. Privé : seul `DecodeurMf::nouveau` (`Oui`)
/// et le constructeur réservé aux tests (`Non`) le choisissent.
#[derive(Clone, Copy)]
enum FaibleLatence {
    Oui,
    // Construit seulement par `nouveau_pour_essai_sans_faible_latence`, absent
    // hors de la fonctionnalité d'essai.
    #[cfg_attr(not(feature = "essai-sans-faible-latence"), allow(dead_code))]
    Non,
}

/// Un décodeur Media Foundation ouvert sur le périphérique de la fenêtre.
pub struct DecodeurMf {
    // L'ORDRE DE DESTRUCTION compte (sonde, `mf.rs:514-517`) : le MFT d'abord,
    // puis l'arrêt de son activation, le gestionnaire, et Media Foundation en
    // dernier. `Drop` libère explicitement le MFT (d'où le `ManuallyDrop`)
    // AVANT `ShutdownObject`, comme la sonde ; les autres champs tombent
    // ensuite dans l'ordre de leur déclaration. Le périphérique, lui,
    // appartient à la fenêtre et lui survit (`Visionnage` déclare le décodeur
    // avant l'afficheur).
    transformation: ManuallyDrop<IMFTransform>,
    activation: IMFActivate,
    _gestionnaire: IMFDXGIDeviceManager,
    /// Le flux de sortie tel que déclaré au dernier type choisi : relu à chaque
    /// changement de flux, et `tirer` y vérifie que le décodeur fournit
    /// toujours ses images (D5).
    info: MFT_OUTPUT_STREAM_INFO,
    /// Taille d'AFFICHAGE du type de sortie courant, jamais nulle
    /// (`taille_d_affichage`).
    largeur: u32,
    hauteur: u32,
    /// `MFT_MESSAGE_NOTIFY_BEGIN_STREAMING` a-t-il été accepté ? `Drop`
    /// n'envoie la fin de diffusion qu'alors, comme la sonde : un décodeur
    /// tombé à mi-ouverture n'a rien commencé.
    diffusion_commencee: bool,
    _plateforme: Plateforme,
}

impl Drop for DecodeurMf {
    fn drop(&mut self) {
        unsafe {
            if self.diffusion_commencee {
                let _ = self
                    .transformation
                    .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            }
            ManuallyDrop::drop(&mut self.transformation);
            let _ = self.activation.ShutdownObject();
        }
    }
}

impl DecodeurMf {
    /// Ouvre le décodeur Media Foundation de `codec` sur `appareil`, pour un
    /// flux annoncé à cette taille. Tout échec, et tout signe d'un chemin non
    /// matériel, rend [`ErreurDecodeur::MediaFoundation`] ; l'absence de tout
    /// décodeur pour le codec rend [`ErreurDecodeur::AucunDecodeur`].
    pub fn nouveau(
        codec: CodecMf,
        appareil: &ID3D11Device,
        largeur_annoncee: u32,
        hauteur_annoncee: u32,
    ) -> Result<Self, ErreurDecodeur> {
        Self::ouvrir(codec, appareil, largeur_annoncee, hauteur_annoncee, FaibleLatence::Oui)
    }

    /// RÉSERVÉ AUX TESTS : ouvre le décodeur avec `MF_LOW_LATENCY = 0`, le seul
    /// moyen connu de produire, sur du matériel réel, un décodeur qui RETIENT
    /// des images (mesuré, RTX 4060, HEVC : l'appel k rend l'image k−2). C'est
    /// ce qui permet de prouver que le filtre d'horodatage de [`Self::decoder`]
    /// est bien branché (revue finale du sous-jalon « toutes cartes », I2).
    ///
    /// LE CHEMIN DE PRODUCTION NE PEUT PAS L'EMPRUNTER : la fonction n'existe
    /// que sous la fonctionnalité `essai-sans-faible-latence`, que seule la
    /// dépendance de DÉVELOPPEMENT de `sky-decode` sur elle-même active
    /// (`Cargo.toml`). Avec le résolveur 2 du workspace, une fonctionnalité
    /// tirée par une dépendance de développement n'entre dans aucune
    /// construction qui ne compile pas de tests — donc ni `tauri build` ni
    /// `cargo build -p sky-app`. Ne jamais l'ajouter aux fonctionnalités d'une
    /// dépendance normale : la spec (D5) exige `MF_LOW_LATENCY = 1`.
    #[cfg(feature = "essai-sans-faible-latence")]
    #[doc(hidden)]
    pub fn nouveau_pour_essai_sans_faible_latence(
        codec: CodecMf,
        appareil: &ID3D11Device,
        largeur_annoncee: u32,
        hauteur_annoncee: u32,
    ) -> Result<Self, ErreurDecodeur> {
        Self::ouvrir(codec, appareil, largeur_annoncee, hauteur_annoncee, FaibleLatence::Non)
    }

    fn ouvrir(
        codec: CodecMf,
        appareil: &ID3D11Device,
        largeur_annoncee: u32,
        hauteur_annoncee: u32,
        faible_latence: FaibleLatence,
    ) -> Result<Self, ErreurDecodeur> {
        let plateforme = Plateforme::demarrer()?;

        // La puce d'abord : un refus ici nomme la vraie cause, et c'est la seule
        // garde de taille pour HEVC — le MFT HEVC s'ouvre en 16384×16384 sans
        // elle (mesuré, RTX 4060). Pas `ResolutionTropGrande` : DXVA répond oui
        // ou non pour une taille, sans dire son maximum, que cette variante
        // exige (spec §4).
        if !dxva_accepte(appareil, codec, largeur_annoncee, hauteur_annoncee) {
            return Err(ErreurDecodeur::MediaFoundation(format!(
                "la puce ne décode pas {codec:?} en {largeur_annoncee}×{hauteur_annoncee}"
            )));
        }

        // `SORTANDFILTER` trie : le premier est le meilleur.
        let activation = enumerer(codec)?
            .into_iter()
            .next()
            .ok_or(ErreurDecodeur::AucunDecodeur)?;

        // Le gestionnaire avant le MFT, comme la sonde (`mf.rs:380-394`) : il
        // doit lui survivre.
        let mut jeton = 0u32;
        let mut gestionnaire: Option<IMFDXGIDeviceManager> = None;
        unsafe { MFCreateDXGIDeviceManager(&mut jeton, &mut gestionnaire) }
            .map_err(|e| mf("MFCreateDXGIDeviceManager", e))?;
        let gestionnaire = gestionnaire.ok_or_else(|| {
            ErreurDecodeur::MediaFoundation(
                "MFCreateDXGIDeviceManager n'a rendu aucun gestionnaire".into(),
            )
        })?;
        unsafe { gestionnaire.ResetDevice(appareil, jeton) }
            .map_err(|e| mf("IMFDXGIDeviceManager::ResetDevice", e))?;

        let transformation: IMFTransform = unsafe { activation.ActivateObject() }
            .map_err(|e| mf("IMFActivate::ActivateObject", e))?;
        // Le MFT est confié TOUT DE SUITE à la structure qui porte `Drop` : tout
        // échec qui suit l'arrête par `ShutdownObject`, sans chemin de
        // nettoyage écrit à la main (`tasks/lessons.md`, 22/08, session NVENC).
        let mut decodeur = Self {
            transformation: ManuallyDrop::new(transformation),
            activation,
            _gestionnaire: gestionnaire,
            info: MFT_OUTPUT_STREAM_INFO::default(),
            largeur: 0,
            hauteur: 0,
            diffusion_commencee: false,
            _plateforme: plateforme,
        };
        let t: &IMFTransform = &decodeur.transformation;

        let attributs =
            unsafe { t.GetAttributes() }.map_err(|e| mf("IMFTransform::GetAttributes", e))?;
        // Le chemin asynchrone n'a jamais été exercé : la machine de la sonde
        // n'avait aucun MFT asynchrone (`fabrication.md` §5). On le refuse
        // plutôt que de le découvrir chez quelqu'un.
        if matches!(unsafe { attributs.GetUINT32(&MF_TRANSFORM_ASYNC) }, Ok(v) if v != 0) {
            return Err(ErreurDecodeur::MediaFoundation(
                "décodeur asynchrone : chemin non éprouvé".into(),
            ));
        }
        // Faible latence : c'est elle qui retient au plus une image (spec D5).
        // Sûre sur le chemin matériel seulement ; le logiciel est refusé plus bas.
        // Porteuse, mesuré (RTX 4060, tâche 5) : à 0, l'unité 0 ne rend aucune
        // image au même appel, en HEVC comme en H.264. `FaibleLatence::Non`
        // n'est atteignable que par le constructeur réservé aux tests.
        let valeur = match faible_latence {
            FaibleLatence::Oui => 1,
            FaibleLatence::Non => 0,
        };
        unsafe { attributs.SetUINT32(&MF_LOW_LATENCY, valeur) }
            .map_err(|e| mf(&format!("MF_LOW_LATENCY = {valeur}"), e))?;

        unsafe {
            t.ProcessMessage(
                MFT_MESSAGE_SET_D3D_MANAGER,
                decodeur._gestionnaire.as_raw() as usize,
            )
        }
        .map_err(|e| {
            ErreurDecodeur::MediaFoundation(format!(
                "le décodeur refuse le périphérique Direct3D 11 : pas de décodage matériel ({e})"
            ))
        })?;

        let entree = unsafe { MFCreateMediaType() }.map_err(|e| mf("MFCreateMediaType", e))?;
        unsafe {
            entree
                .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                .map_err(|e| mf("MF_MT_MAJOR_TYPE", e))?;
            entree
                .SetGUID(&MF_MT_SUBTYPE, &codec.sous_type())
                .map_err(|e| mf("MF_MT_SUBTYPE", e))?;
            // La taille ANNONCÉE par la négociation ; le flux peut la contredire
            // (`MF_E_TRANSFORM_STREAM_CHANGE`, traité par `decoder`).
            entree
                .SetUINT64(
                    &MF_MT_FRAME_SIZE,
                    ((largeur_annoncee as u64) << 32) | hauteur_annoncee as u64,
                )
                .map_err(|e| mf("MF_MT_FRAME_SIZE", e))?;
            entree
                .SetUINT64(&MF_MT_FRAME_RATE, (60u64 << 32) | 1)
                .map_err(|e| mf("MF_MT_FRAME_RATE", e))?;
            entree
                .SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)
                .map_err(|e| mf("MF_MT_INTERLACE_MODE", e))?;
            t.SetInputType(0, &entree, 0)
                .map_err(|e| mf("SetInputType", e))?;
        }

        let (largeur, hauteur) = choisir_sortie(t)?;
        let info = unsafe { t.GetOutputStreamInfo(0) }.map_err(|e| mf("GetOutputStreamInfo", e))?;
        // Un MFT qui attend qu'on lui fournisse les échantillons travaille en
        // mémoire centrale (sonde : drapeaux 0x107 en matériel, 0x7 en logiciel).
        if info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 == 0 {
            return Err(ErreurDecodeur::MediaFoundation(
                "le décodeur ne fournit pas ses images : pas de décodage matériel".into(),
            ));
        }

        unsafe { t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0) }
            .map_err(|e| mf("NOTIFY_BEGIN_STREAMING", e))?;
        decodeur.diffusion_commencee = true;
        unsafe {
            decodeur
                .transformation
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
        }
        .map_err(|e| mf("NOTIFY_START_OF_STREAM", e))?;
        decodeur.info = info;
        decodeur.largeur = largeur;
        decodeur.hauteur = hauteur;
        Ok(decodeur)
    }

    /// Pousse UNE unité d'accès entière et rend l'image de CETTE unité, au même
    /// appel — le contrat de `sky-partage/src/doublure.rs::DecodeurFactice`, dont
    /// dépend la garde d'affichage.
    ///
    /// Le contrat est GARANTI par le filtre d'horodatage ([`image_de_l_unite`]) :
    /// seule sort l'image qui porte l'horodatage poussé. Un décodeur qui retient
    /// une image (l'appel k rendrait k−1) échoue VISIBLEMENT, par une erreur qui
    /// le dit, au lieu de faire afficher l'image d'une autre unité. Sur la
    /// RTX 4060, le contrat est tenu sans que le filtre ait à refuser quoi que
    /// ce soit : `tests/media_foundation.rs::*_chaque_unite_rend_sa_propre_image_a_sa_taille`.
    /// Le cas du décodeur en retard a été produit sur ce même matériel, en
    /// éteignant `MF_LOW_LATENCY` (mesuré, ronde de correction 1 de la tâche 5) :
    /// le MFT HEVC rend alors, à l'appel k, l'image k−2, et le filtre en fait
    /// une erreur à chaque appel ; le MFT H.264 ne rend rien sur 8 unités. Ce
    /// cas est REJOUÉ par un test, qui prouve que le filtre est branché ici et
    /// pas seulement juste en lui-même :
    /// `tests/media_foundation.rs::hevc_420_un_decodeur_qui_retient_des_images_echoue_au_lieu_d_afficher_faux`
    /// (remplacer l'appel ci-dessous par « dernière image tirée » le fait rougir).
    ///
    /// `Ok(None)` : aucune image tirée (en-têtes seuls).
    pub fn decoder(
        &mut self,
        unite: &[u8],
        horodatage_ms: u64,
    ) -> Result<Option<ImageMf>, ErreurDecodeur> {
        let echantillon = echantillon_de(unite, horodatage_ms)?;
        // Toutes les images tirées pendant cet appel, vidage compris : le
        // filtre choisit parmi elles.
        let mut tirees = Vec::new();
        let mut refus = 0;
        loop {
            match unsafe { self.transformation.ProcessInput(0, &echantillon, 0) } {
                Ok(()) => break,
                Err(e) if e.code() == MF_E_NOTACCEPTING => {
                    // Une sortie attend : la vider puis réessayer. Ces images
                    // précèdent l'acceptation de l'unité : elles ne peuvent pas
                    // être la sienne, et le filtre ne les rendra jamais.
                    refus += 1;
                    if refus > 16 {
                        return Err(mf("ProcessInput refuse toujours l'entrée", e));
                    }
                    while let Some(image) = self.tirer()? {
                        tirees.push((image.horodatage_ms, image));
                    }
                }
                Err(e) => return Err(mf("ProcessInput", e)),
            }
        }
        while let Some(image) = self.tirer()? {
            tirees.push((image.horodatage_ms, image));
        }
        image_de_l_unite(horodatage_ms, tirees).map_err(|recus| {
            ErreurDecodeur::MediaFoundation(format!(
                "le décodeur rend l'image d'une autre unité : horodatage attendu \
                 {horodatage_ms} ms, reçu {recus:?} ms"
            ))
        })
    }

    /// Tire une sortie du décodeur. `Ok(None)` : il demande plus d'entrée.
    fn tirer(&mut self) -> Result<Option<ImageMf>, ErreurDecodeur> {
        // Au plus 4 changements de flux de suite : un décodeur qui en annonce
        // sans fin bouclerait ici pour toujours.
        let mut changements = 0;
        loop {
            // D5 : `nouveau` l'a exigé, mais un changement de flux relit ces
            // drapeaux ; un décodeur qui cesserait de fournir ses images
            // travaillerait en mémoire centrale.
            if self.info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 == 0 {
                return Err(ErreurDecodeur::MediaFoundation(
                    "le décodeur ne fournit plus ses images : pas de décodage matériel".into(),
                ));
            }
            let mut tampon = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                // Le décodeur fournit ses échantillons (exigé par `nouveau`).
                pSample: ManuallyDrop::new(None),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            };
            let mut statut = 0u32;
            let resultat = unsafe {
                self.transformation
                    .ProcessOutput(0, std::slice::from_mut(&mut tampon), &mut statut)
            };
            // Reprendre les deux références AVANT tout retour : sans ce `take`,
            // l'échantillon et la collection d'événements fuiraient (COM).
            let sortie = unsafe { ManuallyDrop::take(&mut tampon.pSample) };
            drop(unsafe { ManuallyDrop::take(&mut tampon.pEvents) });
            match resultat {
                Ok(()) => {
                    let echantillon = sortie.ok_or_else(|| {
                        ErreurDecodeur::MediaFoundation(
                            "ProcessOutput a réussi sans rendre d'échantillon".into(),
                        )
                    })?;
                    return self.image_de(echantillon).map(Some);
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    changements += 1;
                    if changements > 4 {
                        return Err(mf("changements de flux sans fin", e));
                    }
                    (self.largeur, self.hauteur) = choisir_sortie(&self.transformation)?;
                    self.info = unsafe { self.transformation.GetOutputStreamInfo(0) }
                        .map_err(|e| mf("GetOutputStreamInfo", e))?;
                }
                Err(e) => return Err(mf("ProcessOutput", e)),
            }
        }
    }

    fn image_de(&self, echantillon: IMFSample) -> Result<ImageMf, ErreurDecodeur> {
        let tampon =
            unsafe { echantillon.GetBufferByIndex(0) }.map_err(|e| mf("GetBufferByIndex", e))?;
        // D5 : une image hors GPU signe un repli logiciel, jamais affiché.
        let dxgi: IMFDXGIBuffer = tampon.cast().map_err(|_| {
            ErreurDecodeur::MediaFoundation(
                "image rendue en mémoire centrale : décodage logiciel refusé".into(),
            )
        })?;
        let mut brut: *mut core::ffi::c_void = std::ptr::null_mut();
        unsafe { dxgi.GetResource(&ID3D11Texture2D::IID, &mut brut) }
            .map_err(|e| mf("IMFDXGIBuffer::GetResource", e))?;
        // `GetResource` a réussi : `brut` porte une référence qui nous revient.
        let texture = unsafe { ID3D11Texture2D::from_raw(brut) };
        let tranche =
            unsafe { dxgi.GetSubresourceIndex() }.map_err(|e| mf("GetSubresourceIndex", e))?;
        let temps = unsafe { echantillon.GetSampleTime() }.map_err(|e| mf("GetSampleTime", e))?;
        Ok(ImageMf {
            largeur: self.largeur,
            hauteur: self.hauteur,
            // Unités de 100 ns, arrondies à la milliseconde la plus proche.
            horodatage_ms: (temps.max(0) as u64 + 5_000) / 10_000,
            texture,
            tranche,
            _echantillon: echantillon,
        })
    }
}

/// Le choix de l'image que rend `decoder`, sur les seuls horodatages : la
/// dernière image tirée qui porte l'horodatage `attendu`. Les autres sont
/// jetées.
///
/// - aucune image tirée : `Ok(None)` ;
/// - des images tirées, aucune à l'horodatage attendu : `Err` avec les
///   horodatages reçus — un décodeur en retard doit échouer, ni afficher
///   faux, ni se taire.
///
/// L'égalité exacte est sûre : ms → 100 ns (×10 000) → ms (arrondi) est un
/// aller-retour exact.
fn image_de_l_unite<T>(attendu: u64, tirees: Vec<(u64, T)>) -> Result<Option<T>, Vec<u64>> {
    if tirees.is_empty() {
        return Ok(None);
    }
    let recus: Vec<u64> = tirees.iter().map(|(h, _)| *h).collect();
    tirees
        .into_iter()
        .rev()
        .find(|(h, _)| *h == attendu)
        .map(|(_, image)| Some(image))
        .ok_or(recus)
}

/// L'échantillon d'entrée d'une unité d'accès : une copie en mémoire centrale
/// (le flux arrive du réseau), horodatée en unités de 100 ns.
fn echantillon_de(unite: &[u8], horodatage_ms: u64) -> Result<IMFSample, ErreurDecodeur> {
    let longueur = u32::try_from(unite.len()).map_err(|_| {
        ErreurDecodeur::MediaFoundation(format!("unité d'accès démesurée ({} octets)", unite.len()))
    })?;
    let temps = i64::try_from(horodatage_ms)
        .ok()
        .and_then(|ms| ms.checked_mul(10_000))
        .ok_or_else(|| {
            ErreurDecodeur::MediaFoundation(format!("horodatage démesuré ({horodatage_ms} ms)"))
        })?;
    unsafe {
        let tampon = MFCreateMemoryBuffer(longueur).map_err(|e| mf("MFCreateMemoryBuffer", e))?;
        let mut p: *mut u8 = std::ptr::null_mut();
        tampon
            .Lock(&mut p, None, None)
            .map_err(|e| mf("IMFMediaBuffer::Lock", e))?;
        std::ptr::copy_nonoverlapping(unite.as_ptr(), p, unite.len());
        tampon
            .Unlock()
            .map_err(|e| mf("IMFMediaBuffer::Unlock", e))?;
        tampon
            .SetCurrentLength(longueur)
            .map_err(|e| mf("SetCurrentLength", e))?;
        let echantillon = MFCreateSample().map_err(|e| mf("MFCreateSample", e))?;
        echantillon
            .AddBuffer(&tampon)
            .map_err(|e| mf("IMFSample::AddBuffer", e))?;
        echantillon
            .SetSampleTime(temps)
            .map_err(|e| mf("SetSampleTime", e))?;
        // Une image à 60 im/s : la durée n'est qu'indicative pour le décodeur.
        echantillon
            .SetSampleDuration(166_667)
            .map_err(|e| mf("SetSampleDuration", e))?;
        Ok(echantillon)
    }
}

/// Une image décodée par Media Foundation : une tranche d'une texture NV12
/// (tableau en H.264, texture d'une seule tranche en HEVC, relevé RTX 4060)
/// liée au décodeur. Son contenu tient tant que l'`ImageMf` vit.
pub struct ImageMf {
    /// Taille d'AFFICHAGE (1080 lignes pour un flux codé sur 1088), jamais nulle.
    pub largeur: u32,
    pub hauteur: u32,
    pub horodatage_ms: u64,
    texture: ID3D11Texture2D,
    tranche: u32,
    /// Tant que l'échantillon vit, le décodeur ne réattribue pas la tranche.
    /// Mesuré sur RTX 4060 (`tests/media_foundation.rs::*_une_image_gardee_ne_bouge_pas`) :
    /// trois images gardées pendant 40 décodages restent intactes à l'octet ;
    /// le MFT H.264 (un tableau de 8 tranches) contourne les tranches gardées,
    /// le MFT HEVC (une texture d'une tranche par image, 6 en rotation) les
    /// textures gardées. Sans ce champ (échantillon relâché aussitôt), les
    /// images gardées sont réécrites dans les deux codecs. Combien d'images
    /// peuvent rester gardées avant que le décodeur ne cale : non mesuré.
    _echantillon: IMFSample,
}

impl ImageMf {
    /// La texture NV12 du décodeur, dont l'image occupe [`ImageMf::tranche`].
    /// Non lisible par un nuanceur : à copier.
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    pub fn tranche(&self) -> u32 {
        self.tranche
    }

    /// RÉSERVÉ AUX TESTS ET AUX MESURES : copie le plan de luminance (Y) de
    /// l'image en mémoire centrale, `largeur × hauteur` octets, ligne à ligne.
    /// Bloque jusqu'à la fin de la copie GPU.
    pub fn copier_luminance(&self) -> anyhow::Result<Vec<u8>> {
        use anyhow::Context;
        let mut source = D3D11_TEXTURE2D_DESC::default();
        unsafe { self.texture.GetDesc(&mut source) };
        // Une boîte qui déborde de la source fait ignorer la copie SANS erreur
        // (Direct3D 11) : on le refuse ici plutôt que de lire des zéros.
        anyhow::ensure!(
            self.largeur <= source.Width && self.hauteur <= source.Height,
            "image {}×{} plus grande que sa texture {}×{}",
            self.largeur,
            self.hauteur,
            source.Width,
            source.Height
        );
        let appareil = unsafe { self.texture.GetDevice() }.context("GetDevice")?;
        let contexte = unsafe { appareil.GetImmediateContext() }.context("GetImmediateContext")?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: self.largeur,
            Height: self.hauteur,
            MipLevels: 1,
            ArraySize: 1,
            Format: source.Format,
            SampleDesc: source.SampleDesc,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut lecture: Option<ID3D11Texture2D> = None;
        unsafe { appareil.CreateTexture2D(&desc, None, Some(&mut lecture)) }
            .context("CreateTexture2D (lecture)")?;
        let lecture = lecture.context("texture de lecture nulle")?;
        let boite = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: self.largeur,
            bottom: self.hauteur,
            back: 1,
        };
        let mut carte = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            contexte.CopySubresourceRegion(
                &lecture,
                0,
                0,
                0,
                0,
                &self.texture,
                self.tranche,
                Some(&boite),
            );
            contexte
                .Map(&lecture, 0, D3D11_MAP_READ, 0, Some(&mut carte))
                .context("Map (lecture)")?;
        }
        let (l, h) = (self.largeur as usize, self.hauteur as usize);
        let mut y = vec![0u8; l * h];
        for j in 0..h {
            let ligne = unsafe {
                std::slice::from_raw_parts(
                    (carte.pData as *const u8).add(j * carte.RowPitch as usize),
                    l,
                )
            };
            y[j * l..(j + 1) * l].copy_from_slice(ligne);
        }
        unsafe { contexte.Unmap(&lecture, 0) };
        Ok(y)
    }
}

/// Choisit NV12 en sortie et rend la taille d'affichage du type retenu.
///
/// Fonction libre et non méthode : pendant `nouveau`, le décodeur est encore en
/// construction.
///
/// Un type qui ne dit pas sa taille est une ERREUR, pas un repli : la sonde
/// repliait sur la taille annoncée, mais après un changement de flux la
/// dernière taille connue est précisément celle que le flux vient de
/// contredire. Une image de taille inventée se copierait mal (ou pas du tout :
/// une boîte qui déborde est ignorée par Direct3D 11 sans erreur). Aucun
/// relevé ne montre un tel type : le cas est supposé rare, jamais observé.
fn choisir_sortie(t: &IMFTransform) -> Result<(u32, u32), ErreurDecodeur> {
    let mut vus = Vec::new();
    for i in 0.. {
        let Ok(ty) = (unsafe { t.GetOutputAvailableType(0, i) }) else {
            break;
        };
        let sous_type = unsafe { ty.GetGUID(&MF_MT_SUBTYPE) }.unwrap_or_default();
        if sous_type == MFVideoFormat_NV12 {
            unsafe { t.SetOutputType(0, &ty, 0) }.map_err(|e| mf("SetOutputType(NV12)", e))?;
            return taille_d_affichage(&ty).ok_or_else(|| {
                ErreurDecodeur::MediaFoundation(
                    "le type de sortie NV12 ne porte aucune taille d'image".into(),
                )
            });
        }
        vus.push(format!("{sous_type:?}"));
    }
    Err(ErreurDecodeur::MediaFoundation(format!(
        "aucun type de sortie NV12 proposé (types vus : {})",
        vus.join(", ")
    )))
}

/// La taille d'AFFICHAGE : l'ouverture minimale si le type la porte (un flux
/// 1080 lignes est codé sur 1088), sinon la taille de l'image. `None` si le
/// type ne porte ni l'une ni l'autre (ou une taille nulle).
fn taille_d_affichage(ty: &IMFMediaType) -> Option<(u32, u32)> {
    let mut zone = MFVideoArea::default();
    let octets = unsafe {
        std::slice::from_raw_parts_mut(
            (&mut zone as *mut MFVideoArea).cast::<u8>(),
            std::mem::size_of::<MFVideoArea>(),
        )
    };
    let mut lus = 0u32;
    if unsafe { ty.GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, octets, Some(&mut lus)) }.is_ok()
        && lus as usize == std::mem::size_of::<MFVideoArea>()
        && zone.Area.cx > 0
        && zone.Area.cy > 0
    {
        return Some((zone.Area.cx as u32, zone.Area.cy as u32));
    }
    let taille = unsafe { ty.GetUINT64(&MF_MT_FRAME_SIZE) }.ok()?;
    let (largeur, hauteur) = ((taille >> 32) as u32, taille as u32);
    (largeur > 0 && hauteur > 0).then_some((largeur, hauteur))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sur_cette_machine_media_foundation_decode_hevc_et_h264_en_materiel() {
        // Machine du propriétaire : RTX 4060, extension HEVC installée (sonde du
        // 02/10/2026, `spike/mesures/sonde-amd/rapport-test-machine-nvidia.txt`).
        let (appareil, _) =
            crate::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
        assert!(
            mf_sait_decoder(&appareil, CodecMf::H264, 2560, 1440),
            "H.264 2560×1440"
        );
        assert!(
            mf_sait_decoder(&appareil, CodecMf::Hevc, 2560, 1440),
            "HEVC 2560×1440"
        );
    }

    #[test]
    fn une_taille_demesuree_est_refusee_par_dxva() {
        let (appareil, _) =
            crate::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
        // 16384×16384 dépasse toute limite DXVA connue (4096 sur le portable AMD, mesuré).
        assert!(!dxva_accepte(&appareil, CodecMf::H264, 16384, 16384));
        assert!(!mf_sait_decoder(&appareil, CodecMf::H264, 16384, 16384));
        // L'assertion qui prouve la garde DXVA de `nouveau` (neutralisation de
        // la tâche 4, mesurée sur la RTX 4060) : sans elle, le MFT HEVC
        // (« HEVCVideoExtension ») s'OUVRE en 16384×16384, alors que le MFT
        // H.264 refuse la taille de lui-même — l'assertion H.264 ci-dessus
        // reste donc verte sans la garde, deux gardes répondant l'une pour
        // l'autre.
        assert!(!dxva_accepte(&appareil, CodecMf::Hevc, 16384, 16384));
        assert!(
            !mf_sait_decoder(&appareil, CodecMf::Hevc, 16384, 16384),
            "HEVC 16384×16384"
        );
    }

    /// Ronde de correction 1, IMPORTANT 1 : seule sort l'image de l'unité
    /// poussée. Neutralisation : faire rendre à `image_de_l_unite` la dernière
    /// image tirée quel que soit son horodatage — `un_decodeur_en_retard…` et
    /// `l_image_de_l_unite_est_choisie…` rougissent.
    #[test]
    fn sans_image_tiree_l_unite_ne_rend_rien() {
        assert_eq!(image_de_l_unite::<&str>(7, Vec::new()), Ok(None));
    }

    #[test]
    fn l_image_de_l_unite_est_choisie_parmi_les_images_tirees() {
        assert_eq!(image_de_l_unite(7, vec![(7, "k")]), Ok(Some("k")));
        // Une image en retard sort AVANT celle de l'unité : elle est jetée.
        assert_eq!(
            image_de_l_unite(7, vec![(6, "k-1"), (7, "k")]),
            Ok(Some("k"))
        );
        // Et APRÈS : jetée aussi, la dernière tirée n'est pas la bonne.
        assert_eq!(
            image_de_l_unite(7, vec![(7, "k"), (6, "k-1")]),
            Ok(Some("k"))
        );
    }

    #[test]
    fn un_decodeur_en_retard_d_une_image_echoue_au_lieu_d_afficher_faux() {
        // L'appel k ne rend que k−1 : c'est une erreur, ni l'image de k−1
        // (afficher faux), ni `Ok(None)` (se taire).
        assert_eq!(image_de_l_unite(7, vec![(6, "k-1")]), Err(vec![6]));
    }

    /// Un horodatage qui déborderait en unités de 100 ns est refusé, jamais
    /// tronqué. Neutralisation : revenir à `horodatage_ms as i64 * 10_000` —
    /// débordement (panique en débogage), ce test rougit.
    #[test]
    fn un_horodatage_demesure_est_refuse() {
        let _plateforme = Plateforme::demarrer().expect("Media Foundation");
        assert!(matches!(
            echantillon_de(&[0], u64::MAX / 10_000),
            Err(ErreurDecodeur::MediaFoundation(_))
        ));
        assert!(echantillon_de(&[0], 1_000).is_ok(), "contrôle positif");
    }

    /// Constat 2 de la revue de la tâche 4 : jamais une taille nulle. Un type
    /// qui ne porte aucune taille, ou une taille nulle, n'en donne aucune —
    /// `choisir_sortie` en fait une erreur. Neutralisation (mesurée) : rendre
    /// `Some((largeur, hauteur))` sans la garde `> 0` — « hauteur nulle »
    /// rougit sur `Some((1920, 0))`.
    #[test]
    fn un_type_sans_taille_ne_donne_aucune_taille() {
        let _plateforme = Plateforme::demarrer().expect("Media Foundation");
        let ty = unsafe { MFCreateMediaType() }.expect("type");
        assert_eq!(taille_d_affichage(&ty), None, "type sans aucune taille");
        unsafe { ty.SetUINT64(&MF_MT_FRAME_SIZE, 1920u64 << 32) }.expect("taille");
        assert_eq!(taille_d_affichage(&ty), None, "hauteur nulle");
        unsafe { ty.SetUINT64(&MF_MT_FRAME_SIZE, (1920u64 << 32) | 1088) }.expect("taille");
        assert_eq!(
            taille_d_affichage(&ty),
            Some((1920, 1088)),
            "contrôle positif"
        );
    }

    /// RELEVÉ, pas une preuve : ce que l'énumération choisit sur cette machine,
    /// et ce que le décodeur ouvert déclare. Lire avec `--nocapture`.
    #[test]
    fn releve_du_decodeur_choisi() {
        use windows::core::PWSTR;
        let (appareil, _) =
            crate::creer_appareil_video().expect("périphérique Direct3D 11 matériel");
        for codec in [CodecMf::Hevc, CodecMf::H264] {
            let _plateforme = Plateforme::demarrer().expect("Media Foundation");
            let activations = enumerer(codec).expect("énumération");
            for (i, a) in activations.iter().enumerate() {
                let mut nom = PWSTR::null();
                let mut n = 0u32;
                let texte = match unsafe {
                    a.GetAllocatedString(&MFT_FRIENDLY_NAME_Attribute, &mut nom, &mut n)
                } {
                    Ok(()) => {
                        let s = unsafe { nom.to_string() }.unwrap_or_default();
                        unsafe { CoTaskMemFree(Some(nom.0 as _)) };
                        s
                    }
                    Err(_) => "(sans nom)".into(),
                };
                let drapeaux = unsafe { a.GetUINT32(&MF_TRANSFORM_FLAGS_Attribute) }.unwrap_or(0);
                println!("{codec:?} n° {i} : « {texte} », drapeaux d'énumération 0x{drapeaux:X}");
            }
            let d = DecodeurMf::nouveau(codec, &appareil, 2560, 1440).expect("ouverture");
            let attributs = unsafe { d.transformation.GetAttributes() }.expect("attributs");
            let d3d11 = unsafe { attributs.GetUINT32(&MF_SA_D3D11_AWARE) };
            let asynchrone = unsafe { attributs.GetUINT32(&MF_TRANSFORM_ASYNC) };
            let faible = unsafe { attributs.GetUINT32(&MF_LOW_LATENCY) };
            println!(
                "{codec:?} ouvert : MF_SA_D3D11_AWARE {d3d11:?}, MF_TRANSFORM_ASYNC {asynchrone:?}, \
                 MF_LOW_LATENCY {faible:?}, flux de sortie : drapeaux 0x{:X}, cbSize {}, taille {}×{}",
                d.info.dwFlags, d.info.cbSize, d.largeur, d.hauteur
            );
        }
    }
}
