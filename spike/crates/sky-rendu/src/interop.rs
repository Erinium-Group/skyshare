//! Pont entre la mémoire CUDA où NVDEC écrit et les textures Direct3D 11 que la
//! fenêtre affiche.
//!
//! Les textures sont enregistrées UNE FOIS auprès de CUDA, puis chaque image y
//! est copiée de périphérique à périphérique (décision D5).
//!
//! Pourquoi c'est structurant : une image décodée en 4:4:4 à 2560×1440 fait
//! 11 059 200 octets, soit 663 Mo/s à 60 im/s et 1,18 Go/s aux 107 im/s mesurées
//! au jalon 0. Un aller-retour par la mémoire centrale ne tient pas ce débit.
//!
//! **Trois textures à un canal, et non une à trois canaux.** Le choix est dicté
//! par la forme de la source : NVDEC rend un 4:4:4 *planaire*, trois plans Y, U,
//! V qui se suivent verticalement dans une seule allocation, chacun avec son pas
//! de ligne. Trois textures `R8_UNORM` reçoivent donc chacune un plan en un seul
//! `cuMemcpy2D` rectangulaire, qui est exactement ce que la disposition offre.
//! Une texture à quatre canaux exigerait d'entrelacer les composantes, donc une
//! lecture-écriture par pixel — un noyau CUDA à écrire et à maintenir, pour un
//! résultat que le nuanceur obtient gratuitement en échantillonnant trois plans.
//! (Et `DXGI_FORMAT` n'offre aucun format à trois canaux de 8 bits : ce serait
//! de toute façon quatre canaux, dont un inutile, soit un tiers de bande passante
//! gâché.)

use std::ffi::c_void;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use cudarc::driver::sys as cu;
use cudarc::driver::CudaContext;
use libloading::os::windows::{Library as WinLibrary, LOAD_LIBRARY_SEARCH_SYSTEM32};
use libloading::Library;
use sky_decode::SurfaceCuda;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC};

/// Ce que le pont sait afficher : une image YUV 4:4:4 encore sur le GPU.
///
/// Un trait plutôt que le type concret `ImageDecodee` pour une raison précise :
/// `ImageDecodee::nouvelle` est `pub(crate)` dans `sky-decode` et exige une
/// session NVDEC vivante, donc **aucun code hors de `sky-decode` ne peut en
/// fabriquer une**. Le test de couleur a pourtant besoin d'une surface dont il
/// connaisse la couleur exacte, sans décoder — sinon il mesurerait le décodage,
/// que la tâche 2 mesure déjà. Le trait laisse `afficher` traverser exactement le
/// même chemin pour l'image du décodeur et pour celle du test.
pub trait ImageAAfficher {
    /// Largeur d'affichage en pixels.
    fn largeur(&self) -> u32;
    /// Hauteur d'affichage en pixels.
    fn hauteur(&self) -> u32;
    /// La poignée de surface GPU, sans aucun octet de pixel.
    fn surface(&self) -> SurfaceCuda;
}

impl ImageAAfficher for sky_decode::ImageDecodee {
    fn largeur(&self) -> u32 {
        self.largeur
    }
    fn hauteur(&self) -> u32 {
        self.hauteur
    }
    fn surface(&self) -> SurfaceCuda {
        sky_decode::ImageDecodee::surface(self)
    }
}

/// Nom de la DLL du pilote CUDA.
const NVCUDA_DLL: &str = "nvcuda.dll";

/// `CU_GRAPHICS_REGISTER_FLAGS_NONE`. Écrit en clair parce que `cudarc` ne lie
/// pas la fonction d'enregistrement Direct3D 11, donc pas non plus son énumérée
/// de drapeaux côté D3D.
const REGISTER_FLAGS_NONE: u32 = 0;

type RegisterD3D11ResourceFn = unsafe extern "C" fn(
    *mut cu::CUgraphicsResource,
    *mut c_void, // ID3D11Resource*
    u32,
) -> cu::CUresult;

/// La seule fonction d'interopérabilité que `cudarc` ne fournit pas.
///
/// `cuGraphicsD3D11RegisterResource` est déclarée dans `cudaD3D11.h`, que le
/// générateur de liaisons de `cudarc` ne lit pas ; tout le reste du chemin
/// (`cuGraphicsMapResources`, `cuGraphicsSubResourceGetMappedArray`,
/// `cuMemcpy2D`, `cuGraphicsUnmapResources`, `cuGraphicsUnregisterResource`)
/// vient de `cuda.h` et est donc déjà lié. On résout celle-ci dans la DLL du
/// pilote, comme `sky-decode` le fait pour `nvcuvid.dll`.
struct ApiD3D11 {
    // Ne sert qu'à garder la DLL chargée : le pointeur ci-dessous pointe dedans.
    _lib: Library,
    enregistrer_ressource: RegisterD3D11ResourceFn,
}

impl ApiD3D11 {
    fn charger() -> anyhow::Result<Self> {
        // Recherche restreinte à System32, comme pour `nvcuvid.dll` : le
        // répertoire courant et le `PATH` sont le vecteur classique de
        // « DLL planting ».
        let win_lib =
            unsafe { WinLibrary::load_with_flags(NVCUDA_DLL, LOAD_LIBRARY_SEARCH_SYSTEM32) }
                .with_context(|| format!("chargement de {NVCUDA_DLL}"))?;
        let lib = Library::from(win_lib);
        let enregistrer_ressource = *unsafe { lib.get(b"cuGraphicsD3D11RegisterResource\0") }
            .context("cuGraphicsD3D11RegisterResource introuvable dans nvcuda.dll")?;
        Ok(Self {
            _lib: lib,
            enregistrer_ressource,
        })
    }
}

/// Un plan : la vue par laquelle le nuanceur lit sa texture, et la ressource
/// CUDA qui désigne cette même texture.
///
/// La texture elle-même n'est pas gardée ici : la vue et l'enregistrement CUDA
/// en détiennent chacun une référence COM, donc elle survit à la poignée Rust
/// qui l'a créée. La garder serait une quatrième référence dont personne ne se
/// sert.
struct Plan {
    vue: ID3D11ShaderResourceView,
    ressource: cu::CUgraphicsResource,
}

/// Les trois plans d'une image, enregistrés auprès de CUDA.
pub(crate) struct Pont {
    /// Le contexte CUDA dans lequel les ressources sont enregistrées. Gardé
    /// vivant : les désenregistrer dans `Drop` exige qu'il existe encore.
    contexte: Arc<CudaContext>,
    plans: [Plan; 3],
    largeur: u32,
    hauteur: u32,
}

impl Pont {
    pub(crate) fn nouveau(
        appareil: &ID3D11Device,
        largeur: u32,
        hauteur: u32,
    ) -> anyhow::Result<Self> {
        if largeur == 0 || hauteur == 0 {
            return Err(anyhow!("taille d'image nulle : {largeur}×{hauteur}"));
        }
        // Chargée pour la durée de cette fonction seulement : le seul pointeur
        // qu'on en tire, `enregistrer_ressource`, n'est appelé que d'ici. Les
        // poignées `CUgraphicsResource` qu'il produit, elles, sont manipulées
        // ensuite par les fonctions que `cudarc` résout dans le même `nvcuda.dll`,
        // que son chargement paresseux garde en mémoire pour la vie du processus.
        let api = ApiD3D11::charger()?;
        // `CudaContext::new` retient le contexte **primaire** du périphérique 0
        // (`cuDevicePrimaryCtxRetain`), celui-là même que `sky-decode` retient :
        // les deux crates travaillent donc dans le même contexte, condition sine
        // qua non d'une copie de périphérique à périphérique. Le périphérique 0
        // n'est pas un choix libre ici, c'est celui que `sky-decode` décode sur.
        let contexte = CudaContext::new(0).context("contexte CUDA")?;

        let mut plans = Vec::with_capacity(3);
        for numero in 0..3 {
            let (texture, vue) = creer_texture_de_plan(appareil, largeur, hauteur)
                .with_context(|| format!("texture du plan {numero}"))?;
            let mut ressource: cu::CUgraphicsResource = std::ptr::null_mut();
            // `as_raw` rend le pointeur d'interface ; `ID3D11Texture2D` hérite de
            // `ID3D11Resource`, que CUDA attend.
            let code = unsafe {
                (api.enregistrer_ressource)(&mut ressource, texture.as_raw(), REGISTER_FLAGS_NONE)
            };
            verifier(code)
                .with_context(|| format!("cuGraphicsD3D11RegisterResource sur le plan {numero}"))?;
            // La poignée Rust de la texture peut tomber : la vue et
            // l'enregistrement CUDA en détiennent chacun une référence COM. Voir
            // `struct Plan`.
            drop(texture);
            plans.push(Plan { vue, ressource });
        }
        let plans: [Plan; 3] = plans
            .try_into()
            .map_err(|_| anyhow!("trois plans attendus"))?;

        Ok(Self {
            contexte,
            plans,
            largeur,
            hauteur,
        })
    }

    pub(crate) fn largeur(&self) -> u32 {
        self.largeur
    }

    pub(crate) fn hauteur(&self) -> u32 {
        self.hauteur
    }

    /// Les trois vues à donner au nuanceur, dans l'ordre Y, U, V.
    pub(crate) fn vues(&self) -> [Option<ID3D11ShaderResourceView>; 3] {
        [
            Some(self.plans[0].vue.clone()),
            Some(self.plans[1].vue.clone()),
            Some(self.plans[2].vue.clone()),
        ]
    }

    /// Copie la surface décodée dans les textures, sans repasser par l'hôte.
    pub(crate) fn televerser(&mut self, image: &dyn ImageAAfficher) -> anyhow::Result<()> {
        if (image.largeur(), image.hauteur()) != (self.largeur, self.hauteur) {
            return Err(anyhow!(
                "image {}×{} pour un pont {}×{}",
                image.largeur(),
                image.hauteur(),
                self.largeur,
                self.hauteur
            ));
        }
        // Les appels CUDA lisent le contexte courant du FIL, que n'importe quel
        // autre code du même fil peut avoir remplacé par un `cuCtxSetCurrent`.
        // Même précaution que `SessionNvdec::rendre_contexte_courant`.
        self.contexte.bind_to_thread().context("contexte CUDA")?;

        let surface = image.surface();
        let mut ressources = [
            self.plans[0].ressource,
            self.plans[1].ressource,
            self.plans[2].ressource,
        ];
        verifier(unsafe {
            cu::cuGraphicsMapResources(3, ressources.as_mut_ptr(), std::ptr::null_mut())
        })
        .context("cuGraphicsMapResources")?;

        // Le démappage doit avoir lieu même si une copie échoue : sans lui, la
        // texture resterait indéfiniment prêtée à CUDA et Direct3D ne pourrait
        // plus l'échantillonner.
        let resultat = self.copier_les_plans(&surface);
        let demappage = verifier(unsafe {
            cu::cuGraphicsUnmapResources(3, ressources.as_mut_ptr(), std::ptr::null_mut())
        })
        .context("cuGraphicsUnmapResources");

        resultat.and(demappage)
    }

    /// Un `cuMemcpy2D` par plan, périphérique vers tableau CUDA — donc de la
    /// mémoire du GPU vers la texture du GPU, sans détour par l'hôte.
    fn copier_les_plans(&self, surface: &SurfaceCuda) -> anyhow::Result<()> {
        // L'espacement des plans se calcule sur `hauteur_surface`, la hauteur de
        // la surface de SORTIE mappée, et non sur la hauteur codée du flux. Voir
        // la documentation de `SurfaceCuda` : mesuré en 1920×1080, où la hauteur
        // codée vaut 1088 et fait déborder cette copie.
        let saut_de_plan = u64::from(surface.pas) * u64::from(surface.hauteur_surface);
        for numero in 0..3u64 {
            let mut tableau: cu::CUarray = std::ptr::null_mut();
            verifier(unsafe {
                cu::cuGraphicsSubResourceGetMappedArray(
                    &mut tableau,
                    self.plans[numero as usize].ressource,
                    0,
                    0,
                )
            })
            .with_context(|| format!("cuGraphicsSubResourceGetMappedArray sur le plan {numero}"))?;

            let copie = cu::CUDA_MEMCPY2D {
                srcXInBytes: 0,
                srcY: 0,
                srcMemoryType: cu::CUmemorytype::CU_MEMORYTYPE_DEVICE,
                srcHost: std::ptr::null(),
                srcDevice: surface.pointeur + numero * saut_de_plan,
                srcArray: std::ptr::null_mut(),
                srcPitch: surface.pas as usize,
                dstXInBytes: 0,
                dstY: 0,
                // Le tableau vit dans la texture Direct3D : la destination est
                // sur le périphérique, tout comme la source. Aucun octet de pixel
                // ne touche la mémoire centrale (décision D5).
                dstMemoryType: cu::CUmemorytype::CU_MEMORYTYPE_ARRAY,
                dstHost: std::ptr::null_mut(),
                dstDevice: 0,
                dstArray: tableau,
                // Ignoré pour une destination de type tableau.
                dstPitch: 0,
                WidthInBytes: self.largeur as usize,
                Height: self.hauteur as usize,
            };
            verifier(unsafe { cu::cuMemcpy2D_v2(&copie) })
                .with_context(|| format!("cuMemcpy2D sur le plan {numero}"))?;
        }
        Ok(())
    }
}

impl Drop for Pont {
    fn drop(&mut self) {
        // Désenregistrer exige le contexte courant. Sans lui, rien à faire : on
        // ne peut pas signaler depuis un `Drop`, et l'échec se répare encore
        // moins.
        if self.contexte.bind_to_thread().is_err() {
            return;
        }
        for plan in &self.plans {
            unsafe { cu::cuGraphicsUnregisterResource(plan.ressource) };
        }
    }
}

/// Une texture à un canal pour un plan, et sa vue de lecture.
fn creer_texture_de_plan(
    appareil: &ID3D11Device,
    largeur: u32,
    hauteur: u32,
) -> anyhow::Result<(ID3D11Texture2D, ID3D11ShaderResourceView)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: largeur,
        Height: hauteur,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        // `DEFAULT` et non `DYNAMIC` : l'enregistrement CUDA refuse une texture
        // accessible au processeur, et de toute façon personne ne l'écrit depuis
        // l'hôte.
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let mut texture = None;
    unsafe { appareil.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .context("création de la texture de plan")?;
    let texture = texture.ok_or_else(|| anyhow!("texture de plan nulle"))?;
    let mut vue = None;
    unsafe { appareil.CreateShaderResourceView(&texture, None, Some(&mut vue)) }
        .context("vue de lecture du plan")?;
    let vue = vue.ok_or_else(|| anyhow!("vue de plan nulle"))?;
    Ok((texture, vue))
}

/// Transforme un code de retour CUDA en `Result`.
pub(crate) fn verifier(code: cu::CUresult) -> anyhow::Result<()> {
    if code == cu::CUresult::CUDA_SUCCESS {
        Ok(())
    } else {
        Err(anyhow!("appel CUDA en échec (code {})", code as i32))
    }
}
