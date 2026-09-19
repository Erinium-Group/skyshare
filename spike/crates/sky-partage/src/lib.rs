//! `sky-partage` : la négociation d'une connexion par la boîte aux lettres,
//! puis la diffusion et la réception (jalon 1, décision D5 de la spec).
//! Utilisée par `sky-probe` (affichage terminal) et par l'application
//! (`sky-app`). Elle ne fait AUCUNE sortie terminal : elle rend des
//! événements typés et accepte un signal d'arrêt.

pub mod arret;
pub mod rendez_vous;

pub use arret::{synchroniser_sauf_arret, Arret, ErreurAttente, HorlogeArretable, PAS_D_ATTENTE};
