//! Le rapport : écrit à l'écran et, au fil de l'eau, dans un fichier.
//!
//! Le fichier est réécrit en entier après chaque section : si la sonde
//! s'arrête en route (coupure, plantage d'un pilote), ce qui a été mesuré
//! jusque-là reste lisible.

use std::path::PathBuf;

pub struct Rapport {
    texte: String,
    pub chemin: Option<PathBuf>,
    pub echecs: Vec<String>,
    pub mesures: Vec<String>,
}

impl Rapport {
    pub fn nouveau(chemin: Option<PathBuf>) -> Self {
        Self { texte: String::new(), chemin, echecs: Vec::new(), mesures: Vec::new() }
    }

    pub fn ligne(&mut self, s: impl AsRef<str>) {
        let s = s.as_ref();
        println!("{s}");
        self.texte.push_str(s);
        self.texte.push_str("\r\n");
    }

    pub fn titre(&mut self, s: &str) {
        self.ligne("");
        self.ligne("=".repeat(78));
        self.ligne(s);
        self.ligne("=".repeat(78));
    }

    pub fn sous_titre(&mut self, s: &str) {
        self.ligne("");
        self.ligne(format!("--- {s} ---"));
    }

    /// Un échec : le contexte et le message d'erreur exact.
    pub fn echec(&mut self, contexte: &str, message: &str) {
        self.ligne(format!("  [ÉCHEC] {contexte} : {message}"));
        self.echecs.push(format!("{contexte} : {message}"));
    }

    /// Une mesure qui mérite de figurer dans la synthèse finale.
    pub fn mesure(&mut self, s: &str) {
        self.ligne(format!("  [MESURÉ] {s}"));
        self.mesures.push(s.to_string());
    }

    /// Réécrit le fichier. UTF-8 avec BOM et fins de ligne CRLF : le Bloc-notes
    /// l'ouvre avec ses accents.
    pub fn sauver(&mut self) {
        let Some(chemin) = &self.chemin else { return };
        let mut octets = vec![0xEF, 0xBB, 0xBF];
        octets.extend_from_slice(self.texte.as_bytes());
        if let Err(e) = std::fs::write(chemin, octets) {
            let message = format!("écriture de {} impossible : {e}", chemin.display());
            println!("  [ÉCHEC] {message}");
            self.chemin = None;
        }
    }
}

/// Message d'une erreur Windows avec son code HRESULT, tel quel.
pub fn err_win(e: &windows::core::Error) -> String {
    let m = e.message();
    let m = m.trim();
    if m.is_empty() {
        format!("HRESULT 0x{:08X}", e.code().0 as u32)
    } else {
        format!("{m} (HRESULT 0x{:08X})", e.code().0 as u32)
    }
}

/// Ajoute le nom de l'étape à une erreur Windows.
pub trait Etape<T> {
    fn etape(self, nom: &str) -> Result<T, String>;
}

impl<T> Etape<T> for windows::core::Result<T> {
    fn etape(self, nom: &str) -> Result<T, String> {
        self.map_err(|e| format!("{nom} : {}", err_win(&e)))
    }
}
