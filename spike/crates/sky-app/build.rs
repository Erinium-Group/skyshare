fn main() {
    // Arbitrage du contrôleur (tâche 6) : la version de développement et de
    // test ne doit jamais toucher aux données de la version installée. Sous
    // le profil `debug` (cargo test, cargo run), l'identifiant de
    // l'application reçoit le suffixe `.dev` par-dessus tauri.conf.json —
    // Tauri fusionne `TAURI_CONFIG` (JSON) au moment de la construction.
    // Sans cela, l'instance unique et le trousseau verraient la version de
    // développement et la version publiée comme une seule et même
    // application.
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        // `build.rs` tourne dans un processus séparé : `set_var` ici ne
        // changerait rien à la compilation de la crate elle-même. Il faut
        // passer par `cargo:rustc-env`, que Cargo répercute sur l'invocation
        // de `rustc` qui suit — c'est elle qui exécute `generate_context!`.
        println!("cargo:rustc-env=TAURI_CONFIG={{\"identifier\":\"fr.jlskyzer.skyshare.dev\"}}");
    }
    tauri_build::build()
}
