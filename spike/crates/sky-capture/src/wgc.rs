use std::time::{Duration, Instant};

use anyhow::{anyhow, Context};
use windows::core::Interface;
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
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
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
}

impl WgcCapture {
    pub fn new(monitor_index: usize) -> anyhow::Result<Self> {
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

        // 4. Le frame pool. 2 tampons suffisent et minimisent la latence :
        //    davantage ne ferait qu'accumuler des images périmées.
        let frame_pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            2,
            size,
        )
        .context("création du frame pool")?;

        let session = frame_pool.CreateCaptureSession(&item)?;

        // Windows 11 build >= 22000 : supprime la bordure jaune de capture.
        let _ = session.SetIsBorderRequired(false);
        // Le curseur est transmis séparément (spec §6.3), pas incrusté ici.
        let _ = session.SetIsCursorCaptureEnabled(false);

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
            if Instant::now() >= deadline {
                // "dropped" compte les délais d'attente dépassés (timeouts de
                // next_frame), pas des images perdues par WGC lui-même : le nom
                // vient de l'interface attendue par les tâches suivantes.
                self.dropped += 1;
                return Ok(None);
            }
            std::thread::sleep(Duration::from_micros(200));
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
