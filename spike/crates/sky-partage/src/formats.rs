//! Passages entre ce que les cartes savent faire et ce que la piste transporte.
//!
//! Un seul endroit, pour l'hôte comme pour le spectateur : deux traductions
//! écrites à deux endroits finiraient par diverger.

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
}
