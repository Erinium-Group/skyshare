//! Fenêtre native de visionnage : sa chaîne d'échange DXGI et les états qu'elle
//! affiche quand il n'y a pas d'image.
//!
//! Pourquoi une fenêtre native et pas la vue web de l'application : une image
//! décodée en 4:4:4 à 2560×1440 pèse 11 059 200 octets, soit 663 Mo/s à 60
//! images par seconde. Aucun pont d'IPC ne tient ce débit.

mod etat;
mod fenetre;

pub use etat::EtatVisionnage;
pub use fenetre::{EvenementFenetre, Fenetre};
