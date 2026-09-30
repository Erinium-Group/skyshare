//! Le chemin GPU ne doit pas altérer la couleur. Ce test ne mesure pas le
//! décodage (la tâche 2 s'en charge) mais la traversée interopérabilité +
//! nuanceur : on pousse une surface YUV 4:4:4 dont on connaît la couleur
//! exacte, et on relit le pixel présenté.

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
    // interopérabilité, et la spec assume qu'une telle machine ne peut pas
    // recevoir. Partout ailleurs, tout ce qui suit doit réussir — un `else` qui
    // avalerait l'échec laisserait passer la régression que ce test traque.
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
