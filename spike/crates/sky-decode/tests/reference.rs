//! Le test le plus important du jalon 2.
//!
//! Il discrimine réellement, et c'est mesuré : la sonde du 27/09/2026 a établi
//! qu'une conversion 4:2:0 parasite fait perdre 19 à 20 dB, et qu'une matrice
//! BT.709 au lieu de BT.601 plafonne à 36 dB — là où le décodage juste donne
//! 89,78 dB contre cette même référence. Le seuil de 80 dB sépare donc les deux
//! erreurs les plus probables de tout le jalon.
//!
//! **Cette implémentation-ci mesure 85,50 dB, et ce n'est pas une régression.**
//! Les deux nombres mesurent la même propriété par deux chemins de conversion
//! différents : la référence PNG est sortie du convertisseur en virgule fixe de
//! swscale, cette implémentation convertit en `f64` exact. L'écart maximal entre
//! les deux images est de **1 LSB** — 2 028 composantes sur 11 059 200 ici, 756
//! pour les 89,78 dB. Les deux décodages sont justes au bit près, à un demi-LSB
//! d'arrondi. C'est l'assertion `ECART_MAX_TOLERE` ci-dessous qui le prouve ; le
//! seuil en dB, plus grossier à ce niveau, ne sert qu'à la robustesse.

// `nvidia-video-codec-sdk` référence `NvEncodeAPICreateInstance` et
// `NvEncodeAPIGetMaxSupportedVersion` dans un bloc `extern "C"` lié
// statiquement. La bibliothèque `sky-decode` ne fournit ses souches que sous
// `#[cfg(test)]` : un test d'intégration la compile SANS `cfg(test)` et
// échouerait donc au lien (LNK2019). `sky-encode` les définit sans condition
// (`sky-encode/src/nvenc_sys.rs`), et c'est la définition unique du dépôt — la
// redéfinir ici en produirait un doublon (LNK2005) dès que `sky-partage`
// dépendra des deux crates. On se contente donc de lier `sky-encode`.
use sky_encode as _;

use sky_decode::Decodeur;

const FLUX: &str = "../../cmp-hevc-444.h265";
const REFERENCE: &str = "../../mesures/frame120-hevc-444.png";
const IMAGE_COMPAREE: usize = 120;
const SEUIL_DB: f64 = 80.0;
/// Écart maximal toléré sur une composante, en niveaux.
///
/// C'est la vraie preuve, et elle est bien plus fine que le seuil en dB : à 1,
/// seul un demi-LSB d'arrondi est admis. Toute erreur structurelle — matrice,
/// chrominance, décalage de plan, mauvaise image — produit au moins un écart de
/// plusieurs dizaines de niveaux quelque part, que la moyenne d'un PSNR dilue.
const ECART_MAX_TOLERE: u8 = 1;

/// Ouvre le décodeur, ou fait échouer le test en disant que le matériel manque.
///
/// Ce test est celui du **receveur** : le jalon 0 a établi qu'une machine sans
/// carte NVIDIA ne peut qu'émettre. Se taire en vert sans décodeur serait « une
/// preuve qui passerait aussi bien dans le cas négatif » — la faute que ce dépôt
/// documente nommément.
fn decodeur_ou_echouer() -> Decodeur {
    match Decodeur::nouveau(2560, 1440) {
        Ok(decodeur) => decodeur,
        Err(e) => {
            panic!("ce test exige un décodeur NVIDIA HEVC 4:4:4, et cette machine n'en a pas : {e}")
        }
    }
}

#[test]
fn l_image_120_est_identique_a_la_reference_du_jalon_0() {
    let flux = std::fs::read(FLUX).expect("le flux du jalon 0 doit être présent");
    let mut decodeur = decodeur_ou_echouer();

    let mut rendues = Vec::new();
    for (rang, unite) in unites_acces(&flux).into_iter().enumerate() {
        if let Some(image) = decodeur.decoder(&unite, rang as u64).expect("décodage") {
            rendues.push(image.copier_vers_memoire_centrale().expect("copie de test"));
            if rendues.len() > IMAGE_COMPAREE {
                break;
            }
        }
    }

    let obtenue = &rendues[IMAGE_COMPAREE];
    let attendue = lire_png(REFERENCE);
    let db = psnr(obtenue, &attendue);
    let ecart_max = ecart_maximal(obtenue, &attendue);
    println!("PSNR mesuré : {db:.2} dB, écart maximal : {ecart_max} niveau(x)");
    assert!(
        db >= SEUIL_DB,
        "PSNR {db:.2} dB sous le seuil de {SEUIL_DB} dB"
    );
    assert!(
        ecart_max <= ECART_MAX_TOLERE,
        "écart maximal de {ecart_max} niveaux, l'arrondi ne l'explique plus"
    );
}

#[test]
fn les_entetes_seuls_ne_rendent_aucune_image_et_ce_n_est_pas_une_erreur() {
    let flux = std::fs::read(FLUX).expect("flux présent");
    // Même exigence de matériel que le test de référence, et pour la même raison.
    let mut decodeur = decodeur_ou_echouer();
    // Les trois premières unités d'accès d'un flux NVENC sont VPS, SPS, PPS.
    for (rang, unite) in unites_acces(&flux).into_iter().take(3).enumerate() {
        let rendu = decodeur
            .decoder(&unite, rang as u64)
            .expect("pas une erreur");
        assert!(rendu.is_none(), "un en-tête ne produit pas d'image");
    }
}

/// Découpe un flux Annex-B en unités d'accès sur les codes de départ.
fn unites_acces(flux: &[u8]) -> Vec<Vec<u8>> {
    let mut debuts = Vec::new();
    let mut i = 0;
    while i + 3 < flux.len() {
        if flux[i] == 0 && flux[i + 1] == 0 && flux[i + 2] == 1 {
            debuts.push(i);
            i += 3;
        } else {
            i += 1;
        }
    }
    debuts
        .iter()
        .enumerate()
        .map(|(n, &d)| {
            let fin = debuts.get(n + 1).copied().unwrap_or(flux.len());
            flux[d..fin].to_vec()
        })
        .collect()
}

/// Rapport signal/bruit de crête entre deux images RGB de même taille.
fn psnr(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(
        a.len(),
        b.len(),
        "les deux images doivent avoir la même taille"
    );
    let somme: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| {
            let d = *x as f64 - *y as f64;
            d * d
        })
        .sum();
    let eqm = somme / a.len() as f64;
    if eqm == 0.0 {
        return f64::INFINITY;
    }
    10.0 * (255.0f64 * 255.0 / eqm).log10()
}

/// Plus grand écart entre deux composantes de même rang.
fn ecart_maximal(a: &[u8], b: &[u8]) -> u8 {
    assert_eq!(
        a.len(),
        b.len(),
        "les deux images doivent avoir la même taille"
    );
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

fn lire_png(chemin: &str) -> Vec<u8> {
    let fichier = std::fs::File::open(chemin).expect("la référence doit être présente");
    let mut lecteur = png::Decoder::new(fichier).read_info().expect("PNG lisible");
    let mut tampon = vec![0; lecteur.output_buffer_size()];
    let info = lecteur.next_frame(&mut tampon).expect("image PNG");
    tampon.truncate(info.buffer_size());
    tampon
}
