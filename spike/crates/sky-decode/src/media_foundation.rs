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
//! Cette tâche n'écrit que l'OUVERTURE du décodeur ([`DecodeurMf::nouveau`]) :
//! la sonde de décodage s'en sert, et il n'en existe qu'une. Le décodage
//! lui-même (`decoder`) vient à la tâche suivante.

use std::mem::ManuallyDrop;

use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11VideoDevice, D3D11_VIDEO_DECODER_DESC,
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
    #[allow(dead_code)] // lu par `decoder` (tâche 5)
    info: MFT_OUTPUT_STREAM_INFO,
    #[allow(dead_code)] // lu par `decoder` (tâche 5)
    largeur: u32,
    #[allow(dead_code)] // lu par `decoder` (tâche 5)
    hauteur: u32,
    _plateforme: Plateforme,
}

impl Drop for DecodeurMf {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .transformation
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
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
        unsafe { attributs.SetUINT32(&MF_LOW_LATENCY, 1) }
            .map_err(|e| mf("MF_LOW_LATENCY = 1", e))?;

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

        unsafe {
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(|e| mf("NOTIFY_BEGIN_STREAMING", e))?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|e| mf("NOTIFY_START_OF_STREAM", e))?;
        }
        decodeur.info = info;
        decodeur.largeur = largeur;
        decodeur.hauteur = hauteur;
        Ok(decodeur)
    }
}

/// Choisit NV12 en sortie et rend la taille d'affichage du type retenu.
///
/// Fonction libre et non méthode : pendant `nouveau`, le décodeur est encore en
/// construction.
fn choisir_sortie(t: &IMFTransform) -> Result<(u32, u32), ErreurDecodeur> {
    let mut vus = Vec::new();
    for i in 0.. {
        let Ok(ty) = (unsafe { t.GetOutputAvailableType(0, i) }) else {
            break;
        };
        let sous_type = unsafe { ty.GetGUID(&MF_MT_SUBTYPE) }.unwrap_or_default();
        if sous_type == MFVideoFormat_NV12 {
            unsafe { t.SetOutputType(0, &ty, 0) }.map_err(|e| mf("SetOutputType(NV12)", e))?;
            return Ok(taille_d_affichage(&ty));
        }
        vus.push(format!("{sous_type:?}"));
    }
    Err(ErreurDecodeur::MediaFoundation(format!(
        "aucun type de sortie NV12 proposé (types vus : {})",
        vus.join(", ")
    )))
}

/// La taille d'AFFICHAGE : l'ouverture minimale si le type la porte (un flux
/// 1080 lignes est codé sur 1088), sinon la taille de l'image.
fn taille_d_affichage(ty: &IMFMediaType) -> (u32, u32) {
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
        return (zone.Area.cx as u32, zone.Area.cy as u32);
    }
    let taille = unsafe { ty.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(0);
    ((taille >> 32) as u32, taille as u32)
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
