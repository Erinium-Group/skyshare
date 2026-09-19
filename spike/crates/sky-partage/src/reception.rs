//! La comptabilité du spectateur, extraite de la boucle de `cmd_view` (jalon
//! C2) pour être testable sans réseau : ce qui compte une image, le débit,
//! la gigue RFC 3550 et le transit. Rend la charge utile ; c'est l'appelant
//! qui décide de l'écrire ou de la jeter.

use crate::evenement::Quantiles;
use crate::hote::EN_TETE_MORCEAU;

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
    /// Absorbe un message du canal. `None` pour un message trop court pour
    /// porter un en-tête complet (ignoré, comme au C2) ; sinon la charge
    /// utile, sans son en-tête.
    ///
    /// Seul le PREMIER morceau d'une image (drapeau non nul) porte les
    /// statistiques par image : compteur, transit, gigue — calculés image à
    /// image, pas morceau à morceau (ce qui mesurerait notre découpage).
    pub fn absorber<'d>(&mut self, d: &'d [u8], arrivee_us: u64) -> Option<&'d [u8]> {
        if d.len() <= EN_TETE_MORCEAU {
            return None;
        }
        let emission = u64::from_le_bytes(d[..8].try_into().expect("8 octets d'horodatage"));
        let premier_morceau = d[8] != 0;
        let charge = &d[EN_TETE_MORCEAU..];
        if premier_morceau {
            self.transits_us.push(arrivee_us as i64 - emission as i64);
            if let (Some(prec_emis), Some(prec_arr)) = (self.dernier_emission, self.derniere_arrivee) {
                let delta_emission = emission as i64 - prec_emis as i64;
                let delta_arrivee = arrivee_us as i64 - prec_arr as i64;
                let ecart = (delta_arrivee - delta_emission).unsigned_abs() as f64;
                self.gigue_us += (ecart - self.gigue_us) / 16.0;
            }
            self.dernier_emission = Some(emission);
            self.derniere_arrivee = Some(arrivee_us);
            self.images += 1;
        }
        self.octets += charge.len() as u64;
        Some(charge)
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

    /// Horodatage d'émission du dernier paquet vidéo reçu — renvoyé tel quel
    /// à l'hôte, qui en tire un aller-retour réel.
    pub fn dernier_horodatage_emission(&self) -> Option<u64> {
        self.dernier_emission
    }

    /// Médiane et p99 du transit (arrivée − émission), en ms. `None` sans image.
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

    /// Un morceau tel que l'hôte l'envoie : 8 octets d'horodatage
    /// d'émission, 1 octet drapeau, la charge.
    fn morceau(emission_us: u64, premier: bool, charge: &[u8]) -> Vec<u8> {
        let mut m = emission_us.to_le_bytes().to_vec();
        m.push(u8::from(premier));
        m.extend_from_slice(charge);
        m
    }

    #[test]
    fn seul_le_premier_morceau_compte_une_image() {
        // Neutralisation : compter une image par morceau — `images` vaut 2.
        let mut r = Reception::default();
        r.absorber(&morceau(0, true, b"abc"), 1_000);
        r.absorber(&morceau(0, false, b"de"), 1_100);
        assert_eq!(r.images(), 1);
        assert_eq!(r.octets(), 5);
    }

    #[test]
    fn la_charge_rendue_est_sans_en_tete_et_un_message_trop_court_est_ignore() {
        // Neutralisation : `<` au lieu de `<=` sur la longueur — le message
        // de 9 octets rend `Some(&[])` au lieu de `None`.
        let mut r = Reception::default();
        let m = morceau(0, true, b"xyz");
        assert_eq!(r.absorber(&m, 5), Some(&b"xyz"[..]));
        assert_eq!(r.absorber(&[0u8; EN_TETE_MORCEAU], 6), None);
        assert_eq!(r.octets(), 3);
    }

    #[test]
    fn la_gigue_suit_la_rfc_3550() {
        // Émissions à 0 et 10 000 µs, arrivées à 1 000 et 13 000 µs : écart
        // 2 000 µs, lissé au 1/16 → 125 µs. Neutralisation : diviser par 8.
        let mut r = Reception::default();
        r.absorber(&morceau(0, true, b"a"), 1_000);
        r.absorber(&morceau(10_000, true, b"b"), 13_000);
        assert!((r.gigue_ms() - 0.125).abs() < 1e-9, "gigue {}", r.gigue_ms());
    }

    #[test]
    fn le_transit_est_mesure_image_par_image() {
        // Neutralisation : un échantillon de transit par morceau — 2 au lieu de 1.
        let mut r = Reception::default();
        assert_eq!(r.transit_ms(), None);
        r.absorber(&morceau(0, true, b"a"), 2_000);
        r.absorber(&morceau(0, false, b"b"), 9_000);
        let q = r.transit_ms().unwrap();
        assert_eq!(q.echantillons, 1, "un seul échantillon : le second morceau n'ouvre pas d'image");
        assert!((q.p50 - 2.0).abs() < 1e-9);
    }
}
