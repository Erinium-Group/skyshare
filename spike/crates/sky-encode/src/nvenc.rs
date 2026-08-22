//! Session NVENC alimentée **directement** par une texture Direct3D 11.
//!
//! C'est le pari technique central de SkyShare : la texture produite par
//! Windows.Graphics.Capture reste en mémoire vidéo de bout en bout. NVENC la
//! lit là où elle est ; rien ne redescend en RAM sauf le flux déjà compressé.
//!
//! Chaîne d'appels (celle qu'utilise OBS) :
//!
//! ```text
//! nvEncOpenEncodeSessionEx(device = ID3D11Device*, deviceType = DIRECTX)
//!   └─ nvEncRegisterResource(resourceType = DIRECTX, resourceToRegister = ID3D11Texture2D*)
//!        └─ nvEncMapInputResource
//!             └─ nvEncEncodePicture
//!                  └─ nvEncLockBitstream  ->  octets
//! ```
//!
//! La plomberie de chargement (`nvEncodeAPI64.dll`, table de fonctions) vient
//! de [`crate::nvenc_sys`] : on ne recharge jamais la DLL ici.
//!
//! **Durée de vie.** `NV_ENCODE_API_FUNCTION_LIST` est `Copy` : en garder une
//! copie survivrait au déchargement de la DLL et laisserait des pointeurs de
//! fonction vers de la mémoire démappée. [`NvencEncoder`] **possède** donc son
//! [`NvencApi`] et ne lit la table qu'au moment de l'appel, via
//! `self.api.functions()`.

use std::ffi::{c_void, CStr};
use std::ptr;
use std::time::Instant;

use anyhow::{anyhow, Context};
use nvidia_video_codec_sdk::sys::nvEncodeAPI::{
    GUID, NVENCAPI_VERSION, NVENCSTATUS, NVENC_INFINITE_GOPLENGTH, NV_ENC_AV1_PROFILE_MAIN_GUID,
    NV_ENC_BUFFER_FORMAT, NV_ENC_BUFFER_USAGE, NV_ENC_CODEC_AV1_GUID, NV_ENC_CODEC_H264_GUID,
    NV_ENC_CODEC_HEVC_GUID, NV_ENC_CONFIG, NV_ENC_CONFIG_VER, NV_ENC_CREATE_BITSTREAM_BUFFER,
    NV_ENC_CREATE_BITSTREAM_BUFFER_VER, NV_ENC_DEVICE_TYPE, NV_ENC_H264_PROFILE_HIGH_444_GUID,
    NV_ENC_H264_PROFILE_HIGH_GUID, NV_ENC_HEVC_PROFILE_FREXT_GUID, NV_ENC_INITIALIZE_PARAMS,
    NV_ENC_INITIALIZE_PARAMS_VER, NV_ENC_INPUT_RESOURCE_TYPE, NV_ENC_LOCK_BITSTREAM,
    NV_ENC_LOCK_BITSTREAM_VER, NV_ENC_MAP_INPUT_RESOURCE, NV_ENC_MAP_INPUT_RESOURCE_VER,
    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
    NV_ENC_PARAMS_RC_MODE, NV_ENC_PIC_FLAGS, NV_ENC_PIC_PARAMS, NV_ENC_PIC_PARAMS_VER,
    NV_ENC_PIC_STRUCT, NV_ENC_PIC_TYPE, NV_ENC_PRESET_CONFIG, NV_ENC_PRESET_CONFIG_VER,
    NV_ENC_PRESET_P4_GUID, NV_ENC_REGISTER_RESOURCE, NV_ENC_REGISTER_RESOURCE_VER,
    NV_ENC_TUNING_INFO,
};
use sky_capture::CapturedFrame;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};

use crate::nvenc_sys::{self, NvencApi};
use crate::{Codec, EncodedPacket};

/// Récupère un pointeur de fonction dans la table NVENC **au moment de
/// l'appel**. Jamais stocké : voir la note de durée de vie du module.
macro_rules! nvenc_fn {
    ($api:expr, $nom:ident) => {
        $api.functions().$nom.ok_or_else(|| {
            anyhow!(concat!(
                "NVENC : ",
                stringify!($nom),
                " absente de la table de fonctions"
            ))
        })?
    };
}

/// Format d'entrée : la texture WGC est en `DXGI_FORMAT_B8G8R8A8_UNORM`, ce
/// que NVENC appelle `ARGB` (A8R8G8B8 petit-boutiste = B,G,R,A en mémoire).
/// NVENC fait lui-même la conversion RGB -> YUV sur le GPU ; en 4:4:4 elle est
/// sans perte de résolution chromatique, ce qui est tout l'objet du pari.
const FORMAT_ENTREE: NV_ENC_BUFFER_FORMAT = NV_ENC_BUFFER_FORMAT::NV_ENC_BUFFER_FORMAT_ARGB;

/// Encodeur NVENC alimenté par des textures Direct3D 11.
///
/// Non `Send` par construction (champs pointeurs bruts) : une session NVENC
/// ne s'utilise que depuis un seul thread à la fois.
pub struct NvencEncoder {
    /// Possédé, jamais copié : garde la DLL chargée et la table valide aussi
    /// longtemps que la session existe.
    api: NvencApi,
    encoder: *mut c_void,
    bitstream: *mut c_void,
    codec: Codec,
    width: u32,
    height: u32,
    frame_index: u64,
}

impl NvencEncoder {
    /// Ouvre une session NVENC **sur le device Direct3D 11 fourni**.
    ///
    /// Le device doit être celui qui a produit les textures à encoder
    /// (`WgcCapture::d3d_device()`) : NVENC refuse d'enregistrer une ressource
    /// appartenant à un autre device.
    pub fn new(
        device: &ID3D11Device,
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate_bps: u32,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(fps > 0, "fps doit être > 0");
        anyhow::ensure!(width > 0 && height > 0, "dimensions nulles");

        let api = NvencApi::load().context("chargement de nvEncodeAPI64.dll")?;

        let open_session = nvenc_fn!(api, nvEncOpenEncodeSessionEx);
        let mut params = NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS {
            version: NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
            deviceType: NV_ENC_DEVICE_TYPE::NV_ENC_DEVICE_TYPE_DIRECTX,
            // `as_raw` rend le `ID3D11Device*` brut attendu par NVENC. Le COM
            // reste possédé par l'appelant : NVENC ajoute sa propre référence.
            device: device.as_raw(),
            apiVersion: NVENCAPI_VERSION,
            ..Default::default()
        };
        let mut session: *mut c_void = ptr::null_mut();
        nvenc_sys::check(unsafe { open_session(&mut params, &mut session) })
            .context("nvEncOpenEncodeSessionEx (device Direct3D 11)")?;

        // Dès que la session existe, on la confie à `NvencEncoder` : son `Drop`
        // la détruira même si l'initialisation ci-dessous échoue.
        let mut enc = Self {
            api,
            encoder: session,
            bitstream: ptr::null_mut(),
            codec,
            width,
            height,
            frame_index: 0,
        };
        enc.initialiser(fps, bitrate_bps)?;
        Ok(enc)
    }

    /// Configure l'encodeur puis alloue le tampon de sortie.
    fn initialiser(&mut self, fps: u32, bitrate_bps: u32) -> anyhow::Result<()> {
        let (codec_guid, profil_guid, chroma_format_idc) = parametres_codec(self.codec);

        // 1. Partir de la configuration du préréglage P4 / latence ultra-basse,
        //    puis n'ajuster que ce qui nous concerne. C'est la méthode
        //    recommandée : les champs qu'on ne comprend pas restent cohérents.
        let get_preset = nvenc_fn!(self.api, nvEncGetEncodePresetConfigEx);
        let mut preset = NV_ENC_PRESET_CONFIG {
            version: NV_ENC_PRESET_CONFIG_VER,
            presetCfg: NV_ENC_CONFIG {
                version: NV_ENC_CONFIG_VER,
                ..Default::default()
            },
            ..Default::default()
        };
        nvenc_sys::check(unsafe {
            get_preset(
                self.encoder,
                codec_guid,
                NV_ENC_PRESET_P4_GUID,
                NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
                &mut preset,
            )
        })
        .with_context(|| format!("nvEncGetEncodePresetConfigEx : {}", self.derniere_erreur()))?;

        let mut config = preset.presetCfg;
        config.version = NV_ENC_CONFIG_VER;
        config.profileGUID = profil_guid;
        // GOP infini + rafraîchissement intra : aucune image-clé périodique,
        // donc aucun pic de débit. Un spectateur qui arrive se resynchronise
        // en une période de rafraîchissement.
        config.gopLength = NVENC_INFINITE_GOPLENGTH;
        // 1 = aucune image bidirectionnelle : latence minimale.
        config.frameIntervalP = 1;

        config.rcParams.rateControlMode = NV_ENC_PARAMS_RC_MODE::NV_ENC_PARAMS_RC_CBR;
        config.rcParams.averageBitRate = bitrate_bps;
        config.rcParams.maxBitRate = bitrate_bps;
        // 1 seule image de tampon VBV : anti-pics, la contrainte de débit est
        // respectée image par image plutôt que sur une fenêtre glissante.
        config.rcParams.vbvBufferSize = bitrate_bps / fps;
        config.rcParams.vbvInitialDelay = bitrate_bps / fps;

        let periode_refresh = fps.saturating_mul(2).max(2);
        let compte_refresh = fps.max(1);

        // SAFETY : `encodeCodecConfig` est une union ; on écrit la variante qui
        // correspond au GUID de codec passé à `nvEncInitializeEncoder`, seule
        // que NVENC relira.
        unsafe {
            if codec_guid == NV_ENC_CODEC_H264_GUID {
                let h264 = &mut config.encodeCodecConfig.h264Config;
                h264.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                h264.chromaFormatIDC = chroma_format_idc;
                h264.set_enableIntraRefresh(1);
                h264.intraRefreshPeriod = periode_refresh;
                h264.intraRefreshCnt = compte_refresh;
                // SPS/PPS répétés : un flux brut reste décodable même si le
                // début manque (utile pour ffprobe comme pour un spectateur
                // qui rejoint en cours de route).
                h264.set_repeatSPSPPS(1);
            } else if codec_guid == NV_ENC_CODEC_HEVC_GUID {
                let hevc = &mut config.encodeCodecConfig.hevcConfig;
                hevc.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                // Écart avec le brief : en HEVC, `chromaFormatIDC` est un champ
                // de bits (2 bits) — accès par `set_chromaFormatIDC`, alors
                // qu'en H.264 c'est un `u32` ordinaire.
                hevc.set_chromaFormatIDC(chroma_format_idc);
                hevc.set_enableIntraRefresh(1);
                hevc.intraRefreshPeriod = periode_refresh;
                hevc.intraRefreshCnt = compte_refresh;
                hevc.set_repeatSPSPPS(1);
            } else {
                let av1 = &mut config.encodeCodecConfig.av1Config;
                av1.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                av1.set_chromaFormatIDC(chroma_format_idc);
                av1.set_enableIntraRefresh(1);
                av1.intraRefreshPeriod = periode_refresh;
                av1.intraRefreshCnt = compte_refresh;
            }
        }

        // 2. Initialisation de l'encodeur.
        let initialize = nvenc_fn!(self.api, nvEncInitializeEncoder);
        let mut init = NV_ENC_INITIALIZE_PARAMS {
            version: NV_ENC_INITIALIZE_PARAMS_VER,
            encodeGUID: codec_guid,
            presetGUID: NV_ENC_PRESET_P4_GUID,
            encodeWidth: self.width,
            encodeHeight: self.height,
            darWidth: self.width,
            darHeight: self.height,
            frameRateNum: fps,
            frameRateDen: 1,
            enableEncodeAsync: 0,
            enablePTD: 1,
            // Le pointeur ne doit être valide que le temps de cet appel :
            // NVENC recopie la configuration.
            encodeConfig: &mut config,
            maxEncodeWidth: self.width,
            maxEncodeHeight: self.height,
            tuningInfo: NV_ENC_TUNING_INFO::NV_ENC_TUNING_INFO_ULTRA_LOW_LATENCY,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { initialize(self.encoder, &mut init) })
            .with_context(|| format!("nvEncInitializeEncoder : {}", self.derniere_erreur()))?;

        // 3. Tampon de sortie (taille 0 = NVENC choisit).
        let create_bitstream = nvenc_fn!(self.api, nvEncCreateBitstreamBuffer);
        let mut create = NV_ENC_CREATE_BITSTREAM_BUFFER {
            version: NV_ENC_CREATE_BITSTREAM_BUFFER_VER,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { create_bitstream(self.encoder, &mut create) })
            .with_context(|| format!("nvEncCreateBitstreamBuffer : {}", self.derniere_erreur()))?;
        self.bitstream = create.bitstreamBuffer;

        Ok(())
    }

    /// Encode une image capturée. `Ok(None)` si NVENC réclame plus d'entrée
    /// (ne devrait pas arriver avec `frameIntervalP = 1`).
    pub fn encode(&mut self, frame: &CapturedFrame) -> anyhow::Result<Option<EncodedPacket>> {
        anyhow::ensure!(
            frame.width == self.width && frame.height == self.height,
            "image {}x{} incompatible avec la session {}x{}",
            frame.width,
            frame.height,
            self.width,
            self.height
        );

        let t0 = Instant::now();

        let enregistree = self.enregistrer(&frame.texture)?;

        let mappee = match self.mapper(enregistree) {
            Ok(m) => m,
            Err(e) => {
                // Rien n'est mappé : on désenregistre seul.
                let _ = self.desenregistrer(enregistree);
                return Err(e);
            }
        };

        let resultat = self.encoder_mappee(mappee, t0);

        // Libération systématique, y compris sur erreur : démapper d'abord,
        // désenregistrer ensuite (l'inverse est refusé par NVENC).
        let demappage = self.demapper(mappee);
        let desenregistrement = self.desenregistrer(enregistree);

        let paquet = resultat?;
        demappage?;
        desenregistrement?;
        Ok(paquet)
    }

    /// Enregistre la texture auprès de NVENC. Aucune copie : NVENC prend une
    /// référence sur la surface Direct3D telle qu'elle est en mémoire vidéo.
    fn enregistrer(&self, texture: &ID3D11Texture2D) -> anyhow::Result<*mut c_void> {
        let register = nvenc_fn!(self.api, nvEncRegisterResource);
        let mut params = NV_ENC_REGISTER_RESOURCE {
            version: NV_ENC_REGISTER_RESOURCE_VER,
            resourceType: NV_ENC_INPUT_RESOURCE_TYPE::NV_ENC_INPUT_RESOURCE_TYPE_DIRECTX,
            width: self.width,
            height: self.height,
            // 0 : pour une ressource Direct3D, le pas est celui de la texture.
            pitch: 0,
            subResourceIndex: 0,
            resourceToRegister: texture.as_raw(),
            bufferFormat: FORMAT_ENTREE,
            bufferUsage: NV_ENC_BUFFER_USAGE::NV_ENC_INPUT_IMAGE,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { register(self.encoder, &mut params) })
            .with_context(|| format!("nvEncRegisterResource : {}", self.derniere_erreur()))?;
        Ok(params.registeredResource)
    }

    fn mapper(&self, enregistree: *mut c_void) -> anyhow::Result<*mut c_void> {
        let map = nvenc_fn!(self.api, nvEncMapInputResource);
        let mut params = NV_ENC_MAP_INPUT_RESOURCE {
            version: NV_ENC_MAP_INPUT_RESOURCE_VER,
            registeredResource: enregistree,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { map(self.encoder, &mut params) })
            .with_context(|| format!("nvEncMapInputResource : {}", self.derniere_erreur()))?;
        Ok(params.mappedResource)
    }

    fn demapper(&self, mappee: *mut c_void) -> anyhow::Result<()> {
        let unmap = nvenc_fn!(self.api, nvEncUnmapInputResource);
        nvenc_sys::check(unsafe { unmap(self.encoder, mappee) })
            .context("nvEncUnmapInputResource")?;
        Ok(())
    }

    fn desenregistrer(&self, enregistree: *mut c_void) -> anyhow::Result<()> {
        let unregister = nvenc_fn!(self.api, nvEncUnregisterResource);
        nvenc_sys::check(unsafe { unregister(self.encoder, enregistree) })
            .context("nvEncUnregisterResource")?;
        Ok(())
    }

    /// Soumet l'image mappée puis récupère les octets du flux.
    fn encoder_mappee(
        &mut self,
        mappee: *mut c_void,
        t0: Instant,
    ) -> anyhow::Result<Option<EncodedPacket>> {
        let encode_picture = nvenc_fn!(self.api, nvEncEncodePicture);
        let mut pic = NV_ENC_PIC_PARAMS {
            version: NV_ENC_PIC_PARAMS_VER,
            inputWidth: self.width,
            inputHeight: self.height,
            inputPitch: self.width,
            frameIdx: self.frame_index as u32,
            inputTimeStamp: self.frame_index,
            inputDuration: 1,
            inputBuffer: mappee,
            outputBitstream: self.bitstream,
            bufferFmt: FORMAT_ENTREE,
            pictureStruct: NV_ENC_PIC_STRUCT::NV_ENC_PIC_STRUCT_FRAME,
            ..Default::default()
        };
        let statut = unsafe { encode_picture(self.encoder, &mut pic) };
        self.frame_index += 1;

        if statut == NVENCSTATUS::NV_ENC_ERR_NEED_MORE_INPUT {
            // Configuration sans image B ni lookahead : ne devrait pas se
            // produire. On rend la main sans flux plutôt que d'échouer.
            return Ok(None);
        }
        nvenc_sys::check(statut)
            .with_context(|| format!("nvEncEncodePicture : {}", self.derniere_erreur()))?;

        let lock = nvenc_fn!(self.api, nvEncLockBitstream);
        let unlock = nvenc_fn!(self.api, nvEncUnlockBitstream);

        let mut verrou = NV_ENC_LOCK_BITSTREAM {
            version: NV_ENC_LOCK_BITSTREAM_VER,
            outputBitstream: self.bitstream,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { lock(self.encoder, &mut verrou) })
            .with_context(|| format!("nvEncLockBitstream : {}", self.derniere_erreur()))?;

        // Seule copie vers la RAM de tout le pipeline — et elle ne porte que
        // le flux déjà compressé, jamais l'image.
        let data = if verrou.bitstreamBufferPtr.is_null() || verrou.bitstreamSizeInBytes == 0 {
            Vec::new()
        } else {
            // SAFETY : NVENC garantit `bitstreamSizeInBytes` octets lisibles à
            // `bitstreamBufferPtr` tant que le verrou est tenu.
            unsafe {
                std::slice::from_raw_parts(
                    verrou.bitstreamBufferPtr.cast::<u8>(),
                    verrou.bitstreamSizeInBytes as usize,
                )
            }
            .to_vec()
        };
        let is_keyframe = matches!(
            verrou.pictureType,
            NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_IDR | NV_ENC_PIC_TYPE::NV_ENC_PIC_TYPE_I
        );

        // Déverrouillage systématique : rien entre le verrou et ici ne peut
        // échouer, mais le tampon doit être rendu avant l'image suivante.
        nvenc_sys::check(unsafe { unlock(self.encoder, self.bitstream) })
            .context("nvEncUnlockBitstream")?;

        Ok(Some(EncodedPacket {
            data,
            is_keyframe,
            encode_us: t0.elapsed().as_micros() as u64,
        }))
    }

    /// Message d'erreur détaillé du driver, pour les diagnostics.
    fn derniere_erreur(&self) -> String {
        let Some(get_last) = self.api.functions().nvEncGetLastErrorString else {
            return "(nvEncGetLastErrorString absente)".into();
        };
        let ptr = unsafe { get_last(self.encoder) };
        if ptr.is_null() {
            return "(aucun détail)".into();
        }
        // SAFETY : NVENC renvoie une chaîne C statique appartenant au driver.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

impl Drop for NvencEncoder {
    fn drop(&mut self) {
        // Ordre imposé par NVENC : fin de flux, puis tampon, puis session.
        // `self.api` est encore vivant ici (Drop::drop précède la libération
        // des champs), donc la table de fonctions est valide.
        let fns = self.api.functions();

        if let Some(encode_picture) = fns.nvEncEncodePicture {
            let mut eos = NV_ENC_PIC_PARAMS {
                version: NV_ENC_PIC_PARAMS_VER,
                encodePicFlags: NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_EOS as u32,
                ..Default::default()
            };
            let _ = unsafe { encode_picture(self.encoder, &mut eos) };
        }

        if !self.bitstream.is_null() {
            if let Some(destroy_bitstream) = fns.nvEncDestroyBitstreamBuffer {
                let _ = unsafe { destroy_bitstream(self.encoder, self.bitstream) };
            }
            self.bitstream = ptr::null_mut();
        }

        if !self.encoder.is_null() {
            if let Some(destroy) = fns.nvEncDestroyEncoder {
                let _ = unsafe { destroy(self.encoder) };
            }
            self.encoder = ptr::null_mut();
        }
    }
}

/// GUID de codec, GUID de profil et `chromaFormatIDC` (1 = 4:2:0, 3 = 4:4:4).
///
/// Rappel matériel : NVENC ne produit pas d'AV1 4:4:4, même sur Ada.
fn parametres_codec(codec: Codec) -> (GUID, GUID, u32) {
    match codec {
        Codec::H264_420 => (NV_ENC_CODEC_H264_GUID, NV_ENC_H264_PROFILE_HIGH_GUID, 1),
        Codec::H264_444 => (NV_ENC_CODEC_H264_GUID, NV_ENC_H264_PROFILE_HIGH_444_GUID, 3),
        Codec::Hevc444 => (NV_ENC_CODEC_HEVC_GUID, NV_ENC_HEVC_PROFILE_FREXT_GUID, 3),
        Codec::Av1_420 => (NV_ENC_CODEC_AV1_GUID, NV_ENC_AV1_PROFILE_MAIN_GUID, 1),
    }
}
