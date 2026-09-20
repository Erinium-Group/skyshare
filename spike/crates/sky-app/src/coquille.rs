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

use tauri::{AppHandle, Emitter};

use crate::vue::Instantane;

pub trait Coquille: Send + Sync {
    /// Événement `etat` : l'instantané complet, après chaque changement.
    fn publier_etat(&self, instantane: &Instantane);
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
}
