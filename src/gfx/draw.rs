//! Dibujo en superficies de DirectComposition con Direct2D y DirectWrite.

use super::device::Gfx;
use super::{pt, scale_translate, translation, wide};
use crate::theme::Color;
use crate::ui::layout::Rect;
use std::sync::OnceLock;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::*;

pub struct Surface {
    pub surface: IDCompositionSurface,
    pub w: u32,
    pub h: u32,
}

pub fn create_surface(gfx: &Gfx, w: u32, h: u32) -> Result<Surface> {
    let surface = unsafe { gfx.dcomp()?.CreateSurface(w.max(1), h.max(1), DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)? };
    Ok(Surface { surface, w: w.max(1), h: h.max(1) })
}

/// BeginDraw/EndDraw. `f` dibuja en píxeles lógicos: la transformación aplica `scale` y el offset de la superficie.
pub fn draw(s: &Surface, scale: f32, f: impl FnOnce(&ID2D1DeviceContext) -> Result<()>) -> Result<()> {
    unsafe {
        let mut off = POINT::default();
        let dc: ID2D1DeviceContext = s.surface.BeginDraw(None, &mut off)?;
        dc.SetTransform(&scale_translate(scale, scale, off.x as f32, off.y as f32));
        dc.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
        let r = f(&dc);
        s.surface.EndDraw()?;
        r
    }
}

pub fn rrect(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() }, radiusX: radius, radiusY: radius }
}

pub fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() }
}

pub fn brush(dc: &ID2D1DeviceContext, c: Color) -> Result<ID2D1SolidColorBrush> {
    unsafe { dc.CreateSolidColorBrush(&c.to_d2d(), None) }
}

pub fn fill_round_rect(dc: &ID2D1DeviceContext, r: Rect, radius: f32, c: Color) -> Result<()> {
    let b = brush(dc, c)?;
    unsafe { dc.FillRoundedRectangle(&rrect(r, radius), &b) };
    Ok(())
}

pub fn stroke_round_rect(dc: &ID2D1DeviceContext, r: Rect, radius: f32, c: Color, width: f32) -> Result<()> {
    let b = brush(dc, c)?;
    let inset = r.inset(width / 2.0);
    unsafe { dc.DrawRoundedRectangle(&rrect(inset, (radius - width / 2.0).max(0.0)), &b, width, None) };
    Ok(())
}

pub fn fill_rect(dc: &ID2D1DeviceContext, r: Rect, c: Color) -> Result<()> {
    let b = brush(dc, c)?;
    unsafe { dc.FillRectangle(&d2d_rect(r), &b) };
    Ok(())
}

pub fn fill_circle(dc: &ID2D1DeviceContext, cx: f32, cy: f32, radius: f32, c: Color) -> Result<()> {
    let b = brush(dc, c)?;
    unsafe { dc.FillEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: radius, radiusY: radius }, &b) };
    Ok(())
}

pub fn stroke_circle(dc: &ID2D1DeviceContext, cx: f32, cy: f32, radius: f32, c: Color, width: f32) -> Result<()> {
    let b = brush(dc, c)?;
    unsafe { dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: radius, radiusY: radius }, &b, width, None) };
    Ok(())
}

pub fn line(dc: &ID2D1DeviceContext, x0: f32, y0: f32, x1: f32, y1: f32, b: &ID2D1SolidColorBrush, width: f32, style: &ID2D1StrokeStyle) {
    unsafe { dc.DrawLine(pt(x0, y0), pt(x1, y1), b, width, Some(style)) };
}

/// Dibuja un bitmap de `src_px`×`src_px` escalado a `dest`, recortado con esquinas de `radius`.
pub fn draw_bitmap(dc: &ID2D1DeviceContext, bmp: &ID2D1Bitmap1, src_px: u32, dest: Rect, radius: f32, opacity: f32) -> Result<()> {
    unsafe {
        let s = dest.w / src_px as f32;
        let props = D2D1_BITMAP_BRUSH_PROPERTIES1 {
            extendModeX: D2D1_EXTEND_MODE_CLAMP,
            extendModeY: D2D1_EXTEND_MODE_CLAMP,
            interpolationMode: D2D1_INTERPOLATION_MODE_HIGH_QUALITY_CUBIC,
        };
        let bp = D2D1_BRUSH_PROPERTIES { opacity, transform: scale_translate(s, dest.h / src_px as f32, dest.x, dest.y) };
        let b = dc.CreateBitmapBrush(bmp, Some(&props), Some(&bp))?;
        if radius > 0.0 {
            dc.FillRoundedRectangle(&rrect(dest, radius), &b);
        } else {
            dc.FillRectangle(&d2d_rect(dest), &b);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextStyle {
    pub size: f32,
    pub bold: bool,
    pub mono: bool,
}

impl TextStyle {
    pub const fn normal(size: f32) -> TextStyle {
        TextStyle { size, bold: false, mono: false }
    }
    pub const fn bold(size: f32) -> TextStyle {
        TextStyle { size, bold: true, mono: false }
    }
    pub const fn mono(size: f32) -> TextStyle {
        TextStyle { size, bold: false, mono: true }
    }
}

static FAMILY: OnceLock<Vec<u16>> = OnceLock::new();

/// "Segoe UI Variable Text" si existe, si no "Segoe UI".
fn family(gfx: &Gfx) -> &'static [u16] {
    FAMILY.get_or_init(|| unsafe {
        let mut coll: Option<IDWriteFontCollection> = None;
        let candidate = wide("Segoe UI Variable Text");
        let Ok(dw) = gfx.dwrite() else { return wide("Segoe UI") };
        if dw.GetSystemFontCollection(&mut coll, false).is_ok() {
            if let Some(c) = coll {
                let mut idx = 0u32;
                let mut exists = BOOL(0);
                if c.FindFamilyName(PCWSTR(candidate.as_ptr()), &mut idx, &mut exists).is_ok() && exists.as_bool() {
                    return candidate;
                }
            }
        }
        wide("Segoe UI")
    })
}

pub fn text_format(gfx: &Gfx, style: &TextStyle) -> Result<IDWriteTextFormat> {
    unsafe {
        let fam = if style.mono { wide("Consolas") } else { family(gfx).to_vec() };
        let weight = if style.bold { DWRITE_FONT_WEIGHT_SEMI_BOLD } else { DWRITE_FONT_WEIGHT_NORMAL };
        let f = gfx.dwrite()?.CreateTextFormat(PCWSTR(fam.as_ptr()), None, weight, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, style.size, w!(""))?;
        f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
        f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        let sign = gfx.dwrite()?.CreateEllipsisTrimmingSign(&f)?;
        f.SetTrimming(&DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 }, &sign)?;
        Ok(f)
    }
}

/// Layout de una línea, recortado con puntos suspensivos a `max_w`; `max_h` define el alto para centrar verticalmente.
pub fn text_layout(gfx: &Gfx, text: &str, style: &TextStyle, max_w: f32, max_h: f32) -> Result<IDWriteTextLayout> {
    unsafe {
        let f = text_format(gfx, style)?;
        let w: Vec<u16> = text.encode_utf16().collect();
        gfx.dwrite()?.CreateTextLayout(&w, &f, max_w.max(1.0), max_h.max(1.0))
    }
}

pub fn measure(layout: &IDWriteTextLayout) -> (f32, f32) {
    let mut m = DWRITE_TEXT_METRICS::default();
    unsafe {
        let _ = layout.GetMetrics(&mut m);
    }
    (m.widthIncludingTrailingWhitespace, m.height)
}

/// Ancho natural de un texto con un estilo, sin recorte.
pub fn text_width(gfx: &Gfx, text: &str, style: &TextStyle) -> f32 {
    match text_layout(gfx, text, style, 10_000.0, 100.0) {
        Ok(l) => measure(&l).0,
        Err(_) => 0.0,
    }
}

pub fn draw_text(dc: &ID2D1DeviceContext, layout: &IDWriteTextLayout, x: f32, y: f32, c: Color) -> Result<()> {
    let b = brush(dc, c)?;
    unsafe { dc.DrawTextLayout(pt(x, y), layout, &b, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT) };
    Ok(())
}

pub fn set_text_align(layout: &IDWriteTextLayout, center: bool) {
    unsafe {
        let _ = layout.SetTextAlignment(if center { DWRITE_TEXT_ALIGNMENT_CENTER } else { DWRITE_TEXT_ALIGNMENT_LEADING });
    }
}

// ---- Glifos, dibujados con líneas dentro de `r` ----

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Glyph {
    Terminal,
    Plus,
    Gear,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    Check,
    Close,
    Folder,
    Pencil,
    ArrowLeft,
    ArrowRight,
    Trash,
    Play,
    Globe,
    Monitor,
    Keyboard,
    Theme,
    Bolt,
    Refresh,
    Download,
}

pub fn glyph(gfx: &Gfx, dc: &ID2D1DeviceContext, g: Glyph, r: Rect, c: Color) -> Result<()> {
    let b = brush(dc, c)?;
    let st: ID2D1StrokeStyle = gfx.round_stroke()?.clone();
    let w = (r.w / 12.0).max(1.4);
    let (cx, cy) = r.center();
    let u = r.w / 24.0; // unidad: el glifo se diseña en una caja de 24×24
    let x = |v: f32| r.x + v * u;
    let y = |v: f32| r.y + v * u;
    unsafe {
        match g {
            Glyph::Terminal => {
                dc.DrawRoundedRectangle(&rrect(Rect::new(x(3.0), y(4.0), 18.0 * u, 16.0 * u), 3.0 * u), &b, w, Some(&st));
                line(dc, x(7.0), y(9.0), x(10.0), y(12.0), &b, w, &st);
                line(dc, x(10.0), y(12.0), x(7.0), y(15.0), &b, w, &st);
                line(dc, x(12.0), y(15.0), x(17.0), y(15.0), &b, w, &st);
            }
            Glyph::Plus => {
                line(dc, cx, y(5.0), cx, y(19.0), &b, w * 1.15, &st);
                line(dc, x(5.0), cy, x(19.0), cy, &b, w * 1.15, &st);
            }
            Glyph::Gear => {
                dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: 3.2 * u, radiusY: 3.2 * u }, &b, w, Some(&st));
                dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: 7.0 * u, radiusY: 7.0 * u }, &b, w, Some(&st));
                for i in 0..8 {
                    let a = i as f32 * std::f32::consts::FRAC_PI_4;
                    let (sx, sy) = (cx + a.cos() * 7.0 * u, cy + a.sin() * 7.0 * u);
                    let (ex, ey) = (cx + a.cos() * 10.0 * u, cy + a.sin() * 10.0 * u);
                    line(dc, sx, sy, ex, ey, &b, w * 1.3, &st);
                }
            }
            Glyph::ChevronLeft => {
                line(dc, x(14.5), y(7.0), x(9.5), cy, &b, w, &st);
                line(dc, x(9.5), cy, x(14.5), y(17.0), &b, w, &st);
            }
            Glyph::ChevronRight => {
                line(dc, x(9.5), y(7.0), x(14.5), cy, &b, w, &st);
                line(dc, x(14.5), cy, x(9.5), y(17.0), &b, w, &st);
            }
            Glyph::ChevronDown => {
                line(dc, x(7.0), y(9.5), cx, y(14.5), &b, w, &st);
                line(dc, cx, y(14.5), x(17.0), y(9.5), &b, w, &st);
            }
            Glyph::Check => {
                line(dc, x(5.0), y(12.5), x(10.0), y(17.0), &b, w * 1.2, &st);
                line(dc, x(10.0), y(17.0), x(19.0), y(7.5), &b, w * 1.2, &st);
            }
            Glyph::Close => {
                line(dc, x(7.0), y(7.0), x(17.0), y(17.0), &b, w, &st);
                line(dc, x(17.0), y(7.0), x(7.0), y(17.0), &b, w, &st);
            }
            Glyph::Folder => {
                line(dc, x(3.0), y(7.0), x(9.0), y(7.0), &b, w, &st);
                line(dc, x(9.0), y(7.0), x(11.0), y(9.0), &b, w, &st);
                line(dc, x(11.0), y(9.0), x(21.0), y(9.0), &b, w, &st);
                dc.DrawRoundedRectangle(&rrect(Rect::new(x(3.0), y(7.0), 18.0 * u, 12.0 * u), 2.0 * u), &b, w, Some(&st));
            }
            Glyph::Pencil => {
                line(dc, x(12.0), y(20.0), x(21.0), y(20.0), &b, w, &st);
                line(dc, x(16.5), y(3.5), x(20.5), y(7.5), &b, w, &st);
                line(dc, x(20.5), y(7.5), x(7.0), y(19.0), &b, w, &st);
                line(dc, x(7.0), y(19.0), x(3.0), y(20.0), &b, w, &st);
                line(dc, x(3.0), y(20.0), x(4.0), y(16.0), &b, w, &st);
                line(dc, x(4.0), y(16.0), x(16.5), y(3.5), &b, w, &st);
            }
            Glyph::ArrowLeft => {
                line(dc, x(19.0), cy, x(5.0), cy, &b, w, &st);
                line(dc, x(12.0), y(5.0), x(5.0), cy, &b, w, &st);
                line(dc, x(5.0), cy, x(12.0), y(19.0), &b, w, &st);
            }
            Glyph::ArrowRight => {
                line(dc, x(5.0), cy, x(19.0), cy, &b, w, &st);
                line(dc, x(12.0), y(5.0), x(19.0), cy, &b, w, &st);
                line(dc, x(19.0), cy, x(12.0), y(19.0), &b, w, &st);
            }
            Glyph::Trash => {
                line(dc, x(3.0), y(6.0), x(21.0), y(6.0), &b, w, &st);
                line(dc, x(8.0), y(6.0), x(8.0), y(4.0), &b, w, &st);
                line(dc, x(8.0), y(4.0), x(16.0), y(4.0), &b, w, &st);
                line(dc, x(16.0), y(4.0), x(16.0), y(6.0), &b, w, &st);
                line(dc, x(19.0), y(6.0), x(18.0), y(20.0), &b, w, &st);
                line(dc, x(18.0), y(20.0), x(6.0), y(20.0), &b, w, &st);
                line(dc, x(6.0), y(20.0), x(5.0), y(6.0), &b, w, &st);
            }
            Glyph::Play => {
                line(dc, x(6.0), y(4.0), x(19.0), y(12.0), &b, w, &st);
                line(dc, x(19.0), y(12.0), x(6.0), y(20.0), &b, w, &st);
                line(dc, x(6.0), y(20.0), x(6.0), y(4.0), &b, w, &st);
            }
            Glyph::Globe => {
                dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: 9.0 * u, radiusY: 9.0 * u }, &b, w, Some(&st));
                dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: 4.0 * u, radiusY: 9.0 * u }, &b, w, Some(&st));
                line(dc, x(3.0), cy, x(21.0), cy, &b, w, &st);
            }
            Glyph::Monitor => {
                dc.DrawRoundedRectangle(&rrect(Rect::new(x(3.0), y(5.0), 18.0 * u, 12.0 * u), 2.0 * u), &b, w, Some(&st));
                line(dc, x(8.0), y(21.0), x(16.0), y(21.0), &b, w, &st);
                line(dc, cx, y(17.0), cx, y(21.0), &b, w, &st);
            }
            Glyph::Keyboard => {
                dc.DrawRoundedRectangle(&rrect(Rect::new(x(2.0), y(6.0), 20.0 * u, 12.0 * u), 2.0 * u), &b, w, Some(&st));
                for i in 0..4 {
                    let kx = x(6.0 + i as f32 * 4.0);
                    line(dc, kx, y(10.0), kx + 0.01, y(10.0), &b, w * 1.5, &st);
                }
                line(dc, x(8.0), y(14.0), x(16.0), y(14.0), &b, w, &st);
            }
            Glyph::Theme => {
                dc.DrawEllipse(&D2D1_ELLIPSE { point: pt(cx, cy), radiusX: 9.0 * u, radiusY: 9.0 * u }, &b, w, Some(&st));
                let geo = gfx.d2d_factory()?.CreatePathGeometry()?;
                let sink = geo.Open()?;
                sink.BeginFigure(pt(cx, y(3.0)), D2D1_FIGURE_BEGIN_FILLED);
                sink.AddArc(&D2D1_ARC_SEGMENT { point: pt(cx, y(21.0)), size: D2D_SIZE_F { width: 9.0 * u, height: 9.0 * u }, rotationAngle: 0.0, sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE, arcSize: D2D1_ARC_SIZE_SMALL });
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                sink.Close()?;
                dc.FillGeometry(&geo, &b, None);
            }
            Glyph::Bolt => {
                line(dc, x(13.0), y(2.0), x(4.0), y(14.0), &b, w, &st);
                line(dc, x(4.0), y(14.0), x(11.0), y(14.0), &b, w, &st);
                line(dc, x(11.0), y(14.0), x(10.0), y(22.0), &b, w, &st);
                line(dc, x(10.0), y(22.0), x(19.0), y(10.0), &b, w, &st);
                line(dc, x(19.0), y(10.0), x(12.0), y(10.0), &b, w, &st);
                line(dc, x(12.0), y(10.0), x(13.0), y(2.0), &b, w, &st);
            }
            Glyph::Refresh => {
                let geo = gfx.d2d_factory()?.CreatePathGeometry()?;
                let sink = geo.Open()?;
                sink.BeginFigure(pt(x(18.0), y(5.3)), D2D1_FIGURE_BEGIN_HOLLOW);
                sink.AddArc(&D2D1_ARC_SEGMENT { point: pt(x(5.0), y(10.0)), size: D2D_SIZE_F { width: 9.0 * u, height: 9.0 * u }, rotationAngle: 0.0, sweepDirection: D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE, arcSize: D2D1_ARC_SIZE_LARGE });
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
                sink.Close()?;
                dc.DrawGeometry(&geo, &b, w, Some(&st));
                line(dc, x(21.0), y(3.0), x(21.0), y(9.0), &b, w, &st);
                line(dc, x(21.0), y(9.0), x(15.0), y(9.0), &b, w, &st);
            }
            Glyph::Download => {
                line(dc, cx, y(3.0), cx, y(15.0), &b, w, &st);
                line(dc, x(7.0), y(10.0), cx, y(15.0), &b, w, &st);
                line(dc, cx, y(15.0), x(17.0), y(10.0), &b, w, &st);
                line(dc, x(4.0), y(21.0), x(20.0), y(21.0), &b, w, &st);
            }
        }
    }
    Ok(())
}

/// Transformación de dibujo local: aplica `translation(x,y)` sobre la actual y la restaura al salir.
pub fn with_translation(dc: &ID2D1DeviceContext, x: f32, y: f32, f: impl FnOnce(&ID2D1DeviceContext) -> Result<()>) -> Result<()> {
    unsafe {
        let mut m = windows_numerics::Matrix3x2::default();
        dc.GetTransform(&mut m);
        let t = translation(x, y);
        let combined = windows_numerics::Matrix3x2 {
            M11: t.M11 * m.M11 + t.M12 * m.M21,
            M12: t.M11 * m.M12 + t.M12 * m.M22,
            M21: t.M21 * m.M11 + t.M22 * m.M21,
            M22: t.M21 * m.M12 + t.M22 * m.M22,
            M31: t.M31 * m.M11 + t.M32 * m.M21 + m.M31,
            M32: t.M31 * m.M12 + t.M32 * m.M22 + m.M32,
        };
        dc.SetTransform(&combined);
        let r = f(dc);
        dc.SetTransform(&m);
        r
    }
}

pub fn push_clip(dc: &ID2D1DeviceContext, r: Rect) {
    unsafe { dc.PushAxisAlignedClip(&d2d_rect(r), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
}

pub fn pop_clip(dc: &ID2D1DeviceContext) {
    unsafe { dc.PopAxisAlignedClip() };
}
