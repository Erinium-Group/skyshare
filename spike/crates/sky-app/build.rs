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

    // CONSCIENCE DPI DE L'EXÉCUTABLE (jalon 2, tâche 10). Sans elle, à une
    // mise à l'échelle de 150 % ou 200 % — le réglage par défaut de la plupart
    // des portables —, Windows virtualise les coordonnées de la fenêtre de
    // visionnage : la chaîne d'échange serait créée à la taille logique puis
    // ÉTIRÉE, et le texte de l'écran partagé deviendrait flou. C'est tout
    // l'argument du produit.
    //
    // Pourquoi le MANIFESTE et pas un appel au démarrage :
    // - le réglage appartient au processus et doit précéder toute fenêtre ;
    //   déclaré dans le manifeste, il est posé par Windows avant la première
    //   instruction, sans dépendre de l'ordre d'initialisation de quiconque ;
    // - `tao` (sous Tauri) appelle bien `SetProcessDpiAwarenessContext`
    //   (PER_MONITOR_AWARE_V2) à la création de sa boucle d'événements — lu
    //   dans `tao-0.35.3/src/platform_impl/windows/dpi.rs` —, mais c'est un
    //   réglage par défaut d'une dépendance, désactivable, pas une décision de
    //   ce projet. Le manifeste rend la propriété explicite et vérifiable dans
    //   le binaire. Quand le manifeste l'a posé, l'appel de `tao` échoue sans
    //   effet (il ignore le résultat) : les deux ne se contredisent pas.
    //
    // Le manifeste remplace celui de `tauri-build` : il en reprend donc la
    // dépendance aux contrôles communs v6.
    println!("cargo:rerun-if-changed=manifeste-windows.xml");
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("manifeste-windows.xml"));
    if let Err(erreur) =
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
    {
        // Même sortie que `tauri_build::build()`, qui n'accepte pas d'attributs.
        println!("{erreur:#}");
        std::process::exit(1);
    }
}
