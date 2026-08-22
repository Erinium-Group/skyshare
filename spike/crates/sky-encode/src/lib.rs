pub mod caps;

// Plomberie FFI partagée entre `caps` (détection) et la future session
// d'encodage de la Tâche 3 (`nvenc.rs`) — interne à la crate, pas exposée
// aux consommateurs de sky-encode (ex. sky-probe).
pub(crate) mod nvenc_sys;

pub use caps::{pick_best, probe_hardware, Codec, EncodeError, EncoderCaps};
