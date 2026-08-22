/// Les combinaisons codec + sous-échantillonnage que NVENC sait produire.
///
/// Note matérielle : NVENC ne fait pas de 4:4:4 en AV1, même sur Ada (RTX 40).
/// Le 4:4:4 — indispensable pour que le texte reste lisible — n'existe qu'en
/// H.264 et HEVC. C'est l'arbitrage central du partage d'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264_420,
    H264_444,
    Hevc444,
    Av1_420,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264_420 => "H.264 4:2:0",
            Codec::H264_444 => "H.264 4:4:4",
            Codec::Hevc444 => "HEVC 4:4:4",
            Codec::Av1_420 => "AV1 4:2:0",
        }
    }

    pub fn is_444(self) -> bool {
        matches!(self, Codec::H264_444 | Codec::Hevc444)
    }
}

#[derive(Debug, Clone)]
pub struct EncoderCaps {
    pub gpu_name: String,
    pub codecs: Vec<Codec>,
}

/// Choisit le meilleur codec disponible.
///
/// `prefer_text` = true privilégie la netteté du texte (4:4:4) sur l'efficacité.
/// Ordre : HEVC 4:4:4 > H.264 4:4:4 > AV1 4:2:0 > H.264 4:2:0
/// Sans exigence de texte : AV1 4:2:0 > HEVC 4:4:4 > H.264 4:4:4 > H.264 4:2:0
pub fn pick_best(caps: &EncoderCaps, prefer_text: bool) -> Option<Codec> {
    let ordre: &[Codec] = if prefer_text {
        &[
            Codec::Hevc444,
            Codec::H264_444,
            Codec::Av1_420,
            Codec::H264_420,
        ]
    } else {
        &[
            Codec::Av1_420,
            Codec::Hevc444,
            Codec::H264_444,
            Codec::H264_420,
        ]
    };
    ordre.iter().copied().find(|c| caps.codecs.contains(c))
}

/// Erreurs de détection matérielle NVENC.
///
/// Écart avec le brief : l'interface déclare `Result<EncoderCaps, EncodeError>`
/// (pas `anyhow::Result` comme le montrait le code du Step 7). On définit donc
/// ici un type d'erreur dédié avec `thiserror`, seul moyen cohérent d'honorer
/// à la fois la signature de l'interface et la dépendance `thiserror` déclarée
/// pour cette crate.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("aucun GPU NVIDIA utilisable : {0}")]
    Cuda(#[from] cudarc::driver::DriverError),

    #[error("impossible de charger nvEncodeAPI64.dll : {0}")]
    Dll(#[from] libloading::Error),

    #[error("appel NVENC échoué : {0:?}")]
    Nvenc(NVENCSTATUS),
}

use std::ffi::c_void;

use libloading::{Library, Symbol};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    GUID, NVENCAPI_VERSION, NVENCSTATUS, NV_ENCODE_API_FUNCTION_LIST,
    NV_ENCODE_API_FUNCTION_LIST_VER, NV_ENC_BUFFER_FORMAT, NV_ENC_CODEC_AV1_GUID,
    NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID, NV_ENC_DEVICE_TYPE,
    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
};

/// Nom de la DLL livrée avec le driver NVIDIA (`C:\Windows\System32`),
/// distincte du NVIDIA Video Codec SDK : le SDK ne fournit qu'une bibliothèque
/// d'import statique (`nvEncodeAPI.lib`), pas le code exécutable. FFmpeg et
/// OBS ne lient pas non plus contre ce .lib : ils font `LoadLibrary` puis
/// `GetProcAddress("NvEncodeAPICreateInstance")` au runtime — même approche
/// ici, via la crate `libloading`.
const NVENC_DLL: &str = "nvEncodeAPI64.dll";

type CreateInstanceFn = unsafe extern "C" fn(*mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

// `nvidia-video-codec-sdk` déclare `NvEncodeAPICreateInstance` et
// `NvEncodeAPIGetMaxSupportedVersion` dans un bloc `extern "C"` lié
// statiquement (son module `safe`, que `probe_hardware` n'utilise jamais —
// voir plus bas). En release, le lien final élimine ce code mort ; en debug
// sur MSVC, l'initialiseur paresseux de `safe::api::ENCODE_API` (une
// `lazy_static`) reste dans le binaire et ces deux symboles restent exigés,
// même si rien ne les appelle jamais. On leur fournit donc nous-mêmes une
// définition ici : cela satisfait le lien, et elle ne s'exécute jamais tant
// que `sky_encode` n'appelle pas `nvidia_video_codec_sdk::safe::Encoder`
// (ce que `probe_hardware` ne fait pas — il charge NVENC dynamiquement).
#[no_mangle]
extern "C" fn NvEncodeAPICreateInstance(
    _function_list: *mut NV_ENCODE_API_FUNCTION_LIST,
) -> NVENCSTATUS {
    unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-encode")
}

#[no_mangle]
extern "C" fn NvEncodeAPIGetMaxSupportedVersion(_version: *mut u32) -> NVENCSTATUS {
    unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-encode")
}

/// Convertit un code retour NVENC en `Result`.
fn check(status: NVENCSTATUS) -> Result<(), EncodeError> {
    if status == NVENCSTATUS::NV_ENC_SUCCESS {
        Ok(())
    } else {
        Err(EncodeError::Nvenc(status))
    }
}

/// Interroge NVENC pour savoir ce que la carte sait réellement encoder.
///
/// On ne se fie pas au nom du GPU : les capacités dépendent aussi du driver.
/// On demande donc à NVENC lui-même, codec par codec, quels formats d'entrée
/// il accepte — c'est la seule source de vérité.
///
/// Implémentation directe sur `sys` (pas `safe::Encoder` de la crate) :
/// `safe::Encoder` appelle `NvEncodeAPICreateInstance` via un bloc
/// `extern "C"` lié statiquement, ce qui exigerait `nvEncodeAPI.lib` du SDK
/// NVIDIA — absent ici, et absent de tout runner CI qui n'installe pas ce
/// SDK. En réalité, `NvEncodeAPICreateInstance` est livrée dans
/// `nvEncodeAPI64.dll`, aux côtés du driver (vérifié présent dans
/// `System32`). On la charge donc dynamiquement, puis on appelle tout le
/// reste de NVENC à travers la table de pointeurs qu'elle remplit : c'est le
/// *seul* symbole qui a besoin d'être résolu.
pub fn probe_hardware() -> Result<EncoderCaps, EncodeError> {
    let cuda = cudarc::driver::CudaContext::new(0)?;
    let gpu_name = cuda.name().unwrap_or_else(|_| "GPU NVIDIA".into());

    // `lib` doit rester en vie tant qu'on appelle un pointeur de fonction qui
    // pointe dedans — y compris ceux de `function_list`, remplie par un appel
    // à travers `lib`.
    let lib = unsafe { Library::new(NVENC_DLL) }?;
    let create_instance: Symbol<CreateInstanceFn> =
        unsafe { lib.get(b"NvEncodeAPICreateInstance\0") }?;

    let mut function_list = NV_ENCODE_API_FUNCTION_LIST {
        version: NV_ENCODE_API_FUNCTION_LIST_VER,
        ..Default::default()
    };
    check(unsafe { create_instance(&mut function_list) })?;

    const MSG: &str = "la table de fonctions NVENC doit être remplie par NvEncodeAPICreateInstance";
    let open_session_ex = function_list.nvEncOpenEncodeSessionEx.expect(MSG);
    let get_guid_count = function_list.nvEncGetEncodeGUIDCount.expect(MSG);
    let get_guids = function_list.nvEncGetEncodeGUIDs.expect(MSG);
    let get_format_count = function_list.nvEncGetInputFormatCount.expect(MSG);
    let get_formats = function_list.nvEncGetInputFormats.expect(MSG);
    let destroy_encoder = function_list.nvEncDestroyEncoder.expect(MSG);

    // Ouvre une session NVENC sur le contexte CUDA (cast valide : `CUcontext`
    // est un pointeur opaque, exactement ce que NVENC attend comme `device`).
    let mut session_params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
        version: NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
        deviceType: NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_CUDA,
        apiVersion: NVENCAPI_VERSION,
        device: cuda.cu_ctx().cast::<c_void>(),
        ..Default::default()
    };
    let mut session: *mut c_void = std::ptr::null_mut();
    check(unsafe { open_session_ex(&mut session_params, &mut session) })?;

    let codecs = enumerate_codecs(
        session,
        get_guid_count,
        get_guids,
        get_format_count,
        get_formats,
    );

    // Ferme la session dans tous les cas — que l'énumération ait réussi ou non.
    let destroy_result = check(unsafe { destroy_encoder(session) });
    let codecs = codecs?;
    destroy_result?;

    Ok(EncoderCaps { gpu_name, codecs })
}

/// Énumère les GUID de codecs supportés, puis pour chacun les formats
/// d'entrée, pour en déduire la liste de [`Codec`] réellement disponibles.
fn enumerate_codecs(
    session: *mut c_void,
    get_guid_count: unsafe extern "C" fn(*mut c_void, *mut u32) -> NVENCSTATUS,
    get_guids: unsafe extern "C" fn(*mut c_void, *mut GUID, u32, *mut u32) -> NVENCSTATUS,
    get_format_count: unsafe extern "C" fn(*mut c_void, GUID, *mut u32) -> NVENCSTATUS,
    get_formats: unsafe extern "C" fn(
        *mut c_void,
        GUID,
        *mut NV_ENC_BUFFER_FORMAT,
        u32,
        *mut u32,
    ) -> NVENCSTATUS,
) -> Result<Vec<Codec>, EncodeError> {
    let mut guid_count = 0u32;
    check(unsafe { get_guid_count(session, &mut guid_count) })?;
    let mut guids = vec![GUID::default(); guid_count as usize];
    let mut actual_guid_count = 0u32;
    check(unsafe {
        get_guids(
            session,
            guids.as_mut_ptr(),
            guid_count,
            &mut actual_guid_count,
        )
    })?;
    guids.truncate(actual_guid_count as usize);

    let mut codecs = Vec::new();
    for guid in guids {
        let mut format_count = 0u32;
        check(unsafe { get_format_count(session, guid, &mut format_count) })?;
        let mut formats =
            vec![NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED; format_count as usize];
        let mut actual_format_count = 0u32;
        check(unsafe {
            get_formats(
                session,
                guid,
                formats.as_mut_ptr(),
                format_count,
                &mut actual_format_count,
            )
        })?;
        formats.truncate(actual_format_count as usize);

        let a_444 = formats.iter().any(|f| {
            matches!(
                *f,
                NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_YUV444
                    | NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_YUV444_10BIT
            )
        });

        if guid == NV_ENC_CODEC_H264_GUID {
            codecs.push(Codec::H264_420);
            if a_444 {
                codecs.push(Codec::H264_444);
            }
        } else if guid == NV_ENC_CODEC_HEVC_GUID && a_444 {
            codecs.push(Codec::Hevc444);
        } else if guid == NV_ENC_CODEC_AV1_GUID {
            codecs.push(Codec::Av1_420);
        }
    }

    Ok(codecs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps_rtx4060() -> EncoderCaps {
        EncoderCaps {
            gpu_name: "NVIDIA GeForce RTX 4060".into(),
            codecs: vec![
                Codec::H264_420,
                Codec::H264_444,
                Codec::Hevc444,
                Codec::Av1_420,
            ],
        }
    }

    #[test]
    fn prefere_hevc444_pour_le_texte() {
        // Pour du partage d'écran (texte, code), la couleur pleine résolution
        // prime sur l'efficacité de compression.
        assert_eq!(pick_best(&caps_rtx4060(), true), Some(Codec::Hevc444));
    }

    #[test]
    fn prefere_av1_pour_la_video() {
        // Sans exigence de texte net, AV1 gagne : ~40 % de débit en moins.
        assert_eq!(pick_best(&caps_rtx4060(), false), Some(Codec::Av1_420));
    }

    #[test]
    fn retombe_sur_h264_444_si_hevc_absent() {
        let caps = EncoderCaps {
            gpu_name: "GTX 970".into(),
            codecs: vec![Codec::H264_420, Codec::H264_444],
        };
        assert_eq!(pick_best(&caps, true), Some(Codec::H264_444));
    }

    #[test]
    fn retombe_sur_420_si_aucun_444() {
        let caps = EncoderCaps {
            gpu_name: "vieux GPU".into(),
            codecs: vec![Codec::H264_420],
        };
        assert_eq!(pick_best(&caps, true), Some(Codec::H264_420));
    }

    #[test]
    fn aucun_codec_disponible() {
        let caps = EncoderCaps {
            gpu_name: "aucun".into(),
            codecs: vec![],
        };
        assert_eq!(pick_best(&caps, true), None);
    }
}
