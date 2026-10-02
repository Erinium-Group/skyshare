//! Décodage Media Foundation de flux NVENC réels, HEVC 4:2:0 et H.264.
//! Exige une carte NVIDIA (pour encoder) : panique sans elle, comme
//! `aller_retour.rs`.
//!
//! Le modèle est `aller_retour.rs::chaque_unite_poussee_rend_sa_propre_image_sans_retard` :
//! deux témoins indépendants de « l'image rendue est celle de l'unité poussée,
//! au même appel » — l'horodatage recopié de l'échantillon d'entrée vers
//! l'échantillon de sortie, et le CONTENU (deux motifs alternés de bandes, donc
//! l'image k−1 a la luminance inverse de l'image k sur toute sa surface).
//!
//! Neutralisations mesurées (RTX 4060, 02/10/2026, tâche 5) — chacune fait
//! rougir les deux tests :
//! - `MF_LOW_LATENCY = 0` : l'unité 0 ne rend aucune image au même appel ;
//! - `taille_d_affichage` sans l'ouverture d'affichage : 1920×1088 — le type
//!   porte bien `MF_MT_MINIMUM_DISPLAY_APERTURE`, et c'est elle qui donne 1080 ;
//! - `MF_E_TRANSFORM_STREAM_CHANGE` rendu en erreur : le décodeur l'émet (une
//!   fois, à l'unité 0) puisque la taille annoncée (2560×1440) n'est pas celle
//!   du flux ;
//! - `decoder` rendant `Ok(None)` à l'unité 0 ;
//! - le motif attendu inversé : 100 % de pixels mal classés (le témoin de
//!   contenu discrimine).
//!
//! Ronde de correction 1 :
//! - le filtre d'horodatage de `decoder` retiré (dernière image tirée rendue
//!   telle quelle) : ces tests-ci RESTENT VERTS, la RTX 4060 ne retenant aucune
//!   image en faible latence ; ce sont les tests purs de
//!   `media_foundation::tests` (`image_de_l_unite`) qui rougissent ;
//! - `ImageMf` sans son échantillon (relâché aussitôt) : les deux
//!   `*_une_image_gardee_ne_bouge_pas` rougissent, 2 073 600 octets changés.

use sky_capture::CapturedFrame;
use sky_decode::{creer_appareil_video, CodecMf, DecodeurMf};
use sky_encode::{Codec, NvencEncoder};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

const LARGEUR: u32 = 1920;
/// 1080 et non 1088 : la hauteur codée est arrondie au bloc, et le jalon 2 a
/// payé cette différence (`SurfaceCuda`). L'image doit sortir à sa hauteur
/// d'AFFICHAGE.
const HAUTEUR: u32 = 1080;
/// Taille annoncée au décodeur, plus grande que le flux : le décodeur doit
/// suivre la vraie taille (`MF_E_TRANSFORM_STREAM_CHANGE`), spec §2 inconnue 3.
const LARGEUR_ANNONCEE: u32 = 2560;
const HAUTEUR_ANNONCEE: u32 = 1440;
/// Pair, pour que les deux motifs alternés y figurent autant de fois.
const IMAGES: usize = 8;
/// Hauteur d'une bande de couleur, en lignes (comme `aller_retour.rs`).
const BANDE: u32 = 16;
/// Débit délibérément large : on prouve une correspondance d'images, pas une
/// compression.
const DEBIT_BPS: u32 = 80_000_000;
/// Luminance BT.601 pleine plage du rouge saturé (76) et du bleu saturé (29) :
/// le seuil est à mi-chemin. Quelle que soit la matrice qu'emploie NVENC
/// (BT.601 ou BT.709, pleine ou réduite), le rouge reste au-dessus et le bleu
/// au-dessous (BT.709 réduite : 63 et 32 — déduit des coefficients, non mesuré).
const SEUIL_LUMINANCE: u8 = 52;
/// Proportion de pixels mal classés tolérée : la compression ne se trompe que
/// sur quelques pixels de part et d'autre d'une frontière de bande ; l'image
/// d'une autre unité les inverse presque tous.
const PART_MAL_CLASSEE_TOLEREE: f64 = 0.02;

/// `nombre` unités NVENC encodées sur `appareil`. Les `alternees` premières
/// alternent les deux motifs (rouge en tête aux rangs pairs) ; les suivantes
/// sont NOIRES, d'un contenu qu'aucune des premières n'a (le booléen rendu ne
/// les concerne pas). L'unité 0 est celle que NVENC produit en premier :
/// en-têtes de séquence et IDR dans le même paquet, exactement ce que l'hôte
/// écrit sur la piste.
fn paquets_alternes(
    appareil: &ID3D11Device,
    codec_nvenc: Codec,
    nombre: usize,
    alternees: usize,
) -> Vec<(bool, Vec<u8>)> {
    let mut encodeur = NvencEncoder::new(appareil, codec_nvenc, LARGEUR, HAUTEUR, 60, DEBIT_BPS)
        .unwrap_or_else(|e| panic!("ce test exige NVENC ({codec_nvenc:?}) : {e:#}"));
    let rouge_en_tete = texture_de_bandes(appareil, Some(true)).expect("texture rouge en tête");
    let bleu_en_tete = texture_de_bandes(appareil, Some(false)).expect("texture bleu en tête");
    let noire = texture_de_bandes(appareil, None).expect("texture noire");
    (0..nombre)
        .map(|rang| {
            let rouge = rang % 2 == 0;
            let texture = if rang >= alternees {
                &noire
            } else if rouge {
                &rouge_en_tete
            } else {
                &bleu_en_tete
            };
            let image = CapturedFrame {
                texture: texture.clone(),
                width: LARGEUR,
                height: HAUTEUR,
                captured_at: std::time::Instant::now(),
            };
            let paquet = encodeur
                .encode(&image)
                .expect("encodage")
                .expect("NVENC rend un paquet par image");
            (rouge, paquet.data)
        })
        .collect()
}

fn chaque_unite_rend_sa_propre_image(codec_nvenc: Codec, codec_mf: CodecMf) {
    let (appareil, _) = creer_appareil_video().expect("périphérique Direct3D 11 matériel");
    let paquets = paquets_alternes(&appareil, codec_nvenc, IMAGES, IMAGES);
    let mut decodeur = DecodeurMf::nouveau(codec_mf, &appareil, LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)
        .unwrap_or_else(|e| panic!("ce test exige Media Foundation matériel ({codec_mf:?}) : {e}"));
    for (rang, (rouge, paquet)) in paquets.iter().enumerate() {
        let image = decodeur
            .decoder(paquet, rang as u64)
            .expect("décodage")
            .unwrap_or_else(|| panic!("l'unité {rang} n'a rendu aucune image au même appel"));
        assert_eq!(
            image.horodatage_ms, rang as u64,
            "l'unité {rang} a rendu l'image d'une autre"
        );
        assert_eq!(
            (image.largeur, image.hauteur),
            (LARGEUR, HAUTEUR),
            "taille d'affichage"
        );
        let luminance = image.copier_luminance().expect("copie de test");
        assert_eq!(luminance.len(), (LARGEUR * HAUTEUR) as usize);
        let part = part_mal_classee_luminance(&luminance, *rouge);
        println!(
            "{codec_mf:?} unité {rang} : {}×{}, tranche {}, mal classés {:.3} %",
            image.largeur,
            image.hauteur,
            image.tranche(),
            part * 100.0
        );
        assert!(
            part <= PART_MAL_CLASSEE_TOLEREE,
            "l'unité {rang} a le contenu de l'autre motif ({:.1} %)",
            part * 100.0
        );
    }
}

#[test]
fn hevc_420_chaque_unite_rend_sa_propre_image_a_sa_taille() {
    chaque_unite_rend_sa_propre_image(Codec::Hevc420, CodecMf::Hevc);
}

#[test]
fn h264_chaque_unite_rend_sa_propre_image_a_sa_taille() {
    chaque_unite_rend_sa_propre_image(Codec::H264_420, CodecMf::H264);
}

/// Nombre d'images gardées vivantes ensemble : k, k+1, k+2.
const GARDEES: usize = 3;
/// Nombre total d'unités décodées pendant que les `GARDEES` premières restent
/// vivantes : bien plus que le tableau de textures du MFT H.264 (8 tranches au
/// moins, relevé), pour qu'une tranche relâchée trop tôt soit réécrite.
const UNITES_DECODEES: usize = 40;

/// Une `ImageMf` gardée pendant qu'on décode les unités suivantes garde-t-elle
/// son contenu ? (ronde de correction 1, IMPORTANT 3 : la doc d'`ImageMf`
/// l'affirmait sans mesure, alors que le HEVC rend toujours la tranche 0.)
///
/// Les `GARDEES` premières images restent vivantes pendant le décodage de
/// `UNITES_DECODEES` unités ; les autres sont relâchées aussitôt rendues. La
/// luminance de chaque image gardée est copiée aussitôt rendue, puis relue à la
/// fin : les deux copies doivent être identiques à l'octet, et la seconde doit
/// encore porter le motif de SON unité (les motifs alternent : une tranche
/// réécrite par une autre unité aurait souvent le motif inverse). Relève aussi
/// la texture et la tranche de chaque image (`--nocapture`).
fn une_image_gardee_ne_bouge_pas(codec_nvenc: Codec, codec_mf: CodecMf) {
    let (appareil, _) = creer_appareil_video().expect("périphérique Direct3D 11 matériel");
    // Les unités qui suivent les gardées sont noires : une tranche réutilisée
    // sous une image gardée change forcément d'octets. (Avec des motifs
    // alternés partout, le cycle du MFT — 8 tranches en H.264, 6 textures en
    // HEVC, relevé — étant pair, une réécriture reproduisait le même motif et
    // passait inaperçue : mesuré, neutralisation C du rapport.)
    let paquets = paquets_alternes(&appareil, codec_nvenc, UNITES_DECODEES, GARDEES);
    let mut decodeur = DecodeurMf::nouveau(codec_mf, &appareil, LARGEUR_ANNONCEE, HAUTEUR_ANNONCEE)
        .unwrap_or_else(|e| panic!("ce test exige Media Foundation matériel ({codec_mf:?}) : {e}"));
    let mut gardees = Vec::new();
    for (rang, (rouge, paquet)) in paquets.iter().enumerate() {
        let image = decodeur
            .decoder(paquet, rang as u64)
            .expect("décodage")
            .unwrap_or_else(|| panic!("l'unité {rang} n'a rendu aucune image au même appel"));
        println!(
            "{codec_mf:?} unité {rang} : texture {:?}, tranche {}",
            image.texture().as_raw(),
            image.tranche()
        );
        if rang < GARDEES {
            let avant = image.copier_luminance().expect("copie de test");
            gardees.push((*rouge, image, avant));
        }
    }
    for (rang, (rouge, image, avant)) in gardees.iter().enumerate() {
        let apres = image.copier_luminance().expect("copie de test");
        let differents = avant.iter().zip(&apres).filter(|(a, b)| a != b).count();
        let part = part_mal_classee_luminance(&apres, *rouge);
        println!(
            "{codec_mf:?} unité {rang} relue après {} décodages : {differents} octets changés, \
             mal classés {:.3} %",
            UNITES_DECODEES - 1 - rang,
            part * 100.0
        );
        assert_eq!(
            differents, 0,
            "l'image de l'unité {rang}, gardée vivante, a changé pendant le décodage des suivantes"
        );
        assert!(
            part <= PART_MAL_CLASSEE_TOLEREE,
            "l'image gardée de l'unité {rang} porte le contenu de l'autre motif ({:.1} %)",
            part * 100.0
        );
    }
}

#[test]
fn hevc_420_une_image_gardee_ne_bouge_pas() {
    une_image_gardee_ne_bouge_pas(Codec::Hevc420, CodecMf::Hevc);
}

#[test]
fn h264_une_image_gardee_ne_bouge_pas() {
    une_image_gardee_ne_bouge_pas(Codec::H264_420, CodecMf::H264);
}

/// Couleur attendue d'une ligne du motif « rouge en tête » : `true` pour rouge.
fn ligne_rouge(y: u32) -> bool {
    (y / BANDE).is_multiple_of(2)
}

/// Part des pixels dont la classe de luminance (clair = rouge, sombre = bleu)
/// contredit le motif attendu.
fn part_mal_classee_luminance(luminance: &[u8], rouge_en_tete: bool) -> f64 {
    let mut mal_classes = 0u64;
    for y in 0..HAUTEUR {
        let attendu_rouge = ligne_rouge(y) == rouge_en_tete;
        let ligne = &luminance[(y * LARGEUR) as usize..((y + 1) * LARGEUR) as usize];
        mal_classes += ligne
            .iter()
            .filter(|&&v| (v > SEUIL_LUMINANCE) != attendu_rouge)
            .count() as u64;
    }
    mal_classes as f64 / (LARGEUR * HAUTEUR) as f64
}

/// Bandes de [`BANDE`] lignes, rouge en tête si `Some(true)`, bleu en tête si
/// `Some(false)` (recopie de `aller_retour.rs`) ; toute noire si `None`.
fn texture_de_bandes(
    peripherique: &ID3D11Device,
    rouge_en_tete: Option<bool>,
) -> windows::core::Result<ID3D11Texture2D> {
    // BGRA, comme les textures du pool de capture.
    let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        // (bleu, rouge) de la ligne.
        let (b, r) = match rouge_en_tete {
            Some(tete) if ligne_rouge(y) == tete => (0, 255),
            Some(_) => (255, 0),
            None => (0, 0),
        };
        for x in 0..LARGEUR {
            let i = ((y * LARGEUR + x) * 4) as usize;
            pixels[i] = b; // B
            pixels[i + 1] = 0; // G
            pixels[i + 2] = r; // R
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
