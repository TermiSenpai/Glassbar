//! Acrylic por instantánea: captura de pantalla, desenfoque, tinte, grano, borde y sombra.

use super::device::{Gfx, bitmap_props};
use super::draw::{Surface, brush, d2d_rect, draw, rrect};
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
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// Copia una región de la pantalla en píxeles físicos. `None` si falla o si sale toda negra.
pub fn capture_screen(x: i32, y: i32, w: u32, h: u32) -> Option<Capture> {
    if w == 0 || h == 0 {
        return None;
    }
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
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
        let ok = BitBlt(mem, 0, 0, w as i32, h as i32, Some(screen), x, y, SRCCOPY).is_ok();
        let mut out = Vec::new();
        if ok && !bits.is_null() {
            out = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
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
        Some(Capture { w, h, bgra: out })
    }
}

thread_local! {
    static HDR_CACHE: RefCell<HashMap<isize, bool>> = RefCell::new(HashMap::new());
}

/// La captura GDI de una salida HDR sale oscura; en ese caso se usa un plato plano.
pub fn output_is_hdr(hmonitor: HMONITOR) -> bool {
    let key = hmonitor.0 as isize;
    if let Some(v) = HDR_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return v;
    }
    let v = unsafe { probe_hdr(hmonitor) };
    HDR_CACHE.with(|c| c.borrow_mut().insert(key, v));
    v
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

/// Dibuja en `s` el plato de cristal. `capture` corresponde exactamente al rectángulo del plato en físicos.
pub fn render_plate(gfx: &Gfx, s: &Surface, scale: f32, spec: &PlateSpec, capture: Option<&Capture>, pal: &Palette) -> Result<()> {
    unsafe {
        let full = D2D_SIZE_U { width: s.w, height: s.h };
        let plate_phys = Rect::new(spec.plate.x * scale, spec.plate.y * scale, spec.plate.w * scale, spec.plate.h * scale);
        let rr_phys = rrect(plate_phys, spec.radius * scale);
        let odc = &gfx.dc;

        // 1. Fondo desenfocado en un bitmap del tamaño de la superficie (en físicos).
        let blurred = match capture {
            Some(c) => {
                let target = odc.CreateBitmap(full, None, 0, &bitmap_props(true))?;
                odc.SetTarget(&target);
                odc.BeginDraw();
                odc.SetTransform(&scale_translate(1.0, 1.0, 0.0, 0.0));
                odc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
                let src = odc.CreateBitmap(D2D_SIZE_U { width: c.w, height: c.h }, Some(c.bgra.as_ptr() as *const c_void), c.w * 4, &bitmap_props(false))?;
                let blur = odc.CreateEffect(&CLSID_D2D1GaussianBlur)?;
                blur.SetInput(0, &src, true);
                let sd: f32 = 20.0 * scale;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sd.to_le_bytes())?;
                let bm: u32 = D2D1_BORDER_MODE_HARD.0 as u32;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_ENUM, &bm.to_le_bytes())?;
                let out = blur.GetOutput()?;
                odc.DrawImage(&out, Some(&pt(plate_phys.x, plate_phys.y)), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
                let r = odc.EndDraw(None, None);
                odc.SetTarget(None);
                r?;
                Some(target)
            }
            None => None,
        };

        // 2. Silueta para la sombra.
        let silhouette = if spec.shadow {
            let target = odc.CreateBitmap(full, None, 0, &bitmap_props(true))?;
            odc.SetTarget(&target);
            odc.BeginDraw();
            odc.SetTransform(&scale_translate(1.0, 1.0, 0.0, 0.0));
            odc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            let black = brush(odc, Color::black(0.42))?;
            odc.FillRoundedRectangle(&rr_phys, &black);
            let r = odc.EndDraw(None, None);
            odc.SetTarget(None);
            r?;
            Some(target)
        } else {
            None
        };

        let noise = noise_bitmap(gfx)?;

        // 3. Superficie final, en físicos (scale = 1 dentro de `draw`, ya escalamos aquí).
        draw(s, 1.0, |dc| {
            if let Some(sil) = &silhouette {
                let shadow = dc.CreateEffect(&CLSID_D2D1Shadow)?;
                shadow.SetInput(0, sil, true);
                let ssd: f32 = 10.0 * scale;
                shadow.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &ssd.to_le_bytes())?;
                let sout = shadow.GetOutput()?;
                dc.DrawImage(&sout, Some(&pt(0.0, 6.0 * scale)), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            }
            if let Some(bl) = &blurred {
                let bb = dc.CreateBitmapBrush(bl, None, None)?;
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
            let _ = d2d_rect;
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
