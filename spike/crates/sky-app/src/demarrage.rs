//! Démarrage avec la session Windows (spec D4).

use std::path::Path;

/// Argument passé par l'extension de démarrage automatique : l'application
/// démarre alors cachée, près de l'horloge.
pub const ARGUMENT_DEMARRAGE: &str = "--demarrage";

pub fn lance_au_demarrage(arguments: impl IntoIterator<Item = String>) -> bool {
    arguments.into_iter().any(|a| a == ARGUMENT_DEMARRAGE)
}

/// `true` au tout premier appel pour ce dossier, `false` ensuite : un témoin
/// vide y est écrit.
pub fn premier_lancement(dossier: &Path) -> std::io::Result<bool> {
    let temoin = dossier.join("premier-lancement-fait");
    if temoin.exists() {
        return Ok(false);
    }
    std::fs::create_dir_all(dossier)?;
    std::fs::write(&temoin, b"")?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_premier_lancement_n_est_vu_qu_une_fois() {
        // Neutralisation : ne pas écrire le témoin — le second appel rend
        // encore `true` et le démarrage automatique serait réactivé à chaque
        // lancement, contre le choix de l'utilisateur.
        let dossier = std::env::temp_dir().join(format!("skyshare-test-premier-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dossier);
        assert!(premier_lancement(&dossier).unwrap());
        assert!(!premier_lancement(&dossier).unwrap());
        std::fs::remove_dir_all(&dossier).unwrap();
    }

    #[test]
    fn seul_l_argument_de_demarrage_cache_la_fenetre() {
        assert!(lance_au_demarrage(["sky-app.exe".to_string(), ARGUMENT_DEMARRAGE.to_string()]));
        assert!(!lance_au_demarrage(["sky-app.exe".to_string()]));
    }
}
