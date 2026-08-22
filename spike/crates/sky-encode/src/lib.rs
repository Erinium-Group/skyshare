pub mod caps;
pub mod nvenc;

// Plomberie FFI partagée entre `caps` (détection) et la session d'encodage
// D3D11 (`nvenc`) — interne à la crate, pas exposée aux consommateurs de
// sky-encode (ex. sky-probe).
pub(crate) mod nvenc_sys;

pub use caps::{pick_best, probe_hardware, Codec, EncodeError, EncoderCaps};
pub use nvenc::NvencEncoder;

/// Un paquet encodé, prêt à partir sur le réseau.
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    /// Durée de l'appel complet à `NvencEncoder::encode` (enregistrement de la
    /// ressource, encodage, récupération des octets), en microsecondes.
    pub encode_us: u64,
}
