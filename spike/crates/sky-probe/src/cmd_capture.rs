use std::time::{Duration, Instant};

use sky_capture::wgc::WgcCapture;

pub fn run(seconds: u64, monitor: usize) -> anyhow::Result<()> {
    let mut cap = WgcCapture::new(monitor)?;
    println!("Capture démarrée. Bouge des fenêtres pour générer du mouvement.");

    let fin = Instant::now() + Duration::from_secs(seconds);
    let mut pires_ecarts = Vec::new();
    let mut precedente = Instant::now();

    while Instant::now() < fin {
        if cap.next_frame(Duration::from_millis(50))?.is_some() {
            let maintenant = Instant::now();
            pires_ecarts.push(maintenant.duration_since(precedente).as_secs_f32() * 1000.0);
            precedente = maintenant;
        }
    }

    let s = cap.stats();
    pires_ecarts.sort_by(|a, b| b.partial_cmp(a).unwrap());

    println!("\n--- Q1 : capture ---");
    println!("Images capturées : {}", s.frames);
    println!("Délais dépassés  : {}", s.dropped);
    println!("FPS moyen        : {:.1}", s.avg_fps);
    println!(
        "Pire intervalle  : {:.1} ms",
        pires_ecarts.first().copied().unwrap_or(0.0)
    );
    println!(
        "Verdict          : {}",
        if s.avg_fps >= 59.0 { "SUCCÈS" } else { "ÉCHEC" }
    );
    Ok(())
}
