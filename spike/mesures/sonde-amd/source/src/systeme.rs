//! Le contexte de la machine : système, processeur, alimentation, adaptateurs,
//! et les paquets du Microsoft Store qui portent des codecs.

use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{ERROR_SUCCESS, LUID};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, IDXGIAdapter1, IDXGIDevice, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows::Win32::System::SystemInformation::GetLocalTime;

use crate::rapport::{err_win, Rapport};

pub struct Adaptateur {
    pub interface: IDXGIAdapter1,
    pub nom: String,
    pub logiciel: bool,
    pub luid: LUID,
}

impl Adaptateur {
    /// Fragment du nom d'instance des compteurs « GPU Engine » pour cet adaptateur.
    pub fn motif_luid(&self) -> String {
        format!("luid_0x{:08x}_0x{:08x}", self.luid.HighPart as u32, self.luid.LowPart)
    }
}

fn texte_registre(cle: HKEY, chemin: &str, valeur: &str) -> Option<String> {
    let c: Vec<u16> = chemin.encode_utf16().chain([0]).collect();
    let v: Vec<u16> = valeur.encode_utf16().chain([0]).collect();
    let mut tampon = vec![0u16; 512];
    let mut taille = (tampon.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            cle,
            PCWSTR(c.as_ptr()),
            PCWSTR(v.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(tampon.as_mut_ptr().cast()),
            Some(&mut taille),
        )
    };
    if r != ERROR_SUCCESS {
        return None;
    }
    let n = (taille as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&tampon[..n]).trim_end_matches('\0').trim().to_string())
}

fn mot_registre(cle: HKEY, chemin: &str, valeur: &str) -> Option<u32> {
    let c: Vec<u16> = chemin.encode_utf16().chain([0]).collect();
    let v: Vec<u16> = valeur.encode_utf16().chain([0]).collect();
    let mut d = 0u32;
    let mut taille = 4u32;
    let r = unsafe {
        RegGetValueW(
            cle,
            PCWSTR(c.as_ptr()),
            PCWSTR(v.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut d as *mut u32).cast()),
            Some(&mut taille),
        )
    };
    (r == ERROR_SUCCESS).then_some(d)
}

pub fn decrire_machine(r: &mut Rapport) {
    let t = unsafe { GetLocalTime() };
    r.ligne(format!(
        "Date de l'exécution : {:02}/{:02}/{} {:02}:{:02}:{:02} (heure locale)",
        t.wDay, t.wMonth, t.wYear, t.wHour, t.wMinute, t.wSecond
    ));
    let cv = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let produit = texte_registre(HKEY_LOCAL_MACHINE, cv, "ProductName").unwrap_or_else(|| "?".into());
    let version = texte_registre(HKEY_LOCAL_MACHINE, cv, "DisplayVersion").unwrap_or_else(|| "?".into());
    let build = texte_registre(HKEY_LOCAL_MACHINE, cv, "CurrentBuild").unwrap_or_else(|| "?".into());
    let ubr = mot_registre(HKEY_LOCAL_MACHINE, cv, "UBR").map(|u| u.to_string()).unwrap_or_else(|| "?".into());
    r.ligne(format!(
        "Système : {produit} {version}, build {build}.{ubr} (ProductName vaut « Windows 10 » sur bien des Windows 11 : seul le numéro de build fait foi)"
    ));
    let cpu = texte_registre(
        HKEY_LOCAL_MACHINE,
        "HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0",
        "ProcessorNameString",
    )
    .unwrap_or_else(|| "?".into());
    let coeurs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0);
    r.ligne(format!("Processeur : {cpu} ({coeurs} processeurs logiques)"));

    let mut p = SYSTEM_POWER_STATUS::default();
    match unsafe { GetSystemPowerStatus(&mut p) } {
        Ok(()) => {
            let secteur = match p.ACLineStatus {
                0 => "NON — sur batterie : les cadences peuvent être bridées",
                1 => "oui",
                _ => "inconnu",
            };
            r.ligne(format!("Branché sur secteur : {secteur}"));
        }
        Err(e) => r.ligne(format!("Branché sur secteur : inconnu ({})", err_win(&e))),
    }
}

/// Paquets du Store dont le nom évoque une extension vidéo. Indice tiré du
/// registre de l'utilisateur, pas une preuve que le codec fonctionne : la
/// preuve, c'est l'énumération Media Foundation puis le décodage réel.
pub fn paquets_codecs(r: &mut Rapport) {
    let chemin = "Software\\Classes\\Local Settings\\Software\\Microsoft\\Windows\\CurrentVersion\\AppModel\\Repository\\Packages";
    let c: Vec<u16> = chemin.encode_utf16().chain([0]).collect();
    let mut cle = HKEY::default();
    let ouvert = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(c.as_ptr()), Some(0), KEY_READ, &mut cle) };
    if ouvert != ERROR_SUCCESS {
        r.ligne(format!("  (registre des paquets illisible : code {})", ouvert.0));
        return;
    }
    let mut trouves = Vec::new();
    let mut i = 0u32;
    loop {
        let mut nom = vec![0u16; 512];
        let mut taille = nom.len() as u32;
        let res = unsafe {
            RegEnumKeyExW(cle, i, Some(windows::core::PWSTR(nom.as_mut_ptr())), &mut taille, None, None, None, None)
        };
        if res != ERROR_SUCCESS {
            break;
        }
        let n = String::from_utf16_lossy(&nom[..taille as usize]);
        let bas = n.to_lowercase();
        if bas.contains("videoextension") || bas.contains("hevc") || bas.contains("av1") || bas.contains("vp9") {
            trouves.push(n);
        }
        i += 1;
    }
    unsafe {
        let _ = RegCloseKey(cle);
    }
    if trouves.is_empty() {
        r.ligne("  Aucun paquet d'extension vidéo trouvé dans le registre de l'utilisateur.");
    } else {
        for n in trouves {
            r.ligne(format!("  Paquet installé : {n}"));
        }
    }
}

pub fn adaptateurs(r: &mut Rapport) -> Vec<Adaptateur> {
    let fabrique: IDXGIFactory1 = match unsafe { CreateDXGIFactory1() } {
        Ok(f) => f,
        Err(e) => {
            r.echec("CreateDXGIFactory1", &err_win(&e));
            return Vec::new();
        }
    };
    let mut v = Vec::new();
    for i in 0.. {
        let a = match unsafe { fabrique.EnumAdapters1(i) } {
            Ok(a) => a,
            Err(_) => break,
        };
        let d = match unsafe { a.GetDesc1() } {
            Ok(d) => d,
            Err(e) => {
                r.echec(&format!("GetDesc1 de l'adaptateur {i}"), &err_win(&e));
                continue;
            }
        };
        let nom = String::from_utf16_lossy(&d.Description).trim_end_matches('\0').trim().to_string();
        let logiciel = (d.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0;
        let fournisseur = match d.VendorId {
            0x1002 | 0x1022 => "AMD",
            0x10DE => "NVIDIA",
            0x8086 => "Intel",
            0x1414 => "Microsoft",
            _ => "autre",
        };
        let pilote = match unsafe { a.CheckInterfaceSupport(&IDXGIDevice::IID) } {
            Ok(v) => {
                let v = v as u64;
                format!("{}.{}.{}.{}", v >> 48, (v >> 32) & 0xffff, (v >> 16) & 0xffff, v & 0xffff)
            }
            Err(e) => format!("inconnue ({})", err_win(&e)),
        };
        r.ligne(format!(
            "Adaptateur {i} : {nom} — {fournisseur} (VEN_{:04X} DEV_{:04X} REV_{:02X}), pilote {pilote}, \
             mémoire dédiée {} Mo, partagée {} Mo{}",
            d.VendorId,
            d.DeviceId,
            d.Revision,
            d.DedicatedVideoMemory / (1024 * 1024),
            d.SharedSystemMemory / (1024 * 1024),
            if logiciel { " — ADAPTATEUR LOGICIEL, ignoré" } else { "" }
        ));
        v.push(Adaptateur { interface: a, nom, logiciel, luid: d.AdapterLuid });
    }
    v
}
