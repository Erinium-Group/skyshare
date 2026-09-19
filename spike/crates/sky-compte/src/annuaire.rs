//! L'annuaire : synchronisation de l'état du compte (amis, demandes reçues,
//! appareils, enveloppes en attente), enregistrement d'appareil, ajout et
//! acceptation d'amis, résolution d'un ami par son nom ou son identifiant.
//!
//! Toutes les fonctions authentifiées de ce module passent par
//! `avec_jeton_valide` (`session.rs`) — jamais par `jeton_courant` seule —
//! pour bénéficier de la reprise sur 401 sans avoir à la réimplémenter ici.

use serde::{Deserialize, Serialize};

use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config, ReponseHttp};
use crate::session::avec_jeton_valide;
use crate::Coffre;

/// État complet de synchronisation, tel que rendu par `GET /api/sky/sync`.
///
/// `enveloppes` est le SEUL canal par lequel une enveloppe scellée arrive :
/// le serveur l'efface en la livrant (`releverPour`, côté site). Toute
/// fonction de ce module qui construit un `Etat` doit donc préserver ce
/// champ tel que reçu — jamais le vider en dehors du cas `inchange` de
/// `synchroniser`, où c'est au contraire la seule valeur correcte (voir sa
/// documentation).
#[derive(Debug, Clone, PartialEq)]
pub struct Etat {
    pub version: u64,
    pub code: String,
    pub amis: Vec<Ami>,
    pub demandes: Vec<Demande>,
    pub listes: Vec<Liste>,
    pub appareils: Vec<Appareil>,
    pub enveloppes: Vec<EnveloppeRecue>,
}

/// Un ami au statut accepté — seuls les champs nécessaires à le désigner
/// (`resoudre_ami`) et à sceller une enveloppe à son intention.
#[derive(Debug, Clone, PartialEq)]
pub struct Ami {
    pub id: i64,
    pub discord_name: String,
    pub appareils: Vec<AppareilDAmi>,
}

/// Appareil d'un ami accepté — exactement deux champs, ceux qui permettent
/// de sceller une enveloppe à son intention (`AppareilDAmi`, côté site) :
/// ni nom, ni plateforme, ni dates.
#[derive(Debug, Clone, PartialEq)]
pub struct AppareilDAmi {
    pub id: i64,
    pub public_key: String,
}

/// Demande d'ami reçue — une amitié en attente dont l'utilisateur courant
/// n'est pas le demandeur. `friendship_id` est ce que `accepter_ami`
/// attend en entrée.
#[derive(Debug, Clone, PartialEq)]
pub struct Demande {
    pub friendship_id: i64,
    pub demandeur_id: i64,
    pub discord_name: String,
    pub discord_avatar: Option<String>,
    pub created_at: String,
}

/// Un appareil enregistré par l'utilisateur courant — la vue de gestion
/// « mes appareils enregistrés » (ne porte jamais sa propre clé publique,
/// voir `Appareil`, côté site).
#[derive(Debug, Clone, PartialEq)]
pub struct Appareil {
    pub id: i64,
    pub nom: String,
    pub plateforme: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// Une enveloppe scellée reçue par `GET /api/sky/sync`. `id` est une
/// chaîne (`BIGSERIAL` côté site, rendu en `string` par le pilote Neon),
/// alors que les deux identifiants d'appareil sont des entiers.
#[derive(Debug, Clone, PartialEq)]
pub struct EnveloppeRecue {
    pub id: String,
    pub expediteur_device_id: i64,
    pub destinataire_device_id: i64,
    pub charge: String,
}

/// Une liste de diffusion de l'utilisateur courant, telle que rendue par
/// `GET /api/sky/sync` dans `listes[]` — forme `ListeAvecMembres` du site
/// (`src/lib/sky/listes.ts`, `listesDe`, jalon 1). `membres` : identifiants
/// d'UTILISATEURS (pas d'amitiés), triés croissants par le site.
#[derive(Debug, Clone, PartialEq)]
pub struct Liste {
    pub id: i64,
    pub nom: String,
    pub couleur: Option<String>,
    pub emoji: Option<String>,
    pub created_at: String,
    pub membres: Vec<i64>,
}

// --- Formes brutes du contrat JSON du site ----------------------------
//
// Séparées des types publics ci-dessus : la forme du fil (camelCase par
// endroits, snake_case ailleurs — voir le commentaire de module de
// `faux_serveur/mod.rs`) n'a pas à fuiter dans l'API de ce crate.

#[derive(Debug, Deserialize)]
struct AppareilDAmiBrut {
    id: i64,
    public_key: String,
}

#[derive(Debug, Deserialize)]
struct AmiBrut {
    id: i64,
    discord_name: String,
    appareils: Vec<AppareilDAmiBrut>,
}

#[derive(Debug, Deserialize)]
struct DemandeBrute {
    #[serde(rename = "friendshipId")]
    friendship_id: i64,
    #[serde(rename = "demandeurId")]
    demandeur_id: i64,
    discord_name: String,
    discord_avatar: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct AppareilBrut {
    id: i64,
    nom: String,
    plateforme: String,
    created_at: String,
    last_seen_at: Option<String>,
    revoked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EnveloppeBrute {
    id: String,
    expediteur_device_id: i64,
    destinataire_device_id: i64,
    charge: String,
}

/// `membres` est EXIGÉ, sans valeur par défaut : un site qui ne le rendrait
/// pas encore (tâche 1 non déployée) ferait échouer la synchronisation en
/// erreur de protocole plutôt que d'afficher des listes faussement vides —
/// que l'écran Listes réenregistrerait vides.
#[derive(Debug, Deserialize)]
struct ListeBrute {
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
    created_at: String,
    membres: Vec<i64>,
}

/// Forme complète de `GET /api/sky/sync`.
#[derive(Debug, Deserialize)]
struct EtatBrut {
    version: u64,
    code: String,
    amis: Vec<AmiBrut>,
    demandes: Vec<DemandeBrute>,
    listes: Vec<ListeBrute>,
    appareils: Vec<AppareilBrut>,
    enveloppes: Vec<EnveloppeBrute>,
}

/// Réponse de `GET /api/sky/sync` : soit l'aveu bref, soit l'état complet.
/// L'ordre des variantes compte pour `serde(untagged)` : `Inchange` échoue
/// à se désérialiser (champ `inchange` absent) sur une réponse complète, et
/// `Complet` échoue (champs obligatoires absents) sur `{"inchange":true}` —
/// aucune ambiguïté entre les deux formes.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ReponseSync {
    Inchange {
        #[allow(dead_code)] // seule sa présence importe, jamais sa valeur.
        inchange: bool,
    },
    Complet(EtatBrut),
}

/// Décode `valeur` en base64 standard et vérifie qu'il s'agit bien d'une
/// clé publique X25519 : exactement 32 octets, ET le réencodage redonne la
/// chaîne de départ à l'identique (forme canonique) — même règle que
/// `clePubliqueValide` côté site (`src/app/api/sky/devices/route.ts`).
///
/// Le contrôle par aller-retour ferme la même fenêtre que côté site : une
/// chaîne non canonique (espaces, octets de bourrage superflus, alphabet
/// non standard) peut très bien décoder vers 32 octets sans être la valeur
/// que l'émetteur voulait réellement transmettre.
fn cle_publique_valide(valeur: &str) -> bool {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    match STANDARD.decode(valeur) {
        Ok(octets) => octets.len() == 32 && STANDARD.encode(&octets) == valeur,
        Err(_) => false,
    }
}

/// Convertit l'état brut du fil en `Etat` public.
///
/// Un `AppareilDAmi` dont `public_key` n'est pas du base64 canonique
/// décodant en exactement 32 octets est ÉCARTÉ, pas rejeté en bloc : une
/// ligne corrompue (ou un intermédiaire hostile) ne doit pas empêcher la
/// synchronisation de tous les autres amis et appareils. Le site valide
/// déjà `publicKey` à l'écriture (`clePubliqueValide`, `POST
/// /api/sky/devices`) — ce cas ne devrait donc se produire que sur
/// corruption après coup, jamais en fonctionnement normal.
fn convertir_etat(brut: EtatBrut) -> Etat {
    let amis = brut
        .amis
        .into_iter()
        .map(|a| Ami {
            id: a.id,
            discord_name: a.discord_name,
            appareils: a
                .appareils
                .into_iter()
                .filter(|ap| cle_publique_valide(&ap.public_key))
                .map(|ap| AppareilDAmi { id: ap.id, public_key: ap.public_key })
                .collect(),
        })
        .collect();

    let demandes = brut
        .demandes
        .into_iter()
        .map(|d| Demande {
            friendship_id: d.friendship_id,
            demandeur_id: d.demandeur_id,
            discord_name: d.discord_name,
            discord_avatar: d.discord_avatar,
            created_at: d.created_at,
        })
        .collect();

    let listes = brut
        .listes
        .into_iter()
        .map(|l| Liste {
            id: l.id,
            nom: l.nom,
            couleur: l.couleur,
            emoji: l.emoji,
            created_at: l.created_at,
            membres: l.membres,
        })
        .collect();

    let appareils = brut
        .appareils
        .into_iter()
        .map(|a| Appareil {
            id: a.id,
            nom: a.nom,
            plateforme: a.plateforme,
            created_at: a.created_at,
            last_seen_at: a.last_seen_at,
            revoked_at: a.revoked_at,
        })
        .collect();

    let enveloppes = brut
        .enveloppes
        .into_iter()
        .map(|e| EnveloppeRecue {
            id: e.id,
            expediteur_device_id: e.expediteur_device_id,
            destinataire_device_id: e.destinataire_device_id,
            charge: e.charge,
        })
        .collect();

    Etat { version: brut.version, code: brut.code, amis, demandes, listes, appareils, enveloppes }
}

/// Synchronise l'état du compte. `?version=` n'est envoyé que si
/// `precedent` est fourni — c'est ce qui permet au serveur de répondre
/// `{ inchange: true }` plutôt que de renvoyer tout l'état.
///
/// LE PIÈGE DE CETTE FONCTION : `enveloppes` est le SEUL canal par lequel
/// une enveloppe arrive, et le serveur l'efface en la livrant. Sur un état
/// complet, ce champ est donc reporté tel quel depuis la réponse — jamais
/// jeté ni fusionné avec `precedent` : un appel sans rapport (lister ses
/// amis, par exemple) ne doit jamais perdre silencieusement une offre
/// qu'un ami vient de déposer.
///
/// Sur `{ inchange: true }` : le serveur ne répond ainsi que si RIEN
/// n'attend, enveloppes comprises (voir `construireEtat`, côté site) — les
/// enveloppes de `precedent` ont donc déjà été livrées par le sync qui l'a
/// produit. Les recopier ferait retraiter la même offre à chaque
/// interrogation suivante, toutes les deux secondes. `synchroniser` rend
/// donc une copie de `precedent` avec `enveloppes` VIDÉ.
///
/// Un `{ inchange: true }` reçu SANS `precedent` est une erreur de
/// protocole : rien n'a pu être comparé à une version qui n'existe pas.
pub fn synchroniser(config: &Config, coffre: &Coffre, precedent: Option<&Etat>) -> Result<Etat, ErreurCompte> {
    let chemin = match precedent {
        Some(p) => format!("/api/sky/sync?version={}", p.version),
        None => "/api/sky/sync".to_string(),
    };
    let client = ClientHttp::new(config);

    let reponse: ReponseSync = avec_jeton_valide(config, coffre, |jeton| client.get_json(&chemin, Some(jeton)))?;

    resoudre_reponse_sync(reponse, precedent)
}

/// Traduit une `ReponseSync` déjà reçue en `Etat`, en fonction de
/// `precedent` — extrait de `synchroniser` pour rester testable en unité,
/// sans passer par une vraie requête HTTP : c'est ici, et seulement ici,
/// que vit la décision « inchangé => vider les enveloppes du précédent »
/// / « inchangé sans précédent => erreur de protocole ».
fn resoudre_reponse_sync(reponse: ReponseSync, precedent: Option<&Etat>) -> Result<Etat, ErreurCompte> {
    match reponse {
        ReponseSync::Inchange { .. } => match precedent {
            Some(p) => Ok(Etat { enveloppes: Vec::new(), ..p.clone() }),
            None => Err(ErreurCompte::Protocole(
                "réponse « inchangé » reçue sans état précédent connu".to_string(),
            )),
        },
        ReponseSync::Complet(brut) => Ok(convertir_etat(brut)),
    }
}

/// Rend la valeur attendue par le champ `plateforme` du site
/// (`PLATEFORMES_VALIDES`, `POST /api/sky/devices`) pour le système actuel,
/// ou une erreur de protocole AVANT tout appel réseau si le système n'est
/// aucun des trois supportés.
fn plateforme_locale() -> Result<&'static str, ErreurCompte> {
    match std::env::consts::OS {
        "windows" => Ok("windows"),
        "macos" => Ok("macos"),
        "linux" => Ok("linux"),
        autre => Err(ErreurCompte::Protocole(format!(
            "plateforme non supportée par l'API de signaling : {autre}"
        ))),
    }
}

#[derive(Serialize)]
struct CorpsAppareil<'a> {
    #[serde(rename = "publicKey")]
    public_key: &'a str,
    nom: &'a str,
    plateforme: &'a str,
}

#[derive(Deserialize)]
struct ReponseAppareil {
    id: i64,
}

/// Vérifie `nom` contre la même règle que `nomValide`/`texteStockable`
/// côté site (`devices/route.ts`, `texte.ts`) — AVANT tout appel réseau,
/// même principe que valider une taille avant un dépôt : on valide contre
/// ce que le serveur refuse, pas contre ce que ce client juge acceptable.
///
/// Longueur mesurée en UNITÉS DE CODE UTF-16 (`encode_utf16().count()`),
/// PAS en caractères Unicode : c'est ainsi que `nom.length` compte en
/// JavaScript, et c'est cette mesure-là que `nomValide` applique. Un seul
/// caractère hors du plan de base (ex. la plupart des emoji) compte pour 2
/// unités UTF-16 mais pour 1 seul `char` Rust — les deux mesures divergent
/// dès qu'une telle valeur apparaît dans `nom`.
///
/// `texteStockable` refuse aussi un substitut Unicode isolé
/// (U+D800-U+DFFF) — NON reproduit ici : un `&str` Rust est garanti UTF-8
/// valide et ne peut structurellement pas porter une telle valeur (il n'y
/// a aucune séquence UTF-8 pour un substitut isolé). Seuls l'octet NUL et
/// la longueur restent donc à vérifier côté client.
fn nom_appareil_valide(nom: &str) -> bool {
    let longueur_utf16 = nom.encode_utf16().count();
    (1..=64).contains(&longueur_utf16) && !nom.contains('\0')
}

/// Enregistre l'appareil courant. `cle` est la clé publique X25519 de
/// l'appareil, encodée en base64 standard avant l'envoi — c'est la forme
/// que `POST /api/sky/devices` attend (`clePubliqueValide`, côté site).
///
/// Range l'identifiant rendu par le serveur dans `coffre`
/// (`Coffre::ranger_identifiant_appareil`, tâche 9) AVANT de le rendre à
/// l'appelant : c'est ce que `deposer` (`boite.rs`) lit ensuite comme
/// `expediteur_device_id`. Le faire ICI, jamais laissé à la charge de
/// l'appelant, évite la classe de défaut la plus coûteuse de ce jalon — une
/// fonction juste que personne n'appelle jamais dans le bon ordre.
///
/// DÉSYNCHRONISATION COFFRE/SERVEUR (RONDE DE CORRECTION 1, Mineur 2 de la
/// revue de la tâche 9) : si l'écriture locale échoue APRÈS que le serveur a
/// déjà créé l'appareil (`id` existe côté serveur), l'erreur rendue le dit
/// explicitement — voir plus bas. Ni reprise automatique ni suppression
/// côté serveur ici : cas rare (échec d'écriture du trousseau), et une
/// correction automatique dépasserait la portée de cette tâche.
pub fn enregistrer_appareil(config: &Config, coffre: &Coffre, nom: &str, cle: &[u8; 32]) -> Result<i64, ErreurCompte> {
    if !nom_appareil_valide(nom) {
        return Err(ErreurCompte::Protocole(
            "nom d'appareil invalide : attendu 1 a 64 unites UTF-16, sans octet NUL".to_string(),
        ));
    }
    let plateforme = plateforme_locale()?;

    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let cle_b64 = STANDARD.encode(cle);

    let client = ClientHttp::new(config);
    let corps = CorpsAppareil { public_key: &cle_b64, nom, plateforme };

    let id = avec_jeton_valide(config, coffre, |jeton| {
        let reponse: ReponseAppareil = client.post_json("/api/sky/devices", &corps, Some(jeton))?;
        Ok(reponse.id)
    })?;

    // À ce point, l'appareil `id` existe DÉJÀ côté serveur : un échec
    // d'écriture locale ici (trousseau indisponible, refusé par l'OS, ...)
    // ne doit jamais ressembler à un échec d'enregistrement ordinaire. Sans
    // ce message précis, l'appelant croirait l'opération entièrement ratée
    // et réessaierait — créant un SECOND appareil côté serveur, pendant que
    // le premier reste orphelin (son identifiant introuvable localement,
    // donc plus aucun dépôt possible tant qu'il n'est pas retrouvé).
    if let Err(erreur_coffre) = coffre.ranger_identifiant_appareil(id) {
        return Err(ErreurCompte::Coffre(format!(
            "appareil {id} créé côté serveur mais son identifiant n'a pas pu être enregistré \
             localement ({erreur_coffre}) — un nouvel appel à enregistrer_appareil créera un \
             second appareil, celui-ci restera orphelin côté serveur"
        )));
    }

    Ok(id)
}

/// Issue d'un `rattacher_appareil` réussi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rattachement {
    /// Le coffre ne porte aucun identifiant d'appareil : rien à rattacher.
    AucunAppareil,
    /// L'appareil `ancien` figurait parmi ceux du compte : révoqué, puis
    /// réenregistré avec la même clé et le même `nom`, sous l'identifiant
    /// `nouveau`, rattaché à la session courante.
    Rattache { ancien: i64, nouveau: i64, nom: String },
    /// L'appareil `ancien` ne figure PAS parmi ceux du compte (supprimé, ou
    /// enregistré sous un autre compte Discord sur cette machine) :
    /// réenregistré quand même avec la même clé, sous un nom par défaut.
    ReenregistreSousNomParDefaut { ancien: i64, nouveau: i64, nom: String },
}

/// Rattache l'appareil de cette machine à la session COURANTE.
///
/// POURQUOI (revue finale, I1) : le site ne connaît « l'appareil courant »
/// que par `devices.session_id` (`sync/route.ts`), posé une seule fois, à
/// l'enregistrement. Chaque connexion crée une NOUVELLE session
/// (`creerSessionNative`), qui expire au plus tard sept jours après. Sans
/// rattachement, après un nouveau `login`, `GET /api/sky/sync` ne résout
/// aucun appareil : aucune enveloppe n'est plus jamais livrée à cette
/// machine, sans la moindre erreur. Un renouvellement de jeton, lui, garde
/// la même session (`refresh/route.ts`) : il ne rompt rien.
///
/// Déroulé : synchroniser, retrouver l'appareil de l'identifiant rangé
/// (pour son nom), le RÉVOQUER (`DELETE /api/sky/devices/{id}` — révoque
/// aussi l'ancienne session qu'il portait, et le retire des appareils vus
/// par les amis, qui ne scellent donc plus rien pour lui), puis le
/// réenregistrer avec la MÊME clé publique, ce qui le lie à la session
/// courante et range le nouvel identifiant (`enregistrer_appareil`).
///
/// ORDRE : l'identifiant rangé n'est remplacé qu'APRÈS un réenregistrement
/// réussi — c'est `enregistrer_appareil` qui le range, à son succès. Un
/// échec entre la révocation et le réenregistrement laisse donc l'ancien
/// identifiant (révoqué) dans le coffre : un nouvel appel refait la
/// révocation (acceptée, 204 ou 404) puis le réenregistrement.
///
/// /!\ SYNCHRONISE : consomme les enveloppes en attente, comme toute
/// synchronisation (voir `synchroniser`). Appelée juste après une
/// connexion, quand aucune négociation n'est censée être en cours.
pub fn rattacher_appareil(config: &Config, coffre: &Coffre) -> Result<Rattachement, ErreurCompte> {
    let Some(ancien) = coffre.identifiant_appareil()? else {
        return Ok(Rattachement::AucunAppareil);
    };
    // La MÊME clé : les enveloppes déjà scellées pour cet appareil, et la
    // clé que ses amis connaissent, restent valables.
    let cle = coffre.identite()?.public_key();

    let etat = synchroniser(config, coffre, None)?;
    let nom_connu = etat.appareils.iter().find(|a| a.id == ancien).map(|a| a.nom.clone());

    revoquer_appareil(config, coffre, ancien)?;

    match nom_connu {
        Some(nom) => {
            let nouveau = enregistrer_appareil(config, coffre, &nom, &cle)?;
            Ok(Rattachement::Rattache { ancien, nouveau, nom })
        }
        None => {
            let nom = nom_par_defaut();
            let nouveau = enregistrer_appareil(config, coffre, &nom, &cle)?;
            Ok(Rattachement::ReenregistreSousNomParDefaut { ancien, nouveau, nom })
        }
    }
}

/// `DELETE /api/sky/devices/{id}`. `204` (révoqué, y compris un appareil
/// déjà révoqué) et `404` (inexistant, ou pas à ce compte) sont tous deux
/// acceptés : dans les deux cas, cet identifiant ne désigne plus un appareil
/// actif de ce compte, ce qui est tout ce que le rattachement exige. Tout
/// autre refus (400 identifiant invalide, 403 session partielle, ...) est
/// une erreur.
fn revoquer_appareil(config: &Config, coffre: &Coffre, id: i64) -> Result<(), ErreurCompte> {
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/devices/{id}");
    avec_jeton_valide(config, coffre, |jeton| match client.delete_reponse_vide_avec_refus(&chemin, Some(jeton))? {
        ReponseHttp::Succes(()) | ReponseHttp::Refus { statut: 404, .. } => Ok(()),
        ReponseHttp::Refus { statut, corps } => {
            Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")))
        }
    })
}

/// Nom d'un appareil réenregistré alors que l'ancien n'est plus connu du
/// compte : le nom de la machine (`COMPUTERNAME`) s'il est acceptable par le
/// site, sinon un nom générique.
fn nom_par_defaut() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|nom| nom_appareil_valide(nom))
        .unwrap_or_else(|| "Appareil SkyShare".to_string())
}

/// Alphabet réel des codes ami — même valeur que `ALPHABET` côté site
/// (`src/lib/sky/codes.ts`) : sans `I`, `L`, `O`, `0`, `1`, pour éviter
/// l'ambiguïté visuelle. NE JAMAIS CHANGER (invaliderait les codes déjà émis
/// en production, voir `CLAUDE.md` du dépôt) — recopié ici, pas importé :
/// cette bibliothèque ne dépend d'aucun code du site.
const ALPHABET_CODE_AMI: &str = "ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// Même règle que `codeValide` côté site : exactement 8 caractères, tous
/// dans `ALPHABET_CODE_AMI`.
fn code_ami_valide_forme(brut: &str) -> bool {
    brut.chars().count() == 8 && brut.chars().all(|c| ALPHABET_CODE_AMI.contains(c))
}

/// Ramène une saisie humaine de code ami à sa forme canonique, ou `None` —
/// reproduit EXACTEMENT `normaliserCode` côté site (`src/lib/sky/codes.ts`) :
/// espaces de bord retirés, mise en capitales, PUIS préfixe `SKY-` retiré
/// s'il est présent, PUIS tous les tirets retirés ; le résultat doit avoir
/// la forme d'un code valide (8 caractères de `ALPHABET_CODE_AMI`).
///
/// IMPLÉMENTATION PROPRE À CETTE BIBLIOTHÈQUE, PAS REPRISE DU SERVEUR
/// DOUBLE : si le double appelait cette même fonction, il imiterait le
/// client au lieu du site qu'il est censé témoigner — ce sont deux copies
/// volontairement distinctes.
///
/// L'ORDRE COMPTE : la mise en capitales précède le retrait du préfixe, donc
/// `"sky-abcdefgh"` (préfixe en minuscules) est valide — le retirer avant de
/// capitaliser le manquerait.
///
/// Capitalisation par `to_uppercase()` (Unicode complet), JAMAIS
/// `to_ascii_uppercase()` : JavaScript applique la casse Unicode complète,
/// et un `ſ` (s long, U+017F) devient `S` des deux côtés — `S` appartient à
/// l'alphabet.
///
/// `trim()` en JavaScript retire aussi U+FEFF (BOM), que `str::trim()` de
/// Rust ne retire PAS (U+FEFF n'a pas la propriété Unicode `White_Space`,
/// contrairement à tous les autres caractères que `trim()` de Rust couvre
/// déjà) — retiré explicitement ici pour reproduire le comportement du site.
pub fn normaliser_code_ami(brut: &str) -> Option<String> {
    let recadre = brut.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
    let majuscules = recadre.to_uppercase();
    let sans_prefixe = majuscules.strip_prefix("SKY-").unwrap_or(&majuscules);
    let sans_tirets: String = sans_prefixe.chars().filter(|&c| c != '-').collect();
    if code_ami_valide_forme(&sans_tirets) {
        Some(sans_tirets)
    } else {
        None
    }
}

/// Issue d'un `ajouter_ami` — les deux refus attendus (`404`, `409`) sont
/// des valeurs, pas des erreurs : ils font partie du déroulement normal de
/// l'ajout d'un ami, contrairement à un échec réseau ou de protocole.
#[derive(Debug, Clone, PartialEq)]
pub enum AjoutAmi {
    /// `201` — la demande a été créée, avec l'identifiant de l'amitié.
    Envoyee { friendship_id: i64 },
    /// `404 {"error":"Code ami introuvable"}` — code inconnu. Couvre AUSSI,
    /// par conception, un utilisateur qui a bloqué l'appelant : le site
    /// rend délibérément le même refus pour les deux causes
    /// (`demanderAmi`, côté site), et ce client ne doit pas chercher à les
    /// redistinguer.
    CodeIntrouvable,
    /// `409 {"error":"Demande déjà existante"}` — une amitié (ou une
    /// demande) existe déjà entre les deux comptes.
    DejaDemandee,
}

#[derive(Serialize)]
struct CorpsAjoutAmi<'a> {
    code: &'a str,
}

#[derive(Deserialize)]
struct ReponseAjoutAmi {
    id: i64,
}

/// Envoie une demande d'ami par code.
///
/// Les 400 possibles de `POST /api/sky/friends` (`code` absent ou non
/// chaîne, forme de code invalide, tentative de s'ajouter soi-même) ne
/// sont volontairement PAS distingués ici : ce sont des erreurs d'appelant
/// (un code malformé ou son propre code, jamais un état atteignable par un
/// usage normal de ce client), donc `ErreurCompte::Protocole` ordinaire —
/// contrairement à `CodeIntrouvable`/`DejaDemandee`, qui sont des refus
/// qu'un utilisateur légitime peut rencontrer en cours d'usage normal.
pub fn ajouter_ami(config: &Config, coffre: &Coffre, code: &str) -> Result<AjoutAmi, ErreurCompte> {
    let client = ClientHttp::new(config);
    let corps = CorpsAjoutAmi { code };

    avec_jeton_valide(config, coffre, |jeton| {
        let issue: ReponseHttp<ReponseAjoutAmi> =
            client.post_json_avec_refus("/api/sky/friends", &corps, Some(jeton))?;
        match issue {
            ReponseHttp::Succes(r) => Ok(AjoutAmi::Envoyee { friendship_id: r.id }),
            ReponseHttp::Refus { statut: 404, .. } => Ok(AjoutAmi::CodeIntrouvable),
            ReponseHttp::Refus { statut: 409, .. } => Ok(AjoutAmi::DejaDemandee),
            ReponseHttp::Refus { statut, corps } => {
                Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")))
            }
        }
    })
}

/// Issue d'un `accepter_ami` — même principe que `AjoutAmi` : le refus
/// attendu (`404`) est une valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptation {
    /// `200 {"ok": true}`.
    Acceptee,
    /// `404 {"error":"Amitié introuvable"}` — couvre trois cas distincts et
    /// volontairement indiscernables côté site (amitié inexistante, amitié
    /// d'autrui, amitié qu'on a soi-même demandée) : ce client ne doit pas
    /// chercher à les redistinguer non plus.
    Introuvable,
}

#[derive(Deserialize)]
struct ReponseAcceptation {
    #[allow(dead_code)]
    ok: bool,
}

/// Accepte une demande d'ami reçue. L'identifiant voyage dans le chemin
/// (`POST /api/sky/friends/{id}/accept`) — la route ne lit aucun corps,
/// donc ce client en envoie un vide.
pub fn accepter_ami(config: &Config, coffre: &Coffre, friendship_id: i64) -> Result<Acceptation, ErreurCompte> {
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/friends/{friendship_id}/accept");
    let corps_vide = serde_json::json!({});

    avec_jeton_valide(config, coffre, |jeton| {
        let issue: ReponseHttp<ReponseAcceptation> =
            client.post_json_avec_refus(&chemin, &corps_vide, Some(jeton))?;
        match issue {
            ReponseHttp::Succes(_) => Ok(Acceptation::Acceptee),
            ReponseHttp::Refus { statut: 404, .. } => Ok(Acceptation::Introuvable),
            ReponseHttp::Refus { statut, corps } => {
                Err(ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}")))
            }
        }
    })
}

/// Résout une désignation (nom Discord exact, ou identifiant numérique) en
/// un ami unique de `etat`.
///
/// Rassemble les correspondances par identifiant ET par nom EXACT
/// ensemble : zéro correspondance est un échec, et PLUS D'UNE correspondance
/// DISTINCTE est également un échec — même si l'une provient du nom et
/// l'autre de l'identifiant (un ami nommé « 2 » et un ami d'identifiant 2).
/// Se tromper de destinataire ici revient à sceller ses adresses pour la
/// mauvaise personne : mieux vaut demander de préciser que de choisir au
/// hasard entre deux candidats.
pub fn resoudre_ami<'a>(etat: &'a Etat, designation: &str) -> Result<&'a Ami, ErreurCompte> {
    let par_id = designation.parse::<i64>().ok();

    let mut trouves: Vec<&Ami> = Vec::new();
    for ami in &etat.amis {
        let correspond = par_id == Some(ami.id) || ami.discord_name == designation;
        if correspond && !trouves.iter().any(|a| a.id == ami.id) {
            trouves.push(ami);
        }
    }

    match trouves.len() {
        0 => Err(ErreurCompte::Protocole(format!("aucun ami ne correspond à « {designation} »"))),
        1 => Ok(trouves[0]),
        _ => Err(ErreurCompte::Protocole(format!(
            "plusieurs amis correspondent à « {designation} » — précisez l'identifiant"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ami_avec_appareils(id: i64, nom: &str, appareils: Vec<AppareilDAmi>) -> Ami {
        Ami { id, discord_name: nom.to_string(), appareils }
    }

    /// Clé publique X25519 de test, valide (32 octets, base64 canonique).
    fn cle_de_test() -> String {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        STANDARD.encode([7u8; 32])
    }

    fn ami(id: i64, nom: &str) -> Ami {
        ami_avec_appareils(id, nom, vec![AppareilDAmi { id: id * 10, public_key: cle_de_test() }])
    }

    fn etat_avec(amis: Vec<Ami>) -> Etat {
        Etat {
            version: 1,
            code: "CODE1234".to_string(),
            amis,
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        }
    }

    #[test]
    fn deux_amis_de_meme_nom_font_refuser_plutot_que_choisir() {
        // Échoue si `resoudre_ami` choisissait le premier trouvé au lieu de
        // refuser l'ambiguïté. Se tromper de destinataire ici, c'est
        // sceller ses adresses pour la mauvaise personne.
        let etat = etat_avec(vec![ami(1, "bob"), ami(2, "bob")]);
        let r = resoudre_ami(&etat, "bob");
        assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
        assert_eq!(resoudre_ami(&etat, "2").unwrap().id, 2);
    }

    #[test]
    fn un_nom_et_un_identifiant_qui_designent_deux_amis_distincts_refusent_aussi() {
        // Le cas explicitement cité par le cahier des charges : un ami
        // NOMMÉ « 2 » et un ami D'IDENTIFIANT 2 sont deux personnes
        // différentes. Échoue si `resoudre_ami` ne rassemblait pas les
        // deux voies de correspondance dans le même calcul d'ambiguïté.
        let etat = etat_avec(vec![ami(2, "alice"), ami(3, "2")]);
        let r = resoudre_ami(&etat, "2");
        assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    }

    // L'ancien test `un_ami_sans_appareil_donne_un_tableau_vide_pas_une_erreur`
    // a été retiré ici (ronde de correction 1) : il vérifiait une fixture
    // construite par le test lui-même via `resoudre_ami`, qui rend une
    // RÉFÉRENCE — `.appareils` ne pouvait être que ce que le test y avait
    // mis, aucune implémentation de `resoudre_ami` ne pouvait le faire
    // rougir. Remplacé par
    // `un_ami_sans_appareil_se_desserialise_en_tableau_vide_sans_erreur`
    // dans `annuaire_test.rs`, qui passe par le vrai chemin de
    // désérialisation — voir son commentaire pour ce qu'il prouve
    // réellement (moins que son nom ne le suggérerait).

    #[test]
    fn aucune_correspondance_est_une_erreur() {
        let etat = etat_avec(vec![ami(1, "bob")]);
        assert!(matches!(resoudre_ami(&etat, "quelquun_dautre"), Err(ErreurCompte::Protocole(_))));
    }

    #[test]
    fn une_cle_publique_tronquee_est_ecartee_une_valide_est_gardee() {
        // Remplace l'ancien test creux du brief (`la_cle_publique_fait_bien_32_octets`),
        // qui vérifiait une clé fabriquée par le test lui-même — aucune
        // implémentation de production ne pouvait le faire rougir. Ici,
        // `convertir_etat` doit ÉCARTER la clé tronquée sans rejeter le
        // reste de l'ami, et garder la clé valide.
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;
        let valide = STANDARD.encode([9u8; 32]);
        let tronquee = STANDARD.encode([9u8; 16]); // 16 octets, pas 32.

        let brut = EtatBrut {
            version: 1,
            code: "CODE1234".to_string(),
            amis: vec![AmiBrut {
                id: 1,
                discord_name: "bob".to_string(),
                appareils: vec![
                    AppareilDAmiBrut { id: 10, public_key: tronquee },
                    AppareilDAmiBrut { id: 11, public_key: valide.clone() },
                ],
            }],
            demandes: Vec::new(),
            listes: Vec::new(),
            appareils: Vec::new(),
            enveloppes: Vec::new(),
        };

        let etat = convertir_etat(brut);
        assert_eq!(etat.amis[0].appareils.len(), 1);
        assert_eq!(etat.amis[0].appareils[0].id, 11);
        assert_eq!(etat.amis[0].appareils[0].public_key, valide);
    }

    #[test]
    fn inchange_sans_precedent_est_une_erreur_de_protocole() {
        // Situation que le vrai protocole ne devrait jamais produire (le
        // client n'envoie `?version=` que s'il a un précédent), mais que
        // `resoudre_reponse_sync` doit refuser explicitement plutôt que de
        // paniquer sur un `.clone()` d'un `Option` vide ou de rendre un état
        // par défaut silencieux.
        let r = resoudre_reponse_sync(ReponseSync::Inchange { inchange: true }, None);
        assert!(matches!(r, Err(ErreurCompte::Protocole(_))));
    }

    #[test]
    fn inchange_avec_precedent_vide_les_enveloppes_et_garde_le_reste() {
        // NEUTRALISATION CIBLÉE : si cette fonction recopiait
        // `precedent.enveloppes` tel quel plutôt que `Vec::new()`, ce test
        // rougirait précisément sur `enveloppes`, jamais sur `code`
        // (conservé, lui, à l'identique) — la preuve que les deux
        // assertions discriminent des chemins de code différents.
        let precedent =
            Etat { version: 3, code: "ANCIEN01".to_string(), amis: Vec::new(), demandes: Vec::new(), listes: Vec::new(), appareils: Vec::new(), enveloppes: vec![EnveloppeRecue { id: "1".to_string(), expediteur_device_id: 1, destinataire_device_id: 2, charge: "YQ==".to_string() }] };

        let etat = resoudre_reponse_sync(ReponseSync::Inchange { inchange: true }, Some(&precedent)).unwrap();
        assert_eq!(etat.code, "ANCIEN01");
        assert!(etat.enveloppes.is_empty());
    }

    #[test]
    fn cle_publique_valide_refuse_une_forme_non_canonique() {
        // Prouve que le contrôle est bien un ALLER-RETOUR, pas seulement
        // une longueur décodée : trois caractères de bruit ajoutés à une
        // clé valide décodent encore vers les mêmes 32 octets (même piège
        // que documenté côté site, `clePubliqueValide`).
        let valide = cle_de_test();
        let bruitee = format!("{valide}!!!");
        assert!(cle_publique_valide(&valide));
        assert!(!cle_publique_valide(&bruitee));
    }

    #[test]
    fn le_nom_d_appareil_se_mesure_en_unites_utf16_frontiere_64_65() {
        // T8 (revue finale) : la garde client n'était testée que côté double.
        // 32 emoji = 64 unités UTF-16 mais 32 `char` ; un « a » de plus = 65
        // unités, 33 `char`. Échoue si `nom_appareil_valide` mesurait en
        // `chars().count()` : 33 <= 64, le nom de 65 unités serait accepté
        // alors que le site (`nom.length <= 64`) le refuse en 400.
        let nom_64 = "😀".repeat(32);
        assert_eq!(nom_64.encode_utf16().count(), 64);
        assert!(nom_appareil_valide(&nom_64), "64 unités UTF-16 : accepté par le site");

        let nom_65 = format!("{nom_64}a");
        assert_eq!(nom_65.encode_utf16().count(), 65);
        assert_eq!(nom_65.chars().count(), 33);
        assert!(!nom_appareil_valide(&nom_65), "65 unités UTF-16 : refusé par le site");
    }

    // --- normaliser_code_ami -----------------------------------------

    #[test]
    fn un_code_deja_canonique_est_inchange() {
        assert_eq!(normaliser_code_ami("ABCDEFGH").as_deref(), Some("ABCDEFGH"));
    }

    #[test]
    fn espaces_tirets_prefixe_et_minuscules_sont_tolere() {
        assert_eq!(normaliser_code_ami("  sky-abcd-efgh  ").as_deref(), Some("ABCDEFGH"));
    }

    #[test]
    fn la_majuscule_precede_le_retrait_du_prefixe() {
        // NEUTRALISATION CIBLÉE : si l'implémentation retirait le préfixe
        // "SKY-" AVANT de capitaliser, cette entrée en minuscules ne
        // correspondrait plus au préfixe (comparaison sensible à la casse)
        // et le résultat resterait "SKYABCDEFGH" (11 caractères) — invalide.
        // Seule la capitalisation PUIS le retrait dans cet ordre rend
        // "ABCDEFGH".
        assert_eq!(normaliser_code_ami("sky-abcdefgh").as_deref(), Some("ABCDEFGH"));
    }

    #[test]
    fn un_s_long_unicode_se_capitalise_comme_en_javascript() {
        // NEUTRALISATION CIBLÉE : `to_ascii_uppercase()` laisserait 'ſ'
        // (U+017F, LATIN SMALL LETTER LONG S) inchangé — hors de
        // ALPHABET_CODE_AMI, donc refusé. `to_uppercase()` (casse Unicode
        // complète, comme JavaScript) le convertit en 'S', qui appartient à
        // l'alphabet : le code devient valide.
        assert_eq!(normaliser_code_ami("ABCDEFGſ").as_deref(), Some("ABCDEFGS"));
    }

    #[test]
    fn lalphabet_reste_verifie_apres_normalisation() {
        // NEUTRALISATION CIBLÉE : si le contrôle d'alphabet disparaissait
        // (seule la longueur restant vérifiée), "INCONNU1" — huit
        // caractères, mais I, O et 1 sont absents de ALPHABET_CODE_AMI —
        // serait accepté à tort.
        assert_eq!(normaliser_code_ami("INCONNU1"), None);
    }

    #[test]
    fn une_longueur_incorrecte_est_refusee() {
        assert_eq!(normaliser_code_ami("ABCDEFG"), None); // 7
        assert_eq!(normaliser_code_ami("ABCDEFGHJ"), None); // 9
    }

    #[test]
    fn le_bom_unicode_est_retire_comme_par_trim_javascript() {
        // NEUTRALISATION CIBLÉE : `str::trim()` de Rust seul ne retire PAS
        // U+FEFF (il n'a pas la propriété Unicode White_Space) — sans le
        // retrait explicite, ce code resterait précédé du caractère et la
        // longueur totale (9 avant filtrage par l'alphabet, qui de toute
        // façon rejetterait le caractère) échouerait la validation de forme.
        let avec_bom = "\u{FEFF}ABCDEFGH\u{FEFF}";
        assert_eq!(normaliser_code_ami(avec_bom).as_deref(), Some("ABCDEFGH"));
    }
}
