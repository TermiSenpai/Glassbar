//! Tooltip de cristal: ventana popup sin activación, encima de la celda.

use crate::gfx::anim::{AnimSpec, Ease, apply_offset_y, apply_opacity};
use crate::gfx::backdrop::{self, PlateSpec};
use crate::gfx::draw::{self, Surface, TextStyle, create_surface};
use crate::gfx::Gfx;
use crate::theme::Palette;
use crate::ui::layout::Rect;
use crate::ui::window::{self, Monitor, PopupStyle, Window, WindowCtx, WindowHandler};
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

pub const SHOW_MS: u32 = 120;
pub const GAP: f32 = 10.0;
const PAD_X: f32 = 9.0;
const PAD_Y: f32 = 5.0;
const LINE_H: f32 = 17.0;
const MARGIN: f32 = 12.0;

struct Passthrough;

impl WindowHandler for Passthrough {
    fn on_message(&mut self, _ctx: &WindowCtx, msg: u32, _w: WPARAM, _l: LPARAM) -> Option<LRESULT> {
        if msg == WM_NCHITTEST { Some(LRESULT(HTTRANSPARENT as isize)) } else { None }
    }
}

pub struct Tooltip {
    win: Window,
    _plate: Surface,
    _text: Surface,
}

impl Tooltip {
    /// `anchor_cx` y `anchor_top` en píxeles físicos de pantalla.
    pub fn show(gfx: &Rc<Gfx>, pal: &Palette, lines: &[&str], anchor_cx: i32, anchor_top: i32, mon: &Monitor, animate: bool) -> Result<Tooltip> {
        let scale = mon.scale;
        let style = TextStyle::normal(12.0);
        let mut text_w: f32 = 0.0;
        for l in lines {
            text_w = text_w.max(draw::text_width(gfx, l, &style));
        }
        let w = (text_w + PAD_X * 2.0).ceil().min(360.0);
        let h = PAD_Y * 2.0 + LINE_H * lines.len().max(1) as f32;
        let win_w = w + MARGIN * 2.0;
        let win_h = h + MARGIN * 2.0;
        let phys = |v: f32| (v * scale).round() as i32;

        let mut x = anchor_cx - phys(win_w) / 2;
        let min_x = mon.work_phys.left - phys(MARGIN);
        let max_x = mon.work_phys.right - phys(win_w) + phys(MARGIN);
        x = x.clamp(min_x, max_x.max(min_x));
        let y = anchor_top - phys(GAP) - phys(h) - phys(MARGIN);

        let handler: window::Handler = Rc::new(RefCell::new(Passthrough));
        let win = Window::create(gfx, "Glassbar.Tip", PopupStyle::TOOLTIP, phys(win_w), phys(win_h), handler)?;
        win.move_to(x, y, phys(win_w), phys(win_h));

        let plate_rect = Rect::new(MARGIN, MARGIN, w, h);
        let cap = backdrop::capture_screen_div(x + phys(MARGIN), y + phys(MARGIN), phys(w) as u32, phys(h) as u32, 2);
        let plate = create_surface(gfx, phys(win_w) as u32, phys(win_h) as u32)?;
        let spec = PlateSpec { plate: plate_rect, radius: 7.0, shadow: true, tint: pal.tooltip_bg, border: pal.plate_border };
        backdrop::render_plate(gfx, &plate, scale, &spec, cap.as_ref(), pal)?;

        let text = create_surface(gfx, phys(win_w) as u32, phys(win_h) as u32)?;
        let color = pal.text;
        let dim = pal.text_dim;
        draw::draw(&text, scale, |dc| {
            for (i, l) in lines.iter().enumerate() {
                let layout = draw::text_layout(gfx, l, &style, w - PAD_X * 2.0, LINE_H)?;
                draw::draw_text(dc, &layout, MARGIN + PAD_X, MARGIN + PAD_Y + LINE_H * i as f32, if i == 0 { color } else { dim })?;
            }
            Ok(())
        })?;

        let dcomp = gfx.dcomp()?;
        let root = win.root()?;
        let group = unsafe { dcomp.CreateVisual()? };
        let pv = unsafe { dcomp.CreateVisual()? };
        let tv = unsafe { dcomp.CreateVisual()? };
        unsafe {
            pv.SetContent(&plate.surface)?;
            tv.SetContent(&text.surface)?;
            group.AddVisual(&pv, true, None)?;
            group.AddVisual(&tv, true, &pv)?;
            root.AddVisual(&group, true, None)?;
        }
        apply_opacity(dcomp, &group, &AnimSpec::new(0.0, 1.0, SHOW_MS, 0, Ease::OutCubic), animate)?;
        apply_offset_y(dcomp, &group, &AnimSpec::new(4.0 * scale, 0.0, SHOW_MS, 0, Ease::OutCubic), animate)?;
        win.show(false);
        win.commit()?;
        Ok(Tooltip { win, _plate: plate, _text: text })
    }

    pub fn hide(self) {
        self.win.destroy();
    }
}
