//! Identité de l'utilisateur courant, via `GET /api/auth/me`.
//!
//! `connecter` (`session.rs`) ne rend que des `Jetons`, et l'`Etat` de
//! `annuaire.rs` (`GET /api/sky/sync`) ne porte pas l'utilisateur courant —
//! cette route est le SEUL moyen pour ce client de connaître le nom Discord
//! affiché après connexion.

use serde::Deserialize;

use crate::coffre::Coffre;
use crate::erreur::ErreurCompte;
use crate::http::{ClientHttp, Config};
use crate::session::avec_jeton_valide;

/// Utilisateur courant, tel que rendu par `GET /api/auth/me`.
///
/// La route rend bien d'autres champs côté site (`discordId`,
/// `discordAvatar`, `totpEnabled`, `isStaff`, ...,
/// `src/app/api/auth/me/route.ts`) : seuls les deux dont ce client a
/// l'usage sont portés ici — un champ JSON inconnu d'une struct
/// `Deserialize` est ignoré par défaut, pas besoin de les déclarer pour les
/// ignorer proprement.
#[derive(Debug, Clone, PartialEq)]
pub struct Moi {
    pub id: i64,
    pub discord_name: String,
}

/// Forme brute de `GET /api/auth/me` : `discordName` est en camelCase, forme
/// exacte de la réponse du site (voir `route.ts`).
#[derive(Debug, Deserialize)]
struct MoiBrut {
    id: i64,
    #[serde(rename = "discordName")]
    discord_name: String,
}

/// Récupère l'identité de l'utilisateur courant.
///
/// La route rend `403` sur une session partielle (second facteur TOTP non
/// encore vérifié) et `404` si l'utilisateur n'existe plus — aucun des deux
/// n'a de traitement particulier ici : ce sont des
/// `ErreurCompte::Protocole` ordinaires, comme tout statut que ce client ne
/// distingue pas explicitement de ce contrat.
pub fn moi(config: &Config, coffre: &Coffre) -> Result<Moi, ErreurCompte> {
    let client = ClientHttp::new(config);
    let brut: MoiBrut =
        avec_jeton_valide(config, coffre, |jeton| client.get_json("/api/auth/me", Some(jeton)))?;
    Ok(Moi { id: brut.id, discord_name: brut.discord_name })
}
