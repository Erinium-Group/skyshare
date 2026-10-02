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
use sky_decode::{SourceImage, SurfaceCuda};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8_UNORM, DXGI_SAMPLE_DESC};

/// Ce que la fenêtre sait afficher : une image décodée, 4:4:4 en mémoire CUDA
/// (NVDEC) ou NV12 en texture Direct3D 11 (Media Foundation), encore sur le GPU.
///
/// Un trait plutôt que les types concrets du décodeur pour une raison précise :
/// `ImageDecodee::nouvelle` est `pub(crate)` dans `sky-decode` et exige une
/// session NVDEC vivante, et une `ImageMf` exige un décodeur Media Foundation —
/// donc **aucun code hors de `sky-decode` ne peut en fabriquer une**. Les tests
/// de couleur ont pourtant besoin d'une image dont ils connaissent la couleur
/// exacte, sans décoder — sinon ils mesureraient le décodage. Le trait laisse
/// `afficher` traverser exactement le même chemin pour l'image du décodeur et
/// pour celle du test.
pub trait ImageAAfficher {
    /// Largeur d'affichage en pixels.
    fn largeur(&self) -> u32;
    /// Hauteur d'affichage en pixels.
    fn hauteur(&self) -> u32;
    /// D'où lire l'image : une poignée GPU, sans aucun octet de pixel.
    fn source(&self) -> SourceImage<'_>;
}

impl ImageAAfficher for sky_decode::ImageDecodee {
    fn largeur(&self) -> u32 {
        self.largeur
    }
    fn hauteur(&self) -> u32 {
        self.hauteur
    }
    fn source(&self) -> SourceImage<'_> {
        SourceImage::Cuda444(sky_decode::ImageDecodee::surface(self))
    }
}

impl ImageAAfficher for sky_decode::ImageMf {
    fn largeur(&self) -> u32 {
        self.largeur
    }
    fn hauteur(&self) -> u32 {
        self.hauteur
    }
    fn source(&self) -> SourceImage<'_> {
        SourceImage::Nv12 {
            texture: self.texture(),
            tranche: self.tranche(),
        }
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

        // `Pont` n'existe pas encore, donc son `Drop` ne s'exécutera pas : sur
        // échec au milieu de l'enregistrement, les plans déjà pris doivent être
        // désenregistrés À LA MAIN, sans quoi ils fuient. Même motif que
        // `Fenetre::creer`, qui libère sa boîte et détruit sa fenêtre par la même
        // logique. Ce chemin n'a rien de rare : il est rejoué à chaque changement
        // de taille de la fenêtre.
        let mut plans: Vec<Plan> = Vec::with_capacity(3);
        for numero in 0..3 {
            match enregistrer_un_plan(appareil, &api, largeur, hauteur) {
                Ok(plan) => plans.push(plan),
                Err(e) => {
                    desenregistrer(&contexte, &plans);
                    return Err(e.context(format!("plan {numero}")));
                }
            }
        }
        let plans: [Plan; 3] = match plans.try_into() {
            Ok(plans) => plans,
            Err(deja_pris) => {
                desenregistrer(&contexte, &deja_pris);
                return Err(anyhow!("trois plans attendus"));
            }
        };

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

    /// Copie la surface décodée, d'une image `taille`, dans les textures, sans
    /// repasser par l'hôte.
    pub(crate) fn televerser(
        &mut self,
        taille: (u32, u32),
        surface: SurfaceCuda,
    ) -> anyhow::Result<()> {
        if taille != (self.largeur, self.hauteur) {
            return Err(anyhow!(
                "image {}×{} pour un pont {}×{}",
                taille.0,
                taille.1,
                self.largeur,
                self.hauteur
            ));
        }
        // Les appels CUDA lisent le contexte courant du FIL, que n'importe quel
        // autre code du même fil peut avoir remplacé par un `cuCtxSetCurrent`.
        // Même précaution que `SessionNvdec::rendre_contexte_courant`.
        self.contexte.bind_to_thread().context("contexte CUDA")?;

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

            let copie = decrire_copie_de_plan(
                surface.pointeur + numero * saut_de_plan,
                surface.pas,
                tableau,
                self.largeur,
                self.hauteur,
            );
            verifier(unsafe { cu::cuMemcpy2D_v2(&copie) })
                .with_context(|| format!("cuMemcpy2D sur le plan {numero}"))?;
        }
        Ok(())
    }
}

impl Drop for Pont {
    fn drop(&mut self) {
        desenregistrer(&self.contexte, &self.plans);
    }
}

/// Décrit la copie d'un plan : source sur le périphérique, destination dans le
/// tableau CUDA que la texture Direct3D expose.
///
/// **Fonction pure, et séparée exprès.** Ce que la décision D5 exige de CETTE
/// copie tient dans quatre champs — les deux types de mémoire et les deux
/// pointeurs hôte. Les sortir ici les rend affirmables par un test **sans GPU**,
/// qui rougit en nommant D5 au lieu de rendre un code d'erreur CUDA opaque.
///
/// **Portée exacte de cette garantie, mesurée et non supposée.** Le test épingle
/// la forme de cette copie-ci : changer un `DEVICE` en `HOST` le fait rougir. Il
/// ne prouve **pas** l'absence d'autres copies — un aller-retour par l'hôte ajouté
/// *autour* de ce descripteur le laisse vert, c'est vérifié. Ne pas croire D5
/// scellée par ce seul test : il ferme la porte la plus probable, pas toutes. Le
/// reste dépend de la relecture et de la mesure de débit de la tâche 11.
fn decrire_copie_de_plan(
    source: cu::CUdeviceptr,
    pas: u32,
    destination: cu::CUarray,
    largeur: u32,
    hauteur: u32,
) -> cu::CUDA_MEMCPY2D {
    cu::CUDA_MEMCPY2D {
        srcXInBytes: 0,
        srcY: 0,
        srcMemoryType: cu::CUmemorytype::CU_MEMORYTYPE_DEVICE,
        srcHost: std::ptr::null(),
        srcDevice: source,
        srcArray: std::ptr::null_mut(),
        srcPitch: pas as usize,
        dstXInBytes: 0,
        dstY: 0,
        // Le tableau vit dans la texture Direct3D : la destination est sur le
        // périphérique, tout comme la source.
        dstMemoryType: cu::CUmemorytype::CU_MEMORYTYPE_ARRAY,
        dstHost: std::ptr::null_mut(),
        dstDevice: 0,
        dstArray: destination,
        // Ignoré pour une destination de type tableau.
        dstPitch: 0,
        WidthInBytes: largeur as usize,
        Height: hauteur as usize,
    }
}

/// Désenregistre des plans auprès de CUDA.
///
/// Partagée entre `Drop` et le chemin d'échec de [`Pont::nouveau`], où le `Pont`
/// n'existe pas encore et où son `Drop` ne passera donc jamais. Une seule
/// implémentation, pour qu'on ne corrige pas un jour l'une en croyant l'autre
/// identique.
///
/// Désenregistrer exige le contexte courant. Sans lui, rien à faire : cette
/// fonction est appelée depuis un `Drop`, qui ne peut rien signaler, et un
/// désenregistrement raté ne se répare pas.
fn desenregistrer(contexte: &CudaContext, plans: &[Plan]) {
    if contexte.bind_to_thread().is_err() {
        return;
    }
    for plan in plans {
        unsafe { cu::cuGraphicsUnregisterResource(plan.ressource) };
    }
}

/// Crée la texture d'un plan et l'enregistre auprès de CUDA.
fn enregistrer_un_plan(
    appareil: &ID3D11Device,
    api: &ApiD3D11,
    largeur: u32,
    hauteur: u32,
) -> anyhow::Result<Plan> {
    let (texture, vue) = creer_texture_de_plan(appareil, largeur, hauteur).context("texture")?;
    let mut ressource: cu::CUgraphicsResource = std::ptr::null_mut();
    // `as_raw` rend le pointeur d'interface ; `ID3D11Texture2D` hérite de
    // `ID3D11Resource`, que CUDA attend.
    let code = unsafe {
        (api.enregistrer_ressource)(&mut ressource, texture.as_raw(), REGISTER_FLAGS_NONE)
    };
    verifier(code).context("cuGraphicsD3D11RegisterResource")?;
    // La poignée Rust de la texture peut tomber : la vue et l'enregistrement CUDA
    // en détiennent chacun une référence COM. Voir `struct Plan`.
    drop(texture);
    Ok(Plan { vue, ressource })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Le descripteur d'une copie de plan désigne la mémoire du périphérique en
    /// source et le tableau de la texture en destination — ce que la décision D5
    /// exige de cette copie.
    ///
    /// **Le nom dit la portée, et c'est délibéré.** Il a d'abord été
    /// `la_copie_d_un_plan_ne_touche_jamais_la_memoire_centrale`, ce qui promettait
    /// une propriété globale que ce test ne prouve pas. À un vert de `cargo test`
    /// on ne lit que le nom, jamais la note de portée : un nom qui promet plus que
    /// sa preuve est le mécanisme même par lequel ce dépôt a laissé passer des
    /// tests verts pour la mauvaise raison.
    ///
    /// Ce qui, précisément, ferait échouer ce test : faire passer cette copie par
    /// la mémoire centrale, ce qui exige `CU_MEMORYTYPE_HOST` d'un côté et le
    /// pointeur hôte correspondant. Vérifié par neutralisation.
    ///
    /// Ce qu'il ne prouve pas, et c'est mesuré : l'absence d'autres copies. Un
    /// aller-retour hôte ajouté autour de ce descripteur laisse ce test vert. Voir
    /// la note de portée sur [`decrire_copie_de_plan`].
    ///
    /// Aucun GPU requis : c'est le seul test de D5 qui tourne partout.
    #[test]
    fn d5_la_copie_d_un_plan_est_decrite_de_peripherique_a_tableau() {
        // Des valeurs quelconques : la fonction est pure, et ce test ne porte que
        // sur la désignation des deux extrémités. Le tableau de destination est non
        // nul pour qu'une destination oubliée se distingue d'une destination
        // correcte.
        let destination = 0x2000_usize as cu::CUarray;
        let copie = decrire_copie_de_plan(0x1000, 1024, destination, 640, 360);

        assert_eq!(
            copie.srcMemoryType,
            cu::CUmemorytype::CU_MEMORYTYPE_DEVICE,
            "la source doit être la mémoire du périphérique, pas celle de l'hôte (D5)"
        );
        assert_eq!(
            copie.dstMemoryType,
            cu::CUmemorytype::CU_MEMORYTYPE_ARRAY,
            "la destination doit être le tableau de la texture Direct3D (D5)"
        );
        assert!(
            copie.srcHost.is_null(),
            "un pointeur hôte en source signifierait une copie depuis la mémoire centrale (D5)"
        );
        assert!(
            copie.dstHost.is_null(),
            "un pointeur hôte en destination signifierait une copie vers la mémoire centrale (D5)"
        );
        assert_eq!(
            copie.dstArray, destination,
            "la destination doit être la texture qu'on a mappée"
        );

        // Les deux champs symétriques de ceux ci-dessus, et il faut être franc sur
        // ce qu'ils ajoutent : rien à la détection de D5. Les deux types de mémoire
        // sont déjà épinglés, et ce sont eux qui décident quelle mémoire est
        // touchée ; par ailleurs un `CUarray` vit lui aussi sur le périphérique,
        // donc même un descripteur qui en désignerait un en source ne violerait pas
        // D5. Ce que ces deux assertions interdisent, c'est un descripteur
        // AMBIGU — deux sources ou deux destinations à la fois — c'est-à-dire un
        // champ laissé en place par une modification future. Garde-fou de lisibilité,
        // pas détection de plus.
        //
        // Que CUDA ignore bien ces champs est MESURÉ, et non repris de la
        // documentation (le SDK n'est pas installé, `cuda.h` n'est pas sur la
        // machine) : en posant `srcArray: destination` ici, seul ce test rougit —
        // les copies réelles de `tests/couleur.rs` et de `tests/image_reelle.rs`
        // continuent de rendre les pixels justes, à 1 niveau près. Le pilote n'a
        // donc pas lu ce champ.
        assert!(
            copie.srcArray.is_null(),
            "un tableau source en plus d'une mémoire de périphérique rendrait la \
             source ambiguë"
        );
        assert_eq!(
            copie.dstDevice, 0,
            "une mémoire de périphérique en destination en plus du tableau rendrait \
             la destination ambiguë"
        );
    }
}
