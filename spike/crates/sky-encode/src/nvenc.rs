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
    NV_ENC_CODEC_HEVC_GUID, NV_ENC_CONFIG, NV_ENC_CONFIG_H264_VUI_PARAMETERS, NV_ENC_CONFIG_VER,
    NV_ENC_CREATE_BITSTREAM_BUFFER, NV_ENC_CREATE_BITSTREAM_BUFFER_VER, NV_ENC_DEVICE_TYPE,
    NV_ENC_H264_PROFILE_HIGH_444_GUID, NV_ENC_H264_PROFILE_HIGH_GUID,
    NV_ENC_HEVC_PROFILE_FREXT_GUID, NV_ENC_INITIALIZE_PARAMS, NV_ENC_INITIALIZE_PARAMS_VER,
    NV_ENC_INPUT_RESOURCE_TYPE, NV_ENC_LOCK_BITSTREAM, NV_ENC_LOCK_BITSTREAM_VER,
    NV_ENC_MAP_INPUT_RESOURCE, NV_ENC_MAP_INPUT_RESOURCE_VER, NV_ENC_MULTI_PASS,
    NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS, NV_ENC_OPEN_ENCODE_SESSION_EX_PARAMS_VER,
    NV_ENC_PARAMS_RC_MODE, NV_ENC_PIC_FLAGS, NV_ENC_PIC_PARAMS, NV_ENC_PIC_PARAMS_VER,
    NV_ENC_PIC_STRUCT, NV_ENC_PIC_TYPE, NV_ENC_PRESET_CONFIG, NV_ENC_PRESET_CONFIG_VER,
    NV_ENC_PRESET_P4_GUID, NV_ENC_REGISTER_RESOURCE, NV_ENC_REGISTER_RESOURCE_VER,
    NV_ENC_SEQUENCE_PARAM_PAYLOAD, NV_ENC_SEQUENCE_PARAM_PAYLOAD_VER, NV_ENC_TUNING_INFO,
    NV_ENC_VUI_COLOR_PRIMARIES, NV_ENC_VUI_MATRIX_COEFFS, NV_ENC_VUI_TRANSFER_CHARACTERISTIC,
    NV_ENC_VUI_VIDEO_FORMAT,
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
    /// Levé par [`NvencEncoder::forcer_image_cle`], rabaissé par l'encodage
    /// qui le consomme : la demande vaut pour la prochaine image seulement.
    cle_demandee: bool,
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
            cle_demandee: false,
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

        // Le préréglage P4 active un double passage interne (estimation à
        // résolution 1/4, puis passage final). Mesuré pendant la mise au
        // point de la Tâche 4 (comparatif codecs) comme cause d'une
        // conformité au débit très erratique sur du contenu à forte entropie
        // chroma : jusqu'à 74 Mbps réels pour une cible à 10 Mbps. Un seul
        // passage tient la cible de façon stable en H.264 4:2:0, HEVC 4:4:4
        // et AV1 4:2:0. Écart résiduel non expliqué : H.264 4:4:4 continue de
        // largement dépasser la cible même en un seul passage — voir le
        // rapport de la Tâche 4, ce n'est pas ce paramètre qui en est cause.
        config.rcParams.multiPass = NV_ENC_MULTI_PASS::NV_ENC_MULTI_PASS_DISABLED;

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
                // ATTENTION à la sémantique : NVENC émet SPS/PPS « à chaque
                // image IDR », et `idrPeriod = INFINITE` n'en produit qu'une.
                // Ce drapeau est donc SANS EFFET tant qu'aucun IDR n'est produit,
                // ce qui est le régime normal ici — mesuré : 1 seul SPS sur 1201
                // images. Il ne rend PAS le flux rejoignable en cours de route.
                // Voir le rapport de la Tâche 3 : le jalon 2 passe par
                // `nvEncGetSequenceParams` pour un spectateur qui arrive tard.
                // Mais il REDEVIENT ACTIF dès qu'un IDR est forcé : en HEVC, il
                // produit alors les en-têtes à lui seul, comme
                // `NV_ENC_PIC_FLAG_OUTPUT_SPSPPS` — voir `encoder_mappee`, où la
                // redondance est expliquée. (Mesuré en HEVC ; en H.264, non.)
                h264.set_repeatSPSPPS(1);
                appliquer_vui(&mut h264.h264VUIParameters);
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
                appliquer_vui(&mut hevc.hevcVUIParameters);
            } else {
                let av1 = &mut config.encodeCodecConfig.av1Config;
                av1.idrPeriod = NVENC_INFINITE_GOPLENGTH;
                av1.set_chromaFormatIDC(chroma_format_idc);
                av1.set_enableIntraRefresh(1);
                av1.intraRefreshPeriod = periode_refresh;
                av1.intraRefreshCnt = compte_refresh;
                // Équivalent AV1 de `repeatSPSPPS`, posé pour que les trois
                // codecs soient configurés de la même façon — condition d'un
                // comparatif honnête en Tâche 4. Même réserve qu'en H.264 :
                // lié aux images clés, donc sans effet sous GOP infini.
                av1.set_repeatSeqHdr(1);
                // AV1 porte la description couleur dans des champs propres,
                // pas dans une structure VUI.
                av1.colorPrimaries = NV_ENC_VUI_COLOR_PRIMARIES::NV_ENC_VUI_COLOR_PRIMARIES_BT709;
                av1.transferCharacteristics =
                    NV_ENC_VUI_TRANSFER_CHARACTERISTIC::NV_ENC_VUI_TRANSFER_CHARACTERISTIC_SRGB;
                av1.matrixCoefficients = NV_ENC_VUI_MATRIX_COEFFS::NV_ENC_VUI_MATRIX_COEFFS_BT470BG;
                av1.colorRange = 1;
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

    /// Encode une image capturée.
    ///
    /// Rend toujours `Some` en cas de succès : avec `frameIntervalP = 1`, une
    /// image entrée donne une image sortie. L'`Option` est conservée parce que
    /// l'interface du jalon la déclare, et qu'un encodeur à images B en aurait
    /// besoin.
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

        let resultat = self.encoder_mappee(mappee);

        // Libération systématique, y compris sur erreur : démapper d'abord,
        // désenregistrer ensuite (l'inverse est refusé par NVENC).
        let demappage = self.demapper(mappee);
        let desenregistrement = self.desenregistrer(enregistree);

        let (data, is_keyframe) = resultat?;
        demappage?;
        desenregistrement?;

        // Le chronomètre s'arrête ici, et pas plus tôt : `encode_us` doit
        // couvrir l'appel entier, démappage et désenregistrement compris. Ces
        // deux libérations sont un coût réel par image du chemin tel qu'il est
        // écrit ; les exclure gonflerait la conclusion de Q2 du bon côté.
        Ok(Some(EncodedPacket {
            data,
            is_keyframe,
            encode_us: t0.elapsed().as_micros() as u64,
        }))
    }

    /// Les en-têtes de séquence (VPS, SPS et PPS en HEVC), en Annex-B.
    ///
    /// À placer **en tête du flux d'un spectateur, avant la première image qu'on
    /// lui envoie** : sous GOP infini l'encodeur n'émet ces en-têtes qu'avec le
    /// tout premier IDR, que le spectateur n'a jamais reçu s'il arrive après. Sans
    /// eux le décodeur refuse le flux entier et l'écran reste noir.
    ///
    /// Ne dépend d'aucune image : peut être appelé dès la création de l'encodeur,
    /// et autant de fois qu'il y a de spectateurs.
    pub fn entetes_de_sequence(&self) -> anyhow::Result<Vec<u8>> {
        // Trois en-têtes HEVC tiennent en quelques dizaines d'octets ; 1024
        // laisse de la marge, et NVENC échoue plutôt que de déborder.
        const TAILLE_TAMPON: usize = 1024;

        let get_params = nvenc_fn!(self.api, nvEncGetSequenceParams);
        let mut tampon = vec![0u8; TAILLE_TAMPON];
        let mut ecrits: u32 = 0;
        let mut charge = NV_ENC_SEQUENCE_PARAM_PAYLOAD {
            version: NV_ENC_SEQUENCE_PARAM_PAYLOAD_VER,
            inBufferSize: TAILLE_TAMPON as u32,
            spsppsBuffer: tampon.as_mut_ptr().cast(),
            outSPSPPSPayloadSize: &mut ecrits,
            ..Default::default()
        };
        nvenc_sys::check(unsafe { get_params(self.encoder, &mut charge) })
            .with_context(|| format!("nvEncGetSequenceParams : {}", self.derniere_erreur()))?;

        anyhow::ensure!(
            ecrits > 0 && ecrits as usize <= TAILLE_TAMPON,
            "nvEncGetSequenceParams a rendu {ecrits} octets pour un tampon de {TAILLE_TAMPON}"
        );
        tampon.truncate(ecrits as usize);
        Ok(tampon)
    }

    /// Fait de la **prochaine** image encodée un IDR, précédé de ses en-têtes.
    ///
    /// À appeler quand un spectateur demande un point de reprise par le canal
    /// de contrôle, puis à continuer la boucle d'envoi normalement : l'image
    /// suivante sort en `is_keyframe`, et le drapeau retombe ensuite de lui-même.
    /// Plusieurs appels avant l'encodage suivant n'en font qu'un : c'est voulu,
    /// dix spectateurs qui arrivent ensemble ne coûtent qu'un IDR.
    pub fn forcer_image_cle(&mut self) {
        self.cle_demandee = true;
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

    /// Soumet l'image et rend les octets du flux plus l'indicateur d'image clé.
    /// Le chronométrage est fait par l'appelant, pour couvrir aussi la
    /// libération des ressources.
    fn encoder_mappee(&mut self, mappee: *mut c_void) -> anyhow::Result<(Vec<u8>, bool)> {
        let encode_picture = nvenc_fn!(self.api, nvEncEncodePicture);
        // Une image clé demandée devient un IDR accompagné de ses en-têtes :
        // un point de reprise n'en est un que si le décodeur peut l'ouvrir.
        // Mesuré : `repeatSPSPPS` (posé à la configuration) et `OUTPUT_SPSPPS`
        // produisent chacun seuls ces en-têtes sur un IDR forcé ; retirer l'un des
        // deux ne fait rougir aucun test, retirer les deux fait rougir celui de
        // `forcer_une_image_cle`. Le drapeau est gardé : il rend le point de
        // reprise indépendant de la configuration.
        let drapeaux = if std::mem::take(&mut self.cle_demandee) {
            NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_FORCEIDR as u32
                | NV_ENC_PIC_FLAGS::NV_ENC_PIC_FLAG_OUTPUT_SPSPPS as u32
        } else {
            0
        };
        let mut pic = NV_ENC_PIC_PARAMS {
            version: NV_ENC_PIC_PARAMS_VER,
            encodePicFlags: drapeaux,
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
            // Impossible avec la configuration actuelle (ni image B, ni
            // lookahead). Si ça survient, c'est que la configuration a changé —
            // et l'appelant s'apprête alors à démapper une ressource que NVENC
            // considère encore retenue, ce que le SDK interdit. On échoue fort
            // plutôt que de transformer une violation d'invariant en silence.
            anyhow::bail!(
                "NVENC a renvoyé NV_ENC_ERR_NEED_MORE_INPUT : la configuration \
                 de l'encodeur retient désormais des images, le démappage \
                 immédiat n'est plus valide (revoir frameIntervalP / lookahead)"
            );
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

        Ok((data, is_keyframe))
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

/// Décrit au décodeur la conversion couleur que NVENC a appliquée.
///
/// L'entrée est du RGB (`ARGB`) : c'est NVENC qui le convertit en YUV sur le
/// GPU. Sans ces champs, le décodeur *devine* la matrice et la plage, et une
/// mauvaise interprétation se lit comme un défaut de qualité de l'encodeur
/// alors qu'elle n'est qu'un défaut de signalisation.
///
/// Valeurs alignées sur la correspondance qu'applique FFmpeg (`nvenc.c`) pour
/// une entrée RGB : primaires BT.709, transfert sRGB, matrice BT.601
/// (`BT470BG`), plage pleine. La plage est vérifiée expérimentalement par
/// aller-retour ; la matrice ne l'est pas — voir le rapport de la Tâche 3.
fn appliquer_vui(vui: &mut NV_ENC_CONFIG_H264_VUI_PARAMETERS) {
    vui.videoSignalTypePresentFlag = 1;
    vui.videoFormat = NV_ENC_VUI_VIDEO_FORMAT::NV_ENC_VUI_VIDEO_FORMAT_UNSPECIFIED;
    // Le RGB d'un écran occupe 0-255, pas 16-235.
    vui.videoFullRangeFlag = 1;
    vui.colourDescriptionPresentFlag = 1;
    vui.colourPrimaries = NV_ENC_VUI_COLOR_PRIMARIES::NV_ENC_VUI_COLOR_PRIMARIES_BT709;
    vui.transferCharacteristics =
        NV_ENC_VUI_TRANSFER_CHARACTERISTIC::NV_ENC_VUI_TRANSFER_CHARACTERISTIC_SRGB;
    vui.colourMatrix = NV_ENC_VUI_MATRIX_COEFFS::NV_ENC_VUI_MATRIX_COEFFS_BT470BG;
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

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
        D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
        D3D11_USAGE_DEFAULT,
    };
    use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

    const LARGEUR: u32 = 1280;
    const HAUTEUR: u32 = 720;

    /// Un encodeur HEVC 4:4:4 et l'image à lui donner, ou `None` quand cette
    /// machine n'a pas d'encodeur NVIDIA HEVC 4:4:4 : les tests en sortent alors
    /// sans rien prouver, et le disent sur la sortie d'erreur.
    ///
    /// **Toute autre erreur de création fait échouer le test.** Sans cette
    /// distinction, un encodeur que notre code viendrait de casser serait pris
    /// pour une machine sans GPU, et tous les tests passeraient en silence. Le
    /// verdict « pas de matériel » vient de `probe_hardware`, pas de l'échec de
    /// la création qu'on cherche à juger.
    ///
    /// Sans fenêtre ni capture : le périphérique et la texture sont fabriqués
    /// ici, et l'image est tenue avec l'encodeur parce qu'elle doit venir du
    /// même périphérique que lui.
    fn encodeur_de_test() -> Option<(NvencEncoder, CapturedFrame)> {
        match crate::probe_hardware() {
            Ok(caps) if caps.codecs.contains(&Codec::Hevc444) => {}
            Ok(_) => {
                eprintln!("test ignoré : pas d'encodage HEVC 4:4:4 sur cette machine");
                return None;
            }
            Err(e) => {
                eprintln!("test ignoré : pas de carte NVIDIA utilisable ({e})");
                return None;
            }
        }
        Some(construire_encodeur_de_test().expect("création de l'encodeur de test"))
    }

    fn construire_encodeur_de_test() -> anyhow::Result<(NvencEncoder, CapturedFrame)> {
        let mut peripherique: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                windows::Win32::Foundation::HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                None,
                D3D11_SDK_VERSION,
                Some(&mut peripherique),
                None,
                None,
            )
        }
        .context("D3D11CreateDevice")?;
        let peripherique = peripherique.ok_or_else(|| anyhow!("aucun périphérique rendu"))?;

        let encodeur = NvencEncoder::new(
            &peripherique,
            Codec::Hevc444,
            LARGEUR,
            HAUTEUR,
            60,
            20_000_000,
        )?;
        let image = image_de_test(&peripherique)?;
        Ok((encodeur, image))
    }

    /// Un dégradé : assez de matière pour que l'encodeur ait quelque chose à
    /// coder, sans quoi les images suivantes se réduiraient à presque rien.
    fn image_de_test(peripherique: &ID3D11Device) -> anyhow::Result<CapturedFrame> {
        let mut pixels = vec![0u8; (LARGEUR * HAUTEUR * 4) as usize];
        for y in 0..HAUTEUR {
            for x in 0..LARGEUR {
                let i = ((y * LARGEUR + x) * 4) as usize;
                pixels[i] = (x % 256) as u8; // B
                pixels[i + 1] = (y % 256) as u8; // G
                pixels[i + 2] = ((x + y) % 256) as u8; // R
                pixels[i + 3] = 255; // A
            }
        }
        let description = D3D11_TEXTURE2D_DESC {
            Width: LARGEUR,
            Height: HAUTEUR,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let donnees = D3D11_SUBRESOURCE_DATA {
            pSysMem: pixels.as_ptr().cast(),
            SysMemPitch: LARGEUR * 4,
            SysMemSlicePitch: 0,
        };
        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe { peripherique.CreateTexture2D(&description, Some(&donnees), Some(&mut texture)) }
            .context("CreateTexture2D")?;
        Ok(CapturedFrame {
            texture: texture.ok_or_else(|| anyhow!("aucune texture rendue"))?,
            width: LARGEUR,
            height: HAUTEUR,
            captured_at: Instant::now(),
        })
    }

    /// Types de NAL HEVC de tous les codes de départ `00 00 01` du flux (un code
    /// de départ sur quatre octets `00 00 00 01` en contient un sur trois).
    /// VPS = 32, SPS = 33, PPS = 34. Le type occupe les six bits de poids fort
    /// de l'octet qui suit le code, le bit de poids fort étant toujours nul.
    fn types_de_nal(flux: &[u8]) -> Vec<u8> {
        let mut types = Vec::new();
        let mut i = 0;
        while i + 3 < flux.len() {
            if flux[i] == 0 && flux[i + 1] == 0 && flux[i + 2] == 1 {
                types.push((flux[i + 3] >> 1) & 0x3f);
                i += 4;
            } else {
                i += 1;
            }
        }
        types
    }

    /// Un IDR en HEVC porte le type de NAL 19 (IDR_W_RADL) ou 20 (IDR_N_LP).
    fn contient_idr(flux: &[u8]) -> bool {
        types_de_nal(flux).iter().any(|t| *t == 19 || *t == 20)
    }

    #[test]
    fn les_entetes_de_sequence_contiennent_vps_sps_et_pps() {
        let Some((encodeur, _)) = encodeur_de_test() else {
            return;
        };
        let entetes = encodeur.entetes_de_sequence().expect("en-têtes");
        let types = types_de_nal(&entetes);
        assert!(types.contains(&32), "VPS absent : {types:?}");
        assert!(types.contains(&33), "SPS absent : {types:?}");
        assert!(types.contains(&34), "PPS absent : {types:?}");
    }

    #[test]
    fn forcer_une_image_cle_produit_un_idr_a_l_image_suivante() {
        let Some((mut encodeur, image)) = encodeur_de_test() else {
            return;
        };
        // Une première image, qui est déjà un IDR.
        let _ = encodeur.encode(&image).expect("première image");
        // Une deuxième, qui ne doit PAS l'être. Elle est encodée avant tout appel
        // à `forcer_image_cle` : son assertion ne dit donc rien sur cette
        // fonction. Elle écarte un encodeur qui sortirait un IDR à chaque image,
        // auquel cas « la troisième est un IDR » ne prouverait rien non plus. Ce
        // qui éprouve `forcer_image_cle`, c'est la troisième assertion.
        let ordinaire = encodeur
            .encode(&image)
            .expect("deuxième image")
            .expect("un paquet");
        assert!(
            !contient_idr(&ordinaire.data),
            "la deuxième image ne devrait pas être un IDR"
        );

        encodeur.forcer_image_cle();
        let forcee = encodeur
            .encode(&image)
            .expect("troisième image")
            .expect("un paquet");
        assert!(
            contient_idr(&forcee.data),
            "l'image forcée doit être un IDR"
        );
        // Et un IDR sans ses en-têtes ne sert à rien à un spectateur qui n'a
        // jamais reçu ceux de la séquence : c'est l'autre moitié du geste.
        let types = types_de_nal(&forcee.data);
        for (type_nal, nom) in [(32, "VPS"), (33, "SPS"), (34, "PPS")] {
            assert!(
                types.contains(&type_nal),
                "l'image forcée doit porter son {nom} : {types:?}"
            );
        }
        // Le drapeau est rabaissé : l'image d'après redevient ordinaire.
        let suivante = encodeur
            .encode(&image)
            .expect("quatrième image")
            .expect("un paquet");
        assert!(
            !contient_idr(&suivante.data),
            "le drapeau doit être rabaissé après une image"
        );
    }
}
