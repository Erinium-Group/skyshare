//! Listes de diffusion (jalon 1, spec §3) : création, modification,
//! suppression, définition des membres. La LECTURE passe par `synchroniser`
//! (`Etat::listes`, membres compris) : aucune route de lecture ici.
//!
//! Chaque borne est celle de la route du site qui lit le paramètre
//! (`src/app/api/sky/lists/route.ts`, `lists/[id]/route.ts`,
//! `lists/[id]/members/route.ts`, `src/lib/sky/listes.ts`), appliquée AVANT
//! tout appel réseau : on valide contre ce que le serveur refuse, ni plus
//! strictement, ni plus largement.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::annuaire::Liste;
use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config, ReponseHttp};
use crate::session::avec_jeton_valide;
use crate::Coffre;

/// `NOM_MAX` de `lists/route.ts` et `lists/[id]/route.ts`.
const NOM_MAX: usize = 40;
/// `EMOJI_OCTETS_MAX` des mêmes routes.
const EMOJI_OCTETS_MAX: usize = 8;
/// `MEMBRES_MAX` de `members/route.ts` et de `definirMembres`.
pub const MEMBRES_MAX: usize = 200;
/// `POSTGRES_INTEGER_MAX` de `listes.ts` : au-delà, `definirMembres` refuse.
const POSTGRES_INTEGER_MAX: i64 = 2_147_483_647;

/// `nomValide` : 1 à 40 unités UTF-16 (`length` en JavaScript, pas des
/// `char`), sans octet NUL (`texteStockable`). Un substitut isolé ne peut
/// pas exister dans un `&str`.
pub fn nom_liste_valide(nom: &str) -> bool {
    let unites = nom.encode_utf16().count();
    (1..=NOM_MAX).contains(&unites) && !nom.contains('\0')
}

/// `couleurValide` : aucune, ou exactement `#` suivi de six chiffres
/// hexadécimaux, majuscules ou minuscules.
pub fn couleur_valide(couleur: Option<&str>) -> bool {
    match couleur {
        None => true,
        Some(c) => c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|o| o.is_ascii_hexdigit()),
    }
}

/// `emojiValide` : aucun, ou 8 OCTETS UTF-8 au plus (`Buffer.byteLength`),
/// sans octet NUL. Une chaîne vide est acceptée par le site : elle l'est ici.
pub fn emoji_valide(emoji: Option<&str>) -> bool {
    match emoji {
        None => true,
        Some(e) => e.len() <= EMOJI_OCTETS_MAX && !e.contains('\0'),
    }
}

/// `membreIdsValide` puis les gardes de `definirMembres` : au plus 200
/// identifiants, chacun entre 1 et la borne INTEGER de Postgres.
pub fn membres_valides(membres: &[i64]) -> bool {
    membres.len() <= MEMBRES_MAX && membres.iter().all(|m| (1..=POSTGRES_INTEGER_MAX).contains(m))
}

fn refuse_avant_le_reseau(motif: &str) -> ErreurCompte {
    ErreurCompte::Protocole(format!("{motif} — refusé avant tout appel au site"))
}

fn statut_inattendu(statut: u16, corps: String) -> ErreurCompte {
    ErreurCompte::Protocole(format!("statut {statut} inattendu : {corps}"))
}

/// Issue d'une création : le nom déjà pris est un refus ATTENDU, pas une
/// erreur (409, `lists/route.ts:112`).
#[derive(Debug, Clone, PartialEq)]
pub enum CreationListe {
    Creee(Liste),
    NomDejaPris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModificationListe {
    Modifiee,
    /// 404 : inexistante OU pas à l'appelant — indiscernables à dessein.
    Introuvable,
    NomDejaPris,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuppressionListe {
    Supprimee,
    Introuvable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionMembres {
    Definis,
    /// 400 UNIFORME du site : liste pas à l'appelant, OU un identifiant qui
    /// n'est pas un ami accepté. Le client ne cherche pas à les distinguer.
    Refuses,
}

/// Champs d'une modification partielle. `None` : clé absente, champ
/// inchangé. `Some(None)` pour `couleur`/`emoji` : `null`, remise à rien.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChampsListe<'a> {
    pub nom: Option<&'a str>,
    pub couleur: Option<Option<&'a str>>,
    pub emoji: Option<Option<&'a str>>,
}

impl ChampsListe<'_> {
    /// Corps JSON du `PATCH` : SEULES les clés présentes, comme `"cle" in
    /// donnees` côté site.
    fn en_corps(&self) -> Map<String, Value> {
        let mut corps = Map::new();
        if let Some(nom) = self.nom {
            corps.insert("nom".to_string(), Value::from(nom));
        }
        if let Some(couleur) = self.couleur {
            corps.insert("couleur".to_string(), couleur.map_or(Value::Null, Value::from));
        }
        if let Some(emoji) = self.emoji {
            corps.insert("emoji".to_string(), emoji.map_or(Value::Null, Value::from));
        }
        corps
    }
}

#[derive(Serialize)]
struct CorpsCreation<'a> {
    nom: &'a str,
    couleur: Option<&'a str>,
    emoji: Option<&'a str>,
}

/// Réponse 201 de `POST /api/sky/lists` : `COLONNES_LISTE`, sans `membres`.
#[derive(Deserialize)]
struct ListeCreee {
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
struct CorpsMembres<'a> {
    #[serde(rename = "membreIds")]
    membre_ids: &'a [i64],
}

/// `POST /api/sky/lists`.
pub fn creer_liste(
    config: &Config,
    coffre: &Coffre,
    nom: &str,
    couleur: Option<&str>,
    emoji: Option<&str>,
) -> Result<CreationListe, ErreurCompte> {
    if !nom_liste_valide(nom) {
        return Err(refuse_avant_le_reseau("nom de liste invalide : 1 à 40 unités UTF-16, sans octet NUL"));
    }
    if !couleur_valide(couleur) {
        return Err(refuse_avant_le_reseau("couleur invalide : #RRGGBB ou aucune"));
    }
    if !emoji_valide(emoji) {
        return Err(refuse_avant_le_reseau("émoji invalide : 8 octets au plus"));
    }
    let client = ClientHttp::new(config);
    let demande = CorpsCreation { nom, couleur, emoji };
    avec_jeton_valide(config, coffre, |jeton| {
        let issue: ReponseHttp<ListeCreee> = client.post_json_avec_refus("/api/sky/lists", &demande, Some(jeton))?;
        match issue {
            ReponseHttp::Succes(l) => Ok(CreationListe::Creee(Liste {
                id: l.id,
                nom: l.nom,
                couleur: l.couleur,
                emoji: l.emoji,
                created_at: l.created_at,
                membres: Vec::new(),
            })),
            ReponseHttp::Refus { statut: 409, .. } => Ok(CreationListe::NomDejaPris),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}

/// `PATCH /api/sky/lists/{id}`.
pub fn modifier_liste(
    config: &Config,
    coffre: &Coffre,
    id: i64,
    champs: &ChampsListe<'_>,
) -> Result<ModificationListe, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    if champs.nom.is_none() && champs.couleur.is_none() && champs.emoji.is_none() {
        return Err(refuse_avant_le_reseau("aucun champ à modifier"));
    }
    if champs.nom.is_some_and(|n| !nom_liste_valide(n)) {
        return Err(refuse_avant_le_reseau("nom de liste invalide : 1 à 40 unités UTF-16, sans octet NUL"));
    }
    if champs.couleur.is_some_and(|c| !couleur_valide(c)) {
        return Err(refuse_avant_le_reseau("couleur invalide : #RRGGBB ou aucune"));
    }
    if champs.emoji.is_some_and(|e| !emoji_valide(e)) {
        return Err(refuse_avant_le_reseau("émoji invalide : 8 octets au plus"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}");
    let demande = champs.en_corps();
    avec_jeton_valide(config, coffre, |jeton| {
        match client.envoyer_json_reponse_vide_avec_refus("PATCH", &chemin, &demande, Some(jeton))? {
            ReponseHttp::Succes(()) => Ok(ModificationListe::Modifiee),
            ReponseHttp::Refus { statut: 404, .. } => Ok(ModificationListe::Introuvable),
            ReponseHttp::Refus { statut: 409, .. } => Ok(ModificationListe::NomDejaPris),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}

/// `DELETE /api/sky/lists/{id}`. Attention : la version de synchronisation
/// peut ne pas progresser (la ligne disparaît) — l'appelant qui veut voir la
/// suppression resynchronise SANS précédent.
pub fn supprimer_liste(config: &Config, coffre: &Coffre, id: i64) -> Result<SuppressionListe, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}");
    avec_jeton_valide(config, coffre, |jeton| match client.delete_reponse_vide_avec_refus(&chemin, Some(jeton))? {
        ReponseHttp::Succes(()) => Ok(SuppressionListe::Supprimee),
        ReponseHttp::Refus { statut: 404, .. } => Ok(SuppressionListe::Introuvable),
        ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
    })
}

/// `PUT /api/sky/lists/{id}/members` — remplace TOUT l'ensemble des membres.
///
/// Après les contrôles faits ici (identifiant, taille, bornes), un 400 du
/// site ne peut plus être qu'un refus de forme déjà exclu ou le refus
/// uniforme de `definirMembres` : tout 400 devient donc `Refuses`.
pub fn definir_membres(
    config: &Config,
    coffre: &Coffre,
    id: i64,
    membres: &[i64],
) -> Result<DefinitionMembres, ErreurCompte> {
    if id <= 0 {
        return Err(refuse_avant_le_reseau("identifiant de liste invalide"));
    }
    if !membres_valides(membres) {
        return Err(refuse_avant_le_reseau("membres invalides : 200 identifiants au plus, chacun positif"));
    }
    let client = ClientHttp::new(config);
    let chemin = format!("/api/sky/lists/{id}/members");
    let demande = CorpsMembres { membre_ids: membres };
    avec_jeton_valide(config, coffre, |jeton| {
        match client.envoyer_json_reponse_vide_avec_refus("PUT", &chemin, &demande, Some(jeton))? {
            ReponseHttp::Succes(()) => Ok(DefinitionMembres::Definis),
            ReponseHttp::Refus { statut: 400, .. } => Ok(DefinitionMembres::Refuses),
            ReponseHttp::Refus { statut, corps } => Err(statut_inattendu(statut, corps)),
        }
    })
}
