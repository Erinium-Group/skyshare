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

use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NVENCAPI_VERSION, NVENCSTATUS, NV_ENC_BUFFER_FORMAT, NV_ENC_CODEC_AV1_GUID,
    NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID, NV_ENC_DEVICE_TYPE,
    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
};

use crate::nvenc_sys::{self, NvencApi};

/// Interroge NVENC pour savoir ce que la carte sait réellement encoder.
///
/// On ne se fie pas au nom du GPU : les capacités dépendent aussi du driver.
/// On demande donc à NVENC lui-même, codec par codec, quels formats d'entrée
/// il accepte — c'est la seule source de vérité.
///
/// La plomberie FFI (chargement de la DLL, table de fonctions, énumération
/// brute des GUID/formats) vit dans [`crate::nvenc_sys`] — partagée avec la
/// future session d'encodage D3D11. Ici, on ne fait qu'ouvrir une session sur
/// le device CUDA et traduire les GUID + formats bruts en [`Codec`].
pub fn probe_hardware() -> Result<EncoderCaps, EncodeError> {
    let cuda = cudarc::driver::CudaContext::new(0)?;
    let gpu_name = cuda.name().unwrap_or_else(|_| "GPU NVIDIA".into());

    let api = NvencApi::load()?;
    let fns = api.functions();

    const MSG: &str = "la table de fonctions NVENC doit être remplie par NvEncodeAPICreateInstance";
    let open_session_ex = fns.nvEncOpenEncodeSessionEx.expect(MSG);
    let destroy_encoder = fns.nvEncDestroyEncoder.expect(MSG);

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
    nvenc_sys::check(unsafe { open_session_ex(&mut session_params, &mut session) })?;

    let raw = nvenc_sys::enumerate_codec_formats(session, fns);

    // Ferme la session dans tous les cas — que l'énumération ait réussi ou non.
    let destroy_result = nvenc_sys::check(unsafe { destroy_encoder(session) });
    let raw = raw?;
    destroy_result?;

    let mut codecs = Vec::new();
    for (guid, formats) in raw {
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

    Ok(EncoderCaps { gpu_name, codecs })
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
