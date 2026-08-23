//! Le bloc de signaling échangé pendant la poignée de main.

use anyhow::{anyhow, Context};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use flate2::write::DeflateEncoder;
use flate2::read::DeflateDecoder;
use flate2::Compression;
use std::io::{Read, Write};

/// Préfixe et version du format. Un bloc produit par une version antérieure
/// n'est pas silencieusement mal interprété : il est refusé avec un message
/// qui dit quoi faire.
const PREFIXE: &str = "SKY2:";
/// Version du contenu binaire, indépendante du préfixe textuel.
const VERSION: u8 = 1;

/// Ce qui s'échange entre les deux machines pendant la poignée de main.
///
/// Le bloc est copié-collé à la main dans une messagerie, donc **sa longueur
/// est une contrainte de conception** : Discord limite un message à 2 000
/// caractères sans abonnement. Le format précédent encodait le SDP en tableau
/// de nombres décimaux JSON, puis le tout en base64 — environ 5,3 caractères
/// par octet utile, soit près de 4 000 caractères. Celui-ci empaquette en
/// binaire, comprime, et n'encode qu'une fois.
#[derive(Debug)]
pub struct Blob {
    /// Clé publique de l'émetteur du bloc, en clair : c'est elle qui permet à
    /// l'autre bord de sceller sa réponse.
    pub public_key: [u8; 32],
    /// Description de session (SDP), **déjà comprimée**. Scellée dans la
    /// réponse, en clair dans l'offre.
    ///
    /// La compression précède toujours le scellement : un contenu chiffré est
    /// indistinguable du hasard et ne se comprime pas.
    pub sealed_sdp: Vec<u8>,
}

/// Comprime un SDP. Ce format est très répétitif : la compression le réduit
/// des deux tiers environ.
pub fn comprimer(sdp: &str) -> Vec<u8> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(sdp.as_bytes()).expect("écriture en mémoire");
    enc.finish().expect("compression en mémoire")
}

/// Opération inverse de `comprimer`.
pub fn decomprimer(donnees: &[u8]) -> anyhow::Result<String> {
    let mut sortie = String::new();
    DeflateDecoder::new(donnees)
        .read_to_string(&mut sortie)
        .map_err(|_| anyhow!("bloc illisible — contenu corrompu ou tronqué"))?;
    Ok(sortie)
}

impl Blob {
    pub fn to_text(&self) -> String {
        let mut brut = Vec::with_capacity(33 + self.sealed_sdp.len());
        brut.push(VERSION);
        brut.extend_from_slice(&self.public_key);
        brut.extend_from_slice(&self.sealed_sdp);
        // base64url sans remplissage : pas de « + », « / » ni « = », donc rien
        // qu'une messagerie puisse échapper ou couper.
        format!("{PREFIXE}{}", URL_SAFE_NO_PAD.encode(brut))
    }

    pub fn from_text(s: &str) -> anyhow::Result<Self> {
        let nettoye: String = s.trim().split_whitespace().collect();
        if nettoye.starts_with("SKY1:") {
            return Err(anyhow!(
                "bloc à l'ancien format : les deux machines doivent utiliser la même version du programme"
            ));
        }
        let corps = nettoye
            .strip_prefix(PREFIXE)
            .context("préfixe SKY2: absent — le bloc a-t-il été copié en entier ?")?;
        let brut = URL_SAFE_NO_PAD
            .decode(corps)
            .context("bloc illisible — la copie est probablement incomplète")?;

        if brut.len() < 33 {
            return Err(anyhow!("bloc trop court — copie incomplète"));
        }
        if brut[0] != VERSION {
            return Err(anyhow!(
                "version de bloc inconnue — les deux machines doivent utiliser la même version du programme"
            ));
        }
        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(&brut[1..33]);
        Ok(Blob {
            public_key,
            sealed_sdp: brut[33..].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_texte() {
        let b = Blob {
            public_key: [7u8; 32],
            sealed_sdp: vec![1, 2, 3],
        };
        let t = b.to_text();
        assert!(t.starts_with("SKY2:"));
        let r = Blob::from_text(&t).unwrap();
        assert_eq!(r.public_key, [7u8; 32]);
        assert_eq!(r.sealed_sdp, vec![1, 2, 3]);
    }

    #[test]
    fn tolere_espaces_et_retours_ligne() {
        // Un copier-coller depuis une messagerie ramène souvent des espaces, et
        // certaines découpent un bloc long sur plusieurs lignes.
        let b = Blob {
            public_key: [1u8; 32],
            sealed_sdp: vec![9; 40],
        };
        let t = b.to_text();
        let coupe = format!("  {}
{}  
", &t[..30], &t[30..]);
        let r = Blob::from_text(&coupe).unwrap();
        assert_eq!(r.sealed_sdp, vec![9; 40]);
    }

    #[test]
    fn rejette_un_bloc_tronque() {
        assert!(Blob::from_text("SKY2:abc").is_err());
    }

    #[test]
    fn rejette_lancien_format_avec_un_message_utile() {
        let e = Blob::from_text("SKY1:eyJwdWJsaWNfa2V5Ijpb").unwrap_err();
        assert!(e.to_string().contains("même version"));
    }

    #[test]
    fn compression_aller_retour() {
        let sdp = ["v=0", "o=- 123 2 IN IP4 0.0.0.0", "a=candidate:1 1 udp 2130706431"]
            .join("
");
        assert_eq!(decomprimer(&comprimer(&sdp)).unwrap(), sdp);
    }

    #[test]
    fn un_sdp_reel_tient_largement_sous_la_limite_discord() {
        // Contrainte de conception : 2 000 caracteres par message sans
        // abonnement. Un SDP reel avec deux candidats fait environ 800 octets.
        let lignes = [
            "v=0",
            "o=- 3724927680283698863 2 IN IP4 0.0.0.0",
            "s=-",
            "t=0 0",
            "a=group:BUNDLE VDM",
            "a=extmap-allow-mixed",
            "a=msid-semantic: WMS ALAcvFnB8tgX5mauXlPXsxg0BXUka5",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
            "c=IN IP4 0.0.0.0",
            "a=candidate:fffeff7e9cee09875a5ad4a9f 1 udp 2130706175 192.168.1.4 53339 typ host ufrag m0boJUJ9as6nCBuP",
            "a=candidate:fffe7f6442a266c139302d3dca 1 udp 1686109951 82.67.25.85 53339 typ srflx raddr 0.0.0.0 rport 0 ufrag m0boJUJ9as6nCBuP",
            "a=ice-ufrag:m0boJUJ9as6nCBuP",
            "a=ice-pwd:BULyBDGNqY9rdC4B2AUYHB",
            "a=ice-options:trickle",
            "a=fingerprint:sha-256 55:8F:4F:A0:10:E0:6A:42:5C:0B:35:DF:32:3B:C7:F8:B2:1B:C2:B4:70:BC:C4:1E:3E:67:60:B7:2B:39:AD:DB",
            "a=setup:actpass",
            "a=mid:VDM",
            "a=sctp-port:5000",
            "a=max-message-size:262144",
        ];
        let sdp = lignes.join("
");

        let blob = Blob {
            public_key: [0xABu8; 32],
            sealed_sdp: comprimer(&sdp),
        };
        let texte = blob.to_text();
        println!("bloc reel : {} caracteres", texte.len());
        assert!(
            texte.len() < 2000,
            "bloc de {} caracteres, au-dessus de la limite Discord",
            texte.len()
        );
        // Marge exigee : la reponse scellee ajoute 48 octets, et certains SDP
        // portent un candidat de plus.
        assert!(
            texte.len() < 1200,
            "bloc de {} caracteres, marge insuffisante",
            texte.len()
        );
    }
}
