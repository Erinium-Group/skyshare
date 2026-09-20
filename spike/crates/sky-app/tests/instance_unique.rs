//! Instance unique (spec D4) : deux instances se voleraient les enveloppes,
//! que le serveur efface en les livrant. Ce test lance le VRAI exécutable
//! deux fois — c'est le branchement de l'extension qu'il prouve, pas
//! l'extension elle-même.
//!
//! `SKY_API_URL` vise un port fermé : aucune requête n'atteint le site.
//! `--demarrage` garde la fenêtre cachée. Le démarrage automatique n'est
//! activé qu'en version publiée : ce test n'écrit pas dans le registre.

use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn lancer() -> Child {
    Command::new(env!("CARGO_BIN_EXE_sky-app"))
        .arg("--demarrage")
        .env("SKY_API_URL", "http://127.0.0.1:1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("lancement de sky-app")
}

fn fin_dans(enfant: &mut Child, delai: Duration) -> Option<ExitStatus> {
    let limite = Instant::now() + delai;
    while Instant::now() < limite {
        if let Some(statut) = enfant.try_wait().expect("try_wait") {
            return Some(statut);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

#[test]
fn une_seconde_instance_rend_la_main_et_la_premiere_continue() {
    // Neutralisation : retirer `.plugin(tauri_plugin_single_instance::init(…))`
    // de `lancer()` — la seconde instance ne s'arrête pas en 15 s.
    let mut premiere = lancer();
    if let Some(statut) = fin_dans(&mut premiere, Duration::from_secs(5)) {
        panic!(
            "la première instance s'est arrêtée d'elle-même ({statut}) : une instance de SkyShare \
             tourne-t-elle déjà sur cette machine (version installée) ? La quitter, puis relancer."
        );
    }
    let mut seconde = lancer();
    let fin_seconde = fin_dans(&mut seconde, Duration::from_secs(15));
    let premiere_vivante = premiere.try_wait().expect("try_wait").is_none();

    let _ = seconde.kill();
    let _ = premiere.kill();
    let _ = seconde.wait();
    let _ = premiere.wait();

    let statut = fin_seconde.expect("la seconde instance tournait encore après 15 s : deux instances en même temps");
    assert!(statut.success(), "la seconde instance doit rendre la main proprement ({statut})");
    assert!(premiere_vivante, "la première instance doit continuer de tourner");
}
