//! Le chemin NV12, éprouvé sur de vraies images Media Foundation.
//!
//! `tests/couleur.rs` mesure la matrice sur une texture NV12 fabriquée à la
//! main, d'une seule tranche et lisible. Il ne dit rien de ce que Media
//! Foundation rend réellement : une tranche d'un tableau de textures liées au
//! seul décodeur (8 tranches en H.264, 6 textures en HEVC, relevé RTX 4060),
//! d'une hauteur codée qui peut dépasser la hauteur d'affichage. Ce test est
//! l'appelant réel de `impl ImageAAfficher for ImageMf` et de `PontNv12`.
//!
//! Une image en quatre quadrants unis (rouge, vert, bleu, blanc), encodée par
//! NVENC en H.264 puis en HEVC 4:2:0, décodée par `DecodeurMf` sur le
//! périphérique de la fenêtre, affichée ; on relit le centre de chaque quadrant.
//! Les quadrants TOURNENT d'une image à l'autre : chaque unité a sa propre
//! disposition, si bien qu'une tranche lue à la place d'une autre (celle d'une
//! autre unité) se voit au moins sur une partie des unités.
//!
//! Exige une carte NVIDIA (pour encoder) et l'extension HEVC : panique sans
//! elles, comme `sky-decode/tests/media_foundation.rs`.

use sky_capture::CapturedFrame;
use sky_decode::{CodecMf, DecodeurMf};
use sky_encode::{Codec, NvencEncoder};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

const LARGEUR: u32 = 1280;
/// 720 est un multiple de 16 en H.264 comme en HEVC : pas de remplissage ici.
/// Le remplissage (1080 contre 1088) est couvert par `sky-decode` ; ce test-ci
/// porte sur l'affichage.
const HAUTEUR: u32 = 720;
/// Plus que les 8 tranches du MFT H.264 (relevé) : les tranches non nulles
/// sont toutes atteintes.
const IMAGES: usize = 12;
/// Débit délibérément large : on prouve une traversée, pas une compression.
const DEBIT_BPS: u32 = 80_000_000;
/// Écart maximal toléré par canal, en niveaux.
///
/// 8 et non les 2 du test 4:4:4 (`image_reelle.rs`) : celui-là compare à un
/// décodeur de référence qui lit la même surface, donc seul l'arrondi les
/// sépare ; celui-ci compare à la couleur SOURCE, avant encodage, et porte donc
/// la perte de compression (et l'arrondi de la conversion RVB → YUV de NVENC).
/// Au centre d'un quadrant uni, loin de toute frontière, cette perte est petite ;
/// une matrice fausse (BT.709 au lieu de BT.601) ou U et V échangés déplacent
/// un rouge ou un bleu saturé de plusieurs dizaines de niveaux.
const ECART_MAX: i32 = 8;

/// Les quatre couleurs, en RVB.
const COULEURS: [(&str, [u8; 3]); 4] = [
    ("rouge", [255, 0, 0]),
    ("vert", [0, 255, 0]),
    ("bleu", [0, 0, 255]),
    ("blanc", [255, 255, 255]),
];

/// L'indice dans `COULEURS` du quadrant `quadrant` (0 haut-gauche, 1
/// haut-droit, 2 bas-gauche, 3 bas-droit) à l'unité `rang`.
fn couleur_du_quadrant(rang: usize, quadrant: usize) -> usize {
    (quadrant + rang) % 4
}

fn une_image_reelle_nv12_s_affiche_juste(codec_nvenc: Codec, codec_mf: CodecMf) {
    // La fenêtre à la taille de l'image : rendu à l'échelle 1 pour 1.
    let mut fenetre = sky_rendu::Fenetre::ouvrir_masquee("image nv12", LARGEUR, HAUTEUR)
        .expect("fenêtre masquée");
    assert_eq!(
        fenetre.taille(),
        (LARGEUR, HAUTEUR),
        "Windows n'a pas accordé la zone cliente demandée : les centres de \
         quadrant relus ne seraient plus ceux de l'image"
    );

    // Encodeur et décodeur sur le périphérique de la fenêtre : celui sur
    // lequel l'application décode, et le seul d'où la copie peut lire.
    let appareil = fenetre.appareil().clone();
    let mut encodeur = NvencEncoder::new(&appareil, codec_nvenc, LARGEUR, HAUTEUR, 60, DEBIT_BPS)
        .unwrap_or_else(|e| panic!("ce test exige NVENC ({codec_nvenc:?}) : {e:#}"));
    let textures: Vec<ID3D11Texture2D> = (0..4)
        .map(|rotation| texture_des_quadrants(&appareil, rotation).expect("texture source"))
        .collect();
    let mut decodeur = DecodeurMf::nouveau(codec_mf, &appareil, LARGEUR, HAUTEUR)
        .unwrap_or_else(|e| panic!("ce test exige Media Foundation matériel ({codec_mf:?}) : {e}"));

    let mut tranches = Vec::new();
    for rang in 0..IMAGES {
        let image = CapturedFrame {
            texture: textures[rang % 4].clone(),
            width: LARGEUR,
            height: HAUTEUR,
            captured_at: std::time::Instant::now(),
        };
        let paquet = encodeur
            .encode(&image)
            .expect("encodage")
            .expect("NVENC rend un paquet par image");
        let decodee = decodeur
            .decoder(&paquet.data, rang as u64)
            .expect("décodage")
            .unwrap_or_else(|| panic!("l'unité {rang} n'a rendu aucune image au même appel"));
        assert_eq!((decodee.largeur, decodee.hauteur), (LARGEUR, HAUTEUR));
        tranches.push(decodee.tranche());

        fenetre.afficher(&decodee).expect("affichage");
        let pixels = fenetre.pixels_de_la_cible().expect("lecture du tampon");
        for quadrant in 0..4 {
            let x = (quadrant as u32 % 2) * LARGEUR / 2 + LARGEUR / 4;
            let y = (quadrant as u32 / 2) * HAUTEUR / 2 + HAUTEUR / 4;
            let i = ((y * LARGEUR + x) * 4) as usize;
            // La cible est en BGRA.
            let obtenu = [pixels[i + 2], pixels[i + 1], pixels[i]];
            let (nom, attendu) = COULEURS[couleur_du_quadrant(rang, quadrant)];
            println!(
                "{codec_mf:?} unité {rang} (tranche {}), quadrant {quadrant} {nom} : \
                 obtenu {obtenu:?}, attendu {attendu:?}",
                decodee.tranche()
            );
            for canal in 0..3 {
                let ecart = (obtenu[canal] as i32 - attendu[canal] as i32).abs();
                assert!(
                    ecart <= ECART_MAX,
                    "{codec_mf:?} unité {rang} (tranche {}), quadrant {quadrant} {nom}, \
                     canal {canal} : obtenu {obtenu:?}, attendu {attendu:?}",
                    decodee.tranche()
                );
            }
        }
    }
    println!("{codec_mf:?} : tranches rendues {tranches:?}");
}

#[test]
fn h264_une_image_reelle_nv12_s_affiche_juste() {
    une_image_reelle_nv12_s_affiche_juste(Codec::H264_420, CodecMf::H264);
}

#[test]
fn hevc_420_une_image_reelle_nv12_s_affiche_juste() {
    une_image_reelle_nv12_s_affiche_juste(Codec::Hevc420, CodecMf::Hevc);
}

/// Les quatre quadrants unis, tournés de `rotation`, en BGRA comme les textures
/// du pool de capture.
fn texture_des_quadrants(
    appareil: &ID3D11Device,
    rotation: usize,
) -> windows::core::Result<ID3D11Texture2D> {
    let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        for x in 0..LARGEUR {
            let quadrant = (x >= LARGEUR / 2) as usize + 2 * (y >= HAUTEUR / 2) as usize;
            let [r, v, b] = COULEURS[couleur_du_quadrant(rotation, quadrant)].1;
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
