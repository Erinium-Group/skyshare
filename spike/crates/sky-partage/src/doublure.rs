//! Doublures de test du transport : de quoi éprouver les boucles d'envoi et de
//! réception sans GPU, sans écran et sans réseau.
//!
//! Ce module est un module de crate, compilé sous `#[cfg(test)]`, et non un
//! morceau du module de tests de `hote` : le côté spectateur en a besoin aussi,
//! et ne pourrait pas l'atteindre s'il vivait dans `mod tests`.

use std::collections::VecDeque;

use sky_net::{ErreurEnvoi, LinkEvent, MessageControle};

use crate::hote::LienVideo;

/// Une unité d'accès telle que l'hôte l'a écrite sur la piste média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ecriture {
    pub unite: Vec<u8>,
    pub horodatage_ms: u64,
}

/// Un lien pair-à-pair sans réseau : il retient ce qu'on lui écrit et rend les
/// événements qu'on lui injecte.
///
/// Il imite un comportement mesuré de `PeerLink::ecrire_image` : un refus
/// `TropDImagesEnAttente` ne se lève pas tout seul, et un `poll` ne libère
/// **qu'une** place. C'est ce qui permet de vérifier que l'appelant sait faire
/// le geste attendu — poller puis réécrire la MÊME image — plusieurs fois de
/// suite sans conclure à un échec.
pub struct LienFactice {
    ecritures: Vec<Ecriture>,
    messages: Vec<MessageControle>,
    evenements: VecDeque<LinkEvent>,
    /// Nombre de places encore à libérer avant qu'une écriture soit acceptée.
    places_a_liberer: usize,
    polls: u64,
}

impl LienFactice {
    pub fn nouveau() -> LienFactice {
        LienFactice {
            ecritures: Vec::new(),
            messages: Vec::new(),
            evenements: VecDeque::new(),
            places_a_liberer: 0,
            polls: 0,
        }
    }

    /// Les unités d'accès acceptées, dans l'ordre. Une écriture refusée n'y
    /// figure pas.
    pub fn ecritures(&self) -> &[Ecriture] {
        &self.ecritures
    }

    /// Les messages de contrôle que l'hôte a envoyés, dans l'ordre.
    pub fn messages_envoyes(&self) -> &[MessageControle] {
        &self.messages
    }

    /// Fait rendre `evenement` par le prochain `poll`. Les événements sortent
    /// dans l'ordre d'injection ; quand la file est vide, `poll` rend `Idle`.
    pub fn injecter(&mut self, evenement: LinkEvent) {
        self.evenements.push_back(evenement);
    }

    /// Refuse les écritures tant que `places` n'ont pas été libérées par autant
    /// de `poll` — la file de paquetisation vue par l'appelant.
    pub fn saturer(&mut self, places: usize) {
        self.places_a_liberer = places;
    }

    pub fn polls(&self) -> u64 {
        self.polls
    }
}

impl LienVideo for LienFactice {
    fn ecrire_image(&mut self, unite: &[u8], horodatage_ms: u64) -> Result<(), ErreurEnvoi> {
        if self.places_a_liberer > 0 {
            return Err(ErreurEnvoi::TropDImagesEnAttente);
        }
        self.ecritures.push(Ecriture { unite: unite.to_vec(), horodatage_ms });
        Ok(())
    }

    fn envoyer_controle(&mut self, message: &MessageControle) -> Result<(), ErreurEnvoi> {
        self.messages.push(message.clone());
        Ok(())
    }

    fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        self.polls += 1;
        self.places_a_liberer = self.places_a_liberer.saturating_sub(1);
        Ok(self.evenements.pop_front().unwrap_or(LinkEvent::Idle))
    }
}

/// Un encodeur réduit aux deux gestes de reprise : il ne compresse rien, il
/// compte. `NvencEncoder` ne se double pas — il consomme une texture Direct3D —
/// mais `Reprise` est tout ce dont la boucle d'envoi a besoin de lui ici.
pub struct RepriseFactice {
    entetes: Vec<u8>,
    images_cle_forcees: u64,
}

impl RepriseFactice {
    /// `entetes` tient la place des VPS/SPS/PPS d'Annex-B qu'un vrai encodeur
    /// rendrait ; les tests y mettent des NAL reconnaissables.
    pub fn avec_entetes(entetes: Vec<u8>) -> RepriseFactice {
        RepriseFactice { entetes, images_cle_forcees: 0 }
    }

    pub fn images_cle_forcees(&self) -> u64 {
        self.images_cle_forcees
    }
}

impl crate::hote::Reprise for RepriseFactice {
    fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>> {
        Ok(self.entetes.clone())
    }

    fn forcer_image_cle(&mut self) {
        self.images_cle_forcees += 1;
    }
}
