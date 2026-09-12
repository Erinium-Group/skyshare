use anyhow::anyhow;
use crypto_box::{aead::OsRng, PublicKey, SecretKey};

/// L'identité cryptographique d'un appareil.
///
/// La clé privée ne quitte jamais la machine (spec §4.2). Dans le spike elle
/// est éphémère ; au jalon 1 elle ira dans le coffre-fort du système.
pub struct Identity {
    secret: SecretKey,
}

impl Identity {
    pub fn generate() -> Self {
        Self {
            secret: SecretKey::generate(&mut OsRng),
        }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.secret.public_key().to_bytes()
    }

    /// Sérialise la clé privée en octets bruts, pour le coffre-fort du
    /// système (jalon 1).
    ///
    /// Ces octets sont le secret complet de l'identité : quiconque les
    /// obtient peut se faire passer pour cet appareil. Ils ne doivent
    /// jamais être journalisés ni transiter ailleurs que vers le
    /// gestionnaire d'identifiants du système.
    pub fn en_octets(&self) -> [u8; 32] {
        self.secret.to_bytes()
    }

    /// Reconstruit une identité à partir d'octets bruts (symétrique de
    /// [`Identity::en_octets`]).
    pub fn depuis_octets(octets: &[u8; 32]) -> Self {
        Self {
            secret: SecretKey::from_bytes(*octets),
        }
    }

    /// Scelle un message pour un destinataire.
    ///
    /// Utilise le mode « sealed box » : une paire de clés éphémère est générée
    /// à chaque appel, ce qui rend deux scellages du même message indistinguables
    /// et n'exige pas que le destinataire connaisse l'expéditeur.
    pub fn seal(&self, destinataire: &[u8; 32], message: &[u8]) -> Vec<u8> {
        let pk = PublicKey::from(*destinataire);
        // API réelle de crypto_box 0.9 : méthode `PublicKey::seal`, pas de
        // fonction libre `crypto_box::seal` (voir écart documenté au commit).
        pk.seal(&mut OsRng, message).expect("scellage")
    }

    pub fn open(&self, scelle: &[u8]) -> anyhow::Result<Vec<u8>> {
        self.secret
            .unseal(scelle)
            .map_err(|_| anyhow!("descellage impossible : mauvaise clé ou message altéré"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");
        let ouvert = bob.open(&scelle).unwrap();

        assert_eq!(ouvert, b"192.0.2.1:51234");
    }

    #[test]
    fn un_tiers_ne_peut_pas_ouvrir() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let mallory = Identity::generate();

        let scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");

        // C'est la garantie centrale : Vercel stocke ce blob et ne peut rien en faire.
        assert!(mallory.open(&scelle).is_err());
    }

    #[test]
    fn message_altere_rejete() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let mut scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");
        let dernier = scelle.len() - 1;
        scelle[dernier] ^= 0xFF;

        assert!(bob.open(&scelle).is_err());
    }

    #[test]
    fn deux_scellages_du_meme_message_different() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let a = alice.seal(&bob.public_key(), b"identique");
        let b = alice.seal(&bob.public_key(), b"identique");

        // Sans cela, un observateur repérerait les connexions répétées
        // vers le même pair rien qu'en comparant les blobs.
        assert_ne!(a, b);
    }

    #[test]
    fn surcout_borne() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let scelle = alice.seal(&bob.public_key(), b"x");
        // 32 octets de clé éphémère + 16 de tag d'authentification.
        assert_eq!(scelle.len(), 1 + 48);
    }
}
