//! Serveur double de l'API du site (jalon C2, tâche 4).
//!
//! Écoute en HTTP sur `127.0.0.1`, sur un port libre choisi par le système
//! (port 0) — deux tests qui tournent en parallèle ne se disputent donc
//! jamais un port fixe. Aucun compte Discord réel, aucune base Postgres
//! réelle : les tâches suivantes du jalon pilotent ce double via
//! `EtatFaux` au lieu de toucher la production, qui porte 11 comptes
//! Discord réels et 5 sessions réelles.
//!
//! ## Origine des formes
//!
//! Chaque forme JSON ci-dessous est DÉRIVÉE des tests du site
//! (`src/app/api/sky/**/__tests__/*.ts`, `src/lib/sky/*.ts`), jamais de ma
//! compréhension de l'API — voir `task-4-report.md` pour le détail
//! fichier par fichier. Un double qui répondrait ce que l'application
//! ESPÈRE plutôt que ce que le vrai serveur RÉPOND donnerait une confiance
//! imméritée : c'est exactement le défaut que ce relevé évite.
//!
//! Aucun runtime asynchrone : `tiny_http` est synchrone, une seule
//! requête traitée à la fois sur le fil dédié du double.

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;

use serde::Serialize;
use serde_json::{json, Value};
use tiny_http::{Header, Method, Response, Server};

/// Code de synchronisation rendu par le double — valeur fixe, dans
/// l'alphabet réel des codes du site (`codes.ts::ALPHABET`, sans O/0, I/1/L
/// pour éviter l'ambiguïté visuelle), sans qu'aucun test actuel n'en
/// dépende.
const CODE_FAUX: &str = "FAUX2345";

/// Appareil d'un AMI ACCEPTÉ, tel que rendu par `GET /api/sky/sync` dans
/// `amis[].appareils` — forme figée par `AppareilDAmi` (site :
/// `src/lib/sky/appareils.ts`). EXACTEMENT deux champs : un ami accepté
/// n'expose ni nom, ni plateforme, ni dates de son appareil, seulement ce
/// qu'il faut pour sceller une enveloppe à son intention.
#[derive(Debug, Clone, Serialize)]
pub struct AppareilDAmiFaux {
    pub id: i64,
    pub public_key: String,
}

/// Ami tel que rendu par `GET /api/sky/sync` — forme figée par `Ami` (site :
/// `src/lib/sky/amis.ts`, fonction `amisDe`).
///
/// `friendshipId` est en camelCase, tout le reste (`discord_name`,
/// `discord_avatar`) en snake_case : c'est la forme EXACTE observée dans
/// `amisDe` (alias SQL `AS "friendshipId"` contre les colonnes brutes
/// `u.discord_name`/`u.discord_avatar`), pas une convention uniforme
/// inventée pour ce double.
#[derive(Debug, Clone, Serialize)]
pub struct AmiFaux {
    #[serde(rename = "friendshipId")]
    pub friendship_id: i64,
    pub id: i64,
    pub discord_name: String,
    pub discord_avatar: Option<String>,
    pub depuis: String,
    /// Tableau VIDE, jamais absent, pour un ami sans appareil — voir
    /// `AmiFaux::sans_appareil` et le commentaire de `Ami::appareils`
    /// côté site.
    pub appareils: Vec<AppareilDAmiFaux>,
}

impl AmiFaux {
    /// Ami qui n'a enregistré aucun appareil (ou dont tous sont révoqués) :
    /// `appareils` doit rester `[]`, jamais absent — c'est le piège que la
    /// tâche 4 doit vérifier explicitement (voir le test
    /// `le_double_rend_un_tableau_vide_jamais_absent` plus bas).
    pub fn sans_appareil(discord_name: &str) -> AmiFaux {
        AmiFaux::avec_appareils(discord_name, Vec::new())
    }

    /// Ami avec des appareils explicites — pour les tests qui ont besoin
    /// d'une clé publique connue d'avance (ex. pour sceller une enveloppe
    /// vers ce destinataire).
    pub fn avec_appareils(discord_name: &str, appareils: Vec<AppareilDAmiFaux>) -> AmiFaux {
        let id = id_deterministe(discord_name);
        AmiFaux {
            friendship_id: id,
            id,
            discord_name: discord_name.to_string(),
            discord_avatar: None,
            depuis: "2026-01-01T00:00:00.000Z".to_string(),
            appareils,
        }
    }
}

/// Dérive un identifiant positif stable à partir d'un texte — utilisé pour
/// donner à chaque `AmiFaux` un `id`/`friendshipId` déterministe sans état
/// mutable partagé entre les appels de construction. Aucune prétention
/// cryptographique : seule l'unicité pratique entre quelques noms de test
/// compte ici.
fn id_deterministe(texte: &str) -> i64 {
    let mut h: u64 = 1469598103934665603; // FNV-1a, offset de base
    for octet in texte.as_bytes() {
        h ^= *octet as u64;
        h = h.wrapping_mul(1099511628211);
    }
    // Borné à un i64 positif, largement sous POSTGRES_INTEGER_MAX côté
    // site : ce double n'a aucune raison de produire une valeur qui
    // dépasserait un INTEGER réel.
    (h % 2_000_000_000).max(1) as i64
}

/// Enveloppe scellée telle que rendue par `GET /api/sky/sync` — forme figée
/// par `Enveloppe` (site : `src/lib/sky/enveloppes.ts`) ET par la fonction
/// `serialiser` de `sync/route.ts` qui convertit `charge` (un `Buffer` côté
/// site) en base64 avant de l'envoyer sur le fil.
///
/// PIÈGE DE FORME SIGNALÉ PAR LE BRIEF, VÉRIFIÉ ICI : `id` est une CHAÎNE
/// (`envelopes.id` est `BIGSERIAL` côté site, rendu en `string` par le
/// pilote Neon pour ne pas perdre en silence une valeur au-delà de
/// `Number.MAX_SAFE_INTEGER`), alors que `expediteur_device_id` et
/// `destinataire_device_id` sont des ENTIERS (`INTEGER`, référence vers
/// `devices.id` qui est `SERIAL`). Les typer pareil serait exactement le
/// défaut de forme que le brief demande d'éviter.
#[derive(Debug, Clone, Serialize)]
pub struct EnveloppeFausse {
    pub id: String,
    pub expediteur_device_id: i64,
    pub destinataire_device_id: i64,
    /// Base64 — jamais le tableau d'octets décimaux que produirait un
    /// `JSON.stringify` naïf d'un `Buffer` côté site (voir le commentaire
    /// de `serialiser` dans `sync/route.ts`).
    pub charge: String,
}

/// État piloté par les tests — les tâches suivantes du jalon (6, 7, 9)
/// mutent ces champs via `FauxServeur::etat_mut()` pour commander les
/// réponses du double, sans jamais toucher la production.
///
/// `amis`, `enveloppes` et `version` sont les trois champs exigés par le
/// brief de cette tâche. Les cinq champs suivants sont un AJOUT délibéré,
/// tranché par le coordinateur en dehors du brief : les tâches 6, 7 et 9
/// ont déjà leurs tests écrits contre ces noms précis, et sky-compte ne
/// compilerait pas sans eux. Voir le commentaire de chacun pour le
/// raisonnement.
#[derive(Debug, Default)]
pub struct EtatFaux {
    pub amis: Vec<AmiFaux>,
    pub enveloppes: Vec<EnveloppeFausse>,
    pub version: u64,

    /// Fait répondre `401 {"error":"Code refuse"}` à `POST /api/auth/native`
    /// quel que soit le corps envoyé.
    ///
    /// À CETTE TÂCHE, ce champ est REDONDANT avec le comportement PAR
    /// DÉFAUT de la route (voir `gerer_auth_native` plus bas) : `EtatFaux`
    /// ne porte encore aucun moyen d'enregistrer un couple (code, secret)
    /// "valide", donc le double refuse déjà systématiquement tout échange,
    /// exactement la forme observée côté site pour un code qui n'a jamais
    /// été émis (`le_double_refuse_sans_distinguer` ci-dessous le prouve
    /// SANS jamais toucher ce champ). Il est exposé maintenant pour que
    /// les tâches 6/7/9 compilent contre lui, et pour rester le point de
    /// court-circuit le jour où une tâche future donnerait au double un
    /// chemin de succès conditionnel.
    pub refuser_echange: bool,

    /// Fait répondre 401 au PROCHAIN appel authentifié (`GET
    /// /api/sky/sync` ou `POST /api/sky/envelopes`), UNE SEULE FOIS :
    /// consommé (remis à `false`) dès qu'il a servi à refuser un appel.
    /// Sert à éprouver le renouvellement de jeton côté client : refus,
    /// renouvellement via `POST /api/auth/refresh`, nouvelle tentative
    /// réussie.
    pub refuser_le_premier_appel: bool,

    /// Refuse TOUT appel authentifié ET tout renouvellement — à la
    /// différence de `refuser_le_premier_appel`, qui épargne
    /// délibérément `POST /api/auth/refresh` (sans quoi éprouver le
    /// renouvellement lui-même serait impossible). Utile pour simuler un
    /// compte totalement révoqué.
    pub refuser_tout: bool,

    /// Nombre d'appels reçus sur `POST /api/auth/refresh`, acceptés ou
    /// refusés confondus — observable pour prouver qu'un client renouvelle
    /// une fois et retente, plutôt que de boucler indéfiniment.
    pub appels_de_renouvellement: u64,

    /// Nombre de dépôts d'enveloppe ACCEPTÉS par `POST /api/sky/envelopes`
    /// (forme valide, authentification passée) — observable pour les
    /// tests de boîte aux lettres des tâches suivantes.
    pub depots_recus: u64,
}

/// Serveur double : un `tiny_http::Server` sur un port éphémère, dans un
/// fil dédié, avec un `Arc<Mutex<EtatFaux>>` partagé entre ce fil et les
/// tests qui le pilotent.
pub struct FauxServeur {
    url: String,
    etat: Arc<Mutex<EtatFaux>>,
    serveur: Arc<Server>,
    fil: Option<thread::JoinHandle<()>>,
}

impl FauxServeur {
    /// Démarre le double. Écoute sur `127.0.0.1:0` — port 0 délègue le
    /// choix au système d'exploitation, qui rend un port libre : c'est ce
    /// qui permet à plusieurs tests de tourner en parallèle sans jamais se
    /// disputer un port fixe.
    pub fn demarrer() -> FauxServeur {
        let serveur =
            Server::http("127.0.0.1:0").expect("le double n'a pas pu ouvrir de socket local");
        let serveur = Arc::new(serveur);
        let port = adresse_port(&serveur);
        let url = format!("http://127.0.0.1:{port}");

        let etat = Arc::new(Mutex::new(EtatFaux::default()));

        let serveur_fil = Arc::clone(&serveur);
        let etat_fil = Arc::clone(&etat);
        let fil = thread::spawn(move || {
            // `incoming_requests()` bloque jusqu'à la prochaine requête OU
            // jusqu'à `Server::unblock()` (appelé par `Drop`, plus bas), qui
            // le fait sortir proprement de la boucle.
            for requete in serveur_fil.incoming_requests() {
                repondre(requete, &etat_fil);
            }
        });

        FauxServeur { url, etat, serveur, fil: Some(fil) }
    }

    /// URL réelle du double (`http://127.0.0.1:<port choisi par le
    /// système>`), à utiliser comme `SKY_API_URL` / `Config::vers(...)`
    /// dans les tests des tâches suivantes.
    pub fn url(&self) -> String {
        self.url.clone()
    }

    /// Accès exclusif à l'état piloté — pour préparer les réponses avant
    /// d'appeler le double (`s.etat_mut().amis.push(...)`) ou pour
    /// observer un compteur après coup (`s.etat_mut().depots_recus`).
    pub fn etat_mut(&self) -> MutexGuard<'_, EtatFaux> {
        self.etat.lock().expect("le mutex de l'etat faux est empoisonne")
    }
}

impl Drop for FauxServeur {
    fn drop(&mut self) {
        // Débloque le fil s'il est en train d'attendre une requête dans
        // `incoming_requests()`, pour qu'il puisse sortir de la boucle et
        // se terminer proprement plutôt que de laisser un fil orphelin par
        // test.
        self.serveur.unblock();
        if let Some(fil) = self.fil.take() {
            let _ = fil.join();
        }
    }
}

fn adresse_port(serveur: &Server) -> u16 {
    // `Server::http("127.0.0.1:0")` ne peut rendre qu'une adresse IP —
    // `ListenAddr` n'a pas d'autre variante tant que la fonctionnalité
    // socket Unix de tiny_http n'est pas activée (elle ne l'est pas ici).
    let tiny_http::ListenAddr::IP(adresse) = serveur.server_addr();
    adresse.port()
}

/// Traite une requête HTTP reçue par le fil du double et lui répond.
fn repondre(mut requete: tiny_http::Request, etat: &Arc<Mutex<EtatFaux>>) {
    let methode = requete.method().clone();
    let (chemin, version_connue) = decouper_chemin(requete.url());

    let mut corps_brut = String::new();
    if matches!(methode, Method::Post) {
        // Meilleur effort : un corps illisible devient une chaîne vide,
        // que chaque gestionnaire refuse alors comme un JSON invalide —
        // jamais de panique sur une requête malformée.
        let _ = requete.as_reader().read_to_string(&mut corps_brut);
    }

    let (statut, corps_reponse) = match (&methode, chemin.as_str()) {
        (Method::Get, "/api/sky/sync") => gerer_sync(etat, version_connue),
        (Method::Post, "/api/auth/native") => gerer_auth_native(etat),
        (Method::Post, "/api/auth/refresh") => gerer_refresh(etat),
        (Method::Post, "/api/sky/envelopes") => gerer_depot(etat, &corps_brut),
        _ => (404u16, json!({ "error": "route inconnue du double" }).to_string()),
    };

    let en_tete_json = Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..])
        .expect("en-tete Content-Type statique toujours valide");
    let reponse = Response::from_string(corps_reponse)
        .with_status_code(statut)
        .with_header(en_tete_json);
    let _ = requete.respond(reponse);
}

/// Sépare le chemin de la requête de son paramètre `?version=`, en suivant
/// exactement la tolérance de `analyserVersion` côté site (`sync/route.ts`) :
/// une valeur absente OU mal formée (non numérique, négative, décimale) est
/// traitée comme absente, jamais comme une erreur.
fn decouper_chemin(brut: &str) -> (String, Option<u64>) {
    match brut.split_once('?') {
        None => (brut.to_string(), None),
        Some((chemin, requete)) => {
            let version = requete
                .split('&')
                .find_map(|paire| paire.strip_prefix("version="))
                .and_then(|valeur| valeur.parse::<u64>().ok());
            (chemin.to_string(), version)
        }
    }
}

/// Applique les deux leviers de refus partagés par les routes authentifiées
/// du double (`GET /api/sky/sync`, `POST /api/sky/envelopes`) :
/// `refuser_tout` (permanent) et `refuser_le_premier_appel` (un coup,
/// consommé dès qu'il a servi). Rend `Some((401, corps))` si l'appel doit
/// être refusé, `None` sinon.
///
/// Forme du refus alignée sur `handleAuthError` côté site
/// (`src/lib/auth/middleware.ts`) pour une session absente :
/// `401 {"error":"Non authentifie"}`.
fn refuser_appel_authentifie(etat: &mut EtatFaux) -> Option<(u16, String)> {
    if etat.refuser_tout {
        return Some((401, json!({ "error": "Non authentifie" }).to_string()));
    }
    if etat.refuser_le_premier_appel {
        etat.refuser_le_premier_appel = false;
        return Some((401, json!({ "error": "Non authentifie" }).to_string()));
    }
    None
}

fn gerer_sync(etat: &Arc<Mutex<EtatFaux>>, version_connue: Option<u64>) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = refuser_appel_authentifie(&mut e) {
        return refus;
    }

    if version_connue == Some(e.version) {
        return (200, json!({ "inchange": true }).to_string());
    }

    let corps = json!({
        "version": e.version,
        "code": CODE_FAUX,
        "amis": e.amis,
        // Pas encore pilotables par EtatFaux (hors scope de cette tâche) —
        // mais TOUJOURS présents, tableaux vides plutôt qu'absents, même
        // règle que `amis[].appareils`.
        "demandes": Vec::<Value>::new(),
        "listes": Vec::<Value>::new(),
        "appareils": Vec::<Value>::new(),
        "enveloppes": e.enveloppes,
    });
    (200, corps.to_string())
}

/// `POST /api/auth/native` — voir le commentaire de `EtatFaux::refuser_echange`
/// pour la raison pour laquelle cette route refuse systématiquement à ce
/// stade du jalon.
fn gerer_auth_native(etat: &Arc<Mutex<EtatFaux>>) -> (u16, String) {
    let e = etat.lock().expect("mutex etat faux empoisonne");
    if e.refuser_echange || e.refuser_tout {
        return (401, r#"{"error":"Code refuse"}"#.to_string());
    }
    // Aucun mécanisme n'existe encore, à cette tâche, pour enregistrer un
    // couple (code, secret) "valide" dans EtatFaux : le double refuse donc
    // TOUJOURS l'échange, par CE chemin plutôt que par le court-circuit
    // ci-dessus — distinction qui aura un sens le jour où une tâche future
    // ajoutera un succès conditionnel ici, sans changer la forme du refus.
    (401, r#"{"error":"Code refuse"}"#.to_string())
}

/// `POST /api/auth/refresh` — succès à un seul champ `acces` (jamais
/// `refresh` en retour, forme confirmée par `api/auth/refresh/route.ts`).
/// La valeur `"jeton-neuf"` est un CHOIX DU DOUBLE, pas une forme relevée
/// du site : c'est la valeur que les tests de la tâche 7 attendent
/// (demande explicite du coordinateur).
fn gerer_refresh(etat: &Arc<Mutex<EtatFaux>>) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_de_renouvellement += 1;
    if e.refuser_tout {
        return (401, json!({ "error": "Renouvellement refuse" }).to_string());
    }
    (200, json!({ "acces": "jeton-neuf" }).to_string())
}

/// `POST /api/sky/envelopes` — succès `204` sans corps (forme confirmée par
/// `envelopes/route.ts`), refus de forme `400`, refus authentifié partagé
/// avec `GET /api/sky/sync` via `refuser_appel_authentifie`.
fn gerer_depot(etat: &Arc<Mutex<EtatFaux>>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = refuser_appel_authentifie(&mut e) {
        return refus;
    }

    let valeur: Value = match serde_json::from_str(corps_brut) {
        Ok(v) => v,
        Err(_) => return (400, json!({ "error": "Corps JSON invalide" }).to_string()),
    };
    let expediteur = valeur.get("expediteur_device_id").and_then(Value::as_i64);
    let destinataire = valeur.get("destinataire_device_id").and_then(Value::as_i64);
    let charge = valeur.get("charge").and_then(Value::as_str).map(str::to_string);

    let (Some(expediteur), Some(destinataire), Some(charge)) = (expediteur, destinataire, charge)
    else {
        return (400, json!({ "error": "Corps invalide" }).to_string());
    };

    e.depots_recus += 1;
    let id = (e.enveloppes.len() + 1).to_string();
    e.enveloppes.push(EnveloppeFausse {
        id,
        expediteur_device_id: expediteur,
        destinataire_device_id: destinataire,
        charge,
    });

    (204, String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    // « Qu'est-ce qui, précisément, ferait échouer ce test ? » Réponse pour
    // chacun des tests ci-dessous en commentaire, avec la neutralisation
    // pratiquée pour le prouver — voir task-4-report.md pour le relevé
    // complet des neutralisations et de leur rougissement.

    #[test]
    fn le_double_rend_un_tableau_vide_jamais_absent() {
        // Échoue si `appareils` est sérialisé absent (Option::None -> champ
        // manquant) plutôt que `[]`, ou si AmiFaux ne construit pas un
        // tableau vide par défaut.
        let s = FauxServeur::demarrer();
        s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu["amis"][0]["appareils"], serde_json::json!([]));
    }

    #[test]
    fn le_double_refuse_sans_distinguer() {
        // Les quatre causes d'echec rendent la MEME reponse, comme le vrai.
        // Échoue si le double distingue "code inconnu" d'un autre refus, ou
        // s'il répond autre chose que 401 / ce corps exact.
        let s = FauxServeur::demarrer();
        for corps in [r#"{"code":"inconnu","secret":"x"}"#, r#"{"code":"","secret":""}"#] {
            let e = ureq::post(&format!("{}/api/auth/native", s.url()))
                .send_string(corps)
                .unwrap_err();
            let reponse = match e {
                ureq::Error::Status(_, r) => r,
                _ => panic!("attendu 401"),
            };
            assert_eq!(reponse.status(), 401);
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Code refuse"}"#);
        }
    }

    #[test]
    fn chaque_serveur_double_choisit_un_port_different() {
        // Échoue si `demarrer()` écoutait sur un port fixe : deux instances
        // en parallèle se disputeraient alors le même port, et l'une des
        // deux échouerait à démarrer plutôt que de rendre une URL distincte.
        let a = FauxServeur::demarrer();
        let b = FauxServeur::demarrer();
        assert_ne!(a.url(), b.url());
    }

    #[test]
    fn refuser_le_premier_appel_ne_refuse_que_le_premier() {
        // Échoue si le refus n'est pas consommé (tous les appels
        // refuseraient) ou s'il est consommé trop tôt (aucun appel ne
        // refuserait).
        let s = FauxServeur::demarrer();
        s.etat_mut().refuser_le_premier_appel = true;

        let premier = ureq::get(&format!("{}/api/sky/sync", s.url())).call();
        assert_eq!(premier.unwrap_err().into_response().unwrap().status(), 401);

        let second = ureq::get(&format!("{}/api/sky/sync", s.url())).call();
        assert_eq!(second.unwrap().status(), 200);
    }

    #[test]
    fn refuser_tout_refuse_aussi_le_renouvellement() {
        // Échoue si `refuser_tout` laissait passer `POST /api/auth/refresh`
        // — exactement la distinction avec `refuser_le_premier_appel`, qui
        // elle épargne le renouvellement.
        let s = FauxServeur::demarrer();
        s.etat_mut().refuser_tout = true;

        let reponse = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(r#"{"refresh":"peu importe"}"#)
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 401);
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn renouvellement_rend_jeton_neuf_et_compte_lappel() {
        // Échoue si la valeur rendue n'est pas exactement "jeton-neuf" (ce
        // que la tâche 7 attend), ou si le compteur ne progresse pas.
        let s = FauxServeur::demarrer();
        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(r#"{"refresh":"peu importe"}"#)
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["acces"], "jeton-neuf");
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn depot_denveloppe_est_compte_et_relu_par_sync() {
        // Échoue si `depots_recus` ne progresse pas, ou si l'enveloppe
        // déposée ne réapparaît pas dans `GET /api/sky/sync` avec `id`
        // typé chaîne et les deux device_id typés nombre.
        let s = FauxServeur::demarrer();
        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .send_string(r#"{"expediteur_device_id":7,"destinataire_device_id":9,"charge":"YWJj"}"#)
            .unwrap();
        assert_eq!(reponse.status(), 204);
        assert_eq!(s.etat_mut().depots_recus, 1);

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        let enveloppe = &recu["enveloppes"][0];
        assert!(enveloppe["id"].is_string());
        assert_eq!(enveloppe["expediteur_device_id"], 7);
        assert_eq!(enveloppe["destinataire_device_id"], 9);
        assert_eq!(enveloppe["charge"], "YWJj");
    }

    #[test]
    fn sync_rend_inchange_a_version_connue_et_complet_sinon() {
        // Échoue si `{ inchange: true }` n'est jamais rendu, ou si il l'est
        // pour une version qui ne correspond pas à l'état courant.
        let s = FauxServeur::demarrer();
        s.etat_mut().version = 42;

        let a_jour: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=42", s.url()))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(a_jour, serde_json::json!({ "inchange": true }));

        let perime: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=1", s.url()))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(perime["version"], 42);
    }
}
