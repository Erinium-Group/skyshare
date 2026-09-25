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
use sky_encode::Codec;
use sky_partage::{
    Arret, Designation, ErreurPartage, Evenement, Fin, Mesures, ParametresHote,
    ParametresSpectateur, SourceImages,
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
        codec: Codec,
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
        codec: Codec,
        ecran: usize,
        arret: &Arret,
        evenements: &mut dyn FnMut(Evenement),
    ) -> Result<Fin, ErreurPartage> {
        sky_partage::heberger(
            noyau.config(),
            noyau.coffre(),
            |_| noyau.synchroniser(),
            ParametresHote {
                codec,
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
        // Aucun puits : le flux est mesuré puis JETÉ (spec D2). L'écriture dans
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
        Ok(Fin::TamponSature { .. }) => FinVue::TropLente,
        Ok(Fin::AucuneDemande) => FinVue::AucuneDemande,
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
        Err(ErreurPartage::Autre(e)) => FinVue::Autre { message: e.to_string() },
    }
}

/// Le partage affiché après un événement, ou `None` s'il ne change rien.
///
/// Les événements qui ne changent rien à l'écran (`Pret`, `Negociation`,
/// `DemandeEcartee`, `EchecLocal`, `ReponseRecue`, `Diffusion`) rendent `None` :
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
                rtt_ms: 0.0,
                ecran: *ecran,
            })
        }
        (PartageVue::Diffuse { spectateur, ecran, .. }, Evenement::Connecte { .. }) => {
            Some(PartageVue::Diffuse {
                spectateur: spectateur.clone(),
                depuis_ms: maintenant_ms,
                debit_mbps: 0.0,
                rtt_ms: 0.0,
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
                depuis_ms: maintenant_ms,
            })
        }
        (
            PartageVue::Regarde { ami, connecte_en_s, depuis_ms, .. },
            Evenement::Mesures(Mesures::Reception { debit_mbps, images_par_s, gigue_ms }),
        ) => Some(PartageVue::Regarde {
            ami: ami.clone(),
            connecte_en_s: *connecte_en_s,
            debit_mbps: *debit_mbps,
            images_par_s: *images_par_s,
            gigue_ms: *gigue_ms,
            depuis_ms: *depuis_ms,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_compte::{Ami, AppareilDAmi};
    use sky_partage::Diagnostic;
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
        assert_eq!(fin_vue(&Ok(Fin::TamponSature { morceau: 3, morceaux: 9 })), FinVue::TropLente);
        assert_eq!(
            fin_vue(&Err(ErreurPartage::Compte(ErreurCompte::Refuse))),
            FinVue::SessionExpiree
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
                rtt_ms: 0.0,
                ecran: 1
            })
        );
    }

    #[test]
    fn la_connexion_du_spectateur_garde_sa_duree_et_les_mesures_s_y_ajoutent() {
        let demande = PartageVue::Demande { ami: "Bob".into(), debut_ms: 0 };
        let connecte = Evenement::Connecte {
            en: Duration::from_millis(600),
            depuis_le_lancement: Some(Duration::from_secs(7)),
        };
        let regarde = appliquer(&demande, &connecte, 9_000, None).unwrap();
        let mesure = Evenement::Mesures(Mesures::Reception {
            debit_mbps: 12.4,
            images_par_s: 107,
            gigue_ms: 5.0,
        });
        assert_eq!(
            appliquer(&regarde, &mesure, 10_000, None),
            Some(PartageVue::Regarde {
                ami: "Bob".into(),
                connecte_en_s: 0.6,
                debit_mbps: 12.4,
                images_par_s: 107,
                gigue_ms: 5.0,
                depuis_ms: 9_000,
            })
        );
    }
}
