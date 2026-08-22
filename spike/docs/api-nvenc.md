# Noms exacts de l'API NVENC — `nvidia-video-codec-sdk` 0.4.0

Relevé dans la crate **telle qu'installée**, pas dans la documentation en
ligne. Source : `~/.cargo/registry/src/index.crates.io-*/nvidia-video-codec-sdk-0.4.0/src/sys/`.

Version d'API : `NVENCAPI_VERSION = 16777228` (majeure 12, mineure 1).

## Où vivent les symboles

| Contenu | Fichier |
|---|---|
| Structures, énumérations, pointeurs de fonction | `sys/windows_sys/nvEncodeAPI.rs` (bindgen 0.65.1) |
| GUID de codec, profil, préréglage | `sys/guid.rs` — bindgen les génère faux, la crate les redéfinit |
| Constantes `*_VER` | `sys/version.rs` — bindgen ne les génère pas |

`guid.rs` et `version.rs` sont réexportés en tête de `nvEncodeAPI.rs`
(`pub use super::super::{guid::*, version::*};`) : **tout s'importe depuis
`nvidia_video_codec_sdk::sys::nvEncodeAPI`**, un seul chemin.

## Convention de nommage

bindgen déclare les types sous leur nom C interne préfixé d'un `_`
(`_NV_ENC_REGISTER_RESOURCE`), puis publie l'alias sans préfixe :

- **structures / unions** : alias `pub type` en tête de fichier
  (`pub type NV_ENC_REGISTER_RESOURCE = _NV_ENC_REGISTER_RESOURCE;`) ;
- **énumérations** : bloc `pub use self::{ _X as X, ... }` en **fin** de
  fichier (ligne ~11439). Un `grep "pub type NV_ENC_DEVICE_TYPE"` ne trouve
  rien — c'est normal, chercher `as NV_ENC_DEVICE_TYPE`.

Exception : `NV_ENC_TUNING_INFO` est déclarée **sans** préfixe `_` et n'a
donc aucun alias.

## Symboles utilisés par `sky-encode`

Tous présents, aucun manquant (le repli du Step 8 n'a pas eu à servir).

| Rôle | Nom exact |
|---|---|
| Type de device | `NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX` |
| Type de ressource | `NV_ENC_INPUT_RESOURCE_TYPE::NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX` |
| Usage de tampon | `NV_ENC_BUFFER_USAGE::NV_ENC_INPUT_IMAGE` |
| Format d'entrée BGRA | `NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_ARGB` |
| Contrôle de débit | `NV_ENC_PARAMS_RC_MODE::NV_ENC_PARAMS_RC_CBR` |
| Structure d'image | `NV_ENC_PIC_STRUCT::NV_ENC_PIC_STRUCT_FRAME` |
| Type d'image | `NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_IDR` / `_I` |
| Réglage | `NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY` |
| Fin de flux | `NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_EOS` |
| GOP infini | `NVENC_INFINITE_GOPLENGTH` (`u32`, pas `i32`) |
| Préréglage | `NV_ENC_PRESET_P4_GUID` |
| Profils 4:4:4 | `NV_ENC_H264_PROFILE_HIGH_444_GUID`, `NV_ENC_HEVC_PROFILE_FREXT_GUID` |

Structures : `NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS`, `NV_ENC_PRESET_CONFIG`,
`NV_ENC_CONFIG`, `NV_ENC_INITIALIZE_PARAMS`, `NV_ENC_CREATE_BITSTREAM_BUFFER`,
`NV_ENC_REGISTER_RESOURCE`, `NV_ENC_MAP_INPUT_RESOURCE`, `NV_ENC_PIC_PARAMS`,
`NV_ENC_LOCK_BITSTREAM` — chacune avec sa constante `*_VER` homonyme.

## Pièges relevés

1. **`chromaFormatIDC` n'a pas le même type selon le codec.**
   En H.264 c'est un `u32` ordinaire (`h264.chromaFormatIDC = 3`).
   En HEVC **et** en AV1 c'est un champ de bits de 2 bits : il faut passer par
   l'accesseur généré `set_chromaFormatIDC(3)`.

2. **`encodeCodecConfig` est une union**, donc son écriture est `unsafe`.
   Seule la variante correspondant au GUID passé à `nvEncInitializeEncoder`
   est relue par NVENC.

3. **`nvEncGetEncodePresetConfigEx` exige deux versions.**
   `NV_ENC_PRESET_CONFIG.version = NV_ENC_PRESET_CONFIG_VER` **et**
   `presetCfg.version = NV_ENC_CONFIG_VER` avant l'appel. Oublier la seconde
   donne `NV_ENC_ERR_INVALID_VERSION`.

4. **Les `Default::default()` générés écrivent des zéros bruts.**
   `NV_ENC_PIC_STRUCT` n'a pas de variante 0 : l'instant entre
   `Default::default()` et l'écriture de `pictureStruct` porte une valeur
   d'énumération invalide. C'est ce que la crate fournit ; on écrase le champ
   immédiatement dans le même littéral de structure.

5. **`NV_ENCODE_API_FUNCTION_LIST` dérive `Copy`.** Ne jamais en garder de
   copie : les pointeurs pointent dans `nvEncodeAPI64.dll`. Voir la note de
   durée de vie en tête de `sky-encode/src/nvenc.rs`.
