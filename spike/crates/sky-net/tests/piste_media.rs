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
use sky_net::PeerLink;

/// Profil Main 4:4:4 de HEVC. `profile_id = 1` est Main, qui serait un mensonge.
const PROFIL_MAIN_444: u8 = 4;

/// Une offre de spectateur, et de quoi ouvrir la réponse qui lui sera scellée.
///
/// L'identité est dédoublée par ses octets : `offrant` consomme la sienne, et
/// `PeerLink` n'expose aucun descellement — c'est au test de garder la clé.
fn offre_de_spectateur() -> (Identity, String) {
    let secret = Identity::generate().en_octets();
    let (_lien, offre) = PeerLink::offrant(Identity::depuis_octets(&secret)).expect("offre");
    (Identity::depuis_octets(&secret), offre)
}

/// Le SDP que l'hôte retient réellement, descellé par le spectateur.
fn hote_repond(spectateur: &Identity, offre: &str) -> String {
    let (_lien, reponse) = PeerLink::repondant(Identity::generate(), offre).expect("réponse");
    let blob = Blob::from_text(&reponse).expect("bloc de réponse illisible");
    let comprime = spectateur
        .open(&blob.sealed_sdp)
        .expect("réponse non descellable");
    handshake::decomprimer(&comprime).expect("réponse non décompressable")
}

/// La ligne `a=fmtp:` du type de charge retenu pour H265, s'il y en a un.
///
/// Le type de charge n'est pas supposé : il est lu sur la ligne `a=rtpmap:`, car
/// la négociation peut le réattribuer.
fn ligne_fmtp_h265(sdp: &str) -> Option<String> {
    let pt = sdp
        .lines()
        .filter_map(|l| l.strip_prefix("a=rtpmap:"))
        .find(|reste| {
            reste
                .split_once(' ')
                .is_some_and(|(_, codec)| codec.starts_with("H265/"))
        })
        .and_then(|reste| reste.split_once(' '))
        .map(|(pt, _)| pt.to_string())?;

    let prefixe = format!("a=fmtp:{pt} ");
    sdp.lines()
        .find_map(|l| l.strip_prefix(&prefixe))
        .map(|params| params.trim_end().to_string())
}

#[test]
fn la_reponse_sdp_retient_le_profil_main_444() {
    let (spectateur, offre) = offre_de_spectateur();
    let reponse = hote_repond(&spectateur, &offre);
    let fmtp = ligne_fmtp_h265(&reponse).expect("la réponse doit décrire H265");
    assert!(
        fmtp.contains(&format!("profile-id={PROFIL_MAIN_444}")),
        "le profil annoncé n'est pas Main 4:4:4 : {fmtp}"
    );
}
