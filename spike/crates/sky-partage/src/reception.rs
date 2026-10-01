//! La comptabilité du spectateur, extraite de la boucle de `cmd_view` (jalon
//! C2) pour être testable sans réseau : ce qui compte une image, le débit, la
//! gigue RFC 3550 et le transit.
//!
//! Depuis la tâche 9, elle ne découpe plus rien. Le découpage maison — un
//! en-tête de 9 octets (horodatage sur 8, drapeau de premier morceau sur 1)
//! préfixé à chaque message du canal de données — a disparu des deux côtés :
//! l'hôte écrit des unités d'accès entières sur la piste média (tâche 8) et
//! `LinkEvent::Image` porte déjà l'horodatage et le drapeau d'image clé. Un
//! événement reçu **est** une image : il n'y a plus de premier morceau à
//! reconnaître, donc plus d'en-tête à retirer.

use crate::evenement::Quantiles;

#[derive(Default)]
pub struct Reception {
    octets: u64,
    images: u64,
    dernier_emission: Option<u64>,
    derniere_arrivee: Option<u64>,
    gigue_us: f64,
    transits_us: Vec<i64>,
}

impl Reception {
    /// Compte une unité d'accès arrivée sur la piste média : ses octets, son
    /// transit, et la gigue qu'elle révèle.
    ///
    /// `emission_us` est l'horodatage de l'émetteur (celui de
    /// `LinkEvent::Image`, ramené en microsecondes) et `arrivee_us` notre
    /// horloge locale.
    pub fn compter_image(&mut self, octets: usize, emission_us: u64, arrivee_us: u64) {
        self.transits_us.push(arrivee_us as i64 - emission_us as i64);
        if let (Some(prec_emis), Some(prec_arr)) = (self.dernier_emission, self.derniere_arrivee) {
            let delta_emission = emission_us as i64 - prec_emis as i64;
            let delta_arrivee = arrivee_us as i64 - prec_arr as i64;
            let ecart = (delta_arrivee - delta_emission).unsigned_abs() as f64;
            self.gigue_us += (ecart - self.gigue_us) / 16.0;
        }
        self.dernier_emission = Some(emission_us);
        self.derniere_arrivee = Some(arrivee_us);
        self.images += 1;
        self.octets += octets as u64;
    }

    pub fn octets(&self) -> u64 {
        self.octets
    }

    pub fn images(&self) -> u64 {
        self.images
    }

    pub fn gigue_ms(&self) -> f64 {
        self.gigue_us / 1000.0
    }

    /// Médiane et p99 du transit (arrivée − émission), en ms. `None` sans image.
    ///
    /// **Seul l'écart entre ces valeurs a un sens, pas leur valeur absolue** :
    /// les deux horloges n'ont pas la même origine — celle de l'émetteur compte
    /// depuis SON départ (voir `LinkEvent::Image::horodatage_ms`), celle du
    /// spectateur depuis le début de la réception. C'était déjà vrai du
    /// découpage maison, qui portait un horodatage d'émetteur tout aussi local ;
    /// rien ne s'est dégradé ici, mais rien ne s'est corrigé non plus.
    pub fn transit_ms(&mut self) -> Option<Quantiles> {
        if self.transits_us.is_empty() {
            return None;
        }
        self.transits_us.sort_unstable();
        let centile = |p: usize| {
            let t = &self.transits_us;
            t[(t.len() * p / 100).min(t.len() - 1)] as f64 / 1000.0
        };
        Some(Quantiles { p50: centile(50), p99: centile(99), echantillons: self.transits_us.len() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_image_recue_compte_ses_octets_et_une_image() {
        // Remplace `seul_le_premier_morceau_compte_une_image` : il n'y a plus de
        // morceaux, mais la comptabilité par image reste ce qui alimente le
        // débit et les images par seconde. Neutralisation : ne pas additionner
        // `octets` — `octets()` vaut 0.
        let mut r = Reception::default();
        r.compter_image(3, 0, 1_000);
        r.compter_image(2, 16_000, 17_000);
        assert_eq!(r.images(), 2);
        assert_eq!(r.octets(), 5);
    }

    #[test]
    fn la_gigue_suit_la_rfc_3550() {
        // Émissions à 0 et 10 000 µs, arrivées à 1 000 et 13 000 µs : écart
        // 2 000 µs, lissé au 1/16 → 125 µs. Neutralisation : diviser par 8.
        let mut r = Reception::default();
        r.compter_image(1, 0, 1_000);
        r.compter_image(1, 10_000, 13_000);
        assert!((r.gigue_ms() - 0.125).abs() < 1e-9, "gigue {}", r.gigue_ms());
    }

    #[test]
    fn le_transit_s_echantillonne_une_fois_par_image() {
        // Neutralisation : ne rien pousser dans `transits_us` — `transit_ms()`
        // reste `None` et le test rougit sur le `unwrap`.
        let mut r = Reception::default();
        assert_eq!(r.transit_ms(), None);
        r.compter_image(1, 0, 2_000);
        r.compter_image(1, 1_000, 5_000);
        let q = r.transit_ms().unwrap();
        assert_eq!(q.echantillons, 2, "un échantillon par image reçue");
        assert!((q.p50 - 4.0).abs() < 1e-9, "p50 {}", q.p50);
    }
}
