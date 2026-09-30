//! Le chemin GPU, éprouvé sur une vraie surface NVDEC.
//!
//! `tests/couleur.rs` mesure la justesse de la matrice de conversion sur des
//! surfaces fabriquées à la main. Il ne dit rien de ce qui relie ce crate au
//! décodeur : l'implémentation de `ImageAAfficher` pour `ImageDecodee`, et la
//! géométrie réelle d'une surface mappée par `cuvidMapVideoFrame64` — son pas de
//! ligne, que le matériel choisit, et l'espacement de ses plans. Ce dépôt a déjà
//! produit trois fois le même défaut : une protection juste, testée, et appelée
//! par personne. Ce test est l'appelant qui manquait.
//!
//! Il compare le pixel présenté par le GPU au même pixel converti en `f64` sur le
//! processeur par `ImageDecodee::copier_vers_memoire_centrale`. Les deux chemins
//! partagent la matrice BT.601 — ce n'est donc pas elle qu'ils mesurent, c'est la
//! traversée : un plan lu au mauvais endroit, un pas de ligne confondu avec la
//! largeur, un U et un V échangés, une mise à l'échelle parasite.
//!
//! **Pourquoi fabriquer un flux plutôt que réutiliser celui du jalon 0** : ce
//! dernier est en 2560×1440, et Windows n'accorde pas une zone cliente de 1440
//! lignes sur un écran de 1440 lignes (mesuré : 1421). Sans échelle 1 pour 1, le
//! filtrage mêle des texels voisins et il n'y a plus rien d'exact à comparer.

use sky_capture::CapturedFrame;
use sky_decode::Decodeur;
use sky_encode::{Codec, NvencEncoder};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

/// Assez petit pour que Windows accorde la zone cliente demandée, assez grand
/// pour que le pas de ligne choisi par NVDEC dépasse la largeur.
const LARGEUR: u32 = 640;
const HAUTEUR: u32 = 360;
/// Nombre d'images encodées avant celle qu'on examine. La première image d'un
/// flux NVENC porte une rampe de qualité (piège du tampon VBV du jalon 0).
const IMAGES: usize = 8;
/// Débit délibérément large : on veut prouver une traversée mémoire, pas mesurer
/// une compression.
const DEBIT_BPS: u32 = 80_000_000;
/// Écart maximal toléré sur une composante, en niveaux.
///
/// À 2, seul l'arrondi sépare les deux chemins : le nuanceur calcule en `f32`
/// puis quantifie à l'écriture, `copier_vers_memoire_centrale` calcule en `f64` et
/// arrondit au plus proche. Toute erreur de géométrie déplace des composantes de
/// plusieurs dizaines de niveaux.
const ECART_MAX: i32 = 2;

#[test]
fn une_image_vraiment_decodee_traverse_le_chemin_gpu_sans_deriver() {
    if sky_rendu::cartes_cuda_disponibles() == 0 {
        println!("aucune carte CUDA : ni décodeur ni interopérabilité à éprouver ici");
        return;
    }
    // La fenêtre est ouverte à la taille de l'image pour que le rendu soit à
    // l'échelle 1 pour 1 : chaque pixel présenté vient alors d'un texel unique.
    let mut fenetre = sky_rendu::Fenetre::ouvrir_masquee("image réelle", LARGEUR, HAUTEUR)
        .expect("fenêtre masquée");
    assert_eq!(
        fenetre.taille(),
        (LARGEUR, HAUTEUR),
        "Windows n'a pas accordé la zone cliente demandée : la comparaison exacte \
         que fait ce test n'aurait plus de sens à une autre échelle"
    );

    // Le même appareil que la fenêtre, donc la même carte : c'est la condition de
    // l'interopérabilité, et l'encodeur n'a aucune raison d'en prendre un autre.
    let paquets = encoder_un_motif(fenetre.appareil());
    let mut decodeur =
        Decodeur::nouveau(LARGEUR, HAUTEUR).expect("décodeur NVIDIA HEVC 4:4:4 requis");

    let mut derniere = None;
    for (rang, paquet) in paquets.iter().enumerate() {
        if let Some(image) = decodeur.decoder(paquet, rang as u64).expect("décodage") {
            derniere = Some(image);
        }
    }
    let image = derniere.expect("au moins une image décodée");
    assert_eq!((image.largeur, image.hauteur), (LARGEUR, HAUTEUR));

    // GARDE DE PERTINENCE : si le pas de ligne était égal à la largeur, ce test
    // ne distinguerait plus un `cuMemcpy2D` qui respecte le pas d'un qui le
    // confond avec la largeur — et c'est l'un des deux défauts qu'il traque. La
    // valeur vient du matériel, pas de nous.
    let pas = image.surface().pas;
    assert!(
        pas > LARGEUR,
        "ce test ne prouve plus rien : NVDEC a choisi un pas de ligne égal à la \
         largeur ({pas}), donc pas et largeur ne se distinguent plus"
    );

    // Le chemin processeur d'abord : il lit la même surface, et l'affichage ne la
    // modifie pas.
    let attendu = image
        .copier_vers_memoire_centrale()
        .expect("conversion de référence sur le processeur");
    fenetre.afficher(&image).expect("affichage");
    let obtenu = fenetre.pixels_de_la_cible().expect("lecture du tampon");

    let (largeur, hauteur) = (LARGEUR as usize, HAUTEUR as usize);
    assert_eq!(obtenu.len(), largeur * hauteur * 4);
    assert_eq!(attendu.len(), largeur * hauteur * 3);

    let mut ecart_max = 0;
    let mut pire = (0usize, 0usize, 0usize);
    for y in 0..hauteur {
        for x in 0..largeur {
            let gpu = (y * largeur + x) * 4;
            let processeur = (y * largeur + x) * 3;
            // La cible est en BGRA, la référence en RVB.
            for (canal, decalage) in [2usize, 1, 0].into_iter().enumerate() {
                let ecart =
                    (obtenu[gpu + decalage] as i32 - attendu[processeur + canal] as i32).abs();
                if ecart > ecart_max {
                    ecart_max = ecart;
                    pire = (x, y, canal);
                }
            }
        }
    }
    println!(
        "pas de ligne NVDEC : {pas} octets pour {LARGEUR} pixels ; \
         écart maximal GPU/processeur : {ecart_max} niveau(x)"
    );
    assert!(
        ecart_max <= ECART_MAX,
        "écart de {ecart_max} niveaux en (x, y, canal) = {pire:?}, au-delà de {ECART_MAX}"
    );
}

/// Encode quelques images d'un motif coloré et rend les paquets produits.
fn encoder_un_motif(appareil: &ID3D11Device) -> Vec<Vec<u8>> {
    let mut encodeur = NvencEncoder::new(appareil, Codec::Hevc444, LARGEUR, HAUTEUR, 60, DEBIT_BPS)
        .expect("encodeur NVENC HEVC 4:4:4 requis");
    let texture = texture_du_motif(appareil).expect("texture source");
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
    paquets
}

/// La couleur du motif en un point, en RVB.
///
/// Elle varie selon `x` **et** selon `y`, et les trois canaux varient
/// différemment : un décalage horizontal, un décalage vertical ou deux canaux
/// échangés déplacent chacun la valeur attendue. Un damier uni ne dirait rien du
/// premier, un dégradé vertical rien du second.
fn couleur(x: u32, y: u32) -> [u8; 3] {
    let bande = ((x / 37) + (y / 23)) % 3;
    let rampe = (x * 255 / LARGEUR) as u8;
    let contre_rampe = 255 - (y * 255 / HAUTEUR) as u8;
    match bande {
        0 => [255 - rampe, contre_rampe, rampe],
        1 => [rampe, 255 - contre_rampe, contre_rampe],
        _ => [contre_rampe, rampe, 255 - rampe],
    }
}

/// La texture source, en BGRA comme celles du pool de capture : NVENC ne voit
/// aucune différence avec une vraie image d'écran.
fn texture_du_motif(appareil: &ID3D11Device) -> windows::core::Result<ID3D11Texture2D> {
    let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        for x in 0..LARGEUR {
            let [r, v, b] = couleur(x, y);
            let i = ((y * LARGEUR + x) * 4) as usize;
            pixels[i] = b;
            pixels[i + 1] = v;
            pixels[i + 2] = r;
            pixels[i + 3] = 255;
        }
    }
    let desc = D3D11_TEXTURE2D_DESC {
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
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let donnees = D3D11_SUBRESOURCE_DATA {
        pSysMem: pixels.as_ptr().cast(),
        SysMemPitch: LARGEUR * 4,
        SysMemSlicePitch: 0,
    };
    let mut texture = None;
    unsafe { appareil.CreateTexture2D(&desc, Some(&donnees), Some(&mut texture)) }?;
    texture.ok_or_else(windows::core::Error::from_thread)
}
