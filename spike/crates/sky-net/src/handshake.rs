//! Le bloc de signaling échangé pendant la poignée de main.

use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

/// Ce qui s'échange entre les deux machines pendant la poignée de main.
///
/// Dans le spike, ce blob se copie-colle à la main dans une messagerie. Au
/// jalon 1, il transitera par la boîte aux lettres chiffrée — sans changer de
/// forme.
#[derive(Serialize, Deserialize)]
pub struct Blob {
    /// Clé publique de l'émetteur du bloc, en clair : c'est elle qui permet à
    /// l'autre bord de sceller sa réponse.
    pub public_key: [u8; 32],
    /// Description de session (SDP). Scellée dans la réponse, en clair dans
    /// l'offre — voir la note de confidentialité sur `PeerLink::host`.
    pub sealed_sdp: Vec<u8>,
}

impl Blob {
    pub fn to_text(&self) -> String {
        let json = serde_json::to_vec(self).expect("sérialisation");
        format!("SKY1:{}", STANDARD.encode(json))
    }

    pub fn from_text(s: &str) -> anyhow::Result<Self> {
        let corps = s
            .trim()
            .strip_prefix("SKY1:")
            .context("préfixe SKY1: absent — le bloc a-t-il été copié en entier ?")?;
        let json = STANDARD
            .decode(corps.trim())
            .context("bloc illisible — la copie est probablement incomplète")?;
        serde_json::from_slice(&json).context("bloc illisible — structure inattendue")
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
        assert!(t.starts_with("SKY1:"));
        let r = Blob::from_text(&t).unwrap();
        assert_eq!(r.public_key, [7u8; 32]);
        assert_eq!(r.sealed_sdp, vec![1, 2, 3]);
    }

    #[test]
    fn tolere_espaces_et_retours_ligne() {
        // Un copier-coller depuis une messagerie ramène souvent des espaces parasites.
        let b = Blob {
            public_key: [1u8; 32],
            sealed_sdp: vec![9],
        };
        let t = format!("  \n{}\n  ", b.to_text());
        assert!(Blob::from_text(&t).is_ok());
    }

    #[test]
    fn rejette_un_bloc_tronque() {
        assert!(Blob::from_text("SKY1:abc").is_err());
    }
}
