//! `sky-probe view` : un affichage de `sky_partage::regarder` (jalon 1,
//! tâche 5). Le flux reçu est écrit dans `sortie` dès le premier paquet —
//! `sky-partage` le jetterait ; c'est ce fichier qui lui fournit le puits.

use std::fs::File;
use std::time::{Duration, Instant};

use sky_compte::synchroniser;
use sky_partage::{
    regarder, Arret, Bilan, BilanReception, Designation, Evenement, Fin, Mesures, ParametresSpectateur, Puits,
};

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre};
use crate::cmd_host::{afficher, afficher_diagnostic, erreur_partage};

pub fn run(ami_designe: &str, secondes: u64, sortie: &str) -> anyhow::Result<()> {
    // Pris AVANT le coffre, comme au C2 : c'est d'ici que se mesurent
    // « Réponse reçue après » et « depuis le lancement de view ».
    let lancement = Instant::now();
    let (config, coffre) = config_et_coffre()?;
    // Jamais demandé : `view` s'arrête à `--seconds`, comme au C2.
    let arret = Arret::nouveau();
    let fin = regarder(
        &config,
        &coffre,
        |precedent| synchroniser(&config, &coffre, precedent),
        ParametresSpectateur {
            ami: Designation::Texte(ami_designe),
            duree_max: Some(Duration::from_secs(secondes)),
            lancement,
        },
        || Ok(Some(Box::new(File::create(sortie)?) as Puits)),
        &arret,
        &mut |evenement| afficher(&lignes_spectateur(&evenement, sortie)),
    )
    .map_err(erreur_partage)?;
    afficher_fin(fin, sortie)
}

/// Les lignes du C2, pour chaque événement du spectateur.
fn lignes_spectateur(evenement: &Evenement, sortie: &str) -> Vec<String> {
    match evenement {
        Evenement::DemandeEnvoyee { nom, deposes, appareils, attente } => vec![
            format!("\nDemande envoyée à {nom} ({deposes} appareil(s) sur {appareils})."),
            format!("J'attends sa réponse pendant {} s au maximum.", attente.as_secs()),
            avertissement_consommation("la réponse de ton ami"),
        ],
        Evenement::ReponseRecue { apres, synchronisations } => vec![format!(
            "Réponse reçue après {:.1} s ({synchronisations} synchronisations).",
            apres.as_secs_f32()
        )],
        Evenement::Negociation => vec!["Négociation en cours...".to_string()],
        Evenement::Connecte { en, depuis_le_lancement } => vec![
            format!(
                "CONNECTÉ en {:.1} s ({:.1} s depuis le lancement de view)",
                en.as_secs_f32(),
                depuis_le_lancement.unwrap_or_default().as_secs_f32()
            ),
            format!("Écriture du flux reçu dans {sortie}, dès le premier paquet.\n"),
        ],
        Evenement::Mesures(Mesures::Reception { debit_mbps, images_par_s, gigue_ms }) => {
            vec![format!("  {debit_mbps:.1} Mbps | {images_par_s} images/s | gigue {gigue_ms:.2} ms")]
        }
        // Événements de l'hôte : `regarder` ne les émet jamais.
        Evenement::Pret
        | Evenement::Disponible { .. }
        | Evenement::DemandeEcartee { .. }
        | Evenement::EchecLocal { .. }
        | Evenement::DemandeRecue { .. }
        | Evenement::Diffusion { .. }
        | Evenement::Mesures(Mesures::Envoi { .. }) => Vec::new(),
    }
}

fn afficher_fin(fin: Fin, sortie: &str) -> anyhow::Result<()> {
    match fin {
        Fin::AucunAppareilLocal => anyhow::bail!(
            "aucun appareil enregistré sur cette machine — lance d'abord \
             `sky-probe device register <nom>`."
        ),
        Fin::AucunAppareilChezLAmi { nom } => {
            println!("{nom} n'a aucun appareil enregistré : personne à qui envoyer la demande.")
        }
        Fin::DemandeRefusee { nom, appareils } => {
            println!("Le serveur a refusé la demande pour les {appareils} appareil(s) de {nom} : rien n'a été envoyé.");
            println!("{}", causes_d_un_depot_refuse());
        }
        Fin::PasDeReponse { nom } => println!("{nom} n'a pas répondu — est-il en partage ?"),
        Fin::NegociationRompue(raison) => println!("ÉCHEC : {raison}"),
        Fin::EtablissementEchoue(diagnostic) => afficher_diagnostic(&diagnostic),
        Fin::LienTombe(raison) => println!("\nÉCHEC : {raison}"),
        Fin::DureeEcoulee(bilan) => {
            if let Bilan::Reception(b) = *bilan {
                afficher_bilan(&b, sortie);
            }
        }
        // `view` ne demande jamais l'arrêt ; les autres fins sont celles de l'hôte.
        Fin::Arrete | Fin::AucuneDemande | Fin::ReponseRefusee | Fin::TamponSature { .. } => {}
    }
    Ok(())
}

fn afficher_bilan(b: &BilanReception, sortie: &str) {
    println!(
        "\nDébit moyen reçu : {:.1} Mbps sur {:.0} s ({} images, {} Mo)",
        b.octets as f64 * 8.0 / b.duree_s / 1e6,
        b.duree_s,
        b.images,
        b.octets / 1_000_000
    );
    println!("Fichier écrit    : {sortie}");
    match &b.transit_ms {
        None => println!("Transit sur le lien : non mesuré (aucune image reçue)"),
        Some(q) => println!(
            "Transit sur le lien (médian / p99) : {:.2} ms / {:.2} ms  ({} échantillons)",
            q.p50, q.p99, q.echantillons
        ),
    }
    println!("Gigue finale (RFC 3550, lissée) : {:.2} ms", b.gigue_ms);
    if b.vers_internet == 0 {
        println!("Rappel : aucun paquet n'est parti vers internet — lien local.");
    } else {
        println!("Lien reseau reel : {} paquets emis vers internet.", b.vers_internet);
        println!("Ces mesures sont celles d'une vraie liaison entre deux machines.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_ligne_connecte_du_spectateur_est_celle_du_c2() {
        let lignes = lignes_spectateur(
            &Evenement::Connecte {
                en: Duration::from_millis(600),
                depuis_le_lancement: Some(Duration::from_millis(7_100)),
            },
            "recu.h265",
        );
        assert_eq!(
            lignes,
            vec![
                "CONNECTÉ en 0.6 s (7.1 s depuis le lancement de view)".to_string(),
                "Écriture du flux reçu dans recu.h265, dès le premier paquet.\n".to_string(),
            ]
        );
    }
}
