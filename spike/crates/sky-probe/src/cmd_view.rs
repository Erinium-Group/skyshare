//! `sky-probe view` : un affichage de `sky_partage::regarder` (jalon 1,
//! tâche 5). Le flux reçu est écrit dans `sortie` dès le premier paquet —
//! `sky-partage` le jetterait ; c'est ce fichier qui lui fournit le puits.

use std::fs::File;
use std::time::{Duration, Instant};

use sky_compte::synchroniser;
use sky_partage::{
    regarder, Arret, Bilan, BilanReception, Designation, Evenement, Fin, FormatVideo, Mesures,
    ParametresSpectateur, Puits,
};

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre};
use crate::cmd_host::{afficher, afficher_diagnostic, erreur_partage, format_depuis_texte};

pub fn run(ami_designe: &str, secondes: u64, sortie: &str, format: &str) -> anyhow::Result<()> {
    // Validé avant tout : une faute de frappe coûte une seconde, pas une
    // négociation.
    let formats_imposes = formats_imposes(format)?;
    // Pris AVANT le coffre, comme au C2 : c'est d'ici que se mesurent
    // « Réponse reçue après » et « depuis le lancement de view ».
    let lancement = Instant::now();
    conscience_dpi_par_moniteur();
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
            formats_imposes,
        },
        || Ok(Some(Box::new(File::create(sortie)?) as Puits)),
        &arret,
        &mut |evenement| afficher(&lignes_spectateur(&evenement, sortie)),
    )
    .map_err(erreur_partage)?;
    afficher_fin(fin, sortie)
}

/// `auto` : rien d'imposé. Sinon le seul format nommé.
fn formats_imposes(texte: &str) -> anyhow::Result<Option<Vec<FormatVideo>>> {
    if texte == "auto" {
        return Ok(None);
    }
    Ok(Some(vec![format_depuis_texte(texte)?]))
}

/// La conscience DPI par moniteur, posée AVANT que `regarder` n'ouvre la
/// fenêtre de visionnage (jalon 2, tâche 10).
///
/// Sans elle, à 150 % ou 200 % de mise à l'échelle, Windows étire la fenêtre et
/// le texte de l'écran reçu devient flou — exactement ce que `view` sert à
/// juger. L'application la déclare dans son manifeste (`sky-app/build.rs`) ;
/// ici, un appel suffit et reste limité à `view` : les commandes de mesure du
/// jalon 0 (`capture`, `encode`…) gardent le comportement qu'on a mesuré.
///
/// Un échec n'arrête rien : la documentation de Windows le prévoit quand le
/// réglage est déjà posé, ou sur un Windows antérieur à 10 version 1703, et
/// l'image reste correcte, seulement moins nette.
fn conscience_dpi_par_moniteur() {
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_err() {
        println!("Conscience DPI non posée : l'image peut être moins nette à une mise à l'échelle de Windows.");
    }
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
        Evenement::Mesures(Mesures::Reception(m)) => vec![format!(
            // « reçues » : le compte inclut les images écartées faute d'image
            // clé, ce n'est pas une cadence d'affichage (mineur 49).
            "  {:.1} Mbps | {:.0} images reçues/s | gigue {:.2} ms | décodage {:.2} ms | {} images écartées",
            f64::from(m.debit_kbps) / 1000.0,
            m.images_par_seconde,
            m.gigue_ms,
            m.latence_decodage_ms,
            m.images_abandonnees
        )],
        Evenement::Format(f) => vec![format!("format négocié : {}", f.libelle())],
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
        // Une fin NORMALE : avant la tâche 10, elle arrivait en `LienTombe` et
        // s'imprimait « ÉCHEC ».
        Fin::PartageArrete => println!("\nTon ami a arrêté son partage. Fichier écrit : {sortie}"),
        Fin::DureeEcoulee(bilan) => {
            if let Bilan::Reception(b) = *bilan {
                afficher_bilan(&b, sortie);
            }
        }
        // `view` ne demande jamais l'arrêt par signal ; la croix de la fenêtre,
        // elle, le lève (un seul chemin d'arrêt, tâche 10). Les autres fins
        // sont celles de l'hôte.
        Fin::Arrete => println!("\nFenêtre fermée : visionnage arrêté. Fichier écrit : {sortie}"),
        Fin::AucuneDemande
        | Fin::ReponseRefusee
        | Fin::FileDePaquetisationPleine
        | Fin::AucunFormatEncodable => {}
        Fin::AucunFormatCommun => println!(
            "\nÉCHEC : aucun format vidéo en commun — la carte de ton ami n'encode aucun des \
             formats que cette machine offre (ce qu'elle décode, restreint par `--format`)."
        ),
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
    fn auto_n_impose_rien_et_un_nom_impose_ce_seul_format() {
        assert_eq!(formats_imposes("auto").unwrap(), None);
        assert_eq!(formats_imposes("H264").unwrap(), Some(vec![FormatVideo::H264]));
        assert!(formats_imposes("av1").is_err());
    }

    #[test]
    fn le_format_negocie_s_affiche_comme_chez_l_hote() {
        assert_eq!(
            lignes_spectateur(&Evenement::Format(FormatVideo::Hevc420), "recu.h265"),
            vec![format!("format négocié : {}", FormatVideo::Hevc420.libelle())]
        );
    }

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
