//! Q2 : la chaîne capture -> encodage tient-elle sans jamais passer par la RAM ?
//!
//! Deux sources d'images, toutes deux encodées par le **même** chemin NVENC :
//!
//! - `ecran` : les textures réelles de Windows.Graphics.Capture. C'est la
//!   preuve de bout en bout, mais WGC ne délivre une image que lorsque l'écran
//!   change : sur un écran figé, la mesure de débit et de latence n'est pas
//!   représentative.
//! - `synthetique` : une texture Direct3D 11 créée sur le device de la capture,
//!   dont le contenu défile à chaque image. Elle isole le chemin NVENC de la
//!   disponibilité des images et permet une mesure de latence significative.

use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use sky_capture::wgc::WgcCapture;
use sky_capture::CapturedFrame;
use sky_encode::{nvenc::NvencEncoder, Codec};
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_BIND_RENDER_TARGET,
    D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;

/// Cadence visée pour la boucle d'encodage. `pub(crate)` : réutilisée par
/// `cmd_codecs` (Q3) pour que la comparaison tourne à la même cadence.
pub(crate) const FPS: u32 = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Ecran,
    Synthetique,
}

pub fn parse_codec(s: &str) -> anyhow::Result<Codec> {
    match s
        .to_ascii_lowercase()
        .replace(['-', '_', ':', '.'], "")
        .as_str()
    {
        "h264" | "h264420" => Ok(Codec::H264_420),
        "h264444" => Ok(Codec::H264_444),
        "hevc" | "hevc444" | "h265444" => Ok(Codec::Hevc444),
        "av1" | "av1420" => Ok(Codec::Av1_420),
        autre => Err(anyhow!(
            "codec inconnu « {autre} » (attendu : h264420, h264444, hevc444, av1420)"
        )),
    }
}

pub fn parse_source(s: &str) -> anyhow::Result<Source> {
    match s.to_ascii_lowercase().as_str() {
        "ecran" | "écran" | "screen" => Ok(Source::Ecran),
        "synthetique" | "synthétique" | "synth" => Ok(Source::Synthetique),
        autre => Err(anyhow!(
            "source inconnue « {autre} » (attendu : ecran ou synthetique)"
        )),
    }
}

pub fn run(
    seconds: u64,
    codec: Codec,
    bitrate_mbps: u32,
    sortie: &str,
    monitor: usize,
    source: Source,
) -> anyhow::Result<()> {
    let mut cap = WgcCapture::new(monitor, None).context("démarrage de la capture")?;
    println!("Adaptateur GPU    : {}", nom_adaptateur(cap.d3d_device())?);

    // Première image réelle : elle donne les dimensions et prouve que la
    // capture tourne, même en mode synthétique.
    let attente_max = Instant::now() + Duration::from_secs(5);
    let premiere = loop {
        if let Some(f) = cap.next_frame(Duration::from_millis(200))? {
            break f;
        }
        if Instant::now() >= attente_max {
            return Err(anyhow!(
                "aucune image capturée en 5 s — l'écran est-il totalement figé ?"
            ));
        }
    };
    let (largeur, hauteur) = (premiere.width, premiere.height);

    let mut enc = NvencEncoder::new(
        cap.d3d_device(),
        codec,
        largeur,
        hauteur,
        FPS,
        bitrate_mbps * 1_000_000,
    )
    .context("ouverture de la session NVENC sur le device Direct3D 11 de la capture")?;

    let mut fichier = File::create(sortie).with_context(|| format!("création de {sortie}"))?;

    let stats = match source {
        Source::Ecran => encoder_ecran(&mut cap, &mut enc, seconds, &mut fichier)?,
        Source::Synthetique => {
            let mut synth = TextureSynthetique::new(cap.d3d_device(), largeur, hauteur)
                .context("création de la texture Direct3D 11 synthétique")?;
            encoder_synthetique(&mut enc, &mut synth, seconds, &mut fichier)?
        }
    };
    drop(fichier);

    let StatsEncodage {
        octets,
        images,
        cles,
        duree,
        p50_us: p50,
        p99_us: p99,
    } = stats;

    println!("\n--- Q2 : encodage {} ---", codec.label());
    println!(
        "Source            : {}",
        match source {
            Source::Ecran => "écran réel (Windows.Graphics.Capture)",
            Source::Synthetique => "texture D3D11 synthétique (même device que la capture)",
        }
    );
    println!("Résolution        : {largeur}x{hauteur}");
    println!("Durée mesurée     : {duree:.1} s");
    println!("Images encodées   : {images}  (dont {cles} image(s) clé)");
    println!("Cadence obtenue   : {:.1} i/s", images as f64 / duree);
    println!(
        "Débit réel        : {:.1} Mbps",
        octets as f64 * 8.0 / duree / 1e6
    );
    println!("Octets écrits     : {octets}");
    println!("Encodage médian   : {:.2} ms", p50 as f64 / 1000.0);
    println!("Encodage p99      : {:.2} ms", p99 as f64 / 1000.0);
    println!("Fichier           : {sortie}");

    if images == 0 {
        println!("Verdict           : ÉCHEC (aucune image encodée)");
    } else if source == Source::Ecran {
        // Un écran figé ne fournit qu'une poignée d'images : annoncer un seuil
        // de performance atteint sur cette base serait mensonger.
        println!(
            "Verdict           : chemin NVENC fonctionnel ({images} image(s)). \
             Latence NON concluante sur cette source — relancer avec \
             --source synthetique pour une mesure représentative."
        );
    } else if p99 < 16_000 {
        println!("Verdict           : SUCCÈS (p99 < 16 ms à {FPS} i/s)");
    } else {
        println!("Verdict           : ÉCHEC (p99 >= 16 ms)");
    }

    Ok(())
}

/// Résultat d'une boucle d'encodage. `pub(crate)` : partagé avec `cmd_codecs`
/// (Q3), qui a besoin des mêmes chiffres (octets, débit réel, p99) que Q2.
pub(crate) struct StatsEncodage {
    pub octets: u64,
    pub images: u64,
    pub cles: u64,
    pub duree: f64,
    pub p50_us: u64,
    pub p99_us: u64,
}

/// Boucle d'encodage sur l'écran réel : une image par changement détecté par
/// WGC, au rythme de l'écran — pas de cadence forcée.
fn encoder_ecran(
    cap: &mut WgcCapture,
    enc: &mut NvencEncoder,
    seconds: u64,
    fichier: &mut File,
) -> anyhow::Result<StatsEncodage> {
    let mut octets = 0u64;
    let mut images = 0u64;
    let mut cles = 0u64;
    let mut temps_encodage: Vec<u64> = Vec::new();

    let debut = Instant::now();
    let fin = debut + Duration::from_secs(seconds);

    while Instant::now() < fin {
        let Some(image) = cap.next_frame(Duration::from_millis(50))? else {
            continue;
        };
        if let Some(pkt) = enc.encode(&image)? {
            fichier.write_all(&pkt.data)?;
            octets += pkt.data.len() as u64;
            images += 1;
            if pkt.is_keyframe {
                cles += 1;
            }
            temps_encodage.push(pkt.encode_us);
        }
    }
    let duree = debut.elapsed().as_secs_f64().max(0.001);
    fichier.flush()?;

    temps_encodage.sort_unstable();
    Ok(StatsEncodage {
        octets,
        images,
        cles,
        duree,
        p50_us: percentile(&temps_encodage, 50),
        p99_us: percentile(&temps_encodage, 99),
    })
}

/// Boucle d'encodage sur la texture synthétique, bridée à [`FPS`].
///
/// `pub(crate)` : c'est la même boucle qu'utilise `cmd_codecs` (Q3) pour
/// encoder les quatre combinaisons codec/chroma sur exactement la même
/// séquence d'images — `synth` détermine tout le contenu, et son compteur
/// interne ne dépend que du nombre d'appels à `prochaine_image`.
pub(crate) fn encoder_synthetique(
    enc: &mut NvencEncoder,
    synth: &mut TextureSynthetique,
    seconds: u64,
    fichier: &mut File,
) -> anyhow::Result<StatsEncodage> {
    let mut octets = 0u64;
    let mut images = 0u64;
    let mut cles = 0u64;
    let mut temps_encodage: Vec<u64> = Vec::new();

    let debut = Instant::now();
    let fin = debut + Duration::from_secs(seconds);
    let periode = Duration::from_micros(1_000_000 / FPS as u64);
    let mut prochaine = Instant::now();

    while Instant::now() < fin {
        // Cadence bridée à FPS : la mesure reflète une charge de 60 i/s, pas
        // un débit maximal théorique.
        let maintenant = Instant::now();
        if maintenant < prochaine {
            std::thread::sleep(prochaine - maintenant);
        }
        prochaine += periode;
        let image = synth.prochaine_image()?;

        if let Some(pkt) = enc.encode(&image)? {
            fichier.write_all(&pkt.data)?;
            octets += pkt.data.len() as u64;
            images += 1;
            if pkt.is_keyframe {
                cles += 1;
            }
            temps_encodage.push(pkt.encode_us);
        }
    }
    let duree = debut.elapsed().as_secs_f64().max(0.001);
    fichier.flush()?;

    temps_encodage.sort_unstable();
    Ok(StatsEncodage {
        octets,
        images,
        cles,
        duree,
        p50_us: percentile(&temps_encodage, 50),
        p99_us: percentile(&temps_encodage, 99),
    })
}

fn percentile(tries: &[u64], p: usize) -> u64 {
    if tries.is_empty() {
        return 0;
    }
    let idx = (tries.len() * p / 100).min(tries.len() - 1);
    tries[idx]
}

/// Nom de l'adaptateur derrière le device Direct3D 11 — NVENC exige que ce
/// soit le GPU NVIDIA, pas un iGPU.
fn nom_adaptateur(device: &ID3D11Device) -> anyhow::Result<String> {
    let dxgi: IDXGIDevice = device.cast()?;
    let adapter = unsafe { dxgi.GetAdapter() }?;
    let desc = unsafe { adapter.GetDesc() }?;
    let nom: String = String::from_utf16_lossy(&desc.Description);
    Ok(nom.trim_end_matches('\0').trim().to_string())
}

/// Marge verticale du motif : le décalage d'une ligne à l'autre simule un
/// défilement, ce qui donne à l'encodeur un vrai travail de compensation de
/// mouvement plutôt qu'une image figée.
const MARGE_LIGNES: u32 = 64;

/// Texture Direct3D 11 dont le contenu change à chaque image.
///
/// Créée sur le device de la capture, au même format que les textures WGC
/// (`B8G8R8A8_UNORM`) : NVENC ne voit aucune différence avec une vraie image.
///
/// Le motif est téléversé **une seule fois** dans un atlas plus haut que
/// l'image ; chaque image n'est ensuite qu'une copie GPU->GPU d'une fenêtre
/// décalée de cet atlas. Aucune donnée d'image ne traverse le bus à chaque
/// tour de boucle : le banc de test ne pollue pas la mesure d'encodage.
///
/// `pub(crate)` : `cmd_codecs` (Q3) en recrée une par codec, pour que le
/// compteur `n` reparte de 0 à chaque fois — c'est ce qui garantit que les
/// quatre flux voient la même séquence d'images plutôt qu'une suite décalée.
pub(crate) struct TextureSynthetique {
    atlas: ID3D11Texture2D,
    texture: ID3D11Texture2D,
    contexte: ID3D11DeviceContext,
    largeur: u32,
    hauteur: u32,
    n: u32,
}

impl TextureSynthetique {
    pub(crate) fn new(device: &ID3D11Device, largeur: u32, hauteur: u32) -> anyhow::Result<Self> {
        let contexte: ID3D11DeviceContext =
            unsafe { device.GetImmediateContext() }.context("GetImmediateContext")?;

        // Atlas : le motif complet, téléversé une fois pour toutes.
        let motif = motif_detaille(largeur, hauteur + MARGE_LIGNES);
        let donnees = D3D11_SUBRESOURCE_DATA {
            pSysMem: motif.as_ptr() as *const c_void,
            SysMemPitch: largeur * 4,
            SysMemSlicePitch: 0,
        };
        let desc_atlas = D3D11_TEXTURE2D_DESC {
            Height: hauteur + MARGE_LIGNES,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            ..desc_bgra(largeur, hauteur + MARGE_LIGNES)
        };
        let mut atlas: Option<ID3D11Texture2D> = None;
        unsafe { device.CreateTexture2D(&desc_atlas, Some(&donnees), Some(&mut atlas)) }
            .context("CreateTexture2D (atlas)")?;
        let atlas = atlas.ok_or_else(|| anyhow!("texture atlas nulle"))?;

        // Destination : mêmes drapeaux qu'une texture de pool WGC.
        let desc = desc_bgra(largeur, hauteur);
        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
            .context("CreateTexture2D (destination)")?;
        let texture = texture.ok_or_else(|| anyhow!("texture nulle"))?;

        Ok(Self {
            atlas,
            texture,
            contexte,
            largeur,
            hauteur,
            n: 0,
        })
    }

    /// Recopie la fenêtre suivante de l'atlas (GPU -> GPU) et rend l'image.
    pub(crate) fn prochaine_image(&mut self) -> anyhow::Result<CapturedFrame> {
        let ligne = self.n % MARGE_LIGNES;
        let fenetre = D3D11_BOX {
            left: 0,
            top: ligne,
            front: 0,
            right: self.largeur,
            bottom: ligne + self.hauteur,
            back: 1,
        };
        // SAFETY : l'atlas fait `hauteur + MARGE_LIGNES` lignes et
        // `ligne < MARGE_LIGNES`, donc la fenêtre reste dans ses bornes.
        unsafe {
            self.contexte.CopySubresourceRegion(
                &self.texture,
                0,
                0,
                0,
                0,
                &self.atlas,
                0,
                Some(&fenetre),
            );
        }
        self.n += 1;
        Ok(CapturedFrame {
            texture: self.texture.clone(),
            width: self.largeur,
            height: self.hauteur,
            captured_at: Instant::now(),
        })
    }
}

/// Descripteur BGRA calqué sur celui des textures du pool WGC.
fn desc_bgra(largeur: u32, hauteur: u32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Width: largeur,
        Height: hauteur,
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
    }
}

/// Panneau « éditeur de code » : le reste de l'image (bureau, marges) reste
/// en fond uni. Un plein écran d'alternance rouge/bleu pixel à pixel est
/// hors norme (aucun écran de travail réel n'est aussi dense sur toute sa
/// surface) et fait déborder le contrôle de débit CBR de NVENC bien au-delà
/// de la cible même sur du 1 image de tampon VBV — mesuré à 15-18 Mbps pour
/// une cible à 10 Mbps, ce qui aurait invalidé la comparaison « à débit
/// égal ». Confiner le pire cas à un panneau réaliste laisse assez de
/// surface plate (fond uni, ~0 bit après la première image) pour que le
/// contrôleur de débit tienne sa cible sur les quatre codecs.
const PANNEAU_X0: u32 = 200;
const PANNEAU_Y0: u32 = 150;
const PANNEAU_LARGEUR: u32 = 1200;
const PANNEAU_HAUTEUR: u32 = 800;

/// Motif BGRA détaillé : fond sombre d'éditeur de code, lignes de texte de
/// ~12 px de haut, glyphes rendus en 1 pixel de large alternant rouge pur et
/// bleu pur, confinés au panneau ci-dessus.
///
/// C'est délibérément le pire cas pour un sous-échantillonnage chroma : le
/// 4:2:0 divise la résolution de U et V par 2 dans les deux axes, donc une
/// alternance rouge/bleu à la colonne près est exactement ce qu'il ne peut
/// pas représenter — les deux couleurs saturées se moyennent en un violet
/// délavé dès que deux colonnes voisines partagent un même bloc chroma. Le
/// 4:4:4 garde une chroma pleine résolution et n'a pas ce problème. C'est
/// cette différence, mesurable en PSNR sur les plans U/V, que la Tâche 4
/// vérifie.
///
/// (Première version : glyphes gris clairs sur fond clair, sans aucune
/// saturation de couleur — U et V y étaient déjà proches de zéro avant tout
/// encodage, donc le sous-échantillonnage n'y perdait rien à mesurer. Motif
/// corrigé pour donner à la thèse du projet une chance réelle d'être
/// infirmée par la mesure, pas seulement confirmée par construction.)
fn motif_detaille(largeur: u32, hauteur: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (largeur as usize) * (hauteur as usize) * 4];
    for y in 0..hauteur {
        for x in 0..largeur {
            let dans_panneau = (PANNEAU_X0..PANNEAU_X0 + PANNEAU_LARGEUR).contains(&x)
                && (PANNEAU_Y0..PANNEAU_Y0 + PANNEAU_HAUTEUR).contains(&y);

            // Ligne de texte : 11 px d'encre + 6 px d'interligne, motif
            // proche d'un corps 12 px. Les groupes de colonnes (x/9) qui
            // tombent sur `% 3 == 0` restent en fond : ce sont les espaces
            // entre les « mots ». Rien de ceci n'est évalué hors du panneau
            // (soustraction non signée sinon en underflow) : `dans_panneau`
            // court-circuite les deux `&&` avant.
            let glyphe = dans_panneau && {
                let xr = x - PANNEAU_X0;
                let yr = y - PANNEAU_Y0;
                (yr % 17 < 11) && !(xr / 9 + yr / 17).is_multiple_of(3)
            };

            let (b, g, r) = if glyphe {
                if x % 2 == 0 {
                    (0, 0, 255) // BGR -> rouge pur (255,0,0)
                } else {
                    (255, 0, 0) // BGR -> bleu pur (0,0,255)
                }
            } else {
                (24, 20, 18) // fond sombre d'éditeur de code / bureau
            };
            let i = ((y as usize) * (largeur as usize) + x as usize) * 4;
            buf[i] = b;
            buf[i + 1] = g;
            buf[i + 2] = r;
            buf[i + 3] = 255;
        }
    }
    buf
}

/// Écrit `nb_images` images BGRA brutes (rawvideo, sans en-tête) dans
/// `chemin` : c'est exactement la séquence de pixels que
/// [`TextureSynthetique`] envoie à NVENC, image par image, mais calculée
/// entièrement côté CPU — pas besoin de lecture GPU->CPU.
///
/// `pub(crate)` : sert de référence non compressée à `cmd_codecs` (Q3) pour
/// le calcul de PSNR/SSIM. Recalculer le motif ici plutôt que de faire un
/// aller-retour GPU garantit en plus l'exactitude bit à bit avec ce que les
/// quatre encodeurs ont reçu, puisque c'est la même fonction `motif_detaille`
/// et la même arithmétique de fenêtre que [`TextureSynthetique::prochaine_image`].
pub(crate) fn ecrire_reference_brute(
    largeur: u32,
    hauteur: u32,
    nb_images: u32,
    chemin: &str,
) -> anyhow::Result<()> {
    let atlas = motif_detaille(largeur, hauteur + MARGE_LIGNES);
    let pas = (largeur as usize) * 4;
    let mut fichier = File::create(chemin).with_context(|| format!("création de {chemin}"))?;

    for n in 0..nb_images {
        let ligne = (n % MARGE_LIGNES) as usize;
        let debut = ligne * pas;
        let fin = debut + (hauteur as usize) * pas;
        fichier.write_all(&atlas[debut..fin])?;
    }
    fichier.flush()?;
    Ok(())
}
