//! Le périphérique Direct3D 11 unique, pour la sonde comme pour la fenêtre.

use anyhow::{anyhow, Context};
use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0,
};
use windows::Win32::Graphics::Direct3D10::ID3D10Multithread;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, DXGI_ERROR_NOT_FOUND,
};

/// Identifiant de fabricant PCI de NVIDIA.
const FABRICANT_NVIDIA: u32 = 0x10DE;

/// Le premier adaptateur DXGI de NVIDIA, s'il y en a un.
fn adaptateur_nvidia() -> anyhow::Result<Option<IDXGIAdapter1>> {
    let fabrique: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.context("fabrique DXGI")?;
    // `EnumAdapters1` échoue avec DXGI_ERROR_NOT_FOUND une fois la liste épuisée.
    // On ne traite QUE ce code comme une fin de liste : n'importe quelle autre
    // erreur doit remonter. Sans ce filtre, une énumération réellement cassée
    // rendrait `None`, ferait replier l'appareil sur l'adaptateur par défaut —
    // possiblement l'Intel intégré — et l'interopérabilité CUDA casserait sans que
    // rien ne le dise.
    for indice in 0.. {
        let adaptateur = match unsafe { fabrique.EnumAdapters1(indice) } {
            Ok(adaptateur) => adaptateur,
            Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(e) => return Err(anyhow::Error::new(e).context("énumération des adaptateurs DXGI")),
        };
        let desc = unsafe { adaptateur.GetDesc1() }.context("description de l'adaptateur")?;
        if desc.VendorId == FABRICANT_NVIDIA {
            return Ok(Some(adaptateur));
        }
    }
    Ok(None)
}

/// LE périphérique Direct3D 11 de SkyShare : celui de la fenêtre, du décodage
/// Media Foundation et de la sonde de décodage. Il n'est créé qu'ici : deux
/// règles de choix d'adaptateur divergeraient, et la sonde validerait une carte
/// pendant que la fenêtre en ouvrirait une autre (spec §4, `sky-decode`).
pub fn creer_appareil_video() -> anyhow::Result<(ID3D11Device, ID3D11DeviceContext)> {
    // Pourquoi on choisit l'adaptateur au lieu de passer `None` : sur un portable
    // à deux cartes graphiques, l'adaptateur par défaut peut être l'Intel
    // intégré. Or l'interopérabilité CUDA exige que la texture Direct3D et la
    // surface NVDEC vivent sur LA MÊME carte, celle de NVIDIA : sur l'Intel, la
    // copie de périphérique à périphérique échouerait. Ne pas simplifier en `None`.
    // La même règle sert désormais aussi au décodage Media Foundation : sur une
    // machine NVIDIA, c'est la carte NVIDIA qui décode le 4:2:0 (spec D4).
    //
    // Repli sur l'adaptateur par défaut quand aucune carte NVIDIA n'existe : sur
    // une machine sans NVIDIA, l'adaptateur par défaut est aussi celui qui décode
    // (Media Foundation).
    let nvidia = adaptateur_nvidia()?;
    let (adaptateur, pilote): (Option<IDXGIAdapter>, _) = match &nvidia {
        // Avec un adaptateur explicite, Direct3D exige le type de pilote UNKNOWN.
        Some(a) => (Some(a.cast()?), D3D_DRIVER_TYPE_UNKNOWN),
        None => (None, D3D_DRIVER_TYPE_HARDWARE),
    };
    let mut appareil = None;
    let mut contexte = None;
    unsafe {
        D3D11CreateDevice(
            adaptateur.as_ref(),
            pilote,
            HMODULE::default(),
            // BGRA : exigé pour que Direct2D dessine sur nos textures.
            // VIDEO_SUPPORT : posé par la recette éprouvée de la sonde AMD pour
            // le décodage DXVA que Media Foundation mène sur ce périphérique
            // (`spike/mesures/sonde-amd/source/src/dxva.rs`). Sur la RTX 4060,
            // l'ouverture Media Foundation réussit aussi sans lui (mesuré) ; sa
            // nécessité ailleurs est supposée, pas prouvée. Pas de couche de
            // débogage.
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut appareil),
            None,
            Some(&mut contexte),
        )
    }
    .context("création de l'appareil Direct3D 11")?;
    let appareil = appareil.ok_or_else(|| anyhow!("appareil Direct3D 11 nul"))?;
    let contexte = contexte.ok_or_else(|| anyhow!("contexte Direct3D 11 nul"))?;

    // La protection multi-fil est exigée par le gestionnaire de périphérique de
    // Media Foundation (`IMFDXGIDeviceManager`) : le décodeur peut toucher au
    // contexte depuis ses propres fils. Le jalon 2 l'avait délibérément laissée
    // éteinte, faute de second utilisateur du périphérique ; ce second
    // utilisateur existe désormais. Le coût est une prise de verrou par appel de
    // contexte. L'OUVERTURE du décodeur réussit aussi sans elle (mesuré sur la
    // RTX 4060, tâche 4) : ce qu'elle protège, ce sont les appels de décodage,
    // et seul le test de protection ci-dessous la tient aujourd'hui.
    let multi: ID3D10Multithread = appareil.cast().context("ID3D10Multithread")?;
    // La valeur rendue est l'état PRÉCÉDENT de la protection, sans intérêt ici.
    let _ = unsafe { multi.SetMultithreadProtected(true) };
    Ok((appareil, contexte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D11::ID3D11VideoDevice;
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;

    #[test]
    fn l_appareil_est_protege_contre_les_acces_concurrents() {
        let (appareil, _) =
            creer_appareil_video().expect("ce test exige un périphérique Direct3D 11 matériel");
        let multi: ID3D10Multithread = appareil.cast().unwrap();
        assert!(unsafe { multi.GetMultithreadProtected() }.as_bool());
    }

    #[test]
    fn l_appareil_offre_l_api_video() {
        let (appareil, _) =
            creer_appareil_video().expect("ce test exige un périphérique Direct3D 11 matériel");
        let _: ID3D11VideoDevice = appareil
            .cast()
            .expect("ID3D11VideoDevice refusé : pas d'API vidéo sur ce périphérique");
        // L'interface seule NE PROUVE PAS le drapeau : sur la RTX 4060 (pilote
        // du 02/10/2026), elle est accordée même sans VIDEO_SUPPORT, et Media
        // Foundation s'ouvre aussi (neutralisation de la tâche 4, mesurée). Le
        // drapeau est gardé parce que la recette éprouvée sur AMD le pose, sans
        // preuve qu'il y soit nécessaire ; c'est donc lui qu'on vérifie
        // directement, faute de conséquence observable ici.
        let drapeaux = unsafe { appareil.GetCreationFlags() };
        assert_ne!(
            drapeaux & D3D11_CREATE_DEVICE_VIDEO_SUPPORT.0,
            0,
            "périphérique créé sans D3D11_CREATE_DEVICE_VIDEO_SUPPORT (drapeaux 0x{drapeaux:X})"
        );
    }

    /// L'appareil doit vivre sur la carte NVIDIA quand il y en a une : c'est ce
    /// qui rend possible l'interopérabilité CUDA de la fenêtre (`sky-rendu`).
    #[test]
    fn l_appareil_vit_sur_la_carte_nvidia_quand_il_y_en_a_une() {
        let nvidia = adaptateur_nvidia().expect("énumération DXGI");
        // Recoupement par une source indépendante de DXGI : le pilote CUDA. Sans
        // lui, un `None` venu d'une énumération cassée ferait sortir ce test en
        // silence — c'est-à-dire exactement dans le cas qu'il doit attraper. Les
        // deux sources doivent s'accorder. (Même appel que
        // `sky_rendu::cartes_cuda_disponibles`.)
        let cartes_cuda = cudarc::driver::CudaContext::device_count().unwrap_or(0);
        assert_eq!(
            nvidia.is_some(),
            cartes_cuda > 0,
            "DXGI et le pilote CUDA ne s'accordent pas sur la présence d'une carte NVIDIA \
             (DXGI : {:?}, cartes CUDA : {cartes_cuda})",
            nvidia.is_some()
        );
        if nvidia.is_none() {
            println!("pas de carte NVIDIA sur cette machine : repli légitime, rien à prouver");
            return;
        }
        let (appareil, _) = creer_appareil_video().expect("appareil Direct3D 11");
        let dxgi: IDXGIDevice = appareil.cast().expect("IDXGIDevice");
        let adaptateur: IDXGIAdapter = unsafe { dxgi.GetAdapter() }.expect("adaptateur");
        let desc = unsafe { adaptateur.GetDesc() }.expect("description");
        assert_eq!(
            desc.VendorId, FABRICANT_NVIDIA,
            "l'appareil n'est pas sur la carte NVIDIA"
        );
    }
}
