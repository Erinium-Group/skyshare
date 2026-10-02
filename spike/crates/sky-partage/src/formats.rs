//! Passages entre ce que les cartes savent faire et ce que la piste transporte.
//!
//! Un seul endroit, pour l'hôte comme pour le spectateur : deux traductions
//! écrites à deux endroits finiraient par diverger.

use sky_decode::Decodables;
use sky_encode::Codec;
use sky_net::FormatVideo;

/// Les formats transmissibles parmi ce que l'encodeur sait produire, dans
/// l'ordre de préférence commun. `H264_444` et `Av1_420` n'y figurent pas :
/// aucune piste ne les négocie.
pub fn formats_encodables(codecs: &[Codec]) -> Vec<FormatVideo> {
    FormatVideo::PREFERENCE
        .into_iter()
        .filter(|f| codecs.contains(&codec_de(*f)))
        .collect()
}

/// Le réglage NVENC qui produit un format.
pub fn codec_de(format: FormatVideo) -> Codec {
    match format {
        FormatVideo::Hevc444 => Codec::Hevc444,
        FormatVideo::Hevc420 => Codec::Hevc420,
        FormatVideo::H264 => Codec::H264_420,
    }
}

/// Les formats que ce spectateur sait décoder, dans l'ordre de préférence.
pub fn formats_decodables(d: &Decodables) -> Vec<FormatVideo> {
    FormatVideo::PREFERENCE
        .into_iter()
        .filter(|f| match f {
            FormatVideo::Hevc444 => d.hevc_444,
            FormatVideo::Hevc420 => d.hevc_420,
            FormatVideo::H264 => d.h264,
        })
        .collect()
}

/// Ce que le spectateur offre : ce qu'il décode, restreint (jamais élargi) aux
/// formats imposés par `sky-probe view --format`.
pub fn formats_a_offrir(decodables: Vec<FormatVideo>, imposes: Option<&[FormatVideo]>) -> Vec<FormatVideo> {
    match imposes {
        Some(imposes) => decodables.into_iter().filter(|f| imposes.contains(f)).collect(),
        None => decodables,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_rtx_4060_transmet_les_trois_formats_dans_l_ordre() {
        let codecs = [Codec::H264_420, Codec::H264_444, Codec::Hevc420, Codec::Hevc444, Codec::Av1_420];
        assert_eq!(
            formats_encodables(&codecs),
            vec![FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264]
        );
    }

    #[test]
    fn une_carte_sans_hevc_transmet_le_h264() {
        assert_eq!(formats_encodables(&[Codec::H264_420]), vec![FormatVideo::H264]);
    }

    #[test]
    fn h264_444_et_av1_ne_sont_pas_transmissibles() {
        assert!(formats_encodables(&[Codec::H264_444, Codec::Av1_420]).is_empty());
    }

    #[test]
    fn un_portable_amd_decode_le_hevc_420_et_le_h264() {
        let d = Decodables { hevc_444: false, hevc_420: true, h264: true };
        assert_eq!(formats_decodables(&d), vec![FormatVideo::Hevc420, FormatVideo::H264]);
    }

    #[test]
    fn une_machine_sans_l_extension_hevc_decode_le_h264() {
        let d = Decodables { hevc_444: false, hevc_420: false, h264: true };
        assert_eq!(formats_decodables(&d), vec![FormatVideo::H264]);
    }

    #[test]
    fn une_machine_qui_ne_decode_rien_n_offre_rien() {
        let d = Decodables { hevc_444: false, hevc_420: false, h264: false };
        assert!(formats_decodables(&d).is_empty());
    }

    #[test]
    fn une_machine_nvidia_decode_les_trois_dans_l_ordre() {
        let d = Decodables { hevc_444: true, hevc_420: true, h264: true };
        assert_eq!(
            formats_decodables(&d),
            vec![FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264]
        );
    }

    #[test]
    fn sans_imposition_on_offre_tout_ce_qu_on_decode() {
        let decodables = vec![FormatVideo::Hevc420, FormatVideo::H264];
        assert_eq!(formats_a_offrir(decodables.clone(), None), decodables);
    }

    #[test]
    fn un_format_impose_restreint_l_offre() {
        let decodables = vec![FormatVideo::Hevc420, FormatVideo::H264];
        assert_eq!(formats_a_offrir(decodables, Some(&[FormatVideo::H264])), vec![FormatVideo::H264]);
    }

    #[test]
    fn un_format_impose_n_elargit_jamais_l_offre() {
        // Le portable ne décode pas le 4:4:4 : l'imposer ne le lui fait pas offrir.
        let decodables = vec![FormatVideo::Hevc420, FormatVideo::H264];
        assert!(formats_a_offrir(decodables, Some(&[FormatVideo::Hevc444])).is_empty());
    }

    #[test]
    fn l_ordre_de_preference_survit_a_l_imposition() {
        let decodables = vec![FormatVideo::Hevc444, FormatVideo::Hevc420, FormatVideo::H264];
        // Imposés dans le désordre : l'ordre de l'offre reste celui des décodables.
        assert_eq!(
            formats_a_offrir(decodables, Some(&[FormatVideo::H264, FormatVideo::Hevc444])),
            vec![FormatVideo::Hevc444, FormatVideo::H264]
        );
    }
}
