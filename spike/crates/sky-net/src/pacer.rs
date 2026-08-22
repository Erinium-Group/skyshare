use std::time::Duration;

/// Contrôle de congestion à plancher garanti.
///
/// La différence essentielle avec l'algorithme de WebRTC : celui-ci accepte de
/// descendre indéfiniment pour préserver la continuité, ce qui produit l'image
/// baveuse de Discord. Ici le débit ne passe jamais sous un plancher choisi par
/// l'utilisateur : en cas de congestion durable, on préfère perdre des images
/// plutôt que de la netteté.
pub struct Pacer {
    plancher_bps: u32,
    plafond_bps: u32,
    cible_bps: u32,
}

impl Pacer {
    /// Part maximale retirée en un seul retour d'information.
    const CHUTE_MAX: f32 = 0.15;
    /// Part ajoutée par tick quand le réseau est sain.
    const MONTEE: f32 = 0.08;
    /// En dessous, la perte est considérée comme du bruit normal.
    const SEUIL_PERTE: f32 = 2.0;
    /// Au-delà, le tampon réseau se remplit : on lève le pied.
    const SEUIL_RTT_MS: u32 = 150;

    pub fn new(plancher_bps: u32, plafond_bps: u32) -> Self {
        Self {
            plancher_bps,
            plafond_bps,
            cible_bps: plancher_bps,
        }
    }

    pub fn target_bps(&self) -> u32 {
        self.cible_bps
    }

    pub fn on_feedback(&mut self, perte_pct: f32, rtt_ms: u32, _ecoule: Duration) {
        let congestionne = perte_pct > Self::SEUIL_PERTE || rtt_ms > Self::SEUIL_RTT_MS;

        let brut = if congestionne {
            // Chute proportionnelle à la sévérité, bornée à CHUTE_MAX.
            let severite = (perte_pct / 100.0).clamp(0.0, 1.0);
            let facteur = 1.0 - (Self::CHUTE_MAX * severite.max(0.3));
            self.cible_bps as f32 * facteur
        } else {
            self.cible_bps as f32 * (1.0 + Self::MONTEE)
        };

        self.cible_bps = (brut as u32).clamp(self.plancher_bps, self.plafond_bps);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const TICK: Duration = Duration::from_millis(100);

    fn pacer() -> Pacer {
        // Plancher 8 Mbps, plafond 30 Mbps.
        Pacer::new(8_000_000, 30_000_000)
    }

    #[test]
    fn demarre_au_plancher() {
        assert_eq!(pacer().target_bps(), 8_000_000);
    }

    #[test]
    fn monte_vers_le_plafond_sans_perte() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);
    }

    #[test]
    fn ne_descend_jamais_sous_le_plancher() {
        let mut p = pacer();
        // Perte catastrophique et durable : 50 % pendant 10 secondes.
        for _ in 0..100 {
            p.on_feedback(50.0, 500, TICK);
        }
        // C'est LA différence avec WebRTC standard, qui s'effondrerait ici.
        assert_eq!(p.target_bps(), 8_000_000);
    }

    #[test]
    fn remonte_en_moins_de_deux_secondes() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);

        // Un à-coup bref.
        p.on_feedback(20.0, 300, TICK);
        let apres_chute = p.target_bps();
        assert!(apres_chute < 30_000_000);

        // Le réseau se dégage : retour au plafond en moins de 2 s (20 ticks).
        for _ in 0..20 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);
    }

    #[test]
    fn descente_progressive_pas_brutale() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        let avant = p.target_bps();
        p.on_feedback(10.0, 200, TICK);
        let apres = p.target_bps();

        // Une chute d'un seul coup fait pulser l'image de façon très visible.
        // On n'enlève jamais plus de 15 % par tick.
        assert!(apres as f32 >= avant as f32 * 0.85);
        assert!(apres < avant);
    }
}
