//! `sky-probe host` : un affichage de `sky_partage::heberger` (jalon 1,
//! tâche 5). La négociation et la diffusion vivent dans `sky-partage` ; ce
//! fichier ne garde que les textes du terminal, inchangés depuis le C2.

use std::io::Write;
use std::time::Duration;

use sky_compte::{synchroniser, ErreurCompte};
use sky_encode::Codec;
use sky_partage::hote::BUDGET_RETRY_ENVOI;
use sky_partage::rendez_vous::FENETRE_HOTE;
use sky_partage::{
    heberger, Arret, Bilan, BilanEnvoi, Diagnostic, ErreurPartage, Evenement, Fin, Images, Mesures,
    ParametresHote, SourceImages,
};
use windows::Win32::Graphics::Direct3D11::ID3D11Device;

use crate::cmd_compte::{avertissement_consommation, causes_d_un_depot_refuse, config_et_coffre, message_utilisateur};
use crate::cmd_encode::{Source, TextureSynthetique};

/// Paramètres de la chaîne complète (inchangés depuis le C2).
pub struct Parametres {
    pub secondes: u64,
    pub codec: Codec,
    pub bitrate_mbps: u32,
    pub floor_mbps: u32,
    pub monitor: usize,
    pub source: Source,
    pub largeur_synth: u32,
    pub hauteur_synth: u32,
}

impl Images for TextureSynthetique {
    fn prochaine_image(&mut self) -> anyhow::Result<sky_capture::CapturedFrame> {
        TextureSynthetique::prochaine_image(self)
    }
}

fn fabrique_synthetique(device: &ID3D11Device, largeur: u32, hauteur: u32) -> anyhow::Result<Box<dyn Images>> {
    Ok(Box::new(TextureSynthetique::new(device, largeur, hauteur)?))
}

pub fn run(p: Parametres) -> anyhow::Result<()> {
    let (config, coffre) = config_et_coffre()?;
    let source = match p.source {
        Source::Ecran => SourceImages::Ecran,
        Source::Synthetique => SourceImages::Synthetique {
            largeur: p.largeur_synth,
            hauteur: p.hauteur_synth,
            fabrique: fabrique_synthetique,
        },
    };
    let parametres = ParametresHote {
        codec: p.codec,
        plafond_mbps: p.bitrate_mbps,
        plancher_mbps: p.floor_mbps,
        moniteur: p.monitor,
        source,
        duree_max: Some(Duration::from_secs(p.secondes)),
    };
    // Jamais demandé : `host` s'arrête à `--seconds`, comme au C2.
    let arret = Arret::nouveau();
    let fin = heberger(
        &config,
        &coffre,
        |precedent| synchroniser(&config, &coffre, precedent),
        parametres,
        &arret,
        &mut |evenement| afficher(&lignes_hote(&evenement)),
    )
    .map_err(erreur_partage)?;
    afficher_fin(fin, p.floor_mbps, p.bitrate_mbps)
}

pub(crate) fn afficher(lignes: &[String]) {
    for ligne in lignes {
        println!("{ligne}");
    }
    std::io::stdout().flush().ok();
}

/// Les lignes du C2, pour chaque événement de l'hôte.
pub(crate) fn lignes_hote(evenement: &Evenement) -> Vec<String> {
    match evenement {
        // `Pret` suit la validation des bornes du Pacer, comme l'avertissement
        // au C2. `config_et_coffre`, appelé avant, ne peut pas échouer
        // (`Config::depuis_env`, `Coffre::nouveau`) : l'ordre affiché est
        // celui du C2 dans tous les cas.
        Evenement::Pret => vec![avertissement_consommation("la demande de ton ami")],
        Evenement::Disponible { fenetre } => vec![format!(
            "En attente de la demande d'un ami, pendant {} minutes au maximum...",
            fenetre.as_secs() / 60
        )],
        Evenement::EchecLocal { raison } => vec![message_echec_local(raison)],
        Evenement::DemandeEcartee { raison } => vec![format!("  Demande écartée : {raison}")],
        Evenement::DemandeRecue { apres, synchronisations, .. } => vec![format!(
            "Demande reçue après {} s ({synchronisations} synchronisations).",
            apres.as_secs()
        )],
        Evenement::Negociation => vec!["Réponse envoyée. Négociation en cours...".to_string()],
        Evenement::Connecte { en, .. } => vec![format!("CONNECTÉ en {:.1} s", en.as_secs_f32())],
        Evenement::Diffusion { largeur, hauteur, codec, plancher_mbps, plafond_mbps } => vec![format!(
            "\nRésolution {largeur}x{hauteur}, {}, plancher {plancher_mbps} Mbps, plafond {plafond_mbps} Mbps.\n",
            codec.label()
        )],
        Evenement::Mesures(Mesures::Envoi { debit_mbps, cible_mbps, images_sautees, rtt_ms }) => vec![format!(
            "  {debit_mbps:.1} Mbps envoyés | cible pacer {cible_mbps:.1} Mbps | {images_sautees} images sautées cumulées | RTT {rtt_ms:.1} ms"
        )],
        // Événements du spectateur : `heberger` ne les émet jamais.
        Evenement::DemandeEnvoyee { .. } | Evenement::ReponseRecue { .. } | Evenement::Mesures(Mesures::Reception { .. }) => {
            Vec::new()
        }
    }
}

fn afficher_fin(fin: Fin, plancher: u32, plafond: u32) -> anyhow::Result<()> {
    match fin {
        Fin::AucunAppareilLocal => anyhow::bail!(
            "aucun appareil enregistré sur cette machine — lance d'abord \
             `sky-probe device register <nom>`."
        ),
        Fin::AucuneDemande => println!(
            "Aucune demande reçue en {} minutes. Relance `sky-probe host` quand ton ami est prêt.",
            FENETRE_HOTE.as_secs() / 60
        ),
        Fin::ReponseRefusee => {
            println!("Le serveur a refusé la réponse : rien n'a été envoyé.");
            println!("{}", causes_d_un_depot_refuse());
        }
        Fin::NegociationRompue(raison) => println!("ÉCHEC : {raison}"),
        Fin::EtablissementEchoue(diagnostic) => afficher_diagnostic(&diagnostic),
        Fin::LienTombe(raison) => println!("\nÉCHEC : {raison}"),
        Fin::TamponSature { morceau, morceaux } => println!(
            "\nÉCHEC : tampon d'émission saturé plus de {} ms \
             (morceau {morceau}/{morceaux}) — arrêt pour ne pas \
             produire un flux corrompu.",
            BUDGET_RETRY_ENVOI.as_millis(),
        ),
        Fin::DureeEcoulee(bilan) => {
            if let Bilan::Envoi(b) = *bilan {
                afficher_bilan(&b, plancher, plafond);
            }
        }
        // `host` ne demande jamais l'arrêt ; les autres fins sont celles du spectateur.
        Fin::Arrete | Fin::AucunAppareilChezLAmi { .. } | Fin::DemandeRefusee { .. } | Fin::PasDeReponse { .. } => {}
    }
    Ok(())
}

/// Le résumé de fin de `run` au C2, lu dans le bilan de `heberger`.
fn afficher_bilan(b: &BilanEnvoi, plancher: u32, plafond: u32) {
    println!("\n--- Résumé de la chaîne complète (Q4) ---");
    println!("Durée              : {:.1} s", b.duree_s);
    println!("Images encodées    : {}", b.images_encodees);
    println!("Images sautées     : {} (régulation du débit)", b.images_sautees);
    if let Some(q) = &b.encodage_ms {
        println!("Encodage médian/p99: {:.2} ms / {:.2} ms  ({} échantillons)", q.p50, q.p99, q.echantillons);
    }
    println!(
        "Débit soutenu      : {:.1} Mbps ({} Mo envoyés)",
        b.envoyes_octets as f64 * 8.0 / b.duree_s / 1e6,
        b.envoyes_octets / 1_000_000
    );
    println!(
        "Cible finale pacer : {:.1} Mbps (plancher {}, plafond {})",
        b.cible_finale_bps as f64 / 1e6,
        plancher,
        plafond
    );
    println!("Retours reçus      : {}", b.retours);
    println!("Échecs d'envoi     : {} / {}", b.echecs_envoi, b.tentatives_envoi);
    match &b.rtt_ms {
        None => println!("RTT                : non mesuré (aucun retour reçu)"),
        Some(q) => println!(
            "RTT médian / p99   : {:.2} ms / {:.2} ms  ({} échantillons)",
            q.p50, q.p99, q.echantillons
        ),
    }
    // Ce rappel s'affichait systematiquement, y compris pendant un vrai test
    // entre deux machines. Il faisait passer des mesures reseau reelles pour des
    // mesures en memoire — exactement le genre d'affirmation non verifiee qui
    // fait chercher au mauvais endroit. On regarde desormais si le pair a ete
    // joint par une adresse publique.
    if b.vers_internet == 0 {
        println!("Rappel : aucun paquet n'est parti vers internet — lien local.");
        println!("Ce debit et ce RTT mesurent le chiffrement et le transport en");
        println!("memoire, pas un reseau.");
    } else {
        println!("Lien reseau reel : {} paquets emis vers internet.", b.vers_internet);
        println!("Ce debit et ce RTT sont ceux d'une vraie liaison entre deux machines.");
    }
}

/// Message affiché quand `repondant` échoue pour une cause LOCALE (réseau, port
/// UDP) — l'offre de l'ami, elle, était valide.
///
/// Il ne promet AUCUNE reprise (revue finale, I2) : le serveur a effacé l'offre
/// en la livrant, `interroger` ne présente chaque état qu'une fois, et la
/// synchronisation suivante rend des enveloppes vidées. L'ancien message
/// annonçait « nouvelle tentative au prochain sondage » : les deux côtés
/// attendaient alors pour rien, et l'ami lisait « n'a pas répondu ».
pub(crate) fn message_echec_local(message: &str) -> String {
    format!(
        "  Échec local ({message}) : cette demande est perdue, le serveur l'a déjà \
         livrée. Demande à ton ami de relancer `sky-probe view`."
    )
}

/// Une erreur de compte, rédigée pour l'utilisateur : `Refuse` reçoit le
/// message unique de `cmd_compte`, les autres leur `Display` déjà expurgé.
/// Partagée avec `cmd_view`.
pub(crate) fn erreur_compte(erreur: ErreurCompte) -> anyhow::Error {
    anyhow::anyhow!(message_utilisateur(&erreur))
}

/// `ErreurPartage` → l'erreur que `host`/`view` rendaient au C2.
pub(crate) fn erreur_partage(e: ErreurPartage) -> anyhow::Error {
    match e {
        ErreurPartage::Compte(c) => erreur_compte(c),
        ErreurPartage::Autre(a) => a,
    }
}

/// Explique l'échec sans accuser le NAT à tort.
///
/// `etablir` attend l'ouverture du canal de données, qui vient bien après ICE :
/// un échec peut donc venir du perçage de NAT, ou de la poignée de main chiffrée
/// qui le suit. Ce sont deux verdicts opposés pour la question centrale du
/// jalon, et c'est ce message qui sera consigné comme réponse. Il doit donc
/// distinguer les deux, et dire quand il n'est pas sûr de lui.
pub(crate) fn afficher_diagnostic(d: &Diagnostic) {
    if d.ice_connecte {
        println!(
            "ÉCHEC : le canal de données ne s'est pas ouvert en {} s.",
            d.delai.as_secs()
        );
        println!("ATTENTION : la traversée de NAT n'est PAS en cause. Les deux machines");
        println!("se sont bel et bien trouvées — c'est la poignée de main chiffrée");
        println!("(DTLS/SCTP) qui n'a pas abouti. À ne pas compter comme un échec Q5.");
    } else {
        let (emis, recus, erreurs) = (d.emis, d.recus, d.erreurs);
        println!(
            "ÉCHEC : aucune connexion directe en {} s.",
            d.delai.as_secs()
        );
        println!();
        println!("  Datagrammes émis   : {emis}");
        println!("  Datagrammes reçus  : {recus}");
        println!("  Erreurs de socket  : {erreurs}");
        let (prive, public) = (d.vers_local, d.vers_internet);
        println!("  dont vers reseau local : {prive}");
        println!("  dont vers internet     : {public}");
        println!();

        // Ces trois nombres distinguent des causes que « NAT strict » confondait.
        if emis == 0 {
            println!("Aucun paquet n'a été émis : l'agent ICE n'a pas de destination.");
            println!("Le bloc reçu ne contenait donc aucune adresse exploitable.");
            println!("C'est un défaut de notre côté, pas un problème de réseau.");
        } else if recus == 0 {
            println!("Nous avons émis sans jamais rien recevoir en retour.");
            println!("Trois causes, et rien d'ici ne permet de trancher :");
            println!("  - le correspondant n'avait plus son programme ouvert ;");
            println!("  - il ne l'a pas lancé au même moment que nous ;");
            println!("  - ses paquets sortent mais les nôtres n'arrivent pas jusqu'à lui.");
            println!();
            println!("Si vous avez tous les deux obtenu « réseau compatible » avec");
            println!("`sky-probe netcheck`, la première cause est de loin la plus probable :");
            println!("les deux programmes doivent rester ouverts jusqu'à l'établissement.");
        } else {
            println!("Des paquets ont circulé DANS LES DEUX SENS, sans que la négociation");
            println!("aboutisse. Le réseau fait son travail : la traversée de NAT n'est pas");
            println!("en cause. Le défaut est dans notre code ou dans la négociation ICE.");
        }
    }

    let erreurs = d.erreurs_socket;
    if erreurs > 0 {
        println!();
        println!("Réserve : {erreurs} erreur(s) sur le port UDP local pendant la tentative.");
        println!("Une cause locale (pare-feu, interface qui change) n'est pas exclue :");
        println!("le diagnostic ci-dessus est à prendre avec précaution.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_echec_local_ne_promet_aucune_reprise_et_renvoie_a_view() {
        // Échoue si le message reprenait une promesse de reprise : rien n'est
        // retenté, et c'est l'ami qui doit relancer `view`.
        let ligne = message_echec_local("port UDP indisponible");
        assert!(ligne.contains("port UDP indisponible"), "la cause observée doit être citée");
        assert!(ligne.contains("perdue"));
        assert!(ligne.contains("relancer `sky-probe view`"));
        let minuscule = ligne.to_lowercase();
        for promesse in ["nouvelle tentative", "prochain sondage", "retent", "réessa"] {
            assert!(!minuscule.contains(promesse), "le message promet une reprise : « {promesse} »");
        }
    }

    #[test]
    fn la_ligne_de_mesure_de_l_hote_est_celle_du_c2() {
        let lignes = lignes_hote(&Evenement::Mesures(Mesures::Envoi {
            debit_mbps: 12.34,
            cible_mbps: 20.0,
            images_sautees: 3,
            rtt_ms: 85.24,
        }));
        assert_eq!(
            lignes,
            vec!["  12.3 Mbps envoyés | cible pacer 20.0 Mbps | 3 images sautées cumulées | RTT 85.2 ms".to_string()]
        );
    }

    #[test]
    fn la_demande_recue_affiche_des_secondes_entieres() {
        // C2 : `debut_attente.elapsed().as_secs()`, pas une décimale.
        let lignes = lignes_hote(&Evenement::DemandeRecue {
            expediteur_device_id: 4,
            apres: Duration::from_millis(12_900),
            synchronisations: 7,
        });
        assert_eq!(lignes, vec!["Demande reçue après 12 s (7 synchronisations).".to_string()]);
    }
}
