# Leçons — SkyShare

Format : `[date] | ce qui a mal tourné | règle pour l'éviter`

---

[2026-08-22] | `nvidia-video-codec-sdk` (crate Rust) exige au link les .lib du NVIDIA Video Codec SDK (nvEncodeAPI.lib, nvcuvid.lib) — un package NVIDIA distinct du driver ET du CUDA Toolkit, absent sur cette machine. Sans lui, même `cargo test` sur du code n'utilisant pas NVENC échoue (le build.rs panique dès qu'on déclare la dépendance). | Avant d'ajouter cette crate : vérifier la présence de `nvEncodeAPI.lib`/`nvcuvid.lib` (env `NVIDIA_VIDEO_CODEC_SDK_PATH` ou repertoire CUDA). Absents → activer la feature `ci-check` de la crate (elle force aussi `cudarc` en `dynamic-loading`, évitant `nvcc`/CUDA Toolkit) pour que la logique pure compile et se teste partout ; le lien réel d'un binaire qui appelle NVENC restera bloqué (LNK2019 sur `NvEncodeAPICreateInstance`) tant que le SDK n'est pas installé — décision d'infra à remonter, pas à contourner soi-même.
