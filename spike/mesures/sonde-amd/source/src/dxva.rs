//! Question 1 : ce que la puce déclare via l'API vidéo Direct3D 11.

use windows::core::{Interface, GUID};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11VideoDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_VIDEO_DECODER_CONFIG,
    D3D11_VIDEO_DECODER_DESC,
};
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::rapport::{err_win, Rapport};
use crate::systeme::Adaptateur;

/// Tailles demandées, dans l'ordre. 2560x2880 est la porteuse « empaquetée »
/// d'un écran 2560x1440 : c'est la taille décisive.
pub const TAILLES: [(u32, u32); 6] = [
    (1920, 1080),
    (2560, 1440),
    (2560, 2880),
    (3840, 2160),
    (4096, 2304),
    (4096, 4096),
];

/// GUID -> nom, d'après `dxva.h` du Windows SDK 10.0.26100.0.
fn nom_profil(g: &GUID) -> Option<&'static str> {
    let b = |d1: u32, d2: u16, d3: u16, d4: [u8; 8]| GUID::from_values(d1, d2, d3, d4);
    let dxva1 = |d1: u32| b(d1, 0xa0c7, 0x11d3, [0xb9, 0x84, 0x00, 0xc0, 0x4f, 0x2e, 0x73, 0xc5]);
    let table: [(GUID, &'static str); 46] = [
        (dxva1(0x1b81be01), "H.261 A"),
        (dxva1(0x1b81be02), "H.261 B"),
        (dxva1(0x1b81be09), "MPEG-1 A"),
        (dxva1(0x1b81be0a), "MPEG-2 A (MoComp)"),
        (dxva1(0x1b81be0b), "MPEG-2 B (IDCT)"),
        (dxva1(0x1b81be0c), "MPEG-2 C"),
        (dxva1(0x1b81be0d), "MPEG-2 D"),
        (dxva1(0x1b81be64), "H.264 A (MoComp, sans FGT)"),
        (dxva1(0x1b81be65), "H.264 B (MoComp, FGT)"),
        (dxva1(0x1b81be66), "H.264 C (IDCT, sans FGT)"),
        (dxva1(0x1b81be67), "H.264 D (IDCT, FGT)"),
        (dxva1(0x1b81be68), "H.264 VLD (sans FGT) — le profil H.264 usuel"),
        (dxva1(0x1b81be69), "H.264 VLD (FGT)"),
        (dxva1(0x1b81be80), "WMV8 PostProc"),
        (dxva1(0x1b81be81), "WMV8 MoComp"),
        (dxva1(0x1b81be90), "WMV9 PostProc"),
        (dxva1(0x1b81be91), "WMV9 MoComp"),
        (dxva1(0x1b81be94), "WMV9 IDCT"),
        (dxva1(0x1b81bea0), "VC-1 PostProc"),
        (dxva1(0x1b81bea1), "VC-1 MoComp"),
        (dxva1(0x1b81bea2), "VC-1 IDCT"),
        (dxva1(0x1b81bea3), "VC-1 VLD"),
        (dxva1(0x1b81bea4), "VC-1 VLD 2010"),
        (b(0x6f3ec719, 0x3735, 0x42cc, [0x80, 0x63, 0x65, 0xcc, 0x3c, 0xb3, 0x66, 0x16]), "MPEG-1 VLD"),
        (b(0x86695f12, 0x340e, 0x4f04, [0x9f, 0xd3, 0x92, 0x53, 0xdd, 0x32, 0x74, 0x60]), "MPEG-2 et MPEG-1 VLD"),
        (b(0xee27417f, 0x5e28, 0x4e65, [0xbe, 0xea, 0x1d, 0x26, 0xb5, 0x08, 0xad, 0xc9]), "MPEG-2 VLD"),
        (b(0xd5f04ff9, 0x3418, 0x45d8, [0x95, 0x61, 0x32, 0xa7, 0x6a, 0xae, 0x2d, 0xdd]), "H.264 VLD avec FMO/ASO"),
        (b(0xd79be8da, 0x0cf1, 0x4c81, [0xb8, 0x2a, 0x69, 0xa4, 0xe2, 0x36, 0xf4, 0x3d]), "H.264 VLD stéréo progressif"),
        (b(0xf9aaccbb, 0xc2b6, 0x4cfc, [0x87, 0x79, 0x57, 0x07, 0xb1, 0x76, 0x05, 0x52]), "H.264 VLD stéréo"),
        (b(0x705b9d82, 0x76cf, 0x49d6, [0xb7, 0xe6, 0xac, 0x88, 0x72, 0xdb, 0x01, 0x3c]), "H.264 VLD multivue"),
        (b(0xefd64d74, 0xc9e8, 0x41d7, [0xa5, 0xe9, 0xe9, 0xb0, 0xe3, 0x9f, 0xa3, 0x19]), "MPEG-4 Part 2 VLD Simple"),
        (b(0xed418a9f, 0x010d, 0x4eda, [0x9a, 0xe3, 0x9a, 0x65, 0x35, 0x8d, 0x8d, 0x2e]), "MPEG-4 Part 2 VLD Advanced Simple sans GMC"),
        (b(0x5b11d51b, 0x2f4c, 0x4452, [0xbc, 0xc3, 0x09, 0xf2, 0xa1, 0x16, 0x0c, 0xc0]), "HEVC Main (8 bits 4:2:0)"),
        (b(0x107af0e0, 0xef1a, 0x4d19, [0xab, 0xa8, 0x67, 0xa1, 0x63, 0x07, 0x3d, 0x13]), "HEVC Main 10 (10 bits 4:2:0)"),
        (b(0x0685b993, 0x3d8c, 0x43a0, [0x8b, 0x28, 0xd7, 0x4c, 0x2d, 0x68, 0x99, 0xa4]), "HEVC Monochrome"),
        (b(0x142a1d0f, 0x69dd, 0x4ec9, [0x85, 0x91, 0xb1, 0x2f, 0xfc, 0xb9, 0x1a, 0x29]), "HEVC Monochrome 10"),
        (b(0x1a72925f, 0x0c2c, 0x4f15, [0x96, 0xfb, 0xb1, 0x7d, 0x14, 0x73, 0x60, 0x3f]), "HEVC Main 12"),
        (b(0x0bac4fe5, 0x1532, 0x4429, [0xa8, 0x54, 0xf8, 0x4d, 0xe0, 0x49, 0x53, 0xdb]), "HEVC Main 10 4:2:2"),
        (b(0x55bcac81, 0xf311, 0x4093, [0xa7, 0xd0, 0x1c, 0xbc, 0x0b, 0x84, 0x9b, 0xee]), "HEVC Main 12 4:2:2"),
        (b(0x4008018f, 0xf537, 0x4b36, [0x98, 0xcf, 0x61, 0xaf, 0x8a, 0x2c, 0x1a, 0x33]), "HEVC Main 4:4:4 (8 bits)"),
        (b(0x9cc55490, 0xe37c, 0x4932, [0x86, 0x84, 0x49, 0x20, 0xf9, 0xf6, 0x40, 0x9c]), "HEVC Main 10 Ext"),
        (b(0x0dabeffa, 0x4458, 0x4602, [0xbc, 0x03, 0x07, 0x95, 0x65, 0x9d, 0x61, 0x7c]), "HEVC Main 10 4:4:4"),
        (b(0x9798634d, 0xfe9d, 0x48e5, [0xb4, 0xda, 0xdb, 0xec, 0x45, 0xb3, 0xdf, 0x01]), "HEVC Main 12 4:4:4"),
        (b(0xa4fbdbb0, 0xa113, 0x482b, [0xa2, 0x32, 0x63, 0x5c, 0xc0, 0x69, 0x7f, 0x6d]), "HEVC Main 16"),
        (b(0x463707f8, 0xa1d0, 0x4585, [0x87, 0x6d, 0x83, 0xaa, 0x6d, 0x60, 0xb8, 0x9e]), "VP9 Profile 0 (8 bits)"),
        (b(0xa4c749ef, 0x6ecf, 0x48aa, [0x84, 0x48, 0x50, 0xa7, 0xa1, 0x16, 0x5f, 0xf7]), "VP9 Profile 2 (10 bits)"),
    ];
    let autres: [(GUID, &'static str); 13] = [
        (b(0x90b899ea, 0x3a62, 0x4705, [0x88, 0xb3, 0x8d, 0xf0, 0x4b, 0x27, 0x44, 0xe7]), "VP8"),
        (b(0xb8be4ccb, 0xcf53, 0x46ba, [0x8d, 0x59, 0xd6, 0xb8, 0xa6, 0xda, 0x5d, 0x2a]), "AV1 Profile 0 (Main)"),
        (b(0x6936ff0f, 0x45b1, 0x4163, [0x9c, 0xc1, 0x64, 0x6e, 0xf6, 0x94, 0x61, 0x08]), "AV1 Profile 1 (High, 4:4:4)"),
        (b(0x0c5f2aa1, 0xe541, 0x4089, [0xbb, 0x7b, 0x98, 0x11, 0x0a, 0x19, 0xd7, 0xc8]), "AV1 Profile 2 (Professional)"),
        (b(0x17127009, 0xa00f, 0x4ce1, [0x99, 0x4e, 0xbf, 0x40, 0x81, 0xf6, 0xf3, 0xf0]), "AV1 Profile 2 12 bits"),
        (b(0x2d80bed6, 0x9cac, 0x4835, [0x9e, 0x91, 0x32, 0x7b, 0xbc, 0x4f, 0x9e, 0xe8]), "AV1 Profile 2 12 bits 4:2:0"),
        (b(0x725cb506, 0x0c29, 0x43c4, [0x94, 0x40, 0x8e, 0x93, 0x97, 0x90, 0x3a, 0x04]), "MJPEG 4:2:0"),
        (b(0x5b77b9cd, 0x1a35, 0x4c30, [0x9f, 0xd8, 0xef, 0x4b, 0x60, 0xc0, 0x35, 0xdd]), "MJPEG 4:2:2"),
        (b(0xd95161f9, 0x0d44, 0x47e6, [0xbc, 0xf5, 0x1b, 0xfb, 0xfb, 0x26, 0x8f, 0x97]), "MJPEG 4:4:4"),
        (b(0xc91748d5, 0xfd18, 0x4aca, [0x9d, 0xb3, 0x3a, 0x66, 0x34, 0xab, 0x54, 0x7d]), "MJPEG 4:4:4:4"),
        (b(0xcf782c83, 0xbef5, 0x4a2c, [0x87, 0xcb, 0x60, 0x19, 0xe7, 0xb1, 0x75, 0xac]), "JPEG 4:2:0"),
        (b(0xf04df417, 0xeee2, 0x4067, [0xa7, 0x78, 0xf3, 0x5c, 0x15, 0xab, 0x97, 0x21]), "JPEG 4:2:2"),
        (b(0x4cd00e17, 0x89ba, 0x48ef, [0xb9, 0xf9, 0xed, 0xcb, 0x82, 0x71, 0x3f, 0x65]), "JPEG 4:4:4"),
    ];
    table.iter().chain(autres.iter()).find(|(k, _)| k == g).map(|(_, n)| *n)
}

const FORMATS: [(DXGI_FORMAT, &str); 10] = [
    (DXGI_FORMAT_NV12, "NV12"),
    (DXGI_FORMAT_P010, "P010"),
    (DXGI_FORMAT_P016, "P016"),
    (DXGI_FORMAT_AYUV, "AYUV"),
    (DXGI_FORMAT_Y410, "Y410"),
    (DXGI_FORMAT_Y416, "Y416"),
    (DXGI_FORMAT_YUY2, "YUY2"),
    (DXGI_FORMAT_Y210, "Y210"),
    (DXGI_FORMAT_Y216, "Y216"),
    (DXGI_FORMAT_420_OPAQUE, "420_OPAQUE"),
];

pub fn creer_peripherique(a: &Adaptateur) -> Result<(ID3D11Device, D3D_FEATURE_LEVEL), String> {
    let mut d: Option<ID3D11Device> = None;
    let mut niveau = D3D_FEATURE_LEVEL::default();
    unsafe {
        D3D11CreateDevice(
            &a.interface,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut d),
            Some(&mut niveau),
            None,
        )
    }
    .map_err(|e| format!("D3D11CreateDevice (avec D3D11_CREATE_DEVICE_VIDEO_SUPPORT) : {}", err_win(&e)))?;
    d.map(|d| (d, niveau)).ok_or_else(|| "D3D11CreateDevice n'a rendu aucun périphérique".into())
}

/// Résumé d'une taille pour la synthèse : (profil, taille, verdict court).
pub type Ligne = (String, (u32, u32), String);

pub fn sonder(r: &mut Rapport, a: &Adaptateur) -> Vec<Ligne> {
    let mut synthese = Vec::new();
    let (peripherique, niveau) = match creer_peripherique(a) {
        Ok(x) => x,
        Err(e) => {
            r.echec(&format!("{} : création du périphérique", a.nom), &e);
            return synthese;
        }
    };
    r.ligne(format!("Périphérique Direct3D 11 créé, niveau de fonctionnalité 0x{:X}", niveau.0));
    let video: ID3D11VideoDevice = match peripherique.cast() {
        Ok(v) => v,
        Err(e) => {
            r.echec(&format!("{} : ID3D11VideoDevice", a.nom), &err_win(&e));
            return synthese;
        }
    };
    let n = unsafe { video.GetVideoDecoderProfileCount() };
    r.mesure(&format!("{} déclare {n} profil(s) de décodage (GetVideoDecoderProfileCount)", a.nom));

    let mut a_tester = Vec::new();
    for i in 0..n {
        let g = match unsafe { video.GetVideoDecoderProfile(i) } {
            Ok(g) => g,
            Err(e) => {
                r.echec(&format!("GetVideoDecoderProfile({i})"), &err_win(&e));
                continue;
            }
        };
        let nom = nom_profil(&g).map(str::to_string).unwrap_or_else(|| "inconnu (GUID propre au fabricant ?)".into());
        let mut formats = Vec::new();
        for (f, fnom) in FORMATS {
            match unsafe { video.CheckVideoDecoderFormat(&g, f) } {
                Ok(b) if b.as_bool() => formats.push(fnom),
                _ => {}
            }
        }
        r.ligne(format!(
            "  Profil {i:>2} : {nom}\r\n             GUID {{{g:?}}}\r\n             formats de sortie acceptés : {}",
            if formats.is_empty() { "aucun parmi ceux testés".to_string() } else { formats.join(", ") }
        ));
        if nom.starts_with("HEVC") || nom.starts_with("H.264") {
            let f = FORMATS.iter().find(|(_, fnom)| formats.contains(fnom)).map(|(f, fnom)| (*f, *fnom));
            a_tester.push((g, nom, f));
        }
    }

    r.sous_titre(&format!("Tailles acceptées par profil HEVC et H.264 ({})", a.nom));
    r.ligne("  Méthode : GetVideoDecoderConfigCount sur une description de décodeur, puis, si au moins une");
    r.ligne("  configuration existe, CreateVideoDecoder avec la première — la création effective est la preuve.");
    for (g, nom, f) in a_tester {
        let Some((format, fnom)) = f else {
            r.ligne(format!("  {nom} : aucun format de sortie testé n'est accepté, tailles non testées"));
            continue;
        };
        r.ligne(format!("  {nom} (sortie {fnom}) :"));
        for (l, h) in TAILLES {
            let desc = D3D11_VIDEO_DECODER_DESC { Guid: g, SampleWidth: l, SampleHeight: h, OutputFormat: format };
            let verdict = match unsafe { video.GetVideoDecoderConfigCount(&desc) } {
                Err(e) => format!("REFUSÉE — GetVideoDecoderConfigCount : {}", err_win(&e)),
                Ok(0) => "REFUSÉE — 0 configuration".to_string(),
                Ok(c) => {
                    let mut cfg = D3D11_VIDEO_DECODER_CONFIG::default();
                    match unsafe { video.GetVideoDecoderConfig(&desc, 0, &mut cfg) } {
                        Err(e) => format!("{c} configuration(s), mais GetVideoDecoderConfig(0) échoue : {}", err_win(&e)),
                        Ok(()) => match unsafe { video.CreateVideoDecoder(&desc, &cfg) } {
                            Ok(_) => format!("ACCEPTÉE — {c} configuration(s), décodeur créé"),
                            Err(e) => format!("{c} configuration(s), mais CreateVideoDecoder échoue : {}", err_win(&e)),
                        },
                    }
                }
            };
            let marque = if (l, h) == (2560, 2880) { "  <== taille décisive" } else { "" };
            r.ligne(format!("      {l:>4}x{h:<4} : {verdict}{marque}"));
            synthese.push((nom.clone(), (l, h), verdict));
        }
    }
    synthese
}
