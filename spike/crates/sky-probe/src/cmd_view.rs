//! Côté spectateur : consomme l'offre, renvoie la réponse scellée, reçoit le
//! flux vidéo réel et l'écrit dans `recu.h265` — la chaîne complète (Tâche 8).
//!
//! Piège documenté (Tâche 3) : les en-têtes de séquence (VPS/SPS/PPS) ne sont
//! émis qu'une fois, au tout début du flux — GOP infini oblige. Ce spectateur
//! écrit donc depuis le tout premier paquet reçu après connexion : il n'existe
//! aucun chemin de code ici qui commencerait à enregistrer en cours de route.

use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

use crate::cmd_host::{epoch_us, etablir, EN_TETE_MORCEAU};

/// Période d'émission du retour vers l'émetteur.
///
/// Le retour ne prouve plus seulement que le canal fonctionne dans les deux
/// sens (Tâche 7) : il porte désormais l'horodatage du dernier paquet vidéo
/// reçu, ce qui permet à l'émetteur de calculer un aller-retour (RTT) réel —
/// c'est ce dont le `Pacer` de la Tâche 8 se nourrit.
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

pub fn run(secondes: u64, sortie: &str) -> anyhow::Result<()> {
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

    // Même bug que côté émetteur, et il est ici encore plus dommageable :
    // l'attente y dure plus longtemps. Tant que rien n'est reçu, l'agent ICE
    // n'a aucune raison d'émettre, donc le port annoncé dans notre réponse
    // n'est plus ouvert quand le correspondant s'en sert enfin.
    let garde = link.maintenir_mapping()?;

    let debut_attente = Instant::now();
    if !attendre_contact(&mut link)? {
        // Formuler ce qu'on observe, pas ce qu'on en déduit — voir Tâche 7.
        println!(
            "\nÉCHEC : aucun paquet ne nous est parvenu en {} s.",
            debut_attente.elapsed().as_secs()
        );
        let (emis, recus, erreurs) = link.trafic();
        println!();
        println!("  Datagrammes émis   : {emis}");
        println!("  Datagrammes reçus  : {recus}");
        println!("  Erreurs de socket  : {erreurs}");
        println!();
        println!("Deux causes possibles, et rien de ce qu'on voit d'ici ne permet");
        println!("de choisir entre elles :");
        println!("  - le correspondant n'a pas encore collé notre bloc de son côté ;");
        println!("  - il l'a fait, mais ses paquets n'ont pas franchi le réseau.");
        println!();
        println!("Envoie ces trois nombres à ton correspondant : confrontés aux siens,");
        println!("ils désignent la cause. Redemande-lui un bloc et recommence, c'est");
        println!("sans risque.");
        return Ok(());
    }

    // Le contact est établi : la négociation produit désormais son propre trafic.
    drop(garde);

    let Some(duree) = etablir(&mut link)? else {
        return Ok(());
    };
    println!("CONNECTÉ en {:.1} s", duree.as_secs_f32());
    println!("Écriture du flux reçu dans {sortie}, dès le premier paquet.\n");
    std::io::stdout().flush().ok();

    let mut fichier = File::create(sortie)?;

    let mut recus_octets = 0u64;
    let mut images = 0u64;
    // Dernier horodatage d'ÉMISSION vu (celui embarqué par l'hôte dans le
    // paquet), et dernier instant d'ARRIVÉE — les deux sur la même horloge
    // système (les deux processus tournent sur la même machine dans ce
    // spike). C'est ce qui permet un calcul de gigue conforme à RFC 3550,
    // plutôt qu'une simple variation d'intervalle d'affichage.
    let mut dernier_horodatage_emission: Option<u64> = None;
    let mut dernier_horodatage_arrivee: Option<u64> = None;
    let mut gigue_us: f64 = 0.0;
    let mut transit_echantillons_us: Vec<i64> = Vec::new();

    let t0 = Instant::now();
    let mut dernier_affichage = Instant::now();
    let mut dernier_retour = Instant::now();
    let mut recus_precedent = 0u64;
    let mut images_precedent = 0u64;

    while t0.elapsed() < Duration::from_secs(secondes) {
        match link.poll()? {
            LinkEvent::Data(d) => {
                if d.len() > EN_TETE_MORCEAU {
                    // En-tête de morceau (voir `cmd_host::EN_TETE_MORCEAU`) :
                    // 8 octets d'horodatage d'émission, 1 octet drapeau (non
                    // nul = premier morceau de l'image). Un paquet NVENC peut
                    // être redécoupé en plusieurs morceaux ; seul le premier
                    // porte les statistiques par IMAGE (transit, gigue,
                    // compteur) — la charge utile de chacun est de toute façon
                    // recollée dans l'ordre d'arrivée.
                    let horodatage_emission = u64::from_le_bytes(d[..8].try_into().unwrap());
                    let premier_morceau = d[8] != 0;
                    let charge_utile = &d[EN_TETE_MORCEAU..];

                    if premier_morceau {
                        let horodatage_arrivee = epoch_us();

                        // Temps de transit sur le lien : arrivée moins
                        // émission, même horloge système. Composante de la
                        // latence de bout en bout distincte de la durée
                        // d'encodage (déjà comptée par NVENC avant
                        // l'horodatage) — voir le rapport.
                        transit_echantillons_us
                            .push(horodatage_arrivee as i64 - horodatage_emission as i64);

                        // Gigue RFC 3550 : écart entre la variation d'arrivée
                        // et la variation d'émission, lissé sur une fenêtre
                        // glissante, calculée IMAGE à IMAGE (pas morceau à
                        // morceau, ce qui mesurerait notre propre découpage).
                        if let (Some(prec_emis), Some(prec_arr)) =
                            (dernier_horodatage_emission, dernier_horodatage_arrivee)
                        {
                            let delta_emission = horodatage_emission as i64 - prec_emis as i64;
                            let delta_arrivee = horodatage_arrivee as i64 - prec_arr as i64;
                            let dev = ((delta_arrivee - delta_emission).unsigned_abs()) as f64;
                            gigue_us += (dev - gigue_us) / 16.0;
                        }
                        dernier_horodatage_emission = Some(horodatage_emission);
                        dernier_horodatage_arrivee = Some(horodatage_arrivee);
                        images += 1;
                    }

                    fichier.write_all(charge_utile)?;
                    recus_octets += charge_utile.len() as u64;
                }
                // Un message trop court pour porter un en-tête complet est
                // ignoré plutôt que d'écrire un fragment d'en-tête dans le
                // fichier.
            }
            LinkEvent::Failed(raison) => {
                println!("\nÉCHEC : {raison}");
                fichier.flush().ok();
                return Ok(());
            }
            _ => {}
        }

        if dernier_retour.elapsed() >= PERIODE_RETOUR {
            // Le retour porte l'horodatage d'émission du dernier paquet vidéo
            // reçu, tel quel : c'est ce qui permet à l'hôte de calculer un RTT
            // réel. Rien à renvoyer tant qu'aucun paquet n'est encore arrivé.
            if let Some(h) = dernier_horodatage_emission {
                let _ = link.send(&h.to_le_bytes());
            }
            dernier_retour = Instant::now();
        }

        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let delta_octets = recus_octets - recus_precedent;
            let delta_images = images - images_precedent;
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            println!(
                "  {:.1} Mbps | {delta_images} images/s | gigue {:.2} ms",
                delta_octets as f64 * 8.0 / ecoule / 1e6,
                gigue_us / 1000.0
            );
            std::io::stdout().flush().ok();
            recus_precedent = recus_octets;
            images_precedent = images;
            dernier_affichage = Instant::now();
        }
    }

    fichier.flush()?;

    let ecoule = t0.elapsed().as_secs_f64();
    transit_echantillons_us.sort_unstable();

    println!(
        "\nDébit moyen reçu : {:.1} Mbps sur {ecoule:.0} s ({images} images, {} Mo)",
        recus_octets as f64 * 8.0 / ecoule / 1e6,
        recus_octets / 1_000_000
    );
    println!("Fichier écrit    : {sortie}");
    if transit_echantillons_us.is_empty() {
        println!("Transit sur le lien : non mesuré (aucune image reçue)");
    } else {
        println!(
            "Transit sur le lien (médian / p99) : {:.2} ms / {:.2} ms  ({} échantillons)",
            percentile_i64(&transit_echantillons_us, 50) as f64 / 1000.0,
            percentile_i64(&transit_echantillons_us, 99) as f64 / 1000.0,
            transit_echantillons_us.len()
        );
    }
    println!(
        "Gigue finale (RFC 3550, lissée) : {:.2} ms",
        gigue_us / 1000.0
    );
    println!("Rappel : lien en boucle locale sur cette machine — ce débit, ce transit");
    println!("et cette gigue mesurent le chiffrement et le transport en mémoire,");
    println!("jamais un réseau.");
    Ok(())
}

fn percentile_i64(tries: &[i64], p: usize) -> i64 {
    if tries.is_empty() {
        return 0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
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
            let (emis, recus, _) = link.trafic();
            println!(
                "  toujours en attente — encore {} min {:02} s. Ne ferme pas cette fenêtre.",
                reste.as_secs() / 60,
                reste.as_secs() % 60
            );
            println!("    (émis {emis}, reçus {recus} — reçus > 0 signifie qu'il nous a trouvés)");
            std::io::stdout().flush().ok();
            dernier_rappel = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(false)
}
