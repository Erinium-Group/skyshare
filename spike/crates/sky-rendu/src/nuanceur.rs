//! Le nuanceur qui convertit les trois plans YUV en pixels RVB, et sa
//! compilation.
//!
//! Le HLSL est une chaîne littérale et non un fichier chargé à l'exécution : la
//! build empaquetée doit rester autonome. La compilation passe par `D3DCompile`
//! (`d3dcompiler_47.dll`, livrée avec Windows) au moment où la fenêtre s'ouvre,
//! une fois par processus.

use anyhow::{anyhow, Context};
use windows::core::{s, PCSTR};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCompile, D3DCOMPILE_OPTIMIZATION_LEVEL3};
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11PixelShader, ID3D11SamplerState, ID3D11VertexShader,
    D3D11_COMPARISON_NEVER, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC,
    D3D11_TEXTURE_ADDRESS_CLAMP,
};

/// Le programme complet : nuanceur de sommets `sommet`, nuanceur de pixels
/// `pixel`.
const SOURCE: &str = r#"
// Trois plans à un canal : c'est la forme que NVDEC rend (4:4:4 planaire), et
// l'échantillonnage les recombine sans qu'aucun noyau CUDA n'ait à entrelacer
// quoi que ce soit. Voir l'en-tête de `interop.rs`.
Texture2D<float> plan_y : register(t0);
Texture2D<float> plan_u : register(t1);
Texture2D<float> plan_v : register(t2);
SamplerState echantillonneur : register(s0);

struct Sortie {
    float4 position : SV_Position;
    float2 coordonnees : TEXCOORD0;
};

// Quatre sommets déduits de leur indice, dessinés en bande de triangles : aucun
// tampon de sommets à créer ni à téléverser. Le rectangle produit couvre tout le
// viewport ; c'est le viewport, posé par l'appelant, qui préserve le rapport
// d'image et laisse les bandes noires autour.
Sortie sommet(uint indice : SV_VertexID) {
    float2 coin = float2(indice & 1, (indice >> 1) & 1);
    Sortie sortie;
    sortie.position = float4(coin.x * 2.0 - 1.0, 1.0 - coin.y * 2.0, 0.0, 1.0);
    sortie.coordonnees = coin;
    return sortie;
}

// BT.601 PLEINE ÉCHELLE, et non BT.709. Ce n'est pas le choix qu'on attend d'un
// flux HD, et c'est pourtant le bon : c'est la matrice avec laquelle la chaîne
// capture → encodage du jalon 0 a produit son YUV. Mesuré contre la référence du
// jalon 0 : BT.601 pleine échelle donne 85,50 dB (89,78 dB pour la mesure
// d'origine de la sonde, l'écart entre les deux n'étant qu'un arrondi entre le
// convertisseur en virgule fixe de swscale et un calcul en f64), là où BT.709
// tombe à 36,13 dB mesurés. Ne pas « corriger » vers BT.709 : le test de
// référence de sky-decode et le test de couleur de ce crate rougiraient tous les
// deux.
//
// Pleine échelle veut dire que Y couvre 0..255 et non 16..235 : aucun
// retranchement de 16, aucun facteur 255/219.
float3 yuv_vers_rgb(float3 yuv) {
    float y = yuv.x * 255.0;
    float u = yuv.y * 255.0 - 128.0;
    float v = yuv.z * 255.0 - 128.0;
    return float3(
        y + 1.402 * v,
        y - 0.344136 * u - 0.714136 * v,
        y + 1.772 * u
    ) / 255.0;
}

float4 pixel(Sortie entree) : SV_Target {
    float3 yuv = float3(
        plan_y.Sample(echantillonneur, entree.coordonnees),
        plan_u.Sample(echantillonneur, entree.coordonnees),
        plan_v.Sample(echantillonneur, entree.coordonnees));
    // Le bornage à 0..1 est celui de la cible `UNORM` elle-même : la matrice
    // BT.601 sort de l'intervalle sur des couples YUV que rien n'interdit au flux
    // de contenir (un rouge saturé donne déjà -0,2 en bleu). `saturate` le dit
    // explicitement plutôt que de le laisser au hasard du matériel.
    return float4(saturate(yuv_vers_rgb(yuv)), 1.0);
}
"#;

/// Les objets de pipeline que le rendu d'image réclame.
pub(crate) struct Programme {
    pub(crate) sommets: ID3D11VertexShader,
    pub(crate) pixels: ID3D11PixelShader,
    pub(crate) echantillonneur: ID3D11SamplerState,
}

impl Programme {
    pub(crate) fn compiler(appareil: &ID3D11Device) -> anyhow::Result<Self> {
        let code_sommets = compiler_etage(s!("sommet"), s!("vs_5_0"))?;
        let code_pixels = compiler_etage(s!("pixel"), s!("ps_5_0"))?;
        let mut sommets = None;
        unsafe { appareil.CreateVertexShader(octets(&code_sommets), None, Some(&mut sommets)) }
            .context("création du nuanceur de sommets")?;
        let mut pixels = None;
        unsafe { appareil.CreatePixelShader(octets(&code_pixels), None, Some(&mut pixels)) }
            .context("création du nuanceur de pixels")?;

        // Filtrage linéaire : l'image est mise à l'échelle dès que la fenêtre ne
        // fait pas exactement la taille du flux, et un filtrage au plus proche y
        // produirait des escaliers sur du texte. À l'échelle 1 pour 1, les deux
        // donnent le même résultat (les points d'échantillonnage tombent au centre
        // des texels), donc ce choix ne coûte rien à la justesse que le test de
        // couleur mesure.
        //
        // Bornage des coordonnées : sans lui, l'échantillonnage répéterait le bord
        // opposé sur la dernière demi-ligne de l'image.
        let description = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let mut echantillonneur = None;
        unsafe { appareil.CreateSamplerState(&description, Some(&mut echantillonneur)) }
            .context("état d'échantillonnage")?;

        Ok(Self {
            sommets: sommets.ok_or_else(|| anyhow!("nuanceur de sommets nul"))?,
            pixels: pixels.ok_or_else(|| anyhow!("nuanceur de pixels nul"))?,
            echantillonneur: echantillonneur.ok_or_else(|| anyhow!("échantillonneur nul"))?,
        })
    }
}

/// Compile un étage et rend son code objet. Le message du compilateur HLSL est
/// remonté tel quel : sans lui, une erreur de nuanceur n'est qu'un code HRESULT.
fn compiler_etage(point_d_entree: PCSTR, cible: PCSTR) -> anyhow::Result<ID3DBlob> {
    let mut code = None;
    let mut erreurs = None;
    let resultat = unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            s!("sky-rendu/nuanceur.rs"),
            None,
            None,
            point_d_entree,
            cible,
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut erreurs),
        )
    };
    if let Err(e) = resultat {
        let message = erreurs
            .map(|blob| {
                String::from_utf8_lossy(octets(&blob))
                    .trim_end_matches('\0')
                    .to_string()
            })
            .unwrap_or_default();
        return Err(anyhow!("compilation HLSL en échec : {e}\n{message}"));
    }
    code.ok_or_else(|| anyhow!("code objet HLSL nul"))
}

/// Les octets d'un code objet, tels que `CreateVertexShader` les attend.
fn octets(blob: &ID3DBlob) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(blob.GetBufferPointer().cast::<u8>(), blob.GetBufferSize())
    }
}

/// Le rectangle de destination qui préserve le rapport d'image : l'image est
/// agrandie autant que la cible le permet, centrée, et le reste laissé aux
/// bandes noires.
///
/// Déformer du texte serait inacceptable pour l'usage visé — on regarde
/// quelqu'un travailler, pas un film.
pub(crate) fn rectangle_centre(image: (u32, u32), cible: (u32, u32)) -> (f32, f32, f32, f32) {
    let (li, hi) = (image.0 as f32, image.1 as f32);
    let (lc, hc) = (cible.0 as f32, cible.1 as f32);
    let echelle = (lc / li).min(hc / hi);
    let (largeur, hauteur) = (li * echelle, hi * echelle);
    (
        ((lc - largeur) / 2.0).max(0.0),
        ((hc - hauteur) / 2.0).max(0.0),
        largeur,
        hauteur,
    )
}

#[cfg(test)]
mod tests {
    use super::rectangle_centre;

    /// Ce qui ferait échouer celui-ci : une mise à l'échelle qui étirerait l'image
    /// sur toute la cible. Le rapport 16:9 dans un carré doit laisser deux bandes.
    #[test]
    fn une_image_plus_large_que_la_cible_recoit_des_bandes_en_haut_et_en_bas() {
        let (x, y, largeur, hauteur) = rectangle_centre((1920, 1080), (800, 800));
        assert_eq!((x, largeur), (0.0, 800.0), "la largeur doit être saturée");
        assert_eq!(hauteur, 450.0, "1080 × 800/1920 = 450");
        assert_eq!(y, 175.0, "(800 - 450) / 2");
    }

    /// Le symétrique : une image plus haute que large reçoit des bandes sur les
    /// côtés. Sans ce cas, un `min` remplacé par un `max` passerait la moitié du
    /// temps.
    #[test]
    fn une_image_plus_haute_que_la_cible_recoit_des_bandes_sur_les_cotes() {
        let (x, y, largeur, hauteur) = rectangle_centre((1080, 1920), (800, 800));
        assert_eq!((y, hauteur), (0.0, 800.0), "la hauteur doit être saturée");
        assert_eq!(largeur, 450.0);
        assert_eq!(x, 175.0);
    }

    /// Rapports identiques : aucune bande, et le rectangle couvre exactement la
    /// cible. C'est le cas du plein écran, et celui où une erreur d'un demi-pixel
    /// se verrait.
    #[test]
    fn un_rapport_identique_ne_laisse_aucune_bande() {
        assert_eq!(
            rectangle_centre((1920, 1080), (960, 540)),
            (0.0, 0.0, 960.0, 540.0)
        );
    }
}
