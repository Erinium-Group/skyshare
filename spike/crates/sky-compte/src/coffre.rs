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
use std::sync::{Mutex, MutexGuard};

use keyring::Entry;
use serde::{Deserialize, Serialize};
use sky_crypto::Identity;

use crate::erreur::ErreurCompte;

/// Verrou global sérialisant tout accès de ce crate au trousseau système (Windows
/// Credential Manager) — production comme tests.
///
/// CAUSE ÉTABLIE, PARTIELLEMENT : `cargo test` (mode par défaut, aucun drapeau)
/// exécute les tests d'un même binaire sur des threads concurrents. Plusieurs
/// d'entre eux créent des `Coffre::pour_test` DISTINCTS (préfixes vérifiés
/// uniques — grep sur tout le crate) qui touchent néanmoins le MÊME magasin
/// d'identifiants réel de la machine, PAS un magasin isolé par test. Une
/// reproduction ciblée (40 threads, chacun sur sa propre cible, écriture puis
/// lecture immédiate, 120 tentatives, sans réseau ni `FauxServeur`) N'A PAS
/// suffi à elle seule à provoquer l'incohérence observée dans la vraie suite —
/// le déclencheur exact semble exiger la charge combinée de la suite réelle
/// (accès au trousseau concurrents AVEC des serveurs `tiny_http` et des requêtes
/// `ureq` sur d'autres threads). La cause précise côté Windows (contention
/// interne au service de gestion des identifiants, limite de simultanéité,
/// interférence d'un antivirus scrutant les binaires fraîchement compilés, ou
/// autre) n'a donc PAS pu être identifiée avec certitude au-delà de ce qui est
/// exclu ci-dessus (pas une collision de nom de cible, pas un bug du registre de
/// nettoyage `NETTOYAGE_TESTS`). Ce qui est établi sans ambiguïté : le symptôme
/// n'apparaît QUE sous exécution parallèle, jamais avec `--test-threads=1`, et
/// disparaît quand ce verrou retire la variable qui reste sous notre contrôle —
/// deux opérations de ce crate sur le trousseau ne peuvent plus jamais se
/// recouvrir dans le temps, quel que soit le mécanisme exact en cause côté OS.
///
/// Coût : les opérations de coffre, dans CE process, ne se recouvrent plus
/// jamais entre elles. Acceptable : ni fréquentes ni sur un chemin chaud, en
/// production (une poignée d'appels par lancement de l'application, jamais deux
/// à la fois — un seul `Coffre` de production par processus) comme en test (une
/// opération locale, jamais un appel réseau sous le verrou).
static VERROU_TROUSSEAU: Mutex<()> = Mutex::new(());

/// Acquiert `VERROU_TROUSSEAU`, en absorbant un empoisonnement éventuel : une
/// panique d'un test PENDANT qu'il détient ce verrou (peu probable — aucun code
/// sous le verrou ne panique volontairement) ne doit pas condamner tous les
/// tests suivants à leur tour. Le contenu protégé est `()` : rien à corrompre.
fn verrou_trousseau() -> MutexGuard<'static, ()> {
    VERROU_TROUSSEAU.lock().unwrap_or_else(|empoisonne| empoisonne.into_inner())
}

/// Nom de service du coffre de production. Stable entre les lancements :
/// c'est ce qui permet à l'identité de survivre à un redémarrage.
///
/// Choisi délibérément, pas un nom technique laissé au hasard : c'est le nom
/// du produit, celui que l'utilisateur verra dans son gestionnaire
/// d'identifiants Windows.
const SERVICE_PRODUCTION: &str = "SkyShare";
const UTILISATEUR_IDENTITE: &str = "identite";
const UTILISATEUR_JETONS: &str = "jetons";
const UTILISATEUR_APPAREIL: &str = "appareil";

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
        let _verrou = verrou_trousseau();
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
    ///
    /// En cas de contenu illisible (JSON corrompu ou trafiqué), l'erreur ne
    /// reprend jamais ce contenu : le message de `serde_json` échote la
    /// valeur fautive en clair (p. ex. `invalid type: integer \`123456789\`,
    /// expected a string`), ce qui ferait fuiter un fragment du coffre par
    /// un message d'erreur — exactement ce que `corps_sans_en_tete` (dans
    /// `http.rs`) évite déjà pour les réponses du serveur.
    pub fn jetons(&self) -> Result<Option<Jetons>, ErreurCompte> {
        let _verrou = verrou_trousseau();
        let entree = self.entree(UTILISATEUR_JETONS)?;
        match entree.get_password() {
            Ok(json) => serde_json::from_str(&json).map(Some).map_err(|_| {
                ErreurCompte::Coffre("jetons illisibles dans le coffre".to_string())
            }),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(erreur_coffre(e)),
        }
    }

    /// Range les jetons de session, en écrasant les précédents s'il y en a.
    pub fn ranger_jetons(&self, jetons: &Jetons) -> Result<(), ErreurCompte> {
        let _verrou = verrou_trousseau();
        let entree = self.entree(UTILISATEUR_JETONS)?;
        // Sérialisation d'une struct de deux `String` : n'échoue en pratique
        // jamais. Même prudence que côté lecture par cohérence : le message
        // d'erreur ne doit jamais reprendre le contenu qu'il tente d'écrire.
        let json = serde_json::to_string(jetons).map_err(|_| {
            ErreurCompte::Coffre("échec de sérialisation des jetons".to_string())
        })?;
        entree.set_password(&json).map_err(erreur_coffre)
    }

    /// Oublie les jetons de session (déconnexion). N'affecte pas l'identité
    /// de l'appareil : perdre sa session ne doit pas régénérer une clé.
    pub fn oublier(&self) -> Result<(), ErreurCompte> {
        let _verrou = verrou_trousseau();
        let entree = self.entree(UTILISATEUR_JETONS)?;
        match entree.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(erreur_coffre(e)),
        }
    }

    /// Relit l'identifiant d'appareil local (ajouté à la tâche 9) — celui
    /// que le serveur a attribué au dernier `enregistrer_appareil` réussi
    /// (`annuaire.rs`), et que `deposer` (`boite.rs`) doit envoyer comme
    /// `expediteur_device_id`. `None` si aucun enregistrement n'a encore
    /// réussi sur cette machine.
    ///
    /// Vit À CÔTÉ de l'identité, PAS des jetons : `oublier()` (déconnexion)
    /// ne l'efface pas — se déconnecter ne doit pas rendre `deposer`
    /// inutilisable au prochain lancement, seulement exiger une nouvelle
    /// authentification.
    ///
    /// RISQUE CONNU si un AUTRE compte Discord se connecte sur cette même
    /// machine ensuite : cet identifiant continue de désigner l'appareil du
    /// PREMIER compte, puisqu'il n'est pas lié aux jetons qui changent de
    /// compte. Un dépôt tenté sous le second compte échoue alors par le 404
    /// uniforme du serveur (« pas mon appareil ») — pas une fuite, le
    /// serveur refuse bien — mais sans diagnostic clair tant que
    /// `enregistrer_appareil` n'est pas rappelé pour ce second compte, ce
    /// qui écrase cette entrée avec le nouvel identifiant.
    pub fn identifiant_appareil(&self) -> Result<Option<i64>, ErreurCompte> {
        let _verrou = verrou_trousseau();
        let entree = self.entree(UTILISATEUR_APPAREIL)?;
        match entree.get_password() {
            Ok(valeur) => valeur.trim().parse::<i64>().map(Some).map_err(|_| {
                ErreurCompte::Coffre("identifiant d'appareil illisible dans le coffre".to_string())
            }),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(erreur_coffre(e)),
        }
    }

    /// Range l'identifiant d'appareil local, en écrasant le précédent s'il y
    /// en a un — c'est précisément le cas d'un ré-enregistrement décrit sur
    /// `identifiant_appareil`. Appelée par `enregistrer_appareil`
    /// (`annuaire.rs`) dès que le serveur a confirmé la création, jamais
    /// laissée à la charge de l'appelant : voir son commentaire pour la
    /// raison (une serrure posée que personne ne branche).
    pub fn ranger_identifiant_appareil(&self, id: i64) -> Result<(), ErreurCompte> {
        let _verrou = verrou_trousseau();
        let entree = self.entree(UTILISATEUR_APPAREIL)?;
        entree.set_password(&id.to_string()).map_err(erreur_coffre)
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
            supprimer_silencieusement(service, UTILISATEUR_APPAREIL);
        }
    }
}

fn supprimer_silencieusement(service: &str, utilisateur: &str) {
    let _verrou = verrou_trousseau();
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
    fn jetons_corrompus_ne_fuient_pas_dans_l_erreur() {
        // Ecrit directement, en contournant `ranger_jetons`, un contenu qui
        // n'a pas la forme de `Jetons` — comme le ferait une corruption ou
        // une falsification du trousseau. Le marqueur ci-dessous simule un
        // fragment du secret reel : si le message d'erreur le reprend, la
        // fuite est prouvee.
        let coffre = Coffre::pour_test("sky-test-jetons-corrompus");
        let entree = Entry::new(&coffre.service, UTILISATEUR_JETONS).unwrap();
        entree
            .set_password(r#"{"session":123456789,"renouvellement":"SECRET-DE-RENOUVELLEMENT"}"#)
            .unwrap();

        let erreur = coffre.jetons().unwrap_err();
        let message = erreur.to_string();

        assert_eq!(message, "échec du coffre local : jetons illisibles dans le coffre");
        assert!(!message.contains("123456789"));
        assert!(!message.contains("SECRET-DE-RENOUVELLEMENT"));
    }

    #[test]
    fn aucun_identifiant_dappareil_au_depart() {
        let coffre = Coffre::pour_test("sky-test-appareil-absent");
        assert_eq!(coffre.identifiant_appareil().unwrap(), None);
    }

    #[test]
    fn ranger_puis_relire_lidentifiant_dappareil() {
        let coffre = Coffre::pour_test("sky-test-appareil-aller-retour");
        coffre.ranger_identifiant_appareil(42).unwrap();
        assert_eq!(coffre.identifiant_appareil().unwrap(), Some(42));
    }

    #[test]
    fn ranger_ecrase_le_precedent_identifiant_dappareil() {
        // Le cas d'un ré-enregistrement (nouveau compte, ou appareil
        // recréé) : la valeur la plus récente doit remplacer l'ancienne,
        // pas s'y ajouter.
        let coffre = Coffre::pour_test("sky-test-appareil-ecrase");
        coffre.ranger_identifiant_appareil(1).unwrap();
        coffre.ranger_identifiant_appareil(2).unwrap();
        assert_eq!(coffre.identifiant_appareil().unwrap(), Some(2));
    }

    #[test]
    fn oublier_nefface_pas_lidentifiant_dappareil() {
        // NEUTRALISATION CIBLÉE : si `identifiant_appareil` vivait sous la
        // même entrée que les jetons (ou si `oublier` l'effaçait aussi), ce
        // test rougirait précisément ici — une déconnexion ne doit pas
        // rendre `deposer` inutilisable au prochain lancement.
        let coffre = Coffre::pour_test("sky-test-appareil-survit-oubli");
        coffre.ranger_identifiant_appareil(7).unwrap();
        coffre
            .ranger_jetons(&Jetons { session: "s".to_string(), renouvellement: "r".to_string() })
            .unwrap();

        coffre.oublier().unwrap();

        assert_eq!(coffre.jetons().unwrap(), None);
        assert_eq!(coffre.identifiant_appareil().unwrap(), Some(7));
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
