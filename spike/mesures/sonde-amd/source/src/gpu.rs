//! Témoin indépendant du décodage matériel : les compteurs « GPU Engine » de
//! Windows (ceux du Gestionnaire des tâches), filtrés sur CE processus et sur
//! l'adaptateur testé. Si le moteur vidéo de la puce a travaillé pour nous,
//! son temps d'activité a augmenté ; s'il est resté à zéro, le décodage n'est
//! pas passé par lui, quoi qu'en disent les autres indices.

use std::collections::BTreeMap;

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::System::Performance::*;

pub struct Compteurs {
    requete: PDH_HQUERY,
    temps: PDH_HCOUNTER,
    utilisation: PDH_HCOUNTER,
    pid: u32,
    motif_luid: String,
    avant: BTreeMap<String, i64>,
}

/// Activité d'un type de moteur pendant la mesure.
pub struct Activite {
    pub moteur: String,
    /// Écart brut du compteur « Running Time », sommé sur les moteurs de ce type.
    pub temps_brut: i64,
    /// « Utilization Percentage » sur l'intervalle, max sur les moteurs de ce type.
    pub utilisation: f64,
}

fn statut(nom: &str, s: u32) -> Result<(), String> {
    if s == 0 {
        Ok(())
    } else {
        Err(format!("{nom} : code PDH 0x{s:08X}"))
    }
}

fn texte(p: PWSTR) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { p.to_string() }.unwrap_or_default()
    }
}

impl Compteurs {
    pub fn ouvrir(motif_luid: &str) -> Result<Self, String> {
        let mut requete = PDH_HQUERY::default();
        statut("PdhOpenQueryW", unsafe { PdhOpenQueryW(PCWSTR::null(), 0, &mut requete) })?;
        let mut temps = PDH_HCOUNTER::default();
        let mut utilisation = PDH_HCOUNTER::default();
        let c1: Vec<u16> = "\\GPU Engine(*)\\Running Time".encode_utf16().chain([0]).collect();
        let c2: Vec<u16> = "\\GPU Engine(*)\\Utilization Percentage".encode_utf16().chain([0]).collect();
        let s = unsafe { PdhAddEnglishCounterW(requete, PCWSTR(c1.as_ptr()), 0, &mut temps) };
        if s != 0 {
            unsafe { PdhCloseQuery(requete) };
            return Err(format!("PdhAddEnglishCounterW(GPU Engine Running Time) : code PDH 0x{s:08X}"));
        }
        let s = unsafe { PdhAddEnglishCounterW(requete, PCWSTR(c2.as_ptr()), 0, &mut utilisation) };
        if s != 0 {
            unsafe { PdhCloseQuery(requete) };
            return Err(format!("PdhAddEnglishCounterW(GPU Engine Utilization) : code PDH 0x{s:08X}"));
        }
        let mut c = Self {
            requete,
            temps,
            utilisation,
            pid: std::process::id(),
            motif_luid: motif_luid.to_lowercase(),
            avant: BTreeMap::new(),
        };
        statut("PdhCollectQueryData (premier relevé)", unsafe { PdhCollectQueryData(requete) })?;
        c.avant = c.temps_bruts()?;
        Ok(c)
    }

    fn nous_concerne(&self, nom: &str) -> bool {
        let n = nom.to_lowercase();
        n.starts_with(&format!("pid_{}_", self.pid)) && n.contains(&self.motif_luid)
    }

    fn temps_bruts(&self) -> Result<BTreeMap<String, i64>, String> {
        let mut taille = 0u32;
        let mut nombre = 0u32;
        let _ = unsafe { PdhGetRawCounterArrayW(self.temps, &mut taille, &mut nombre, None) };
        if taille == 0 {
            return Ok(BTreeMap::new());
        }
        let mut tampon = vec![0u64; taille as usize / 8 + 1];
        let s = unsafe {
            PdhGetRawCounterArrayW(self.temps, &mut taille, &mut nombre, Some(tampon.as_mut_ptr().cast()))
        };
        statut("PdhGetRawCounterArrayW", s)?;
        let items = unsafe { std::slice::from_raw_parts(tampon.as_ptr() as *const PDH_RAW_COUNTER_ITEM_W, nombre as usize) };
        Ok(items
            .iter()
            .map(|i| (texte(i.szName), i.RawValue.FirstValue))
            .filter(|(n, _)| self.nous_concerne(n))
            .collect())
    }

    fn utilisations(&self) -> Result<BTreeMap<String, f64>, String> {
        let mut taille = 0u32;
        let mut nombre = 0u32;
        let _ = unsafe { PdhGetFormattedCounterArrayW(self.utilisation, PDH_FMT_DOUBLE, &mut taille, &mut nombre, None) };
        if taille == 0 {
            return Ok(BTreeMap::new());
        }
        let mut tampon = vec![0u64; taille as usize / 8 + 1];
        let s = unsafe {
            PdhGetFormattedCounterArrayW(
                self.utilisation,
                PDH_FMT_DOUBLE,
                &mut taille,
                &mut nombre,
                Some(tampon.as_mut_ptr().cast()),
            )
        };
        statut("PdhGetFormattedCounterArrayW", s)?;
        let items = unsafe {
            std::slice::from_raw_parts(tampon.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W, nombre as usize)
        };
        Ok(items
            .iter()
            .filter(|i| i.FmtValue.CStatus == 0)
            .map(|i| (texte(i.szName), unsafe { i.FmtValue.Anonymous.doubleValue }))
            .filter(|(n, _)| self.nous_concerne(n))
            .collect())
    }

    /// Second relevé : activité de chaque type de moteur depuis l'ouverture.
    pub fn bilan(self) -> Result<Vec<Activite>, String> {
        let s = unsafe { PdhCollectQueryData(self.requete) };
        statut("PdhCollectQueryData (second relevé)", s)?;
        let apres = self.temps_bruts()?;
        let util = self.utilisations().unwrap_or_default();
        let mut par_type: BTreeMap<String, Activite> = BTreeMap::new();
        for (nom, v) in &apres {
            let moteur = nom.rsplit("engtype_").next().unwrap_or("?").to_string();
            // Une instance apparue en cours de mesure part de zéro.
            let d = v - self.avant.get(nom).copied().unwrap_or(0);
            let e = par_type.entry(moteur.clone()).or_insert(Activite { moteur, temps_brut: 0, utilisation: 0.0 });
            e.temps_brut += d.max(0);
            if let Some(u) = util.get(nom) {
                e.utilisation = e.utilisation.max(*u);
            }
        }
        Ok(par_type.into_values().collect())
    }
}

impl Drop for Compteurs {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.requete) };
    }
}

/// Un moteur de décodage vidéo, quel que soit le nom que lui donne le pilote
/// (« VideoDecode » chez NVIDIA et Intel, « Video Codec » possible chez AMD).
pub fn est_moteur_video(nom: &str) -> bool {
    let n = nom.to_lowercase();
    n.contains("decode") || n.contains("codec")
}
