# Jalon 0 — Spike de faisabilité — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** Prouver par la mesure, en 3 à 5 jours, que la chaîne capture GPU → encodage matériel 4:4:4 → connexion pair-à-pair tient sur du matériel et des réseaux réels — ou identifier précisément où elle casse.

**Architecture :** Un workspace Rust jetable produisant un binaire CLI unique, `sky-probe`, avec une sous-commande par question de faisabilité. Aucune interface graphique, aucun backend Vercel, aucune base de données : le signaling se fait par copier-coller manuel entre deux machines. Chaque module porte le nom de son équivalent dans l'architecture cible pour que le code qui survit puisse être promu plutôt que réécrit.

**Tech Stack :** Rust 1.94 (MSVC) · `windows` (Windows.Graphics.Capture, Direct3D11) · `nvidia-video-codec-sdk` 0.4 (module `sys`, FFI NVENC) · `str0m` 0.23.1 (WebRTC sans-IO) · `crypto_box` (scellage X25519)

**Spec :** [`docs/superpowers/specs/2026-08-22-skyshare-architecture-design.md`](../specs/2026-08-22-skyshare-architecture-design.md)

## Global Constraints

- **Rust 1.94.0 stable, cible `x86_64-pc-windows-msvc`.** Aucune dépendance à un toolchain nightly.
- **Windows 11 build 26200** — `SetIsBorderRequired(false)` est disponible (build ≥ 22000) et doit être utilisé.
- **Matériel de référence : NVIDIA GeForce RTX 4060, driver 610.74.** NVENC 8ᵉ génération : H.264 4:4:4 ✅, HEVC 4:4:4 ✅, AV1 4:2:0 uniquement (pas de 4:4:4 en AV1 — contrainte matérielle NVIDIA, pas un défaut d'implémentation).
- **Aucun serveur.** Signaling par copier-coller. Seuls appels réseau sortants autorisés : `stun.l.google.com:19302` et `stun.cloudflare.com:3478`.
- **Aucune adresse IP ne doit apparaître dans une sortie console, un log ou un fichier**, sauf sous la sous-commande explicite `sky-probe netcheck`. Contrainte du spec §5.2, à respecter dès le spike pour ne pas prendre l'habitude inverse.
- **Time-box strict de 5 jours.** Une tâche qui dépasse son time-box déclenche le repli documenté, pas une rallonge.
- **Le livrable final est le rapport**, pas le code. Le code est explicitement jetable.

## Les 6 questions auxquelles ce spike doit répondre

Chaque tâche produit une réponse chiffrée à l'une d'elles. Le rapport final (Tâche 9) les rassemble.

| Q | Question | Seuil de succès |
|---|----------|-----------------|
| **Q1** | Peut-on capturer 2560×1440 à 60 fps sans copie CPU ? | ≥ 59 fps moyens, < 1 % d'images perdues |
| **Q2** | NVENC accepte-t-il une texture D3D11 en 4:4:4 ? | Session ouverte, bitstream valide produit |
| **Q3** | Quel codec donne le meilleur texte à débit égal ? | Comparatif chiffré des 4 combinaisons |
| **Q4** | La chaîne complète reste-t-elle sous 5 % de CPU ? | < 5 % sur un cœur logique moyen |
| **Q5** | Deux machines derrière deux box se connectent-elles ? | Connexion établie entre deux FAI distincts |
| **Q6** | Le contrôle de congestion tient-il le plancher de débit ? | Ne descend jamais sous le plancher, remonte en < 2 s |

---

## File Structure

```
spike/
├── Cargo.toml                      # workspace, resolver 2
├── README-AMI.md                   # mode d'emploi à envoyer au testeur distant
├── crates/
│   ├── sky-capture/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # trait ScreenCapture, CapturedFrame, CaptureStats
│   │       └── wgc.rs              # implémentation Windows.Graphics.Capture
│   ├── sky-encode/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # trait VideoEncoder, EncodedPacket, EncoderCaps
│   │       ├── caps.rs             # détection matérielle + sélection de codec (testable)
│   │       └── nvenc.rs            # FFI NVENC avec entrée D3D11
│   ├── sky-crypto/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs              # scellage/descellage des adresses (TDD pur)
│   ├── sky-net/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs              # PeerLink : façade sur str0m
│   │       ├── pacer.rs            # contrôle de congestion à plancher (TDD pur)
│   │       └── handshake.rs        # blob de signaling copier-collable
│   └── sky-probe/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs             # dispatch des sous-commandes
│           ├── cmd_hw.rs           # sky-probe hw
│           ├── cmd_capture.rs      # sky-probe capture
│           ├── cmd_encode.rs       # sky-probe encode
│           ├── cmd_codecs.rs       # sky-probe codecs
│           ├── cmd_host.rs         # sky-probe host
│           └── cmd_view.rs         # sky-probe view
└── docs/
    └── rapport-jalon-0.md          # LE livrable
```

**Frontières de responsabilité :**
- `sky-capture` ne connaît ni encodeur ni réseau. Il produit des `CapturedFrame` portant une texture GPU.
- `sky-encode` ne connaît ni capture ni réseau. Il consomme une texture, produit des octets.
- `sky-crypto` et `sky-net::pacer` sont de la logique pure, sans I/O, donc testables unitairement — c'est là que le TDD s'applique réellement.
- `sky-probe` est la seule crate qui assemble, mesure et affiche.

---

## Task 1: Workspace et détection matérielle

**Files:**
- Create: `spike/Cargo.toml`
- Create: `spike/crates/sky-encode/Cargo.toml`
- Create: `spike/crates/sky-encode/src/lib.rs`
- Create: `spike/crates/sky-encode/src/caps.rs`
- Create: `spike/crates/sky-probe/Cargo.toml`
- Create: `spike/crates/sky-probe/src/main.rs`
- Create: `spike/crates/sky-probe/src/cmd_hw.rs`
- Test: `spike/crates/sky-encode/src/caps.rs` (module `#[cfg(test)]` en fin de fichier)

**Interfaces:**
- Consumes: rien (première tâche)
- Produces:
  - `sky_encode::caps::Codec` — enum `{ H264_420, H264_444, Hevc444, Av1_420 }`
  - `sky_encode::caps::EncoderCaps { pub gpu_name: String, pub codecs: Vec<Codec> }`
  - `sky_encode::caps::pick_best(caps: &EncoderCaps, prefer_text: bool) -> Option<Codec>`
  - `sky_encode::caps::probe_hardware() -> Result<EncoderCaps, EncodeError>`

- [ ] **Step 1: Créer le workspace**

```bash
mkdir -p spike/crates
cd spike
```

Créer `spike/Cargo.toml` :

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.package]
version = "0.0.0"
edition = "2021"
rust-version = "1.94"
publish = false

[workspace.dependencies]
anyhow = "1"
```

- [ ] **Step 2: Créer la crate sky-encode**

```bash
cargo new --lib crates/sky-encode
cargo new --lib crates/sky-crypto
cargo new --lib crates/sky-net
cargo new --lib crates/sky-capture
cargo new --bin crates/sky-probe
```

Remplacer `spike/crates/sky-encode/Cargo.toml` par :

```toml
[package]
name = "sky-encode"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true
nvidia-video-codec-sdk = "0.4"
thiserror = "2"
```

- [ ] **Step 3: Écrire le test de sélection de codec (il doit échouer)**

Créer `spike/crates/sky-encode/src/caps.rs` avec **uniquement** le bloc de test :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn caps_rtx4060() -> EncoderCaps {
        EncoderCaps {
            gpu_name: "NVIDIA GeForce RTX 4060".into(),
            codecs: vec![Codec::H264_420, Codec::H264_444, Codec::Hevc444, Codec::Av1_420],
        }
    }

    #[test]
    fn prefere_hevc444_pour_le_texte() {
        // Pour du partage d'écran (texte, code), la couleur pleine résolution
        // prime sur l'efficacité de compression.
        assert_eq!(pick_best(&caps_rtx4060(), true), Some(Codec::Hevc444));
    }

    #[test]
    fn prefere_av1_pour_la_video() {
        // Sans exigence de texte net, AV1 gagne : ~40 % de débit en moins.
        assert_eq!(pick_best(&caps_rtx4060(), false), Some(Codec::Av1_420));
    }

    #[test]
    fn retombe_sur_h264_444_si_hevc_absent() {
        let caps = EncoderCaps {
            gpu_name: "GTX 970".into(),
            codecs: vec![Codec::H264_420, Codec::H264_444],
        };
        assert_eq!(pick_best(&caps, true), Some(Codec::H264_444));
    }

    #[test]
    fn retombe_sur_420_si_aucun_444() {
        let caps = EncoderCaps {
            gpu_name: "vieux GPU".into(),
            codecs: vec![Codec::H264_420],
        };
        assert_eq!(pick_best(&caps, true), Some(Codec::H264_420));
    }

    #[test]
    fn aucun_codec_disponible() {
        let caps = EncoderCaps { gpu_name: "aucun".into(), codecs: vec![] };
        assert_eq!(pick_best(&caps, true), None);
    }
}
```

- [ ] **Step 4: Lancer le test pour vérifier qu'il échoue**

Run: `cargo test -p sky-encode`
Expected: FAIL — `cannot find type EncoderCaps in this scope`

- [ ] **Step 5: Écrire l'implémentation minimale**

Ajouter **au-dessus** du bloc de test dans `caps.rs` :

```rust
/// Les combinaisons codec + sous-échantillonnage que NVENC sait produire.
///
/// Note matérielle : NVENC ne fait pas de 4:4:4 en AV1, même sur Ada (RTX 40).
/// Le 4:4:4 — indispensable pour que le texte reste lisible — n'existe qu'en
/// H.264 et HEVC. C'est l'arbitrage central du partage d'écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264_420,
    H264_444,
    Hevc444,
    Av1_420,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264_420 => "H.264 4:2:0",
            Codec::H264_444 => "H.264 4:4:4",
            Codec::Hevc444 => "HEVC 4:4:4",
            Codec::Av1_420 => "AV1 4:2:0",
        }
    }

    pub fn is_444(self) -> bool {
        matches!(self, Codec::H264_444 | Codec::Hevc444)
    }
}

#[derive(Debug, Clone)]
pub struct EncoderCaps {
    pub gpu_name: String,
    pub codecs: Vec<Codec>,
}

/// Choisit le meilleur codec disponible.
///
/// `prefer_text` = true privilégie la netteté du texte (4:4:4) sur l'efficacité.
/// Ordre : HEVC 4:4:4 > H.264 4:4:4 > AV1 4:2:0 > H.264 4:2:0
/// Sans exigence de texte : AV1 4:2:0 > HEVC 4:4:4 > H.264 4:4:4 > H.264 4:2:0
pub fn pick_best(caps: &EncoderCaps, prefer_text: bool) -> Option<Codec> {
    let ordre: &[Codec] = if prefer_text {
        &[Codec::Hevc444, Codec::H264_444, Codec::Av1_420, Codec::H264_420]
    } else {
        &[Codec::Av1_420, Codec::Hevc444, Codec::H264_444, Codec::H264_420]
    };
    ordre.iter().copied().find(|c| caps.codecs.contains(c))
}
```

- [ ] **Step 6: Lancer le test pour vérifier qu'il passe**

Run: `cargo test -p sky-encode`
Expected: PASS — 5 tests

- [ ] **Step 7: Implémenter la détection matérielle réelle**

Ajouter dans `caps.rs`, sous `pick_best` :

```rust
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    NV_ENC_BUFFER_FORMAT, NV_ENC_CODEC_AV1_GUID, NV_ENC_CODEC_H264_GUID, NV_ENC_CODEC_HEVC_GUID,
};

/// Interroge NVENC pour savoir ce que la carte sait réellement encoder.
///
/// On ne se fie pas au nom du GPU : les capacités dépendent aussi du driver.
/// On demande donc à NVENC lui-même, codec par codec, quels formats d'entrée
/// il accepte — c'est la seule source de vérité.
pub fn probe_hardware() -> anyhow::Result<EncoderCaps> {
    use nvidia_video_codec_sdk::safe::Encoder;

    let cuda = cudarc::driver::CudaContext::new(0)
        .map_err(|e| anyhow::anyhow!("aucun GPU NVIDIA utilisable : {e}"))?;
    let gpu_name = cuda.name().unwrap_or_else(|_| "GPU NVIDIA".into());

    let encoder = Encoder::initialize_with_cuda(cuda)?;
    let encode_guids = encoder.get_encode_guids()?;

    let mut codecs = Vec::new();
    for guid in encode_guids {
        let formats = encoder.get_supported_input_formats(guid)?;
        let a_444 = formats.iter().any(|f| {
            matches!(
                *f,
                NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_YUV444
                    | NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_YUV444_10BIT
            )
        });

        if guid == NV_ENC_CODEC_H264_GUID {
            codecs.push(Codec::H264_420);
            if a_444 {
                codecs.push(Codec::H264_444);
            }
        } else if guid == NV_ENC_CODEC_HEVC_GUID && a_444 {
            codecs.push(Codec::Hevc444);
        } else if guid == NV_ENC_CODEC_AV1_GUID {
            codecs.push(Codec::Av1_420);
        }
    }

    Ok(EncoderCaps { gpu_name, codecs })
}
```

Ajouter à `spike/crates/sky-encode/Cargo.toml` :

```toml
cudarc = { version = "0.17", features = ["cuda-version-from-build-system"] }
```

Créer `spike/crates/sky-encode/src/lib.rs` :

```rust
pub mod caps;

pub use caps::{pick_best, probe_hardware, Codec, EncoderCaps};
```

- [ ] **Step 8: Câbler la sous-commande `sky-probe hw`**

`spike/crates/sky-probe/Cargo.toml` :

```toml
[package]
name = "sky-probe"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true
sky-encode = { path = "../sky-encode" }
clap = { version = "4", features = ["derive"] }
```

`spike/crates/sky-probe/src/main.rs` :

```rust
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
```

`spike/crates/sky-probe/src/cmd_hw.rs` :

```rust
use sky_encode::{pick_best, probe_hardware};

pub fn run() -> anyhow::Result<()> {
    let caps = probe_hardware()?;

    println!("GPU        : {}", caps.gpu_name);
    println!("Codecs     :");
    for c in &caps.codecs {
        println!("  - {} {}", c.label(), if c.is_444() { "(texte net)" } else { "" });
    }

    match pick_best(&caps, true) {
        Some(c) => println!("\nChoix partage d'écran : {}", c.label()),
        None => println!("\nAucun encodeur matériel utilisable."),
    }
    match pick_best(&caps, false) {
        Some(c) => println!("Choix vidéo            : {}", c.label()),
        None => {}
    }
    Ok(())
}
```

- [ ] **Step 9: Exécuter sur la machine de référence**

Run: `cargo run -p sky-probe -- hw`
Expected: la RTX 4060 est détectée et la liste contient au minimum `H.264 4:2:0`, `H.264 4:4:4`, `HEVC 4:4:4`, `AV1 4:2:0`. Le choix partage d'écran doit être `HEVC 4:4:4`.

**Si `cudarc` refuse de compiler faute de CUDA Toolkit installé** : c'est le premier point de bascule. Remplacer la détection par une énumération directe via `sky-probe` sans contexte CUDA n'est pas possible — NVENC exige un device. Repli : installer le CUDA Toolkit (gratuit, ~3 Go) OU passer à la voie 2 décrite en Tâche 3. Time-box de cette étape : **2 heures**.

- [ ] **Step 10: Commit**

```bash
git add spike/
git commit -m "spike: detection materielle NVENC et selection de codec

Constat matériel : NVENC ne fait pas de 4:4:4 en AV1, même sur Ada.
Le 4:4:4 n'existe qu'en H.264 et HEVC — arbitrage central du partage d'écran.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 2: Capture d'écran GPU (Q1)

**Files:**
- Create: `spike/crates/sky-capture/Cargo.toml`
- Create: `spike/crates/sky-capture/src/lib.rs`
- Create: `spike/crates/sky-capture/src/wgc.rs`
- Create: `spike/crates/sky-probe/src/cmd_capture.rs`
- Modify: `spike/crates/sky-probe/src/main.rs`

**Interfaces:**
- Consumes: rien de la Tâche 1
- Produces:
  - `sky_capture::CapturedFrame { pub texture: ID3D11Texture2D, pub width: u32, pub height: u32, pub captured_at: Instant }`
  - `sky_capture::CaptureStats { pub frames: u64, pub dropped: u64, pub avg_fps: f32 }`
  - `sky_capture::wgc::WgcCapture::new(monitor_index: usize) -> anyhow::Result<Self>`
  - `WgcCapture::next_frame(&mut self, timeout: Duration) -> anyhow::Result<Option<CapturedFrame>>`
  - `WgcCapture::stats(&self) -> CaptureStats`
  - `WgcCapture::d3d_device(&self) -> &ID3D11Device` — nécessaire à la Tâche 3

- [ ] **Step 1: Déclarer les dépendances**

`spike/crates/sky-capture/Cargo.toml` :

```toml
[package]
name = "sky-capture"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true

[dependencies.windows]
version = "0.62"
features = [
    "Foundation",
    "Graphics_Capture",
    "Graphics_DirectX",
    "Graphics_DirectX_Direct3D11",
    "Win32_Foundation",
    "Win32_Graphics_Direct3D",
    "Win32_Graphics_Direct3D11",
    "Win32_Graphics_Dxgi",
    "Win32_Graphics_Gdi",
    "Win32_System_WinRT",
    "Win32_System_WinRT_Direct3D11",
    "Win32_System_WinRT_Graphics_Capture",
]
```

- [ ] **Step 2: Définir les types publics**

`spike/crates/sky-capture/src/lib.rs` :

```rust
pub mod wgc;

use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

/// Une image capturée. La texture vit sur le GPU et n'est jamais copiée en RAM.
pub struct CapturedFrame {
    pub texture: ID3D11Texture2D,
    pub width: u32,
    pub height: u32,
    pub captured_at: Instant,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureStats {
    pub frames: u64,
    pub dropped: u64,
    pub avg_fps: f32,
}
```

- [ ] **Step 3: Implémenter la capture WGC**

`spike/crates/sky-capture/src/wgc.rs` :

```rust
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use windows::core::Interface;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, HDC, HMONITOR, MONITORENUMPROC,
};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use crate::{CapturedFrame, CaptureStats};

pub struct WgcCapture {
    _item: GraphicsCaptureItem,
    session: GraphicsCaptureSession,
    frame_pool: Direct3D11CaptureFramePool,
    d3d_device: ID3D11Device,
    started: Instant,
    frames: u64,
    dropped: u64,
    width: u32,
    height: u32,
}

impl WgcCapture {
    pub fn new(monitor_index: usize) -> anyhow::Result<Self> {
        let hmonitor = enumerate_monitors()?
            .into_iter()
            .nth(monitor_index)
            .ok_or_else(|| anyhow!("écran {monitor_index} introuvable"))?;

        // 1. Device D3D11 matériel, avec support BGRA exigé par WGC.
        let mut d3d_device: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                None,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                None,
            )
        }
        .context("création du device D3D11")?;
        let d3d_device = d3d_device.ok_or_else(|| anyhow!("device D3D11 nul"))?;

        // 2. Pont D3D11 -> WinRT, exigé par le frame pool.
        let dxgi: IDXGIDevice = d3d_device.cast()?;
        let winrt_device = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .context("pont WinRT")?;
        let winrt_device: windows::Graphics::DirectX::Direct3D11::IDirect3DDevice =
            winrt_device.cast()?;

        // 3. L'item de capture, obtenu via l'interface d'interop COM.
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = unsafe { interop.CreateForMonitor(hmonitor) }
            .context("CreateForMonitor")?;

        let size = item.Size()?;
        let (width, height) = (size.Width as u32, size.Height as u32);

        // 4. Le frame pool. 2 tampons suffisent et minimisent la latence :
        //    davantage ne ferait qu'accumuler des images périmées.
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )
        .context("création du frame pool")?;

        let session = frame_pool.CreateCaptureSession(&item)?;

        // Windows 11 build >= 22000 : supprime la bordure jaune de capture.
        let _ = session.SetIsBorderRequired(false);
        // Le curseur est transmis séparément (spec §6.3), pas incrusté ici.
        let _ = session.SetIsCursorCaptureEnabled(false);

        session.StartCapture().context("StartCapture")?;

        Ok(Self {
            _item: item,
            session,
            frame_pool,
            d3d_device,
            started: Instant::now(),
            frames: 0,
            dropped: 0,
            width,
            height,
        })
    }

    pub fn d3d_device(&self) -> &ID3D11Device {
        &self.d3d_device
    }

    /// Récupère l'image suivante. Renvoie Ok(None) si aucune image n'est prête
    /// dans le délai imparti — cas normal quand l'écran est statique.
    pub fn next_frame(&mut self, timeout: Duration) -> anyhow::Result<Option<CapturedFrame>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(frame) = self.frame_pool.TryGetNextFrame() {
                let surface = frame.Surface()?;
                let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
                let texture: ID3D11Texture2D = unsafe { access.GetInterface() }?;

                self.frames += 1;
                return Ok(Some(CapturedFrame {
                    texture,
                    width: self.width,
                    height: self.height,
                    captured_at: Instant::now(),
                }));
            }
            if Instant::now() >= deadline {
                self.dropped += 1;
                return Ok(None);
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    }

    pub fn stats(&self) -> CaptureStats {
        let secs = self.started.elapsed().as_secs_f32().max(0.001);
        CaptureStats {
            frames: self.frames,
            dropped: self.dropped,
            avg_fps: self.frames as f32 / secs,
        }
    }
}

impl Drop for WgcCapture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.frame_pool.Close();
    }
}

fn enumerate_monitors() -> anyhow::Result<Vec<HMONITOR>> {
    unsafe extern "system" fn cb(
        hmon: HMONITOR,
        _hdc: HDC,
        _rect: *mut windows::Win32::Foundation::RECT,
        data: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::BOOL {
        let out = &mut *(data.0 as *mut Vec<HMONITOR>);
        out.push(hmon);
        true.into()
    }

    let mut out: Vec<HMONITOR> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(cb as MONITORENUMPROC),
            windows::Win32::Foundation::LPARAM(&mut out as *mut _ as isize),
        )
        .ok()
        .context("EnumDisplayMonitors")?;
    }
    Ok(out)
}
```

- [ ] **Step 4: Compiler**

Run: `cargo build -p sky-capture`
Expected: compilation réussie. Les erreurs de nom de feature `windows` sont le piège habituel — vérifier que chaque `use` a sa feature déclarée dans `Cargo.toml`.

- [ ] **Step 5: Écrire la sous-commande de mesure**

`spike/crates/sky-probe/src/cmd_capture.rs` :

```rust
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
```

Modifier `spike/crates/sky-probe/src/main.rs` — ajouter `mod cmd_capture;`, la variante et le bras :

```rust
    /// Mesure la capture d'écran (Q1)
    Capture {
        #[arg(long, default_value_t = 30)]
        seconds: u64,
        #[arg(long, default_value_t = 0)]
        monitor: usize,
    },
```

```rust
        Cmd::Capture { seconds, monitor } => cmd_capture::run(seconds, monitor),
```

Ajouter à `spike/crates/sky-probe/Cargo.toml` : `sky-capture = { path = "../sky-capture" }`

- [ ] **Step 6: Mesurer**

Run: `cargo run --release -p sky-probe -- capture --seconds 30`

Pendant les 30 secondes, déplacer des fenêtres et faire défiler une page pour produire du mouvement réel.

Expected: FPS moyen ≥ 59.0, verdict SUCCÈS.

Relever en parallèle l'usage CPU du processus dans le Gestionnaire des tâches (onglet Détails, colonne UC) et le noter — il servira à Q4.

- [ ] **Step 7: Commit**

```bash
git add spike/
git commit -m "spike: capture Windows.Graphics.Capture sans copie CPU (Q1)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 3: Encodage NVENC depuis une texture D3D11 (Q2)

C'est la tâche la plus risquée du spike. Elle a un time-box et un repli explicites.

**Files:**
- Create: `spike/crates/sky-encode/src/nvenc.rs`
- Modify: `spike/crates/sky-encode/src/lib.rs`
- Modify: `spike/crates/sky-encode/Cargo.toml`
- Create: `spike/crates/sky-probe/src/cmd_encode.rs`
- Modify: `spike/crates/sky-probe/src/main.rs`

**Interfaces:**
- Consumes: `sky_capture::CapturedFrame`, `WgcCapture::d3d_device()`, `sky_encode::Codec`
- Produces:
  - `sky_encode::EncodedPacket { pub data: Vec<u8>, pub is_keyframe: bool, pub encode_us: u64 }`
  - `sky_encode::nvenc::NvencEncoder::new(device: &ID3D11Device, codec: Codec, w: u32, h: u32, fps: u32, bitrate_bps: u32) -> anyhow::Result<Self>`
  - `NvencEncoder::encode(&mut self, frame: &CapturedFrame) -> anyhow::Result<Option<EncodedPacket>>`

- [ ] **Step 1: Comprendre le chemin D3D11 → NVENC avant d'écrire du code**

Le point clé, et la raison pour laquelle `initialize_with_cuda` ne suffit pas : la crate `nvidia-video-codec-sdk` 0.4 n'expose pas d'initialisation D3D11 dans son module `safe`. Il faut passer par `sys`.

Le chemin exact, celui qu'utilise OBS :

```
NvEncOpenEncodeSessionEx( device = ID3D11Device*, deviceType = NV_ENC_DEVICE_TYPE_DIRECTX )
    └─> NvEncRegisterResource( resourceType = NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX,
                               resourceToRegister = ID3D11Texture2D* )
            └─> NvEncMapInputResource(...)
                    └─> NvEncEncodePicture(...)
                            └─> NvEncLockBitstream(...) -> octets
```

Aucune copie CPU n'intervient : NVENC lit directement la texture en mémoire vidéo.

Lire la référence avant de coder : <https://docs.nvidia.com/video-technologies/video-codec-sdk/13.0/nvenc-video-encoder-api-prog-guide/index.html>, section « Encoding with DirectX ».

- [ ] **Step 2: Vérifier que les symboles nécessaires existent dans la crate**

Run:
```bash
cargo doc -p nvidia-video-codec-sdk --no-deps --open
```

Chercher dans `sys::nvEncodeAPI` la présence de : `NV_ENC_DEVICE_TYPE`, `NV_ENC_INPUT_RESOURCE_TYPE`, `NV_ENC_REGISTER_RESOURCE`, `NV_ENC_MAP_INPUT_RESOURCE`, `NV_ENC_PIC_PARAMS`, `NV_ENC_LOCK_BITSTREAM`, `NV_ENCODE_API_FUNCTION_LIST`.

Noter les noms exacts observés dans `spike/docs/api-nvenc.md` — la génération de bindings peut préfixer ou suffixer les variantes d'enum.

**Time-box de cette étape : 1 heure.** Si les symboles sont absents, passer directement au Step 8 (repli).

- [ ] **Step 3: Écrire l'encodeur**

`spike/crates/sky-encode/src/nvenc.rs` — squelette structurel à compléter avec les noms exacts relevés au Step 2 :

```rust
use std::time::Instant;

use anyhow::{anyhow, Context};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};

use crate::{Codec, EncodedPacket};

pub struct NvencEncoder {
    encoder: *mut std::ffi::c_void,
    fns: nvidia_video_codec_sdk::sys::nvEncodeAPI::NV_ENCODE_API_FUNCTION_LIST,
    bitstream: *mut std::ffi::c_void,
    width: u32,
    height: u32,
    frame_index: u64,
}

// NVENC est utilisable depuis un seul thread à la fois ; on ne dérive pas Send.

impl NvencEncoder {
    pub fn new(
        device: &ID3D11Device,
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_bps: u32,
    ) -> anyhow::Result<Self> {
        // 1. Charger la table de fonctions NVENC (NvEncodeAPICreateInstance).
        // 2. NvEncOpenEncodeSessionEx avec :
        //      device      = device.as_raw()
        //      deviceType  = NV_ENC_DEVICE_TYPE_DIRECTX
        //      apiVersion  = NVENCAPI_VERSION
        // 3. NvEncInitializeEncoder avec :
        //      encodeGUID    = selon `codec`
        //      presetGUID    = NV_ENC_PRESET_P4_GUID
        //      tuningInfo    = NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY
        //      rateControl   = NV_ENC_PARAMS_RC_CBR
        //      averageBitRate= bitrate_bps
        //      vbvBufferSize = bitrate_bps / fps    <- 1 image de tampon : anti-pics
        //      frameIntervalP= 1                    <- aucune image bidirectionnelle
        //      idrPeriod     = NVENC_INFINITE_GOPLENGTH
        //      intraRefresh  = activé, période = fps * 2
        //      chromaFormatIDC = 3 si codec.is_444() sinon 1
        // 4. NvEncCreateBitstreamBuffer -> self.bitstream
        todo!("compléter avec les noms exacts relevés au Step 2")
    }

    pub fn encode(
        &mut self,
        frame: &sky_capture::CapturedFrame,
    ) -> anyhow::Result<Option<EncodedPacket>> {
        let t0 = Instant::now();
        // 1. NvEncRegisterResource(NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX, frame.texture)
        // 2. NvEncMapInputResource -> mappedResource
        // 3. NvEncEncodePicture { inputBuffer: mappedResource, outputBitstream: self.bitstream }
        // 4. NvEncLockBitstream -> copier les octets
        // 5. NvEncUnlockBitstream + NvEncUnmapInputResource + NvEncUnregisterResource
        todo!("compléter avec les noms exacts relevés au Step 2")
    }
}
```

> **Note pour l'exécutant :** les deux `todo!()` ci-dessus ne sont pas des placeholders de complaisance — ce sont les deux seuls endroits du plan dont l'API exacte ne peut pas être connue sans lire la documentation générée de la crate installée. La séquence d'appels, leur ordre et tous les paramètres non triviaux sont donnés en commentaire. Le Step 2 fournit les noms manquants.

**Time-box de cette tâche : 1 jour.**

- [ ] **Step 4: Ajouter la dépendance croisée**

Dans `spike/crates/sky-encode/Cargo.toml` :

```toml
sky-capture = { path = "../sky-capture" }
windows = { version = "0.62", features = ["Win32_Graphics_Direct3D11"] }
```

Dans `spike/crates/sky-encode/src/lib.rs`, ajouter :

```rust
pub mod nvenc;

/// Un paquet encodé, prêt à partir sur le réseau.
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub is_keyframe: bool,
    pub encode_us: u64,
}
```

- [ ] **Step 5: Écrire la sous-commande de mesure**

`spike/crates/sky-probe/src/cmd_encode.rs` :

```rust
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use sky_capture::wgc::WgcCapture;
use sky_encode::{nvenc::NvencEncoder, Codec};

pub fn run(seconds: u64, codec: Codec, bitrate_mbps: u32, sortie: &str) -> anyhow::Result<()> {
    let mut cap = WgcCapture::new(0)?;
    let premiere = loop {
        if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
            break f;
        }
    };

    let mut enc = NvencEncoder::new(
        cap.d3d_device(),
        codec,
        premiere.width,
        premiere.height,
        60,
        bitrate_mbps * 1_000_000,
    )?;

    let mut fichier = File::create(sortie)?;
    let mut octets = 0u64;
    let mut images = 0u64;
    let mut temps_encodage = Vec::new();

    let fin = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < fin {
        if let Some(frame) = cap.next_frame(Duration::from_millis(50))? {
            if let Some(pkt) = enc.encode(&frame)? {
                fichier.write_all(&pkt.data)?;
                octets += pkt.data.len() as u64;
                images += 1;
                temps_encodage.push(pkt.encode_us);
            }
        }
    }

    temps_encodage.sort_unstable();
    let p50 = temps_encodage.get(temps_encodage.len() / 2).copied().unwrap_or(0);
    let p99 = temps_encodage
        .get(temps_encodage.len() * 99 / 100)
        .copied()
        .unwrap_or(0);

    println!("\n--- Q2 : encodage {} ---", codec.label());
    println!("Images encodées   : {images}");
    println!("Débit réel        : {:.1} Mbps", octets as f64 * 8.0 / seconds as f64 / 1e6);
    println!("Encodage médian   : {:.2} ms", p50 as f64 / 1000.0);
    println!("Encodage p99      : {:.2} ms", p99 as f64 / 1000.0);
    println!("Fichier           : {sortie}");
    println!(
        "Verdict           : {}",
        if images > 0 && p99 < 16_000 { "SUCCÈS" } else { "ÉCHEC" }
    );
    Ok(())
}
```

- [ ] **Step 6: Mesurer**

Run: `cargo run --release -p sky-probe -- encode --seconds 20 --codec hevc444 --bitrate-mbps 30 --out test.h265`
Expected: images encodées > 1100, p99 < 16 ms, fichier non vide.

- [ ] **Step 7: Vérifier que le bitstream est valide**

Run: `ffplay test.h265`
Expected: la vidéo se lit et montre l'écran capturé. Si `ffplay` est absent, VLC lit aussi les fichiers Annex B bruts.

**C'est la validation de Q2.** Si la vidéo se lit, la chaîne capture → encodage fonctionne.

- [ ] **Step 8: Repli si le time-box est dépassé**

Si l'étape 3 dépasse une journée, basculer sur `ffmpeg-next` avec l'encodeur `hevc_nvenc` et l'accélération `d3d11va`, qui gère nativement l'entrée D3D11 en zéro-copie :

```toml
ffmpeg-next = "7"
```

Configuration équivalente : `preset=p4`, `tune=ull`, `rc=cbr`, `profile=rext` (pour le 4:4:4), `bf=0`, `intra-refresh=1`.

Coût : une dépendance FFmpeg à installer (vcpkg) et un binaire final plus lourd. Acceptable pour un spike ; à réévaluer au jalon 2.

**Noter la bascule dans le rapport** — c'est une information de faisabilité en soi.

- [ ] **Step 9: Commit**

```bash
git add spike/
git commit -m "spike: encodage NVENC depuis texture D3D11 (Q2)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 4: Comparatif des codecs (Q3)

**Files:**
- Create: `spike/crates/sky-probe/src/cmd_codecs.rs`
- Modify: `spike/crates/sky-probe/src/main.rs`
- Create: `spike/docs/comparatif-codecs.md`

**Interfaces:**
- Consumes: `NvencEncoder`, `Codec`, `WgcCapture`
- Produces: `spike/docs/comparatif-codecs.md` et quatre fichiers vidéo

- [ ] **Step 1: Préparer une scène de test reproductible**

Ouvrir côte à côte, sur l'écran capturé :
- un éditeur de texte affichant du code coloré en police 12 px (le cas le plus exigeant pour le 4:2:0)
- une vidéo YouTube en lecture (le cas favorable à AV1)

Cette scène doit rester identique entre les quatre encodages pour que la comparaison ait un sens.

- [ ] **Step 2: Écrire la sous-commande**

`spike/crates/sky-probe/src/cmd_codecs.rs` :

```rust
use sky_encode::Codec;

/// Encode la même scène avec les quatre combinaisons, au MÊME débit,
/// pour que la comparaison porte sur la qualité et non sur la quantité.
pub fn run(seconds: u64, bitrate_mbps: u32) -> anyhow::Result<()> {
    let combinaisons = [
        (Codec::H264_420, "cmp-h264-420.h264"),
        (Codec::H264_444, "cmp-h264-444.h264"),
        (Codec::Hevc444, "cmp-hevc-444.h265"),
        (Codec::Av1_420, "cmp-av1-420.ivf"),
    ];

    for (codec, sortie) in combinaisons {
        println!("\n=== {} -> {} ===", codec.label(), sortie);
        println!("Prépare la scène identique, puis appuie sur Entrée.");
        let mut _l = String::new();
        std::io::stdin().read_line(&mut _l)?;
        crate::cmd_encode::run(seconds, codec, bitrate_mbps, sortie)?;
    }

    println!("\nExtrais une image de chaque fichier pour comparer le texte :");
    for (_, sortie) in combinaisons {
        println!("  ffmpeg -i {sortie} -vf \"select=eq(n\\,120)\" -vframes 1 {sortie}.png");
    }
    Ok(())
}
```

- [ ] **Step 3: Produire les quatre encodages**

Run: `cargo run --release -p sky-probe -- codecs --seconds 15 --bitrate-mbps 10`

Un débit volontairement bas (10 Mbps) : c'est là que les différences de sous-échantillonnage deviennent visibles. À 50 Mbps, tout est beau et la comparaison n'apprend rien.

- [ ] **Step 4: Extraire et comparer les images**

Exécuter les quatre commandes `ffmpeg` affichées, puis ouvrir les PNG à 100 % de zoom et comparer la zone de texte.

- [ ] **Step 5: Rédiger le comparatif**

Créer `spike/docs/comparatif-codecs.md` avec, pour chaque codec : lisibilité du texte (lisible / flou / illisible), présence de franges colorées sur le texte, temps d'encodage p99, et une recommandation argumentée.

- [ ] **Step 6: Commit**

```bash
git add spike/
git commit -m "spike: comparatif des 4 codecs a debit egal (Q3)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 5: Scellage des adresses réseau

Logique pure, sans I/O : TDD strict s'applique intégralement.

**Files:**
- Create: `spike/crates/sky-crypto/Cargo.toml`
- Create: `spike/crates/sky-crypto/src/lib.rs`

**Interfaces:**
- Consumes: rien
- Produces:
  - `sky_crypto::Identity::generate() -> Identity`
  - `Identity::public_key(&self) -> [u8; 32]`
  - `Identity::seal(&self, destinataire: &[u8; 32], message: &[u8]) -> Vec<u8>`
  - `Identity::open(&self, scelle: &[u8]) -> anyhow::Result<Vec<u8>>`

- [ ] **Step 1: Déclarer les dépendances**

`spike/crates/sky-crypto/Cargo.toml` :

```toml
[package]
name = "sky-crypto"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true
crypto_box = { version = "0.9", features = ["seal"] }
rand_core = { version = "0.6", features = ["getrandom"] }
```

- [ ] **Step 2: Écrire les tests (ils doivent échouer)**

`spike/crates/sky-crypto/src/lib.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");
        let ouvert = bob.open(&scelle).unwrap();

        assert_eq!(ouvert, b"192.0.2.1:51234");
    }

    #[test]
    fn un_tiers_ne_peut_pas_ouvrir() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let mallory = Identity::generate();

        let scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");

        // C'est la garantie centrale : Vercel stocke ce blob et ne peut rien en faire.
        assert!(mallory.open(&scelle).is_err());
    }

    #[test]
    fn message_altere_rejete() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let mut scelle = alice.seal(&bob.public_key(), b"192.0.2.1:51234");
        let dernier = scelle.len() - 1;
        scelle[dernier] ^= 0xFF;

        assert!(bob.open(&scelle).is_err());
    }

    #[test]
    fn deux_scellages_du_meme_message_different() {
        let alice = Identity::generate();
        let bob = Identity::generate();

        let a = alice.seal(&bob.public_key(), b"identique");
        let b = alice.seal(&bob.public_key(), b"identique");

        // Sans cela, un observateur repérerait les connexions répétées
        // vers le même pair rien qu'en comparant les blobs.
        assert_ne!(a, b);
    }

    #[test]
    fn surcout_borne() {
        let alice = Identity::generate();
        let bob = Identity::generate();
        let scelle = alice.seal(&bob.public_key(), b"x");
        // 32 octets de clé éphémère + 16 de tag d'authentification.
        assert_eq!(scelle.len(), 1 + 48);
    }
}
```

- [ ] **Step 3: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p sky-crypto`
Expected: FAIL — `cannot find type Identity in this scope`

- [ ] **Step 4: Implémenter**

Ajouter au-dessus du bloc de test :

```rust
use anyhow::anyhow;
use crypto_box::{
    aead::{Aead, OsRng},
    PublicKey, SecretKey,
};

/// L'identité cryptographique d'un appareil.
///
/// La clé privée ne quitte jamais la machine (spec §4.2). Dans le spike elle
/// est éphémère ; au jalon 1 elle ira dans le coffre-fort du système.
pub struct Identity {
    secret: SecretKey,
}

impl Identity {
    pub fn generate() -> Self {
        Self { secret: SecretKey::generate(&mut OsRng) }
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.secret.public_key().to_bytes()
    }

    /// Scelle un message pour un destinataire.
    ///
    /// Utilise le mode « sealed box » : une paire de clés éphémère est générée
    /// à chaque appel, ce qui rend deux scellages du même message indistinguables
    /// et n'exige pas que le destinataire connaisse l'expéditeur.
    pub fn seal(&self, destinataire: &[u8; 32], message: &[u8]) -> Vec<u8> {
        let pk = PublicKey::from(*destinataire);
        crypto_box::seal(&mut OsRng, &pk, message).expect("scellage")
    }

    pub fn open(&self, scelle: &[u8]) -> anyhow::Result<Vec<u8>> {
        crypto_box::seal_open(&self.secret, scelle)
            .map_err(|_| anyhow!("descellage impossible : mauvaise clé ou message altéré"))
    }
}
```

- [ ] **Step 5: Lancer les tests**

Run: `cargo test -p sky-crypto`
Expected: PASS — 5 tests

Si `surcout_borne` échoue, ajuster la constante à la valeur réellement observée et documenter pourquoi dans un commentaire — le format exact dépend de la version de `crypto_box`.

- [ ] **Step 6: Commit**

```bash
git add spike/
git commit -m "spike: scellage des adresses reseau, teste

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 6: Contrôle de congestion à plancher garanti (Q6)

Logique pure, testable sans réseau : TDD strict.

**Files:**
- Create: `spike/crates/sky-net/Cargo.toml`
- Create: `spike/crates/sky-net/src/lib.rs`
- Create: `spike/crates/sky-net/src/pacer.rs`

**Interfaces:**
- Consumes: rien
- Produces:
  - `sky_net::pacer::Pacer::new(plancher_bps: u32, plafond_bps: u32) -> Pacer`
  - `Pacer::on_feedback(&mut self, perte_pct: f32, rtt_ms: u32, ecoule: Duration)`
  - `Pacer::target_bps(&self) -> u32`

- [ ] **Step 1: Déclarer la crate**

`spike/crates/sky-net/Cargo.toml` :

```toml
[package]
name = "sky-net"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
anyhow.workspace = true
str0m = "0.23"
sky-crypto = { path = "../sky-crypto" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
```

- [ ] **Step 2: Écrire les tests (ils doivent échouer)**

`spike/crates/sky-net/src/pacer.rs` :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const TICK: Duration = Duration::from_millis(100);

    fn pacer() -> Pacer {
        // Plancher 8 Mbps, plafond 30 Mbps.
        Pacer::new(8_000_000, 30_000_000)
    }

    #[test]
    fn demarre_au_plancher() {
        assert_eq!(pacer().target_bps(), 8_000_000);
    }

    #[test]
    fn monte_vers_le_plafond_sans_perte() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);
    }

    #[test]
    fn ne_descend_jamais_sous_le_plancher() {
        let mut p = pacer();
        // Perte catastrophique et durable : 50 % pendant 10 secondes.
        for _ in 0..100 {
            p.on_feedback(50.0, 500, TICK);
        }
        // C'est LA différence avec WebRTC standard, qui s'effondrerait ici.
        assert_eq!(p.target_bps(), 8_000_000);
    }

    #[test]
    fn remonte_en_moins_de_deux_secondes() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);

        // Un à-coup bref.
        p.on_feedback(20.0, 300, TICK);
        let apres_chute = p.target_bps();
        assert!(apres_chute < 30_000_000);

        // Le réseau se dégage : retour au plafond en moins de 2 s (20 ticks).
        for _ in 0..20 {
            p.on_feedback(0.0, 20, TICK);
        }
        assert_eq!(p.target_bps(), 30_000_000);
    }

    #[test]
    fn descente_progressive_pas_brutale() {
        let mut p = pacer();
        for _ in 0..100 {
            p.on_feedback(0.0, 20, TICK);
        }
        let avant = p.target_bps();
        p.on_feedback(10.0, 200, TICK);
        let apres = p.target_bps();

        // Une chute d'un seul coup fait pulser l'image de façon très visible.
        // On n'enlève jamais plus de 15 % par tick.
        assert!(apres as f32 >= avant as f32 * 0.85);
        assert!(apres < avant);
    }
}
```

- [ ] **Step 3: Lancer les tests pour vérifier qu'ils échouent**

Run: `cargo test -p sky-net`
Expected: FAIL — `cannot find type Pacer in this scope`

- [ ] **Step 4: Implémenter**

Ajouter au-dessus du bloc de test :

```rust
use std::time::Duration;

/// Contrôle de congestion à plancher garanti.
///
/// La différence essentielle avec l'algorithme de WebRTC : celui-ci accepte de
/// descendre indéfiniment pour préserver la continuité, ce qui produit l'image
/// baveuse de Discord. Ici le débit ne passe jamais sous un plancher choisi par
/// l'utilisateur : en cas de congestion durable, on préfère perdre des images
/// plutôt que de la netteté.
pub struct Pacer {
    plancher_bps: u32,
    plafond_bps: u32,
    cible_bps: u32,
}

impl Pacer {
    /// Part maximale retirée en un seul retour d'information.
    const CHUTE_MAX: f32 = 0.15;
    /// Part ajoutée par tick quand le réseau est sain.
    const MONTEE: f32 = 0.08;
    /// En dessous, la perte est considérée comme du bruit normal.
    const SEUIL_PERTE: f32 = 2.0;
    /// Au-delà, le tampon réseau se remplit : on lève le pied.
    const SEUIL_RTT_MS: u32 = 150;

    pub fn new(plancher_bps: u32, plafond_bps: u32) -> Self {
        Self { plancher_bps, plafond_bps, cible_bps: plancher_bps }
    }

    pub fn target_bps(&self) -> u32 {
        self.cible_bps
    }

    pub fn on_feedback(&mut self, perte_pct: f32, rtt_ms: u32, _ecoule: Duration) {
        let congestionne = perte_pct > Self::SEUIL_PERTE || rtt_ms > Self::SEUIL_RTT_MS;

        let brut = if congestionne {
            // Chute proportionnelle à la sévérité, bornée à CHUTE_MAX.
            let severite = (perte_pct / 100.0).clamp(0.0, 1.0);
            let facteur = 1.0 - (Self::CHUTE_MAX * severite.max(0.3));
            self.cible_bps as f32 * facteur
        } else {
            self.cible_bps as f32 * (1.0 + Self::MONTEE)
        };

        self.cible_bps = (brut as u32).clamp(self.plancher_bps, self.plafond_bps);
    }
}
```

Créer `spike/crates/sky-net/src/lib.rs` :

```rust
pub mod handshake;
pub mod pacer;

pub use pacer::Pacer;
```

- [ ] **Step 5: Lancer les tests**

Run: `cargo test -p sky-net`
Expected: PASS — 5 tests

- [ ] **Step 6: Commit**

```bash
git add spike/
git commit -m "spike: controle de congestion a plancher garanti, teste (Q6)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 7: Connexion pair-à-pair entre deux machines (Q5)

**Files:**
- Create: `spike/crates/sky-net/src/handshake.rs`
- Create: `spike/crates/sky-probe/src/cmd_host.rs`
- Create: `spike/crates/sky-probe/src/cmd_view.rs`
- Create: `spike/README-AMI.md`
- Modify: `spike/crates/sky-probe/src/main.rs`

**Interfaces:**
- Consumes: `sky_crypto::Identity`, `sky_net::Pacer`
- Produces:
  - `sky_net::handshake::Blob { pub public_key: [u8;32], pub sealed_sdp: Vec<u8> }`
  - `Blob::to_text(&self) -> String` (base64, copier-collable)
  - `Blob::from_text(s: &str) -> anyhow::Result<Blob>`
  - `sky_net::PeerLink::host(identity, plancher_bps, plafond_bps) -> anyhow::Result<(PeerLink, String)>`
  - `PeerLink::accept_answer(&mut self, texte: &str) -> anyhow::Result<()>`
  - `PeerLink::viewer(identity, offre_texte: &str) -> anyhow::Result<(PeerLink, String)>`
  - `PeerLink::poll(&mut self) -> anyhow::Result<LinkEvent>` où `LinkEvent = { Connected, Data(Vec<u8>), Idle, Failed(String) }`
  - `PeerLink::send(&mut self, data: &[u8]) -> anyhow::Result<()>`

- [ ] **Step 1: Écrire le blob de signaling**

`spike/crates/sky-net/src/handshake.rs` :

```rust
use anyhow::Context;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

/// Ce qui s'échange entre les deux machines pendant la poignée de main.
///
/// Dans le spike, ce blob se copie-colle à la main dans Discord. Au jalon 1,
/// il transitera par la boîte aux lettres Vercel — sans changer de forme.
#[derive(Serialize, Deserialize)]
pub struct Blob {
    pub public_key: [u8; 32],
    pub sealed_sdp: Vec<u8>,
}

impl Blob {
    pub fn to_text(&self) -> String {
        let json = serde_json::to_vec(self).expect("sérialisation");
        format!("SKY1:{}", STANDARD.encode(json))
    }

    pub fn from_text(s: &str) -> anyhow::Result<Self> {
        let corps = s
            .trim()
            .strip_prefix("SKY1:")
            .context("préfixe SKY1: absent — le bloc a-t-il été copié en entier ?")?;
        let json = STANDARD.decode(corps).context("base64 invalide")?;
        serde_json::from_slice(&json).context("structure invalide")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_texte() {
        let b = Blob { public_key: [7u8; 32], sealed_sdp: vec![1, 2, 3] };
        let t = b.to_text();
        assert!(t.starts_with("SKY1:"));
        let r = Blob::from_text(&t).unwrap();
        assert_eq!(r.public_key, [7u8; 32]);
        assert_eq!(r.sealed_sdp, vec![1, 2, 3]);
    }

    #[test]
    fn tolere_espaces_et_retours_ligne() {
        // Un copier-coller depuis Discord ramène souvent des espaces parasites.
        let b = Blob { public_key: [1u8; 32], sealed_sdp: vec![9] };
        let t = format!("  \n{}\n  ", b.to_text());
        assert!(Blob::from_text(&t).is_ok());
    }

    #[test]
    fn rejette_un_bloc_tronque() {
        assert!(Blob::from_text("SKY1:abc").is_err());
    }
}
```

- [ ] **Step 2: Lancer les tests du blob**

Run: `cargo test -p sky-net handshake`
Expected: PASS — 3 tests

- [ ] **Step 3: Implémenter PeerLink sur str0m**

Ajouter dans `spike/crates/sky-net/src/lib.rs` :

```rust
use std::net::UdpSocket;
use std::time::Instant;

use anyhow::{anyhow, Context};
use str0m::change::SdpOffer;
use str0m::net::{Protocol, Receive};
use str0m::{Candidate, Event, Input, Output, Rtc};

use sky_crypto::Identity;

pub enum LinkEvent {
    Connected,
    Data(Vec<u8>),
    Idle,
    Failed(String),
}

pub struct PeerLink {
    rtc: Rtc,
    socket: UdpSocket,
    identity: Identity,
    peer_key: Option<[u8; 32]>,
    connected: bool,
}

impl PeerLink {
    fn socket_et_candidats(rtc: &mut Rtc) -> anyhow::Result<UdpSocket> {
        let socket = UdpSocket::bind("0.0.0.0:0").context("bind UDP")?;
        socket.set_nonblocking(true)?;
        let local = socket.local_addr()?;

        // Candidat hôte : l'adresse locale. str0m découvrira l'adresse publique
        // via STUN pendant la négociation.
        let c = Candidate::host(local, "udp").map_err(|e| anyhow!("candidat : {e}"))?;
        rtc.add_local_candidate(c);
        Ok(socket)
    }

    /// Côté émetteur : produit l'offre à envoyer au spectateur.
    pub fn host(identity: Identity) -> anyhow::Result<(Self, String)> {
        let mut rtc = Rtc::builder().set_rtp_mode(true).build(Instant::now());
        let socket = Self::socket_et_candidats(&mut rtc)?;

        let mut change = rtc.sdp_api();
        change.add_channel("sky".into());
        let (offer, _pending) = change.apply().ok_or_else(|| anyhow!("aucun changement SDP"))?;

        let sdp_texte = offer.to_sdp_string();
        let blob = handshake::Blob {
            public_key: identity.public_key(),
            // Dans le spike l'offre n'est pas scellée : le destinataire n'est pas
            // encore connu. C'est la réponse qui l'est. Au jalon 1, l'offre sera
            // scellée avec la clé du destinataire, connue via la base.
            sealed_sdp: sdp_texte.into_bytes(),
        };

        Ok((
            Self { rtc, socket, identity, peer_key: None, connected: false },
            blob.to_text(),
        ))
    }

    /// Côté spectateur : consomme l'offre, produit la réponse scellée.
    pub fn viewer(identity: Identity, offre_texte: &str) -> anyhow::Result<(Self, String)> {
        let blob = handshake::Blob::from_text(offre_texte)?;
        let sdp = String::from_utf8(blob.sealed_sdp).context("SDP non UTF-8")?;
        let offer = SdpOffer::from_sdp_string(&sdp).map_err(|e| anyhow!("offre invalide : {e}"))?;

        let mut rtc = Rtc::builder().set_rtp_mode(true).build(Instant::now());
        let socket = Self::socket_et_candidats(&mut rtc)?;

        let answer = rtc.sdp_api().accept_offer(offer).map_err(|e| anyhow!("{e}"))?;

        // La réponse contient nos adresses : elle est scellée avec la clé de l'hôte.
        let sealed = identity.seal(&blob.public_key, answer.to_sdp_string().as_bytes());
        let reponse = handshake::Blob { public_key: identity.public_key(), sealed_sdp: sealed };

        Ok((
            Self {
                rtc,
                socket,
                identity,
                peer_key: Some(blob.public_key),
                connected: false,
            },
            reponse.to_text(),
        ))
    }

    /// Côté émetteur : intègre la réponse du spectateur.
    pub fn accept_answer(&mut self, texte: &str) -> anyhow::Result<()> {
        let blob = handshake::Blob::from_text(texte)?;
        let sdp = self.identity.open(&blob.sealed_sdp)?;
        let sdp = String::from_utf8(sdp).context("réponse non UTF-8")?;
        let answer = str0m::change::SdpAnswer::from_sdp_string(&sdp)
            .map_err(|e| anyhow!("réponse invalide : {e}"))?;

        let pending = self
            .rtc
            .sdp_api()
            .apply()
            .ok_or_else(|| anyhow!("aucune offre en attente"))?
            .1;
        self.rtc
            .sdp_api()
            .accept_answer(pending, answer)
            .map_err(|e| anyhow!("{e}"))?;
        self.peer_key = Some(blob.public_key);
        Ok(())
    }

    pub fn poll(&mut self) -> anyhow::Result<LinkEvent> {
        // 1. Vider les sorties de str0m.
        loop {
            match self.rtc.poll_output().map_err(|e| anyhow!("{e}"))? {
                Output::Timeout(_) => break,
                Output::Transmit(t) => {
                    let _ = self.socket.send_to(&t.contents, t.destination);
                }
                Output::Event(e) => match e {
                    Event::IceConnectionStateChange(s)
                        if format!("{s:?}").contains("Connected") =>
                    {
                        self.connected = true;
                        return Ok(LinkEvent::Connected);
                    }
                    Event::ChannelData(d) => return Ok(LinkEvent::Data(d.data)),
                    _ => {}
                },
            }
        }

        // 2. Injecter ce qui arrive du socket.
        let mut buf = vec![0u8; 2000];
        match self.socket.recv_from(&mut buf) {
            Ok((n, source)) => {
                buf.truncate(n);
                let destination = self.socket.local_addr()?;
                self.rtc
                    .handle_input(Input::Receive(
                        Instant::now(),
                        Receive {
                            proto: Protocol::Udp,
                            source,
                            destination,
                            contents: buf.as_slice().try_into().map_err(|_| anyhow!("paquet"))?,
                        },
                    ))
                    .map_err(|e| anyhow!("{e}"))?;
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Ok(LinkEvent::Failed(e.to_string())),
        }

        self.rtc
            .handle_input(Input::Timeout(Instant::now()))
            .map_err(|e| anyhow!("{e}"))?;

        Ok(LinkEvent::Idle)
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }
}
```

> **Note :** les noms exacts de `Event::IceConnectionStateChange` et `Event::ChannelData` doivent être confirmés contre `cargo doc -p str0m --open`. La comparaison par chaîne sur l'état ICE est un raccourci volontaire de spike ; à remplacer par un `match` sur la variante réelle.

- [ ] **Step 4: Écrire les sous-commandes host et view**

`spike/crates/sky-probe/src/cmd_host.rs` :

```rust
use std::io::Write;
use std::time::{Duration, Instant};

use sky_crypto::Identity;
use sky_net::{LinkEvent, PeerLink};

pub fn run() -> anyhow::Result<()> {
    let (mut link, offre) = PeerLink::host(Identity::generate())?;

    println!("\n=== ÉTAPE 1 : envoie ce bloc à ton correspondant ===\n");
    println!("{offre}\n");
    println!("=== ÉTAPE 2 : colle sa réponse ici puis Entrée ===\n");

    let mut reponse = String::new();
    std::io::stdin().read_line(&mut reponse)?;
    link.accept_answer(&reponse)?;

    println!("\nNégociation en cours...");
    let debut = Instant::now();
    loop {
        match link.poll()? {
            LinkEvent::Connected => {
                println!("CONNECTÉ en {:.1} s", debut.elapsed().as_secs_f32());
                break;
            }
            LinkEvent::Failed(e) => {
                println!("ÉCHEC : {e}");
                return Ok(());
            }
            _ => {}
        }
        if debut.elapsed() > Duration::from_secs(8) {
            // Seuil du spec §5.5 : jamais d'attente indéfinie.
            println!("ÉCHEC : aucune connexion directe en 8 s.");
            println!("Cause probable : NAT strict d'un côté (4G, CGNAT, réseau d'entreprise).");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    // Envoi continu pour mesurer le débit réellement soutenu.
    let charge = vec![0xABu8; 1200];
    let mut envoyes = 0u64;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(30) {
        if link.send(&charge).is_ok() {
            envoyes += charge.len() as u64;
        }
        let _ = link.poll()?;
        std::io::stdout().flush().ok();
    }
    println!(
        "\nDébit soutenu : {:.1} Mbps sur 30 s",
        envoyes as f64 * 8.0 / 30.0 / 1e6
    );
    Ok(())
}
```

`spike/crates/sky-probe/src/cmd_view.rs` : symétrique — lit l'offre sur stdin, affiche la réponse, boucle sur `poll()` en comptant les octets reçus et en affichant le débit toutes les secondes.

- [ ] **Step 5: Écrire le mode d'emploi pour le testeur distant**

Créer `spike/README-AMI.md` :

```markdown
# Test SkyShare — 5 minutes

Salut ! Merci de tester. Aucune installation, aucun compte, rien à configurer.

## Ce que ça fait
Ton PC et le mien essaient de se parler **directement**, sans passer par un
serveur. Le test vérifie si vos deux box Internet laissent passer la connexion.

## Ce qui est envoyé
Rien de personnel. Uniquement l'adresse réseau de ton PC, chiffrée, et
uniquement vers moi. Aucun fichier n'est lu, aucun écran n'est capturé
pendant ce test.

## Marche à suivre
1. Télécharge `sky-probe.exe`
2. Ouvre un terminal dans le dossier (clic droit → « Ouvrir dans le Terminal »)
3. Lance :

       .\sky-probe.exe view

4. Je t'envoie un bloc de texte commençant par `SKY1:` — colle-le, puis Entrée
5. Le programme affiche un bloc en retour : renvoie-le-moi en entier
6. Attends : ça affiche `CONNECTÉ` ou `ÉCHEC` en moins de 10 secondes

Envoie-moi une capture du résultat, quel qu'il soit. Un échec est une
information tout aussi utile qu'un succès.

Windows affichera « Windows a protégé votre ordinateur » : c'est normal,
l'exécutable n'est pas signé. Clique sur « Informations complémentaires »
puis « Exécuter quand même ».
```

- [ ] **Step 6: Produire le binaire autonome**

Run: `cargo build --release -p sky-probe`

Le binaire est dans `spike/target/release/sky-probe.exe`. Vérifier qu'il fonctionne sur une machine sans Rust installé — il ne doit dépendre que des DLL système.

- [ ] **Step 7: Réaliser le test réel**

Envoyer `sky-probe.exe` et `README-AMI.md` au testeur distant. Lancer `sky-probe host` localement et suivre le protocole.

Expected: `CONNECTÉ` en moins de 8 secondes, débit soutenu > 20 Mbps.

**C'est la réponse à Q5, le risque n°1 du projet.**

En cas d'échec, relancer avec le journal détaillé pour identifier quel côté ne perce pas :

```bash
$env:RUST_LOG="str0m=debug"; .\sky-probe.exe host
```

- [ ] **Step 8: Commit**

```bash
git add spike/
git commit -m "spike: connexion P2P entre deux machines via str0m (Q5)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 8: Chaîne complète et mesures de bout en bout (Q4)

**Files:**
- Modify: `spike/crates/sky-probe/src/cmd_host.rs`
- Modify: `spike/crates/sky-probe/src/cmd_view.rs`

**Interfaces:**
- Consumes: tout ce qui précède
- Produces: aucune nouvelle interface — assemblage et mesure

- [ ] **Step 1: Brancher capture → encodage → réseau côté hôte**

Dans `cmd_host.rs`, remplacer l'envoi de charge factice par la vraie chaîne : `WgcCapture::next_frame()` → `NvencEncoder::encode()` → `link.send(&pkt.data)`, en pilotant le débit cible avec `Pacer::target_bps()`.

- [ ] **Step 2: Écrire le flux reçu côté spectateur**

Dans `cmd_view.rs`, écrire les octets reçus dans `recu.h265` et afficher toutes les secondes : débit reçu, images reçues, gigue.

- [ ] **Step 3: Mesurer le CPU de la chaîne complète**

Lancer `sky-probe host` pendant 60 secondes en 1440p60 HEVC 4:4:4 à 30 Mbps. Relever dans le Gestionnaire des tâches (onglet Détails) : UC du processus, et vérifier dans l'onglet Performance que l'encodage GPU (« Video Encode ») est bien actif.

Expected pour Q4 : UC du processus < 5 %, et le graphe « Video Encode » du GPU nettement non nul — c'est la preuve que l'encodage se fait bien sur le matériel et non sur le processeur.

- [ ] **Step 4: Mesurer la latence de bout en bout**

Méthode de référence, celle utilisée pour évaluer Parsec et Moonlight :

1. Ouvrir un chronomètre en millisecondes en plein écran sur la machine émettrice
2. Sur la machine réceptrice, lire `recu.h265` en direct : `ffplay -fflags nobuffer -flags low_delay recu.h265`
3. Photographier les deux écrans côte à côte avec un téléphone
4. La différence entre les deux chronomètres est la latence de bout en bout
5. Répéter 5 fois, retenir la médiane

Noter séparément les composantes déjà mesurées : encodage p50/p99 (Tâche 3) et RTT réseau (Tâche 7). Leur somme doit être cohérente avec la mesure photographique ; un écart important signale un tampon caché.

- [ ] **Step 5: Vérifier le flux décodé**

Ouvrir `recu.h265` et confirmer que l'image est correcte, le texte lisible et qu'il n'y a ni bloc corrompu ni artefact persistant.

- [ ] **Step 6: Commit**

```bash
git add spike/
git commit -m "spike: chaine complete capture->encode->reseau et mesures (Q4)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Task 9: Rapport de faisabilité

C'est le livrable réel du jalon 0. Le code est jetable ; ce document ne l'est pas.

**Files:**
- Create: `spike/docs/rapport-jalon-0.md`
- Modify: `tasks/todo.md`
- Modify: `tasks/lessons.md`

**Interfaces:**
- Consumes: toutes les mesures des tâches 1 à 8
- Produces: la décision go / no-go du jalon 1

- [ ] **Step 1: Rédiger le rapport**

Créer `spike/docs/rapport-jalon-0.md` avec cette structure exacte :

```markdown
# Rapport de faisabilité — Jalon 0

Matériel : RTX 4060, driver 610.74, Windows 11 build 26200
Réseaux testés : [FAI local] ↔ [FAI du testeur distant]
Date : [date des mesures]

## Réponses aux 6 questions

| Q | Question | Seuil | Mesuré | Verdict |
|---|----------|-------|--------|---------|
| Q1 | Capture 1440p60 sans copie CPU | ≥ 59 fps | | |
| Q2 | NVENC accepte une texture D3D11 en 4:4:4 | bitstream valide | | |
| Q3 | Meilleur codec pour le texte | comparatif | | |
| Q4 | CPU de la chaîne complète | < 5 % | | |
| Q5 | Connexion entre deux box | établie < 8 s | | |
| Q6 | Plancher de débit tenu | jamais franchi | | |

## Latence de bout en bout
Encodage p50 / p99 : … ms
RTT réseau : … ms
Mesure photographique (médiane sur 5) : … ms

## Ce qui a changé par rapport au spec
[Écarts constatés entre les hypothèses du document d'architecture et la réalité
mesurée. C'est la section la plus précieuse : elle alimente la révision du spec.]

## Décision
[ ] GO — les paris tiennent, on enchaîne sur le jalon 1
[ ] GO CONDITIONNEL — tient sauf sur [point], à traiter avant le jalon 2
[ ] NO-GO — [quel pari est invalidé, et quelle architecture alternative]

## Code à promouvoir
[Quels modules du spike méritent d'être repris au jalon 1 plutôt que réécrits.]
```

- [ ] **Step 2: Remplir chaque case avec les mesures réelles**

Aucune case vide, aucun « environ ». Une mesure non prise se note « non mesurée » avec la raison — c'est une information honnête, contrairement à une estimation.

- [ ] **Step 3: Mettre à jour le suivi**

Dans `tasks/todo.md`, marquer le jalon 0 selon le verdict et reporter les écarts constatés.

Dans `tasks/lessons.md`, ajouter une ligne par surprise rencontrée, au format `[date] | ce qui a mal tourné | règle pour l'éviter`.

- [ ] **Step 4: Réviser le spec si nécessaire**

Si le rapport contredit une décision du document d'architecture, modifier
`docs/superpowers/specs/2026-08-22-skyshare-architecture-design.md` en conséquence et
mentionner le rapport en justification. Un spec qui ment sur la réalité mesurée est pire
que pas de spec du tout.

- [ ] **Step 5: Commit**

```bash
git add spike/docs/rapport-jalon-0.md tasks/ docs/
git commit -m "spike: rapport de faisabilite du jalon 0

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
```

---

## Self-review du plan

**Couverture du spec.** Le jalon 0 couvre §3 (architecture des modules), §4.2 (identité
cryptographique), §5.1 et §5.5 (rencontre P2P et échec), §6.2 à §6.5 (pipeline vidéo).
Volontairement non couverts, car appartenant aux jalons 1 et suivants : §4.1 (OAuth
Discord), §4.3 et §4.4 (base et amis), §4.5 (synchronisation), §5.3 et §5.4 (modes de
partage et liens), §7 (client de visionnage), §8 (distribution).

**Deux `todo!()` assumés en Tâche 3.** Ce sont les seuls du plan. Ils correspondent aux
appels FFI dont l'API générée ne peut être connue qu'en lisant la documentation de la
crate installée ; la séquence d'appels, l'ordre et tous les paramètres de configuration
sont fournis en commentaire, et le Step 2 de la tâche indique exactement comment obtenir
les noms manquants. Les inventer aurait produit du code qui ne compile pas.

**Cohérence des types.** `Codec`, `EncoderCaps`, `CapturedFrame`, `EncodedPacket`,
`Identity`, `Blob`, `Pacer` et `PeerLink` portent les mêmes noms et les mêmes signatures
partout où ils apparaissent. `WgcCapture::d3d_device()` est déclaré en Tâche 2 et
consommé en Tâche 3.

**Points de bascule explicites.** Tâche 1 Step 9 (CUDA Toolkit, 2 h), Tâche 3 Step 8
(FFmpeg, 1 jour). Chacun a un critère de déclenchement mesurable et un chemin alternatif
décrit, pas un simple « voir plus tard ».
