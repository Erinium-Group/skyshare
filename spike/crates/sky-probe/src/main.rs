mod cmd_capture;
mod cmd_codecs;
mod cmd_encode;
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
    /// Encode des textures Direct3D 11 avec NVENC, sans copie CPU (Q2)
    Encode {
        #[arg(long, default_value_t = 20)]
        seconds: u64,
        /// h264420 | h264444 | hevc444 | av1420
        #[arg(long, default_value = "hevc444", value_parser = cmd_encode::parse_codec)]
        codec: sky_encode::Codec,
        #[arg(long, default_value_t = 30)]
        bitrate_mbps: u32,
        #[arg(long, default_value = "test.h265")]
        out: String,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
        /// ecran (textures réelles) | synthetique (texture D3D11 animée)
        #[arg(long, default_value = "ecran", value_parser = cmd_encode::parse_source)]
        source: cmd_encode::Source,
    },
    /// Compare les 4 combinaisons codec/chroma à débit égal, sur la même
    /// scène synthétique déterministe (Q3)
    Codecs {
        #[arg(long, default_value_t = 15)]
        seconds: u64,
        #[arg(long, default_value_t = 10)]
        bitrate_mbps: u32,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hw => cmd_hw::run(),
        Cmd::Capture { seconds, monitor } => cmd_capture::run(seconds, monitor),
        Cmd::Encode {
            seconds,
            codec,
            bitrate_mbps,
            out,
            monitor,
            source,
        } => cmd_encode::run(seconds, codec, bitrate_mbps, &out, monitor, source),
        Cmd::Codecs {
            seconds,
            bitrate_mbps,
            monitor,
        } => cmd_codecs::run(seconds, bitrate_mbps, monitor),
    }
}
