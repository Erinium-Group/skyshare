//! Le partage dans l'application (spec §3, §4) : `sky-partage` branché sur la
//! synchronisation du `Noyau` — la SEULE de l'application —, ses événements
//! traduits en `PartageVue`, ses fins en causes affichables.
//!
//! AUCUN CHAMP SENSIBLE NE TRAVERSE CE MODULE. Les `Evenement` de `sky-partage`
//! ne portent déjà ni adresse, ni SDP, ni clé (`evenement.rs` le dit en
//! en-tête) ; `appliquer` n'en retient qu'un sous-ensemble — nom Discord, durées
//! et mesures — et `fin_vue` ne recopie de `Diagnostic` que le fait binaire
//! « ICE a-t-il trouvé un chemin ». Les compteurs `vers_local` / `vers_internet`
//! du diagnostic ne sont PAS repris : ils sont un indice de topologie réseau, et
//! la promesse du projet est qu'aucune adresse n'est journalisée NI affichée.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sky_compte::{ErreurCompte, Etat};
use sky_partage::rendez_vous::FENETRE_HOTE;
use sky_partage::{
    Arret, Designation, ErreurDecodeur, ErreurPartage, ErreurVisionnage, Evenement, Fin, FormatVideo,
    Mesures, ParametresHote, ParametresSpectateur, SourceImages,
};

use crate::noyau::Noyau;
use crate::vue::{FinVue, PartageVue};

/// Plafond et plancher du débit : les valeurs par défaut de `sky-probe host`,
/// celles de l'essai réel du C2.
pub const PLAFOND_MBPS: u32 = 30;
pub const PLANCHER_MBPS: u32 = 10;

pub trait Partageur: Send + Sync {
    fn heberger(
        &self,
        noyau: &Noyau,
        formats: Vec<FormatVideo>,
        ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage>;

    /// `lancement` : l'instant du CLIC, pris par le `Noyau` avant tout appel.
    /// C'est de lui que `sky-partage` mesure « connecté en X s depuis le
    /// lancement » (`ParametresSpectateur::lancement`, tâche 5) ; le prendre ici
    /// ferait partir la mesure après l'exigence de session et la réservation.
    fn regarder(
        &self,
        noyau: &Noyau,
        ami: i64,
        lancement: Instant,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage>;
}

/// `sky-partage` pour de vrai. La fermeture de synchronisation ignore le
/// précédent que lui passe `heberger` : le `Noyau` tient le sien, le même.
pub struct PartageurReel;

impl Partageur for PartageurReel {
    fn heberger(
        &self,
        noyau: &Noyau,
        formats: Vec<FormatVideo>,
        ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        sky_partage::heberger(
            noyau.config(),
            noyau.coffre(),
            |_| noyau.synchroniser(),
            ParametresHote {
                formats,
                plafond_mbps: PLAFOND_MBPS,
                plancher_mbps: PLANCHER_MBPS,
                moniteur: ecran,
                source: SourceImages::Ecran,
                duree_max: None,
            },
            arret,
            evenements,
        )
    }

    fn regarder(
        &self,
        noyau: &Noyau,
        ami: i64,
        lancement: Instant,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        // Aucun puits : le flux est affiché, jamais ENREGISTRÉ (spec D2). L'écriture dans
        // un fichier reste une option de `sky-probe view`, pas de l'application.
        sky_partage::regarder(
            noyau.config(),
            noyau.coffre(),
            |_| noyau.synchroniser(),
            ParametresSpectateur { ami: Designation::Identifiant(ami), duree_max: None, lancement },
            || Ok(None),
            arret,
            evenements,
        )
    }
}

/// L'horloge murale, en millisecondes : l'interface l'affiche en durées
/// écoulées, et `Instant` ne se sérialise pas.
pub fn maintenant_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// La cause affichée d'une fin de partage (spec §4, « Échecs, tous en clair »).
///
/// `ReseauBloque` n'est rendu que si ICE n'a trouvé AUCUN chemin. Son texte
/// (« Aucune connexion directe n'a pu s'établir entre vos deux réseaux »)
/// énonce ce qui a été constaté, pas une cause supposée — arbitrage du
/// contrôleur contre l'ancien « Ton réseau bloque la connexion directe », qui
/// accusait un réseau sans l'avoir mesuré. Même prudence que le diagnostic du C2.
pub fn fin_vue(issue: &Result<Fin, ErreurPartage>) -> FinVue {
    match issue {
        Ok(Fin::Arrete) | Ok(Fin::DureeEcoulee(_)) => FinVue::Arrete,
        Ok(Fin::PasDeReponse { nom }) => FinVue::PasEnPartage { ami: nom.clone() },
        Ok(Fin::EtablissementEchoue(diagnostic)) if !diagnostic.ice_connecte => FinVue::ReseauBloque,
        Ok(Fin::EtablissementEchoue(_)) => FinVue::Autre {
            message: "Les deux machines se sont trouvées, mais la négociation chiffrée n'a pas \
                      abouti."
                .to_string(),
        },
        Ok(Fin::FileDePaquetisationPleine) => FinVue::EnvoiEnRetard,
        Ok(Fin::PartageArrete) => FinVue::PartageArrete,
        // La durée affichée vient de la constante du cœur, jamais d'un littéral
        // de l'interface (revue finale, M3) : `FENETRE_HOTE` est ce que `hote`
        // a réellement attendu avant de rendre `AucuneDemande`.
        Ok(Fin::AucuneDemande) => FinVue::AucuneDemande { fenetre_s: FENETRE_HOTE.as_secs() },
        // `Noyau::partager` refuse déjà ce cas avant tout démarrage ; cette
        // branche ne sert que si ce refus disparaissait un jour — et elle dit
        // alors la même chose.
        Ok(Fin::AucunFormatEncodable) => {
            FinVue::Autre { message: crate::noyau::MESSAGE_AUCUN_FORMAT.to_string() }
        }
        Ok(Fin::AucunFormatCommun) => {
            FinVue::Autre { message: crate::noyau::MESSAGE_AUCUN_FORMAT_COMMUN.to_string() }
        }
        Ok(Fin::AucunAppareilLocal) => FinVue::Autre {
            message: "Cette machine n'a pas d'appareil enregistré : reconnecte-toi.".to_string(),
        },
        Ok(Fin::AucunAppareilChezLAmi { nom }) => {
            FinVue::Autre { message: format!("{nom} n'a aucun appareil enregistré.") }
        }
        Ok(Fin::DemandeRefusee { nom, .. }) => {
            FinVue::Autre { message: format!("Le site a refusé la demande pour {nom}.") }
        }
        Ok(Fin::ReponseRefusee) => FinVue::Autre {
            message: "Le site a refusé la réponse : ton ami ne l'a pas reçue.".to_string(),
        },
        Ok(Fin::NegociationRompue(raison)) | Ok(Fin::LienTombe(raison)) => {
            FinVue::Autre { message: format!("Connexion interrompue : {raison}") }
        }
        Err(ErreurPartage::Compte(ErreurCompte::Refuse)) => FinVue::SessionExpiree,
        Err(ErreurPartage::Compte(e)) => FinVue::Autre { message: crate::noyau::message_erreur(e) },
        Err(ErreurPartage::Visionnage(v)) => fin_de_visionnage(v),
        Err(ErreurPartage::Autre(e)) => FinVue::Autre { message: e.to_string() },
    }
}

/// La cause affichée d'un visionnage qui n'a pas pu décoder (spec §7).
///
/// Branchée sur la VARIANTE, jamais sur le texte : le détail
/// d'`AucuneCarteNvidia` vient de `libloading`, et Windows peut le rendre dans
/// sa langue — un branchement sur ce texte passerait ici et casserait ailleurs.
/// Les deux `match` sont exhaustifs, sans joker : une variante ajoutée à
/// `ErreurDecodeur` ou à `ErreurVisionnage` ne compile pas tant qu'elle n'a pas
/// sa cause.
///
/// LE MOMENT COMPTE AUTANT QUE LA VARIANTE (ronde de correction 1). Les
/// messages de la spec §7 décrivent l'OUVERTURE : « n'a pas pu démarrer »,
/// « cette carte ne prend pas en charge… ». Levée après une image affichée, la
/// même `ErreurDecodeur` les rendrait faux. Le flux a donc son propre message,
/// qui ne désigne aucune cause. Depuis la vague finale (I3), c'est `sky-partage`
/// qui tranche le moment, sur l'image AFFICHÉE et non sur le lieu de l'appel :
/// une session refusée au premier paquet arrive ici en `Ouverture`.
fn fin_de_visionnage(erreur: &ErreurVisionnage) -> FinVue {
    match erreur {
        ErreurVisionnage::ImageIrreconstituable => FinVue::ImageIrreconstituable,
        // La dernière erreur du décodeur n'est PAS lue : à ce stade, elle ne
        // dit rien que l'utilisateur puisse lever.
        ErreurVisionnage::DecodageInterrompu(_) => FinVue::DecodageInterrompu,
        ErreurVisionnage::Ouverture(decodeur) => match decodeur {
            ErreurDecodeur::AucuneCarteNvidia(_) => FinVue::SansCarteNvidia,
            ErreurDecodeur::QuatreQuatreQuatreNonPris => FinVue::SansDecodage444,
            // Deux causes, un message : ce qu'il dit est vrai des deux — le
            // décodeur n'a pas démarré, et une autre application qui occupe la
            // carte est la cause plausible que l'utilisateur peut lever.
            ErreurDecodeur::SessionRefusee(_) | ErreurDecodeur::ContexteCuda(_) => {
                FinVue::DecodeurRefuse
            }
            // Aucun moteur matériel pour aucun format : la machine ne peut pas
            // recevoir, et le pilote est la seule piste que l'utilisateur tient.
            ErreurDecodeur::AucunDecodeur => FinVue::Autre {
                message: crate::noyau::MESSAGE_AUCUN_DECODEUR.to_string(),
            },
            // Un décodeur existe mais n'a pas démarré : même message honnête que
            // `SessionRefusee`. Le détail (HRESULT, texte localisé) ne traverse pas.
            ErreurDecodeur::MediaFoundation(_) => FinVue::DecodeurRefuse,
            ErreurDecodeur::ResolutionTropGrande { largeur, hauteur, maximum } => {
                FinVue::ResolutionTropGrande {
                    largeur: *largeur,
                    hauteur: *hauteur,
                    largeur_max: maximum.0,
                    hauteur_max: maximum.1,
                }
            }
        },
    }
}

/// Le partage affiché après un événement, ou `None` s'il ne change rien.
///
/// Les événements qui ne changent rien à l'écran (`Pret`, `Negociation`,
/// `DemandeEcartee`, `EchecLocal`, `ReponseRecue`, `Format`, `Diffusion`) rendent `None` :
/// pas de publication, donc pas de réveil de la boucle pour rien.
pub fn appliquer(
    actuel: &PartageVue,
    evenement: &Evenement,
    maintenant_ms: u64,
    etat: Option<&Etat>,
) -> Option<PartageVue> {
    match (actuel, evenement) {
        (PartageVue::Disponible { ecran, .. }, Evenement::Disponible { fenetre }) => {
            Some(PartageVue::Disponible {
                debut_ms: maintenant_ms,
                fenetre_s: fenetre.as_secs(),
                ecran: *ecran,
            })
        }
        (PartageVue::Disponible { ecran, .. }, Evenement::DemandeRecue { expediteur_device_id, .. }) => {
            // « Le panneau central montre qui regarde » (spec §4) : l'appareil
            // expéditeur est rattaché à son propriétaire par l'état déjà en
            // mémoire. `None` si l'ami n'y est pas encore — jamais un
            // identifiant brut à l'écran.
            let spectateur = etat
                .and_then(|e| {
                    e.amis
                        .iter()
                        .find(|a| a.appareils.iter().any(|ap| ap.id == *expediteur_device_id))
                })
                .map(|a| a.discord_name.clone());
            Some(PartageVue::Diffuse {
                spectateur,
                depuis_ms: maintenant_ms,
                debit_mbps: 0.0,
                rtt_ms: None,
                ecran: *ecran,
            })
        }
        (PartageVue::Diffuse { spectateur, ecran, .. }, Evenement::Connecte { .. }) => {
            Some(PartageVue::Diffuse {
                spectateur: spectateur.clone(),
                depuis_ms: maintenant_ms,
                debit_mbps: 0.0,
                rtt_ms: None,
                ecran: *ecran,
            })
        }
        (
            PartageVue::Diffuse { spectateur, depuis_ms, ecran, .. },
            Evenement::Mesures(Mesures::Envoi { debit_mbps, rtt_ms, .. }),
        ) => Some(PartageVue::Diffuse {
            spectateur: spectateur.clone(),
            depuis_ms: *depuis_ms,
            debit_mbps: *debit_mbps,
            rtt_ms: *rtt_ms,
            ecran: *ecran,
        }),
        (PartageVue::Demande { ami, .. }, Evenement::DemandeEnvoyee { .. }) => {
            Some(PartageVue::Demande { ami: ami.clone(), debut_ms: maintenant_ms })
        }
        (PartageVue::Demande { ami, .. }, Evenement::Connecte { en, .. }) => {
            Some(PartageVue::Regarde {
                ami: ami.clone(),
                connecte_en_s: en.as_secs_f64(),
                debit_mbps: 0.0,
                images_par_s: 0,
                gigue_ms: 0.0,
                latence_decodage_ms: 0.0,
                images_abandonnees: 0,
                depuis_ms: maintenant_ms,
            })
        }
        (
            PartageVue::Regarde { ami, connecte_en_s, depuis_ms, .. },
            Evenement::Mesures(Mesures::Reception(m)),
        ) => Some(PartageVue::Regarde {
            ami: ami.clone(),
            connecte_en_s: *connecte_en_s,
            debit_mbps: f64::from(m.debit_kbps) / 1000.0,
            // Un nombre d'images, pas une mesure à décimales : l'interface
            // l'affiche en entier (`PanneauPartage`).
            images_par_s: m.images_par_seconde.round() as u64,
            gigue_ms: f64::from(m.gigue_ms),
            latence_decodage_ms: f64::from(m.latence_decodage_ms),
            images_abandonnees: m.images_abandonnees,
            depuis_ms: *depuis_ms,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::{Ami, AppareilDAmi};
    use sky_partage::{Diagnostic, MesuresVisionnage};
    use std::time::Duration;

    fn diagnostic(ice_connecte: bool) -> Diagnostic {
        Diagnostic {
            ice_connecte,
            emis: 10,
            recus: 0,
            erreurs: 0,
            vers_local: 0,
            vers_internet: 10,
            erreurs_socket: 0,
            delai: Duration::from_secs(25),
        }
    }

    #[test]
    fn chaque_echec_de_la_spec_a_sa_cause() {
        // Spec §4, « Échecs, tous en clair ». Neutralisations : faire rendre
        // `Autre` à chacune des quatre branches, une à la fois.
        assert_eq!(
            fin_vue(&Ok(Fin::PasDeReponse { nom: "Bob".into() })),
            FinVue::PasEnPartage { ami: "Bob".into() }
        );
        assert_eq!(fin_vue(&Ok(Fin::EtablissementEchoue(diagnostic(false)))), FinVue::ReseauBloque);
        assert_eq!(fin_vue(&Ok(Fin::FileDePaquetisationPleine)), FinVue::EnvoiEnRetard);
        assert_eq!(
            fin_vue(&Err(ErreurPartage::Compte(ErreurCompte::Refuse))),
            FinVue::SessionExpiree
        );
        // REVUE FINALE, M4 : la fenêtre affichée est la CONSTANTE du cœur, pas
        // un littéral. Comparée à `FENETRE_HOTE` et non à 1800 : écrire le
        // nombre ici recopierait le défaut d'un cran plus haut.
        assert_eq!(
            fin_vue(&Ok(Fin::AucuneDemande)),
            FinVue::AucuneDemande { fenetre_s: FENETRE_HOTE.as_secs() }
        );
    }

    #[test]
    fn les_fins_de_format_disent_chacune_leur_cause() {
        // Neutralisation : intervertir les deux messages — l'un des deux
        // `assert_eq!` rougit.
        assert_eq!(
            fin_vue(&Ok(Fin::AucunFormatEncodable)),
            FinVue::Autre { message: crate::noyau::MESSAGE_AUCUN_FORMAT.to_string() }
        );
        assert_eq!(
            fin_vue(&Ok(Fin::AucunFormatCommun)),
            FinVue::Autre { message: crate::noyau::MESSAGE_AUCUN_FORMAT_COMMUN.to_string() }
        );
    }

    #[test]
    fn une_negociation_chiffree_ratee_n_accuse_pas_le_reseau() {
        // ICE a abouti : le réseau n'est pas en cause (diagnostic du C2).
        // Neutralisation : ignorer `ice_connecte` dans `fin_vue`.
        assert!(matches!(
            fin_vue(&Ok(Fin::EtablissementEchoue(diagnostic(true)))),
            FinVue::Autre { .. }
        ));
    }

    /// Le diagnostic porte des compteurs de topologie (`vers_local`,
    /// `vers_internet`) : ils disent d'où sont partis les paquets. Rien de tout
    /// cela ne doit traverser la frontière vers l'interface (contrainte dure du
    /// projet, arbitrage 2 du contrôleur).
    ///
    /// Neutralisation : faire porter les compteurs au message de la branche
    /// `EtablissementEchoue` avec ICE connecté — ce test rougit.
    #[test]
    fn aucune_fin_ne_recopie_les_compteurs_du_diagnostic() {
        let repere = Diagnostic {
            ice_connecte: true,
            emis: 424_242,
            recus: 313_131,
            erreurs: 0,
            vers_local: 989_898,
            vers_internet: 777_777,
            erreurs_socket: 0,
            delai: Duration::from_secs(25),
        };
        let FinVue::Autre { message } = fin_vue(&Ok(Fin::EtablissementEchoue(repere))) else {
            panic!("ICE connecté : la cause n'est pas le réseau");
        };
        for compteur in ["424242", "313131", "989898", "777777"] {
            assert!(
                !message.contains(compteur),
                "le message montré à l'interface porte un compteur du diagnostic : {message}"
            );
        }
    }

    fn etat_avec_bob() -> Etat {
        Etat {
            version: 1,
            code: "ABCD2345".into(),
            amis: vec![Ami {
                id: 2,
                friendship_id: 12,
                discord_name: "Bob".into(),
                appareils: vec![AppareilDAmi { id: 42, public_key: String::new() }],
            }],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        }
    }

    #[test]
    fn la_demande_recue_nomme_l_ami_proprietaire_de_l_appareil() {
        // « Le panneau central montre qui regarde » (spec §4).
        // Neutralisation : `spectateur: None`.
        let actuel = PartageVue::Disponible { debut_ms: 0, fenetre_s: 1800, ecran: 1 };
        let demande = Evenement::DemandeRecue {
            expediteur_device_id: 42,
            apres: Duration::from_secs(3),
            synchronisations: 2,
        };
        assert_eq!(
            appliquer(&actuel, &demande, 5_000, Some(&etat_avec_bob())),
            Some(PartageVue::Diffuse {
                spectateur: Some("Bob".into()),
                depuis_ms: 5_000,
                debit_mbps: 0.0,
                rtt_ms: None,
                ecran: 1
            })
        );
    }

    #[test]
    fn un_aller_retour_absent_reste_absent_jusqu_a_l_interface() {
        // I4 de la revue finale : l'hôte n'a aucune mesure de RTT, et un 0
        // s'affichait « Aller-retour : 0 ms ». Neutralisation : rendre
        // `rtt_ms: Some(rtt_ms.unwrap_or(0.0))` dans `appliquer` — ce test rougit.
        let actuel = PartageVue::Diffuse {
            spectateur: None,
            depuis_ms: 1,
            debit_mbps: 0.0,
            rtt_ms: None,
            ecran: 0,
        };
        let mesure = Evenement::Mesures(Mesures::Envoi {
            debit_mbps: 12.4,
            cible_mbps: 30.0,
            images_sautees: 0,
            rtt_ms: None,
        });
        let Some(PartageVue::Diffuse { rtt_ms, debit_mbps, .. }) =
            appliquer(&actuel, &mesure, 2_000, None)
        else {
            panic!("une mesure d'envoi met à jour la diffusion");
        };
        assert_eq!(rtt_ms, None, "aucune mesure ne devient un zéro");
        assert_eq!(debit_mbps, 12.4);
    }

    #[test]
    fn la_connexion_du_spectateur_garde_sa_duree_et_les_mesures_s_y_ajoutent() {
        let demande = PartageVue::Demande { ami: "Bob".into(), debut_ms: 0 };
        let connecte = Evenement::Connecte {
            en: Duration::from_millis(600),
            depuis_le_lancement: Some(Duration::from_secs(7)),
        };
        let regarde = appliquer(&demande, &connecte, 9_000, None).unwrap();
        // Neutralisation (tâche 10) : laisser `latence_decodage_ms` ou
        // `images_abandonnees` à zéro dans `appliquer` — les mesures de la
        // spec §8 n'atteindraient plus l'interface, et ce test rougit.
        let mesure = Evenement::Mesures(Mesures::Reception(MesuresVisionnage {
            images_par_seconde: 106.6,
            debit_kbps: 12_400,
            latence_decodage_ms: 1.5,
            images_abandonnees: 4,
            gigue_ms: 5.0,
        }));
        assert_eq!(
            appliquer(&regarde, &mesure, 10_000, None),
            Some(PartageVue::Regarde {
                ami: "Bob".into(),
                connecte_en_s: 0.6,
                debit_mbps: 12.4,
                images_par_s: 107,
                gigue_ms: 5.0,
                latence_decodage_ms: 1.5,
                images_abandonnees: 4,
                depuis_ms: 9_000,
            })
        );
    }

    fn erreur_d_ouverture(erreur: ErreurDecodeur) -> Result<Fin, ErreurPartage> {
        Err(ErreurPartage::Visionnage(ErreurVisionnage::Ouverture(erreur)))
    }

    // D1 (tâche 10) : chaque cause de décodage a SA fin, donc son message. Un
    // test par cause, pour qu'une branche qui rendrait la fin d'une autre
    // rougisse nommément. Neutralisation de chacun : faire rendre à sa branche
    // la fin d'une autre cause.

    #[test]
    fn sans_carte_nvidia_la_fin_le_dit_quelle_que_soit_la_langue_du_detail() {
        // Le détail vient de `libloading`, et Windows le rend dans sa langue.
        // La fin ne doit dépendre que de la VARIANTE : deux détails de langues
        // différentes donnent la même fin, et aucun ne la traverse.
        for detail in [
            "LoadLibraryExW failed: The specified module could not be found.",
            "LoadLibraryExW a échoué : le module spécifié est introuvable.",
        ] {
            assert_eq!(
                fin_vue(&erreur_d_ouverture(ErreurDecodeur::AucuneCarteNvidia(detail.into()))),
                FinVue::SansCarteNvidia
            );
        }
    }

    #[test]
    fn une_carte_sans_decodage_444_a_sa_fin() {
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::QuatreQuatreQuatreNonPris)),
            FinVue::SansDecodage444
        );
    }

    #[test]
    fn une_session_refusee_et_un_contexte_cuda_perdu_partagent_un_message_honnete() {
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::SessionRefusee(-1))),
            FinVue::DecodeurRefuse
        );
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::ContexteCuda("perdu".into()))),
            FinVue::DecodeurRefuse
        );
    }

    #[test]
    fn une_machine_sans_aucun_decodeur_le_dit() {
        // Tâche 5 du sous-jalon « toutes cartes ». Neutralisation : faire
        // rendre `FinVue::DecodeurRefuse` à `AucunDecodeur` — ce test rougit.
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::AucunDecodeur)),
            FinVue::Autre { message: crate::noyau::MESSAGE_AUCUN_DECODEUR.to_string() }
        );
    }

    #[test]
    fn un_echec_media_foundation_a_l_ouverture_est_un_decodeur_refuse() {
        // Le détail technique (code HRESULT, texte localisé de Windows) ne
        // traverse pas : même règle que `AucuneCarteNvidia`.
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::MediaFoundation(
                "ProcessInput : 0xC00D6D61".into()
            ))),
            FinVue::DecodeurRefuse
        );
    }

    #[test]
    fn une_resolution_trop_grande_porte_ses_chiffres() {
        // Les chiffres viennent de l'erreur, jamais d'un littéral de
        // l'interface (même règle que `AucuneDemande { fenetre_s }`).
        // Neutralisation : intervertir `largeur_max` et `hauteur_max`.
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::ResolutionTropGrande {
                largeur: 2560,
                hauteur: 1440,
                maximum: (2048, 1152),
            })),
            FinVue::ResolutionTropGrande {
                largeur: 2560,
                hauteur: 1440,
                largeur_max: 2048,
                hauteur_max: 1152
            }
        );
    }

    #[test]
    fn une_erreur_nee_pendant_le_flux_ne_prend_jamais_un_message_d_ouverture() {
        // Ronde de correction 1, I-1. Trois de ces variantes naissent aussi
        // PENDANT le flux (`decodeur.rs` : `rendre_contexte_courant`,
        // `verifier_format_de_sequence`, les rappels de décodage). Quelle que
        // soit la variante portée, la fin est celle du flux — jamais « n'a pas
        // pu démarrer », jamais « cette carte ne prend pas en charge… ».
        // Neutralisation : faire passer `DecodageInterrompu` par le `match` de
        // l'ouverture — ce test rougit (sur la première variante).
        for erreur in [
            ErreurDecodeur::QuatreQuatreQuatreNonPris,
            ErreurDecodeur::SessionRefusee(-1),
            ErreurDecodeur::ContexteCuda("perdu".into()),
            ErreurDecodeur::AucuneCarteNvidia("détail".into()),
            ErreurDecodeur::ResolutionTropGrande { largeur: 1, hauteur: 1, maximum: (1, 1) },
            // Les deux variantes Media Foundation : `MediaFoundation` naît
            // aussi pendant le flux (`DecodeurMf::decoder`).
            ErreurDecodeur::AucunDecodeur,
            ErreurDecodeur::MediaFoundation("ProcessOutput : échec".into()),
        ] {
            assert_eq!(
                fin_vue(&Err(ErreurPartage::Visionnage(ErreurVisionnage::DecodageInterrompu(
                    erreur
                )))),
                FinVue::DecodageInterrompu
            );
        }
        // Le contrôle positif : la MÊME variante, à l'ouverture, garde le
        // message de la spec. Sans lui, « tout est flux » passerait.
        assert_eq!(
            fin_vue(&erreur_d_ouverture(ErreurDecodeur::QuatreQuatreQuatreNonPris)),
            FinVue::SansDecodage444
        );
    }

    #[test]
    fn une_image_irreconstituable_a_sa_fin() {
        assert_eq!(
            fin_vue(&Err(ErreurPartage::Visionnage(ErreurVisionnage::ImageIrreconstituable))),
            FinVue::ImageIrreconstituable
        );
    }

    #[test]
    fn un_arret_annonce_par_l_hote_n_est_pas_une_erreur() {
        // D3 (tâche 10). Neutralisation : faire rendre `FinVue::Arrete` à
        // `Fin::PartageArrete` — l'utilisateur lirait « Partage arrêté. »,
        // comme s'il l'avait arrêté lui-même.
        assert_eq!(fin_vue(&Ok(Fin::PartageArrete)), FinVue::PartageArrete);
    }
}
