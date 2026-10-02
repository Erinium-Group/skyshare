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

/// Nombre d'images du test « sans retard ». Pair, pour que les deux motifs
/// alternés y figurent autant de fois.
const IMAGES_ALTERNEES: usize = 6;

/// Chaque unité d'accès poussée rend SA propre image, au même appel — et non
/// celle de l'unité précédente.
///
/// C'est la propriété dont dépend la garde d'affichage du spectateur : il
/// décide d'afficher ou non selon l'unité qu'il vient de pousser (image clé,
/// trou). Un décodeur qui rendrait l'image d'avant ferait afficher, à la reprise
/// après une perte, la dernière image décodée sur des références perdues — et
/// sur un écran qui cesse de bouger, la dernière image n'apparaîtrait jamais.
///
/// Deux témoins indépendants, parce qu'un seul pourrait mentir : l'horodatage
/// que NVDEC recopie du paquet vers l'image, et le **contenu** — les images
/// alternent deux motifs (bandes rouges en tête, puis bleues en tête), donc
/// l'image k−1 a la couleur inverse de l'image k sur toute sa surface.
///
/// Neutralisation : retirer `CUVID_PKT_ENDOFPICTURE` dans `Decodeur::decoder`
/// — le premier appel (en-têtes et IDR) ne rend plus rien, et ce test rougit.
#[test]
fn chaque_unite_poussee_rend_sa_propre_image_sans_retard() {
    let peripherique = peripherique_direct3d11()
        .unwrap_or_else(|| panic!("ce test exige un périphérique Direct3D 11 matériel"));
    let mut encodeur = NvencEncoder::new(
        &peripherique,
        Codec::Hevc444,
        LARGEUR,
        HAUTEUR,
        60,
        DEBIT_BPS,
    )
    .unwrap_or_else(|e| panic!("ce test exige un encodeur NVENC HEVC 4:4:4 : {e:#}"));
    let rouge_en_tete = texture_de_bandes(&peripherique, true).expect("texture rouge en tête");
    let bleu_en_tete = texture_de_bandes(&peripherique, false).expect("texture bleu en tête");

    // L'unité 0 est celle que NVENC produit en premier : en-têtes de séquence
    // et IDR dans le même paquet, exactement ce que l'hôte écrit sur la piste.
    let paquets: Vec<(bool, Vec<u8>)> = (0..IMAGES_ALTERNEES)
        .map(|rang| {
            let rouge = rang % 2 == 0;
            let texture = if rouge { &rouge_en_tete } else { &bleu_en_tete };
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
        .collect();

    let mut decodeur = Decodeur::nouveau(LARGEUR, HAUTEUR)
        .unwrap_or_else(|e| panic!("ce test exige un décodeur NVIDIA HEVC 4:4:4 : {e}"));
    for (rang, (rouge, paquet)) in paquets.iter().enumerate() {
        let image = decodeur
            .decoder(paquet, rang as u64)
            .expect("décodage")
            .unwrap_or_else(|| {
                panic!(
                    "l'unité {rang} n'a rendu aucune image au moment où on l'a poussée : \
                     le décodeur garde une image d'avance"
                )
            });
        assert_eq!(
            image.horodatage_ms, rang as u64,
            "l'unité {rang} a rendu l'image horodatée {} : ce n'est pas la sienne",
            image.horodatage_ms
        );
        let rgb = image.copier_vers_memoire_centrale().expect("copie de test");
        let mal_classes = compter_mal_classes(&rgb, *rouge);
        let part = mal_classes as f64 / (LARGEUR * HAUTEUR) as f64;
        assert!(
            part <= PART_MAL_CLASSEE_TOLEREE,
            "l'unité {rang} a rendu une image dont {:.1} % des pixels ont la couleur de \
             l'AUTRE motif : c'est le contenu d'une autre unité",
            part * 100.0
        );
    }
}

/// Les trois morceaux d'un flux qu'un spectateur arrivé en retard recevrait.
struct FluxDeReprise {
    /// Ce que rend `entetes_de_sequence` : VPS, SPS, PPS.
    entetes: Vec<u8>,
    /// Une image ordinaire, prédite de celles qu'un spectateur en retard n'a pas.
    image_p: Vec<u8>,
    /// L'image qui suit un `forcer_image_cle`.
    image_cle_forcee: Vec<u8>,
    /// Les images ordinaires qui suivent l'image clé forcée.
    ///
    /// Elles étaient indispensables avant `CUVID_PKT_ENDOFPICTURE` : le décodeur
    /// gardait alors une image d'avance, et « en-têtes, IDR » seuls ne rendaient
    /// rien (mesuré). Ce n'est plus le cas — voir
    /// `des_entetes_puis_une_image_cle_forcee_s_affichent`. Elles restent pour les
    /// deux tests qui opposent un flux avec et sans en-têtes sur plusieurs images.
    suite: Vec<Vec<u8>>,
}

fn flux_de_reprise() -> FluxDeReprise {
    let peripherique = peripherique_direct3d11()
        .unwrap_or_else(|| panic!("ce test exige un périphérique Direct3D 11 matériel"));
    let mut encodeur = NvencEncoder::new(
        &peripherique,
        Codec::Hevc444,
        LARGEUR,
        HAUTEUR,
        60,
        DEBIT_BPS,
    )
    .unwrap_or_else(|e| panic!("ce test exige un encodeur NVENC HEVC 4:4:4 : {e:#}"));
    let texture = texture_a_bandes(&peripherique).expect("texture source");
    let image = || CapturedFrame {
        texture: texture.clone(),
        width: LARGEUR,
        height: HAUTEUR,
        captured_at: std::time::Instant::now(),
    };

    // L'IDR initial, que le spectateur en retard n'a jamais reçu : écarté.
    encodeur.encode(&image()).expect("première image");
    let image_p = encodeur
        .encode(&image())
        .expect("image P")
        .expect("un paquet")
        .data;
    encodeur.forcer_image_cle();
    let image_cle_forcee = encodeur
        .encode(&image())
        .expect("image clé forcée")
        .expect("un paquet")
        .data;
    let suite = (0..IMAGES)
        .map(|_| {
            encodeur
                .encode(&image())
                .expect("image de la suite")
                .expect("un paquet")
                .data
        })
        .collect();
    FluxDeReprise {
        entetes: encodeur.entetes_de_sequence().expect("en-têtes"),
        image_p,
        image_cle_forcee,
        suite,
    }
}

/// Pousse les paquets à un décodeur neuf et compte les images rendues. Une
/// erreur du décodeur compte pour zéro image : ce qu'on demande ici est « le
/// spectateur voit-il quelque chose ? », pas la forme de son refus.
fn images_rendues(paquets: &[&[u8]]) -> usize {
    let mut decodeur = Decodeur::nouveau(LARGEUR, HAUTEUR)
        .unwrap_or_else(|e| panic!("ce test exige un décodeur NVIDIA HEVC 4:4:4 : {e}"));
    let mut rendues = 0;
    for (rang, paquet) in paquets.iter().enumerate() {
        if let Ok(Some(_)) = decodeur.decoder(paquet, rang as u64) {
            rendues += 1;
        }
    }
    rendues
}

/// Les en-têtes puis l'image clé forcée, ET RIEN D'AUTRE, rendent une image.
///
/// Cette assertion disait l'inverse avant `CUVID_PKT_ENDOFPICTURE` : il fallait
/// une image ordinaire derrière l'IDR pour que celui-ci sorte. C'est le cas d'un
/// hôte dont l'écran est figé après la demande : sans le drapeau, l'image clé
/// n'aurait jamais été montrée. Neutralisation : retirer le drapeau dans
/// `Decodeur::decoder` — 0 image, ce test rougit.
#[test]
fn des_entetes_puis_une_image_cle_forcee_s_affichent() {
    let flux = flux_de_reprise();
    let paquets: Vec<&[u8]> = vec![&flux.entetes, &flux.image_cle_forcee];
    let rendues = images_rendues(&paquets);
    assert_eq!(
        rendues, 1,
        "un spectateur qui reçoit les en-têtes puis l'image clé forcée, sans rien après, \
         doit voir cette image"
    );
}

/// Ce que rend `entetes_de_sequence` suffit, à lui seul, à ouvrir le décodeur :
/// l'image clé forcée portant ses propres en-têtes, le test précédent ne le
/// prouve pas. Ici les en-têtes sont les seuls de tout le flux. Il s'oppose au
/// test suivant, dont les images sont les mêmes.
#[test]
fn les_entetes_de_sequence_seuls_suffisent_a_ouvrir_le_decodeur() {
    let flux = flux_de_reprise();
    let mut paquets: Vec<&[u8]> = vec![&flux.entetes, &flux.image_p];
    paquets.extend(flux.suite.iter().map(Vec::as_slice));
    let rendues = images_rendues(&paquets);
    assert!(
        rendues >= 1,
        "les en-têtes de `entetes_de_sequence` doivent suffire à ouvrir le décodeur"
    );
}

/// Le contraste qui donne son sens au test précédent, et il porte sur les
/// en-têtes : les mêmes images ordinaires, SANS eux, ne donnent rien. Côté
/// spectateur, c'est « En attente de l'image… », une demande d'image clé par
/// seconde, puis l'abandon au bout de dix secondes si rien n'arrive — et non plus
/// la fenêtre noire définitive de l'écart 3. Sans ce test, l'image affichée plus
/// haut pourrait venir d'un flux que le décodeur aurait accepté de toute façon.
///
/// Le contraste « en-têtes puis image P au lieu d'un IDR » a été tenté et ne
/// tient pas : mesuré, NVDEC rend alors des images quand même (le flux est en
/// rafraîchissement intra, il n'attend pas de point d'accès). Ce n'est donc pas
/// l'IDR qui rend le flux lisible, ce sont les en-têtes ; l'IDR sert à repartir
/// d'une image exacte, ce que ce test ne mesure pas.
#[test]
fn des_images_sans_entetes_ne_donnent_rien() {
    let flux = flux_de_reprise();
    let mut paquets: Vec<&[u8]> = vec![&flux.image_p];
    paquets.extend(flux.suite.iter().map(Vec::as_slice));
    let rendues = images_rendues(&paquets);
    assert_eq!(
        rendues, 0,
        "sans en-têtes de séquence, le décodeur ne peut rien ouvrir"
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
    compter_mal_classes(rgb, true)
}

/// Comme [`compter_bandes_mal_classees`], pour le motif choisi : bandes rouges
/// en tête (`rouge_en_tete`) ou son inverse.
fn compter_mal_classes(rgb: &[u8], rouge_en_tete: bool) -> u64 {
    let mut mal_classes = 0;
    for y in 0..HAUTEUR {
        let attendu_rouge = ligne_rouge(y) == rouge_en_tete;
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
    texture_de_bandes(peripherique, true)
}

/// Bandes de [`BANDE`] lignes, rouge en tête si `rouge_en_tete`, bleu sinon.
fn texture_de_bandes(
    peripherique: &ID3D11Device,
    rouge_en_tete: bool,
) -> windows::core::Result<ID3D11Texture2D> {
    // BGRA, comme les textures du pool de capture : NVENC ne voit aucune
    // différence avec une vraie image d'écran.
    let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
    for y in 0..HAUTEUR {
        let rouge = ligne_rouge(y) == rouge_en_tete;
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
