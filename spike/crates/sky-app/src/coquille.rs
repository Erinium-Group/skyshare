//! La frontière avec Tauri : le noyau publie par ce trait, sans rien savoir
//! de la fenêtre. Les tests y branchent un espion.
//!
//! ARBITRAGE DU CONTRÔLEUR (tâche 7) : ce trait ne porte QUE ce que la tâche
//! 7 appelle réellement. Le brief y posait aussi `publier_partage` et
//! `demarrage_automatique`, dont le premier appelant n'arrive qu'aux tâches
//! 10 et 11 ; ils seront ajoutés par la tâche qui les branche. Une méthode
//! écrite, juste, testée et appelée par personne est exactement la classe de
//! défaut que ce projet a déjà payée trois fois (`CLAUDE.md`, « la serrure
//! posée mais jamais branchée »).
//!
//! TÂCHE 10 : `demarrage_automatique` arrive ici, avec son premier appelant —
//! la case « Lancer SkyShare au démarrage de Windows » de l'écran Mon compte.
//! `publier_partage` reste reporté à la tâche 11. Le registre de Windows ne
//! s'écrit pas depuis le noyau : c'est la coquille qui le touche, ce qui garde
//! `Noyau::demarrage_automatique` testable sans fenêtre ni écriture réelle.

use tauri::{AppHandle, Emitter};
use tauri_plugin_autostart::ManagerExt;

use crate::vue::Instantane;

pub trait Coquille: Send + Sync {
    /// Événement `etat` : l'instantané complet, après chaque changement.
    fn publier_etat(&self, instantane: &Instantane);

    /// Active ou non le lancement avec la session Windows (spec D4). L'échec
    /// remonte : une case qui se coche sans que rien ne change au démarrage
    /// serait un mensonge muet.
    fn demarrage_automatique(&self, actif: bool) -> Result<(), String>;
}

pub struct CoquilleTauri {
    app: AppHandle,
}

impl CoquilleTauri {
    pub fn nouvelle(app: AppHandle) -> CoquilleTauri {
        CoquilleTauri { app }
    }
}

impl Coquille for CoquilleTauri {
    fn publier_etat(&self, instantane: &Instantane) {
        let _ = self.app.emit("etat", instantane);
    }

    fn demarrage_automatique(&self, actif: bool) -> Result<(), String> {
        let gestionnaire = self.app.autolaunch();
        let issue = if actif { gestionnaire.enable() } else { gestionnaire.disable() };
        issue.map_err(|e| format!("Windows a refusé de changer le démarrage automatique : {e}"))
    }
}
