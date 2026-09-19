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
pub mod listes;
pub mod session;

pub use annuaire::{
    accepter_ami, ajouter_ami, bloquer_ami, enregistrer_appareil, nom_appareil_valide,
    normaliser_code_ami, rattacher_appareil, regenerer_code, resoudre_ami, retirer_ami,
    revoquer_appareil, synchroniser, Acceptation, Ami, AjoutAmi, Appareil, AppareilDAmi, Blocage,
    Demande, EnveloppeRecue, Etat, Liste, Rattachement, Retrait,
};
pub use boite::{deposer, relever, Message, TAILLE_MAX_CLAIR};
pub use coffre::{Coffre, Jetons};
pub use erreur::ErreurCompte;
pub use http::{ClientHttp, Config, ReponseHttp};
pub use identite::{moi, Moi};
pub use listes::{
    creer_liste, definir_membres, modifier_liste, supprimer_liste, ChampsListe, CreationListe,
    DefinitionMembres, ModificationListe, SuppressionListe,
};
pub use session::{
    avec_jeton_valide, connecter, echanger_le_code, empreinte_du_secret, jeton_courant, renouveler,
    secret_aleatoire, url_de_depart,
};
