//! Côté spectateur : consomme l'offre, renvoie la réponse scellée, reçoit.

use std::io::Write;
use std::time::{Duration, Instant};

use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

use crate::cmd_host::etablir;

/// Petit message de retour vers l'émetteur.
///
/// Il prouve que le canal fonctionne dans les deux sens. Au jalon 1, c'est par
/// ce chemin que remonteront la perte et le RTT dont le `Pacer` a besoin.
const RETOUR: &[u8] = b"sky-retour";
const PERIODE_RETOUR: Duration = Duration::from_millis(200);

/// Combien de temps on laisse au correspondant pour coller notre réponse.
///
/// Ce n'est pas le délai d'établissement : tant que l'émetteur n'a rien collé,
/// il n'y a rien à attendre. Le compte à rebours des 8 s ne démarre qu'au
/// premier signe de vie d'en face. Cette attente-ci reste bornée elle aussi :
/// le programme rend la main plutôt que de tourner indéfiniment.
const ATTENTE_CORRESPONDANT: Duration = Duration::from_secs(120);

pub fn run(secondes: u64) -> anyhow::Result<()> {
    println!("\n=== ÉTAPE 1 : colle ici le bloc reçu puis Entrée ===\n");
    std::io::stdout().flush().ok();

    let mut offre = String::new();
    std::io::stdin().read_line(&mut offre)?;

    let (mut link, reponse) = PeerLink::viewer(Identity::generate(), &offre)?;

    println!("\n=== ÉTAPE 2 : renvoie ce bloc à ton correspondant ===\n");
    println!("{reponse}\n");
    println!("En attente de sa connexion...");
    std::io::stdout().flush().ok();

    if !attendre_contact(&mut link)? {
        println!("ÉCHEC : le correspondant ne s'est pas manifesté.");
        println!("Rien n'est perdu : redemande-lui un bloc et recommence.");
        return Ok(());
    }

    let Some(duree) = etablir(&mut link)? else {
        return Ok(());
    };
    println!("CONNECTÉ en {:.1} s", duree.as_secs_f32());
    std::io::stdout().flush().ok();

    let mut recus = 0u64;
    let mut messages = 0u64;
    let t0 = Instant::now();
    let mut dernier_affichage = Instant::now();
    let mut dernier_retour = Instant::now();
    let mut recus_precedent = 0u64;

    while t0.elapsed() < Duration::from_secs(secondes) {
        match link.poll()? {
            LinkEvent::Data(d) => {
                recus += d.len() as u64;
                messages += 1;
            }
            LinkEvent::Failed(raison) => {
                println!("\nÉCHEC : {raison}");
                return Ok(());
            }
            _ => {}
        }

        if dernier_retour.elapsed() >= PERIODE_RETOUR {
            // Un échec d'envoi n'est pas grave ici : c'est un retour, pas la vidéo.
            let _ = link.send(RETOUR);
            dernier_retour = Instant::now();
        }

        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta = recus - recus_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            println!("  {:.1} Mbps", delta as f64 * 8.0 / ecoule / 1e6);
            std::io::stdout().flush().ok();
            recus_precedent = recus;
            dernier_affichage = Instant::now();
        }
    }

    let ecoule = t0.elapsed().as_secs_f64();
    println!(
        "\nDébit moyen reçu : {:.1} Mbps sur {:.0} s ({messages} messages)",
        recus as f64 * 8.0 / ecoule / 1e6,
        ecoule
    );
    Ok(())
}

/// Attend le premier datagramme du correspondant, sans jamais bloquer sans fin.
///
/// Un lien qui meurt avant le moindre contact et un correspondant qui ne colle
/// jamais rien se soldent par le même diagnostic — rendu une seule fois par
/// l'appelant, pour ne pas afficher deux messages d'échec de suite.
fn attendre_contact(link: &mut PeerLink) -> anyhow::Result<bool> {
    let debut = Instant::now();
    while debut.elapsed() < ATTENTE_CORRESPONDANT {
        if link.contact_recu() {
            return Ok(true);
        }
        if let LinkEvent::Failed(_) = link.poll()? {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(false)
}
