//! Point d'entrée de test minimal pour `tests/faux_serveur/mod.rs`.
//!
//! Cargo ne découvre automatiquement que les fichiers directement sous
//! `tests/` (`tests/*.rs`) comme cibles de test — un `tests/<dossier>/mod.rs`
//! seul, sans fichier `tests/<dossier>.rs` à côté, n'est jamais compilé.
//! Le double lui-même vit dans `tests/faux_serveur/mod.rs` (pas ici) : les
//! tâches suivantes du jalon (6, 7, 9) qui ont besoin du double dans LEUR
//! propre binaire de test l'incluent de la même façon, avec le même
//! `#[path = ...]`, plutôt que de dupliquer son code.
//!
//! Les tests du double lui-même (`faux_serveur/tests_du_double.rs`) ne sont inclus
//! QU'ICI : les autres binaires n'embarquent que le double, pas ses tests — sans quoi
//! chacun les exécuterait une fois de plus (revue finale, m1).
#[path = "faux_serveur/mod.rs"]
mod faux_serveur;

#[path = "faux_serveur/tests_du_double.rs"]
mod tests_du_double;
