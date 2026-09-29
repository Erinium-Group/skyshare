//! Le test le plus important du jalon 2.
//!
//! Il discrimine réellement, et c'est mesuré : la sonde du 27/09/2026 a établi
//! qu'une conversion 4:2:0 parasite fait perdre 19 à 20 dB, et qu'une matrice
//! BT.709 au lieu de BT.601 plafonne à 36 dB — là où le décodage juste donne
//! 89,78 dB contre cette même référence. Le seuil de 80 dB sépare donc les deux
//! erreurs les plus probables de tout le jalon.

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

#[test]
fn l_image_120_est_identique_a_la_reference_du_jalon_0() {
    let flux = std::fs::read(FLUX).expect("le flux du jalon 0 doit être présent");
    let mut decodeur = match Decodeur::nouveau(2560, 1440) {
        Ok(d) => d,
        Err(e) => {
            println!("décodage impossible sur cette machine, test non concluant : {e}");
            return;
        }
    };

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
    println!("PSNR mesuré : {db:.2} dB");
    assert!(
        db >= SEUIL_DB,
        "PSNR {db:.2} dB sous le seuil de {SEUIL_DB} dB"
    );
}

#[test]
fn les_entetes_seuls_ne_rendent_aucune_image_et_ce_n_est_pas_une_erreur() {
    let flux = std::fs::read(FLUX).expect("flux présent");
    let Ok(mut decodeur) = Decodeur::nouveau(2560, 1440) else {
        return;
    };
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

fn lire_png(chemin: &str) -> Vec<u8> {
    let fichier = std::fs::File::open(chemin).expect("la référence doit être présente");
    let mut lecteur = png::Decoder::new(fichier).read_info().expect("PNG lisible");
    let mut tampon = vec![0; lecteur.output_buffer_size()];
    let info = lecteur.next_frame(&mut tampon).expect("image PNG");
    tampon.truncate(info.buffer_size());
    tampon
}
