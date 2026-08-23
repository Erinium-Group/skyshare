use std::fmt;
use std::time::Duration;

/// Bornes de débit incohérentes passées à [`Pacer::new`].
///
/// Existe parce que `clamp(min, max)` **panique** quand `min > max` : sans
/// cette vérification à la construction, un plancher supérieur au plafond ne
/// se manifestait qu'au premier retour d'information, une fois la connexion
/// établie — donc après tout l'aller-retour humain de copier-coller des blocs.
/// L'invariant appartient au constructeur, pas à l'appelant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BornesInvalides {
    pub plancher_bps: u32,
    pub plafond_bps: u32,
}

impl fmt::Display for BornesInvalides {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "plancher de débit ({} bps) supérieur au plafond ({} bps) : \
             le plancher ne peut pas dépasser le débit cible de l'encodeur",
            self.plancher_bps, self.plafond_bps
        )
    }
}

impl std::error::Error for BornesInvalides {}

/// Contrôle de congestion à plancher garanti.
///
/// La différence essentielle avec l'algorithme de WebRTC : celui-ci accepte de
/// descendre indéfiniment pour préserver la continuité, ce qui produit l'image
/// baveuse de Discord. Ici le débit ne passe jamais sous un plancher choisi par
/// l'utilisateur : en cas de congestion durable, on préfère perdre des images
/// plutôt que de la netteté.
#[derive(Debug)]
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

    /// Construit un régulateur dont le débit restera dans `[plancher, plafond]`.
    ///
    /// Échoue si le plancher dépasse le plafond : c'est le seul état où la
    /// formule ne peut rien produire de sensé, et le laisser passer ferait
    /// paniquer `clamp` au premier retour d'information. Deux bornes égales
    /// sont valides — le débit est alors constant.
    pub fn new(plancher_bps: u32, plafond_bps: u32) -> Result<Self, BornesInvalides> {
        if plancher_bps > plafond_bps {
            return Err(BornesInvalides {
                plancher_bps,
                plafond_bps,
            });
        }
        Ok(Self {
            plancher_bps,
            plafond_bps,
            cible_bps: plancher_bps,
        })
    }

    pub fn target_bps(&self) -> u32 {
        self.cible_bps
    }

    /// Intègre un retour d'information et ajuste la cible.
    ///
    /// **Réserve de promotion :** `_ecoule` est reçu et jeté. Les deux
    /// propriétés annoncées — « au plus 15 % retirés par tick » et « remontée
    /// au plafond en 1 tick » — sont donc des propriétés *par appel*, pas par
    /// unité de temps. Elles ne se traduisent en garanties temporelles que
    /// parce que l'appelant actuel sert le régulateur toutes les 100 ms. Un
    /// appelant qui le servirait à 10 ms hériterait d'une descente dix fois
    /// plus brutale sans qu'une ligne change ici. Avant promotion : soit
    /// normaliser la formule par `_ecoule`, soit inscrire la cadence dans le
    /// type.
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
        Pacer::new(8_000_000, 30_000_000).expect("bornes valides")
    }

    #[test]
    fn refuse_un_plancher_au_dessus_du_plafond() {
        // Cas réel : `sky-probe host --floor-mbps 50 --bitrate-mbps 30`.
        // Sans cette validation, `clamp(50e6, 30e6)` paniquait au premier
        // retour d'information — soit ~100 ms après le début de l'envoi,
        // donc après tout l'aller-retour humain de copier-coller.
        let e = Pacer::new(50_000_000, 30_000_000).unwrap_err();
        assert_eq!(e.plancher_bps, 50_000_000);
        assert_eq!(e.plafond_bps, 30_000_000);
    }

    #[test]
    fn accepte_deux_bornes_egales() {
        // Débit constant : dégénéré, mais parfaitement défini.
        let mut p = Pacer::new(30_000_000, 30_000_000).expect("bornes valides");
        p.on_feedback(50.0, 500, TICK);
        assert_eq!(p.target_bps(), 30_000_000);
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
