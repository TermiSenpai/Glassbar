//! Dispositivos: Direct3D 11 WARP, Direct2D, DirectWrite, WIC y DirectComposition.
//!
//! La parte pesada (D3D11, D2D, DWrite, WIC) se crea en un hilo aparte al arrancar y se
//! espera solo cuando hace falta; mientras tanto el hilo de interfaz carga datos, crea la
//! ventana y captura la pantalla. DirectComposition se crea en el hilo de interfaz.

use std::cell::{OnceCell, RefCell};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::thread::JoinHandle;
use windows::Win32::Foundation::E_FAIL;
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

pub struct GfxBase {
    pub d3d: ID3D11Device,
    pub d2d_factory: ID2D1Factory1,
    pub d2d_device: ID2D1Device,
    /// Contexto de trabajo: bitmaps, efectos y dibujo fuera de pantalla.
    pub dc: ID2D1DeviceContext,
    pub dwrite: IDWriteFactory,
    pub wic: IWICImagingFactory,
}

/// Los objetos se crean en el hilo de arranque y se usan solo desde el hilo de interfaz.
/// D3D11 es libre de hilos; la fábrica D2D de un solo hilo no está ligada a un hilo concreto;
/// DirectWrite compartida y WIC (`ThreadingModel=Both`) admiten cualquier apartamento.
struct SendBase(Result<GfxBase>);
unsafe impl Send for SendBase {}

pub struct Gfx {
    base: OnceCell<GfxBase>,
    dcomp: OnceCell<IDCompositionDesktopDevice>,
    handle: RefCell<Option<JoinHandle<SendBase>>>,
    stroke: OnceCell<ID2D1StrokeStyle>,
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

/// Siempre WARP: crear el dispositivo por hardware carga el driver de la GPU y tarda
/// entre 350 ms y 2,6 s por proceso (docs/perf.md). DWM compone y anima en GPU igualmente.
pub fn create_base() -> Result<GfxBase> {
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
        let d3d = d3d.ok_or_else(|| Error::from(E_FAIL))?;
        let dxgi: IDXGIDevice = d3d.cast()?;
        let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d_device = d2d_factory.CreateDevice(&dxgi)?;
        let dc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
        let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER)?;
        warm_up(&dc, &dwrite);
        Ok(GfxBase { d3d, d2d_factory, d2d_device, dc, dwrite, wic })
    }
}

/// Primer uso de los efectos y de las fuentes, que es lo que cuesta: así no cae en el primer frame.
fn warm_up(dc: &ID2D1DeviceContext, dwrite: &IDWriteFactory) {
    unsafe {
        let run = || -> Result<()> {
            let px = [0u8; 4 * 4 * 4];
            let src = dc.CreateBitmap(D2D_SIZE_U { width: 4, height: 4 }, Some(px.as_ptr() as *const c_void), 16, &bitmap_props(false))?;
            let target = dc.CreateBitmap(D2D_SIZE_U { width: 4, height: 4 }, None, 0, &bitmap_props(true))?;
            dc.SetTarget(&target);
            dc.BeginDraw();
            let blur = dc.CreateEffect(&CLSID_D2D1GaussianBlur)?;
            blur.SetInput(0, &src, true);
            dc.DrawImage(&blur.GetOutput()?, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            let shadow = dc.CreateEffect(&CLSID_D2D1Shadow)?;
            shadow.SetInput(0, &src, true);
            dc.DrawImage(&shadow.GetOutput()?, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            let brush = dc.CreateBitmapBrush(&src, None, None)?;
            dc.FillRectangle(&D2D_RECT_F { left: 0.0, top: 0.0, right: 4.0, bottom: 4.0 }, &brush);
            let r = dc.EndDraw(None, None);
            dc.SetTarget(None);
            r?;
            Ok(())
        };
        let _ = run();
        // La colección de fuentes tarda ~15 ms; se calienta aparte, fuera del camino del primer frame.
        let dw = SendDWrite(dwrite.clone());
        let _ = std::thread::Builder::new().name("glassbar-fonts".into()).spawn(move || {
            let dw = dw;
            let mut coll: Option<IDWriteFontCollection> = None;
            let _ = dw.0.GetSystemFontCollection(&mut coll, false);
        });
    }
}

/// La fábrica compartida de DirectWrite es segura entre hilos.
struct SendDWrite(IDWriteFactory);
unsafe impl Send for SendDWrite {}

impl Gfx {
    /// Arranca la creación de dispositivos en un hilo. No bloquea.
    pub fn start() -> Gfx {
        let handle = std::thread::Builder::new()
            .name("glassbar-gfx".into())
            .spawn(|| {
                co_init();
                SendBase(create_base())
            })
            .ok();
        Gfx { base: OnceCell::new(), dcomp: OnceCell::new(), handle: RefCell::new(handle), stroke: OnceCell::new() }
    }

    /// Crea los dispositivos en el hilo actual. Para tests y herramientas.
    pub fn create() -> Result<Gfx> {
        let g = Gfx { base: OnceCell::new(), dcomp: OnceCell::new(), handle: RefCell::new(None), stroke: OnceCell::new() };
        let _ = g.base.set(create_base()?);
        Ok(g)
    }

    pub fn base(&self) -> Result<&GfxBase> {
        if let Some(b) = self.base.get() {
            return Ok(b);
        }
        let handle = self.handle.borrow_mut().take();
        let b = match handle {
            Some(h) => h.join().map_err(|_| Error::from(E_FAIL))?.0?,
            None => create_base()?,
        };
        let _ = self.base.set(b);
        Ok(self.base.get().expect("set"))
    }

    pub fn dcomp(&self) -> Result<&IDCompositionDesktopDevice> {
        if let Some(d) = self.dcomp.get() {
            return Ok(d);
        }
        let base = self.base()?;
        let d: IDCompositionDesktopDevice = unsafe { DCompositionCreateDevice2(&base.d2d_device)? };
        let _ = self.dcomp.set(d);
        Ok(self.dcomp.get().expect("set"))
    }

    pub fn dc(&self) -> Result<&ID2D1DeviceContext> {
        Ok(&self.base()?.dc)
    }

    pub fn dwrite(&self) -> Result<&IDWriteFactory> {
        Ok(&self.base()?.dwrite)
    }

    pub fn wic(&self) -> Result<&IWICImagingFactory> {
        Ok(&self.base()?.wic)
    }

    pub fn d2d_factory(&self) -> Result<&ID2D1Factory1> {
        Ok(&self.base()?.d2d_factory)
    }

    /// Estilo de trazo redondeado compartido, para glifos.
    pub fn round_stroke(&self) -> Result<&ID2D1StrokeStyle> {
        if let Some(s) = self.stroke.get() {
            return Ok(s);
        }
        let s1: ID2D1StrokeStyle1 = unsafe {
            self.d2d_factory()?.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES1 {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                    transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
                },
                None,
            )?
        };
        let s: ID2D1StrokeStyle = s1.cast()?;
        let _ = self.stroke.set(s);
        Ok(self.stroke.get().expect("set"))
    }

    /// Bitmap premultiplicado de `px`×`px` a partir de BGRA.
    pub fn bitmap_from_bgra(&self, px: u32, bgra: &[u8]) -> Result<ID2D1Bitmap1> {
        if bgra.len() != (px * px * 4) as usize {
            return Err(Error::from(windows::Win32::Foundation::E_INVALIDARG));
        }
        unsafe { self.dc()?.CreateBitmap(D2D_SIZE_U { width: px, height: px }, Some(bgra.as_ptr() as *const c_void), px * 4, &bitmap_props(false)) }
    }

    pub fn is_device_lost(err: &Error) -> bool {
        let hr = err.code();
        // D2DERR_RECREATE_TARGET = 0x8899000C
        hr == DXGI_ERROR_DEVICE_REMOVED || hr == DXGI_ERROR_DEVICE_RESET || hr == HRESULT(0x8899000Cu32 as i32)
    }
}
