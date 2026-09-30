//! Les messages de contrôle : ce que les deux bouts d'un lien se disent en
//! dehors du flux vidéo.
//!
//! Ils voyagent sur le canal de données WebRTC, qui n'est mauvais qu'au
//! transport vidéo (mesuré au jalon 0 : 16 % d'échecs d'envoi, tampon SCTP
//! saturé). Pour des messages rares et minuscules, c'est son usage nominal.
//!
//! Le type vit ici et pas dans `sky-partage` : `LinkEvent::Controle` le nomme,
//! et `sky-partage` dépend de `sky-net`.

use serde::{Deserialize, Serialize};

/// Un message de contrôle. Sérialisé en JSON : lisible, et extensible sans
/// casser les anciennes versions, qui ignorent ce qu'elles ne connaissent pas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MessageControle {
    /// Le spectateur a perdu le fil de l'image et demande une image clé.
    DemandeImageCle,
    /// L'hôte cesse de partager.
    PartageArrete,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_message_de_controle_survit_a_l_aller_retour() {
        for message in [
            MessageControle::DemandeImageCle,
            MessageControle::PartageArrete,
        ] {
            let octets = serde_json::to_vec(&message).expect("sérialisation");
            let relu: MessageControle = serde_json::from_slice(&octets).expect("relecture");
            assert_eq!(message, relu);
        }
    }

    /// Un message inconnu ne doit pas faire tomber le lien : une version plus
    /// récente de l'application enverra des messages que celle-ci ne connaît pas.
    #[test]
    fn un_message_inconnu_est_refuse_sans_paniquer() {
        let refus: Result<MessageControle, _> = serde_json::from_slice(br#""RoucouleDuPigeon""#);
        assert!(refus.is_err());
    }
}
