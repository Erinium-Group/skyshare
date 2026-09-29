//! Décodage vidéo par NVDEC : le miroir de `sky-encode` côté réception.

mod capacites;
mod nvcuvid_sys;

pub use capacites::{sonder_materiel, Capacites, ErreurDecodeur};
