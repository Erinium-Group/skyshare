//! Une image YUV 4:4:4 unie, fabriquée sans décodeur.
//!
//! RÉSERVÉ AUX TESTS ET AUX MESURES. Pourquoi ça existe : `ImageDecodee` n'est
//! constructible que par `sky-decode` (son constructeur est `pub(crate)` et exige
//! une session NVDEC vivante). Un test de couleur qui passerait par le décodeur
//! mesurerait le décodage — ce que la tâche 2 mesure déjà — au lieu de mesurer
//! la traversée interopérabilité + nuanceur, qui est la seule chose que ce crate
//! ajoute.
//!
//! La disposition reproduit exactement celle de NVDEC : une allocation unique
//! avec un pas de ligne, trois plans qui se suivent verticalement, espacés de la
//! hauteur de la surface. Sans quoi le test emprunterait un chemin que la
//! production n'emprunte pas.

use std::sync::Arc;

use anyhow::Context;
use cudarc::driver::sys as cu;
use cudarc::driver::CudaContext;
use sky_decode::SurfaceCuda;

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
    fn surface(&self) -> SurfaceCuda {
        self.surface
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
