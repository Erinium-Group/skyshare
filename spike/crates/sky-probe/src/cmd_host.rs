//! Côté émetteur : produit l'offre, intègre la réponse, puis pousse des données.

use std::io::Write;
use std::time::{Duration, Instant};

use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

/// Charge d'un message : proche d'un paquet réseau plein, sans fragmenter.
const CHARGE: usize = 1200;

/// Délai maximal d'établissement, imposé par le document d'architecture (§5.5).
/// Jamais d'attente indéfinie, jamais de roue qui tourne sans fin.
pub const DELAI_ETABLISSEMENT: Duration = Duration::from_secs(8);

pub fn run(secondes: u64) -> anyhow::Result<()> {
    let (mut link, offre) = PeerLink::host(Identity::generate())?;

    println!("\n=== ÉTAPE 1 : envoie ce bloc à ton correspondant ===\n");
    println!("{offre}\n");
    println!("=== ÉTAPE 2 : colle sa réponse ici puis Entrée ===\n");
    std::io::stdout().flush().ok();

    let mut reponse = String::new();
    std::io::stdin().read_line(&mut reponse)?;
    link.accept_answer(&reponse)?;

    println!("\nNégociation en cours...");
    std::io::stdout().flush().ok();

    let Some(duree) = etablir(&mut link)? else {
        return Ok(());
    };
    println!("CONNECTÉ en {:.1} s", duree.as_secs_f32());

    // Émission continue : c'est le débit réellement soutenu qui nous intéresse,
    // pas un pic. Le pilotage fin du débit appartient à la tâche 8.
    let charge = vec![0xABu8; CHARGE];
    let mut envoyes = 0u64;
    let mut retours = 0u64;
    let t0 = Instant::now();

    while t0.elapsed() < Duration::from_secs(secondes) {
        if link.send(&charge).is_ok() {
            envoyes += charge.len() as u64;
        }
        match link.poll()? {
            // Les retours du spectateur : c'est ce qui prouve le sens inverse.
            LinkEvent::Data(_) => retours += 1,
            LinkEvent::Failed(raison) => {
                println!("\nÉCHEC : {raison}");
                return Ok(());
            }
            _ => {}
        }
    }

    let ecoule = t0.elapsed().as_secs_f64();
    println!(
        "\nDébit soutenu : {:.1} Mbps sur {:.0} s ({} Mo envoyés)",
        envoyes as f64 * 8.0 / ecoule / 1e6,
        ecoule,
        envoyes / 1_000_000
    );
    println!("Retours reçus du spectateur : {retours}");
    Ok(())
}

/// Boucle jusqu'à ce que le canal de données soit utilisable, ou renonce.
///
/// Partagée avec `cmd_view` : les deux bords appliquent exactement le même
/// délai et le même diagnostic. Rend `None` quand la tentative a échoué — le
/// message a déjà été affiché.
///
/// Aucune adresse n'est affichée : les diagnostics parlent de causes, jamais
/// de machines.
pub fn etablir(link: &mut PeerLink) -> anyhow::Result<Option<Duration>> {
    let debut = Instant::now();
    loop {
        match link.poll()? {
            LinkEvent::Failed(raison) => {
                println!("ÉCHEC : {raison}");
                return Ok(None);
            }
            LinkEvent::Connected | LinkEvent::Data(_) | LinkEvent::Idle => {}
        }

        // Le canal de données, pas seulement ICE : c'est lui qui transporte.
        if link.canal_ouvert() {
            return Ok(Some(debut.elapsed()));
        }

        if debut.elapsed() > DELAI_ETABLISSEMENT {
            println!(
                "ÉCHEC : aucune connexion directe en {} s.",
                DELAI_ETABLISSEMENT.as_secs()
            );
            println!("Cause probable : NAT strict d'un côté (4G, CGNAT, réseau d'entreprise).");
            return Ok(None);
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}
