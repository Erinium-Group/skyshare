//! L'application SkyShare (jalon 1) : la coquille Tauri — fenêtre, icône
//! près de l'horloge, instance unique, démarrage avec Windows.

pub mod demarrage;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

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
        .setup(move |app| {
            installer_icone(app.handle())?;
            #[cfg(not(debug_assertions))]
            activer_au_premier_lancement(app.handle());
            if !au_demarrage {
                montrer_fenetre(app.handle());
            }
            Ok(())
        })
        .on_window_event(|fenetre, evenement| {
            if let WindowEvent::CloseRequested { api, .. } = evenement {
                // Fermer réduit (spec D4) : l'application reste près de
                // l'horloge ; seul « Quitter » du menu de l'icône la ferme.
                api.prevent_close();
                let _ = fenetre.hide();
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
    use tauri_plugin_autostart::ManagerExt;
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
