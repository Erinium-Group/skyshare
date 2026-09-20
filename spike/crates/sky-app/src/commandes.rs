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
