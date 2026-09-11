//! `sky-compte` : logique cliente vers l'API du site (connexion, amis,
//! boîte aux lettres à enveloppes scellées).
//!
//! Ce crate n'installe aucun runtime asynchrone — `ureq` est synchrone,
//! c'est précisément pourquoi il a été choisi pour ce client.

pub mod erreur;
pub mod http;

pub use erreur::ErreurCompte;
pub use http::{ClientHttp, Config};
