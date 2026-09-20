//! Outillage des tests du cœur — jamais compilé hors tests.

use std::sync::{Arc, Mutex};

use sky_compte::{Coffre, Config, Jetons};

use crate::coquille::Coquille;
use crate::faux_serveur::FauxServeur;
use crate::noyau::{Branchements, Noyau};
use crate::vue::Instantane;

#[derive(Default)]
pub(crate) struct CoquilleEspion {
    pub etats: Mutex<Vec<Instantane>>,
}

impl Coquille for Arc<CoquilleEspion> {
    fn publier_etat(&self, instantane: &Instantane) {
        self.etats.lock().unwrap().push(instantane.clone());
    }
}

pub(crate) struct Contexte {
    pub serveur: FauxServeur,
    pub noyau: Arc<Noyau>,
    pub coquille: Arc<CoquilleEspion>,
    /// Nombre d'appels au connecteur (qui, en production, ouvre le navigateur).
    pub connexions: Arc<Mutex<u32>>,
}

/// Un noyau branché sur un serveur double. `connecte` : des jetons valides
/// sont déjà dans le coffre. La connexion simulée range un jeton reconnu par
/// le double, sans navigateur.
pub(crate) fn contexte(prefixe: &str, connecte: bool) -> Contexte {
    let serveur = FauxServeur::demarrer();
    let jeton = serveur.jeton_de_test();
    let coffre = Coffre::pour_test(prefixe);
    if connecte {
        coffre
            .ranger_jetons(&Jetons {
                session: jeton.clone(),
                renouvellement: "peu-importe".into(),
            })
            .unwrap();
    }
    let coquille = Arc::new(CoquilleEspion::default());
    let connexions = Arc::new(Mutex::new(0u32));
    let compteur = Arc::clone(&connexions);
    let noyau = Arc::new(Noyau::nouveau(
        Config::vers(&serveur.url()),
        coffre,
        Branchements {
            coquille: Box::new(Arc::clone(&coquille)),
            connecter: Box::new(move |_config, coffre| {
                *compteur.lock().unwrap() += 1;
                let jetons =
                    Jetons { session: jeton.clone(), renouvellement: "peu-importe".into() };
                coffre.ranger_jetons(&jetons)?;
                Ok(jetons)
            }),
            nom_machine: Some("MACHINE-DE-TEST".into()),
        },
    ));
    Contexte { serveur, noyau, coquille, connexions }
}
