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
    // L'exposé, ici, c'est l'opérateur — pas le correspondant, dont la réponse
    // voyage scellée. C'est donc dans SA console que l'avertissement a sa place,
    // et pas dans le mode d'emploi de l'ami.
    println!("  /!\\  Ce bloc contient l'adresse publique de cette machine, en clair");
    println!("       pour qui sait le décoder. Envoie-le en message privé, à une");
    println!("       personne précise — jamais dans un salon ouvert ni sur un forum.\n");
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
            diagnostiquer(link);
            return Ok(None);
        }

        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Explique l'échec sans accuser le NAT à tort.
///
/// `etablir` attend l'ouverture du canal de données, qui vient bien après ICE :
/// un échec peut donc venir du perçage de NAT, ou de la poignée de main chiffrée
/// qui le suit. Ce sont deux verdicts opposés pour la question centrale du
/// jalon, et c'est ce message qui sera consigné comme réponse. Il doit donc
/// distinguer les deux, et dire quand il n'est pas sûr de lui.
fn diagnostiquer(link: &PeerLink) {
    if link.is_connected() {
        println!(
            "ÉCHEC : le canal de données ne s'est pas ouvert en {} s.",
            DELAI_ETABLISSEMENT.as_secs()
        );
        println!("ATTENTION : la traversée de NAT n'est PAS en cause. Les deux machines");
        println!("se sont bel et bien trouvées — c'est la poignée de main chiffrée");
        println!("(DTLS/SCTP) qui n'a pas abouti. À ne pas compter comme un échec Q5.");
    } else {
        println!(
            "ÉCHEC : aucune connexion directe en {} s.",
            DELAI_ETABLISSEMENT.as_secs()
        );
        println!("Les deux machines ne se sont jamais trouvées.");
        println!("Cause probable : NAT strict d'un côté (4G, CGNAT, réseau d'entreprise).");
    }

    let erreurs = link.erreurs_socket();
    if erreurs > 0 {
        println!();
        println!("Réserve : {erreurs} erreur(s) sur le port UDP local pendant la tentative.");
        println!("Une cause locale (pare-feu, interface qui change) n'est pas exclue :");
        println!("le diagnostic ci-dessus est à prendre avec précaution.");
    }
}
