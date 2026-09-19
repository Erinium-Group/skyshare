//! L'arrêt : ce qui permet à l'application d'interrompre une attente de
//! 30 minutes (hôte) ou de 60 secondes (spectateur), une négociation ou un
//! flux, sans toucher à `interroger` ni à ses tests (déplacés tels quels).
//!
//! Deux pièces. L'HORLOGE rend la main dès que l'arrêt est demandé, au lieu
//! de dormir ses 2 s. La fermeture `synchroniser_sauf_arret` rend alors une
//! erreur FATALE au tour suivant, qu'`interroger` ne retente pas.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sky_compte::{ErreurCompte, Etat};

use crate::rendez_vous::{ErreurDeSynchronisation, Horloge};

/// Signal d'arrêt partagé entre le fil qui partage et celui qui le demande
/// (le bouton Arrêter). Cloner rend un signal LIÉ, pas une copie.
#[derive(Clone, Default)]
pub struct Arret(Arc<AtomicBool>);

impl Arret {
    pub fn nouveau() -> Arret {
        Arret::default()
    }

    pub fn demander(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn est_demande(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Granularité de l'attente : l'arrêt est vu au plus 50 ms après la
/// demande. Point de départ argumenté, pas une mesure : sous le seuil où un
/// clic « Arrêter » paraît ignoré, et sans réveil inutile toutes les ms.
pub const PAS_D_ATTENTE: Duration = Duration::from_millis(50);

/// L'horloge réelle, interruptible. Remplace `HorlogeReelle` partout où un
/// arrêt doit être possible ; sans arrêt demandé, elle attend exactement
/// autant qu'elle.
pub struct HorlogeArretable<'a> {
    debut: Instant,
    arret: &'a Arret,
}

impl<'a> HorlogeArretable<'a> {
    pub fn demarrer(arret: &'a Arret) -> HorlogeArretable<'a> {
        HorlogeArretable { debut: Instant::now(), arret }
    }
}

impl Horloge for HorlogeArretable<'_> {
    fn ecoule(&self) -> Duration {
        self.debut.elapsed()
    }

    fn attendre(&mut self, duree: Duration) {
        let fin = Instant::now() + duree;
        while !self.arret.est_demande() {
            let reste = fin.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                break;
            }
            std::thread::sleep(reste.min(PAS_D_ATTENTE));
        }
    }
}

/// Erreur d'une attente arrêtable : celle du compte, ou l'arrêt lui-même.
#[derive(Debug)]
pub enum ErreurAttente {
    Compte(ErreurCompte),
    Arrete,
}

impl ErreurDeSynchronisation for ErreurAttente {
    fn est_fatale(&self) -> bool {
        match self {
            ErreurAttente::Compte(e) => e.est_fatale(),
            // Un arrêt ne se « résout » pas en réessayant : le retenter
            // ferait attendre l'utilisateur trois tours de plus.
            ErreurAttente::Arrete => true,
        }
    }
}

/// Enveloppe `synchroniser` : si l'arrêt est demandé, rend `Arrete` SANS
/// appeler le site — une synchronisation de plus consommerait peut-être une
/// enveloppe que plus personne n'attend.
pub fn synchroniser_sauf_arret<'a, S>(
    arret: &'a Arret,
    mut synchroniser: S,
) -> impl FnMut(Option<&Etat>) -> Result<Etat, ErreurAttente> + 'a
where
    S: FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte> + 'a,
{
    move |precedent: Option<&Etat>| {
        if arret.est_demande() {
            return Err(ErreurAttente::Arrete);
        }
        synchroniser(precedent).map_err(ErreurAttente::Compte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendez_vous::{interroger, Horloge};
    use std::time::{Duration, Instant};

    #[derive(Default)]
    struct HorlogeFictive {
        maintenant: Duration,
        attentes: Vec<Duration>,
    }

    impl Horloge for HorlogeFictive {
        fn ecoule(&self) -> Duration {
            self.maintenant
        }
        fn attendre(&mut self, duree: Duration) {
            self.maintenant += duree;
            self.attentes.push(duree);
        }
    }

    fn etat_vide() -> Etat {
        Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis: Vec::new(),
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        }
    }

    #[test]
    fn l_attente_rend_la_main_des_que_l_arret_est_demande() {
        // Neutralisation : un `sleep(duree)` d'un seul tenant dans
        // `attendre` — l'attente dure 5 s, le test rougit.
        let arret = Arret::nouveau();
        let depuis_un_fil = arret.clone();
        let fil = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            depuis_un_fil.demander();
        });
        let debut = Instant::now();
        HorlogeArretable::demarrer(&arret).attendre(Duration::from_secs(5));
        fil.join().unwrap();
        assert!(debut.elapsed() < Duration::from_secs(1), "attente de {:?}", debut.elapsed());
    }

    #[test]
    fn sans_arret_l_attente_dure_ce_qu_on_lui_demande() {
        // Neutralisation : sortir de la boucle au premier tour — l'attente
        // rend la main aussitôt, le test rougit.
        let arret = Arret::nouveau();
        let debut = Instant::now();
        HorlogeArretable::demarrer(&arret).attendre(Duration::from_millis(200));
        assert!(debut.elapsed() >= Duration::from_millis(200));
    }

    #[test]
    fn un_arret_interrompt_interroger_au_tour_suivant_sans_retenter() {
        // L'arrêt est une erreur FATALE : `interroger` ne la retente pas.
        // Neutralisation : `ErreurAttente::Arrete => false` dans
        // `est_fatale` — trois tentatives au lieu d'une, trois attentes.
        let arret = Arret::nouveau();
        let mut appels = 0u32;
        let mut horloge = HorlogeFictive::default();
        let issue = interroger(
            None,
            synchroniser_sauf_arret(&arret, |_| {
                appels += 1;
                arret.demander();
                Ok(etat_vide())
            }),
            |_| None::<()>,
            &mut horloge,
            Duration::from_secs(2),
            Duration::from_secs(60),
        );
        assert!(matches!(issue, Err(ErreurAttente::Arrete)));
        assert_eq!(appels, 1, "la fermeture réelle n'est plus appelée après l'arrêt");
        assert_eq!(horloge.attentes.len(), 1, "une seule attente, entre le succès et l'arrêt");
    }
}
