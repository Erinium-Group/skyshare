pub mod wgc;

use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

/// Une image capturée. La texture vit sur le GPU et n'est jamais copiée en RAM.
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
