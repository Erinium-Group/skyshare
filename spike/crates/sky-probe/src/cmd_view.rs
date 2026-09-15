//! Côté spectateur : demande le partage d'un ami par la boîte aux lettres,
//! intègre sa réponse, reçoit le flux vidéo réel et l'écrit dans `recu.h265`
//! — la chaîne complète (Tâche 8).
//!
//! La négociation ne passe plus par un humain : l'offre est déposée pour les
//! appareils de l'ami, la réponse relevée à la synchronisation. Ce qui décide
//! vit dans `rendez_vous` ; ce fichier ne garde que la colle réseau.
//!
//! Piège documenté (Tâche 3) : les en-têtes de séquence (VPS/SPS/PPS) ne sont
//! émis qu'une fois, au tout début du flux — GOP infini oblige. Ce spectateur
//! écrit donc depuis le tout premier paquet reçu après connexion : il n'existe
//! aucun chemin de code ici qui commencerait à enregistrer en cours de route.

use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use sky_compte::{deposer, relever, resoudre_ami, synchroniser};
use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre};
use crate::cmd_host::{epoch_us, erreur_compte, etablir, EN_TETE_MORCEAU};
use crate::rendez_vous::{
    interroger, reponse_a_l_offre, session_de, HorlogeReelle, ATTENTE_SPECTATEUR, CADENCE,
};

/// Période d'émission du retour vers l'émetteur.
///
/// Le retour ne prouve plus seulement que le canal fonctionne dans les deux
/// sens (Tâche 7) : il porte désormais l'horodatage du dernier paquet vidéo
/// reçu, ce qui permet à l'émetteur de calculer un aller-retour (RTT) réel —
/// c'est ce dont le `Pacer` de la Tâche 8 se nourrit.
const PERIODE_RETOUR: Duration = Duration::from_millis(200);

pub fn run(ami_designe: &str, secondes: u64, sortie: &str) -> anyhow::Result<()> {
    let lancement = Instant::now();
    let (config, coffre) = config_et_coffre()?;

    // Avant tout réseau : sans appareil enregistré, `deposer` refuserait de
    // toute façon — mais seulement après une synchronisation et la
    // découverte d'adresse d'`offrant`.
    if coffre.identifiant_appareil().map_err(erreur_compte)?.is_none() {
        anyhow::bail!(
            "aucun appareil enregistré sur cette machine — lance d'abord \
             `sky-probe device register <nom>`."
        );
    }
    // L'identité DURABLE du coffre : c'est pour sa clé d'annuaire que l'hôte
    // scelle l'enveloppe de sa réponse. La clé de l'offre, elle, est éphémère.
    let identite = coffre.identite().map_err(erreur_compte)?;

    let etat = synchroniser(&config, &coffre, None).map_err(erreur_compte)?;
    let ami = resoudre_ami(&etat, ami_designe).map_err(erreur_compte)?.clone();
    if ami.appareils.is_empty() {
        println!(
            "{} n'a aucun appareil enregistré : personne à qui envoyer la demande.",
            ami.discord_name
        );
        return Ok(());
    }

    let (mut link, offre) = PeerLink::offrant(Identity::generate())?;
    let session = session_de(&offre)?;

    // Le bloc part tel quel : `offrant` a déjà comprimé le SDP, et `deposer`
    // scelle pour chaque appareil de l'ami avec sa clé d'annuaire.
    let deposes = deposer(&config, &coffre, &ami.appareils, offre.as_bytes()).map_err(erreur_compte)?;
    if deposes == 0 {
        println!(
            "Le serveur a refusé la demande pour les {} appareil(s) de {} : rien n'a été envoyé.",
            ami.appareils.len(),
            ami.discord_name
        );
        println!("{}", causes_d_un_depot_refuse());
        return Ok(());
    }
    println!(
        "\nDemande envoyée à {} ({deposes} appareil(s) sur {}).",
        ami.discord_name,
        ami.appareils.len()
    );
    println!(
        "J'attends sa réponse pendant {} s au maximum.",
        ATTENTE_SPECTATEUR.as_secs()
    );
    // Même limite que `host` (revue finale, m3) : la réponse de l'ami arrive
    // par une enveloppe que le serveur efface en la livrant — une autre
    // commande qui synchronise pendant cette attente la consommerait, et ce
    // `view` conclurait à tort « n'a pas répondu ».
    println!("{}", avertissement_consommation("la réponse de ton ami"));
    std::io::stdout().flush().ok();

    // Le mapping NAT du port annoncé dans l'offre doit survivre à l'attente.
    // Le battement n'utilise que le socket : l'horloge de `str0m` ne court pas.
    let garde = link.maintenir_mapping()?;

    let mut synchronisations = 1u32; // celle qui a résolu l'ami
    let mut horloge = HorlogeReelle::demarrer();
    let reponse = interroger(
        Some(etat),
        |precedent| {
            synchronisations += 1;
            synchroniser(&config, &coffre, precedent)
        },
        |etat| reponse_a_l_offre(relever(etat, &identite), session, &ami.appareils),
        &mut horloge,
        CADENCE,
        ATTENTE_SPECTATEUR,
    )
    .map_err(erreur_compte)?;

    let Some(reponse) = reponse else {
        println!("{} n'a pas répondu — est-il en partage ?", ami.discord_name);
        return Ok(());
    };
    println!(
        "Réponse reçue après {:.1} s ({synchronisations} synchronisations).",
        lancement.elapsed().as_secs_f32()
    );

    // La négociation produit désormais son propre trafic.
    drop(garde);
    link.accepter_reponse(&reponse)?;

    println!("Négociation en cours...");
    std::io::stdout().flush().ok();

    let Some(duree) = etablir(&mut link)? else {
        return Ok(());
    };
    println!(
        "CONNECTÉ en {:.1} s ({:.1} s depuis le lancement de view)",
        duree.as_secs_f32(),
        lancement.elapsed().as_secs_f32()
    );
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
    let (_, vers_internet) = link.destinations();
    if vers_internet == 0 {
        println!("Rappel : aucun paquet n'est parti vers internet — lien local.");
    } else {
        println!("Lien reseau reel : {vers_internet} paquets emis vers internet.");
        println!("Ces mesures sont celles d'une vraie liaison entre deux machines.");
    }
    Ok(())
}

fn percentile_i64(tries: &[i64], p: usize) -> i64 {
    if tries.is_empty() {
        return 0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}
