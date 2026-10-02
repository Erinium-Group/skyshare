//! Fenêtre native de visionnage : sa chaîne d'échange DXGI, les états qu'elle
//! affiche quand il n'y a pas d'image, et le chemin GPU qui y dépose les images
//! décodées.
//!
//! Pourquoi une fenêtre native et pas la vue web de l'application : une image
//! décodée en 4:4:4 à 2560×1440 pèse 11 059 200 octets, soit 663 Mo/s à 60
//! images par seconde. Aucun pont d'IPC ne tient ce débit.

// `nvidia-video-codec-sdk`, tiré par `sky-decode`, référence
// `NvEncodeAPICreateInstance` et `NvEncodeAPIGetMaxSupportedVersion` dans un bloc
// `extern "C"` lié statiquement. En debug sur MSVC, le lien les exige même si
// rien ne les appelle : le binaire de test de ce crate échouerait (LNK2019).
// `sky-encode/src/nvenc_sys.rs` les définit sans condition, et c'est la
// définition unique du dépôt — la redéfinir ici en produirait un doublon
// (LNK2005). Il suffit donc de lier `sky-encode`.
#[cfg(test)]
use sky_encode as _;

mod etat;
mod fenetre;
mod image_de_test;
mod interop;
mod nuanceur;
mod nv12;

pub use etat::EtatVisionnage;
pub use fenetre::{EvenementFenetre, Fenetre};
// RÉSERVÉ AUX TESTS ET AUX MESURES (voir l'en-tête du module) : des images
// unies fabriquées sans décodeur, 4:4:4 et NV12.
pub use image_de_test::{image_de_test_nv12, image_de_test_unie, ImageNv12DeTest, ImageUnie};
pub use interop::ImageAAfficher;

/// Le nombre de cartes que le pilote CUDA voit, ou 0 s'il n'est pas là.
///
/// Sert aux tests à distinguer deux situations qu'un simple échec confondrait :
/// une machine sans carte NVIDIA, où il n'y a rien à éprouver, et une machine qui
/// en a une mais dont le chemin GPU est cassé, qui doit rougir. Recoupement utile
/// aussi côté DXGI : le pilote CUDA est une source indépendante de l'énumération
/// des adaptateurs.
pub fn cartes_cuda_disponibles() -> i32 {
    cudarc::driver::CudaContext::device_count().unwrap_or(0)
}
