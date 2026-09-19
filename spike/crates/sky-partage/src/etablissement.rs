//! L'établissement du canal, déplacé de `cmd_host::etablir` (C2). Même délai,
//! même boucle ; le diagnostic rend des NOMBRES observés, et l'arrêt est
//! consulté à chaque tour.

use std::time::{Duration, Instant};

use sky_net::{LinkEvent, PeerLink};

use crate::arret::Arret;
use crate::evenement::Diagnostic;

/// Délai maximal d'établissement, imposé par le document d'architecture (§5.5).
/// Jamais d'attente indéfinie, jamais de roue qui tourne sans fin.
pub const DELAI_ETABLISSEMENT: Duration = Duration::from_secs(25);

pub enum Etablissement {
    /// Canal de données ouvert après cette durée.
    Ouvert(Duration),
    /// Le lien a signalé un échec — message déjà sans adresse (`link.rs`).
    Rompu(String),
    /// Délai dépassé : ce qui a été observé.
    Delai(Diagnostic),
    Arrete,
}

/// Boucle jusqu'à ce que le canal de données soit utilisable, ou renonce.
pub fn etablir(link: &mut PeerLink, arret: &Arret) -> anyhow::Result<Etablissement> {
    let debut = Instant::now();
    loop {
        if arret.est_demande() {
            return Ok(Etablissement::Arrete);
        }
        match link.poll()? {
            LinkEvent::Failed(raison) => return Ok(Etablissement::Rompu(raison)),
            LinkEvent::Connected | LinkEvent::Data(_) | LinkEvent::Idle => {}
        }
        // Le canal de données, pas seulement ICE : c'est lui qui transporte.
        if link.canal_ouvert() {
            return Ok(Etablissement::Ouvert(debut.elapsed()));
        }
        if debut.elapsed() > DELAI_ETABLISSEMENT {
            return Ok(Etablissement::Delai(diagnostic(link)));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Les compteurs que `cmd_host::diagnostiquer` affichait au C2.
pub fn diagnostic(link: &PeerLink) -> Diagnostic {
    let (emis, recus, erreurs) = link.trafic();
    let (vers_local, vers_internet) = link.destinations();
    Diagnostic {
        ice_connecte: link.is_connected(),
        emis,
        recus,
        erreurs,
        vers_local,
        vers_internet,
        erreurs_socket: link.erreurs_socket(),
        delai: DELAI_ETABLISSEMENT,
    }
}
