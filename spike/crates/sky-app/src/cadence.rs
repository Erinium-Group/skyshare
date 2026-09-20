//! Les cadences de la boucle de synchronisation (spec §3). Points de départ
//! de la spec, pas des mesures.

use std::time::Duration;

use crate::vue::PartageVue;

pub const CADENCE_VISIBLE: Duration = Duration::from_secs(30);
pub const CADENCE_REDUITE: Duration = Duration::from_secs(5 * 60);
/// La même que la négociation (C2) : une seule source.
pub const CADENCE_PARTAGE: Duration = sky_partage::rendez_vous::CADENCE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Inactive,
    /// Hôte disponible ou spectateur qui attend la réponse : c'est le partage
    /// qui synchronise, jamais la boucle.
    Attente,
    /// Flux établi.
    EnCours,
}

impl Phase {
    pub fn de(partage: &PartageVue) -> Phase {
        match partage {
            PartageVue::Disponible { .. } | PartageVue::Demande { .. } => Phase::Attente,
            PartageVue::Diffuse { .. } | PartageVue::Regarde { .. } => Phase::EnCours,
            PartageVue::Inactif | PartageVue::Termine { .. } => Phase::Inactive,
        }
    }
}

pub fn cadence(visible: bool, phase: Phase) -> Duration {
    match phase {
        Phase::Attente | Phase::EnCours => CADENCE_PARTAGE,
        Phase::Inactive if visible => CADENCE_VISIBLE,
        Phase::Inactive => CADENCE_REDUITE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vue::FinVue;

    #[test]
    fn la_cadence_suit_la_fenetre_et_le_partage() {
        // Spec §3. Neutralisation : ignorer `visible` — 5 min rendues 30 s.
        assert_eq!(cadence(true, Phase::Inactive), Duration::from_secs(30));
        assert_eq!(cadence(false, Phase::Inactive), Duration::from_secs(300));
        assert_eq!(cadence(false, Phase::Attente), Duration::from_secs(2));
        assert_eq!(cadence(true, Phase::EnCours), Duration::from_secs(2));
    }

    #[test]
    fn la_phase_se_lit_dans_le_partage() {
        assert_eq!(Phase::de(&PartageVue::Inactif), Phase::Inactive);
        assert_eq!(
            Phase::de(&PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 0 }),
            Phase::Attente
        );
        assert_eq!(
            Phase::de(&PartageVue::Demande { ami: "bob".into(), debut_ms: 0 }),
            Phase::Attente
        );
        assert_eq!(
            Phase::de(&PartageVue::Diffuse {
                spectateur: None,
                depuis_ms: 0,
                debit_mbps: 0.0,
                rtt_ms: 0.0,
                ecran: 0
            }),
            Phase::EnCours
        );
        assert_eq!(Phase::de(&PartageVue::Termine { fin: FinVue::Arrete }), Phase::Inactive);
    }
}
