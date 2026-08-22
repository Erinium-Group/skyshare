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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hw => cmd_hw::run(),
    }
}
