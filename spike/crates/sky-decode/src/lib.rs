//! Décodage vidéo, le miroir de `sky-encode` côté réception : NVDEC pour le
//! HEVC 4:4:4, Media Foundation en matériel pour le HEVC 4:2:0 et le H.264. Le
//! crate crée aussi le périphérique Direct3D 11 unique sur lequel tous deux
//! travaillent, et la sonde de ce que la machine sait décoder.

mod appareil;
mod capacites;
mod decodeur;
mod image;
mod media_foundation;
mod nvcuvid_sys;
mod sonde;

pub use appareil::creer_appareil_video;
pub use capacites::{sonder_materiel, Capacites, ErreurDecodeur};
pub use decodeur::Decodeur;
pub use image::{ImageDecodee, SurfaceCuda};
pub use media_foundation::{mf_sait_decoder, CodecMf, DecodeurMf};
pub use sonde::{sonder_decodage, Decodables};
