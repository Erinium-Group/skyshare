//! LA SERRURE QU'IL NE FAUT PAS LAISSER DÉBRANCHÉE.
//!
//! La sonde du 27/09/2026 a transporté correctement notre HEVC 4:4:4 AVEC LE
//! MAUVAIS PROFIL ANNONCÉ — parce que la paquetisation RFC 7798 ne lit pas le
//! contenu du NAL. Autrement dit : aucun test de transport ne verra jamais cette
//! erreur. Ce test porte donc sur ce que la RÉPONSE SDP retient, pas sur ce qui
//! passe.
//!
//! Le test de transport, lui, vit dans le module de tests de `link.rs` : il a
//! besoin des aides `paire_connectee` et `pomper_jusqu_a`, qui y sont privées et
//! qu'il aurait fallu recopier ici. Les deux se lancent ensemble par
//! `cargo test -p sky-net`, et le contraste entre eux se lit tout aussi bien.
//!
//! Confidentialité : le SDP n'est jamais affiché. On en extrait la seule ligne
//! `a=fmtp:`, qui décrit un codec et ne porte aucune adresse.

use sky_crypto::Identity;
use sky_net::handshake::{self, Blob};
use sky_net::{FormatVideo, PeerLink};

/// Une offre de spectateur, et de quoi ouvrir la réponse qui lui sera scellée.
///
/// L'identité est dédoublée par ses octets : `offrant` consomme la sienne, et
/// `PeerLink` n'expose aucun descellement — c'est au test de garder la clé.
fn offre_de_spectateur() -> (Identity, String) {
    let secret = Identity::generate().en_octets();
    let (_lien, offre) =
        PeerLink::offrant(Identity::depuis_octets(&secret), &FormatVideo::PREFERENCE)
            .expect("offre");
    (Identity::depuis_octets(&secret), offre)
}

/// Le SDP que l'hôte retient réellement, descellé par le spectateur.
fn hote_repond(spectateur: &Identity, offre: &str) -> String {
    let (_lien, reponse) =
        PeerLink::repondant(Identity::generate(), offre, &FormatVideo::PREFERENCE)
            .expect("réponse");
    let blob = Blob::from_text(&reponse).expect("bloc de réponse illisible");
    let comprime = spectateur
        .open(&blob.sealed_sdp)
        .expect("réponse non descellable");
    handshake::decomprimer(&comprime).expect("réponse non décompressable")
}

/// La ligne `a=fmtp:` d'un type de charge donné, s'il y en a une.
///
/// Le type de charge est celui de `FormatVideo::type_de_charge` : les deux côtés
/// sont SkyShare, et la table est la seule source de ces numéros.
fn ligne_fmtp(sdp: &str, type_de_charge: u8) -> Option<String> {
    let prefixe = format!("a=fmtp:{type_de_charge} ");
    sdp.lines()
        .find_map(|l| l.strip_prefix(&prefixe))
        .map(|params| params.trim_end().to_string())
}

#[test]
fn la_reponse_sdp_retient_les_trois_profils_annonces() {
    let (spectateur, offre) = offre_de_spectateur();
    let reponse = hote_repond(&spectateur, &offre);

    // `profile-id=4` est le 4:4:4 ; `profile-id=1` (Main) serait un mensonge.
    let hevc_444 = ligne_fmtp(&reponse, FormatVideo::Hevc444.type_de_charge())
        .expect("la réponse doit décrire HEVC 4:4:4");
    assert!(
        hevc_444.contains("profile-id=4"),
        "le profil annoncé n'est pas HEVC 4:4:4 : {hevc_444}"
    );

    let hevc_420 = ligne_fmtp(&reponse, FormatVideo::Hevc420.type_de_charge())
        .expect("la réponse doit décrire HEVC 4:2:0");
    assert!(
        hevc_420.contains("profile-id=1"),
        "le profil annoncé n'est pas HEVC Main : {hevc_420}"
    );

    let h264 = ligne_fmtp(&reponse, FormatVideo::H264.type_de_charge())
        .expect("la réponse doit décrire H.264");
    assert!(
        h264.contains("packetization-mode=1"),
        "H.264 doit annoncer la paquetisation en mode 1 : {h264}"
    );
    assert!(
        h264.contains("profile-level-id=640034"),
        "H.264 doit annoncer le profil High niveau 5.2 : {h264}"
    );
}
