//! Les commandes Tauri : des intentions de l'interface, confiées au `Noyau`
//! sur un fil dédié (`spawn_blocking`) — jamais sur le fil de l'interface :
//! `sky-compte` bloque le temps d'une requête (5 s au plus), `connexion` le
//! temps d'une connexion Discord (5 minutes au plus). C'est le seul fichier du
//! cœur qui contient des `async fn`.

use std::sync::Arc;

use tauri::State;

use crate::noyau::Noyau;
use crate::vue::Instantane;

pub(crate) async fn sur_un_fil<T: Send + 'static>(
    noyau: &Arc<Noyau>,
    travail: impl FnOnce(&Arc<Noyau>) -> T + Send + 'static,
) -> Result<T, String> {
    let noyau = Arc::clone(noyau);
    tauri::async_runtime::spawn_blocking(move || travail(&noyau)).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn etat_courant(noyau: State<'_, Arc<Noyau>>) -> Result<Instantane, String> {
    Ok(noyau.instantane())
}

#[tauri::command]
pub async fn connexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.connexion()).await?
}

#[tauri::command]
pub async fn deconnexion(noyau: State<'_, Arc<Noyau>>) -> Result<(), String> {
    sur_un_fil(noyau.inner(), |n| n.deconnexion()).await?
}

/// `code` : la saisie BRUTE de l'interface. La normalisation appartient au
/// cœur (`sky_compte::normaliser_code_ami`, qui reproduit celle du site), pas à
/// l'interface : deux normalisations divergeraient à la première correction.
#[tauri::command]
pub async fn ajouter_ami(noyau: State<'_, Arc<Noyau>>, code: String) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.ajouter_ami(&code)).await?
}

/// `friendship_id` : identifiant d'AMITIÉ, jamais d'utilisateur — c'est ce que
/// lisent les routes `friends/{id}` du site.
#[tauri::command]
pub async fn accepter_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.accepter_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn retirer_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.retirer_ami(friendship_id)).await?
}

#[tauri::command]
pub async fn bloquer_ami(
    noyau: State<'_, Arc<Noyau>>,
    friendship_id: i64,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.bloquer_ami(friendship_id)).await?
}

// --- Listes de diffusion ---------------------------------------------------
//
// Les arguments voyagent en camelCase depuis l'interface : Tauri 2 résout
// chaque paramètre PAR SA CLÉ, sans repli — une clé absente est une erreur
// d'invocation, pas un `None` silencieux. `friendship_id` s'invoque donc
// `{ friendshipId }`, et les noms d'un seul mot ci-dessous restent identiques
// des deux côtés.

#[tauri::command]
pub async fn creer_liste(
    noyau: State<'_, Arc<Noyau>>,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.creer_liste(&nom, couleur.as_deref(), emoji.as_deref()))
        .await?
}

#[tauri::command]
pub async fn modifier_liste(
    noyau: State<'_, Arc<Noyau>>,
    id: i64,
    nom: String,
    couleur: Option<String>,
    emoji: Option<String>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| {
        n.modifier_liste(id, &nom, couleur.as_deref(), emoji.as_deref())
    })
    .await?
}

#[tauri::command]
pub async fn supprimer_liste(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.supprimer_liste(id)).await?
}

/// `membres` : identifiants d'UTILISATEUR des amis cochés, jamais d'amitié.
#[tauri::command]
pub async fn definir_membres(
    noyau: State<'_, Arc<Noyau>>,
    id: i64,
    membres: Vec<i64>,
) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.definir_membres(id, &membres)).await?
}

// --- Mon compte ------------------------------------------------------------

#[tauri::command]
pub async fn regenerer_code(noyau: State<'_, Arc<Noyau>>) -> Result<String, String> {
    sur_un_fil(noyau.inner(), |n| n.regenerer_code()).await?
}

#[tauri::command]
pub async fn revoquer_appareil(noyau: State<'_, Arc<Noyau>>, id: i64) -> Result<String, String> {
    sur_un_fil(noyau.inner(), move |n| n.revoquer_appareil(id)).await?
}

/// Touche le registre de Windows par la coquille : `spawn_blocking` comme les
/// autres, jamais le fil de l'interface.
#[tauri::command]
pub async fn demarrage_automatique(
    noyau: State<'_, Arc<Noyau>>,
    actif: bool,
) -> Result<(), String> {
    sur_un_fil(noyau.inner(), move |n| n.demarrage_automatique(actif)).await?
}
