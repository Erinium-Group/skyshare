use cudarc::driver::sys::CUresult;
use nvidia_video_codec_sdk::sys::cuviddec::{
    cudaVideoChromaFormat, cudaVideoCodec, cudaVideoSurfaceFormat, CUVIDDECODECAPS,
};

use crate::nvcuvid_sys::NvcuvidApi;

/// Ce que le décodeur de cette machine sait faire.
#[derive(Debug, Clone)]
pub struct Capacites {
    pub hevc_444: bool,
    pub largeur_max: u32,
    pub hauteur_max: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ErreurDecodeur {
    #[error(
        "cette machine n'a pas de décodeur NVIDIA.\n\n\
         La bibliothèque nvcuvid.dll est installée avec le pilote NVIDIA : son \
         absence signifie qu'il n'y a pas de carte NVIDIA, ou que le pilote n'est \
         pas installé.\n\n\
         Détail technique : {0}"
    )]
    AucuneCarteNvidia(String),

    #[error(
        "le décodeur de cette carte ne prend pas en charge le HEVC 4:4:4. Cette \
         machine peut partager un écran, mais pas en recevoir un."
    )]
    QuatreQuatreQuatreNonPris,

    #[error("le décodeur NVIDIA a refusé d'ouvrir une session (code {0})")]
    SessionRefusee(i32),
}

impl Capacites {
    /// Le verdict, séparé de l'appel matériel pour être prouvable sans GPU.
    ///
    /// La source de vérité est `nOutputFormatMask`, pas `bIsSupported` seul : la
    /// sonde du 27/09/2026 a mesuré que pour notre flux HEVC 4:4:4, le masque
    /// n'offre MÊME PAS NV12 — autrement dit NVDEC ne *peut pas* dégrader vers
    /// du 4:2:0. Réciproquement, une carte qui n'offre que NV12 ne sait pas
    /// rendre notre chrominance pleine résolution : il faut refuser (décision D8),
    /// jamais convertir en silence.
    pub fn depuis_brut(brut: &CUVIDDECODECAPS) -> Result<Capacites, ErreurDecodeur> {
        let bit_444 = 1u16 << (cudaVideoSurfaceFormat::cudaVideoSurfaceFormat_YUV444 as u16);
        if brut.bIsSupported == 0 || brut.nOutputFormatMask & bit_444 == 0 {
            return Err(ErreurDecodeur::QuatreQuatreQuatreNonPris);
        }
        Ok(Capacites {
            hevc_444: true,
            largeur_max: brut.nMaxWidth,
            hauteur_max: brut.nMaxHeight,
        })
    }
}

/// Interroge le décodeur NVIDIA de cette machine sur le HEVC 4:4:4 8 bits.
///
/// Une absence de pilote, de carte ou de `nvcuvid.dll` devient
/// [`ErreurDecodeur::AucuneCarteNvidia`] : pour l'utilisateur c'est la même
/// situation, et le détail technique reste dans le message.
pub fn sonder_materiel() -> Result<Capacites, ErreurDecodeur> {
    // `cuvidGetDecoderCaps` exige un contexte CUDA courant sur ce fil : le
    // contexte doit donc vivre jusqu'à la fin de l'appel.
    let _contexte = cudarc::driver::CudaContext::new(0)
        .map_err(|e| ErreurDecodeur::AucuneCarteNvidia(e.to_string()))?;
    let api = NvcuvidApi::load().map_err(|e| ErreurDecodeur::AucuneCarteNvidia(e.to_string()))?;

    let mut brut: CUVIDDECODECAPS = unsafe { std::mem::zeroed() };
    brut.eCodecType = cudaVideoCodec::cudaVideoCodec_HEVC;
    brut.eChromaFormat = cudaVideoChromaFormat::cudaVideoChromaFormat_444;
    brut.nBitDepthMinus8 = 0;

    let code = unsafe { (api.cuvid_get_decoder_caps)(&mut brut) };
    if code != CUresult::CUDA_SUCCESS {
        return Err(ErreurDecodeur::SessionRefusee(code as i32));
    }
    Capacites::depuis_brut(&brut)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fabrique des capacités brutes plausibles ; chaque test n'en change qu'un champ.
    fn brut_capable() -> CUVIDDECODECAPS {
        let mut brut: CUVIDDECODECAPS = unsafe { std::mem::zeroed() };
        brut.eCodecType = cudaVideoCodec::cudaVideoCodec_HEVC;
        brut.eChromaFormat = cudaVideoChromaFormat::cudaVideoChromaFormat_444;
        brut.nBitDepthMinus8 = 0;
        brut.bIsSupported = 1;
        // Bit 2 = cudaVideoSurfaceFormat_YUV444.
        brut.nOutputFormatMask = 1 << 2;
        brut.nMaxWidth = 4096;
        brut.nMaxHeight = 4096;
        brut
    }

    #[test]
    fn une_carte_capable_rend_un_verdict_positif() {
        let caps = Capacites::depuis_brut(&brut_capable()).expect("doit être acceptée");
        assert!(caps.hevc_444);
        assert_eq!(caps.largeur_max, 4096);
    }

    #[test]
    fn sans_format_de_sortie_444_le_verdict_refuse() {
        let mut brut = brut_capable();
        // La carte se dit capable, mais n'offre que du NV12 (bit 0) : c'est le cas
        // des cartes antérieures à Turing. Mesuré à la sonde du 27/09 : pour notre
        // flux, une carte capable n'offre MÊME PAS NV12 — le masque est donc la
        // source de vérité, pas `bIsSupported` seul.
        brut.nOutputFormatMask = 1 << 0;
        let refus = Capacites::depuis_brut(&brut).expect_err("doit être refusée");
        assert!(matches!(refus, ErreurDecodeur::QuatreQuatreQuatreNonPris));
    }

    #[test]
    fn un_codec_non_pris_en_charge_refuse() {
        let mut brut = brut_capable();
        brut.bIsSupported = 0;
        let refus = Capacites::depuis_brut(&brut).expect_err("doit être refusée");
        assert!(matches!(refus, ErreurDecodeur::QuatreQuatreQuatreNonPris));
    }

    #[test]
    fn sonde_materielle_dit_ce_qu_elle_trouve() {
        match sonder_materiel() {
            Ok(caps) => {
                assert!(caps.hevc_444);
                assert!(caps.largeur_max >= 2560, "il faut au moins 2560 de large");
                println!(
                    "décodeur HEVC 4:4:4 présent, jusqu'à {}×{}",
                    caps.largeur_max, caps.hauteur_max
                );
            }
            Err(e) => println!("pas de décodage 4:4:4 sur cette machine : {e}"),
        }
    }
}
