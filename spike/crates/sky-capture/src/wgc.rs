use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use windows::core::Interface;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11Texture2D, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
    D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIDevice, IDXGIFactory1};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR, MONITORENUMPROC};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use crate::{CaptureStats, CapturedFrame};

pub struct WgcCapture {
    _item: GraphicsCaptureItem,
    session: GraphicsCaptureSession,
    frame_pool: Direct3D11CaptureFramePool,
    d3d_device: ID3D11Device,
    started: Instant,
    frames: u64,
    dropped: u64,
    width: u32,
    height: u32,
    /// Signalé par WGC à chaque image disponible. Remplace le sondage actif :
    /// on dort jusqu'au réveil au lieu de brûler un cœur à demander.
    reveil: Receiver<()>,
}

impl WgcCapture {
    /// `fps_max` borne la cadence de capture. `None` = aucune limite : on capture
    /// chaque image que produit l'écran.
    pub fn new(monitor_index: usize, fps_max: Option<u32>) -> anyhow::Result<Self> {
        let hmonitor = enumerate_monitors()?
            .into_iter()
            .nth(monitor_index)
            .ok_or_else(|| anyhow!("écran {monitor_index} introuvable"))?;

        // 1. Device D3D11 matériel, avec support BGRA exigé par WGC.
        let mut d3d_device: Option<ID3D11Device> = None;
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d_device),
                None,
                None,
            )
        }
        .context("création du device D3D11")?;
        let d3d_device = d3d_device.ok_or_else(|| anyhow!("device D3D11 nul"))?;

        // 2. Pont D3D11 -> WinRT, exigé par le frame pool.
        let dxgi: IDXGIDevice = d3d_device.cast()?;
        let winrt_device =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.context("pont WinRT")?;
        let winrt_device: windows::Graphics::DirectX::Direct3D11::IDirect3DDevice =
            winrt_device.cast()?;

        // 3. L'item de capture, obtenu via l'interface d'interop COM.
        let interop: IGraphicsCaptureItemInterop =
            windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem =
            unsafe { interop.CreateForMonitor(hmonitor) }.context("CreateForMonitor")?;

        let size = item.Size()?;
        let (width, height) = (size.Width as u32, size.Height as u32);

        // 4. Le frame pool. 6 tampons : de quoi absorber une pause de l'appelant
        //    sans que WGC recycle une image qu'on n'a pas encore lue.
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            6,
            size,
        )
        .context("création du frame pool")?;

        let session = frame_pool.CreateCaptureSession(&item)?;

        // Windows 11 build >= 22000 : supprime la bordure jaune de capture.
        let _ = session.SetIsBorderRequired(false);
        // Le curseur est transmis séparément (spec §6.3), pas incrusté ici.
        let _ = session.SetIsCursorCaptureEnabled(false);

        // CORRECTIF (23/08/2026) — le plafond de capture le plus coûteux du projet.
        //
        // WGC impose par défaut un intervalle minimal de 16 ms entre deux images,
        // soit 62,5 im/s. Ce défaut n'est mis en avant nulle part et se compose
        // avec la grille de rafraîchissement de l'écran : sur un 165 Hz (6,03 ms
        // par rafraîchissement), la première image autorisée après 16 ms tombe au
        // 3e rafraîchissement et non au 2e — la capture s'effondre à 55 im/s, soit
        // exactement un tiers du taux d'écran.
        //
        // On règle donc l'intervalle sur la cadence réellement voulue. Laisser WGC
        // limiter lui-même vaut mieux que capturer trop puis jeter : les images en
        // trop ne sont jamais produites.
        //
        // L'API demande Windows 11 ; sur un système plus ancien l'appel échoue et
        // la capture reste plafonnée à 60 im/s. C'est une dégradation acceptable,
        // pas une erreur : on la signale sans interrompre.
        let intervalle = intervalle_min_pour(fps_max);
        if let Err(e) =
            session.SetMinUpdateInterval(windows::Foundation::TimeSpan { Duration: intervalle })
        {
            eprintln!(
                "Cadence de capture non réglable sur ce système ({e}) :                  plafond de 60 im/s hérité de Windows."
            );
        }

        // On s'abonne AVANT de démarrer : une image arrivée entre l'abonnement et
        // le premier appel serait sinon perdue.
        //
        // Capacité 1 et `try_send` : le signal dit « au moins une image est
        // prête », pas « en voici une de plus ». Un canal plein n'a rien à
        // apprendre de plus, et le gestionnaire ne doit jamais bloquer — il tourne
        // sur un thread de WGC.
        let (tx, reveil) = sync_channel::<()>(1);
        frame_pool
            .FrameArrived(&TypedEventHandler::<
                Direct3D11CaptureFramePool,
                windows::core::IInspectable,
            >::new(move |_, _| {
                let _ = tx.try_send(());
                Ok(())
            }))
            .context("abonnement à FrameArrived")?;

        session.StartCapture().context("StartCapture")?;

        Ok(Self {
            _item: item,
            session,
            frame_pool,
            d3d_device,
            started: Instant::now(),
            frames: 0,
            dropped: 0,
            width,
            height,
            reveil,
        })
    }

    /// Le device D3D11 qui a produit les textures. La Tâche 3 ouvre sa
    /// session NVENC sur ce même device pour éviter tout partage inter-GPU.
    pub fn d3d_device(&self) -> &ID3D11Device {
        &self.d3d_device
    }

    /// Récupère l'image suivante. Renvoie Ok(None) si aucune image n'est prête
    /// dans le délai imparti — cas normal quand l'écran est statique.
    pub fn next_frame(&mut self, timeout: Duration) -> anyhow::Result<Option<CapturedFrame>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(frame) = self.frame_pool.TryGetNextFrame() {
                let surface = frame.Surface()?;
                let access: IDirect3DDxgiInterfaceAccess = surface.cast()?;
                let texture: ID3D11Texture2D = unsafe { access.GetInterface() }?;

                self.frames += 1;
                return Ok(Some(CapturedFrame {
                    texture,
                    width: self.width,
                    height: self.height,
                    captured_at: Instant::now(),
                }));
            }
            let reste = deadline.saturating_duration_since(Instant::now());
            if reste.is_zero() {
                // Compte les délais d'attente dépassés, pas des images perdues par
                // WGC : le nom vient de l'interface attendue par les tâches suivantes.
                self.dropped += 1;
                return Ok(None);
            }
            match self.reveil.recv_timeout(reste) {
                Ok(()) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    self.dropped += 1;
                    return Ok(None);
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(anyhow!("la capture s'est arrêtée"));
                }
            }
        }
    }

    pub fn stats(&self) -> CaptureStats {
        let secs = self.started.elapsed().as_secs_f32().max(0.001);
        CaptureStats {
            frames: self.frames,
            dropped: self.dropped,
            avg_fps: self.frames as f32 / secs,
        }
    }
}

impl Drop for WgcCapture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.frame_pool.Close();
    }
}

fn enumerate_monitors() -> anyhow::Result<Vec<HMONITOR>> {
    unsafe extern "system" fn cb(
        hmon: HMONITOR,
        _hdc: HDC,
        _rect: *mut windows::Win32::Foundation::RECT,
        data: windows::Win32::Foundation::LPARAM,
    ) -> windows::core::BOOL {
        // Sûr : `data` porte l'adresse de `out` (ci-dessous), posée juste avant
        // l'appel à `EnumDisplayMonitors` et valide pour toute sa durée — Windows
        // n'invoque ce callback que de façon synchrone, pendant cet appel.
        let out = &mut *(data.0 as *mut Vec<HMONITOR>);
        out.push(hmon);
        true.into()
    }

    let mut out: Vec<HMONITOR> = Vec::new();
    // `MONITORENUMPROC` est déjà un `Option<extern "system" fn(...)>` dans
    // cette version de `windows-rs` : on passe directement `Some(cb)`, sans
    // caster vers l'alias (qui inclurait un second niveau d'Option).
    let lpfnenum: MONITORENUMPROC = Some(cb);
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            lpfnenum,
            windows::Win32::Foundation::LPARAM(&mut out as *mut _ as isize),
        )
        .ok()
        .context("EnumDisplayMonitors")?;
    }
    Ok(out)
}

/// INSTRUMENTATION TEMPORAIRE (23/08/2026) — compte les rafraîchissements réels
/// de l'écran pendant une durée donnée.
///
/// Tranche une question à laquelle le taux annoncé par Windows ne répond pas.
/// L'écran est configuré à 165 Hz, mais le VRR couvre 50-170 Hz : le taux réel
/// varie avec le contenu. Si le compositeur ne rafraîchit qu'à ~55 Hz pendant la
/// mesure, la capture ne PEUT pas dépasser 55 images/s — WGC n'émet qu'à la
/// composition — et notre boucle est hors de cause.
///
/// À supprimer une fois la question tranchée.
pub fn compter_vblanks(duree: Duration) -> anyhow::Result<u64> {
    // Factory propre au thread : les objets COM ne traversent pas les threads.
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.context("CreateDXGIFactory1")?;
    let adapter = unsafe { factory.EnumAdapters(0) }.context("EnumAdapters")?;
    let output = unsafe { adapter.EnumOutputs(0) }.context("EnumOutputs")?;

    let fin = Instant::now() + duree;
    let mut n = 0u64;
    while Instant::now() < fin {
        unsafe { output.WaitForVBlank() }.context("WaitForVBlank")?;
        n += 1;
    }
    Ok(n)
}

/// Convertit une cadence maximale en intervalle minimal WGC (unités de 100 ns).
///
/// `None` — ou une cadence nulle, qui n'aurait pas de sens — donne 0 : aucune
/// limite, la capture suit le taux de rafraîchissement de l'écran.
pub fn intervalle_min_pour(fps_max: Option<u32>) -> i64 {
    match fps_max {
        Some(fps) if fps > 0 => 10_000_000 / fps as i64,
        _ => 0,
    }
}

/// Cadence réellement atteignable pour une demande donnée.
///
/// WGC n'émet qu'aux rafraîchissements de l'écran : les seules cadences
/// possibles sont le taux d'écran divisé par un entier. Sur un 165 Hz, demander
/// 120 im/s ne donne pas 120 mais 82,9 — le sous-multiple juste en dessous.
/// Mesuré, pas déduit : 120 demandées ont rendu 82,9, et 60 en ont rendu 55,3.
///
/// Sans cette fonction, l'interface promettrait une cadence que le matériel ne
/// peut pas produire.
pub fn cadence_atteignable(fps_demande: u32, hz_ecran: f32) -> f32 {
    if fps_demande == 0 || hz_ecran <= 0.0 {
        return hz_ecran.max(0.0);
    }
    let brut = hz_ecran / fps_demande as f32;
    // Tolérance de 2 % : demander 165 sur un écran mesuré à 165,8 Hz doit donner
    // le taux plein, pas la moitié à cause d'un arrondi de mesure.
    let diviseur = if (brut - brut.round()).abs() < 0.02 {
        brut.round()
    } else {
        brut.ceil()
    };
    hz_ecran / diviseur.max(1.0)
}

#[cfg(test)]
mod tests {
    use super::{cadence_atteignable, intervalle_min_pour};

    #[test]
    fn cadence_pleine_si_on_demande_le_taux_ecran() {
        // 165 demandées sur 165,8 mesurés : la tolérance évite de tomber à 82,9.
        assert!((cadence_atteignable(165, 165.8) - 165.8).abs() < 0.1);
    }

    #[test]
    fn cent_vingt_sur_ecran_165_donne_la_moitie() {
        // Valeur mesurée sur la machine de référence : 82,9.
        assert!((cadence_atteignable(120, 165.8) - 82.9).abs() < 0.1);
    }

    #[test]
    fn soixante_sur_ecran_165_donne_le_tiers() {
        // Valeur mesurée sur la machine de référence : 55,3.
        assert!((cadence_atteignable(60, 165.8) - 55.27).abs() < 0.1);
    }

    #[test]
    fn soixante_sur_ecran_60_donne_soixante() {
        // Sur un écran 60 Hz, aucune quantification : la demande est servie.
        assert!((cadence_atteignable(60, 60.0) - 60.0).abs() < 0.1);
    }

    #[test]
    fn demande_absurde_bornee_au_taux_ecran() {
        // On ne peut pas capturer plus vite que l'écran ne rafraîchit.
        assert!((cadence_atteignable(1000, 165.8) - 165.8).abs() < 0.1);
    }

    #[test]
    fn aucune_limite_demandee() {
        assert_eq!(intervalle_min_pour(None), 0);
    }

    #[test]
    fn cadence_nulle_traitee_comme_aucune_limite() {
        // Sans ce garde-fou, la division renverrait une erreur d'exécution.
        assert_eq!(intervalle_min_pour(Some(0)), 0);
    }

    #[test]
    fn soixante_images_par_seconde() {
        assert_eq!(intervalle_min_pour(Some(60)), 166_666);
    }

    #[test]
    fn cent_vingt_images_par_seconde() {
        assert_eq!(intervalle_min_pour(Some(120)), 83_333);
    }

    #[test]
    fn cent_soixante_cinq_images_par_seconde() {
        assert_eq!(intervalle_min_pour(Some(165)), 60_606);
    }

    #[test]
    fn strictement_sous_le_defaut_de_wgc() {
        // Le défaut non documenté de WGC vaut 160 000 (16 ms, 62,5 im/s). Toute
        // cadence au-dessus de 62 im/s doit produire un intervalle plus court,
        // sinon le plafond se réinstalle en silence.
        for fps in [63, 60, 90, 120, 144, 165, 240] {
            let intervalle = intervalle_min_pour(Some(fps));
            if fps > 62 {
                assert!(
                    intervalle < 160_000,
                    "{fps} im/s donne {intervalle}, au-dessus du défaut WGC"
                );
            }
        }
    }
}
