//! `sky-compte` : logique cliente vers l'API du site (connexion, amis,
//! boîte aux lettres à enveloppes scellées).
//!
//! Ce crate n'installe aucun runtime asynchrone — `ureq` est synchrone,
//! c'est précisément pourquoi il a été choisi pour ce client.

pub mod annuaire;
pub mod boite;
pub mod coffre;
pub mod erreur;
pub mod http;
pub mod identite;
pub mod session;

pub use annuaire::{
    accepter_ami, ajouter_ami, enregistrer_appareil, normaliser_code_ami, resoudre_ami,
    synchroniser, Acceptation, Ami, AjoutAmi, Appareil, AppareilDAmi, Demande, EnveloppeRecue, Etat,
};
pub use boite::{deposer, relever, Message, TAILLE_MAX_CLAIR};
pub use coffre::{Coffre, Jetons};
pub use erreur::ErreurCompte;
pub use http::{ClientHttp, Config, ReponseHttp};
pub use identite::{moi, Moi};
pub use session::{
    avec_jeton_valide, connecter, echanger_le_code, empreinte_du_secret, jeton_courant, renouveler,
    secret_aleatoire, url_de_depart,
};
