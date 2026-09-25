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
//!
//! TÂCHE 11 : `icone_partage` arrive, avec son appelant — `Noyau::partager` et
//! `Noyau::terminer`.
//!
//! ARBITRAGE DU CONTRÔLEUR (tâche 11) : `publier_partage`, annoncé par les
//! tâches 7 et 10, N'ARRIVE PAS, et le second événement Tauri « partage » non
//! plus. `PartageVue` est un CHAMP de l'`Instantane` que `publier_etat` émet
//! déjà, et `app/src/useInstantane.ts` n'écoute que l'événement `etat` : un
//! second canal porterait exactement la même valeur, sans lecteur. C'est la
//! classe de défaut que `CLAUDE.md` nomme « la serrure posée mais jamais
//! branchée ». Si le jalon 2 a besoin d'un flux d'événements BRUTS (ceux que
//! `appliquer` jette aujourd'hui), c'est alors qu'il faudra l'ajouter — avec
//! son lecteur.

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

    /// L'icône près de l'horloge change tant que dure un partage (spec §4) :
    /// on ne partage jamais sans le savoir. Rien ne remonte : une icône qui ne
    /// change pas n'empêche aucun partage, et il n'y a rien à faire d'un échec.
    fn icone_partage(&self, actif: bool);
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

    fn icone_partage(&self, actif: bool) {
        if let Some(icone) = self.app.tray_by_id(crate::ID_ICONE) {
            let image = if actif {
                tauri::include_image!("icons/partage/32x32.png")
            } else {
                tauri::include_image!("icons/32x32.png")
            };
            let _ = icone.set_icon(Some(image));
            let _ = icone.set_tooltip(Some(if actif { "SkyShare — en partage" } else { "SkyShare" }));
        }
    }
}
