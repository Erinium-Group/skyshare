//! Ce que cette machine sait décoder, établi AVANT toute négociation.

use crate::media_foundation::{mf_sait_decoder, CodecMf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decodables {
    /// NVDEC, HEVC 4:4:4, à la taille demandée.
    pub hevc_444: bool,
    /// Media Foundation matériel, HEVC Main 4:2:0.
    pub hevc_420: bool,
    /// Media Foundation matériel, H.264.
    pub h264: bool,
}

/// Interroge les deux moteurs. Ne panique pas sans NVIDIA (`sonder_materiel`
/// vérifie `nvcuda.dll` avant `cuInit`), et tout échec d'un appel Media
/// Foundation rend `false`. MAIS sans Media Foundation du tout, cette fonction
/// n'est jamais atteinte : `mfplat.dll` est importée STATIQUEMENT (le crate
/// `windows` 0.62 lie en `raw-dylib` ; l'import figure dans `dumpbin /dependents`
/// de l'exécutable, que `spike/scripts/version-portable.ps1` vérifie), donc sur une édition « N » de Windows sans Media Feature Pack le processus ne
/// démarre pas, même pour partager (spec du 02/10/2026, §9 ; non vérifié, aucune
/// machine N). Le remède, un chargement différé, est reporté.
pub fn sonder_decodage(largeur: u32, hauteur: u32) -> Decodables {
    let hevc_444 = matches!(
        crate::sonder_materiel(),
        Ok(c) if c.hevc_444 && c.largeur_max >= largeur && c.hauteur_max >= hauteur
    );
    // Le périphérique de la sonde est créé par la même fonction que celui de la
    // fenêtre : même adaptateur, donc la carte validée ici est celle qui
    // décodera.
    let (hevc_420, h264) = match crate::creer_appareil_video() {
        Ok((appareil, _)) => (
            mf_sait_decoder(&appareil, CodecMf::Hevc, largeur, hauteur),
            mf_sait_decoder(&appareil, CodecMf::H264, largeur, hauteur),
        ),
        Err(_) => (false, false),
    };
    Decodables {
        hevc_444,
        hevc_420,
        h264,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sur_cette_machine_les_trois_formats_sont_decodables() {
        // Machine du propriétaire : RTX 4060 (NVDEC HEVC 4:4:4) et extension HEVC.
        assert_eq!(
            sonder_decodage(2560, 1440),
            Decodables {
                hevc_444: true,
                hevc_420: true,
                h264: true
            }
        );
    }

    #[test]
    fn une_taille_demesuree_n_est_decodable_par_media_foundation_dans_aucun_codec() {
        let d = sonder_decodage(16384, 16384);
        assert!(!d.hevc_420 && !d.h264, "{d:?}");
    }
}
