//! Point d'entrée de test minimal pour `tests/faux_serveur/mod.rs`.
//!
//! Cargo ne découvre automatiquement que les fichiers directement sous
//! `tests/` (`tests/*.rs`) comme cibles de test — un `tests/<dossier>/mod.rs`
//! seul, sans fichier `tests/<dossier>.rs` à côté, n'est jamais compilé.
//! Le double lui-même vit dans `tests/faux_serveur/mod.rs` (pas ici) : les
//! tâches suivantes du jalon (6, 7, 9) qui ont besoin du double dans LEUR
//! propre binaire de test l'incluent de la même façon, avec le même
//! `#[path = ...]`, plutôt que de dupliquer son code.
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;
