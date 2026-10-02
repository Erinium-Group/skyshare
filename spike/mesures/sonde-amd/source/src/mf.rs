//! Questions 2 et 3 : les décodeurs Media Foundation, énumérés puis éprouvés
//! par un décodage réel.

use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};

use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Multithread, ID3D11Query, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_QUERY_DESC, D3D11_QUERY_EVENT,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

use crate::flux::{CodecFlux, Flux};
use crate::gpu;
use crate::rapport::{err_win, Etape};
use crate::systeme::Adaptateur;

/// Durée maximale d'une passe de décodage : au-delà, on abandonne la passe
/// (et on le dit) plutôt que de bloquer la sonde.
const DELAI_MAX: Duration = Duration::from_secs(240);
/// Image dont la luminance est conservée pour comparer matériel et logiciel.
pub const IMAGE_TEMOIN: usize = 300;

pub fn sous_type(c: CodecFlux) -> GUID {
    match c {
        CodecFlux::Hevc => MFVideoFormat_HEVC,
        CodecFlux::H264 => MFVideoFormat_H264,
    }
}

#[derive(Clone)]
pub struct Candidat {
    pub activation: IMFActivate,
    pub nom: String,
    pub clsid: Option<GUID>,
    pub drapeaux: u32,
    pub url_materielle: Option<String>,
    pub fournisseur: Option<String>,
}

impl Candidat {
    pub fn asynchrone(&self) -> bool {
        self.drapeaux & MFT_ENUM_FLAG_ASYNCMFT.0 as u32 != 0
    }
    pub fn materiel(&self) -> bool {
        self.drapeaux & MFT_ENUM_FLAG_HARDWARE.0 as u32 != 0 || self.url_materielle.is_some()
    }
}

fn chaine(a: &IMFAttributes, cle: &GUID) -> Option<String> {
    let mut p = PWSTR::null();
    let mut n = 0u32;
    unsafe { a.GetAllocatedString(cle, &mut p, &mut n) }.ok()?;
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0 as _)) };
    s
}

pub fn texte_drapeaux(d: u32) -> String {
    let mut v = Vec::new();
    for (bit, nom) in [
        (0x1, "SYNCMFT"),
        (0x2, "ASYNCMFT"),
        (0x4, "HARDWARE"),
        (0x8, "FIELDOFUSE"),
        (0x10, "LOCALMFT"),
        (0x20, "TRANSCODE_ONLY"),
        (0x400, "UNTRUSTED_STOREMFT"),
    ] {
        if d & bit != 0 {
            v.push(nom);
        }
    }
    if v.is_empty() {
        "aucun drapeau (MFT synchrone ordinaire)".into()
    } else {
        v.join(" | ")
    }
}

pub fn enumerer(codec: CodecFlux, drapeaux: MFT_ENUM_FLAG) -> Result<Vec<Candidat>, String> {
    let info = MFT_REGISTER_TYPE_INFO { guidMajorType: MFMediaType_Video, guidSubtype: sous_type(codec) };
    let mut tableau: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut n = 0u32;
    unsafe { MFTEnumEx(MFT_CATEGORY_VIDEO_DECODER, drapeaux, Some(&info), None, &mut tableau, &mut n) }
        .etape("MFTEnumEx")?;
    let mut v = Vec::new();
    if !tableau.is_null() {
        let elements = unsafe { std::slice::from_raw_parts_mut(tableau, n as usize) };
        for e in elements.iter_mut() {
            if let Some(a) = e.take() {
                let attr: IMFAttributes = a.cast().etape("IMFActivate -> IMFAttributes")?;
                v.push(Candidat {
                    nom: chaine(&attr, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_else(|| "(sans nom)".into()),
                    clsid: unsafe { attr.GetGUID(&MFT_TRANSFORM_CLSID_Attribute) }.ok(),
                    drapeaux: unsafe { attr.GetUINT32(&MF_TRANSFORM_FLAGS_Attribute) }.unwrap_or(0),
                    url_materielle: chaine(&attr, &MFT_ENUM_HARDWARE_URL_Attribute),
                    fournisseur: chaine(&attr, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute),
                    activation: a,
                });
            }
        }
        unsafe { CoTaskMemFree(Some(tableau as _)) };
    }
    Ok(v)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Décodeur relié à un périphérique Direct3D 11 (DXVA).
    Materiel,
    /// Aucun périphérique : le décodeur travaille en mémoire système.
    Logiciel,
}

pub struct Resultat {
    pub entrees: usize,
    pub sorties: usize,
    pub duree: f64,
    pub cpu_ms: f64,
    pub sorties_dxgi: usize,
    pub sorties_memoire: usize,
    pub texture: Option<String>,
    pub retard_max: usize,
    pub activite: Option<Result<Vec<gpu::Activite>, String>>,
    pub temoin: Option<Vec<u8>>,
    pub interrompu: Option<String>,
    /// Numéro d'image (d'après l'horodatage) de chaque sortie, dans l'ordre.
    pub horodatages: Vec<i64>,
}

impl Resultat {
    /// Quelles images sont sorties : plages présentes, manquantes, désordre.
    pub fn images_rendues(&self, total: usize) -> String {
        if self.horodatages.is_empty() {
            return "aucun horodatage lu".into();
        }
        let mut vus = vec![false; total];
        let mut hors = 0;
        for &i in &self.horodatages {
            if i >= 0 && (i as usize) < total {
                vus[i as usize] = true;
            } else {
                hors += 1;
            }
        }
        let desordre = self.horodatages.windows(2).filter(|w| w[1] <= w[0]).count();
        let mut plages = Vec::new();
        let mut i = 0;
        while i < total {
            if !vus[i] {
                let d = i;
                while i < total && !vus[i] {
                    i += 1;
                }
                plages.push(if i - 1 == d { format!("{d}") } else { format!("{d}–{}", i - 1) });
            }
            i += 1;
        }
        let manquantes = if plages.is_empty() {
            "aucune image manquante".to_string()
        } else if plages.len() > 8 {
            format!("images manquantes en {} plages, dont {}", plages.len(), plages[..8].join(", "))
        } else {
            format!("images manquantes : {}", plages.join(", "))
        };
        format!(
            "{manquantes} ; {desordre} sortie(s) hors d'ordre ; {hors} horodatage(s) hors du flux",
        )
    }
}

fn temps_cpu() -> f64 {
    let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }.is_err() {
        return f64::NAN;
    }
    let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64;
    (v(k) + v(u)) / 10_000.0
}

fn echantillon(donnees: &[u8], i: usize) -> Result<IMFSample, String> {
    unsafe {
        let tampon = MFCreateMemoryBuffer(donnees.len() as u32).etape("MFCreateMemoryBuffer")?;
        let mut p: *mut u8 = std::ptr::null_mut();
        tampon.Lock(&mut p, None, None).etape("IMFMediaBuffer::Lock")?;
        std::ptr::copy_nonoverlapping(donnees.as_ptr(), p, donnees.len());
        tampon.Unlock().etape("IMFMediaBuffer::Unlock")?;
        tampon.SetCurrentLength(donnees.len() as u32).etape("SetCurrentLength")?;
        let e = MFCreateSample().etape("MFCreateSample")?;
        e.AddBuffer(&tampon).etape("IMFSample::AddBuffer")?;
        e.SetSampleTime(i as i64 * 166_667).etape("SetSampleTime")?;
        e.SetSampleDuration(166_667).etape("SetSampleDuration")?;
        Ok(e)
    }
}

/// Choisit NV12 en sortie. Rend la taille annoncée par le type de sortie.
fn type_sortie_nv12(t: &IMFTransform) -> Result<(u32, u32), String> {
    let mut vus = Vec::new();
    for i in 0.. {
        let ty = match unsafe { t.GetOutputAvailableType(0, i) } {
            Ok(ty) => ty,
            Err(_) => break,
        };
        let st = unsafe { ty.GetGUID(&MF_MT_SUBTYPE) }.unwrap_or_default();
        if st == MFVideoFormat_NV12 {
            unsafe { t.SetOutputType(0, &ty, 0) }.etape("SetOutputType(NV12)")?;
            let taille = unsafe { ty.GetUINT64(&MF_MT_FRAME_SIZE) }.unwrap_or(0);
            return Ok(((taille >> 32) as u32, taille as u32));
        }
        vus.push(format!("{st:?}"));
    }
    Err(format!("aucun type de sortie NV12 proposé (types vus : {})", vus.join(", ")))
}

struct Contexte<'a> {
    t: &'a IMFTransform,
    peripherique: Option<&'a ID3D11Device>,
    largeur: u32,
    hauteur: u32,
    info: MFT_OUTPUT_STREAM_INFO,
    res: Resultat,
}

impl Contexte<'_> {
    /// Tire une sortie. `Ok(false)` : le décodeur demande plus d'entrée.
    fn tirer(&mut self) -> Result<bool, String> {
        loop {
            let fournit = self.info.dwFlags
                & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0) as u32
                != 0;
            let mien = if fournit {
                None
            } else {
                let taille = if self.info.cbSize > 0 { self.info.cbSize } else { self.largeur * self.hauteur * 3 / 2 };
                let b = unsafe { MFCreateMemoryBuffer(taille) }.etape("MFCreateMemoryBuffer (sortie)")?;
                let e = unsafe { MFCreateSample() }.etape("MFCreateSample (sortie)")?;
                unsafe { e.AddBuffer(&b) }.etape("AddBuffer (sortie)")?;
                Some(e)
            };
            let mut tampon = MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(mien),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            };
            let mut statut = 0u32;
            let r = unsafe { self.t.ProcessOutput(0, std::slice::from_mut(&mut tampon), &mut statut) };
            let sortie = unsafe { ManuallyDrop::take(&mut tampon.pSample) };
            drop(unsafe { ManuallyDrop::take(&mut tampon.pEvents) });
            match r {
                Ok(()) => {
                    let e = sortie.ok_or("ProcessOutput a réussi sans rendre d'échantillon")?;
                    self.examiner(&e)?;
                    return Ok(true);
                }
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(false),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    let (l, h) = type_sortie_nv12(self.t)?;
                    if l > 0 && h > 0 {
                        self.largeur = l;
                        self.hauteur = h;
                    }
                    self.info = unsafe { self.t.GetOutputStreamInfo(0) }.etape("GetOutputStreamInfo")?;
                }
                Err(e) => return Err(format!("ProcessOutput : {}", err_win(&e))),
            }
        }
    }

    fn examiner(&mut self, e: &IMFSample) -> Result<(), String> {
        let indice = self.res.sorties;
        self.res.sorties += 1;
        if let Ok(t) = unsafe { e.GetSampleTime() } {
            self.res.horodatages.push(((t as f64) / 166_667.0).round() as i64);
        }
        let b = unsafe { e.GetBufferByIndex(0) }.etape("GetBufferByIndex(0)")?;
        if let Ok(dxgi) = b.cast::<IMFDXGIBuffer>() {
            self.res.sorties_dxgi += 1;
            if self.res.texture.is_none() || indice == IMAGE_TEMOIN {
                let mut brut: *mut core::ffi::c_void = std::ptr::null_mut();
                unsafe { dxgi.GetResource(&ID3D11Texture2D::IID, &mut brut) }.etape("IMFDXGIBuffer::GetResource")?;
                let tex = unsafe { ID3D11Texture2D::from_raw(brut) };
                let sous = unsafe { dxgi.GetSubresourceIndex() }.unwrap_or(0);
                let mut d = D3D11_TEXTURE2D_DESC::default();
                unsafe { tex.GetDesc(&mut d) };
                if self.res.texture.is_none() {
                    self.res.texture = Some(format!(
                        "texture Direct3D 11 {}x{}, format DXGI {}, tableau de {} tranche(s), BindFlags 0x{:X}",
                        d.Width, d.Height, d.Format.0, d.ArraySize, d.BindFlags
                    ));
                }
                if indice == IMAGE_TEMOIN {
                    self.res.temoin = Some(self.lire_texture(&tex, sous, &d)?);
                }
            }
        } else {
            self.res.sorties_memoire += 1;
            if indice == IMAGE_TEMOIN {
                self.res.temoin = Some(self.lire_memoire(&b)?);
            }
        }
        Ok(())
    }

    fn lire_texture(&self, tex: &ID3D11Texture2D, sous: u32, d: &D3D11_TEXTURE2D_DESC) -> Result<Vec<u8>, String> {
        let p = self.peripherique.ok_or("texture rendue sans périphérique")?;
        let desc = D3D11_TEXTURE2D_DESC {
            Width: d.Width,
            Height: d.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: d.Format,
            SampleDesc: d.SampleDesc,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut st: Option<ID3D11Texture2D> = None;
        unsafe { p.CreateTexture2D(&desc, None, Some(&mut st)) }.etape("CreateTexture2D (lecture)")?;
        let st = st.ok_or("texture de lecture nulle")?;
        let ctx = unsafe { p.GetImmediateContext() }.etape("GetImmediateContext")?;
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            ctx.CopySubresourceRegion(&st, 0, 0, 0, 0, tex, sous, None);
            ctx.Map(&st, 0, D3D11_MAP_READ, 0, Some(&mut m)).etape("Map (lecture)")?;
        }
        let (l, h) = (self.largeur.min(d.Width) as usize, self.hauteur.min(d.Height) as usize);
        let mut y = vec![0u8; l * h];
        for j in 0..h {
            let src = unsafe { std::slice::from_raw_parts((m.pData as *const u8).add(j * m.RowPitch as usize), l) };
            y[j * l..(j + 1) * l].copy_from_slice(src);
        }
        unsafe { ctx.Unmap(&st, 0) };
        Ok(y)
    }

    fn lire_memoire(&self, b: &IMFMediaBuffer) -> Result<Vec<u8>, String> {
        let (l, h) = (self.largeur as usize, self.hauteur as usize);
        let mut y = vec![0u8; l * h];
        if let Ok(b2) = b.cast::<IMF2DBuffer>() {
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut pas = 0i32;
            unsafe { b2.Lock2D(&mut p, &mut pas) }.etape("Lock2D")?;
            for j in 0..h {
                let src = unsafe { std::slice::from_raw_parts(p.offset(j as isize * pas as isize), l) };
                y[j * l..(j + 1) * l].copy_from_slice(src);
            }
            let _ = unsafe { b2.Unlock2D() };
        } else {
            let mut p: *mut u8 = std::ptr::null_mut();
            let mut n = 0u32;
            unsafe { b.Lock(&mut p, None, Some(&mut n)) }.etape("Lock (sortie)")?;
            let utile = (l * h).min(n as usize);
            y[..utile].copy_from_slice(unsafe { std::slice::from_raw_parts(p, utile) });
            let _ = unsafe { b.Unlock() };
        }
        Ok(y)
    }
}

/// Décode tout le flux une fois. Rend les étapes franchies dans `journal`.
pub fn decoder(
    c: &Candidat,
    flux: &Flux,
    mode: Mode,
    adaptateur: Option<&Adaptateur>,
    faible_latence: bool,
    journal: &mut Vec<String>,
) -> Result<Resultat, String> {
    let (largeur, hauteur) = flux.sps.as_ref().map(|s| (s.largeur, s.hauteur)).unwrap_or((2560, 1440));

    // Le périphérique d'abord : il doit survivre au décodeur.
    let mut peripherique: Option<ID3D11Device> = None;
    let mut gestionnaire: Option<IMFDXGIDeviceManager> = None;
    if mode == Mode::Materiel {
        let a = adaptateur.ok_or("aucun adaptateur matériel")?;
        let (p, _) = crate::dxva::creer_peripherique(a)?;
        let mt: ID3D11Multithread = p.cast().etape("ID3D11Multithread")?;
        let _ = unsafe { mt.SetMultithreadProtected(true) };
        let mut jeton = 0u32;
        unsafe { MFCreateDXGIDeviceManager(&mut jeton, &mut gestionnaire) }.etape("MFCreateDXGIDeviceManager")?;
        let g = gestionnaire.as_ref().ok_or("MFCreateDXGIDeviceManager sans gestionnaire")?;
        unsafe { g.ResetDevice(&p, jeton) }.etape("IMFDXGIDeviceManager::ResetDevice")?;
        journal.push(format!("périphérique Direct3D 11 créé sur « {} »", a.nom));
        peripherique = Some(p);
    }

    let t: IMFTransform = unsafe { c.activation.ActivateObject() }.etape("IMFActivate::ActivateObject")?;
    let resultat = (|| -> Result<Resultat, String> {
        let attributs = unsafe { t.GetAttributes() }.ok();
        let asynchrone = attributs
            .as_ref()
            .map(|a| unsafe { a.GetUINT32(&MF_TRANSFORM_ASYNC) }.unwrap_or(0) != 0)
            .unwrap_or(false);
        if let Some(a) = &attributs {
            let d3d11 = unsafe { a.GetUINT32(&MF_SA_D3D11_AWARE) };
            journal.push(match d3d11 {
                Ok(v) => format!("attribut MF_SA_D3D11_AWARE = {v}"),
                Err(e) => format!("attribut MF_SA_D3D11_AWARE absent ({})", err_win(&e)),
            });
            if asynchrone {
                unsafe { a.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1) }.etape("MF_TRANSFORM_ASYNC_UNLOCK")?;
                journal.push("MFT asynchrone déverrouillé".into());
            }
            let faible = faible_latence as u32;
            match unsafe { a.SetUINT32(&MF_LOW_LATENCY, faible) } {
                Ok(()) => journal.push(format!("MF_LOW_LATENCY = {faible} accepté")),
                Err(e) => journal.push(format!("MF_LOW_LATENCY = {faible} refusé : {}", err_win(&e))),
            }
        } else {
            journal.push("le décodeur n'expose pas d'attributs (GetAttributes échoue)".into());
        }

        if let Some(g) = &gestionnaire {
            match unsafe { t.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, g.as_raw() as usize) } {
                Ok(()) => journal.push("MFT_MESSAGE_SET_D3D_MANAGER : ACCEPTÉ par le décodeur".into()),
                Err(e) => {
                    return Err(format!(
                        "le décodeur REFUSE le périphérique Direct3D 11 (MFT_MESSAGE_SET_D3D_MANAGER) : {}",
                        err_win(&e)
                    ))
                }
            }
        }

        let ti = unsafe { MFCreateMediaType() }.etape("MFCreateMediaType")?;
        unsafe {
            ti.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).etape("MF_MT_MAJOR_TYPE")?;
            ti.SetGUID(&MF_MT_SUBTYPE, &sous_type(flux.codec)).etape("MF_MT_SUBTYPE")?;
            ti.SetUINT64(&MF_MT_FRAME_SIZE, (largeur as u64) << 32 | hauteur as u64).etape("MF_MT_FRAME_SIZE")?;
            ti.SetUINT64(&MF_MT_FRAME_RATE, 60u64 << 32 | 1).etape("MF_MT_FRAME_RATE")?;
            ti.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).etape("MF_MT_INTERLACE_MODE")?;
            t.SetInputType(0, &ti, 0).etape("SetInputType")?;
        }
        journal.push(format!("type d'entrée {} {largeur}x{hauteur} accepté", flux.codec.nom()));
        let (l, h) = type_sortie_nv12(&t)?;
        journal.push(format!("type de sortie NV12 accepté ({l}x{h})"));
        let info = unsafe { t.GetOutputStreamInfo(0) }.etape("GetOutputStreamInfo")?;
        journal.push(format!(
            "flux de sortie : drapeaux 0x{:X} ({}), taille d'échantillon {}",
            info.dwFlags,
            if info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0 {
                "le décodeur fournit ses échantillons"
            } else if info.dwFlags & MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32 != 0 {
                "le décodeur peut fournir ses échantillons"
            } else {
                "la sonde alloue les échantillons en mémoire système"
            },
            info.cbSize
        ));

        let mut ctx = Contexte {
            t: &t,
            peripherique: peripherique.as_ref(),
            largeur: if l > 0 { l } else { largeur },
            hauteur: if h > 0 { h } else { hauteur },
            info,
            res: Resultat {
                entrees: 0,
                sorties: 0,
                duree: 0.0,
                cpu_ms: 0.0,
                sorties_dxgi: 0,
                sorties_memoire: 0,
                texture: None,
                retard_max: 0,
                activite: None,
                temoin: None,
                interrompu: None,
                horodatages: Vec::new(),
            },
        };
        // Les échantillons d'entrée sont préparés hors chronomètre.
        let entrees: Vec<IMFSample> = flux
            .unites
            .iter()
            .enumerate()
            .map(|(i, (d, n))| echantillon(&flux.donnees[*d..*d + *n], i))
            .collect::<Result<_, _>>()?;

        let compteurs = adaptateur.map(|a| gpu::Compteurs::ouvrir(&a.motif_luid()));
        unsafe {
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0).etape("NOTIFY_BEGIN_STREAMING")?;
            t.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0).etape("NOTIFY_START_OF_STREAM")?;
        }
        let cpu0 = temps_cpu();
        let t0 = Instant::now();
        let deroulement = if asynchrone {
            boucle_asynchrone(&mut ctx, &entrees, t0)
        } else {
            boucle_synchrone(&mut ctx, &entrees, t0)
        };
        if let Err(e) = deroulement {
            ctx.res.interrompu = Some(e);
        }
        // Attendre que le GPU ait fini ce qu'on lui a confié avant d'arrêter le chronomètre.
        if let Some(p) = ctx.peripherique {
            attendre_gpu(p);
        }
        ctx.res.duree = t0.elapsed().as_secs_f64();
        ctx.res.cpu_ms = temps_cpu() - cpu0;
        let _ = unsafe { t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0) };
        ctx.res.activite = compteurs.map(|c| c.and_then(|c| c.bilan()));
        Ok(ctx.res)
    })();
    drop(t);
    let _ = unsafe { c.activation.ShutdownObject() };
    drop(gestionnaire);
    drop(peripherique);
    resultat
}

fn suivre_retard(ctx: &mut Contexte) {
    let r = ctx.res.entrees.saturating_sub(ctx.res.sorties);
    ctx.res.retard_max = ctx.res.retard_max.max(r);
}

fn boucle_synchrone(ctx: &mut Contexte, entrees: &[IMFSample], t0: Instant) -> Result<(), String> {
    for (i, e) in entrees.iter().enumerate() {
        if t0.elapsed() > DELAI_MAX {
            return Err(format!("délai de {} s dépassé à l'image {i}", DELAI_MAX.as_secs()));
        }
        let mut refus = 0;
        loop {
            match unsafe { ctx.t.ProcessInput(0, e, 0) } {
                Ok(()) => break,
                Err(err) if err.code() == MF_E_NOTACCEPTING => {
                    refus += 1;
                    if refus > 1000 {
                        return Err(format!("image {i} : le décodeur refuse l'entrée sans rien rendre"));
                    }
                    while ctx.tirer()? {}
                }
                Err(err) => return Err(format!("ProcessInput (image {i}) : {}", err_win(&err))),
            }
        }
        ctx.res.entrees += 1;
        suivre_retard(ctx);
        while ctx.tirer()? {}
    }
    unsafe { ctx.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0) }.etape("NOTIFY_END_OF_STREAM")?;
    unsafe { ctx.t.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0) }.etape("COMMAND_DRAIN")?;
    while ctx.tirer()? {}
    Ok(())
}

/// Modèle à événements des MFT asynchrones (en général matériels).
fn boucle_asynchrone(ctx: &mut Contexte, entrees: &[IMFSample], t0: Instant) -> Result<(), String> {
    let gen: IMFMediaEventGenerator = ctx.t.cast().etape("IMFMediaEventGenerator")?;
    let mut prochaine = 0usize;
    let mut vidange = false;
    let mut dernier_evenement = Instant::now();
    loop {
        if t0.elapsed() > DELAI_MAX {
            return Err(format!("délai de {} s dépassé ({} images entrées)", DELAI_MAX.as_secs(), prochaine));
        }
        let ev = match unsafe { gen.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
            Ok(ev) => ev,
            Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => {
                if dernier_evenement.elapsed() > Duration::from_secs(10) {
                    return Err(format!("aucun événement du décodeur depuis 10 s ({prochaine} images entrées)"));
                }
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(e) => return Err(format!("GetEvent : {}", err_win(&e))),
        };
        dernier_evenement = Instant::now();
        let ty = unsafe { ev.GetType() }.etape("IMFMediaEvent::GetType")?;
        if ty == METransformNeedInput.0 as u32 {
            if prochaine < entrees.len() {
                unsafe { ctx.t.ProcessInput(0, &entrees[prochaine], 0) }
                    .map_err(|e| format!("ProcessInput (image {prochaine}) : {}", err_win(&e)))?;
                prochaine += 1;
                ctx.res.entrees += 1;
                suivre_retard(ctx);
            } else if !vidange {
                vidange = true;
                unsafe { ctx.t.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0) }.etape("NOTIFY_END_OF_STREAM")?;
                unsafe { ctx.t.ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0) }.etape("COMMAND_DRAIN")?;
            }
        } else if ty == METransformHaveOutput.0 as u32 {
            ctx.tirer()?;
        } else if ty == METransformDrainComplete.0 as u32 {
            return Ok(());
        } else if ty == MEError.0 as u32 {
            let s = unsafe { ev.GetStatus() }.map(|h| format!("0x{:08X}", h.0 as u32)).unwrap_or_default();
            return Err(format!("le décodeur signale une erreur (MEError, statut {s})"));
        }
    }
}

fn attendre_gpu(p: &ID3D11Device) {
    let desc = D3D11_QUERY_DESC { Query: D3D11_QUERY_EVENT, MiscFlags: 0 };
    let mut q: Option<ID3D11Query> = None;
    if unsafe { p.CreateQuery(&desc, Some(&mut q)) }.is_err() {
        return;
    }
    let Some(q) = q else { return };
    let Ok(ctx) = (unsafe { p.GetImmediateContext() }) else { return };
    unsafe { ctx.End(&q) };
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(5) {
        let mut fini = windows::core::BOOL(0);
        let _ = unsafe { ctx.GetData(&q, Some((&mut fini as *mut windows::core::BOOL).cast()), 4, 0) };
        if fini.as_bool() {
            return;
        }
        std::thread::yield_now();
    }
}
