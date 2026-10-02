//! Les formats vidéo que SkyShare sait transporter, et leur écriture SDP.
//!
//! Une seule table pour les deux côtés : le spectateur y prend ce qu'il sait
//! décoder, l'hôte ce qu'il sait encoder, et `PeerLink::format_negocie` en
//! déduit le même format des deux côtés (spec §4, `sky-net`).

use str0m::format::{Codec, PayloadParams};

/// Profil HEVC « Format Range Extensions » (`profile-id=4`), où vit le 4:4:4.
pub(crate) const PROFIL_HEVC_444: u8 = 4;
/// Profil HEVC « Main » (`profile-id=1`) : 8 bits, 4:2:0.
pub(crate) const PROFIL_HEVC_MAIN: u8 = 1;
/// Palier « Main » (`tier-flag=0`).
pub(crate) const TIER_MAIN: u8 = 0;
/// Niveau HEVC 6.0 : `level_id = (6 * 10 + 0) * 3 = 180` (ITU-T H.265 Annexe A).
pub(crate) const NIVEAU_HEVC_6_0: u8 = 180;
/// `profile-level-id` H.264 : profil High (0x64), sans contrainte (0x00),
/// niveau 5.2 (0x34). High est le profil que NVENC produit
/// (`NV_ENC_H264_PROFILE_HIGH_GUID`) ; 5.2 couvre 2560×1440 à 60 images/s.
/// `str0m` exige l'égalité du PROFIL ; un écart de niveau ne fait que baisser
/// le score de concordance (lu dans `payload_params.rs`, `match_h264_score`).
pub(crate) const PROFIL_NIVEAU_H264: u32 = 0x64_00_34;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatVideo {
    /// HEVC 4:4:4, décodé par NVDEC.
    Hevc444,
    /// HEVC Main 4:2:0, décodé par Media Foundation.
    Hevc420,
    /// H.264 High 4:2:0, décodé par Media Foundation.
    H264,
}

impl FormatVideo {
    /// L'ordre de préférence, le même pour les deux côtés. Le 4:4:4 d'abord :
    /// c'est la couleur pleine résolution, la raison d'être du projet.
    pub const PREFERENCE: [FormatVideo; 3] = [FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264];

    /// Le nom montré à l'utilisateur dans les mesures.
    pub fn libelle(self) -> &'static str {
        match self {
            FormatVideo::Hevc444 => "HEVC 4:4:4",
            FormatVideo::Hevc420 => "HEVC 4:2:0",
            FormatVideo::H264 => "H.264",
        }
    }

    /// Type de charge RTP. Fixe et propre à chaque format : l'hôte adopte ceux de
    /// l'offre (`str0m`, réponse à un offrant `RecvOnly`), et les deux côtés sont
    /// SkyShare — la table suffit donc à relire un type de charge retenu.
    pub fn type_de_charge(self) -> u8 {
        match self {
            FormatVideo::Hevc444 => 102,
            FormatVideo::Hevc420 => 104,
            FormatVideo::H264 => 106,
        }
    }

    /// Type de charge des retransmissions (RTX).
    pub fn retransmission(self) -> u8 {
        self.type_de_charge() + 1
    }

    /// Le format que décrivent des paramètres négociés, s'il est l'un des nôtres.
    pub fn depuis_parametres(p: &PayloadParams) -> Option<FormatVideo> {
        let spec = p.spec();
        match spec.codec {
            Codec::H264 => Some(FormatVideo::H264),
            Codec::H265 => match spec.format.h265_profile_tier_level.map(|ptl| ptl.profile_id()) {
                Some(PROFIL_HEVC_444) => Some(FormatVideo::Hevc444),
                Some(PROFIL_HEVC_MAIN) => Some(FormatVideo::Hevc420),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_types_de_charge_sont_distincts_et_suivis_de_leur_retransmission() {
        let mut vus = std::collections::HashSet::new();
        for f in FormatVideo::PREFERENCE {
            assert!(vus.insert(f.type_de_charge()), "{f:?} : type de charge en double");
            assert!(vus.insert(f.retransmission()), "{f:?} : retransmission en double");
        }
    }

    #[test]
    fn la_preference_commence_par_la_couleur_pleine_resolution() {
        assert_eq!(FormatVideo::PREFERENCE[0], FormatVideo::Hevc444);
        assert_eq!(FormatVideo::PREFERENCE.len(), 3);
    }
}
