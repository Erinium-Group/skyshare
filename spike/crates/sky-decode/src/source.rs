use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

use crate::SurfaceCuda;

/// D'où l'affichage tire une image décodée.
pub enum SourceImage<'a> {
    /// Trois plans 4:4:4 en mémoire CUDA (NVDEC).
    Cuda444(SurfaceCuda),
    /// Une tranche d'un tableau de textures NV12 (Media Foundation), liée au
    /// décodeur seul : à copier avant d'être lue par un nuanceur.
    Nv12 {
        texture: &'a ID3D11Texture2D,
        tranche: u32,
    },
}
