use std::time::{Duration, Instant};

use sky_capture::wgc::WgcCapture;

pub fn run(seconds: u64, monitor: usize, fps_max: Option<u32>) -> anyhow::Result<()> {
    let mut cap = WgcCapture::new(monitor, fps_max)?;
    println!("Capture démarrée. Bouge des fenêtres pour générer du mouvement.");

    // On compte les rafraîchissements écran PENDANT la capture. C'est la seule
    // référence honnête : « 55 images par seconde » ne veut rien dire tant qu'on
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
    ecarts.sort_by(|a, b| b.partial_cmp(a).unwrap());

    let vblanks = compteur_vblank.join().ok().and_then(|r| r.ok()).unwrap_or(0);
    let hz_reel = vblanks as f32 / seconds as f32;

    println!("\n--- Q1 : capture ---");
    println!("Images capturées  : {}", s.frames);
    println!("Délais dépassés   : {}", s.dropped);
    println!("FPS moyen         : {:.1}", s.avg_fps);
    println!(
        "Pire intervalle   : {:.1} ms",
        ecarts.first().copied().unwrap_or(0.0)
    );
    println!("Rafraîchissements : {vblanks} en {seconds} s, soit {hz_reel:.1} Hz réels");

    // Le seuil du plan (« ≥ 59 im/s ») supposait un écran 60 Hz : il ne veut rien
    // dire sur un 165 Hz. La bonne question est le rapport — capture-t-on chaque
    // image que l'écran produit, ou la cadence demandée si elle est plus basse ?
    let attendu = match fps_max {
        Some(f) => sky_capture::wgc::cadence_atteignable(f, hz_reel),
        None => hz_reel,
    };
    if let Some(f) = fps_max {
        if (attendu - f as f32).abs() > 1.0 {
            println!(
                "Note : {f} im/s inatteignable sur un écran à {hz_reel:.1} Hz.
       \n                 WGC n'émet qu'aux rafraîchissements : cadence servie {attendu:.1} im/s.",
            );
        }
    }
    let taux = if attendu > 0.0 { s.avg_fps / attendu } else { 0.0 };
    println!("Obtenu / attendu  : {taux:.2}");
    println!(
        "Verdict           : {}",
        if taux >= 0.95 { "SUCCÈS" } else { "ÉCHEC" }
    );
    Ok(())
}
