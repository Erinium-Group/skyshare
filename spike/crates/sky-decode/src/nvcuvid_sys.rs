//! Chargement dynamique de NVDEC (`nvcuvid.dll`) et table de fonctions.
//!
//! Même approche que `sky-encode::nvenc_sys` : la DLL est livrée avec le pilote
//! NVIDIA (`C:\Windows\System32`), pas avec le SDK, qui ne fournit qu'une
//! bibliothèque d'import. On la charge à l'exécution avec `libloading` et on
//! résout chaque fonction par son nom ; les déclarations `extern "C"` de
//! `nvidia-video-codec-sdk` ne sont jamais appelées (elles exigeraient
//! `nvcuvid.lib` à l'édition de liens).
//!
//! Garder l'instance de [`NvcuvidApi`] en vie aussi longtemps qu'un pointeur
//! de fonction en est tiré : la DLL est déchargée à sa destruction.

// La tâche 1 n'appelle que `cuvidGetDecoderCaps` ; le reste de la table sert
// dès la session de décodage.
#![allow(dead_code)]

use libloading::os::windows::{Library as WinLibrary, LOAD_LIBRARY_SEARCH_SYSTEM32};
use libloading::Library;
use nvidia_video_codec_sdk::sys::cuviddec::{
    CUvideodecoder, CUVIDDECODECAPS, CUVIDDECODECREATEINFO, CUVIDPICPARAMS, CUVIDPROCPARAMS,
};
use nvidia_video_codec_sdk::sys::nvcuvid::{
    CUvideoparser, CUVIDPARSERPARAMS, CUVIDSOURCEDATAPACKET,
};

use cudarc::driver::sys::CUresult;

/// Nom de la DLL livrée avec le pilote NVIDIA.
const NVCUVID_DLL: &str = "nvcuvid.dll";

type GetDecoderCapsFn = unsafe extern "C" fn(*mut CUVIDDECODECAPS) -> CUresult;
type CreateVideoParserFn =
    unsafe extern "C" fn(*mut CUvideoparser, *mut CUVIDPARSERPARAMS) -> CUresult;
type ParseVideoDataFn = unsafe extern "C" fn(CUvideoparser, *mut CUVIDSOURCEDATAPACKET) -> CUresult;
type DestroyVideoParserFn = unsafe extern "C" fn(CUvideoparser) -> CUresult;
type CreateDecoderFn =
    unsafe extern "C" fn(*mut CUvideodecoder, *mut CUVIDDECODECREATEINFO) -> CUresult;
type DecodePictureFn = unsafe extern "C" fn(CUvideodecoder, *mut CUVIDPICPARAMS) -> CUresult;
type MapVideoFrame64Fn =
    unsafe extern "C" fn(CUvideodecoder, i32, *mut u64, *mut u32, *mut CUVIDPROCPARAMS) -> CUresult;
type UnmapVideoFrame64Fn = unsafe extern "C" fn(CUvideodecoder, u64) -> CUresult;
type DestroyDecoderFn = unsafe extern "C" fn(CUvideodecoder) -> CUresult;

/// Bibliothèque NVDEC chargée et fonctions résolues.
pub struct NvcuvidApi {
    // Ne sert qu'à garder la DLL chargée : les pointeurs ci-dessous pointent
    // dedans et deviendraient invalides si elle était déchargée.
    _lib: Library,
    pub cuvid_get_decoder_caps: GetDecoderCapsFn,
    pub cuvid_create_video_parser: CreateVideoParserFn,
    pub cuvid_parse_video_data: ParseVideoDataFn,
    pub cuvid_destroy_video_parser: DestroyVideoParserFn,
    pub cuvid_create_decoder: CreateDecoderFn,
    pub cuvid_decode_picture: DecodePictureFn,
    pub cuvid_map_video_frame64: MapVideoFrame64Fn,
    pub cuvid_unmap_video_frame64: UnmapVideoFrame64Fn,
    pub cuvid_destroy_decoder: DestroyDecoderFn,
}

impl NvcuvidApi {
    /// Charge `nvcuvid.dll` (recherchée uniquement dans `System32`, pas dans le
    /// répertoire courant ni le `PATH` : c'est le vecteur classique de « DLL
    /// planting ») et résout les fonctions de décodage.
    pub fn load() -> Result<Self, libloading::Error> {
        let win_lib =
            unsafe { WinLibrary::load_with_flags(NVCUVID_DLL, LOAD_LIBRARY_SEARCH_SYSTEM32) }?;
        let lib = Library::from(win_lib);

        // Chaque symbole est copié hors de son `Symbol` : le pointeur brut reste
        // valide tant que `_lib` vit, et `Self` possède `_lib`.
        macro_rules! symbole {
            ($nom:literal) => {
                *unsafe { lib.get(concat!($nom, "\0").as_bytes()) }?
            };
        }

        Ok(Self {
            cuvid_get_decoder_caps: symbole!("cuvidGetDecoderCaps"),
            cuvid_create_video_parser: symbole!("cuvidCreateVideoParser"),
            cuvid_parse_video_data: symbole!("cuvidParseVideoData"),
            cuvid_destroy_video_parser: symbole!("cuvidDestroyVideoParser"),
            cuvid_create_decoder: symbole!("cuvidCreateDecoder"),
            cuvid_decode_picture: symbole!("cuvidDecodePicture"),
            cuvid_map_video_frame64: symbole!("cuvidMapVideoFrame64"),
            cuvid_unmap_video_frame64: symbole!("cuvidUnmapVideoFrame64"),
            cuvid_destroy_decoder: symbole!("cuvidDestroyDecoder"),
            _lib: lib,
        })
    }
}

// `nvidia-video-codec-sdk` déclare `NvEncodeAPICreateInstance` et
// `NvEncodeAPIGetMaxSupportedVersion` dans un bloc `extern "C"` lié
// statiquement, que son module `safe` (jamais utilisé ici) référence. Le lien
// final d'un binaire de release élimine ce code mort ; celui d'un binaire de
// test en debug sur MSVC le garde et exige les deux symboles (LNK2019).
//
// `sky-encode` les définit déjà, et tout binaire qui l'embarque les reçoit de
// lui : les définir ici en plus provoquerait un doublon (LNK2005). On ne les
// fournit donc que pour le binaire de test de cette crate, qui n'embarque pas
// `sky-encode`. Elles ne s'exécutent jamais.
#[cfg(test)]
mod symboles_de_lien_pour_les_tests {
    use nvidia_video_codec_sdk::sys::nvEncodeAPI::{NVENCSTATUS, NV_ENCODE_API_FUNCTION_LIST};

    #[no_mangle]
    extern "C" fn NvEncodeAPICreateInstance(
        _function_list: *mut NV_ENCODE_API_FUNCTION_LIST,
    ) -> NVENCSTATUS {
        unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-decode")
    }

    #[no_mangle]
    extern "C" fn NvEncodeAPIGetMaxSupportedVersion(_version: *mut u32) -> NVENCSTATUS {
        unreachable!("nvidia_video_codec_sdk::safe::Encoder n'est jamais utilisé par sky-decode")
    }
}
