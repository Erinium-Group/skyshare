mod cmd_capture;
mod cmd_hw;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sky-probe", about = "Spike de faisabilité SkyShare — jalon 0")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Détecte le GPU et liste les codecs réellement encodables
    Hw,
    /// Mesure la capture d'écran (Q1)
    Capture {
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hw => cmd_hw::run(),
        Cmd::Capture { seconds, monitor } => cmd_capture::run(seconds, monitor),
    }
}
