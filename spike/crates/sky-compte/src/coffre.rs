//! Coffre-fort système : range la clé privée de l'appareil et les jetons de
//! session dans le gestionnaire d'identifiants du système d'exploitation
//! (Windows Credential Manager, via `keyring`).
//!
//! `sky-crypto::Identity` était éphémère depuis le jalon 0 — son commentaire
//! l'annonçait : « au jalon 1 elle ira dans le coffre-fort du système ». Ce
//! module tient cette promesse. La clé privée ne quitte jamais ce fichier ;
//! rien ici ne doit jamais être journalisé, affiché, ni exposé par un
//! `Debug` qui en révèle le contenu.

use std::cell::RefCell;
use std::fmt;

use keyring::Entry;
use serde::{Deserialize, Serialize};
use sky_crypto::Identity;

use crate::erreur::ErreurCompte;

/// Nom de service du coffre de production. Stable entre les lancements :
/// c'est ce qui permet à l'identité de survivre à un redémarrage.
const SERVICE_PRODUCTION: &str = "SkyShare";
const UTILISATEUR_IDENTITE: &str = "identite";
const UTILISATEUR_JETONS: &str = "jetons";

/// Coffre-fort système pour la clé privée de l'appareil et les jetons de
/// session.
pub struct Coffre {
    service: String,
}

/// Jetons de session émis par l'API de signaling.
///
/// Ne dérive **pas** `Debug` : un jeton dans un journal ou un rapport de bug
/// vaut une session volée. L'implémentation manuelle ci-dessous n'affiche
/// jamais le contenu.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Jetons {
    pub session: String,
    pub renouvellement: String,
}

impl fmt::Debug for Jetons {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Jetons").finish_non_exhaustive()
    }
}

impl Coffre {
    /// Coffre de production : service stable et partagé entre tous les
    /// lancements de l'application sur cette machine.
    pub fn nouveau() -> Result<Coffre, ErreurCompte> {
        Ok(Coffre {
            service: SERVICE_PRODUCTION.to_string(),
        })
    }

    /// Coffre isolé pour les tests.
    ///
    /// `prefixe` doit être **distinct par test** : sans cela, deux tests
    /// utilisant le même nom de service se marcheraient dessus dans le
    /// trousseau réel de la machine, qui est partagé et persistant.
    ///
    /// Les entrées créées sous ce préfixe sont supprimées du trousseau
    /// quand le thread du test se termine — à la fin de la fonction de
    /// test, que celle-ci réussisse ou panique. **Pas** à l'abandon de
    /// chaque `Coffre` individuel : le test de persistance de l'identité
    /// abandonne volontairement un premier `Coffre` avant d'en recréer un
    /// second sous le même préfixe, et l'entrée doit survivre à cet
    /// abandon-là précisément — c'est ce qu'elle prouve. Nettoyer à ce
    /// moment-là aurait donc invalidé le test qu'il est censé protéger.
    ///
    /// Si le processus est tué net (pas d'unwind, pas de retour de thread),
    /// ce nettoyage ne s'exécute pas et les entrées restent dans le
    /// trousseau réel de la machine.
    pub fn pour_test(prefixe: &str) -> Coffre {
        let service = format!("SkyShare-test-{prefixe}");
        enregistrer_pour_nettoyage(service.clone());
        Coffre { service }
    }

    fn entree(&self, utilisateur: &str) -> Result<Entry, ErreurCompte> {
        Entry::new(&self.service, utilisateur).map_err(erreur_coffre)
    }

    /// Renvoie l'identité de l'appareil : la génère et la range au premier
    /// appel, la relit ensuite. C'est cette propriété qui fait que
    /// l'identité — et donc les enveloppes en vol, et la reconnaissance de
    /// l'appareil par ses amis — survit à un redémarrage.
    pub fn identite(&self) -> Result<Identity, ErreurCompte> {
        let entree = self.entree(UTILISATEUR_IDENTITE)?;
        match entree.get_secret() {
            Ok(octets) => {
                let octets: [u8; 32] = octets.try_into().map_err(|_| {
                    ErreurCompte::Coffre(
                        "clé privée de longueur inattendue dans le coffre".to_string(),
                    )
                })?;
                Ok(Identity::depuis_octets(&octets))
            }
            Err(keyring::Error::NoEntry) => {
                let identite = Identity::generate();
                entree.set_secret(&identite.en_octets()).map_err(erreur_coffre)?;
                Ok(identite)
            }
            Err(e) => Err(erreur_coffre(e)),
        }
    }

    /// Relit les jetons de session, s'ils existent.
    pub fn jetons(&self) -> Result<Option<Jetons>, ErreurCompte> {
        let entree = self.entree(UTILISATEUR_JETONS)?;
        match entree.get_password() {
            Ok(json) => serde_json::from_str(&json).map(Some).map_err(erreur_coffre),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(erreur_coffre(e)),
        }
    }

    /// Range les jetons de session, en écrasant les précédents s'il y en a.
    pub fn ranger_jetons(&self, jetons: &Jetons) -> Result<(), ErreurCompte> {
        let entree = self.entree(UTILISATEUR_JETONS)?;
        let json = serde_json::to_string(jetons).map_err(erreur_coffre)?;
        entree.set_password(&json).map_err(erreur_coffre)
    }

    /// Oublie les jetons de session (déconnexion). N'affecte pas l'identité
    /// de l'appareil : perdre sa session ne doit pas régénérer une clé.
    pub fn oublier(&self) -> Result<(), ErreurCompte> {
        let entree = self.entree(UTILISATEUR_JETONS)?;
        match entree.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(erreur_coffre(e)),
        }
    }
}

fn erreur_coffre(e: impl std::fmt::Display) -> ErreurCompte {
    ErreurCompte::Coffre(e.to_string())
}

// --- Nettoyage des coffres de test ------------------------------------
//
// Chaque service enregistré par `Coffre::pour_test` est supprimé du
// trousseau quand le thread qui l'a créé se termine — normalement ou par
// une panique (le déroulement de la pile exécute les destructeurs de
// variables thread-locales). Le test harness de `cargo test` exécute
// chaque fonction de test sur son propre thread : le nettoyage arrive donc
// après la fin de CETTE fonction de test, jamais entre deux `Coffre`
// construits à l'intérieur d'une même fonction.

struct RegistreNettoyage {
    services: Vec<String>,
}

impl Drop for RegistreNettoyage {
    fn drop(&mut self) {
        for service in &self.services {
            supprimer_silencieusement(service, UTILISATEUR_IDENTITE);
            supprimer_silencieusement(service, UTILISATEUR_JETONS);
        }
    }
}

fn supprimer_silencieusement(service: &str, utilisateur: &str) {
    if let Ok(entree) = Entry::new(service, utilisateur) {
        let _ = entree.delete_credential();
    }
}

thread_local! {
    static NETTOYAGE_TESTS: RefCell<RegistreNettoyage> =
        const { RefCell::new(RegistreNettoyage { services: Vec::new() }) };
}

fn enregistrer_pour_nettoyage(service: String) {
    NETTOYAGE_TESTS.with(|registre| {
        let mut registre = registre.borrow_mut();
        if !registre.services.contains(&service) {
            registre.services.push(service);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sky_crypto::Identity;

    #[test]
    fn l_identite_survit_a_un_redemarrage() {
        // La cle privee ne doit PAS etre ephemere : c'est ce que le commentaire
        // de sky-crypto annonce depuis le jalon 0 (« au jalon 1 elle ira dans le
        // coffre-fort du systeme »). Une cle regeneree a chaque lancement
        // invaliderait toutes les enveloppes en vol.
        let coffre = Coffre::pour_test("sky-test-identite");
        let premiere = coffre.identite().unwrap().public_key();
        drop(coffre);

        let coffre = Coffre::pour_test("sky-test-identite");
        assert_eq!(coffre.identite().unwrap().public_key(), premiere);
    }

    #[test]
    fn un_aller_retour_par_les_octets_preserve_la_cle() {
        let a = Identity::generate();
        let b = Identity::depuis_octets(&a.en_octets());
        let scelle = Identity::generate().seal(&b.public_key(), b"secret");
        assert_eq!(a.open(&scelle).unwrap(), b"secret");
    }

    #[test]
    fn aucun_jeton_au_depart() {
        // Ce que ce test casserait si le code de production disparaissait :
        // sans le `match ... Err(NoEntry) => Ok(None)` de `jetons()`, cet
        // appel renverrait une erreur (entrée absente) plutôt que `None`.
        let coffre = Coffre::pour_test("sky-test-jetons-absents");
        assert_eq!(coffre.jetons().unwrap(), None);
    }

    #[test]
    fn ranger_puis_relire_les_jetons() {
        let coffre = Coffre::pour_test("sky-test-jetons-aller-retour");
        let jetons = Jetons {
            session: "session-abc".to_string(),
            renouvellement: "renouvellement-xyz".to_string(),
        };
        coffre.ranger_jetons(&jetons).unwrap();
        assert_eq!(coffre.jetons().unwrap(), Some(jetons));
    }

    #[test]
    fn oublier_efface_les_jetons_mais_pas_l_identite() {
        let coffre = Coffre::pour_test("sky-test-oublier");
        let identite_avant = coffre.identite().unwrap().public_key();
        coffre
            .ranger_jetons(&Jetons {
                session: "s".to_string(),
                renouvellement: "r".to_string(),
            })
            .unwrap();

        coffre.oublier().unwrap();

        assert_eq!(coffre.jetons().unwrap(), None);
        assert_eq!(coffre.identite().unwrap().public_key(), identite_avant);
    }

    #[test]
    fn le_debug_de_jetons_n_affiche_pas_le_contenu() {
        let jetons = Jetons {
            session: "secret-de-session".to_string(),
            renouvellement: "secret-de-renouvellement".to_string(),
        };
        let debug = format!("{jetons:?}");
        assert!(!debug.contains("secret-de-session"));
        assert!(!debug.contains("secret-de-renouvellement"));
    }
}
