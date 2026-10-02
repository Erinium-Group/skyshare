pub mod wgc;

use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

/// Une image capturée. La texture vit sur le GPU et n'est jamais copiée en RAM.
///
/// `Clone` ne copie pas l'image : il prend une référence de plus sur la même
/// texture. Une texture rendue par `WgcCapture::next_frame` retourne au pool de
/// capture, qui peut la réécrire à la capture suivante : qui veut garder une
/// image doit la copier dans une texture qu'il possède.
#[derive(Clone)]
pub struct CapturedFrame {
    pub texture: ID3D11Texture2D,
    pub width: u32,
    pub height: u32,
    pub captured_at: Instant,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureStats {
    pub frames: u64,
    pub dropped: u64,
    pub avg_fps: f32,
}
