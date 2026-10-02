//! Sonde de faisabilité SkyShare : le décodage vidéo sur une puce non NVIDIA.
//!
//! Trois questions, par la mesure :
//!   1. Ce que la puce déclare via l'API vidéo Direct3D 11 (profils, tailles).
//!   2. Si le décodeur HEVC de Media Foundation est là, et s'il décode EN MATÉRIEL.
//!   3. La même chose en H.264, repli possible.
//!
//! Aucun réseau, aucun compte, aucune fenêtre. Le rapport s'écrit à l'écran et
//! dans `rapport-sonde-amd.txt`, à côté de l'exécutable.

mod dxva;
mod flux;
mod gpu;
mod mf;
mod rapport;
mod systeme;

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use windows::Win32::Media::MediaFoundation::{
    MFShutdown, MFStartup, MFSTARTUP_FULL, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ALL, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_LOCALMFT, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_ENUM_FLAG_UNTRUSTED_STOREMFT, MF_VERSION,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

use flux::{CodecFlux, Flux};
use mf::{Candidat, Mode, Resultat};
use rapport::{err_win, Rapport};
use systeme::Adaptateur;

const VERSION: &str = "sonde-amd 1.0 (02/10/2026)";
const NOM_RAPPORT: &str = "rapport-sonde-amd.txt";

/// Exécute `f`, et transforme une panique en échec rapporté : la sonde continue.
fn protege<T>(r: &mut Rapport, contexte: &str, f: impl FnOnce(&mut Rapport) -> T) -> Option<T> {
    match catch_unwind(AssertUnwindSafe(|| f(r))) {
        Ok(v) => Some(v),
        Err(p) => {
            let m = p
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "panique sans message".into());
            r.echec(contexte, &format!("PANIQUE interne de la sonde : {m}"));
            None
        }
    }
}

fn dossier_rapport(exe: &Path) -> Option<PathBuf> {
    let candidats = [
        Some(exe.to_path_buf()),
        std::env::current_dir().ok(),
        Some(std::env::temp_dir()),
    ];
    for d in candidats.into_iter().flatten() {
        let p = d.join(NOM_RAPPORT);
        if std::fs::write(&p, b"").is_ok() {
            return Some(p);
        }
    }
    None
}

struct Synthese {
    flux: String,
    decodeur: String,
    mode: String,
    verdict: String,
    cadence: String,
}

fn main() {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let mut r = Rapport::nouveau(dossier_rapport(&exe_dir));

    r.ligne("SONDE AMD — faisabilité du décodage vidéo hors NVIDIA, pour SkyShare");
    r.ligne(VERSION);
    r.ligne("Chaque résultat est marqué [MESURÉ] ou [ÉCHEC] ; un échec cite le message d'erreur exact.");
    r.ligne("Rien n'est envoyé sur le réseau. Aucune fenêtre n'est ouverte.");
    match &r.chemin {
        Some(p) => {
            let p = p.display().to_string();
            r.ligne(format!("Rapport écrit dans : {p}"));
        }
        None => r.ligne("ATTENTION : aucun dossier inscriptible, le rapport n'existe qu'à l'écran."),
    }
    if let Err(e) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok() {
        r.echec("CoInitializeEx", &err_win(&e));
    }

    // --- 0. La machine --------------------------------------------------------
    r.titre("0. LA MACHINE");
    protege(&mut r, "description de la machine", systeme::decrire_machine);
    r.ligne("Extensions vidéo du Microsoft Store (indice du registre, pas une preuve) :");
    protege(&mut r, "paquets du Store", systeme::paquets_codecs);
    let adaptateurs = protege(&mut r, "énumération des adaptateurs", systeme::adaptateurs).unwrap_or_default();
    let materiels: Vec<&Adaptateur> = adaptateurs.iter().filter(|a| !a.logiciel).collect();
    if materiels.is_empty() {
        r.echec("adaptateurs", "aucun adaptateur graphique matériel trouvé");
    }
    r.sauver();

    // --- 1. DXVA ---------------------------------------------------------------
    r.titre("1. CE QUE LA PUCE DÉCLARE (API vidéo Direct3D 11)");
    let mut lignes_dxva = Vec::new();
    for a in &materiels {
        r.sous_titre(&format!("Adaptateur : {}", a.nom));
        if let Some(l) = protege(&mut r, &format!("sonde DXVA de {}", a.nom), |r| dxva::sonder(r, a)) {
            lignes_dxva.extend(l.into_iter().map(|(p, t, v)| (a.nom.clone(), p, t, v)));
        }
        r.sauver();
    }

    // --- 2. Media Foundation : énumération ------------------------------------
    r.titre("2. LES DÉCODEURS MEDIA FOUNDATION (énumération MFTEnumEx)");
    if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
        r.echec("MFStartup", &err_win(&e));
    }
    let jeux: [(&str, MFT_ENUM_FLAG); 4] = [
        (
            "synchrones locaux (choix par défaut)",
            MFT_ENUM_FLAG(MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_LOCALMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0),
        ),
        ("MATÉRIELS seulement (MFT_ENUM_FLAG_HARDWARE)", MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0)),
        ("tous (MFT_ENUM_FLAG_ALL)", MFT_ENUM_FLAG(MFT_ENUM_FLAG_ALL.0 | MFT_ENUM_FLAG_SORTANDFILTER.0)),
        (
            "tous, y compris les MFT du Store non approuvés",
            MFT_ENUM_FLAG(MFT_ENUM_FLAG_ALL.0 | MFT_ENUM_FLAG_SORTANDFILTER.0 | MFT_ENUM_FLAG_UNTRUSTED_STOREMFT.0),
        ),
    ];
    let mut candidats: Vec<(CodecFlux, Candidat)> = Vec::new();
    for codec in [CodecFlux::Hevc, CodecFlux::H264] {
        r.sous_titre(&format!("Décodeurs {} (entrée {}, sortie quelconque)", codec.nom(), codec.nom()));
        for (libelle, drapeaux) in jeux {
            let res = protege(&mut r, &format!("MFTEnumEx {} {libelle}", codec.nom()), |_| mf::enumerer(codec, drapeaux));
            match res {
                None => {}
                Some(Err(e)) => r.echec(&format!("MFTEnumEx {} — {libelle}", codec.nom()), &e),
                Some(Ok(v)) => {
                    r.mesure(&format!("{} — {libelle} : {} décodeur(s)", codec.nom(), v.len()));
                    for c in &v {
                        r.ligne(format!(
                            "      « {} »{}  CLSID {}  drapeaux : {}{}{}",
                            c.nom,
                            if c.materiel() { " [MFT MATÉRIEL]" } else { "" },
                            c.clsid.map(|g| format!("{{{g:?}}}")).unwrap_or_else(|| "?".into()),
                            mf::texte_drapeaux(c.drapeaux),
                            c.url_materielle.as_ref().map(|u| format!("  URL matérielle : {u}")).unwrap_or_default(),
                            c.fournisseur.as_ref().map(|f| format!("  fournisseur : {f}")).unwrap_or_default(),
                        ));
                        let deja = candidats.iter().any(|(k, x)| *k == codec && x.nom == c.nom && x.clsid == c.clsid);
                        if !deja {
                            candidats.push((codec, c.clone()));
                        }
                    }
                }
            }
        }
        if !candidats.iter().any(|(k, _)| *k == codec) {
            r.echec(
                &format!("décodeur {}", codec.nom()),
                "AUCUN décodeur Media Foundation n'est enregistré pour ce format sur cette machine",
            );
        }
        r.sauver();
    }

    // --- 3. Décodage réel ------------------------------------------------------
    r.titre("3. DÉCODAGE RÉEL DES FLUX DE TEST (Media Foundation)");
    r.ligne("Méthode : chaque unité d'accès est soumise au décodeur (un échantillon par image), au plus vite,");
    r.ligne("sans cadence imposée. « i/s » est donc la cadence MAXIMALE de décodage, pas une lecture à 60 i/s.");
    r.ligne("Mode MATÉRIEL : décodeur relié à un périphérique Direct3D 11. Trois preuves sont exigées :");
    r.ligne("  (1) le décodeur accepte le périphérique (MFT_MESSAGE_SET_D3D_MANAGER) ;");
    r.ligne("  (2) chaque image rendue est une surface DXGI (IMFDXGIBuffer), pas de la mémoire système ;");
    r.ligne("  (3) le compteur Windows « GPU Engine » du moteur vidéo, pour CE processus, a progressé.");
    r.ligne("Mode LOGICIEL : sans périphérique, pour référence. Le temps CPU est celui de tout le processus.");
    let flux_chemins = match flux::trouver(&exe_dir) {
        Ok(v) => v,
        Err(e) => {
            r.echec("recherche des flux", &e);
            Vec::new()
        }
    };
    if flux_chemins.is_empty() {
        r.echec("flux de test", &format!("aucun fichier .h265 ou .h264 dans {}", exe_dir.display()));
    }
    let apercu = std::env::var_os("SONDE_AMD_APERCU").map(PathBuf::from);
    let mut synthese: Vec<Synthese> = Vec::new();
    for chemin in &flux_chemins {
        let nom_fichier = chemin.file_name().unwrap_or_default().to_string_lossy().to_string();
        r.sous_titre(&format!("Flux {nom_fichier}"));
        let f = match protege(&mut r, &format!("lecture de {nom_fichier}"), |_| flux::charger(chemin)) {
            Some(Ok(f)) => f,
            Some(Err(e)) => {
                r.echec(&format!("lecture de {nom_fichier}"), &e);
                continue;
            }
            None => continue,
        };
        decrire_flux(&mut r, &f);
        let mut tests: Vec<(&Candidat, Mode, Option<&Adaptateur>)> = Vec::new();
        for (k, c) in &candidats {
            if *k != f.codec {
                continue;
            }
            for a in &materiels {
                tests.push((c, Mode::Materiel, Some(*a)));
            }
            if !c.asynchrone() {
                tests.push((c, Mode::Logiciel, materiels.first().copied()));
            }
        }
        if tests.is_empty() {
            r.echec(&format!("décodage de {nom_fichier}"), "aucun décodeur à éprouver pour ce format");
        }
        let mut temoins: Vec<(String, Vec<u8>)> = Vec::new();
        // Chaque passe se fait d'abord en faible latence (le régime de SkyShare).
        // Une passe qui perd des images est refaite sans MF_LOW_LATENCY : mesuré
        // sur la machine de fabrication, le décodeur HEVC logiciel de Microsoft
        // cesse de rendre des images après la 120e en faible latence.
        let mut file: std::collections::VecDeque<(&Candidat, Mode, Option<&Adaptateur>, bool)> =
            tests.into_iter().map(|(c, m, a)| (c, m, a, true)).collect();
        while let Some((c, mode, a, faible_latence)) = file.pop_front() {
            let mut libelle_mode = match mode {
                Mode::Materiel => format!("MATÉRIEL sur {}", a.map(|a| a.nom.as_str()).unwrap_or("?")),
                Mode::Logiciel => "LOGICIEL (référence)".to_string(),
            };
            if !faible_latence {
                libelle_mode.push_str(", SANS faible latence (MF_LOW_LATENCY = 0)");
            }
            r.ligne("");
            r.ligne(format!("  >> Décodeur « {} » — mode {libelle_mode}", c.nom));
            let mut journal = Vec::new();
            let res = protege(&mut r, &format!("décodage {nom_fichier} / {}", c.nom), |_| {
                mf::decoder(c, &f, mode, a, faible_latence, &mut journal)
            });
            if faible_latence {
                if let Some(Ok(x)) = &res {
                    if x.sorties < f.unites.len() {
                        file.push_front((c, mode, a, false));
                    }
                }
            }
            for j in &journal {
                r.ligne(format!("     · {j}"));
            }
            let contexte = format!("{nom_fichier} / « {} » / {libelle_mode}", c.nom);
            match res {
                None => {}
                Some(Err(e)) => {
                    r.echec(&contexte, &e);
                    synthese.push(Synthese {
                        flux: nom_fichier.clone(),
                        decodeur: c.nom.clone(),
                        mode: libelle_mode.clone(),
                        verdict: format!("ÉCHEC — {e}"),
                        cadence: "-".into(),
                    });
                }
                Some(Ok(res)) => {
                    let (verdict, cadence) = rapporter(&mut r, &f, &res, mode, &contexte);
                    if let Some(t) = &res.temoin {
                        temoins.push((libelle_mode.clone() + " / " + &c.nom, t.clone()));
                        if let (Some(dir), Some(s)) = (&apercu, &f.sps) {
                            ecrire_pgm(dir, &format!("{nom_fichier}-{}.pgm", synthese.len()), s.largeur, s.hauteur, t);
                        }
                    }
                    synthese.push(Synthese {
                        flux: nom_fichier.clone(),
                        decodeur: c.nom.clone(),
                        mode: libelle_mode.clone(),
                        verdict,
                        cadence,
                    });
                }
            }
            r.sauver();
        }
        comparer_temoins(&mut r, &temoins);
        r.sauver();
    }

    // --- 4. Synthèse -------------------------------------------------------------
    r.titre("4. SYNTHÈSE");
    r.sous_titre("Question 1 — tailles déclarées (HEVC Main et H.264 VLD)");
    for (a, p, (l, h), v) in &lignes_dxva {
        if p.starts_with("HEVC Main (8") || p.starts_with("H.264 VLD (sans FGT)") {
            let court = v.split(" — ").next().unwrap_or(v);
            let court = if court.contains("configuration(s), mais") { v.as_str() } else { court };
            r.ligne(format!("  {a} | {:<28} | {l}x{h} : {court}", p.split(" — ").next().unwrap_or(p)));
        }
    }
    if lignes_dxva.is_empty() {
        r.ligne("  (aucune donnée : voir les échecs)");
    }
    r.sous_titre("Questions 2 et 3 — décodage réel");
    for s in &synthese {
        r.ligne(format!("  {} | « {} » | {}", s.flux, s.decodeur, s.mode));
        r.ligne(format!("      {}", s.verdict));
        r.ligne(format!("      {}", s.cadence));
    }
    if synthese.is_empty() {
        r.ligne("  (aucun décodage tenté : voir les échecs)");
    }
    r.sous_titre(&format!("Ce qui n'a PAS pu être mesuré ou a échoué ({})", r.echecs.len()));
    let echecs = r.echecs.clone();
    if echecs.is_empty() {
        r.ligne("  Aucun échec.");
    }
    for e in echecs {
        r.ligne(format!("  - {e}"));
    }
    r.ligne("");
    r.ligne("Fin de la sonde.");
    r.sauver();
    let _ = unsafe { MFShutdown() };
    attendre_si_fenetre_propre();
}

fn decrire_flux(r: &mut Rapport, f: &Flux) {
    let duree = f.unites.len() as f64 / 60.0;
    r.ligne(format!(
        "  {} unités d'accès, {} octets, soit {:.2} Mbps à 60 i/s ; {} SPS, {} image(s) IDR dans le flux",
        f.unites.len(),
        f.donnees.len(),
        f.donnees.len() as f64 * 8.0 / duree.max(1e-9) / 1e6,
        f.nb_sps,
        f.nb_idr
    ));
    match (&f.sps, &f.sps_erreur) {
        (Some(s), _) => r.mesure(&format!(
            "{} lu dans le SPS : profil {}, niveau {}, {} , {} bits (luma) / {} bits (chroma), {}x{}",
            f.codec.nom(),
            s.profil,
            s.niveau,
            s.chroma_texte(),
            s.profondeur_luma,
            s.profondeur_chroma,
            s.largeur,
            s.hauteur
        )),
        (None, Some(e)) => r.echec(&format!("analyse du SPS de {}", f.nom), e),
        _ => {}
    }
}

/// Écrit le détail d'une passe et rend (verdict, cadence) pour la synthèse.
fn rapporter(r: &mut Rapport, f: &Flux, res: &Resultat, mode: Mode, contexte: &str) -> (String, String) {
    let coeurs = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
    let ips = res.sorties as f64 / res.duree.max(1e-9);
    let cpu_coeur = res.cpu_ms / (res.duree * 1000.0).max(1e-9) * 100.0;
    let cadence = format!(
        "{}/{} images décodées en {:.2} s : {:.1} i/s ({}) ; CPU du processus {:.0} ms, soit {:.0} % d'un cœur ({:.1} % de la machine) ; jusqu'à {} image(s) retenue(s) par le décodeur",
        res.sorties,
        f.unites.len(),
        res.duree,
        ips,
        if ips >= 60.0 { "tient 60 i/s" } else { "NE TIENT PAS 60 i/s" },
        res.cpu_ms,
        cpu_coeur,
        cpu_coeur / coeurs,
        res.retard_max
    );
    r.mesure(&cadence);
    if let Some(e) = &res.interrompu {
        r.echec(&format!("{contexte} (décodage interrompu)"), e);
    }
    if res.sorties < f.unites.len() {
        r.echec(
            contexte,
            &format!(
                "{} image(s) sur {} n'ont pas été rendues par le décodeur, sans erreur signalée ({})",
                f.unites.len() - res.sorties,
                f.unites.len(),
                res.images_rendues(f.unites.len())
            ),
        );
    }
    r.ligne(format!(
        "     Images rendues en surface DXGI : {} ; en mémoire système : {}",
        res.sorties_dxgi, res.sorties_memoire
    ));
    r.ligne(format!("     Horodatages des sorties : {}", res.images_rendues(f.unites.len())));
    if let Some(t) = &res.texture {
        r.ligne(format!("     Surface de sortie : {t}"));
    }

    let mut video_actif: Option<bool> = None;
    let detail_moteur: String;
    match &res.activite {
        None => detail_moteur = "compteurs GPU non relevés (aucun adaptateur)".into(),
        Some(Err(e)) => {
            r.echec(&format!("{contexte} : compteurs GPU Engine"), e);
            detail_moteur = format!("compteurs GPU indisponibles : {e}");
        }
        Some(Ok(v)) => {
            let actifs: Vec<String> = v
                .iter()
                .filter(|a| a.temps_brut > 0 || a.utilisation > 0.0)
                .map(|a| format!("{} ({}, utilisation max {:.1} %)", a.moteur, activite_ms(a.temps_brut), a.utilisation))
                .collect();
            let tous: Vec<&str> = v.iter().map(|a| a.moteur.as_str()).collect();
            r.ligne(format!(
                "     Moteurs GPU vus pour ce processus : {}",
                if tous.is_empty() { "aucun".to_string() } else { tous.join(", ") }
            ));
            r.ligne(format!(
                "     Moteurs actifs pendant le décodage : {}",
                if actifs.is_empty() { "aucun".to_string() } else { actifs.join(" ; ") }
            ));
            let v_actif = v.iter().any(|a| gpu::est_moteur_video(&a.moteur) && (a.temps_brut > 0 || a.utilisation > 0.0));
            let v_present = v.iter().any(|a| gpu::est_moteur_video(&a.moteur));
            video_actif = Some(v_actif);
            detail_moteur = if v_actif {
                v.iter()
                    .filter(|a| gpu::est_moteur_video(&a.moteur) && (a.temps_brut > 0 || a.utilisation > 0.0))
                    .map(|a| format!("moteur « {} » actif ({}, utilisation max {:.1} %)", a.moteur, activite_ms(a.temps_brut), a.utilisation))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else if v_present {
                "le moteur vidéo du GPU n'a enregistré AUCUNE activité pour ce processus".into()
            } else {
                "aucun moteur vidéo n'apparaît pour ce processus dans les compteurs".into()
            };
        }
    }

    let tout_dxgi = res.sorties > 0 && res.sorties_dxgi == res.sorties;
    let verdict = if res.sorties == 0 {
        "AUCUNE IMAGE DÉCODÉE".to_string()
    } else {
        match mode {
            Mode::Materiel => {
                if res.sorties_memoire > 0 {
                    format!(
                        "DÉCODAGE LOGICIEL — le décodeur a accepté le périphérique mais rend {} image(s) en mémoire système ; {detail_moteur}",
                        res.sorties_memoire
                    )
                } else if tout_dxgi && video_actif == Some(true) {
                    format!(
                        "DÉCODAGE MATÉRIEL ÉTABLI — périphérique D3D11 accepté, {}/{} images en surfaces DXGI, {detail_moteur}",
                        res.sorties_dxgi, res.sorties
                    )
                } else if video_actif == Some(false) {
                    format!(
                        "MATÉRIEL NON ÉTABLI, LOGICIEL PROBABLE — surfaces DXGI, mais {detail_moteur} (le décodeur a pu décoder sur le processeur puis téléverser)"
                    )
                } else {
                    format!("MATÉRIEL NON ÉTABLI — surfaces DXGI, mais la troisième preuve manque : {detail_moteur}")
                }
            }
            Mode::Logiciel => {
                let note = match video_actif {
                    Some(true) => format!(" — ANOMALIE : {detail_moteur} alors qu'aucun périphérique n'a été donné"),
                    Some(false) => " — moteur vidéo du GPU inactif, comme attendu".to_string(),
                    None => String::new(),
                };
                format!("DÉCODAGE LOGICIEL (mode de référence, sans périphérique){note}")
            }
        }
    };
    let verdict = if res.sorties > 0 && res.sorties < f.unites.len() {
        format!("{verdict} — MAIS {} image(s) sur {} PERDUE(S)", f.unites.len() - res.sorties, f.unites.len())
    } else {
        verdict
    };
    r.ligne(format!("     VERDICT : {verdict}"));
    (verdict, cadence)
}

/// « Running Time » des compteurs GPU Engine. Unité de 100 ns : recoupée sur la
/// machine de fabrication (476 ms d'activité sur 550 ms de décodage, pour une
/// utilisation lue à 86,3 %), pas documentée par Microsoft à notre connaissance.
fn activite_ms(brut: i64) -> String {
    format!("compteur +{brut}, soit {:.0} ms d'activité", brut as f64 / 10_000.0)
}

fn comparer_temoins(r: &mut Rapport, t: &[(String, Vec<u8>)]) {
    if t.len() < 2 {
        return;
    }
    r.ligne("");
    r.ligne(format!(
        "  Contrôle de justesse : luminance de l'image n° {} comparée entre les passes (la première sert de référence)",
        mf::IMAGE_TEMOIN
    ));
    let (nom0, ref0) = &t[0];
    for (nom, y) in &t[1..] {
        if y.len() != ref0.len() {
            r.ligne(format!("     {nom} contre {nom0} : tailles différentes ({} contre {} octets)", y.len(), ref0.len()));
            continue;
        }
        let mut diff = 0usize;
        let mut max = 0u8;
        let mut eqm = 0f64;
        for (a, b) in y.iter().zip(ref0) {
            let d = a.abs_diff(*b);
            if d > 0 {
                diff += 1;
                max = max.max(d);
                eqm += (d as f64) * (d as f64);
            }
        }
        if diff == 0 {
            r.mesure(&format!("{nom} contre {nom0} : luminances identiques à l'octet près"));
        } else {
            let psnr = 10.0 * (255.0f64 * 255.0 / (eqm / y.len() as f64)).log10();
            r.mesure(&format!(
                "{nom} contre {nom0} : {diff} octets différents sur {}, écart max {max}, PSNR {psnr:.1} dB",
                y.len()
            ));
        }
    }
}

fn ecrire_pgm(dir: &Path, nom: &str, l: u32, h: u32, y: &[u8]) {
    if y.len() != (l * h) as usize {
        return;
    }
    let mut o = format!("P5\n{l} {h}\n255\n").into_bytes();
    o.extend_from_slice(y);
    let _ = std::fs::write(dir.join(nom), o);
}

/// Lancée par un double-clic, la console se fermerait sur le rapport : on
/// attend Entrée seulement si la sonde est seule dans sa console.
fn attendre_si_fenetre_propre() {
    use windows::Win32::System::Console::GetConsoleProcessList;
    let mut ids = [0u32; 4];
    let n = unsafe { GetConsoleProcessList(&mut ids) };
    if n == 1 {
        println!("\nAppuyez sur Entrée pour fermer.");
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
}
