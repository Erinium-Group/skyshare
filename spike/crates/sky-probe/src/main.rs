mod cmd_capture;
mod cmd_compte;
mod cmd_netcheck;
mod cmd_selftest;
mod cmd_codecs;
mod cmd_encode;
mod cmd_host;
mod cmd_hw;
mod cmd_view;

use clap::{Args, Parser, Subcommand};

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
    /// Teste si ton réseau permet une connexion directe (à lancer seul)
    Netcheck,
    /// Négocie avec soi-même : teste tout le code, sans réseau ni correspondant
    Selftest,
    /// Mesure la capture d'écran (Q1)
    Capture {
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
        /// Cadence maximale de capture. Absent = suit le taux de l'écran.
        #[arg(long)]
        fps_max: Option<u32>,
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
    /// Émet : négocie la connexion, puis capture, encode et envoie un flux
    /// vidéo réel, piloté par le Pacer (Q4, Q5)
    Host {
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        /// h264420 | h264444 | hevc444 | av1420
        #[arg(long, default_value = "hevc444", value_parser = cmd_encode::parse_codec)]
        codec: sky_encode::Codec,
        /// Débit cible de la session NVENC — aussi le plafond du Pacer.
        #[arg(long, default_value_t = 30)]
        bitrate_mbps: u32,
        /// Plancher du Pacer : le débit réseau ne descend jamais en dessous.
        #[arg(long, default_value_t = 10)]
        floor_mbps: u32,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
        /// ecran (partage réel) | synthetique (charge reproductible pour la
        /// mesure Q4 — un écran figé ne fournit presque aucune image)
        #[arg(long, default_value = "ecran", value_parser = cmd_encode::parse_source)]
        source: cmd_encode::Source,
        /// Résolution de la source synthétique (sans effet sur `ecran`).
        #[arg(long, default_value_t = 2560)]
        width: u32,
        #[arg(long, default_value_t = 1440)]
        height: u32,
    },
    /// Reçoit : consomme le bloc d'offre, renvoie sa réponse, écrit le flux
    /// reçu et mesure débit/gigue/transit (Q4, Q5)
    View {
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, default_value = "recu.h265")]
        out: String,
    },
    /// Connexion au compte SkyShare (ouvre le navigateur, jalon C2)
    Login,
    /// Appareils enregistrés sur le compte
    Device(ArgsDevice),
    /// Amis et demandes d'ami
    Friends(ArgsFriends),
    /// Affiche le code ami de ce compte
    ///
    /// /!\ Synchronise avec le serveur : une négociation en cours perdrait
    /// l'offre en attente, le serveur l'efface en la livrant.
    Code,
}

#[derive(Args)]
struct ArgsDevice {
    #[command(subcommand)]
    cmd: CmdDevice,
}

#[derive(Subcommand)]
enum CmdDevice {
    /// Enregistre cet appareil auprès du compte connecté
    ///
    /// Refuse par défaut si un appareil est déjà enregistré sur cette
    /// machine : chaque enregistrement en crée un NOUVEAU côté serveur.
    Register {
        /// Nom affiché pour cet appareil (visible par les amis)
        nom: String,
        /// Force un nouvel enregistrement même si un appareil existe déjà
        #[arg(long)]
        force: bool,
    },
    /// Liste les appareils enregistrés sur ce compte
    ///
    /// /!\ Synchronise avec le serveur : une négociation en cours perdrait
    /// l'offre en attente, le serveur l'efface en la livrant.
    List,
}

#[derive(Args)]
struct ArgsFriends {
    #[command(subcommand)]
    cmd: CmdFriends,
}

#[derive(Subcommand)]
enum CmdFriends {
    /// Envoie une demande d'ami par code
    ///
    /// /!\ Synchronise avec le serveur (pour connaître son propre code) :
    /// une négociation en cours perdrait l'offre en attente.
    Add {
        /// Code ami à ajouter (ex. SKY-ABCD-EFGH)
        code: String,
    },
    /// Accepte une demande d'ami reçue
    Accept {
        /// Identifiant de la demande (voir `friends list`)
        id: i64,
    },
    /// Liste les amis et les demandes reçues
    ///
    /// /!\ Synchronise avec le serveur : une négociation en cours perdrait
    /// l'offre en attente, le serveur l'efface en la livrant.
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Hw => cmd_hw::run(),
        Cmd::Netcheck => cmd_netcheck::run(),
        Cmd::Selftest => cmd_selftest::run(),
        Cmd::Capture {
            seconds,
            monitor,
            fps_max,
        } => cmd_capture::run(seconds, monitor, fps_max),
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
        Cmd::Host {
            seconds,
            codec,
            bitrate_mbps,
            floor_mbps,
            monitor,
            source,
            width,
            height,
        } => cmd_host::run(cmd_host::Parametres {
            secondes: seconds,
            codec,
            bitrate_mbps,
            floor_mbps,
            monitor,
            source,
            largeur_synth: width,
            hauteur_synth: height,
        }),
        Cmd::View { seconds, out } => cmd_view::run(seconds, &out),
        Cmd::Login => {
            let (config, coffre) = cmd_compte::config_et_coffre()?;
            cmd_compte::login(&config, &coffre)
        }
        Cmd::Device(args) => {
            let (config, coffre) = cmd_compte::config_et_coffre()?;
            match args.cmd {
                CmdDevice::Register { nom, force } => cmd_compte::device_register(&config, &coffre, &nom, force),
                CmdDevice::List => cmd_compte::device_list(&config, &coffre),
            }
        }
        Cmd::Friends(args) => {
            let (config, coffre) = cmd_compte::config_et_coffre()?;
            match args.cmd {
                CmdFriends::Add { code } => cmd_compte::friends_add(&config, &coffre, &code),
                CmdFriends::Accept { id } => cmd_compte::friends_accept(&config, &coffre, id),
                CmdFriends::List => cmd_compte::friends_list(&config, &coffre),
            }
        }
        Cmd::Code => {
            let (config, coffre) = cmd_compte::config_et_coffre()?;
            cmd_compte::code(&config, &coffre)
        }
    }
}
