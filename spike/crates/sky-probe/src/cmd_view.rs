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
///
/// Dix minutes, et non deux : entre l'affichage de la réponse et le premier
/// paquet d'en face, il faut qu'un humain copie près de 4 000 caractères, les
/// colle dans une messagerie, les envoie, et qu'un second humain les récupère et
/// les colle à son tour. Une fenêtre trop courte ferait échouer le test pour une
/// raison qui n'a rien à voir avec le réseau — et c'est un test qui mobilise
/// deux personnes, donc coûteux à répéter.
const ATTENTE_CORRESPONDANT: Duration = Duration::from_secs(600);

/// À quelle fréquence rappeler qu'on attend toujours.
///
/// Un écran muet pendant dix minutes se referme.
const PERIODE_RAPPEL: Duration = Duration::from_secs(30);

pub fn run(secondes: u64) -> anyhow::Result<()> {
    println!("\n=== ÉTAPE 1 : colle ici le bloc reçu puis Entrée ===\n");
    std::io::stdout().flush().ok();

    let mut offre = String::new();
    std::io::stdin().read_line(&mut offre)?;

    let (mut link, reponse) = PeerLink::viewer(Identity::generate(), &offre)?;

    println!("\n=== ÉTAPE 2 : renvoie ce bloc à ton correspondant ===\n");
    println!("{reponse}\n");
    println!("Renvoie ce bloc en entier, puis laisse cette fenêtre ouverte.");
    println!(
        "J'attends son signal pendant {} minutes au maximum.\n",
        ATTENTE_CORRESPONDANT.as_secs() / 60
    );
    std::io::stdout().flush().ok();

    let debut_attente = Instant::now();
    if !attendre_contact(&mut link)? {
        // Formuler ce qu'on observe, pas ce qu'on en déduit.
        //
        // Tout ce que `contact_en_attente` constate, c'est qu'aucun datagramme
        // n'a atteint le socket. En conclure que « personne n'a essayé » serait
        // faux dans le cas même que Q5 existe pour tester : derrière un NAT
        // symétrique, l'émetteur peut avoir lancé toute sa négociation et émis
        // tous ses sondages sans qu'un seul ne parvienne. Affirmer ici ferait
        // lire « rien ne s'est passé » là où il faut lire « le réseau a tout
        // bloqué » — l'exact symétrique du défaut corrigé côté émetteur.
        //
        // D'où la même prudence que `cmd_host::diagnostiquer` : les deux causes
        // sont laissées ouvertes, car rien ici ne permet de trancher.
        println!(
            "\nÉCHEC : aucun paquet ne nous est parvenu en {} s.",
            debut_attente.elapsed().as_secs()
        );
        println!("Deux causes possibles, et rien de ce qu'on voit d'ici ne permet");
        println!("de choisir entre elles :");
        println!("  - le correspondant n'a pas encore collé notre bloc de son côté ;");
        println!("  - il l'a fait, mais ses paquets n'ont pas franchi le réseau.");
        println!("Redemande-lui un bloc et recommence, c'est sans risque.");
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
/// Cette boucle n'appelle **volontairement pas** `poll` : elle se contente
/// d'observer le socket. Interroger `str0m` ferait courir ses minuteries, et sa
/// poignée de main DTLS abandonnerait au bout d'une trentaine de secondes —
/// bien avant que deux humains aient fini de s'échanger un bloc de 3 800
/// caractères. Voir `PeerLink::maintenant`.
fn attendre_contact(link: &mut PeerLink) -> anyhow::Result<bool> {
    let debut = Instant::now();
    let mut dernier_rappel = Instant::now();

    while debut.elapsed() < ATTENTE_CORRESPONDANT {
        if link.contact_en_attente() {
            return Ok(true);
        }

        if dernier_rappel.elapsed() >= PERIODE_RAPPEL {
            let reste = ATTENTE_CORRESPONDANT.saturating_sub(debut.elapsed());
            println!(
                "  toujours en attente — encore {} min {:02} s. Ne ferme pas cette fenêtre.",
                reste.as_secs() / 60,
                reste.as_secs() % 60
            );
            std::io::stdout().flush().ok();
            dernier_rappel = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(false)
}
