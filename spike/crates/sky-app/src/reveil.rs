//! Le sommeil de la boucle, interruptible : un changement de fenêtre ou la
//! fin d'un partage change la cadence tout de suite, pas au bout de 5 min.

use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Default)]
pub struct Reveil {
    sonne: Mutex<bool>,
    condition: Condvar,
}

impl Reveil {
    pub fn sonner(&self) {
        *self.sonne.lock().expect("réveil empoisonné") = true;
        self.condition.notify_all();
    }

    /// Dort au plus `duree`, moins si `sonner` est appelé. Un réveil sonné
    /// pendant le tour précédent fait rendre la main aussitôt : c'est voulu.
    pub fn attendre(&self, duree: Duration) {
        let garde = self.sonne.lock().expect("réveil empoisonné");
        let (mut garde, _) = self
            .condition
            .wait_timeout_while(garde, duree, |sonne| !*sonne)
            .expect("réveil empoisonné");
        *garde = false;
    }
}

/// Le temps tel que la boucle le voit — injecté pour que les tests ne dorment pas.
pub trait Sommeil {
    fn dormir(&mut self, duree: Duration, reveil: &Reveil);
}

pub struct SommeilReel;

impl Sommeil for SommeilReel {
    fn dormir(&mut self, duree: Duration, reveil: &Reveil) {
        reveil.attendre(duree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn un_reveil_interrompt_le_sommeil() {
        // Neutralisation : retirer `notify_all` de `sonner` — le sommeil dure 10 s.
        let reveil = Arc::new(Reveil::default());
        let depuis_un_fil = Arc::clone(&reveil);
        let fil = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            depuis_un_fil.sonner();
        });
        let debut = Instant::now();
        reveil.attendre(Duration::from_secs(10));
        fil.join().unwrap();
        assert!(debut.elapsed() < Duration::from_secs(2));
    }
}
