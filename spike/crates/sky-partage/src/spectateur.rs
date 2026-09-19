//! Côté spectateur : demander le partage d'un ami, intégrer sa réponse, puis
//! recevoir et MESURER le flux. Déplacé de `sky-probe/src/cmd_view.rs` (C2).
//! Le flux est JETÉ par défaut (spec D2) : seul un appelant qui fournit un
//! puits (`sky-probe view` et son fichier) en garde les octets.

use std::io::Write;
use std::time::{Duration, Instant};

use sky_compte::{deposer, relever, resoudre_ami, Ami, Coffre, Config, ErreurCompte, Etat};
use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

use crate::arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable};
use crate::etablissement::{etablir, Etablissement};
use crate::evenement::{Bilan, BilanReception, ErreurPartage, Evenement, Fin, Mesures};
use crate::hote::epoch_us;
use crate::reception::Reception;
use crate::rendez_vous::{interroger, reponse_a_l_offre, session_de, ATTENTE_SPECTATEUR, CADENCE};

/// Période d'émission du retour vers l'émetteur.
///
/// Le retour ne prouve plus seulement que le canal fonctionne dans les deux
/// sens (Tâche 7) : il porte désormais l'horodatage du dernier paquet vidéo
/// reçu, ce qui permet à l'émetteur de calculer un aller-retour (RTT) réel —
/// c'est ce dont le `Pacer` de la Tâche 8 se nourrit.
const PERIODE_RETOUR: Duration = Duration::from_millis(200);

/// Où écrire le flux reçu, si quelqu'un le veut.
pub type Puits = Box<dyn Write>;

/// Comment l'appelant désigne l'ami à regarder.
pub enum Designation<'a> {
    /// Nom Discord exact ou identifiant écrit en texte (`sky-probe view`) :
    /// passe par `resoudre_ami`, qui refuse toute ambiguïté.
    Texte(&'a str),
    /// Identifiant d'utilisateur (l'application) : jamais confondu avec un nom.
    Identifiant(i64),
}

pub fn trouver_ami<'e>(etat: &'e Etat, designation: &Designation<'_>) -> Result<&'e Ami, ErreurCompte> {
    match designation {
        Designation::Texte(texte) => resoudre_ami(etat, texte),
        Designation::Identifiant(id) => etat
            .amis
            .iter()
            .find(|ami| ami.id == *id)
            .ok_or_else(|| ErreurCompte::Protocole(format!("aucun ami d'identifiant {id}"))),
    }
}

pub struct ParametresSpectateur<'a> {
    pub ami: Designation<'a>,
    /// `None` : jusqu'à l'arrêt (application). `Some` : `sky-probe --seconds`.
    pub duree_max: Option<Duration>,
    /// D'où se mesurent « réponse reçue après » et « depuis le lancement » :
    /// `sky-probe view` le prend AVANT de lire le coffre, comme au C2 (spec
    /// §8, « exactement comme à l'essai réel »).
    pub lancement: Instant,
}

pub fn regarder(
    config: &Config,
    coffre: &Coffre,
    mut synchroniser: impl FnMut(Option<&Etat>) -> Result<Etat, ErreurCompte>,
    p: ParametresSpectateur<'_>,
    ouvrir_puits: impl FnOnce() -> anyhow::Result<Option<Puits>>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let lancement = p.lancement;
    // Avant tout réseau : sans appareil, `deposer` refuserait de toute façon.
    if coffre.identifiant_appareil().map_err(ErreurPartage::Compte)?.is_none() {
        return Ok(Fin::AucunAppareilLocal);
    }
    // L'identité DURABLE : c'est pour sa clé d'annuaire que l'hôte scelle
    // l'enveloppe de sa réponse. La clé de l'offre, elle, est éphémère.
    let identite = coffre.identite().map_err(ErreurPartage::Compte)?;

    let etat = synchroniser(None).map_err(ErreurPartage::Compte)?;
    let ami = trouver_ami(&etat, &p.ami).map_err(ErreurPartage::Compte)?.clone();
    if ami.appareils.is_empty() {
        return Ok(Fin::AucunAppareilChezLAmi { nom: ami.discord_name });
    }

    let (mut link, offre) = PeerLink::offrant(Identity::generate())?;
    let session = session_de(&offre)?;
    let deposes = deposer(config, coffre, &ami.appareils, offre.as_bytes()).map_err(ErreurPartage::Compte)?;
    if deposes == 0 {
        return Ok(Fin::DemandeRefusee { nom: ami.discord_name, appareils: ami.appareils.len() });
    }
    evenements(Evenement::DemandeEnvoyee {
        nom: ami.discord_name.clone(),
        deposes,
        appareils: ami.appareils.len(),
        attente: ATTENTE_SPECTATEUR,
    });

    // Le mapping NAT du port annoncé dans l'offre doit survivre à l'attente.
    let garde = link.maintenir_mapping()?;
    let mut synchronisations = 1u32; // celle qui a résolu l'ami
    let mut horloge = HorlogeArretable::demarrer(arret);
    let attente = interroger(
        Some(etat),
        synchroniser_sauf_arret(arret, |precedent| {
            synchronisations += 1;
            synchroniser(precedent)
        }),
        |etat| reponse_a_l_offre(relever(etat, &identite), session, &ami.appareils),
        &mut horloge,
        CADENCE,
        ATTENTE_SPECTATEUR,
    );
    let reponse = match attente {
        Ok(Some(reponse)) => reponse,
        Ok(None) => return Ok(Fin::PasDeReponse { nom: ami.discord_name }),
        Err(ErreurAttente::Arrete) => return Ok(Fin::Arrete),
        Err(ErreurAttente::Compte(e)) => return Err(ErreurPartage::Compte(e)),
    };
    evenements(Evenement::ReponseRecue { apres: lancement.elapsed(), synchronisations });

    // La négociation produit désormais son propre trafic.
    drop(garde);
    link.accepter_reponse(&reponse)?;
    evenements(Evenement::Negociation);

    let duree = match etablir(&mut link, arret)? {
        Etablissement::Ouvert(duree) => duree,
        Etablissement::Rompu(raison) => return Ok(Fin::NegociationRompue(raison)),
        Etablissement::Delai(diagnostic) => return Ok(Fin::EtablissementEchoue(diagnostic)),
        Etablissement::Arrete => return Ok(Fin::Arrete),
    };
    evenements(Evenement::Connecte { en: duree, depuis_le_lancement: Some(lancement.elapsed()) });

    // Ouvert APRÈS la connexion, comme le fichier de `view` au C2 : jamais de
    // fichier vide laissé par une négociation ratée.
    let puits = ouvrir_puits()?;
    recevoir(&mut link, puits, p.duree_max, arret, evenements)
}

/// La boucle de réception de `cmd_view` (C2), l'écriture du fichier devenue
/// optionnelle. Les en-têtes de séquence n'étant émis qu'une fois (GOP
/// infini), le puits reçoit tout depuis le tout premier paquet.
fn recevoir(
    link: &mut PeerLink,
    mut puits: Option<Puits>,
    duree_max: Option<Duration>,
    arret: &Arret,
    evenements: &mut dyn FnMut(Evenement),
) -> Result<Fin, ErreurPartage> {
    let mut reception = Reception::default();
    let t0 = Instant::now();
    let mut dernier_affichage = Instant::now();
    let mut dernier_retour = Instant::now();
    let mut octets_precedent = 0u64;
    let mut images_precedent = 0u64;

    loop {
        if arret.est_demande() {
            vider(&mut puits);
            return Ok(Fin::Arrete);
        }
        if duree_max.is_some_and(|d| t0.elapsed() >= d) {
            break;
        }
        match link.poll()? {
            LinkEvent::Data(d) => {
                if let Some(charge) = reception.absorber(&d, epoch_us()) {
                    if let Some(p) = puits.as_mut() {
                        p.write_all(charge).map_err(anyhow::Error::from)?;
                    }
                }
            }
            LinkEvent::Failed(raison) => {
                vider(&mut puits);
                return Ok(Fin::LienTombe(raison));
            }
            _ => {}
        }

        if dernier_retour.elapsed() >= PERIODE_RETOUR {
            if let Some(h) = reception.dernier_horodatage_emission() {
                let _ = link.send(&h.to_le_bytes());
            }
            dernier_retour = Instant::now();
        }

        if dernier_affichage.elapsed() >= Duration::from_secs(1) {
            let ecoule = dernier_affichage.elapsed().as_secs_f64();
            evenements(Evenement::Mesures(Mesures::Reception {
                debit_mbps: (reception.octets() - octets_precedent) as f64 * 8.0 / ecoule / 1e6,
                images_par_s: reception.images() - images_precedent,
                gigue_ms: reception.gigue_ms(),
            }));
            octets_precedent = reception.octets();
            images_precedent = reception.images();
            dernier_affichage = Instant::now();
        }
    }

    if let Some(p) = puits.as_mut() {
        p.flush().map_err(anyhow::Error::from)?;
    }
    let (_, vers_internet) = link.destinations();
    Ok(Fin::DureeEcoulee(Box::new(Bilan::Reception(BilanReception {
        duree_s: t0.elapsed().as_secs_f64(),
        images: reception.images(),
        octets: reception.octets(),
        transit_ms: reception.transit_ms(),
        gigue_ms: reception.gigue_ms(),
        vers_internet,
    }))))
}

fn vider(puits: &mut Option<Puits>) {
    if let Some(p) = puits.as_mut() {
        p.flush().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::Ami;

    fn ami(id: i64, nom: &str) -> Ami {
        Ami { id, friendship_id: id, discord_name: nom.to_string(), appareils: Vec::new() }
    }

    #[test]
    fn un_identifiant_ne_se_confond_jamais_avec_un_nom() {
        // L'application désigne l'ami par son identifiant. `resoudre_ami`
        // refuserait ici l'ambiguïté (un ami NOMMÉ « 7 », un autre
        // D'IDENTIFIANT 7). Neutralisation : passer `Identifiant` par
        // `resoudre_ami(etat, &id.to_string())` — erreur, le test rougit.
        let etat = Etat {
            version: 1,
            code: "ABCDEFGH".to_string(),
            amis: vec![ami(3, "7"), ami(7, "bob")],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        };
        assert_eq!(trouver_ami(&etat, &Designation::Identifiant(7)).unwrap().discord_name, "bob");
        assert!(trouver_ami(&etat, &Designation::Texte("7")).is_err());
        assert!(trouver_ami(&etat, &Designation::Identifiant(99)).is_err());
    }
}
