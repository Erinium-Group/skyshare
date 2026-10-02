//! Les flux de test : lecture, découpage en unités d'accès, analyse des en-têtes.
//!
//! La sonde ne croit pas le nom du fichier : elle lit elle-même, dans le SPS,
//! le profil, le sous-échantillonnage, la profondeur et la taille.

use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CodecFlux {
    Hevc,
    H264,
}

impl CodecFlux {
    pub fn nom(self) -> &'static str {
        match self {
            CodecFlux::Hevc => "HEVC",
            CodecFlux::H264 => "H.264",
        }
    }
}

pub struct Flux {
    pub nom: String,
    pub codec: CodecFlux,
    pub donnees: Vec<u8>,
    /// (début, longueur) de chaque unité d'accès dans `donnees`.
    pub unites: Vec<(usize, usize)>,
    pub sps: Option<Sps>,
    pub sps_erreur: Option<String>,
    pub nb_idr: usize,
    pub nb_sps: usize,
}

#[derive(Clone, Debug)]
pub struct Sps {
    pub profil: String,
    pub chroma: u32,
    pub profondeur_luma: u32,
    pub profondeur_chroma: u32,
    pub largeur: u32,
    pub hauteur: u32,
    pub niveau: u32,
}

impl Sps {
    pub fn chroma_texte(&self) -> &'static str {
        match self.chroma {
            0 => "4:0:0",
            1 => "4:2:0",
            2 => "4:2:2",
            3 => "4:4:4",
            _ => "inconnu",
        }
    }
}

/// Les flux `*.h265` / `*.h264` du dossier, avec leur fichier `.tailles`.
pub fn trouver(dossier: &Path) -> Result<Vec<PathBuf>, String> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dossier)
        .map_err(|e| format!("lecture du dossier {} : {e}", dossier.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| matches!(p.extension().and_then(|x| x.to_str()), Some("h265") | Some("h264")))
        .collect();
    // HEVC d'abord : c'est la question principale.
    v.sort_by_key(|p| {
        let n = p.file_name().unwrap_or_default().to_string_lossy().to_string();
        (!n.ends_with(".h265"), n)
    });
    Ok(v)
}

pub fn charger(chemin: &Path) -> Result<Flux, String> {
    let nom = chemin.file_name().unwrap_or_default().to_string_lossy().to_string();
    let codec = if nom.ends_with(".h265") { CodecFlux::Hevc } else { CodecFlux::H264 };
    let donnees = std::fs::read(chemin).map_err(|e| format!("lecture de {nom} : {e}"))?;
    let chemin_tailles = PathBuf::from(format!("{}.tailles", chemin.display()));
    let texte = std::fs::read_to_string(&chemin_tailles)
        .map_err(|e| format!("lecture de {} : {e}", chemin_tailles.display()))?;
    let mut unites = Vec::new();
    let mut pos = 0usize;
    for (i, l) in texte.lines().enumerate() {
        let l = l.trim();
        if l.is_empty() {
            continue;
        }
        let t: usize = l.parse().map_err(|e| format!("{nom}.tailles, ligne {} : « {l} » : {e}", i + 1))?;
        unites.push((pos, t));
        pos += t;
    }
    if pos != donnees.len() {
        return Err(format!(
            "{nom} : la somme des tailles ({pos} octets) ne correspond pas au fichier ({} octets)",
            donnees.len()
        ));
    }

    let (mut nb_idr, mut nb_sps, mut sps_brut) = (0, 0, None);
    for (debut, fin) in nal_positions(&donnees) {
        let nal = &donnees[debut..fin];
        if nal.is_empty() {
            continue;
        }
        let (est_sps, est_idr) = match codec {
            CodecFlux::Hevc => {
                let t = (nal[0] >> 1) & 0x3f;
                (t == 33, t == 19 || t == 20)
            }
            CodecFlux::H264 => {
                let t = nal[0] & 0x1f;
                (t == 7, t == 5)
            }
        };
        if est_sps {
            nb_sps += 1;
            if sps_brut.is_none() {
                sps_brut = Some(nal.to_vec());
            }
        }
        if est_idr {
            nb_idr += 1;
        }
    }
    let (sps, sps_erreur) = match sps_brut {
        None => (None, Some("aucun SPS dans le flux".to_string())),
        Some(b) => {
            let r = match codec {
                CodecFlux::Hevc => analyser_sps_hevc(&b),
                CodecFlux::H264 => analyser_sps_h264(&b),
            };
            match r {
                Ok(s) => (Some(s), None),
                Err(e) => (None, Some(e)),
            }
        }
    };
    Ok(Flux { nom, codec, donnees, unites, sps, sps_erreur, nb_idr, nb_sps })
}

/// (début, fin) de chaque NAL, codes de départ exclus.
fn nal_positions(d: &[u8]) -> Vec<(usize, usize)> {
    let mut debuts = Vec::new();
    let mut i = 0;
    while i + 3 <= d.len() {
        if d[i] == 0 && d[i + 1] == 0 && d[i + 2] == 1 {
            debuts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut v = Vec::new();
    for (k, &deb) in debuts.iter().enumerate() {
        let mut fin = if k + 1 < debuts.len() { debuts[k + 1] - 3 } else { d.len() };
        // Un code de départ sur quatre octets laisse un zéro en fin de NAL.
        while fin > deb && d[fin - 1] == 0 {
            fin -= 1;
        }
        v.push((deb, fin));
    }
    v
}

struct Bits {
    o: Vec<u8>,
    pos: usize,
}

impl Bits {
    /// Retire les octets d'émulation (00 00 03).
    fn nouveau(nal: &[u8]) -> Self {
        let mut o = Vec::with_capacity(nal.len());
        let mut zeros = 0;
        for &b in nal {
            if zeros >= 2 && b == 3 {
                zeros = 0;
                continue;
            }
            zeros = if b == 0 { zeros + 1 } else { 0 };
            o.push(b);
        }
        Self { o, pos: 0 }
    }

    fn u(&mut self, n: u32) -> Result<u32, String> {
        let mut v = 0u32;
        for _ in 0..n {
            let octet = *self.o.get(self.pos / 8).ok_or("SPS tronqué")?;
            let bit = (octet >> (7 - (self.pos % 8))) & 1;
            v = (v << 1) | bit as u32;
            self.pos += 1;
        }
        Ok(v)
    }

    fn sauter(&mut self, n: usize) {
        self.pos += n;
    }

    fn ue(&mut self) -> Result<u32, String> {
        let mut zeros = 0;
        while self.u(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err("exp-Golomb invalide".into());
            }
        }
        Ok((1u32 << zeros) - 1 + self.u(zeros)?)
    }

    fn se(&mut self) -> Result<i32, String> {
        let k = self.ue()?;
        Ok(if k % 2 == 1 { k.div_ceil(2) as i32 } else { -((k / 2) as i32) })
    }
}

fn analyser_sps_hevc(nal: &[u8]) -> Result<Sps, String> {
    let mut b = Bits::nouveau(nal);
    b.sauter(16); // en-tête NAL
    b.u(4)?; // vps id
    let max_sub = b.u(3)?;
    b.u(1)?;
    b.u(2)?; // profile_space
    b.u(1)?; // tier
    let profil_idc = b.u(5)?;
    b.sauter(32 + 48);
    let niveau = b.u(8)?;
    let mut presents = Vec::new();
    for _ in 0..max_sub {
        presents.push((b.u(1)?, b.u(1)?));
    }
    if max_sub > 0 {
        for _ in max_sub..8 {
            b.u(2)?;
        }
    }
    for (p, l) in presents {
        if p == 1 {
            b.sauter(88);
        }
        if l == 1 {
            b.sauter(8);
        }
    }
    b.ue()?; // sps id
    let chroma = b.ue()?;
    if chroma == 3 {
        b.u(1)?;
    }
    let mut largeur = b.ue()?;
    let mut hauteur = b.ue()?;
    if b.u(1)? == 1 {
        let (sx, sy) = match chroma {
            1 => (2, 2),
            2 => (2, 1),
            _ => (1, 1),
        };
        let (g, d, h, ba) = (b.ue()?, b.ue()?, b.ue()?, b.ue()?);
        largeur -= sx * (g + d);
        hauteur -= sy * (h + ba);
    }
    let pl = b.ue()? + 8;
    let pc = b.ue()? + 8;
    let profil = match profil_idc {
        1 => "Main".to_string(),
        2 => "Main 10".to_string(),
        3 => "Main Still Picture".to_string(),
        4 => "Range Extensions (RExt)".to_string(),
        n => format!("profil {n}"),
    };
    Ok(Sps { profil, chroma, profondeur_luma: pl, profondeur_chroma: pc, largeur, hauteur, niveau })
}

fn analyser_sps_h264(nal: &[u8]) -> Result<Sps, String> {
    let mut b = Bits::nouveau(nal);
    b.sauter(8);
    let profil_idc = b.u(8)?;
    b.u(8)?;
    let niveau = b.u(8)?;
    b.ue()?;
    let (mut chroma, mut pl, mut pc) = (1, 8, 8);
    if [100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135].contains(&profil_idc) {
        chroma = b.ue()?;
        if chroma == 3 {
            b.u(1)?;
        }
        pl = b.ue()? + 8;
        pc = b.ue()? + 8;
        b.u(1)?;
        if b.u(1)? == 1 {
            return Err("matrices de quantification présentes : analyse non prise en charge".into());
        }
    }
    b.ue()?;
    let poc = b.ue()?;
    if poc == 0 {
        b.ue()?;
    } else if poc == 1 {
        b.u(1)?;
        b.se()?;
        b.se()?;
        for _ in 0..b.ue()? {
            b.se()?;
        }
    }
    b.ue()?;
    b.u(1)?;
    let l_mb = b.ue()? + 1;
    let h_mb = b.ue()? + 1;
    let frame_only = b.u(1)?;
    if frame_only == 0 {
        b.u(1)?;
    }
    b.u(1)?;
    let mut largeur = l_mb * 16;
    let mut hauteur = (2 - frame_only) * h_mb * 16;
    if b.u(1)? == 1 {
        let (sx, sy) = if chroma == 1 { (2, 2 * (2 - frame_only)) } else { (1, 2 - frame_only) };
        let (g, d, h, ba) = (b.ue()?, b.ue()?, b.ue()?, b.ue()?);
        largeur -= sx * (g + d);
        hauteur -= sy * (h + ba);
    }
    let profil = match profil_idc {
        66 => "Baseline".to_string(),
        77 => "Main".to_string(),
        100 => "High".to_string(),
        110 => "High 10".to_string(),
        122 => "High 4:2:2".to_string(),
        244 => "High 4:4:4 Predictive".to_string(),
        n => format!("profil {n}"),
    };
    Ok(Sps { profil, chroma, profondeur_luma: pl, profondeur_chroma: pc, largeur, hauteur, niveau })
}
