//! Dispositivos: Direct3D 11 WARP, Direct2D, DirectWrite, WIC y DirectComposition.

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;
use windows::core::*;

pub struct Gfx {
    pub d3d: ID3D11Device,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    /// Contexto de trabajo: bitmaps, efectos y dibujo fuera de pantalla.
    pub dc: ID2D1DeviceContext,
    pub dwrite: IDWriteFactory,
    pub wic: IWICImagingFactory,
    pub dcomp: IDCompositionDesktopDevice,
}

/// Inicializa COM en el hilo actual. Idempotente.
pub fn co_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

pub fn bitmap_props(target: bool) -> D2D1_BITMAP_PROPERTIES1 {
    D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: if target { D2D1_BITMAP_OPTIONS_TARGET } else { D2D1_BITMAP_OPTIONS_NONE },
        colorContext: ManuallyDrop::new(None),
    }
}

impl Gfx {
    /// Siempre WARP: crear el dispositivo por hardware carga el driver de la GPU y tarda
    /// entre 350 ms y 2,6 s por proceso (docs/perf.md). DWM compone y anima en GPU igualmente.
    pub fn create() -> Result<Gfx> {
        unsafe {
            let mut d3d: Option<ID3D11Device> = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_WARP,
                windows::Win32::Foundation::HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_SINGLETHREADED,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d = d3d.ok_or_else(|| Error::from(windows::Win32::Foundation::E_FAIL))?;
            let dxgi: IDXGIDevice = d3d.cast()?;
            let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device = d2d_factory.CreateDevice(&dxgi)?;
            let dc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER)?;
            let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d_device)?;
            Ok(Gfx { d3d, d2d_factory, d2d_device, dc, dwrite, wic, dcomp })
        }
    }

    /// Bitmap premultiplicado de `px`×`px` a partir de BGRA.
    pub fn bitmap_from_bgra(&self, px: u32, bgra: &[u8]) -> Result<ID2D1Bitmap1> {
        if bgra.len() != (px * px * 4) as usize {
            return Err(Error::from(windows::Win32::Foundation::E_INVALIDARG));
        }
        unsafe { self.dc.CreateBitmap(D2D_SIZE_U { width: px, height: px }, Some(bgra.as_ptr() as *const c_void), px * 4, &bitmap_props(false)) }
    }

    pub fn is_device_lost(err: &Error) -> bool {
        let hr = err.code();
        // D2DERR_RECREATE_TARGET = 0x8899000C
        hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET || hr == HRESULT(0x8899000Cu32 as i32)
    }
}
