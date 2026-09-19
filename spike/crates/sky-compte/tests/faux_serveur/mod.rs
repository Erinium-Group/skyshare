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

/// Liste telle que rendue par `GET /api/sky/sync` dans `listes[]` — forme
/// `ListeAvecMembres` du site (`src/lib/sky/listes.ts`, `listesDe`, jalon 1
/// tâche 1). `membres` : identifiants d'utilisateurs, triés croissants
/// (`json_agg(... ORDER BY membre_id)`), `[]` jamais absent.
#[derive(Debug, Clone, Serialize)]
pub struct ListeFausse {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub created_at: String,
    pub membres: Vec<i64>,
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

    /// MODÈLE DE SESSION — VAGUE DE CORRECTION FINALE (I1). Remplace l'ancien
    /// `jetons_appareil` (jeton d'accès → appareil), qui liait l'appareil à la
    /// CHAÎNE EXACTE du jeton : un simple renouvellement rompait la liaison, ce
    /// que le site ne fait pas — le double était alors à la fois plus strict
    /// que le site (renouvellement) et muet sur le vrai défaut (nouvelle
    /// connexion).
    ///
    /// Relevé du site : un jeton porte un `sessionId` ; `POST /api/auth/refresh`
    /// réémet un jeton portant LE MÊME `sessionId` (`refresh/route.ts`) ; `POST
    /// /api/auth/native` crée une NOUVELLE session à chaque échange
    /// (`creerSessionNative`, `session.ts`) ; `devices.session_id` lie un
    /// appareil à UNE session (`enregistrerAppareil`, qui détache d'abord
    /// l'appareil portant déjà cette session) ; `GET /api/sky/sync` résout
    /// l'appareil courant par cette session (`UPDATE devices ... WHERE
    /// session_id = $1`), et ne livre RIEN, sans erreur, à une session qui n'en
    /// porte aucun.
    ///
    /// Ici : jeton d'accès délivré → session. Un jeton injecté par un test sans
    /// session explicite est sa propre session (`session_de_l_acces`).
    pub session_du_jeton: HashMap<String, String>,
    /// Jeton de renouvellement délivré → session (même règle).
    pub session_du_renouvellement: HashMap<String, String>,
    /// Sessions révoquées (`sessions.revoked_at`) : tout jeton d'accès ou de
    /// renouvellement qui en porte une est refusé (`verifierSessionActive`).
    pub sessions_revoquees: HashSet<String>,
    /// `devices.session_id`, lu dans l'autre sens : session → appareil qu'elle
    /// porte. Un nouvel enregistrement sur la même session ÉCRASE l'entrée —
    /// c'est le détachement que fait `enregistrerAppareil`.
    pub appareil_de_session: HashMap<String, i64>,
    /// Appareils créés par `POST /api/sky/devices` → clé publique reçue. Ce sont
    /// les appareils de « l'utilisateur » du double, les seuls que `DELETE
    /// /api/sky/devices/{id}` peut révoquer — tout autre identifiant rend 404,
    /// comme l'appareil d'autrui côté site.
    pub cles_des_appareils: HashMap<i64, String>,
    /// Appareils révoqués : retirés des appareils vus par les amis (`amisDe` :
    /// `d.revoked_at IS NULL`).
    pub appareils_revoques: HashSet<i64>,
    /// Nombre de sessions créées par `POST /api/auth/native`.
    pub sessions_creees: u64,
    /// Fait échouer `POST /api/sky/devices` par une erreur interne (500), APRÈS
    /// authentification et validation — pour éprouver un réenregistrement qui
    /// échoue.
    pub refuser_enregistrement_appareil: bool,

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

    /// Listes de « l'utilisateur » du double (jalon 1, tâche 2).
    pub listes: Vec<ListeFausse>,
    /// Prochain identifiant rendu par `POST /api/sky/lists` (premier : 1).
    pub prochain_id_liste: i64,
    /// Nombre de requêtes reçues sur `/api/sky/lists*`, refusées comprises —
    /// prouve qu'une entrée invalide est refusée AVANT tout appel réseau.
    pub appels_listes: u64,

    /// Code ami rendu par `GET /api/sky/sync` ; `None` : `CODE_FAUX`.
    /// Remplacé par `POST /api/sky/friend-code` (jalon 1, tâche 3).
    pub code_ami: Option<String>,
    /// Nombre de régénérations du code ami.
    pub codes_regeneres: u64,
    /// Requêtes reçues sur `/api/sky/friends*` (ajout, acceptation, retrait,
    /// blocage), refusées comprises.
    pub appels_amis: u64,
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
    if matches!(methode, Method::Post | Method::Put | Method::Patch) {
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
        (Method::Delete, chemin_appareil) if chemin_appareil.starts_with("/api/sky/devices/") => {
            gerer_revocation_appareil(etat, jeton.as_deref(), chemin_appareil)
        }
        (Method::Post, chemin_accept)
            if chemin_accept.starts_with("/api/sky/friends/") && chemin_accept.ends_with("/accept") =>
        {
            gerer_accepter_ami(etat, jeton.as_deref(), chemin_accept)
        }
        (Method::Post, "/api/sky/lists") => gerer_creer_liste(etat, jeton.as_deref(), &corps_brut),
        (Method::Put, chemin_membres)
            if chemin_membres.starts_with("/api/sky/lists/") && chemin_membres.ends_with("/members") =>
        {
            gerer_definir_membres(etat, jeton.as_deref(), chemin_membres, &corps_brut)
        }
        (Method::Patch, chemin_liste) if chemin_liste.starts_with("/api/sky/lists/") => {
            gerer_modifier_liste(etat, jeton.as_deref(), chemin_liste, &corps_brut)
        }
        (Method::Delete, chemin_liste) if chemin_liste.starts_with("/api/sky/lists/") => {
            gerer_supprimer_liste(etat, jeton.as_deref(), chemin_liste)
        }
        (Method::Post, "/api/sky/friend-code") => gerer_regenerer_code(etat, jeton.as_deref()),
        (Method::Post, chemin_bloc)
            if chemin_bloc.starts_with("/api/sky/friends/") && chemin_bloc.ends_with("/block") =>
        {
            gerer_bloquer_ami(etat, jeton.as_deref(), chemin_bloc)
        }
        (Method::Delete, chemin_ami) if chemin_ami.starts_with("/api/sky/friends/") => {
            gerer_retirer_ami(etat, jeton.as_deref(), chemin_ami)
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
    let Some(jeton) = jeton.filter(|j| etat.jetons_acceptes.contains(*j)) else {
        return Some((401, json!({ "error": "Non authentifie" }).to_string()));
    };
    // Session révoquée (VAGUE DE CORRECTION FINALE, I1) : même refus qu'un jeton
    // inconnu — côté site, `getSession` ne rend aucune session quand
    // `verifierSessionActive` échoue, et `requireAuth` lève « Non authentifie ».
    if etat.sessions_revoquees.contains(&session_de_l_acces(etat, jeton)) {
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

    // VAGUE DE CORRECTION FINALE (I1) : l'appareil COURANT est celui que porte
    // la SESSION de ce jeton (`appareil_de_session`, posé par `gerer_devices`)
    // — même résolution que `deviceIdCourant` côté site (`UPDATE devices ...
    // WHERE session_id = $1`). Avant, la liaison portait sur la chaîne exacte du
    // jeton, et un renouvellement la rompait. `None` pour une session qui ne
    // porte aucun appareil (session web sans application, ou nouvelle connexion
    // non rattachée) : AUCUNE enveloppe à recevoir, jamais celles d'un autre
    // appareil — et aucune erreur, comme le site.
    let appareil_courant =
        jeton.and_then(|j| e.appareil_de_session.get(&session_de_l_acces(&e, j)).copied());

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

    // Les amis ne voient que les appareils non révoqués (`amisDe` :
    // `d.revoked_at IS NULL`) — VAGUE DE CORRECTION FINALE (I1).
    let amis_vus: Vec<AmiFaux> = e
        .amis
        .iter()
        .cloned()
        .map(|mut ami| {
            ami.appareils.retain(|appareil| !e.appareils_revoques.contains(&appareil.id));
            ami
        })
        .collect();

    let corps = json!({
        "version": e.version,
        "code": e.code_ami.clone().unwrap_or_else(|| CODE_FAUX.to_string()),
        "amis": amis_vus,
        "demandes": e.demandes,
        "listes": e.listes,
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

    let Some(public_key) = public_key.filter(|v| cle_publique_valide(v)) else {
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
    if e.refuser_enregistrement_appareil {
        // Échec APRÈS validation, comme une erreur de base côté site
        // (`handleAuthError` : 500 « Erreur interne »).
        return (500, json!({ "error": "Erreur interne" }).to_string());
    }
    e.prochain_id_appareil += 1;
    let appareil = AppareilFaux {
        id: e.prochain_id_appareil,
        nom: nom.to_string(),
        plateforme: plateforme.to_string(),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_seen_at: None,
        revoked_at: None,
    };

    // C'est ICI, au succès de l'enregistrement, que la SESSION du jeton présenté
    // est liée à l'appareil créé — même moment que le site (`devices.session_id`
    // posé par `enregistrerAppareil`). L'insertion ÉCRASE l'appareil que cette
    // session portait déjà : c'est le détachement que fait `enregistrerAppareil`
    // avant d'insérer. `gerer_sync` lit ce lien pour ne livrer que les
    // enveloppes de CET appareil (VAGUE DE CORRECTION FINALE, I1).
    if let Some(jeton) = jeton {
        let session = session_de_l_acces(&e, jeton);
        e.appareil_de_session.insert(session, appareil.id);
    }
    e.cles_des_appareils.insert(appareil.id, public_key.to_string());
    // `appareilsDe` côté site rend TOUS les appareils de l'utilisateur, révoqués
    // compris : `GET /api/sky/sync` doit donc voir celui-ci dans `appareils`.
    e.appareils.push(appareil.clone());

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
    e.appels_amis += 1;
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
    e.appels_amis += 1;
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

    // USAGE UNIQUE (VAGUE DE CORRECTION FINALE, I1) : côté site, `echangerCode`
    // fait `DELETE FROM auth_codes WHERE code = $1 RETURNING ...` AVANT de
    // comparer l'empreinte du secret — un code présenté avec un MAUVAIS secret
    // est donc consommé lui aussi. Le consommer seulement au succès rendrait ce
    // double plus permissif que le site.
    let attendu = e.code_natif_valide.clone();
    let accepte = match attendu {
        Some((code_attendu, secret_attendu)) if code_attendu == code => {
            e.code_natif_valide = None;
            secret_attendu == secret
        }
        _ => false,
    };
    if !accepte {
        return (401, REFUS.to_string());
    }

    // Valeurs fabriquées par le double, pas relevées du site : aucun test
    // ne dépend de leur contenu précis, seulement de leur capacité à
    // authentifier un appel ultérieur (voir
    // `auth_native_reussit_avec_le_code_enregistre_et_le_jeton_fonctionne_ensuite`).
    //
    // VAGUE DE CORRECTION FINALE (I1) : chaque échange crée une NOUVELLE session
    // (`creerSessionNative`, `session.ts` côté site), que les deux jetons portent.
    e.sessions_creees += 1;
    let numero = e.sessions_creees;
    let session = format!("session-native-{numero}");
    let acces = format!("jeton-acces-natif-{numero}");
    let refresh = format!("jeton-refresh-natif-{numero}");
    e.jetons_acceptes.insert(acces.clone());
    e.jetons_de_renouvellement_valides.insert(refresh.clone());
    e.session_du_jeton.insert(acces.clone(), session.clone());
    e.session_du_renouvellement.insert(refresh.clone(), session);
    (200, json!({ "acces": acces, "refresh": refresh }).to_string())
}

/// `POST /api/auth/refresh` — succès à un seul champ `acces` (jamais
/// `refresh` en retour, forme confirmée par `api/auth/refresh/route.ts`).
/// La valeur `"jeton-neuf-<n>"` (n = numéro de l'appel) est un CHOIX DU
/// DOUBLE, pas une forme relevée du site — distincte à chaque appel depuis la
/// vague de correction finale (voir plus bas). Enregistrée dans
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

    // Session révoquée : refusée, comme par `verifierSessionActive` côté site.
    let session = session_du_renouvellement_de(&e, refresh);
    if e.refuser_tout
        || !e.jetons_de_renouvellement_valides.contains(refresh)
        || e.sessions_revoquees.contains(&session)
    {
        return (401, json!({ "error": "Renouvellement refuse" }).to_string());
    }

    // VAGUE DE CORRECTION FINALE (I1) : le jeton réémis porte LA MÊME session
    // (`refresh/route.ts` : `sessionId: charge.sessionId`) — l'appareil de cette
    // session reste donc l'appareil courant après un renouvellement. Un jeton
    // distinct à chaque appel : une valeur fixe, réémise pour une autre session,
    // ferait changer de session en silence un jeton déjà délivré.
    let acces = format!("jeton-neuf-{}", e.appels_de_renouvellement);
    e.jetons_acceptes.insert(acces.clone());
    e.session_du_jeton.insert(acces.clone(), session);
    (200, json!({ "acces": acces }).to_string())
}

/// Session portée par un jeton d'ACCÈS reconnu — VAGUE DE CORRECTION FINALE
/// (I1). Un jeton délivré par `POST /api/auth/native` ou `POST
/// /api/auth/refresh` porte la session enregistrée dans `session_du_jeton` ; un
/// jeton injecté par un test sans session explicite (`jeton_de_test`, insertion
/// directe dans `jetons_acceptes`) est sa propre session, distincte de toute
/// autre.
fn session_de_l_acces(etat: &EtatFaux, jeton: &str) -> String {
    etat.session_du_jeton.get(jeton).cloned().unwrap_or_else(|| format!("acces:{jeton}"))
}

/// Même règle que `session_de_l_acces`, pour un jeton de RENOUVELLEMENT.
fn session_du_renouvellement_de(etat: &EtatFaux, refresh: &str) -> String {
    etat.session_du_renouvellement
        .get(refresh)
        .cloned()
        .unwrap_or_else(|| format!("renouvellement:{refresh}"))
}

/// `DELETE /api/sky/devices/{id}` — VAGUE DE CORRECTION FINALE (I1). Même ordre
/// et mêmes réponses que `devices/[id]/route.ts` : authentification (401 « Non
/// authentifie »), session partielle (403 « Verification TOTP requise »), forme
/// de l'identifiant (400 « Identifiant invalide »), puis `revoquerAppareil` :
/// 404 « Appareil introuvable » si l'appareil n'existe pas ou n'est pas à cet
/// utilisateur, 204 sans corps sinon — y compris pour un appareil DÉJÀ révoqué
/// (l'`UPDATE` du site ne filtre pas `revoked_at`).
///
/// La révocation révoque aussi la session que l'appareil porte encore (seconde
/// requête de la transaction de `revoquerAppareil`) : les appels suivants avec
/// un jeton de cette session rendent 401. Un appareil détaché de sa session
/// (celle-ci a enregistré un autre appareil depuis) n'en porte plus aucune.
fn gerer_revocation_appareil(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(refus) = autoriser_appel(&mut e, jeton) {
        return refus;
    }
    if e.session_partielle_totp {
        return (403, json!({ "error": "Verification TOTP requise" }).to_string());
    }

    // `Number(id)` puis `Number.isInteger(x) && x > 0` côté site. Plus strict
    // ici — entier décimal seulement : « 7.0 », que JavaScript accepte, rend
    // 400 —, jamais plus permissif.
    let segment = chemin.strip_prefix("/api/sky/devices/").unwrap_or_default();
    let Some(id) = segment.parse::<i64>().ok().filter(|id| *id > 0) else {
        return (400, json!({ "error": "Identifiant invalide" }).to_string());
    };

    // `revoquerAppareil` rend `false` au-delà de la borne INTEGER de Postgres.
    const POSTGRES_INTEGER_MAX: i64 = 2_147_483_647;
    if id > POSTGRES_INTEGER_MAX || !e.cles_des_appareils.contains_key(&id) {
        return (404, json!({ "error": "Appareil introuvable" }).to_string());
    }

    e.appareils_revoques.insert(id);
    if let Some(appareil) = e.appareils.iter_mut().find(|a| a.id == id) {
        appareil.revoked_at = Some("2026-01-01T00:00:00.000Z".to_string());
    }
    let session_portee = e
        .appareil_de_session
        .iter()
        .find(|(_, appareil)| **appareil == id)
        .map(|(session, _)| session.clone());
    if let Some(session) = session_portee {
        e.sessions_revoquees.insert(session);
    }
    (204, String::new())
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

// --- Listes (jalon 1, tâche 2) ------------------------------------------
//
// Chaque règle est celle d'une ligne des routes `src/app/api/sky/lists/**`
// du site, relevée dans le plan du jalon 1 (tâche 2, tableau des bornes).
// Recopiée, pas importée : ce double ne dépend d'aucun code du site, ni de
// `sky_compte::listes` (il imiterait alors le client au lieu du site).

/// Borne d'un `INTEGER` Postgres — `POSTGRES_INTEGER_MAX` de `listes.ts`.
const INTEGER_POSTGRES_MAX: i64 = 2_147_483_647;
const NOM_LISTE_MAX: usize = 40;
const EMOJI_LISTE_OCTETS_MAX: usize = 8;
const MEMBRES_LISTE_MAX: usize = 200;
const REFUS_MEMBRES: &str =
    "Liste introuvable ou un ou plusieurs identifiants ne sont pas des amis acceptes";

/// Identifiant entier strictement positif entre `prefixe` et `suffixe` —
/// `Number(id)` puis `Number.isInteger(x) && x > 0` côté site. Plus strict
/// que le site (« 7.0 » refusé ici), jamais plus permissif.
fn identifiant_de_chemin(chemin: &str, prefixe: &str, suffixe: &str) -> Option<i64> {
    chemin.strip_prefix(prefixe)?.strip_suffix(suffixe)?.parse::<i64>().ok().filter(|id| *id > 0)
}

/// `nomValide` (lists/route.ts:18-25) : chaîne, 1 à 40 unités UTF-16, sans NUL.
fn nom_de_liste(valeur: &Value) -> Option<&str> {
    let nom = valeur.as_str()?;
    let unites = nom.encode_utf16().count();
    ((1..=NOM_LISTE_MAX).contains(&unites) && !nom.contains('\0')).then_some(nom)
}

/// `couleurValide` (lists/route.ts:35-38) : absente ou `null`, sinon `#RRGGBB`.
fn couleur_de_liste(valeur: Option<&Value>) -> Result<Option<String>, ()> {
    match valeur {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(c))
            if c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|o| o.is_ascii_hexdigit()) =>
        {
            Ok(Some(c.clone()))
        }
        _ => Err(()),
    }
}

/// `emojiValide` (lists/route.ts:52-59) : absent ou `null`, sinon 8 octets
/// UTF-8 au plus, sans NUL.
fn emoji_de_liste(valeur: Option<&Value>) -> Result<Option<String>, ()> {
    match valeur {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(e)) if e.len() <= EMOJI_LISTE_OCTETS_MAX && !e.contains('\0') => Ok(Some(e.clone())),
        _ => Err(()),
    }
}

fn refus(statut: u16, message: &str) -> (u16, String) {
    (statut, json!({ "error": message }).to_string())
}

/// `POST /api/sky/lists` — 201 avec les colonnes publiques SANS `membres`
/// (`creerListe` : `RETURNING COLONNES_LISTE`), 400 sur forme, 409 sur nom
/// déjà pris. La version progresse (`updated_at` posé à l'insertion).
fn gerer_creer_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let corps: Value = match serde_json::from_str(corps_brut) {
        Ok(v @ Value::Object(_)) => v,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let Some(nom) = corps.get("nom").and_then(nom_de_liste) else {
        return refus(400, "nom invalide : attendu 1 a 40 caracteres");
    };
    let Ok(couleur) = couleur_de_liste(corps.get("couleur")) else {
        return refus(400, "couleur invalide : attendu #RRGGBB ou null");
    };
    let Ok(emoji) = emoji_de_liste(corps.get("emoji")) else {
        return refus(400, "emoji invalide : attendu au plus 8 octets ou null");
    };
    if e.listes.iter().any(|l| l.nom == nom) {
        return refus(409, "Nom de liste deja utilise");
    }
    e.prochain_id_liste += 1;
    let liste = ListeFausse {
        id: e.prochain_id_liste,
        nom: nom.to_string(),
        couleur,
        emoji,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        membres: Vec::new(),
    };
    e.listes.push(liste.clone());
    e.version += 1;
    let reponse = json!({
        "id": liste.id, "nom": liste.nom, "couleur": liste.couleur,
        "emoji": liste.emoji, "created_at": liste.created_at,
    });
    (201, reponse.to_string())
}

/// `PATCH /api/sky/lists/{id}` — mise à jour PARTIELLE : seules les clés
/// présentes sont validées et écrites (`"cle" in donnees`) ; `null` remet
/// couleur ou émoji à rien. Aucune clé : 400. Liste inconnue : 404.
fn gerer_modifier_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let objet = match serde_json::from_str::<Value>(corps_brut) {
        Ok(Value::Object(o)) => o,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let mut nom = None;
    let mut couleur = None;
    let mut emoji = None;
    if let Some(v) = objet.get("nom") {
        match nom_de_liste(v) {
            Some(n) => nom = Some(n.to_string()),
            None => return refus(400, "nom invalide : attendu 1 a 40 caracteres"),
        }
    }
    if objet.contains_key("couleur") {
        match couleur_de_liste(objet.get("couleur")) {
            Ok(c) => couleur = Some(c),
            Err(()) => return refus(400, "couleur invalide : attendu #RRGGBB ou null"),
        }
    }
    if objet.contains_key("emoji") {
        match emoji_de_liste(objet.get("emoji")) {
            Ok(em) => emoji = Some(em),
            Err(()) => return refus(400, "emoji invalide : attendu au plus 8 octets ou null"),
        }
    }
    if nom.is_none() && couleur.is_none() && emoji.is_none() {
        return refus(400, "Aucun champ a modifier");
    }
    let Some(index) = e.listes.iter().position(|l| l.id == id) else {
        return refus(404, "Liste introuvable");
    };
    if let Some(n) = &nom {
        if e.listes.iter().any(|l| l.id != id && &l.nom == n) {
            return refus(409, "Nom de liste deja utilise");
        }
    }
    let liste = &mut e.listes[index];
    if let Some(n) = nom {
        liste.nom = n;
    }
    if let Some(c) = couleur {
        liste.couleur = c;
    }
    if let Some(em) = emoji {
        liste.emoji = em;
    }
    e.version += 1;
    (204, String::new())
}

/// `DELETE /api/sky/lists/{id}` — 204, ou 404 « Liste introuvable ». NE fait
/// PAS progresser `version` : voir le plan du jalon 1, tâche 2.
fn gerer_supprimer_liste(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.listes.len();
    e.listes.retain(|l| l.id != id);
    if e.listes.len() == avant {
        return refus(404, "Liste introuvable");
    }
    (204, String::new())
}

/// `PUT /api/sky/lists/{id}/members` — `membreIds` : tableau d'au plus 200
/// entiers > 0 (400 sinon) ; puis `definirMembres` : borne INTEGER, liste à
/// l'appelant, TOUS amis acceptés, sinon 400 UNIFORME ; succès : ensemble
/// dédoublonné, trié, version qui progresse.
fn gerer_definir_membres(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str, corps_brut: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_listes += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/lists/", "/members") else {
        return refus(400, "Identifiant invalide");
    };
    let corps: Value = match serde_json::from_str(corps_brut) {
        Ok(v @ Value::Object(_)) => v,
        _ => return refus(400, "Corps JSON invalide"),
    };
    let invalide = "membreIds invalide : attendu un tableau d'au plus 200 identifiants entiers positifs";
    let Some(bruts) = corps.get("membreIds").and_then(Value::as_array) else {
        return refus(400, invalide);
    };
    if bruts.len() > MEMBRES_LISTE_MAX {
        return refus(400, invalide);
    }
    let mut membres = Vec::with_capacity(bruts.len());
    for v in bruts {
        match v.as_i64() {
            Some(m) if m > 0 => membres.push(m),
            _ => return refus(400, invalide),
        }
    }
    if id > INTEGER_POSTGRES_MAX || membres.iter().any(|m| *m > INTEGER_POSTGRES_MAX) {
        return refus(400, REFUS_MEMBRES);
    }
    let Some(index) = e.listes.iter().position(|l| l.id == id) else {
        return refus(400, REFUS_MEMBRES);
    };
    membres.sort_unstable();
    membres.dedup();
    if !membres.iter().all(|m| e.amis.iter().any(|a| a.id == *m)) {
        return refus(400, REFUS_MEMBRES);
    }
    e.listes[index].membres = membres;
    e.version += 1;
    (204, String::new())
}

// --- Amis et compte (jalon 1, tâche 3) ------------------------------------

/// `DELETE /api/sky/friends/{id}` — `retirerAmi` (amis.ts:288-300) SUPPRIME la
/// ligne `friendships`, amitié acceptée OU demande en attente : 204, sinon 404
/// « Amitié introuvable ». La version NE progresse PAS : le site peut rendre
/// `inchange` après un retrait (voir le plan du jalon 1, tâche 3).
fn gerer_retirer_ami(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_amis += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/friends/", "") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.amis.len() + e.demandes.len();
    e.amis.retain(|a| a.friendship_id != id);
    e.demandes.retain(|d| d.friendship_id != id);
    if e.amis.len() + e.demandes.len() == avant {
        return refus(404, "Amitié introuvable");
    }
    (204, String::new())
}

/// `POST /api/sky/friends/{id}/block` — `bloquerAmi` (amis.ts:345-358) : la
/// ligne passe en `bloquee` (elle quitte `amisDe` et `demandesDe`) avec
/// `updated_at = NOW()` : la version progresse. 200 `{ ok: true }` ou 404.
fn gerer_bloquer_ami(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>, chemin: &str) -> (u16, String) {
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    e.appels_amis += 1;
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let Some(id) = identifiant_de_chemin(chemin, "/api/sky/friends/", "/block") else {
        return refus(400, "Identifiant invalide");
    };
    let avant = e.amis.len() + e.demandes.len();
    e.amis.retain(|a| a.friendship_id != id);
    e.demandes.retain(|d| d.friendship_id != id);
    if e.amis.len() + e.demandes.len() == avant {
        return refus(404, "Amitié introuvable");
    }
    e.version += 1;
    (200, json!({ "ok": true }).to_string())
}

/// `POST /api/sky/friend-code` — `regenererCode` (codes.ts:81) : 200
/// `{ code }`. Écrit `users.friend_code`, absent du calcul de version : la
/// version NE progresse PAS. Codes tirés dans l'alphabet réel.
fn gerer_regenerer_code(etat: &Arc<Mutex<EtatFaux>>, jeton: Option<&str>) -> (u16, String) {
    const CODES: [&str; 3] = ["REGENAA2", "REGENBB3", "REGENCC4"];
    let mut e = etat.lock().expect("mutex etat faux empoisonne");
    if let Some(r) = autoriser_appel(&mut e, jeton) {
        return r;
    }
    if e.session_partielle_totp {
        return refus(403, "Verification TOTP requise");
    }
    let code = CODES[(e.codes_regeneres % 3) as usize];
    e.codes_regeneres += 1;
    e.code_ami = Some(code.to_string());
    (200, json!({ "code": code }).to_string())
}

// Les tests propres à ce double vivent dans `tests_du_double.rs`, inclus par
// `faux_serveur_test.rs` seulement — voir l'en-tête de ce fichier pour la raison.
