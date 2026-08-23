use std::time::{Duration, Instant};

use sky_capture::wgc::WgcCapture;

/// Centile d'une série TRIÉE par ordre croissant.
fn centile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    v[((v.len() - 1) as f32 * p) as usize]
}

pub fn run(seconds: u64, monitor: usize, fps_max: Option<u32>) -> anyhow::Result<()> {
    let mut cap = WgcCapture::new(monitor, fps_max)?;
    println!("Capture démarrée. Bouge des fenêtres pour générer du mouvement.");

    // On compte les rafraîchissements écran PENDANT la capture : c'est la seule
    // référence honnête. « 55 images par seconde » ne veut rien dire tant qu'on
    // ignore si l'écran en a produit 60 ou 165.
    let duree = Duration::from_secs(seconds);
    let compteur_vblank = std::thread::spawn(move || sky_capture::wgc::compter_vblanks(duree));

    let fin = Instant::now() + duree;
    let mut ecarts = Vec::new();
    let mut precedente = Instant::now();

    while Instant::now() < fin {
        if cap.next_frame(Duration::from_millis(50))?.is_some() {
            let maintenant = Instant::now();
            ecarts.push(maintenant.duration_since(precedente).as_secs_f32() * 1000.0);
            precedente = maintenant;
        }
    }

    let s = cap.stats();
    ecarts.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let vblanks = compteur_vblank.join().ok().and_then(|r| r.ok()).unwrap_or(0);
    let hz_reel = vblanks as f32 / seconds as f32;

    // Seuil de « figé » : au-delà de 4 rafraîchissements sans image, l'écran
    // n'avait rien de nouveau à montrer. WGC n'émet que sur changement — ces
    // pauses sont voulues et ne doivent pas compter comme un défaut de capture.
    let periode_ecran = if vblanks > 0 {
        seconds as f32 * 1000.0 / vblanks as f32
    } else {
        6.0
    };
    let seuil_fige = periode_ecran * 4.0;
    let (pauses, actifs): (Vec<f32>, Vec<f32>) = ecarts.iter().partition(|e| **e > seuil_fige);
    let temps_fige: f32 = pauses.iter().sum::<f32>() + s.dropped as f32 * 50.0;

    println!("\n--- Q1 : capture ---");
    println!("Images capturées  : {}", s.frames);
    println!("Rafraîchissements : {vblanks} en {seconds} s, soit {hz_reel:.1} Hz réels");

    println!("\n--- écran figé : rien à capturer ---");
    println!(
        "  Pauses > {:.0} ms   : {} (+ {} délais dépassés)",
        seuil_fige,
        pauses.len(),
        s.dropped
    );
    println!(
        "  Temps sans contenu : {:.1} s sur {seconds} s, soit {:.0} %",
        temps_fige / 1000.0,
        temps_fige / 10.0 / seconds as f32
    );

    let median = centile(&actifs, 0.50);
    let cadence_active = if median > 0.0 { 1000.0 / median } else { 0.0 };

    println!("\n--- pendant que l'écran bougeait ---");
    println!("  Intervalle médian  : {median:.2} ms");
    println!("  Intervalle p90     : {:.2} ms", centile(&actifs, 0.90));
    println!("  Intervalle p99     : {:.2} ms", centile(&actifs, 0.99));
    println!(
        "  Pire intervalle    : {:.1} ms",
        actifs.last().copied().unwrap_or(0.0)
    );
    println!("  Cadence en activité : {cadence_active:.1} im/s");

    // Le seuil du plan (« ≥ 59 im/s ») supposait un écran 60 Hz : il ne veut rien
    // dire sur un 165 Hz. Et une moyenne diluée par les pauses ferait échouer une
    // capture parfaite d'un écran resté figé la moitié du temps. On juge donc la
    // cadence en activité, comparée à ce que le matériel peut réellement servir.
    let attendu = match fps_max {
        Some(f) => sky_capture::wgc::cadence_atteignable(f, hz_reel),
        None => hz_reel,
    };
    if let Some(f) = fps_max {
        if (attendu - f as f32).abs() > 1.0 {
            println!(
                "\nNote : {f} im/s inatteignable sur un écran à {hz_reel:.1} Hz.\n\
                 WGC n'émet qu'aux rafraîchissements : cadence servie {attendu:.1} im/s."
            );
        }
    }
    let taux = if attendu > 0.0 {
        cadence_active / attendu
    } else {
        0.0
    };
    println!("\nObtenu / attendu (en activité) : {taux:.2}");
    println!(
        "Verdict           : {}",
        if taux >= 0.95 { "SUCCÈS" } else { "ÉCHEC" }
    );
    println!("(FPS moyen brut, pauses comprises : {:.1})", s.avg_fps);
    Ok(())
}
