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

use std::collections::{HashMap, HashSet};
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

/// Demande d'ami reçue, telle que rendue par `GET /api/sky/sync` dans
/// `demandes[]` — forme figée par `Demande` (site : `src/lib/sky/amis.ts`,
/// fonction `demandesDe`). `friendshipId`/`demandeurId` en camelCase, le
/// reste en snake_case — même relevé de forme que `AmiFaux` ci-dessus, pas
/// une convention uniforme inventée pour ce double. AJOUT DE LA TÂCHE 8
/// (hors scope de la tâche 4) : la tâche 4 figeait ce tableau à `[]` en dur
/// ; sans lui, `accepter_ami` (tâche 8) est inutilisable — le client n'a
/// aucun moyen de connaître l'identifiant d'amitié à accepter.
#[derive(Debug, Clone, Serialize)]
pub struct DemandeFausse {
    #[serde(rename = "friendshipId")]
    pub friendship_id: i64,
    #[serde(rename = "demandeurId")]
    pub demandeur_id: i64,
    pub discord_name: String,
    pub discord_avatar: Option<String>,
    pub created_at: String,
}

/// Appareil de l'utilisateur COURANT, tel que rendu par `GET /api/sky/sync`
/// dans `appareils[]` — forme figée par `Appareil` (site :
/// `src/lib/sky/appareils.ts`). Ne porte PAS `public_key` : c'est la vue de
/// gestion « mes appareils », distincte de `AppareilDAmiFaux` ci-dessus.
/// AJOUT DE LA TÂCHE 8, même raison que `DemandeFausse`.
#[derive(Debug, Clone, Serialize)]
pub struct AppareilFaux {
    pub id: i64,
    pub nom: String,
    pub plateforme: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
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

/// Utilisateur courant tel que rendu par `GET /api/auth/me` — forme figée
/// par la route (`src/app/api/auth/me/route.ts`, côté site). AJOUT DE LA
/// TÂCHE 10 (hors brief de la tâche 4, extension autorisée par le
/// coordinateur — voir « T10 : source du nom tranchée » dans le journal de
/// progression) : `sky_compte::moi` n'a aucun autre moyen de connaître le
/// nom Discord affiché après connexion, ni `connecter` ni `synchroniser` ne
/// le portent.
///
/// Tous les champs en camelCase EXCEPT `id` (déjà sans casse à respecter) :
/// forme exacte de la réponse JSON du site, pas une convention uniforme
/// inventée pour ce double.
#[derive(Debug, Clone, Serialize)]
pub struct MoiFaux {
    pub id: i64,
    #[serde(rename = "discordId")]
    pub discord_id: String,
    #[serde(rename = "discordName")]
    pub discord_name: String,
    #[serde(rename = "discordAvatar")]
    pub discord_avatar: Option<String>,
    #[serde(rename = "discordEmail")]
    pub discord_email: Option<String>,
    #[serde(rename = "totpEnabled")]
    pub totp_enabled: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "isStaff")]
    pub is_staff: bool,
    #[serde(rename = "staffRole")]
    pub staff_role: Option<String>,
    #[serde(rename = "isInfluencer")]
    pub is_influencer: bool,
    #[serde(rename = "influencerCode")]
    pub influencer_code: Option<String>,
}

impl MoiFaux {
    /// Utilisateur minimal, pour les tests qui n'ont besoin que de `id` et
    /// `discord_name` — ce que `sky_compte::Moi` porte réellement.
    pub fn nouveau(id: i64, discord_name: &str) -> MoiFaux {
        MoiFaux {
            id,
            discord_id: id.to_string(),
            discord_name: discord_name.to_string(),
            discord_avatar: None,
            discord_email: None,
            totp_enabled: false,
            created_at: "2026-01-01T00:00:00.000Z".to_string(),
            is_staff: false,
            staff_role: None,
            is_influencer: false,
            influencer_code: None,
        }
    }
}

/// État piloté par les tests — les tâches suivantes du jalon (6, 7, 9)
/// mutent ces champs via `FauxServeur::etat_mut()` pour commander les
/// réponses du double, sans jamais toucher la production.
///
/// `amis`, `enveloppes` et `version` sont les trois champs exigés par le
/// brief de cette tâche. Les champs suivants sont un AJOUT délibéré,
/// tranché par le coordinateur en dehors du brief : les tâches 6, 7 et 9
/// ont déjà leurs tests écrits contre ces noms précis, et sky-compte ne
/// compilerait pas sans eux. Voir le commentaire de chacun pour le
/// raisonnement.
#[derive(Debug, Default)]
pub struct EtatFaux {
    pub amis: Vec<AmiFaux>,
    pub enveloppes: Vec<EnveloppeFausse>,
    pub version: u64,

    /// AJOUTS DE LA TÂCHE 8 (hors brief de la tâche 4, extension autorisée
    /// par le coordinateur — voir le commentaire de `DemandeFausse` et
    /// `AppareilFaux` ci-dessus) : `demandes` et `appareils` étaient figés
    /// en dur à `[]` par la tâche 4. Sans eux, `accepter_ami` n'a aucun
    /// identifiant d'amitié à consommer et `enregistrer_appareil` ne peut
    /// pas être vérifié bout en bout par `synchroniser`.
    pub demandes: Vec<DemandeFausse>,
    pub appareils: Vec<AppareilFaux>,

    /// `POST /api/sky/devices` — prochain identifiant rendu par un
    /// enregistrement réussi. Incrémenté à chaque succès (premier
    /// enregistrement -> 1), jamais réutilisé.
    pub prochain_id_appareil: i64,

    /// `POST /api/sky/friends` — couple (code, identifiant d'amitié résultant)
    /// accepté. `None` par défaut : AUCUN code n'est valide tant qu'un test
    /// ne l'a pas explicitement enregistré — même principe que
    /// `code_natif_valide`.
    pub code_ami_valide: Option<(String, i64)>,

    /// Fait répondre `409 {"error":"Demande déjà existante"}` à `POST
    /// /api/sky/friends` pour le code enregistré dans `code_ami_valide`, au
    /// lieu du succès normal — simule une demande déjà en cours.
    pub ami_deja_demande: bool,

    /// `POST /api/sky/friends/{id}/accept` — identifiant d'amitié qui peut
    /// être accepté avec succès. `None`, ou un identifiant différent de
    /// celui demandé, produit `404 {"error":"Amitié introuvable"}` — même
    /// refus indistinct que le site pour les trois causes qu'il recouvre
    /// (amitié inexistante, amitié d'autrui, amitié qu'on a soi-même
    /// demandée).
    pub amitie_acceptable: Option<i64>,

    /// Jetons que le double reconnaît sur une route authentifiée (`GET
    /// /api/sky/sync`, `POST /api/sky/envelopes`) — PAS une vérification
    /// de signature JWT (hors de portée d'un double : aucun secret partagé
    /// n'existe pour `sky-compte`), seulement la liste de ce que CE double
    /// a lui-même émis (`POST /api/auth/native` réussi, `POST
    /// /api/auth/refresh` réussi), enrichissable directement par un test
    /// via `FauxServeur::jeton_de_test()`. Un jeton absent d'ici — vide,
    /// aléatoire, périmé, ou simplement jamais délivré — est refusé
    /// exactement comme une absence totale de jeton (voir
    /// `autoriser_appel`) : la propriété utile est « un client qui
    /// n'envoie pas un jeton reconnu est détecté », pas « le jeton est
    /// cryptographiquement valide ».
    pub jetons_acceptes: HashSet<String>,

    /// Jetons de RENOUVELLEMENT que `POST /api/auth/refresh` reconnaît —
    /// même principe que `jetons_acceptes`, mais un espace de noms séparé :
    /// un jeton d'accès et un jeton de renouvellement ne doivent jamais
    /// être interchangeables (même distinction que le site, où
    /// `verifyJWT`/`verifyRefreshToken` sont deux vérifications séparées,
    /// `src/lib/auth/jwt.ts`). Peuplé par un `POST /api/auth/native`
    /// réussi, ou directement par un test via
    /// `FauxServeur::jeton_de_renouvellement_de_test()`.
    ///
    /// RONDE DE CORRECTION 2 : avant cette ronde, `POST /api/auth/refresh`
    /// ne lisait jamais son corps — n'importe quelle chaîne réussissait.
    /// Ce champ ferme cet angle mort SANS aller jusqu'à la vérification de
    /// signature JWT (exclue explicitement par le coordinateur) : la
    /// propriété fermée est « le jeton de renouvellement présenté est un
    /// jeton que CE double a lui-même délivré comme jeton de
    /// renouvellement », pas « il est cryptographiquement valide ».
    pub jetons_de_renouvellement_valides: HashSet<String>,

    /// Couple (code, secret) que `POST /api/auth/native` accepte —
    /// `None` par défaut : AUCUN code n'est valide tant qu'un test ne l'a
    /// pas explicitement enregistré ici, fidèle au test prescrit par le
    /// brief (`le_double_refuse_sans_distinguer`, qui ne configure jamais
    /// ce champ et doit continuer à voir un refus). Une tâche qui a
    /// besoin d'un login natif réussi (tâche 6, pour ranger des jetons
    /// dans le coffre) enregistre ce couple avant d'appeler `POST
    /// /api/auth/native` — voir
    /// `auth_native_reussit_avec_le_code_enregistre_et_le_jeton_fonctionne_ensuite`
    /// plus bas pour l'exemple complet.
    pub code_natif_valide: Option<(String, String)>,

    /// Fait répondre `401 {"error":"Code refuse"}` à `POST /api/auth/native`
    /// même si `code_natif_valide` correspond — un court-circuit explicite,
    /// distinct du refus par défaut (absence de `code_natif_valide`).
    pub refuser_echange: bool,

    /// Fait répondre 401 au PROCHAIN appel authentifié (`GET
    /// /api/sky/sync` ou `POST /api/sky/envelopes`), UNE SEULE FOIS :
    /// consommé (remis à `false`) dès qu'il a servi à refuser un appel.
    /// Sert à éprouver le renouvellement de jeton côté client : refus,
    /// renouvellement via `POST /api/auth/refresh`, nouvelle tentative
    /// réussie. S'applique APRÈS la vérification du jeton (voir
    /// `autoriser_appel`) : un appel qui présente déjà un jeton inconnu
    /// échoue pour cette raison-là, jamais pour celle-ci — les deux
    /// causes restent indépendamment pilotables et rendent des messages
    /// différents.
    pub refuser_le_premier_appel: bool,

    /// Refuse TOUT appel authentifié ET tout renouvellement — à la
    /// différence de `refuser_le_premier_appel`, qui épargne
    /// délibérément `POST /api/auth/refresh` (sans quoi éprouver le
    /// renouvellement lui-même serait impossible). Utile pour simuler un
    /// compte totalement révoqué. Même remarque que ci-dessus sur l'ordre
    /// des vérifications : un jeton inconnu échoue pour cette raison-là
    /// avant même que `refuser_tout` soit consulté.
    pub refuser_tout: bool,

    /// Nombre d'appels reçus sur `POST /api/auth/refresh`, acceptés ou
    /// refusés confondus — observable pour prouver qu'un client renouvelle
    /// une fois et retente, plutôt que de boucler indéfiniment.
    pub appels_de_renouvellement: u64,

    /// Fait échouer TOUT dépôt (`POST /api/sky/envelopes`) par ailleurs
    /// bien formé et authentifié, avec le refus UNIFORME et INDISTINCT
    /// `404 {"error":"Depot refuse"}` — la MÊME forme que le vrai serveur
    /// rend pour `pas_ami`, `appareil_inconnu` ET `pas_mon_appareil` (voir
    /// `deposer`, `src/lib/sky/enveloppes.ts` du site) : distinguer ces
    /// cas apprendrait à l'appelant qu'un appareil donné existe,
    /// exactement ce que la garde réelle referme.
    ///
    /// RONDE DE CORRECTION 2 : avant cette ronde, `gerer_depot` acceptait
    /// INCONDITIONNELLEMENT tout dépôt bien formé et authentifié — aucun
    /// chemin vers ce 404, alors même que la forme était déjà relevée à
    /// l'étape 1 du rapport. Ce double ne réplique pas le graphe d'amitié
    /// réel (`EtatFaux` ne modélise pas « mes propres appareils » côté
    /// expéditeur) : ce champ expose un interrupteur pilotable, dans le
    /// même esprit que `refuser_tout`/`refuser_echange`, pour qu'un test
    /// simule « ce dépôt est refusé » sans avoir à modéliser toute la
    /// logique métier — le double imite un contrat, il ne réimplémente pas
    /// le serveur.
    pub refuser_depot: bool,

    /// Nombre de dépôts d'enveloppe ACCEPTÉS par `POST /api/sky/envelopes`
    /// (forme valide, authentification passée) — observable pour les
    /// tests de boîte aux lettres des tâches suivantes.
    pub depots_recus: u64,

    /// Associe un jeton d'ACCÈS à l'appareil que `POST /api/sky/devices` a
    /// créé avec CE jeton — même lien que `devices.session_id` côté site
    /// (`appareils.ts::enregistrerAppareil`, posé à l'enregistrement), lu
    /// ensuite par `GET /api/sky/sync` pour résoudre `deviceIdCourant`
    /// (`sync/route.ts` : `UPDATE devices ... WHERE session_id = $1
    /// RETURNING id`).
    ///
    /// RONDE DE CORRECTION 1 (IMPORTANT 2 de task-9-review.md) : avant
    /// cette ronde, `gerer_sync` ne filtrait `enveloppes` par AUCUN
    /// appareil — tout jeton authentifié recevait la boîte aux lettres
    /// COMPLÈTE, y compris les enveloppes d'un autre appareil. Ce champ est
    /// ce qui permet à `gerer_sync` de reproduire la portée réelle
    /// (`deviceIdCourant`, `etat.ts` "RONDE DE CORRECTION 1 (§2)") : un
    /// jeton absent d'ici (aucun `POST /api/sky/devices` réussi avec lui)
    /// n'a, structurellement, aucun appareil courant — `enveloppes` vaut
    /// alors toujours `[]`, jamais celles d'un autre appareil, exactement
    /// comme une session sans application installée côté site.
    ///
    /// Un jeton peut réenregistrer un NOUVEL appareil (l'entrée est
    /// écrasée, jamais cumulée) : un second `POST /api/sky/devices` avec le
    /// même jeton fait pointer ce jeton vers le second appareil, comme un
    /// second appel réel à `enregistrerAppareil` sur la même session.
    pub jetons_appareil: HashMap<String, i64>,

    /// `GET /api/auth/me` — utilisateur rendu sur succès. `None` par défaut :
    /// AUCUN utilisateur n'est connu tant qu'un test ne l'a pas
    /// explicitement enregistré, ce qui fait rendre `404` (même principe que
    /// `code_ami_valide`, `amitie_acceptable`, etc.) — c'est aussi la forme
    /// EXACTE du refus réel : `findUserById` rend `null` si le compte a
    /// disparu entre l'émission du jeton et cet appel.
    pub moi: Option<MoiFaux>,

    /// Fait répondre `403 {"error":"TOTP verification required"}` à `GET
    /// /api/auth/me`, AVANT même de consulter `moi` — reproduit la garde de
    /// session partielle du site (`!session.totpVerified`), dont le message
    /// est en anglais côté site, sans conséquence pour un client qui ne lit
    /// jamais ce texte.
    pub session_partielle_totp: bool,
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

    /// Jeton pré-approuvé, pratique pour un test qui a seulement besoin
    /// d'un appel authentifié réussi sans passer par l'échange natif
    /// complet : l'insère dans `EtatFaux::jetons_acceptes` s'il n'y est
    /// pas déjà, puis le rend. Toujours la même valeur pour une instance
    /// donnée — appeler cette méthode plusieurs fois rend le même jeton.
    pub fn jeton_de_test(&self) -> String {
        const JETON: &str = "jeton-de-test";
        self.etat_mut().jetons_acceptes.insert(JETON.to_string());
        JETON.to_string()
    }

    /// Jeton d'accès pré-approuvé, DISTINCT de celui rendu par
    /// `jeton_de_test()` (et de tout autre `suffixe`) — AJOUT DE LA RONDE
    /// DE CORRECTION 1 (task-9-review.md, IMPORTANT 2) : prouver que le
    /// double isole les enveloppes par appareil exige de faire coexister
    /// DEUX appareils, donc deux jetons, dans le même `FauxServeur`.
    /// `jeton_de_test()` seule ne le permettait pas : elle rend toujours la
    /// même valeur, quel que soit le nombre d'appels.
    pub fn jeton_de_test_pour(&self, suffixe: &str) -> String {
        let jeton = format!("jeton-de-test-{suffixe}");
        self.etat_mut().jetons_acceptes.insert(jeton.clone());
        jeton
    }

    /// Jeton de RENOUVELLEMENT pré-approuvé — symétrique de
    /// `jeton_de_test()`, mais pour `POST /api/auth/refresh` plutôt que
    /// pour les routes authentifiées : l'insère dans
    /// `EtatFaux::jetons_de_renouvellement_valides` s'il n'y est pas déjà,
    /// puis le rend.
    pub fn jeton_de_renouvellement_de_test(&self) -> String {
        const JETON: &str = "jeton-de-renouvellement-de-test";
        self.etat_mut().jetons_de_renouvellement_valides.insert(JETON.to_string());
        JETON.to_string()
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
    let jeton = extraire_jeton_bearer(&requete);

    let mut corps_brut = String::new();
    if matches!(methode, Method::Post) {
        // Meilleur effort : un corps illisible devient une chaîne vide,
        // que chaque gestionnaire refuse alors comme un JSON invalide —
        // jamais de panique sur une requête malformée.
        let _ = requete.as_reader().read_to_string(&mut corps_brut);
    }

    let (statut, corps_reponse) = match (&methode, chemin.as_str()) {
        (Method::Get, "/api/sky/sync") => gerer_sync(etat, jeton.as_deref(), version_connue),
        (Method::Get, "/api/auth/me") => gerer_me(etat, jeton.as_deref()),
        (Method::Post, "/api/auth/native") => gerer_auth_native(etat, &corps_brut),
        (Method::Post, "/api/auth/refresh") => gerer_refresh(etat, &corps_brut),
        (Method::Post, "/api/sky/envelopes") => gerer_depot(etat, jeton.as_deref(), &corps_brut),
        (Method::Post, "/api/sky/devices") => gerer_devices(etat, jeton.as_deref(), &corps_brut),
        (Method::Post, "/api/sky/friends") => gerer_friends(etat, jeton.as_deref(), &corps_brut),
        (Method::Post, chemin_accept)
            if chemin_accept.starts_with("/api/sky/friends/") && chemin_accept.ends_with("/accept") =>
        {
            gerer_accepter_ami(etat, jeton.as_deref(), chemin_accept)
        }
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

/// Extrait le jeton d'un en-tête `Authorization: Bearer <jeton>`, ou `None`
/// si l'en-tête est absent, mal formé (préfixe autre que `Bearer `), ou
/// vide après le préfixe.
///
/// La comparaison du préfixe est insensible à la casse (`bearer`/`Bearer`),
/// même tolérance que `getSession` côté site
/// (`authHeader.toLowerCase().startsWith("bearer ")`,
/// `src/lib/auth/middleware.ts`) — un des rares points où imiter le site
/// coûte aussi peu que de l'ignorer.
fn extraire_jeton_bearer(requete: &tiny_http::Request) -> Option<String> {
    let en_tete = requete.headers().iter().find(|h| h.field.equiv("Authorization"))?;
    let valeur = en_tete.value.as_str();
    if valeur.len() < 7 || !valeur[..7].eq_ignore_ascii_case("bearer ") {
        return None;
    }
    let jeton = valeur[7..].trim();
    if jeton.is_empty() {
        None
    } else {
        Some(jeton.to_string())
    }
}

/// Décide si un appel à une route authentifiée du double (`GET
/// /api/sky/sync`, `POST /api/sky/envelopes`) peut passer, et pourquoi
/// sinon. Rend `Some((401, corps))` si l'appel doit être refusé, `None`
/// sinon.
///
/// DEUX CAUSES DE REFUS, DEUX MESSAGES, VOLONTAIREMENT DISTINCTS —
/// exigence explicite du coordinateur (task-4-report.md, ronde de
/// correction 1) : la tâche 7 doit pouvoir éprouver le renouvellement de
/// jeton, ce qui suppose de distinguer « ce jeton ne veut rien dire pour
/// moi » (pas de jeton, ou un jeton que ce double n'a jamais reconnu) de
/// « je refuse ce jeton pourtant reconnu, exprès, parce qu'un test me l'a
/// demandé » (`refuser_tout`/`refuser_le_premier_appel`). Un client qui
/// recevrait le MÊME message dans les deux cas ne pourrait pas savoir
/// s'il doit renouveler son jeton ou abandonner :
///
///  1. Jeton absent OU non reconnu (absent de
///     `EtatFaux::jetons_acceptes`) → `401 {"error":"Non authentifie"}`,
///     même forme que `handleAuthError` côté site
///     (`src/lib/auth/middleware.ts`) pour une session absente. C'est la
///     PREMIÈRE vérification : un jeton inconnu échoue pour cette raison,
///     jamais pour `refuser_tout`/`refuser_le_premier_appel`, qui ne sont
///     même pas consultés dans ce cas.
///  2. Jeton reconnu, mais `refuser_tout` (permanent) ou
///     `refuser_le_premier_appel` (un coup, consommé dès qu'il a servi)
///     est actif → `401 {"error":"Acces refuse"}` — message DIFFÉRENT du
///     précédent, à dessein : c'est ce qui permet à un test de vérifier
///     PRÉCISÉMENT laquelle des deux causes a produit le refus.
fn autoriser_appel(etat: &mut EtatFaux, jeton: Option<&str>) -> Option<(u16, String)> {
    let jeton_reconnu = match jeton {
        Some(j) => etat.jetons_acceptes.contains(j),
        None => false,
    };
    if !jeton_reconnu {
        return Some((401, json!({ "error": "Non authentifie" }).to_string()));
    }
    if etat.refuser_tout {
        return Some((401, json!({ "error": "Acces refuse" }).to_string()));
    }
    if etat.refuser_le_premier_appel {
        etat.refuser_le_premier_appel = false;
        return Some((401, json!({ "error": "Acces refuse" }).to_string()));
    }
    None
}

fn gerer_sync(
    etat: &Arc<Mutex<EtatFaux>>,
    jeton: Option<&str>,
    version_connue: Option<u64>,
) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }

    // RONDE DE CORRECTION 1 (IMPORTANT 2 de task-9-review.md) : l'appareil
    // COURANT est celui que `jetons_appareil` associe à CE jeton (posé par
    // `gerer_devices` au dernier `POST /api/sky/devices` réussi avec lui)
    // — même résolution que `deviceIdCourant` côté site, depuis
    // `devices.session_id`. `None` pour un jeton qui n'a jamais enregistré
    // d'appareil (session web sans application) : structurellement, un tel
    // jeton n'a AUCUNE enveloppe à recevoir, jamais celles d'un autre
    // appareil — voir `EtatFaux::jetons_appareil`.
    let appareil_courant = jeton.and_then(|j| e.jetons_appareil.get(j).copied());

    // Enveloppes dont CET appareil est le destinataire — jamais toutes
    // celles de `e.enveloppes`. AVANT cette ronde, `gerer_sync` rendait
    // `e.enveloppes` intégralement à tout jeton authentifié, quel que soit
    // l'appareil qui l'avait déposée : un jeton pouvait relever — et donc
    // EFFACER, la lecture est destructive — une enveloppe scellée pour la
    // clé d'un AUTRE appareil, perdue pour son vrai destinataire.
    let enveloppes_pour_cet_appareil: Vec<EnveloppeFausse> = match appareil_courant {
        Some(id) => e.enveloppes.iter().filter(|env| env.destinataire_device_id == id).cloned().collect(),
        None => Vec::new(),
    };

    // AJOUT DE LA TÂCHE 8, PORTÉE RESTREINTE PAR CETTE RONDE : une
    // enveloppe en attente COURT-CIRCUITE `inchange`, même règle que
    // `construireEtat` côté site (`enveloppe_en_attente || version !==
    // versionConnue`) — mais désormais SEULEMENT une enveloppe en attente
    // POUR CET APPAREIL, comme `enveloppe_en_attente` côté site
    // (`destinataire_device_id = $2` où `$2` est `deviceIdCourant`, jamais
    // « une enveloppe existe quelque part »). Sans ce court-circuit, une
    // enveloppe déposée entre deux synchronisations de MÊME version (le cas
    // courant — déposer une enveloppe ne touche que `enveloppes`, jamais
    // `version`) ne voyagerait plus jamais.
    if version_connue == Some(e.version) && enveloppes_pour_cet_appareil.is_empty() {
        return (200, json!({ "inchange": true }).to_string());
    }

    let corps = json!({
        "version": e.version,
        "code": CODE_FAUX,
        "amis": e.amis,
        "demandes": e.demandes,
        "listes": Vec::<Value>::new(),
        "appareils": e.appareils,
        "enveloppes": enveloppes_pour_cet_appareil,
    });

    // Le vrai serveur EFFACE les enveloppes en les livrant (`releverPour`,
    // `enveloppes.ts` côté site) — sans cette ligne, une même enveloppe
    // réapparaîtrait indéfiniment à chaque synchronisation suivante au lieu
    // de n'être vue qu'une fois. RONDE DE CORRECTION 1 : n'efface plus QUE
    // les enveloppes livrées à CET appareil — celles des autres restent en
    // attente, exactement comme `releverPour([deviceIdCourant])` côté site
    // ne touche jamais les lignes des autres appareils.
    if let Some(id) = appareil_courant {
        e.enveloppes.retain(|env| env.destinataire_device_id != id);
    }

    (200, corps.to_string())
}

/// `GET /api/auth/me` — AJOUT DE LA TÂCHE 10 (hors brief de la tâche 4).
/// Authentification par le même mécanisme que les autres routes protégées
/// (`autoriser_appel`), PUIS la garde de session partielle
/// (`session_partielle_totp`), PUIS `moi` — même ordre que le site
/// (`requireAuth` avant `!session.totpVerified` avant `findUserById`).
fn gerer_me(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }
    if e.session_partielle_totp {
        return (403, json!({ "error": "TOTP verification required" }).to_string());
    }
    match &e.moi {
        Some(moi) => (200, serde_json::to_string(moi).expect("MoiFaux se serialise toujours")),
        None => (404, json!({ "error": "Utilisateur introuvable" }).to_string()),
    }
}

/// Reproduit `nomValide` + `texteStockable` côté site (`devices/route.ts`,
/// `texte.ts`) : longueur en UNITÉS DE CODE UTF-16 (`encode_utf16().count()`),
/// PAS en caractères Unicode — `nom.length` en JavaScript compte ainsi, et
/// l'ancienne mesure de ce double (`chars().count()`) en divergeait dès
/// qu'un caractère hors du plan de base (la plupart des emoji : 2 unités
/// UTF-16, 1 seul `char`) apparaissait dans `nom`.
///
/// RONDE DE CORRECTION 1 : avant cette ronde, ce double n'appliquait NI
/// cette mesure NI le refus de l'octet NUL — un `nom` que le vrai serveur
/// refuse en 400 (`texteStockable`) était accepté en 201 ici, la même
/// classe de défaut que ce projet a déjà payée trois fois (voir
/// task-8-review.md, « Important 2 », et `CLAUDE.md`, « Validation des
/// entrées »).
///
/// Le substitut Unicode isolé (U+D800-U+DFFF), que `texteStockable` refuse
/// aussi côté site, n'est PAS vérifié ici : un `&str` Rust désérialisé
/// depuis un JSON par `serde_json` est garanti UTF-8 valide et ne peut
/// structurellement pas porter une telle valeur.
fn nom_appareil_valide(valeur: &str) -> bool {
    let longueur_utf16 = valeur.encode_utf16().count();
    (1..=64).contains(&longueur_utf16) && !valeur.contains('\0')
}

/// `POST /api/sky/devices` — valide `publicKey`/`nom`/`plateforme` comme le
/// vrai (`clePubliqueValide`, longueur de `nom`, liste blanche de
/// plateformes), incrémente `prochain_id_appareil` à chaque succès. AJOUT
/// DE LA TÂCHE 8 (hors brief de la tâche 4).
fn gerer_devices(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }

    let valeur: Value = match serde_json::from_str(corps_brut) {
        Ok(v) => v,
        Err(_) => return (400, json!({ "error": "Corps JSON invalide" }).to_string()),
    };
    let public_key = valeur.get("publicKey").and_then(Value::as_str);
    let nom = valeur.get("nom").and_then(Value::as_str);
    let plateforme = valeur.get("plateforme").and_then(Value::as_str);

    let Some(_public_key) = public_key.filter(|v| cle_publique_valide(v)) else {
        return (400, json!({ "error": "publicKey invalide : attendu 32 octets en base64" }).to_string());
    };
    let Some(nom) = nom.filter(|v| nom_appareil_valide(v)) else {
        return (400, json!({ "error": "nom invalide : attendu 1 a 64 caracteres" }).to_string());
    };
    const PLATEFORMES_VALIDES: [&str; 3] = ["windows", "macos", "linux"];
    let Some(plateforme) = plateforme.filter(|v| PLATEFORMES_VALIDES.contains(v)) else {
        return (
            400,
            json!({ "error": format!("plateforme invalide : attendu {}", PLATEFORMES_VALIDES.join(", ")) })
                .to_string(),
        );
    };
    e.prochain_id_appareil += 1;
    let appareil = AppareilFaux {
        id: e.prochain_id_appareil,
        nom: nom.to_string(),
        plateforme: plateforme.to_string(),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_seen_at: None,
        revoked_at: None,
    };

    // RONDE DE CORRECTION 1 (IMPORTANT 2) : c'est ICI, au succès de
    // l'enregistrement, que le jeton présenté est lié à l'appareil qu'il
    // vient de créer — même moment que le site (`devices.session_id` posé
    // par `enregistrerAppareil`, `appareils.ts`). `gerer_sync` lit cette
    // association pour ne livrer que les enveloppes de CET appareil — voir
    // le commentaire de `EtatFaux::jetons_appareil`.
    if let Some(jeton) = jeton {
        e.jetons_appareil.insert(jeton.to_string(), appareil.id);
    }

    let corps = serde_json::to_string(&appareil).expect("AppareilFaux se sérialise toujours");
    (201, corps)
}

/// Alphabet réel des codes ami — même valeur que `ALPHABET` côté site
/// (`src/lib/sky/codes.ts`) : sans `I`, `L`, `O`, `0`, `1`, pour éviter
/// l'ambiguïté visuelle. NE JAMAIS CHANGER (invaliderait les codes déjà
/// émis en production, voir `CLAUDE.md` du dépôt) — recopiée ici, pas
/// importée : ce double ne dépend d'aucun code du site.
const ALPHABET_CODE_AMI: &str = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Même règle que `codeValide` côté site : exactement 8 caractères, tous
/// dans `ALPHABET_CODE_AMI`.
fn code_ami_valide_forme(brut: &str) -> bool {
    brut.chars().count() == 8 && brut.chars().all(|c| ALPHABET_CODE_AMI.contains(c))
}

/// Ramène une saisie à sa forme canonique, ou `None` — même règle que
/// `normaliserCode` côté site (`codes.ts`) : espaces de bord retirés, mise
/// en capitales, préfixe `SKY-` retiré s'il est présent, puis tous les
/// tirets retirés. `None` si le résultat n'a pas la forme d'un code valide.
///
/// RONDE DE CORRECTION 1 : avant cette ronde, `gerer_friends` ne
/// reproduisait NI cette normalisation NI `code_ami_valide_forme` — un
/// code hors alphabet (ex. `"INCONNU1"`, qui contient `I`, `O` et `1`)
/// recevait un 404 métier du double alors que le vrai serveur rend un 400
/// de forme AVANT même de chercher le code (`friends/route.ts`, lignes
/// 25-56). Le test `ajouter_ami_avec_un_code_inconnu_...` prouvait alors
/// l'inverse de ce qui se produit en production pour cette entrée précise
/// — voir task-8-review.md, « Important 1 ».
fn normaliser_code_ami(brut: &str) -> Option<String> {
    let majuscules = brut.trim().to_uppercase();
    let sans_prefixe = majuscules.strip_prefix("SKY-").unwrap_or(&majuscules);
    let sans_tirets: String = sans_prefixe.chars().filter(|&c| c != '-').collect();
    if code_ami_valide_forme(&sans_tirets) {
        Some(sans_tirets)
    } else {
        None
    }
}

/// Décode `valeur` en base64 standard et vérifie qu'il s'agit de 32 octets
/// en forme canonique — même règle que `clePubliqueValide` côté site
/// (`src/app/api/sky/devices/route.ts`) : ce double ne doit pas être plus
/// permissif que le vrai serveur.
fn cle_publique_valide(valeur: &str) -> bool {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    match STANDARD.decode(valeur) {
        Ok(octets) => octets.len() == 32 && STANDARD.encode(&octets) == valeur,
        Err(_) => false,
    }
}

/// `POST /api/sky/friends` — refuse par défaut (`404`), sauf correspondance
/// exacte avec `code_ami_valide`, auquel cas `ami_deja_demande` bascule
/// entre succès (`201`) et conflit (`409`). AJOUT DE LA TÂCHE 8.
fn gerer_friends(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }

    let valeur: Value = match serde_json::from_str(corps_brut) {
        Ok(v) => v,
        Err(_) => return (400, json!({ "error": "Corps JSON invalide" }).to_string()),
    };
    let Some(code_brut) = valeur.get("code").and_then(Value::as_str) else {
        return (400, json!({ "error": "code invalide : attendu une chaîne" }).to_string());
    };

    // Normalisation ET validation de forme ICI, avant toute comparaison au
    // code piloté — même endroit et même ordre que le site
    // (`friends/route.ts`) : c'est ce qui distingue le 400 (forme
    // invalide) du 404 (forme correcte, code inconnu).
    let Some(code) = normaliser_code_ami(code_brut) else {
        return (400, json!({ "error": "code invalide : forme inattendue" }).to_string());
    };
    let code = code.as_str();

    match &e.code_ami_valide {
        Some((c, id)) if c == code => {
            if e.ami_deja_demande {
                (409, json!({ "error": "Demande déjà existante" }).to_string())
            } else {
                (201, json!({ "id": id }).to_string())
            }
        }
        _ => (404, json!({ "error": "Code ami introuvable" }).to_string()),
    }
}

/// `POST /api/sky/friends/{id}/accept` — l'identifiant voyage dans le
/// chemin, la route ne lit aucun corps (même contrat que le site). AJOUT DE
/// LA TÂCHE 8.
///
/// RONDE DE CORRECTION 1 (Mineur) : la forme de l'identifiant est
/// maintenant validée — un entier strictement positif, comme le site
/// (`friends/[id]/accept/route.ts` : `Number.isInteger(x) && x > 0`),
/// AVANT toute comparaison à `amitie_acceptable`. Un identifiant mal formé
/// (non numérique, négatif, nul, décimal) rend désormais 400, pas le même
/// 404 qu'un identifiant bien formé mais inexistant — voir
/// task-8-review.md, « Mineur 3 ».
fn gerer_accepter_ami(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }

    let segment = chemin.strip_prefix("/api/sky/friends/").and_then(|s| s.strip_suffix("/accept"));
    let id = segment.and_then(|s| s.parse::<i64>().ok()).filter(|id| *id > 0);

    let Some(id) = id else {
        return (400, json!({ "error": "Identifiant invalide" }).to_string());
    };

    match e.amitie_acceptable {
        Some(acceptable) if id == acceptable => (200, json!({ "ok": true }).to_string()),
        _ => (404, json!({ "error": "Amitié introuvable" }).to_string()),
    }
}

/// `POST /api/auth/native` — refuse toujours SAUF si `code`/`secret`
/// correspondent EXACTEMENT au couple enregistré dans
/// `EtatFaux::code_natif_valide` (voir son commentaire). Un succès émet un
/// jeton d'accès et un jeton de renouvellement, tous deux fabriqués (pas
/// de JWT réel), et enregistre chacun dans l'ensemble reconnu qui lui
/// correspond (`jetons_acceptes` / `jetons_de_renouvellement_valides`) —
/// c'est ce qui permet au premier de servir sur `GET /api/sky/sync`/`POST
/// /api/sky/envelopes`, et au second sur `POST /api/auth/refresh`.
///
/// ORDRE DES VÉRIFICATIONS — forme AVANT refus, comme le vrai
/// (`api/auth/native/route.ts` : `typeof code !== "string" || typeof
/// secret !== "string"` sort en 400 avant même d'appeler `echangerCode`) :
///
///  1. `code`/`secret` doivent être des chaînes présentes dans le corps →
///     sinon `400 {"error":"Requete invalide"}`, MÊME SI
///     `refuser_echange`/`refuser_tout` sont actifs — un corps malformé
///     n'atteint jamais la logique de refus, exactement comme le site
///     n'appelle jamais `echangerCode` sur un corps mal typé.
///  2. `refuser_echange`/`refuser_tout`, ou absence de correspondance avec
///     `code_natif_valide` → `401 {"error":"Code refuse"}`, forme
///     confirmée par le test prescrit par le brief
///     (`le_double_refuse_sans_distinguer`), qui ne configure jamais
///     `code_natif_valide` et doit continuer à voir ce refus.
///
/// Forme de succès `{ acces, refresh }` confirmée par
/// `api/auth/native/route.ts`.
fn gerer_auth_native(etat: &Arc<Mutex<EtatFaux>>, corps_brut: &str) -> (u16, String) {
    let valeur: Option<Value> = serde_json::from_str(corps_brut).ok();
    let code = valeur.as_ref().and_then(|v| v.get("code")).and_then(Value::as_str);
    let secret = valeur.as_ref().and_then(|v| v.get("secret")).and_then(Value::as_str);

    let (Some(code), Some(secret)) = (code, secret) else {
        return (400, json!({ "error": "Requete invalide" }).to_string());
    };

    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    const REFUS: &str = r#"{"error":"Code refuse"}"#;

    if e.refuser_echange || e.refuser_tout {
        return (401, REFUS.to_string());
    }

    let accepte = match &e.code_natif_valide {
        Some((c, s)) => c == code && s == secret,
        None => false,
    };
    if !accepte {
        return (401, REFUS.to_string());
    }

    // Valeurs fabriquées par le double, pas relevées du site : aucun test
    // ne dépend de leur contenu précis, seulement de leur capacité à
    // authentifier un appel ultérieur (voir
    // `auth_native_reussit_avec_le_code_enregistre_et_le_jeton_fonctionne_ensuite`).
    let acces = "jeton-acces-natif".to_string();
    let refresh = "jeton-refresh-natif".to_string();
    e.jetons_acceptes.insert(acces.clone());
    e.jetons_de_renouvellement_valides.insert(refresh.clone());
    (200, json!({ "acces": acces, "refresh": refresh }).to_string())
}

/// `POST /api/auth/refresh` — succès à un seul champ `acces` (jamais
/// `refresh` en retour, forme confirmée par `api/auth/refresh/route.ts`).
/// La valeur `"jeton-neuf"` est un CHOIX DU DOUBLE, pas une forme relevée
/// du site : c'est la valeur que les tests de la tâche 7 attendent
/// (demande explicite du coordinateur). Enregistrée dans
/// `jetons_acceptes` au succès, pour qu'un client puisse effectivement
/// s'en servir sur l'appel suivant — sans quoi « renouveler » ne
/// prouverait rien.
///
/// RONDE DE CORRECTION 2 — le champ `refresh` du corps est maintenant
/// EXIGÉ et VÉRIFIÉ contre `EtatFaux::jetons_de_renouvellement_valides` :
/// avant cette ronde, n'importe quelle chaîne (ou son absence) réussissait
/// tant que `refuser_tout` n'était pas actif. Toujours PAS de vérification
/// de signature (exclue explicitement par le coordinateur) — seulement
/// « ce double a-t-il lui-même délivré ce jeton de renouvellement ».
/// `appels_de_renouvellement` compte TOUT appel reçu, forme invalide
/// comprise : c'est un compteur d'appels, pas de succès.
fn gerer_refresh(etat: &Arc<Mutex<EtatFaux>>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_de_renouvellement += 1;

    let valeur: Option<Value> = serde_json::from_str(corps_brut).ok();
    let refresh = valeur.as_ref().and_then(|v| v.get("refresh")).and_then(Value::as_str);

    let Some(refresh) = refresh else {
        return (400, json!({ "error": "Requete invalide" }).to_string());
    };

    if e.refuser_tout || !e.jetons_de_renouvellement_valides.contains(refresh) {
        return (401, json!({ "error": "Renouvellement refuse" }).to_string());
    }

    e.jetons_acceptes.insert("jeton-neuf".to_string());
    (200, json!({ "acces": "jeton-neuf" }).to_string())
}

/// Taille maximale d'une charge SCELLÉE (octets décodés) — même valeur que
/// `TAILLE_CHARGE_MAX` côté site (`src/lib/sky/enveloppes.ts`).
const TAILLE_CHARGE_MAX: usize = 4096;

/// Estime le nombre d'octets qu'un texte base64 décodera, SANS décoder —
/// reproduction EXACTE de `octetsBase64Estimes` (`envelopes/route.ts`) :
/// arithmétique pure sur la longueur, padding standard géré ('=' final).
/// AJOUT DE LA TÂCHE 9 : avant elle, ce double n'avait AUCUN contrôle de
/// taille — un double plus indulgent que le vrai serveur sur ce point
/// précis aurait laissé passer, sans jamais rougir, une charge que la vraie
/// route refuse par son 404 uniforme.
fn octets_base64_estimes(base64: &str) -> usize {
    if base64.is_empty() {
        return 0;
    }
    let mut rembourrage = 0;
    if base64.ends_with("==") {
        rembourrage = 2;
    } else if base64.ends_with('=') {
        rembourrage = 1;
    }
    (base64.len() * 3) / 4 - rembourrage
}

/// `POST /api/sky/envelopes` — succès `204` sans corps (forme confirmée par
/// `envelopes/route.ts`), refus de forme `400` (un par champ, AJOUT DE LA
/// TÂCHE 9 — voir ci-dessous), refus authentifié partagé avec `GET
/// /api/sky/sync` via `autoriser_appel`, refus sémantique UNIFORME `404`
/// piloté par `EtatFaux::refuser_depot` OU par une charge scellée trop
/// grosse (voir son commentaire et `octets_base64_estimes` — RONDE DE
/// CORRECTION 2 puis TÂCHE 9).
///
/// ORDRE, comme le vrai (`deposer`, `enveloppes.ts`, et la route elle-même) :
/// authentification, PUIS forme du corps (identifiants strictement positifs,
/// charge non vide), PUIS taille estimée, PUIS refus sémantique piloté — un
/// corps malformé rend toujours 400 avant que `refuser_depot` ou la taille
/// ne soient même consultés.
///
/// TÂCHE 9 — TROIS REFUS AJOUTÉS, le double étant auparavant plus indulgent
/// que le site sur ces points précis : un identifiant nul ou négatif était
/// accepté (le vrai exige `Number.isInteger(x) && x > 0`), une charge vide
/// l'était aussi (le vrai exige `charge.length > 0`), et aucune taille
/// n'était jamais vérifiée. Un double plus permissif que le serveur qu'il
/// imite donne une confiance imméritée aux tests qui le pilotent — c'est
/// exactement la classe de défaut qui a déjà failli contaminer quatre tâches
/// de ce jalon (voir `CLAUDE.md`, « Uniformité des refus »).
fn gerer_depot(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }

    let valeur: Value = match serde_json::from_str(corps_brut) {
        Ok(v) => v,
        Err(_) => return (400, json!({ "error": "Corps JSON invalide" }).to_string()),
    };
    let expediteur = valeur.get("expediteur_device_id").and_then(Value::as_i64);
    let destinataire = valeur.get("destinataire_device_id").and_then(Value::as_i64);
    let charge = valeur.get("charge").and_then(Value::as_str).map(str::to_string);

    let Some(expediteur) = expediteur.filter(|v| *v > 0) else {
        return (
            400,
            json!({ "error": "expediteur_device_id invalide : attendu un entier positif" }).to_string(),
        );
    };
    let Some(destinataire) = destinataire.filter(|v| *v > 0) else {
        return (
            400,
            json!({ "error": "destinataire_device_id invalide : attendu un entier positif" }).to_string(),
        );
    };
    let Some(charge) = charge.filter(|v| !v.is_empty()) else {
        return (
            400,
            json!({ "error": "charge invalide : attendu une chaine base64 non vide" }).to_string(),
        );
    };

    if octets_base64_estimes(&charge) > TAILLE_CHARGE_MAX || e.refuser_depot {
        // Refus UNIFORME et INDISTINCT — jamais de détail sur LEQUEL de
        // pas_ami/appareil_inconnu/pas_mon_appareil/trop_gros s'appliquerait,
        // même principe que POST /api/auth/native pour "Code refuse".
        return (404, json!({ "error": "Depot refuse" }).to_string());
    }

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
    use base64::Engine;

    // « Qu'est-ce qui, précisément, ferait échouer ce test ? » Réponse pour
    // chacun des tests ci-dessous en commentaire, avec la neutralisation
    // pratiquée pour le prouver — voir task-4-report.md pour le relevé
    // complet des neutralisations et de leur rougissement.

    /// Enregistre un appareil via `POST /api/sky/devices` avec `jeton`, en
    /// HTTP brut (pas via `sky_compte`, ces tests visent le double
    /// directement) — AJOUT DE LA RONDE DE CORRECTION 1 (IMPORTANT 2) :
    /// plusieurs tests de ce module doivent maintenant faire correspondre
    /// un jeton à un appareil AVANT de vérifier qu'une enveloppe le
    /// rejoint, puisque `gerer_sync` ne livre plus qu'à l'appareil associé
    /// (`EtatFaux::jetons_appareil`). Rend l'identifiant attribué par le
    /// double, à utiliser comme `destinataire_device_id`.
    fn enregistrer_appareil_http(s: &FauxServeur, jeton: &str) -> i64 {
        let cle = base64::engine::general_purpose::STANDARD.encode([5u8; 32]);
        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(r#"{{"publicKey":"{cle}","nom":"Appareil de test","plateforme":"windows"}}"#))
            .expect("enregistrement d'appareil attendu en succes dans ce test")
            .into_json()
            .expect("reponse JSON attendue");
        recu["id"].as_i64().expect("id attendu comme entier")
    }

    #[test]
    fn le_double_rend_un_tableau_vide_jamais_absent() {
        // Échoue si `appareils` est sérialisé absent (Option::None -> champ
        // manquant) plutôt que `[]`, ou si AmiFaux ne construit pas un
        // tableau vide par défaut.
        let s = FauxServeur::demarrer();
        s.etat_mut().amis.push(AmiFaux::sans_appareil("bob"));
        let jeton = s.jeton_de_test();

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
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
        // refuserait). Jeton reconnu attaché aux deux appels : le refus
        // testé ici doit venir de `refuser_le_premier_appel`, jamais d'un
        // jeton absent ou inconnu (voir `sync_exige_un_jeton` pour cette
        // autre cause).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().refuser_le_premier_appel = true;

        let premier = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        let reponse_premier = premier.unwrap_err().into_response().unwrap();
        assert_eq!(reponse_premier.status(), 401);
        assert_eq!(reponse_premier.into_string().unwrap(), r#"{"error":"Acces refuse"}"#);

        let second = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        assert_eq!(second.unwrap().status(), 200);
    }

    #[test]
    fn sync_exige_un_jeton() {
        // Échoue si `GET /api/sky/sync` répondait avec succès sans aucun
        // en-tête `Authorization` — exactement la réserve fermée par cette
        // ronde de correction : un client qui oublierait son jeton ne doit
        // plus jamais passer contre ce double.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url())).call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn sync_refuse_un_jeton_inconnu() {
        // Échoue si un jeton quelconque, jamais délivré par ce double,
        // était accepté — distinct du test précédent (jeton ABSENT) : ici
        // un en-tête est bien présent, mais ne correspond à rien.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", "Bearer nimporte-quoi")
            .call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn sync_accepte_un_jeton_reconnu() {
        // Échoue si un jeton pourtant présent dans `jetons_acceptes`
        // était quand même refusé — la contre-preuve des deux tests
        // précédents : le rejet vient bien de la reconnaissance du jeton,
        // pas d'un refus systématique de toute authentification.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call();
        assert_eq!(reponse.unwrap().status(), 200);
    }

    #[test]
    fn refuser_tout_refuse_aussi_le_renouvellement() {
        // Échoue si `refuser_tout` laissait passer `POST /api/auth/refresh`
        // — exactement la distinction avec `refuser_le_premier_appel`, qui
        // elle épargne le renouvellement. Jeton de renouvellement RECONNU
        // attaché : le refus testé ici doit venir de `refuser_tout`, jamais
        // d'un jeton de renouvellement inconnu (voir
        // `refresh_refuse_un_jeton_de_renouvellement_inconnu`).
        let s = FauxServeur::demarrer();
        let jeton_renouvellement = s.jeton_de_renouvellement_de_test();
        s.etat_mut().refuser_tout = true;

        let reponse = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&format!(r#"{{"refresh":"{jeton_renouvellement}"}}"#))
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 401);
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn renouvellement_rend_jeton_neuf_et_compte_lappel() {
        // Échoue si la valeur rendue n'est pas exactement "jeton-neuf" (ce
        // que la tâche 7 attend), ou si le compteur ne progresse pas.
        let s = FauxServeur::demarrer();
        let jeton_renouvellement = s.jeton_de_renouvellement_de_test();
        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(&format!(r#"{{"refresh":"{jeton_renouvellement}"}}"#))
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["acces"], "jeton-neuf");
        assert_eq!(s.etat_mut().appels_de_renouvellement, 1);
    }

    #[test]
    fn refresh_refuse_un_jeton_de_renouvellement_inconnu() {
        // Échoue si un jeton de renouvellement quelconque, jamais délivré
        // par ce double, était accepté — c'est l'angle mort signalé par le
        // relecteur : avant cette ronde, N'IMPORTE QUELLE chaîne réussissait
        // ici tant que `refuser_tout` n'était pas actif.
        let s = FauxServeur::demarrer();
        let reponse = ureq::post(&format!("{}/api/auth/refresh", s.url()))
            .send_string(r#"{"refresh":"peu importe"}"#)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Renouvellement refuse"}"#);
    }

    #[test]
    fn refresh_400_sur_champ_manquant() {
        // Échoue si un corps sans `refresh` (ou dont `refresh` n'est pas
        // une chaîne) produisait autre chose qu'un 400 — même ordre que le
        // site : la forme est vérifiée AVANT toute décision de refus.
        let s = FauxServeur::demarrer();
        for corps in [r#"{}"#, r#"{"refresh":42}"#, r#"pas du json"#] {
            let reponse =
                ureq::post(&format!("{}/api/auth/refresh", s.url())).send_string(corps).unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "corps testé : {corps}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Requete invalide"}"#);
        }
    }

    #[test]
    fn depot_denveloppe_est_compte_et_relu_par_sync() {
        // Échoue si `depots_recus` ne progresse pas, ou si l'enveloppe
        // déposée ne réapparaît pas dans `GET /api/sky/sync` avec `id`
        // typé chaîne et les deux device_id typés nombre.
        //
        // RONDE DE CORRECTION 1 : le jeton qui relève doit désormais avoir
        // un appareil associé (`jetons_appareil`) — sans quoi `gerer_sync`
        // rend `enveloppes: []` par construction. `enregistrer_appareil_http`
        // fournit cet appareil et son identifiant réel, utilisé ci-dessous
        // comme `destinataire_device_id` au lieu d'un `9` arbitraire.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(
                r#"{{"expediteur_device_id":7,"destinataire_device_id":{id_appareil},"charge":"YWJj"}}"#
            ))
            .unwrap();
        assert_eq!(reponse.status(), 204);
        assert_eq!(s.etat_mut().depots_recus, 1);

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        let enveloppe = &recu["enveloppes"][0];
        assert!(enveloppe["id"].is_string());
        assert_eq!(enveloppe["expediteur_device_id"], 7);
        assert_eq!(enveloppe["destinataire_device_id"], id_appareil);
        assert_eq!(enveloppe["charge"], "YWJj");
    }

    #[test]
    fn depot_denveloppe_exige_aussi_un_jeton() {
        // Échoue si `POST /api/sky/envelopes` acceptait un dépôt sans
        // authentification — même réserve que `sync_exige_un_jeton`, sur
        // l'AUTRE route authentifiée du double.
        let s = FauxServeur::demarrer();
        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .send_string(r#"{"expediteur_device_id":1,"destinataire_device_id":2,"charge":"YQ=="}"#);
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        // Le dépôt n'a pas dû être compté : le refus d'authentification
        // précède toute lecture du corps.
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn sync_rend_inchange_a_version_connue_et_complet_sinon() {
        // Échoue si `{ inchange: true }` n'est jamais rendu, ou si il l'est
        // pour une version qui ne correspond pas à l'état courant.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().version = 42;

        let a_jour: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=42", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(a_jour, serde_json::json!({ "inchange": true }));

        let perime: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=1", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(perime["version"], 42);
    }

    #[test]
    fn auth_native_reussit_avec_le_code_enregistre_et_le_jeton_fonctionne_ensuite() {
        // Échoue si un couple (code, secret) enregistré dans
        // `code_natif_valide` ne produisait pas un succès, ou si le jeton
        // d'accès reçu n'était pas ensuite reconnu par une route
        // authentifiée — la moitié qui prouve que ce chemin de succès sert
        // réellement à quelque chose (tâche 6 : ranger un jeton utilisable
        // dans le coffre).
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        let recu: serde_json::Value = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(r#"{"code":"CODEVALIDE","secret":"secret-correct"}"#)
            .unwrap()
            .into_json()
            .unwrap();
        let acces = recu["acces"].as_str().expect("acces attendu comme chaine").to_string();
        assert!(!acces.is_empty());
        assert!(recu["refresh"].is_string());

        let reponse = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {acces}"))
            .call();
        assert_eq!(reponse.unwrap().status(), 200);
    }

    #[test]
    fn auth_native_refuse_un_secret_incorrect_meme_code_enregistre() {
        // Échoue si la comparaison acceptait un secret différent de celui
        // enregistré — le couple doit correspondre EXACTEMENT, pas
        // seulement le code.
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        let e = ureq::post(&format!("{}/api/auth/native", s.url()))
            .send_string(r#"{"code":"CODEVALIDE","secret":"mauvais-secret"}"#)
            .unwrap_err();
        let reponse = match e {
            ureq::Error::Status(_, r) => r,
            _ => panic!("attendu 401"),
        };
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Code refuse"}"#);
    }

    #[test]
    fn auth_native_400_sur_corps_malforme() {
        // Échoue si un corps où `code`/`secret` sont absents ou ne sont
        // pas des chaînes produisait un 401 plutôt qu'un 400 — le vrai
        // serveur distingue les deux (`typeof code !== "string" ...` sort
        // en 400 AVANT tout appel à `echangerCode`), ce double doit faire
        // pareil. Même avec `code_natif_valide` enregistré, ces corps ne
        // doivent jamais atteindre la comparaison : sinon un attaquant qui
        // envoie un `code` numérique apprendrait quelque chose du timing.
        let s = FauxServeur::demarrer();
        s.etat_mut().code_natif_valide =
            Some(("CODEVALIDE".to_string(), "secret-correct".to_string()));

        for corps in [
            r#"{"code":42,"secret":"secret-correct"}"#,
            r#"{"code":"CODEVALIDE","secret":42}"#,
            r#"{"code":"CODEVALIDE"}"#,
            r#"{}"#,
            r#"pas du json"#,
        ] {
            let reponse =
                ureq::post(&format!("{}/api/auth/native", s.url())).send_string(corps).unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "corps testé : {corps}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Requete invalide"}"#);
        }
    }

    #[test]
    fn depot_refuse_un_identifiant_expediteur_non_positif() {
        // AJOUT DE LA TÂCHE 9 : avant elle, `gerer_depot` acceptait
        // n'importe quel entier, y compris nul ou négatif. Échoue si le
        // double redevenait plus permissif que le site
        // (`idAppareilValide`, `envelopes/route.ts`).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        for id in [0i64, -1] {
            let corps =
                json!({"expediteur_device_id": id, "destinataire_device_id": 2, "charge": "YQ=="})
                    .to_string();
            let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string(&corps)
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(
                reponse.into_string().unwrap(),
                r#"{"error":"expediteur_device_id invalide : attendu un entier positif"}"#
            );
        }
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_un_identifiant_destinataire_non_positif() {
        // Même réserve que le test précédent, sur l'AUTRE champ — les deux
        // messages sont volontairement distincts (voir la route réelle),
        // donc les deux gardes doivent être vérifiées séparément.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        for id in [0i64, -1] {
            let corps =
                json!({"expediteur_device_id": 1, "destinataire_device_id": id, "charge": "YQ=="})
                    .to_string();
            let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string(&corps)
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(
                reponse.into_string().unwrap(),
                r#"{"error":"destinataire_device_id invalide : attendu un entier positif"}"#
            );
        }
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_une_charge_vide() {
        // AJOUT DE LA TÂCHE 9 : avant elle, une chaîne vide décodait vers 0
        // octet et était acceptée sans réserve — le site refuse
        // explicitement `charge.length === 0`.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let corps = json!({"expediteur_device_id": 1, "destinataire_device_id": 2, "charge": ""}).to_string();

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
        assert_eq!(
            reponse.into_string().unwrap(),
            r#"{"error":"charge invalide : attendu une chaine base64 non vide"}"#
        );
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_une_charge_scellee_trop_grosse_avec_le_404_uniforme() {
        // AJOUT DE LA TÂCHE 9 : avant elle, aucune taille n'était jamais
        // vérifiée. 4097 octets décodés dépasse `TAILLE_CHARGE_MAX` (4096)
        // d'exactement un octet — le refus doit être le MÊME 404 uniforme
        // que `refuser_depot`, jamais un message distinct qui dirait
        // "trop gros" (voir le commentaire de `gerer_depot`).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let charge_trop_grosse = base64::engine::general_purpose::STANDARD.encode(vec![0u8; 4097]);
        let corps = json!({
            "expediteur_device_id": 1,
            "destinataire_device_id": 2,
            "charge": charge_trop_grosse,
        })
        .to_string();

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Depot refuse"}"#);
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    #[test]
    fn depot_refuse_de_maniere_uniforme_quand_pilote() {
        // Échoue si `refuser_depot` ne produisait pas le refus UNIFORME
        // `404 {"error":"Depot refuse"}`, ou si le dépôt refusé était quand
        // même compté dans `depots_recus` — c'était le trou signalé par le
        // relecteur : avant cette ronde, aucun chemin ne menait à ce 404.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().refuser_depot = true;

        let reponse = ureq::post(&format!("{}/api/sky/envelopes", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"expediteur_device_id":1,"destinataire_device_id":2,"charge":"YQ=="}"#)
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Depot refuse"}"#);
        assert_eq!(s.etat_mut().depots_recus, 0);
    }

    // --- AJOUTS DE LA TÂCHE 8 (extension du double, brief le permet
    // explicitement — voir le commentaire de `EtatFaux`) -------------------

    #[test]
    fn sync_court_circuite_inchange_quand_une_enveloppe_attend() {
        // LE PIÈGE SIGNALÉ PAR LE CAHIER DES CHARGES DE LA TÂCHE 8 : une
        // enveloppe déposée entre deux appels à VERSION INCHANGÉE doit
        // continuer à voyager. Échoue si `gerer_sync` répondait `inchange`
        // dès que `version_connue == e.version`, sans regarder si une
        // enveloppe attend encore.
        //
        // RONDE DE CORRECTION 1 : le jeton qui synchronise doit avoir un
        // appareil associé — le court-circuit ne regarde désormais QUE les
        // enveloppes de CET appareil (voir `EtatFaux::jetons_appareil`),
        // donc l'enveloppe poussée ci-dessous vise l'identifiant que le
        // double vient réellement d'attribuer, pas un `2` arbitraire.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_appareil,
            charge: "YQ==".to_string(),
        });

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_ne!(recu, serde_json::json!({ "inchange": true }));
        assert_eq!(recu["enveloppes"][0]["id"], "1");
    }

    #[test]
    fn sync_ne_court_circuite_pas_inchange_pour_une_enveloppe_dun_autre_appareil() {
        // NOUVEAU (RONDE DE CORRECTION 1) : symétrique du test précédent —
        // une enveloppe qui attend un AUTRE appareil ne doit PAS empêcher
        // `inchange`. Avant cette ronde, `gerer_sync` regardait
        // `e.enveloppes.is_empty()` globalement : une enveloppe pour
        // n'importe quel appareil aurait fait échouer ce test-ci en
        // court-circuitant `inchange` à tort.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let mon_id = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: mon_id + 1000, // un AUTRE appareil, jamais le mien.
            charge: "YQ==".to_string(),
        });

        let recu: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu, serde_json::json!({ "inchange": true }));
    }

    #[test]
    fn sync_efface_les_enveloppes_apres_livraison() {
        // Échoue si `gerer_sync` ne vidait pas `EtatFaux::enveloppes` après
        // les avoir servies : un second appel, à la MÊME version, verrait
        // alors encore l'enveloppe déjà livrée — soit en la retransmettant
        // (si le court-circuit ci-dessus manquait aussi), soit en
        // continuant à empêcher `inchange` pour toujours.
        //
        // RONDE DE CORRECTION 1 : même adaptation que le test précédent —
        // jeton associé à un appareil réel, enveloppe adressée à celui-ci.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let id_appareil = enregistrer_appareil_http(&s, &jeton);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_appareil,
            charge: "YQ==".to_string(),
        });

        let premier: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(premier["enveloppes"][0]["id"], "1");

        let second: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(second, serde_json::json!({ "inchange": true }));
    }

    #[test]
    fn sync_nefface_pas_les_enveloppes_dun_autre_appareil() {
        // NOUVEAU (RONDE DE CORRECTION 1) : la consommation à la livraison
        // ne doit effacer QUE les enveloppes livrées à CET appareil. Avant
        // cette ronde, `e.enveloppes.clear()` videait tout — une enveloppe
        // pour un autre appareil, jamais vue par ce jeton, disparaissait
        // quand même.
        let s = FauxServeur::demarrer();
        let jeton_a = s.jeton_de_test_pour("a-nefface-pas-autrui");
        let _id_a = enregistrer_appareil_http(&s, &jeton_a); // seul le jeton sert ici.
        let jeton_b = s.jeton_de_test_pour("b-nefface-pas-autrui");
        let id_b = enregistrer_appareil_http(&s, &jeton_b);
        s.etat_mut().version = 5;
        s.etat_mut().enveloppes.push(EnveloppeFausse {
            id: "1".to_string(),
            expediteur_device_id: 1,
            destinataire_device_id: id_b,
            charge: "YQ==".to_string(),
        });

        // A synchronise SANS `?version=` (délibérément, voir ci-dessous) :
        // ne doit RIEN voir (l'enveloppe est pour B), et ne doit RIEN
        // effacer.
        //
        // SANS `?version=` — PAS UN OUBLI : avec `?version=5` (= e.version),
        // et aucune enveloppe pour A, le court-circuit `inchange` renvoie
        // AVANT MÊME D'ATTEINDRE la ligne qui efface — la neutralisation
        // visée ici (retirer le filtre de `retain`) ne serait alors JAMAIS
        // exercée, et ce test resterait vert même cassé. Sans `?version=`,
        // `version_connue` vaut `None`, qui ne peut jamais égaler
        // `Some(e.version)` : la branche complète (et sa consommation) est
        // TOUJOURS empruntée.
        let _: serde_json::Value = ureq::get(&format!("{}/api/sky/sync", s.url()))
            .set("Authorization", &format!("Bearer {jeton_a}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(s.etat_mut().enveloppes.len(), 1, "l'enveloppe de B doit survivre à la synchronisation de A");

        // B synchronise ensuite : doit voir SON enveloppe, encore présente.
        let recu_b: serde_json::Value = ureq::get(&format!("{}/api/sky/sync?version=5", s.url()))
            .set("Authorization", &format!("Bearer {jeton_b}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu_b["enveloppes"][0]["destinataire_device_id"], id_b);
    }

    #[test]
    fn devices_refuse_une_cle_publique_tronquee() {
        // Échoue si le double acceptait une clé publique qui ne décode pas
        // vers exactement 32 octets sous forme canonique — un double plus
        // permissif que le vrai serveur (`clePubliqueValide`) donnerait une
        // confiance imméritée aux tests des tâches suivantes.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle_tronquee = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);

        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(
                r#"{{"publicKey":"{cle_tronquee}","nom":"Mon PC","plateforme":"windows"}}"#
            ))
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
    }

    #[test]
    fn devices_reussit_et_incremente_lidentifiant() {
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([2u8; 32]);

        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&format!(r#"{{"publicKey":"{cle}","nom":"Mon PC","plateforme":"windows"}}"#))
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["id"], 1);
    }

    #[test]
    fn friends_refuse_un_code_inconnu_et_reussit_avec_le_code_enregistre() {
        // "STRANGE9" et "BUDDY234" sont tous deux BIEN FORMÉS (8 caractères
        // de l'alphabet réel `ABCDEFGHJKMNPQRSTUVWXYZ23456789`) — ce test
        // vise la distinction 404 (forme correcte, inconnu) / succès, pas
        // la validation de forme elle-même (voir
        // `friends_refuse_un_code_mal_forme_avec_400` pour celle-ci).
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let refuse = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"STRANGE9"}"#)
            .unwrap_err();
        assert_eq!(refuse.into_response().unwrap().status(), 404);

        s.etat_mut().code_ami_valide = Some(("BUDDY234".to_string(), 42));
        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"BUDDY234"}"#)
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["id"], 42);
    }

    #[test]
    fn friends_refuse_un_code_mal_forme_avec_400() {
        // RONDE DE CORRECTION 1, « Important 1 » : "INCONNU1" contient `I`,
        // `O` et `1`, absents de l'alphabet réel — contre le vrai serveur
        // ce code reçoit un 400 de forme, JAMAIS le 404 métier qu'un
        // double moins strict rendrait. Neutralisation : retirer l'appel à
        // `normaliser_code_ami` dans `gerer_friends` (revenir à une
        // comparaison directe à `code_ami_valide`) fait rougir CE test
        // précis, seul — les codes bien formés des deux tests voisins
        // restent inchangés par une telle neutralisation.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let refuse = ureq::post(&format!("{}/api/sky/friends", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(r#"{"code":"INCONNU1"}"#)
            .unwrap_err();
        let reponse = refuse.into_response().unwrap();
        assert_eq!(reponse.status(), 400);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"code invalide : forme inattendue"}"#);
    }

    #[test]
    fn devices_refuse_un_nom_avec_octet_nul() {
        // RONDE DE CORRECTION 1, « Important 2 » : le site refuse l'octet
        // NUL dans `nom` via `texteStockable` (400) avant que la valeur
        // n'atteigne Postgres, qui la refuserait par un 500. Échoue si le
        // double acceptait encore un `nom` qui le porte.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let corps = serde_json::json!({"publicKey": cle, "nom": "a\u{0}b", "plateforme": "windows"});

        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps.to_string())
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 400);
    }

    #[test]
    fn devices_mesure_la_longueur_du_nom_en_unites_utf16() {
        // RONDE DE CORRECTION 1 : `nomValide` mesure `nom.length` — des
        // unités de code UTF-16, pas des caractères Unicode. Un emoji
        // (U+1F600) compte pour 2 unités UTF-16 mais pour 1 seul `char`
        // Rust : 33 emoji, c'est 33 `chars()` (accepté par l'ANCIENNE
        // mesure de ce double, `chars().count() <= 64`) mais 66 unités
        // UTF-16 (refusé par la vraie mesure, > 64). Échoue si le double
        // mesurait encore en caractères.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        let cle = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let nom_trop_long: String = "😀".repeat(33);
        assert_eq!(nom_trop_long.chars().count(), 33);
        assert_eq!(nom_trop_long.encode_utf16().count(), 66);

        let corps = serde_json::json!({"publicKey": cle, "nom": nom_trop_long, "plateforme": "windows"});
        let reponse = ureq::post(&format!("{}/api/sky/devices", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string(&corps.to_string())
            .unwrap_err();
        assert_eq!(reponse.into_response().unwrap().status(), 400);
    }

    #[test]
    fn friends_accept_refuse_un_identifiant_mal_forme_avec_400() {
        // RONDE DE CORRECTION 1 (Mineur) : un identifiant non entier,
        // négatif ou nul reçoit 400 côté site, pas le même 404 qu'un
        // identifiant bien formé mais inexistant.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().amitie_acceptable = Some(7);

        for id in ["0", "-3", "abc", "7.5"] {
            let reponse = ureq::post(&format!("{}/api/sky/friends/{id}/accept", s.url()))
                .set("Authorization", &format!("Bearer {jeton}"))
                .send_string("{}")
                .unwrap_err();
            let reponse = reponse.into_response().unwrap();
            assert_eq!(reponse.status(), 400, "identifiant testé : {id}");
            assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Identifiant invalide"}"#);
        }
    }

    #[test]
    fn friends_accept_reussit_pour_lidentifiant_pilote_et_refuse_sinon() {
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().amitie_acceptable = Some(7);

        let refuse = ureq::post(&format!("{}/api/sky/friends/8/accept", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string("{}")
            .unwrap_err();
        assert_eq!(refuse.into_response().unwrap().status(), 404);

        let recu: serde_json::Value = ureq::post(&format!("{}/api/sky/friends/7/accept", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .send_string("{}")
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(recu["ok"], true);
    }

    // --- /api/auth/me (AJOUT DE LA TÂCHE 10) --------------------------
    //
    // Même discipline que le reste de ce module : chaque route a ses tests
    // HTTP directs, indépendants de `sky_compte` — ceux-là vivent dans
    // `identite_test.rs`. Sans ceux-ci, `MoiFaux::nouveau` n'était appelée
    // que par `identite_test.rs`, jamais par ce module : `cargo clippy`
    // le rendait mort dans les quatre AUTRES binaires de test qui incluent
    // ce fichier (`faux_serveur_test`, `session_test`, `boite_test`,
    // `annuaire_test`), chacun compilant ce module séparément via `#[path]`.

    #[test]
    fn me_exige_un_jeton() {
        // Échoue si `GET /api/auth/me` répondait sans en-tête `Authorization`
        // — même garde que `sync_exige_un_jeton`.
        let s = FauxServeur::demarrer();
        let reponse = ureq::get(&format!("{}/api/auth/me", s.url())).call();
        let reponse = reponse.unwrap_err().into_response().unwrap();
        assert_eq!(reponse.status(), 401);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Non authentifie"}"#);
    }

    #[test]
    fn me_rend_lidentifiant_et_le_nom_discord_en_camel_case() {
        // Échoue si `gerer_me` rendait `discord_name` (snake_case) plutôt
        // que `discordName` — forme exacte de la réponse du site.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().moi = Some(MoiFaux::nouveau(42, "Killian"));

        let recu: serde_json::Value = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();

        assert_eq!(recu["id"], 42);
        assert_eq!(recu["discordName"], "Killian");
        assert!(recu.get("discord_name").is_none());
    }

    #[test]
    fn me_sans_utilisateur_connu_rend_404() {
        // Même principe que `code_ami_valide`/`amitie_acceptable` : aucun
        // utilisateur n'est connu tant qu'un test ne l'a pas enregistré.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();

        let reponse = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 404);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"Utilisateur introuvable"}"#);
    }

    #[test]
    fn me_avec_session_partielle_rend_403_avant_de_consulter_moi() {
        // Échoue si `gerer_me` consultait `moi` AVANT `session_partielle_totp`
        // — même ordre que le site (`requireAuth` puis `!totpVerified` puis
        // `findUserById`). `moi` est enregistré pour prouver que ce n'est
        // PAS son absence qui produit ce refus.
        let s = FauxServeur::demarrer();
        let jeton = s.jeton_de_test();
        s.etat_mut().moi = Some(MoiFaux::nouveau(1, "peu importe"));
        s.etat_mut().session_partielle_totp = true;

        let reponse = ureq::get(&format!("{}/api/auth/me", s.url()))
            .set("Authorization", &format!("Bearer {jeton}"))
            .call()
            .unwrap_err();
        let reponse = reponse.into_response().unwrap();
        assert_eq!(reponse.status(), 403);
        assert_eq!(reponse.into_string().unwrap(), r#"{"error":"TOTP verification required"}"#);
    }
}
