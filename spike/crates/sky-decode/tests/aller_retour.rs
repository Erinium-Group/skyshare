//! Aller-retour encodage → décodage à une hauteur que le flux du jalon 0 ne
//! couvre pas.
//!
//! Le flux du jalon 0 est en 2560×1440, où la hauteur codée vaut aussi 1440 :
//! aucun test bâti sur lui ne peut distinguer les deux candidats pour
//! l'espacement des plans de chrominance — la hauteur de la surface de sortie et
//! la hauteur codée du flux. Ce test fabrique son propre flux en **1920×1080**,
//! où les deux diffèrent : mesuré, la hauteur codée y vaut **1088** pour une
//! surface de sortie de **1080**. C'est le seul test du dépôt qui tranche, et il
//! le fait sur la résolution d'écran la plus répandue.
//!
//! Ce qu'il a établi, contre l'intuition tirée de l'échantillon `NvDecoder` de
//! NVIDIA : c'est la hauteur de la **surface de sortie** (`ulTargetHeight`) qui
//! commande, pas `coded_height`. Avec 1088, `cuMemcpy2D` déborde sur le
//! troisième plan.
//!
//! Le motif est fait de **bandes horizontales de 16 lignes**, rouge saturé puis
//! bleu saturé. Un décalage des plans de chrominance de quelques lignes déplace
//! la frontière de couleur sans déplacer celle de luminance : les pixels
//! concernés prennent alors la chrominance de la bande voisine, et un rouge
//! devient franchement bleu. C'est cette inversion que le test compte.

use sky_capture::CapturedFrame;
use sky_decode::Decodeur;
use sky_encode::{Codec, NvencEncoder};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
    D3D11_BIND_SHADER_RESOURCE, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

const LARGEUR: u32 = 1920;
const HAUTEUR: u32 = 1080;
/// Hauteur d'une bande de couleur, en lignes.
const BANDE: u32 = 16;
/// Nombre d'images encodées avant celle qu'on examine. La première image d'un
/// flux NVENC porte une rampe de qualité (piège du tampon VBV documenté au
/// jalon 0) ; on regarde la dernière.
const IMAGES: usize = 8;
/// Débit délibérément large : on veut prouver une disposition mémoire, pas
/// mesurer une compression.
const DEBIT_BPS: u32 = 80_000_000;
/// Proportion de pixels dont la couleur peut être mal classée. La compression
/// ne se trompe que sur quelques pixels de part et d'autre d'une frontière de
/// bande ; un décalage de plan en fait basculer la moitié de l'image.
const PART_MAL_CLASSEE_TOLEREE: f64 = 0.02;

#[test]
fn un_flux_1080_traverse_l_aller_retour_sans_decalage_de_chrominance() {
    let peripherique = match peripherique_direct3d11() {
        Some(p) => p,
        None => panic!("ce test exige un périphérique Direct3D 11 matériel"),
    };

    let mut encodeur = match NvencEncoder::new(
        &peripherique,
        Codec::Hevc444,
        LARGEUR,
        HAUTEUR,
        60,
        DEBIT_BPS,
    ) {
        Ok(e) => e,
        Err(e) => panic!("ce test exige un encodeur NVENC HEVC 4:4:4 : {e:#}"),
    };
    let texture = texture_a_bandes(&peripherique).expect("texture source");

    let mut paquets = Vec::new();
    for _ in 0..IMAGES {
        let image = CapturedFrame {
            texture: texture.clone(),
            width: LARGEUR,
            height: HAUTEUR,
            captured_at: std::time::Instant::now(),
        };
        if let Some(paquet) = encodeur.encode(&image).expect("encodage") {
            paquets.push(paquet.data);
        }
    }
    assert!(
        !paquets.is_empty(),
        "l'encodeur n'a produit aucun paquet : rien à décoder"
    );

    let mut decodeur = match Decodeur::nouveau(LARGEUR, HAUTEUR) {
        Ok(d) => d,
        Err(e) => panic!("ce test exige un décodeur NVIDIA HEVC 4:4:4 : {e}"),
    };

    let mut derniere = None;
    let mut hauteur_surface = 0;
    let mut hauteur_codee = 0;
    for (rang, paquet) in paquets.iter().enumerate() {
        if let Some(image) = decodeur.decoder(paquet, rang as u64).expect("décodage") {
            hauteur_surface = image.surface().hauteur_surface;
            hauteur_codee = image.hauteur_codee();
            derniere = Some(image.copier_vers_memoire_centrale().expect("copie de test"));
        }
    }
    let rgb = derniere.expect("au moins une image décodée");

    println!(
        "aller-retour {LARGEUR}×{HAUTEUR} : hauteur affichée {HAUTEUR}, \
         hauteur de surface {hauteur_surface}, hauteur codée du flux {hauteur_codee}"
    );

    // GARDE DE PERTINENCE, et elle porte sur la seule valeur qui ne vienne pas de
    // nous : `hauteur_codee` est ce que NVENC a écrit dans le flux et que NVDEC
    // nous rapporte. Tant qu'elle diffère de la hauteur d'affichage, ce test
    // distingue vraiment les deux candidats pour l'espacement des plans. Si un
    // jour NVENC codait 1080 en 1080, il ne prouverait plus rien — et il doit
    // alors le DIRE, pas passer en silence.
    //
    // Comparer `hauteur_surface` à `HAUTEUR` ne vaudrait rien : les deux sont
    // posées par notre propre code, depuis la même variable.
    assert_ne!(
        hauteur_codee, HAUTEUR,
        "ce test ne prouve plus rien : NVENC code désormais cette hauteur sans \
         alignement ({hauteur_codee}), donc la hauteur codée et la hauteur \
         d'affichage ne se distinguent plus. Choisir une autre géométrie."
    );

    let mal_classes = compter_bandes_mal_classees(&rgb);
    let total = (LARGEUR * HAUTEUR) as f64;
    let part = mal_classes as f64 / total;
    println!(
        "pixels mal classés : {mal_classes} sur {} ({:.3} %)",
        total as u64,
        part * 100.0
    );
    assert!(
        part <= PART_MAL_CLASSEE_TOLEREE,
        "{:.2} % des pixels ont la couleur de la bande voisine ({mal_classes} sur {}) : \
         les plans de chrominance sont lus au mauvais endroit",
        part * 100.0,
        total as u64
    );
}

/// Couleur attendue d'une ligne : `true` pour rouge, `false` pour bleu.
fn ligne_rouge(y: u32) -> bool {
    (y / BANDE).is_multiple_of(2)
}

/// Compte les pixels dont la couleur dominante n'est pas celle de leur bande.
///
/// Rouge contre bleu, et non une comparaison octet par octet : c'est un écart
/// que la compression ne peut pas fabriquer, alors qu'elle produit
/// inévitablement quelques niveaux d'écart sur une valeur exacte.
fn compter_bandes_mal_classees(rgb: &[u8]) -> u64 {
    let mut mal_classes = 0;
    for y in 0..HAUTEUR {
        let attendu_rouge = ligne_rouge(y);
        for x in 0..LARGEUR {
            let i = ((y * LARGEUR + x) * 3) as usize;
            let (r, b) = (rgb[i] as i32, rgb[i + 2] as i32);
            let obtenu_rouge = r > b;
            if obtenu_rouge != attendu_rouge {
                mal_classes += 1;
            }
        }
    }
    mal_classes
}

/// Un périphérique Direct3D 11 matériel, sans fenêtre ni capture d'écran.
fn peripherique_direct3d11() -> Option<ID3D11Device> {
    let mut peripherique: Option<ID3D11Device> = None;
    let resultat = unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut peripherique),
            None,
            None,
        )
    };
    resultat.ok().and(peripherique)
}

/// La texture source : bandes horizontales de [`BANDE`] lignes, rouge puis bleu.
fn texture_a_bandes(peripherique: &ID3D11Device) -> windows::core::Result<ID3D11Texture2D> {
    // BGRA, comme les textures du pool de capture : NVENC ne voit aucune
    // différence avec une vraie image d'écran.
    let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        let rouge = ligne_rouge(y);
        for x in 0..LARGEUR {
            let i = ((y * LARGEUR + x) * 4) as usize;
            pixels[i] = if rouge { 0 } else { 255 }; // B
            pixels[i + 1] = 0; // G
            pixels[i + 2] = if rouge { 255 } else { 0 }; // R
            pixels[i + 3] = 255; // A
        }
    }

    let description = D3D11_TEXTURE2D_DESC {
        Width: LARGEUR,
        Height: HAUTEUR,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let donnees = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: LARGEUR * 4,
        SysMemSlicePitch: 0,
    };
    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe { peripherique.CreateTexture2D(&description, Some(&donnees), Some(&mut texture))? };
    Ok(texture.expect("CreateTexture2D a réussi sans rendre de texture"))
}
