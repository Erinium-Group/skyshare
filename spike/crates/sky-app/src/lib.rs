//! L'application SkyShare (jalon 1) : la coquille Tauri — fenêtre, icône
//! près de l'horloge, instance unique, démarrage avec Windows.

pub mod cadence;
pub mod commandes;
pub mod coquille;
pub mod demarrage;
pub mod materiel;
pub mod noyau;
pub mod reveil;
pub mod vue;

#[cfg(test)]
mod essais;
// Le serveur double de `sky-compte`, partagé plutôt que recopié : un second
// double divergerait du premier, qui est dérivé du code du site.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../sky-compte/tests/faux_serveur/mod.rs"]
mod faux_serveur;

use std::sync::Arc;

use sky_compte::{Coffre, Config};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use crate::coquille::CoquilleTauri;
use crate::noyau::{Branchements, Noyau};
use crate::reveil::SommeilReel;

/// Identifiant de l'icône près de l'horloge — la tâche 11 la retrouve par lui.
pub const ID_ICONE: &str = "principal";

pub fn lancer() {
    let au_demarrage = demarrage::lance_au_demarrage(std::env::args());
    tauri::Builder::default()
        // Spec D4 : une seule instance. La seconde remet la première au premier
        // plan puis s'arrête — elle ne synchronise jamais.
        .plugin(tauri_plugin_single_instance::init(|app, _arguments, _dossier| montrer_fenetre(app)))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![demarrage::ARGUMENT_DEMARRAGE]),
        ))
        .invoke_handler(tauri::generate_handler![
            commandes::etat_courant,
            commandes::connexion,
            commandes::deconnexion,
        ])
        .setup(move |app| {
            installer_icone(app.handle())?;
            #[cfg(not(debug_assertions))]
            activer_au_premier_lancement(app.handle());
            let noyau = Arc::new(Noyau::nouveau(
                Config::depuis_env(),
                Coffre::nouveau()?,
                Branchements {
                    coquille: Box::new(CoquilleTauri::nouvelle(app.handle().clone())),
                    connecter: Box::new(sky_compte::connecter),
                    nom_machine: std::env::var("COMPUTERNAME").ok(),
                },
            ));
            noyau.definir_demarrage_automatique_connu(app.autolaunch().is_enabled().unwrap_or(false));
            noyau.definir_visible(!au_demarrage);
            app.manage(Arc::clone(&noyau));
            // La boucle unique, sur son propre fil (spec §3).
            std::thread::Builder::new().name("synchronisation".into()).spawn(move || {
                noyau.demarrer();
                noyau.boucle(&mut SommeilReel, None);
            })?;
            if !au_demarrage {
                montrer_fenetre(app.handle());
            }
            Ok(())
        })
        .on_window_event(|fenetre, evenement| {
            let noyau = fenetre.try_state::<Arc<Noyau>>();
            match evenement {
                WindowEvent::CloseRequested { api, .. } => {
                    // Fermer réduit (spec D4) : l'application reste près de
                    // l'horloge ; seul « Quitter » du menu de l'icône la ferme.
                    api.prevent_close();
                    let _ = fenetre.hide();
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(false);
                    }
                }
                WindowEvent::Focused(true) => {
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(true);
                    }
                }
                WindowEvent::Resized(_) if fenetre.is_minimized().unwrap_or(false) => {
                    if let Some(noyau) = noyau {
                        noyau.definir_visible(false);
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("échec du lancement de SkyShare");
}

fn montrer_fenetre(app: &AppHandle) {
    if let Some(fenetre) = app.get_webview_window("main") {
        let _ = fenetre.unminimize();
        let _ = fenetre.show();
        let _ = fenetre.set_focus();
    }
    if let Some(noyau) = app.try_state::<Arc<Noyau>>() {
        noyau.definir_visible(true);
    }
}

fn installer_icone(app: &AppHandle) -> tauri::Result<()> {
    let ouvrir = MenuItem::with_id(app, "ouvrir", "Ouvrir SkyShare", true, None::<&str>)?;
    let quitter = MenuItem::with_id(app, "quitter", "Quitter", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&ouvrir, &quitter])?;
    TrayIconBuilder::with_id(ID_ICONE)
        .icon(tauri::include_image!("icons/32x32.png"))
        .tooltip("SkyShare")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, evenement| match evenement.id.as_ref() {
            "ouvrir" => montrer_fenetre(app),
            "quitter" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|icone, evenement| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = evenement {
                montrer_fenetre(icone.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Spec D4 : l'application démarre avec la session Windows. Activé UNE fois,
/// au premier lancement de la version publiée : si l'utilisateur le désactive
/// ensuite (Mon compte), il reste désactivé.
#[cfg(not(debug_assertions))]
fn activer_au_premier_lancement(app: &AppHandle) {
    if let Ok(dossier) = app.path().app_data_dir() {
        if demarrage::premier_lancement(&dossier).unwrap_or(false) {
            let _ = app.autolaunch().enable();
        }
    }
}

#[cfg(test)]
mod tests {
    /// Arbitrage du contrôleur (tâche 6) : la version de développement et de
    /// test ne doit jamais toucher aux données de la version installée. Ce
    /// test s'exécute toujours en profil `debug` (`cargo test`) ; il prouve
    /// que `build.rs` a bien substitué l'identifiant `.dev` à celui de la
    /// version publiée pour ce profil.
    ///
    /// Neutralisation : retirer la substitution `TAURI_CONFIG` dans
    /// `build.rs` — l'identifiant redevient `fr.jlskyzer.skyshare` et ce test
    /// rougit.
    #[test]
    fn l_identifiant_de_developpement_differe_de_celui_publie() {
        let contexte: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let identifiant = contexte.config().identifier.clone();
        assert_eq!(identifiant, "fr.jlskyzer.skyshare.dev");
        assert_ne!(identifiant, "fr.jlskyzer.skyshare");
    }
}
