//! Décodage vidéo par NVDEC : le miroir de `sky-encode` côté réception.

mod capacites;
mod decodeur;
mod image;
mod nvcuvid_sys;

pub use capacites::{sonder_materiel, Capacites, ErreurDecodeur};
pub use decodeur::Decodeur;
pub use image::{ImageDecodee, SurfaceCuda};
