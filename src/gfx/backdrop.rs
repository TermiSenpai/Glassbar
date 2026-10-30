//! Acrylic por instantánea: captura de pantalla, desenfoque, tinte, grano, borde y sombra.
//!
//! La captura y el desenfoque se hacen a resolución reducida (`div`): el desenfoque de
//! radio 20 no deja detalle que recuperar y el coste baja con el cuadrado del factor.

use super::device::{Gfx, bitmap_props};
use super::draw::{Surface, brush, draw, rrect};
use super::{pt, scale_translate};
use crate::icons::cache::fnv1a;
use crate::theme::{Color, Palette};
use crate::ui::layout::Rect;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::null_mut;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::core::*;

pub struct Capture {
    /// Tamaño real del búfer (ya reducido).
    pub w: u32,
    pub h: u32,
    /// Factor por el que hay que ampliar para cubrir el plato.
    pub div: u32,
    pub bgra: Vec<u8>,
}

/// Copia una región de la pantalla en píxeles físicos, reducida `div` veces. `None` si falla o si sale toda negra.
pub fn capture_screen_div(x: i32, y: i32, w: u32, h: u32, div: u32) -> Option<Capture> {
    if w == 0 || h == 0 {
        return None;
    }
    let div = div.max(1);
    let (cw, ch) = ((w / div).max(1), (h / div).max(1));
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cw as i32,
                biHeight: -(ch as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = null_mut();
        let hbm = match CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(_) => {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return None;
            }
        };
        let old = SelectObject(mem, hbm.into());
        let ok = if div == 1 {
            BitBlt(mem, 0, 0, w as i32, h as i32, Some(screen), x, y, SRCCOPY).is_ok()
        } else {
            SetStretchBltMode(mem, HALFTONE);
            StretchBlt(mem, 0, 0, cw as i32, ch as i32, Some(screen), x, y, w as i32, h as i32, SRCCOPY).as_bool()
        };
        let mut out = Vec::new();
        if ok && !bits.is_null() {
            out = std::slice::from_raw_parts(bits as *const u8, (cw * ch * 4) as usize).to_vec();
        }
        SelectObject(mem, old);
        let _ = DeleteObject(hbm.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !ok || out.iter().all(|&b| b == 0) {
            return None;
        }
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Some(Capture { w: cw, h: ch, div, bgra: out })
    }
}

pub fn capture_screen(x: i32, y: i32, w: u32, h: u32) -> Option<Capture> {
    capture_screen_div(x, y, w, h, 1)
}

static HDR_CACHE: std::sync::Mutex<Option<HashMap<isize, bool>>> = std::sync::Mutex::new(None);

/// La captura GDI de una salida HDR sale oscura; en ese caso se usa un plato plano.
///
/// La consulta DXGI tarda hasta 17 ms, así que se hace en un hilo aparte y se recuerda por
/// monitor. La primera apertura en un monitor asume SDR.
pub fn output_is_hdr(hmonitor: HMONITOR) -> bool {
    let key = hmonitor.0 as isize;
    let known = HDR_CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&key).copied()));
    if let Some(v) = known {
        return v;
    }
    if let Ok(mut c) = HDR_CACHE.lock() {
        let map = c.get_or_insert_with(HashMap::new);
        if map.contains_key(&key) {
            return map[&key];
        }
        // Marca "en curso" como SDR para no lanzar dos sondeos.
        map.insert(key, false);
    }
    let _ = std::thread::Builder::new().name("glassbar-hdr".into()).spawn(move || {
        let v = unsafe { probe_hdr(HMONITOR(key as *mut c_void)) };
        if let Ok(mut c) = HDR_CACHE.lock() {
            c.get_or_insert_with(HashMap::new).insert(key, v);
        }
    });
    false
}

unsafe fn probe_hdr(hmonitor: HMONITOR) -> bool {
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return false };
        let mut ai = 0;
        while let Ok(adapter) = factory.EnumAdapters1(ai) {
            ai += 1;
            let mut oi = 0;
            while let Ok(output) = adapter.EnumOutputs(oi) {
                oi += 1;
                let Ok(o6) = output.cast::<IDXGIOutput6>() else { continue };
                if let Ok(desc) = o6.GetDesc1() {
                    if desc.Monitor == hmonitor {
                        return desc.ColorSpace == windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
                    }
                }
            }
        }
        false
    }
}

pub struct PlateSpec {
    /// Rectángulo del plato en píxeles lógicos dentro de la superficie.
    pub plate: Rect,
    pub radius: f32,
    pub shadow: bool,
    pub tint: Color,
    pub border: Color,
}

/// Dibuja en `s` el plato de cristal. `capture` corresponde al rectángulo del plato, reducido `capture.div` veces.
pub fn render_plate(gfx: &Gfx, s: &Surface, scale: f32, spec: &PlateSpec, capture: Option<&Capture>, pal: &Palette) -> Result<()> {
    unsafe {
        let div = capture.map(|c| c.div).unwrap_or(2).max(1);
        let divf = div as f32;
        let small = D2D_SIZE_U { width: (s.w / div).max(1), height: (s.h / div).max(1) };
        let plate_phys = Rect::new(spec.plate.x * scale, spec.plate.y * scale, spec.plate.w * scale, spec.plate.h * scale);
        let rr_phys = rrect(plate_phys, spec.radius * scale);
        let plate_small = Rect::new(plate_phys.x / divf, plate_phys.y / divf, plate_phys.w / divf, plate_phys.h / divf);
        let odc = gfx.dc()?;

        // 1. Fondo desenfocado, a resolución reducida, colocado donde irá el plato.
        let blurred = match capture {
            Some(c) => {
                let target = odc.CreateBitmap(small, None, 0, &bitmap_props(true))?;
                odc.SetTarget(&target);
                odc.BeginDraw();
                odc.SetTransform(&scale_translate(1.0, 1.0, 0.0, 0.0));
                odc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
                let src = odc.CreateBitmap(D2D_SIZE_U { width: c.w, height: c.h }, Some(c.bgra.as_ptr() as *const c_void), c.w * 4, &bitmap_props(false))?;
                let blur = odc.CreateEffect(&CLSID_D2D1GaussianBlur)?;
                blur.SetInput(0, &src, true);
                let sd: f32 = 20.0 * scale / divf;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sd.to_le_bytes())?;
                let bm: u32 = D2D1_BORDER_MODE_HARD.0 as u32;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_ENUM, &bm.to_le_bytes())?;
                let out = blur.GetOutput()?;
                odc.DrawImage(&out, Some(&pt(plate_small.x, plate_small.y)), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
                let r = odc.EndDraw(None, None);
                odc.SetTarget(None);
                r?;
                Some(target)
            }
            None => None,
        };

        // 2. Sombra: silueta reducida, desenfocada, en un bitmap reducido.
        let shadow_bmp = if spec.shadow {
            let sil = odc.CreateBitmap(small, None, 0, &bitmap_props(true))?;
            odc.SetTarget(&sil);
            odc.BeginDraw();
            odc.SetTransform(&scale_translate(1.0, 1.0, 0.0, 0.0));
            odc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let black = brush(odc, Color::black(0.42))?;
            odc.FillRoundedRectangle(&rrect(plate_small, spec.radius * scale / divf), &black);
            let r = odc.EndDraw(None, None);
            odc.SetTarget(None);
            r?;
            let out_bmp = odc.CreateBitmap(small, None, 0, &bitmap_props(true))?;
            odc.SetTarget(&out_bmp);
            odc.BeginDraw();
            odc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let shadow = odc.CreateEffect(&CLSID_D2D1Shadow)?;
            shadow.SetInput(0, &sil, true);
            let ssd: f32 = 10.0 * scale / divf;
            shadow.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &ssd.to_le_bytes())?;
            let sout = shadow.GetOutput()?;
            odc.DrawImage(&sout, Some(&pt(0.0, 6.0 * scale / divf)), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            let r = odc.EndDraw(None, None);
            odc.SetTarget(None);
            r?;
            Some(out_bmp)
        } else {
            None
        };

        let noise = noise_bitmap(gfx)?;
        let up = D2D1_BITMAP_BRUSH_PROPERTIES1 { extendModeX: D2D1_EXTEND_MODE_CLAMP, extendModeY: D2D1_EXTEND_MODE_CLAMP, interpolationMode: D2D1_INTERPOLATION_MODE_LINEAR };
        let up_props = D2D1_BRUSH_PROPERTIES { opacity: 1.0, transform: scale_translate(divf, divf, 0.0, 0.0) };

        // 3. Superficie final en físicos.
        draw(s, 1.0, |dc| {
            if let Some(sb) = &shadow_bmp {
                let b = dc.CreateBitmapBrush(sb, Some(&up), Some(&up_props))?;
                dc.FillRectangle(&D2D_RECT_F { left: 0.0, top: 0.0, right: s.w as f32, bottom: s.h as f32 }, &b);
            }
            if let Some(bl) = &blurred {
                let bb = dc.CreateBitmapBrush(bl, Some(&up), Some(&up_props))?;
                dc.FillRoundedRectangle(&rr_phys, &bb);
            }
            let tint = brush(dc, spec.tint)?;
            dc.FillRoundedRectangle(&rr_phys, &tint);
            let props = D2D1_BITMAP_BRUSH_PROPERTIES1 {
                extendModeX: D2D1_EXTEND_MODE_WRAP,
                extendModeY: D2D1_EXTEND_MODE_WRAP,
                interpolationMode: D2D1_INTERPOLATION_MODE_NEAREST_NEIGHBOR,
            };
            let bp = D2D1_BRUSH_PROPERTIES { opacity: if pal.dark { 0.03 } else { 0.02 }, transform: scale_translate(1.0, 1.0, 0.0, 0.0) };
            let nb = dc.CreateBitmapBrush(&noise, Some(&props), Some(&bp))?;
            dc.FillRoundedRectangle(&rr_phys, &nb);
            let border = brush(dc, spec.border)?;
            let inset = plate_phys.inset(0.5 * scale);
            dc.DrawRoundedRectangle(&rrect(inset, spec.radius * scale - 0.5 * scale), &border, 1.0 * scale, None);
            Ok(())
        })
    }
}

thread_local! {
    static NOISE: RefCell<Option<ID2D1Bitmap1>> = const { RefCell::new(None) };
}

/// Textura de grano de 64×64, gris aleatorio opaco. La opacidad la pone la brocha.
fn noise_bitmap(gfx: &Gfx) -> Result<ID2D1Bitmap1> {
    if let Some(b) = NOISE.with(|n| n.borrow().clone()) {
        return Ok(b);
    }
    let mut px = vec![0u8; 64 * 64 * 4];
    for i in 0..64 * 64 {
        let v = (fnv1a(&(i as u32).to_le_bytes()) & 0xFF) as u8;
        px[i * 4] = v;
        px[i * 4 + 1] = v;
        px[i * 4 + 2] = v;
        px[i * 4 + 3] = 255;
    }
    let b = gfx.bitmap_from_bgra(64, &px)?;
    NOISE.with(|n| *n.borrow_mut() = Some(b.clone()));
    Ok(b)
}
