//! `sky-compte` : logique cliente vers l'API du site (connexion, amis,
//! boîte aux lettres à enveloppes scellées).
//!
//! Ce crate n'installe aucun runtime asynchrone — `ureq` est synchrone,
//! c'est précisément pourquoi il a été choisi pour ce client.

pub mod coffre;
pub mod erreur;
pub mod http;
pub mod session;

pub use coffre::{Coffre, Jetons};
pub use erreur::ErreurCompte;
pub use http::{ClientHttp, Config};
pub use session::{
    avec_jeton_valide, connecter, echanger_le_code, empreinte_du_secret, jeton_courant, renouveler,
    secret_aleatoire, url_de_depart,
};
