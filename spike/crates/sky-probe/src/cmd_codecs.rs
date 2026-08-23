//! Q3 : le 4:4:4 apporte-t-il un gain mesurable sur le texte fin par rapport
//! au 4:2:0, à débit égal ?
//!
//! Écart avec le brief d'origine : celui-ci demandait de préparer quatre
//! fois la même scène à la main (éditeur + vidéo sur l'écran réel) et
//! d'appuyer sur Entrée entre chaque encodage. Deux problèmes : ce protocole
//! ne peut pas tourner sans surveillance, et une scène reconstituée à la
//! main quatre fois de suite n'est *jamais* rigoureusement identique — ce
//! que le brief lui-même exige pour que la comparaison ait un sens.
//!
//! On utilise donc la texture synthétique de Q2 : son contenu ne dépend que
//! du nombre d'appels à `prochaine_image`, donc en recréant la texture (donc
//! son compteur à 0) pour chaque codec, les quatre flux voient exactement la
//! même séquence d'images, image par image. La comparaison porte alors
//! uniquement sur le codec et le sous-échantillonnage chroma — jamais sur
//! une différence de scène.
//!
//! Cette commande produit aussi une référence non compressée
//! (`cmp-reference.bgra`, rawvideo BGRA) : la même séquence, calculée
//! directement côté CPU par [`crate::cmd_encode::ecrire_reference_brute`],
//! sans passer par le GPU. C'est elle que Task 4 utilise pour le calcul de
//! PSNR/SSIM par plan (voir `spike/docs/comparatif-codecs.md`).

use std::fs::File;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use sky_capture::wgc::WgcCapture;
use sky_capture::CapturedFrame;
use sky_encode::{nvenc::NvencEncoder, Codec};

use crate::cmd_encode::{ecrire_reference_brute, encoder_synthetique, TextureSynthetique, FPS};

/// Marge de sécurité (en images) ajoutée à la référence brute au-delà de
/// `seconds * FPS`, pour ne jamais être le flux le plus court des cinq — un
/// éventuel écart de calage viendrait alors d'un encodage, pas de la
/// référence. Une jitter de contrôle de boucle de quelques images sur 15 s à
/// 60 i/s est largement couverte.
const MARGE_IMAGES_REFERENCE: u32 = 120;

pub fn run(seconds: u64, bitrate_mbps: u32, monitor: usize) -> anyhow::Result<()> {
    let combinaisons = [
        (Codec::H264_420, "cmp-h264-420.h264"),
        (Codec::H264_444, "cmp-h264-444.h264"),
        (Codec::Hevc444, "cmp-hevc-444.h265"),
        (Codec::Av1_420, "cmp-av1-420.ivf"),
    ];
    const REFERENCE: &str = "cmp-reference.bgra";

    // Une seule capture réelle, juste pour obtenir les dimensions d'écran et
    // le device Direct3D 11 sur lequel NVENC doit ouvrir sa session. Rien de
    // son contenu n'est utilisé ensuite : la source synthétique l'ignore
    // complètement.
    let mut cap = WgcCapture::new(monitor, None).context("démarrage de la capture (device D3D11)")?;
    let premiere = attendre_premiere_image(&mut cap)?;
    let (largeur, hauteur) = (premiere.width, premiere.height);

    println!("=== Q3 : comparatif des 4 codecs, à débit égal ===");
    println!("Résolution         : {largeur}x{hauteur}");
    println!("Débit visé         : {bitrate_mbps} Mbps (identique pour les 4 combinaisons)");
    println!("Durée par flux      : {seconds} s à {FPS} i/s");
    println!(
        "Scène               : texture synthétique déterministe (panneau de texte fin, \
         rouge/bleu saturés sur fond sombre — voir cmd_encode::motif_detaille)\n"
    );

    let nb_images_reference = (seconds * FPS as u64) as u32 + MARGE_IMAGES_REFERENCE;
    println!(
        "--- Référence brute : {nb_images_reference} images BGRA -> {REFERENCE} ---"
    );
    ecrire_reference_brute(largeur, hauteur, nb_images_reference, REFERENCE)
        .context("écriture de la référence brute (BGRA, sans compression)")?;
    let taille_reference = std::fs::metadata(REFERENCE)?.len();
    println!(
        "Référence écrite    : {taille_reference} octets ({:.1} Mo)\n",
        taille_reference as f64 / 1e6
    );

    let mut resultats = Vec::new();

    for (codec, sortie) in combinaisons {
        println!("=== {} -> {sortie} ===", codec.label());

        let mut enc = NvencEncoder::new(
            cap.d3d_device(),
            codec,
            largeur,
            hauteur,
            FPS,
            bitrate_mbps * 1_000_000,
        )
        .with_context(|| format!("ouverture de la session NVENC pour {}", codec.label()))?;

        // Recréée à chaque codec : son compteur interne `n` repart de 0, ce
        // qui garantit que ce codec voit la MÊME séquence d'images que les
        // trois autres, image par image — pas une suite décalée.
        let mut synth = TextureSynthetique::new(cap.d3d_device(), largeur, hauteur)
            .context("création de la texture synthétique")?;

        let mut fichier = File::create(sortie).with_context(|| format!("création de {sortie}"))?;
        let stats = encoder_synthetique(&mut enc, &mut synth, seconds, &mut fichier)?;
        drop(fichier);
        drop(enc);

        let debit_reel_mbps = stats.octets as f64 * 8.0 / stats.duree / 1e6;
        println!("  images encodées  : {} (dont {} clé(s))", stats.images, stats.cles);
        println!("  octets écrits    : {}", stats.octets);
        println!("  débit réel       : {debit_reel_mbps:.2} Mbps (cible : {bitrate_mbps} Mbps)");
        println!("  encodage médian  : {:.2} ms", stats.p50_us as f64 / 1000.0);
        println!("  encodage p99     : {:.2} ms\n", stats.p99_us as f64 / 1000.0);

        resultats.push((codec, sortie, stats.images, stats.octets, debit_reel_mbps));
    }

    println!("=== Résumé ===");
    println!(
        "{:<14} {:>10} {:>14} {:>14}",
        "Codec", "Images", "Octets", "Débit réel (Mbps)"
    );
    for (codec, _, images, octets, debit) in &resultats {
        println!("{:<14} {:>10} {:>14} {:>14.2}", codec.label(), images, octets, debit);
    }

    println!(
        "\nFichiers produits : {REFERENCE}, {}",
        combinaisons
            .iter()
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "\nMesure PSNR/SSIM par plan et extraction d'images : voir \
         spike/docs/comparatif-codecs.md (méthode et commandes ffmpeg utilisées)."
    );

    Ok(())
}

/// Attend la première image réelle de la capture (uniquement pour connaître
/// les dimensions et le device D3D11) — identique à la logique de
/// `cmd_encode::run`, dupliquée ici pour ne pas complexifier sa signature
/// avec un paramètre utilisé par une seule autre commande.
fn attendre_premiere_image(cap: &mut WgcCapture) -> anyhow::Result<CapturedFrame> {
    let attente_max = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
            return Ok(f);
        }
        if Instant::now() >= attente_max {
            return Err(anyhow!(
                "aucune image capturée en 5 s — l'écran est-il totalement figé ?"
            ));
        }
    }
}
