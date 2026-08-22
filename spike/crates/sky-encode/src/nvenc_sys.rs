//! Chargement dynamique de NVENC et table brute de pointeurs de fonctions.
//!
//! [`NvencApi::load`] charge `nvEncodeAPI64.dll` et résout la table de
//! fonctions NVENC (`NV_ENCODE_API_FUNCTION_LIST`) ; gardez l'instance
//! retournée en vie aussi longtemps que vous utilisez un pointeur de
//! [`NvencApi::functions`] — y compris pendant toute la durée d'une session
//! d'encodage (`nvEncOpenEncodeSessionEx` puis un usage prolongé, pas
//! seulement une détection ponctuelle comme dans `caps::probe_hardware`).
//! Ouvrir/fermer la session NVENC elle-même (type de device, paramètres
//! d'init, encodage) reste la responsabilité de l'appelant : ce module ne
//! fournit que la plomberie FFI partagée, pas la logique métier au-dessus.

use std::ffi::c_void;

use libloading::os::windows::{Library as WinLibrary, LOAD_LIBRARY_SEARCH_SYSTEM32};
use libloading::{Library, Symbol};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    GUID, NVENCSTATUS, NV_ENCODE_API_FUNCTION_LIST, NV_ENCODE_API_FUNCTION_LIST_VER,
    NV_ENC_BUFFER_FORMAT,
};

use crate::caps::EncodeError;

/// Nom de la DLL livrée avec le driver NVIDIA (`C:\Windows\System32`),
/// distincte du NVIDIA Video Codec SDK : le SDK ne fournit qu'une bibliothèque
/// d'import statique (`nvEncodeAPI.lib`), pas le code exécutable. FFmpeg et
/// OBS ne lient pas non plus contre ce .lib : ils font `LoadLibrary` puis
/// `GetProcAddress("NvEncodeAPICreateInstance")` au runtime — même approche
/// ici, via la crate `libloading`.
const NVENC_DLL: &str = "nvEncodeAPI64.dll";

type CreateInstanceFn = unsafe extern "C" fn(*mut NV_ENCODE_API_FUNCTION_LIST) -> NVENCSTATUS;

// `nvidia-video-codec-sdk` déclare `NvEncodeAPICreateInstance` et
// `NvEncodeAPIGetMaxSupportedVersion` dans un bloc `extern "C"` lié
// statiquement (son module `safe`, que ce fichier n'utilise jamais — on
// charge NVENC dynamiquement à la place, voir `NvencApi::load`). En release,
// le lien final élimine ce code mort ; en debug sur MSVC, l'initialiseur
// paresseux de `safe::api::ENCODE_API` (une `lazy_static`) reste dans le
// binaire et ces deux symboles restent exigés, même si rien ne les appelle
// jamais. On leur fournit donc nous-mêmes une définition ici : cela satisfait
// le lien, et elle ne s'exécute jamais tant que rien n'appelle
// `nvidia_video_codec_sdk::safe::Encoder`.
#[no_mangle]
extern "C" fn NvEncodeAPICreateInstance(
    _function_list: *mut NV_ENCODE_API_FUNCTION_LIST,
) -> NVENCSTATUS {
    unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-encode")
}

#[no_mangle]
extern "C" fn NvEncodeAPIGetMaxSupportedVersion(_version: *mut u32) -> NVENCSTATUS {
    unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-encode")
}

/// Convertit un code retour NVENC en `Result`.
pub(crate) fn check(status: NVENCSTATUS) -> Result<(), EncodeError> {
    if status == NVENCSTATUS::NV_ENC_SUCCESS {
        Ok(())
    } else {
        Err(EncodeError::Nvenc(status))
    }
}

/// Bibliothèque NVENC chargée et table de pointeurs de fonctions résolue.
///
/// Un seul symbole est jamais lié statiquement : `NvEncodeAPICreateInstance`
/// est résolue dynamiquement au chargement (voir [`load`](Self::load)) ; tout
/// le reste de NVENC (ouverture de session, encodage, etc.) passe à travers
/// [`functions`](Self::functions), la table qu'elle remplit.
pub struct NvencApi {
    // Ne sert qu'à garder la DLL chargée : `fns` contient des pointeurs
    // qui pointent dedans et deviendraient invalides si elle était déchargée.
    _lib: Library,
    fns: NV_ENCODE_API_FUNCTION_LIST,
}

impl NvencApi {
    /// Charge `nvEncodeAPI64.dll` (recherchée uniquement dans `System32`,
    /// où le driver l'installe — pas dans le répertoire courant ni le PATH)
    /// et résout la table de fonctions NVENC.
    pub fn load() -> Result<Self, EncodeError> {
        // Un nom de DLL nu laisse Windows chercher dans plusieurs répertoires
        // (dont potentiellement le répertoire courant du process) — un vecteur
        // de « DLL planting » classique. On restreint la recherche à
        // System32, seul endroit où `nvEncodeAPI64.dll` vit réellement.
        let win_lib =
            unsafe { WinLibrary::load_with_flags(NVENC_DLL, LOAD_LIBRARY_SEARCH_SYSTEM32) }?;
        let lib = Library::from(win_lib);

        let create_instance: Symbol<CreateInstanceFn> =
            unsafe { lib.get(b"NvEncodeAPICreateInstance\0") }?;

        let mut fns = NV_ENCODE_API_FUNCTION_LIST {
            version: NV_ENCODE_API_FUNCTION_LIST_VER,
            ..Default::default()
        };
        check(unsafe { create_instance(&mut fns) })?;

        Ok(Self { _lib: lib, fns })
    }

    /// Table de pointeurs de fonctions NVENC, valide tant que `self` existe.
    pub fn functions(&self) -> &NV_ENCODE_API_FUNCTION_LIST {
        &self.fns
    }
}

/// Énumère, pour une session NVENC déjà ouverte, les GUID de codecs
/// supportés puis pour chacun les formats d'entrée acceptés.
///
/// Énumération brute : aucune connaissance de [`crate::caps::Codec`] ici —
/// c'est à l'appelant de traduire GUID + formats en codecs applicatifs.
pub(crate) fn enumerate_codec_formats(
    session: *mut c_void,
    fns: &NV_ENCODE_API_FUNCTION_LIST,
) -> Result<Vec<(GUID, Vec<NV_ENC_BUFFER_FORMAT>)>, EncodeError> {
    const MSG: &str = "la table de fonctions NVENC doit être remplie par NvEncodeAPICreateInstance";
    let get_guid_count = fns.nvEncGetEncodeGUIDCount.expect(MSG);
    let get_guids = fns.nvEncGetEncodeGUIDs.expect(MSG);
    let get_format_count = fns.nvEncGetInputFormatCount.expect(MSG);
    let get_formats = fns.nvEncGetInputFormats.expect(MSG);

    let mut guid_count = 0u32;
    check(unsafe { get_guid_count(session, &mut guid_count) })?;
    let mut guids = vec![GUID::default(); guid_count as usize];
    let mut actual_guid_count = 0u32;
    check(unsafe {
        get_guids(
            session,
            guids.as_mut_ptr(),
            guid_count,
            &mut actual_guid_count,
        )
    })?;
    guids.truncate(actual_guid_count as usize);

    let mut result = Vec::with_capacity(guids.len());
    for guid in guids {
        let mut format_count = 0u32;
        check(unsafe { get_format_count(session, guid, &mut format_count) })?;
        let mut formats =
            vec![NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_UNDEFINED; format_count as usize];
        let mut actual_format_count = 0u32;
        check(unsafe {
            get_formats(
                session,
                guid,
                formats.as_mut_ptr(),
                format_count,
                &mut actual_format_count,
            )
        })?;
        formats.truncate(actual_format_count as usize);
        result.push((guid, formats));
    }

    Ok(result)
}
