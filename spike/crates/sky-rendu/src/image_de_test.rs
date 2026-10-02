//! Des images YUV unies, fabriquées sans décodeur : 4:4:4 en mémoire CUDA
//! (`ImageUnie`), et NV12 en texture Direct3D 11 (`ImageNv12DeTest`).
//!
//! RÉSERVÉ AUX TESTS ET AUX MESURES. Pourquoi ça existe : `ImageDecodee` et
//! `ImageMf` ne sont constructibles que par `sky-decode` (il y faut une session
//! NVDEC ou un décodeur Media Foundation vivants). Un test de couleur qui
//! passerait par le décodeur mesurerait le décodage au lieu de mesurer la
//! traversée jusqu'au nuanceur, qui est la seule chose que ce crate ajoute.
//!
//! La disposition 4:4:4 reproduit exactement celle de NVDEC : une allocation
//! unique avec un pas de ligne, trois plans qui se suivent verticalement,
//! espacés de la hauteur de la surface. Sans quoi le test emprunterait un chemin
//! que la production n'emprunte pas. L'image NV12, elle, n'est pas une tranche
//! de tableau liée au seul décodeur comme celle de Media Foundation : c'est le
//! test d'image réelle (`tests/image_nv12.rs`) qui couvre cette forme-là.

use std::sync::Arc;

use anyhow::{anyhow, Context};
use cudarc::driver::sys as cu;
use cudarc::driver::CudaContext;
use sky_decode::{SourceImage, SurfaceCuda};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};

use crate::interop::{verifier, ImageAAfficher};

/// Une image unie sur le GPU, dont on connaît la couleur exacte.
pub struct ImageUnie {
    /// Gardé vivant : libérer l'allocation dans `Drop` exige que le contexte
    /// existe encore.
    contexte: Arc<CudaContext>,
    surface: SurfaceCuda,
    largeur: u32,
    hauteur: u32,
}

impl ImageAAfficher for ImageUnie {
    fn largeur(&self) -> u32 {
        self.largeur
    }
    fn hauteur(&self) -> u32 {
        self.hauteur
    }
    fn source(&self) -> SourceImage<'_> {
        SourceImage::Cuda444(self.surface)
    }
}

/// Fabrique une image de test unie sans passer par le décodeur.
///
/// `yuv` donne les trois composantes, chacune constante sur tout son plan.
pub fn image_de_test_unie(largeur: u32, hauteur: u32, yuv: [u8; 3]) -> anyhow::Result<ImageUnie> {
    anyhow::ensure!(
        largeur > 0 && hauteur > 0,
        "taille d'image nulle : {largeur}×{hauteur}"
    );
    // Le même contexte primaire que `sky-decode` et que le pont : voir
    // `Pont::nouveau`.
    let contexte = CudaContext::new(0).context("contexte CUDA")?;
    contexte.bind_to_thread().context("contexte CUDA")?;

    // Une seule allocation de `3 × hauteur` lignes, et `cuMemAllocPitch` pour
    // que le pas soit celui que le matériel préfère — donc, comme chez NVDEC, un
    // pas supérieur à la largeur.
    let mut pointeur: cu::CUdeviceptr = 0;
    let mut pas: usize = 0;
    verifier(unsafe {
        cu::cuMemAllocPitch_v2(
            &mut pointeur,
            &mut pas,
            largeur as usize,
            hauteur as usize * 3,
            // `ElementSizeBytes` n'accepte que 4, 8 ou 16 : les autres tailles
            // interdisent les transactions mémoire fusionnées, et CUDA rend
            // `CUDA_ERROR_INVALID_VALUE`. Ce n'est pas la taille d'un pixel mais
            // celle de l'accès pour lequel le pas est aligné ; 16 est le plus
            // large, donc celui qui aligne le mieux.
            16,
        )
    })
    .context("cuMemAllocPitch pour l'image de test")?;

    let image = ImageUnie {
        contexte,
        surface: SurfaceCuda {
            pointeur,
            pas: pas as u32,
            // Les plans sont espacés d'exactement `hauteur` lignes ici, ce qui est
            // aussi le cas d'une surface NVDEC : l'espacement suit la hauteur de la
            // surface de sortie, pas la hauteur codée.
            hauteur_surface: hauteur,
        },
        largeur,
        hauteur,
    };

    // Construite avant les remplissages : si l'un échoue, `Drop` libère déjà
    // l'allocation.
    for (numero, valeur) in yuv.iter().enumerate() {
        verifier(unsafe {
            cu::cuMemsetD2D8_v2(
                pointeur + (numero * pas * hauteur as usize) as u64,
                pas,
                *valeur,
                largeur as usize,
                hauteur as usize,
            )
        })
        .with_context(|| format!("remplissage du plan {numero}"))?;
    }
    Ok(image)
}

impl Drop for ImageUnie {
    fn drop(&mut self) {
        // Rien à signaler depuis un `Drop`, et un contexte qu'on n'arrive pas à
        // reposer rendrait la libération inopérante de toute façon.
        if self.contexte.bind_to_thread().is_err() {
            return;
        }
        unsafe { cu::cuMemFree_v2(self.surface.pointeur) };
    }
}

/// Une image NV12 unie dans une texture Direct3D 11, dont on connaît la couleur
/// exacte. Elle s'affiche par le même chemin que l'image de Media Foundation
/// (`SourceImage::Nv12`), en tranche 0 d'une texture d'une seule tranche.
pub struct ImageNv12DeTest {
    texture: ID3D11Texture2D,
    largeur: u32,
    hauteur: u32,
}

impl ImageAAfficher for ImageNv12DeTest {
    fn largeur(&self) -> u32 {
        self.largeur
    }
    fn hauteur(&self) -> u32 {
        self.hauteur
    }
    fn source(&self) -> SourceImage<'_> {
        SourceImage::Nv12 {
            texture: &self.texture,
            tranche: 0,
        }
    }
}

/// Fabrique une image NV12 unie sur `appareil`, sans décodeur.
///
/// `yuv` donne Y (constant sur le plan de luminance) puis U et V (constants sur
/// le plan de chrominance entrelacé). La taille doit être paire, comme toute
/// texture NV12.
pub fn image_de_test_nv12(
    appareil: &ID3D11Device,
    largeur: u32,
    hauteur: u32,
    yuv: [u8; 3],
) -> anyhow::Result<ImageNv12DeTest> {
    anyhow::ensure!(
        largeur > 0 && hauteur > 0 && largeur.is_multiple_of(2) && hauteur.is_multiple_of(2),
        "taille d'image NV12 nulle ou impaire : {largeur}×{hauteur}"
    );
    // Disposition NV12 en mémoire : `hauteur` lignes de Y, puis `hauteur / 2`
    // lignes de U et V entrelacés, toutes au même pas de `largeur` octets.
    let (l, h) = (largeur as usize, hauteur as usize);
    let mut octets = vec![yuv[0]; l * h * 3 / 2];
    for paire in octets[l * h..].chunks_exact_mut(2) {
        paire[0] = yuv[1];
        paire[1] = yuv[2];
    }
    let desc = D3D11_TEXTURE2D_DESC {
        Width: largeur,
        Height: hauteur,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_NV12,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    // Données initiales d'un format planaire : un seul pointeur et un seul pas,
    // le plan UV suivant le plan Y sans intervalle.
    let donnees = D3D11_SUBRESOURCE_DATA {
        pSysMem: octets.as_ptr().cast(),
        SysMemPitch: largeur,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    unsafe { appareil.CreateTexture2D(&desc, Some(&donnees), Some(&mut texture)) }
        .context("création de la texture NV12 de test")?;
    let texture = texture.ok_or_else(|| anyhow!("texture NV12 de test nulle"))?;
    Ok(ImageNv12DeTest {
        texture,
        largeur,
        hauteur,
    })
}
