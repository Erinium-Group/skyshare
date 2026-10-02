//! Le chemin GPU ne doit pas altérer la couleur. Ces tests ne mesurent pas le
//! décodage mais la traversée jusqu'au nuanceur : on pousse une image dont on
//! connaît la couleur exacte — une surface YUV 4:4:4 en mémoire CUDA, ou une
//! texture NV12 Direct3D 11 — et on relit le pixel présenté.

// `nvidia-video-codec-sdk`, tiré par `sky-decode`, référence
// `NvEncodeAPICreateInstance` et `NvEncodeAPIGetMaxSupportedVersion` dans un
// bloc `extern "C"` lié statiquement. `sky-decode` ne fournit ses souches que
// sous `#[cfg(test)]` ; un test d'intégration la compile SANS `cfg(test)` et
// échouerait donc au lien (LNK2019). `sky-encode/src/nvenc_sys.rs` les définit
// sans condition, et c'est la définition unique du dépôt — la redéfinir ici en
// produirait un doublon (LNK2005). On se contente donc de lier `sky-encode`.
use sky_encode as _;

/// Trois couleurs choisies parce qu'elles séparent BT.601 de BT.709 : l'écart
/// entre les deux matrices porte sur les coefficients de U et V, donc un rouge
/// et un bleu saturés discriminent, là où un gris ne dirait rien.
/// Valeurs YUV en BT.601 pleine échelle.
const CAS: [(&str, [u8; 3], [u8; 3]); 3] = [
    ("rouge saturé", [76, 85, 255], [255, 0, 0]),
    ("vert saturé", [150, 44, 21], [0, 255, 0]),
    ("bleu saturé", [29, 255, 107], [0, 0, 255]),
];

#[test]
fn la_conversion_bt601_rend_les_couleurs_attendues() {
    // Sortie franche et non silencieuse : sans carte NVIDIA, il n'y a ni CUDA ni
    // interopérabilité, donc pas de chemin 4:4:4 à éprouver (une telle machine
    // reçoit en NV12, que le test suivant éprouve sans cette sortie). Partout
    // ailleurs, tout ce qui suit doit réussir — un `else` qui avalerait l'échec
    // laisserait passer la régression que ce test traque.
    if sky_rendu::cartes_cuda_disponibles() == 0 {
        println!("aucune carte CUDA : pas d'interopérabilité à éprouver ici");
        return;
    }
    let mut fenetre =
        sky_rendu::Fenetre::ouvrir_masquee("couleur", 64, 64).expect("fenêtre masquée");
    for (nom, yuv, rgb_attendu) in CAS {
        let image = sky_rendu::image_de_test_unie(64, 64, yuv).expect("surface de test");
        fenetre.afficher(&image).expect("affichage");
        let obtenu = fenetre.pixel_central().expect("lecture du tampon");
        println!("{nom} : YUV {yuv:?} -> RVB {obtenu:?} (attendu {rgb_attendu:?})");
        for (canal, (o, a)) in obtenu.iter().zip(rgb_attendu).enumerate() {
            let ecart = (*o as i32 - a as i32).abs();
            assert!(
                ecart <= 4,
                "{nom}, canal {canal} : {o} au lieu de {a} (écart {ecart})"
            );
        }
    }
}

/// Le chemin NV12 (Media Foundation) applique la même matrice BT.601 pleine
/// plage que le 4:4:4 (spec D6) : mêmes cas, mêmes couleurs attendues, même
/// tolérance. Une image NV12 unie a les mêmes U et V sur tout son plan de
/// chrominance, donc le sous-échantillonnage 4:2:0 n'y perd rien.
///
/// Pas de sortie « aucune carte CUDA » ici : le chemin NV12 ne dépend pas de
/// CUDA, il doit fonctionner sur toute machine qui ouvre la fenêtre.
#[test]
fn la_conversion_bt601_nv12_rend_les_couleurs_attendues() {
    let mut fenetre =
        sky_rendu::Fenetre::ouvrir_masquee("couleur nv12", 64, 64).expect("fenêtre masquée");
    for (nom, yuv, rgb_attendu) in CAS {
        let image = sky_rendu::image_de_test_nv12(fenetre.appareil(), 64, 64, yuv)
            .expect("image NV12 de test");
        fenetre.afficher(&image).expect("affichage");
        let obtenu = fenetre.pixel_central().expect("lecture du tampon");
        println!("NV12 {nom} : YUV {yuv:?} -> RVB {obtenu:?} (attendu {rgb_attendu:?})");
        for (canal, (o, a)) in obtenu.iter().zip(rgb_attendu).enumerate() {
            let ecart = (*o as i32 - a as i32).abs();
            assert!(
                ecart <= 4,
                "NV12 {nom}, canal {canal} : {o} au lieu de {a} (écart {ecart})"
            );
        }
    }
}

/// La texture NV12 lisible par le nuanceur est refaite quand la taille de
/// l'image change, comme le pont 4:4:4.
///
/// L'ordre des tailles est ce qui rend ce test discriminant. Une texture
/// gardée à 64×64 sous une image 128×64 unie ne se verrait pas : la copie d'un
/// coin 64×64 d'une image unie a la même couleur que l'image entière. C'est
/// donc la plus GRANDE qui vient d'abord : une texture gardée à 128×64 sous une
/// image 64×64 exige une copie qui déborde de la source — refusée par
/// `PontNv12::televerser`, et que Direct3D 11 ignore sans erreur sinon, ce qui
/// laisse affichée la couleur précédente (mesuré, tâche 6 : refus et
/// recréation retirés ensemble, ce test rougit sur « 254 au lieu de 0 »).
/// Chaque image a sa couleur.
///
/// L'ordre inverse (64×64 puis 128×64) a été mesuré : il reste VERT sous
/// « texture jamais recréée ».
#[test]
fn le_pont_nv12_suit_la_taille_de_l_image() {
    let mut fenetre =
        sky_rendu::Fenetre::ouvrir_masquee("taille nv12", 64, 64).expect("fenêtre masquée");
    let sequence = [(128, 64, CAS[0]), (64, 64, CAS[1]), (128, 64, CAS[2])];
    for (largeur, hauteur, (nom, yuv, rgb_attendu)) in sequence {
        let image = sky_rendu::image_de_test_nv12(fenetre.appareil(), largeur, hauteur, yuv)
            .expect("image NV12 de test");
        fenetre
            .afficher(&image)
            .unwrap_or_else(|e| panic!("affichage {largeur}×{hauteur} : {e:#}"));
        let obtenu = fenetre.pixel_central().expect("lecture du tampon");
        for (canal, (o, a)) in obtenu.iter().zip(rgb_attendu).enumerate() {
            let ecart = (*o as i32 - a as i32).abs();
            assert!(
                ecart <= 4,
                "{largeur}×{hauteur} {nom}, canal {canal} : {o} au lieu de {a} — \
                 l'image précédente est-elle encore affichée ?"
            );
        }
    }
}

/// Une image dont le rapport diffère de celui de la fenêtre doit être centrée
/// sans déformation, donc bordée de noir. Ce qui rend ce test discriminant : on
/// donne à l'image un rapport deux fois plus large que celui de la fenêtre, si
/// bien qu'un rendu étiré remplirait toute la hauteur et qu'aucun pixel de bord
/// ne serait noir.
#[test]
fn le_rapport_d_image_est_preserve_par_des_bandes_noires() {
    if sky_rendu::cartes_cuda_disponibles() == 0 {
        println!("aucune carte CUDA : pas d'interopérabilité à éprouver ici");
        return;
    }
    let mut fenetre =
        sky_rendu::Fenetre::ouvrir_masquee("rapport", 320, 240).expect("fenêtre masquée");
    // La taille réelle, pas celle demandée : Windows impose une largeur minimale.
    let (largeur, hauteur) = fenetre.taille();
    // Deux fois plus large, à hauteur égale : le rapport de l'image vaut le double
    // de celui de la fenêtre, donc l'échelle est bornée par la largeur et il reste
    // une bande en haut et en bas.
    let (largeur_image, hauteur_image) = (largeur * 2, hauteur);
    // Rouge saturé : aucun de ses canaux ne vaut 0 partout, donc le noir des
    // bandes ne peut pas être confondu avec la couleur de l'image.
    let image = sky_rendu::image_de_test_unie(largeur_image, hauteur_image, [76, 85, 255])
        .expect("surface de test");
    fenetre.afficher(&image).expect("affichage");

    let pixels = fenetre.pixels_de_la_cible().expect("lecture du tampon");
    let pixel = |x: u32, y: u32| {
        let i = ((y * largeur + x) * 4) as usize;
        [pixels[i + 2], pixels[i + 1], pixels[i]]
    };
    let (x, y) = (largeur / 2, hauteur / 2);
    // Ce test ne dit rien de la justesse de la conversion : il vérifie seulement
    // que l'image est là au centre et absente des bords. La matrice a son propre
    // test, et lui faire répondre ici ferait rougir les deux pour un seul défaut.
    let centre = pixel(x, y);
    assert_ne!(
        centre,
        [0, 0, 0],
        "le centre doit porter l'image, pas du noir"
    );
    // L'image occupe la moitié de la hauteur, centrée : le quart supérieur et le
    // quart inférieur sont donc entièrement noirs.
    assert_eq!(
        pixel(x, hauteur / 8),
        [0, 0, 0],
        "le haut doit être une bande noire, pas l'image étirée"
    );
    assert_eq!(
        pixel(x, hauteur - 1 - hauteur / 8),
        [0, 0, 0],
        "le bas doit être une bande noire, pas l'image étirée"
    );
}
