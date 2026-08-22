use sky_encode::{pick_best, probe_hardware};

pub fn run() -> anyhow::Result<()> {
    let caps = probe_hardware()?;

    println!("GPU        : {}", caps.gpu_name);
    println!("Codecs     :");
    for c in &caps.codecs {
        println!(
            "  - {} {}",
            c.label(),
            if c.is_444() { "(texte net)" } else { "" }
        );
    }

    match pick_best(&caps, true) {
        Some(c) => println!("\nChoix partage d'écran : {}", c.label()),
        None => println!("\nAucun encodeur matériel utilisable."),
    }
    if let Some(c) = pick_best(&caps, false) {
        println!("Choix vidéo            : {}", c.label());
    }
    Ok(())
}
