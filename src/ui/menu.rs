//! Menú contextual de cristal: ventana popup propia con navegación por teclado.

use crate::gfx::anim::{AnimSpec, Ease, apply_offset_y, apply_opacity};
use crate::gfx::backdrop::{self, PlateSpec};
use crate::gfx::draw::{self, Glyph, Surface, TextStyle, create_surface};
use crate::gfx::Gfx;
use crate::theme::Palette;
use crate::ui::layout::Rect;
use crate::ui::window::{self, Handler, PopupStyle, Window, WindowCtx, WindowHandler};
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

pub const ITEM_H: f32 = 32.0;
pub const SEP_H: f32 = 9.0;
pub const PAD: f32 = 4.0;
pub const MIN_W: f32 = 210.0;
pub const MARGIN: f32 = 16.0;
pub const RADIUS: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub id: u32,
    pub text: String,
    pub glyph: Option<Glyph>,
    pub enabled: bool,
    pub danger: bool,
    pub separator_before: bool,
}

impl MenuItem {
    pub fn new(id: u32, text: &str, glyph: Option<Glyph>) -> MenuItem {
        MenuItem { id, text: text.to_string(), glyph, enabled: true, danger: false, separator_before: false }
    }
    pub fn disabled(mut self, off: bool) -> MenuItem {
        self.enabled = !off;
        self
    }
    pub fn danger(mut self) -> MenuItem {
        self.danger = true;
        self
    }
    pub fn separated(mut self) -> MenuItem {
        self.separator_before = true;
        self
    }
}

pub type PickFn = Box<dyn FnMut(Option<u32>)>;

pub struct Menu {
    gfx: Rc<Gfx>,
    pal: Palette,
    items: Vec<MenuItem>,
    rects: Vec<Rect>,
    hover: Option<usize>,
    win: Option<Window>,
    _plate: Option<Surface>,
    items_surface: Option<Surface>,
    scale: f32,
    w: f32,
    h: f32,
    on_pick: Option<PickFn>,
    done: bool,
}

impl Menu {
    /// `x`, `y`: esquina superior izquierda deseada, en píxeles físicos de pantalla.
    pub fn open(gfx: &Rc<Gfx>, pal: &Palette, items: Vec<MenuItem>, x: i32, y: i32, animate: bool, on_pick: PickFn) -> Result<()> {
        let mon = window::monitor_at(x, y);
        let scale = mon.scale;
        let style = TextStyle::normal(13.0);
        let mut text_w: f32 = 0.0;
        for it in &items {
            text_w = text_w.max(draw::text_width(gfx, &it.text, &style));
        }
        let w = (text_w + 34.0 + 20.0).max(MIN_W).ceil();
        let mut rects = Vec::with_capacity(items.len());
        let mut y_cursor = PAD;
        for it in &items {
            if it.separator_before {
                y_cursor += SEP_H;
            }
            rects.push(Rect::new(PAD, y_cursor, w - 2.0 * PAD, ITEM_H));
            y_cursor += ITEM_H;
        }
        let h = y_cursor + PAD;
        let phys = |v: f32| (v * scale).round() as i32;
        let (win_w, win_h) = (phys(w + 2.0 * MARGIN), phys(h + 2.0 * MARGIN));

        // Posición: dentro del área de trabajo; si no cabe por abajo, hacia arriba.
        let mut px = x - phys(MARGIN);
        let mut py = y - phys(MARGIN);
        if py + win_h > mon.work_phys.bottom {
            py = y - phys(h) - phys(MARGIN);
        }
        if px + win_w > mon.work_phys.right {
            px = mon.work_phys.right - win_w;
        }
        if px < mon.work_phys.left - phys(MARGIN) {
            px = mon.work_phys.left - phys(MARGIN);
        }

        let rc = Rc::new(RefCell::new(Menu { gfx: gfx.clone(), pal: *pal, items, rects, hover: None, win: None, _plate: None, items_surface: None, scale, w, h, on_pick: Some(on_pick), done: false }));
        let handler: Handler = rc.clone();
        let win = Window::create(gfx, "Glassbar.Menu", PopupStyle::MENU, win_w, win_h, handler)?;
        win.move_to(px, py, win_w, win_h);

        let plate_rect = Rect::new(MARGIN, MARGIN, w, h);
        let cap = backdrop::capture_screen_div(px + phys(MARGIN), py + phys(MARGIN), phys(w) as u32, phys(h) as u32, 2);
        let plate = create_surface(gfx, win_w as u32, win_h as u32)?;
        let spec = PlateSpec { plate: plate_rect, radius: RADIUS, shadow: true, tint: pal.menu_bg, border: pal.plate_border };
        backdrop::render_plate(gfx, &plate, scale, &spec, cap.as_ref(), pal)?;
        let items_surface = create_surface(gfx, win_w as u32, win_h as u32)?;

        let dcomp = gfx.dcomp()?;
        let root = win.root()?;
        let group = unsafe { dcomp.CreateVisual()? };
        let pv = unsafe { dcomp.CreateVisual()? };
        let iv = unsafe { dcomp.CreateVisual()? };
        unsafe {
            pv.SetContent(&plate.surface)?;
            iv.SetContent(&items_surface.surface)?;
            group.AddVisual(&pv, true, None)?;
            group.AddVisual(&iv, true, &pv)?;
            root.AddVisual(&group, true, None)?;
        }
        apply_opacity(dcomp, &group, &AnimSpec::new(0.0, 1.0, 120, 0, Ease::OutCubic), animate)?;
        apply_offset_y(dcomp, &group, &AnimSpec::new(-4.0 * scale, 0.0, 120, 0, Ease::OutCubic), animate)?;
        {
            let mut m = rc.borrow_mut();
            m.win = Some(win);
            m._plate = Some(plate);
            m.items_surface = Some(items_surface);
            m.paint_items()?;
        }
        if let Some(w) = &rc.borrow().win {
            w.show(true);
            w.commit()?;
        }
        Ok(())
    }

    fn paint_items(&mut self) -> Result<()> {
        let Some(s) = &self.items_surface else { return Ok(()) };
        let gfx = self.gfx.clone();
        let pal = self.pal;
        let items = &self.items;
        let rects = &self.rects;
        let hover = self.hover;
        let w = self.w;
        draw::draw(s, self.scale, |dc| {
            for (i, (it, r)) in items.iter().zip(rects.iter()).enumerate() {
                let r = Rect::new(r.x + MARGIN, r.y + MARGIN, r.w, r.h);
                if it.separator_before {
                    draw::fill_rect(dc, Rect::new(MARGIN + 8.0, r.y - SEP_H / 2.0, w - 16.0, 1.0), pal.separator)?;
                }
                let alpha = if it.enabled { 1.0 } else { 0.4 };
                if hover == Some(i) && it.enabled {
                    draw::fill_round_rect(dc, r, 5.0, pal.cell_hover)?;
                }
                let color = if it.danger { pal.error } else { pal.text }.with_alpha(alpha);
                if let Some(g) = it.glyph {
                    draw::glyph(&gfx, dc, g, Rect::new(r.x + 10.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), color)?;
                }
                let l = draw::text_layout(&gfx, &it.text, &TextStyle::normal(13.0), r.w - 44.0, r.h)?;
                draw::draw_text(dc, &l, r.x + 34.0, r.y, color)?;
            }
            Ok(())
        })?;
        if let Some(win) = &self.win {
            win.commit()?;
        }
        Ok(())
    }

    fn item_at(&self, lx: f32, ly: f32) -> Option<usize> {
        self.rects.iter().position(|r| Rect::new(r.x + MARGIN, r.y + MARGIN, r.w, r.h).contains(lx, ly))
    }

    fn set_hover(&mut self, h: Option<usize>) {
        if h != self.hover {
            self.hover = h;
            let _ = self.paint_items();
        }
    }

    fn finish(&mut self, pick: Option<u32>) {
        if self.done {
            return;
        }
        self.done = true;
        if let Some(mut f) = self.on_pick.take() {
            f(pick);
        }
        if let Some(w) = self.win.take() {
            w.destroy();
        }
    }

    fn move_hover(&mut self, delta: i32) {
        let n = self.items.len() as i32;
        if n == 0 {
            return;
        }
        let mut i = self.hover.map(|h| h as i32).unwrap_or(if delta > 0 { -1 } else { n });
        for _ in 0..n {
            i = (i + delta).rem_euclid(n);
            if self.items[i as usize].enabled {
                self.set_hover(Some(i as usize));
                return;
            }
        }
    }
}

impl WindowHandler for Menu {
    fn on_message(&mut self, ctx: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.win.is_none() {
            return None;
        }
        match msg {
            WM_MOUSEMOVE => {
                let (x, y) = window::lparam_xy(lparam);
                let h = self.item_at(x as f32 / self.scale, y as f32 / self.scale);
                self.set_hover(h);
                window::invalidate_tracking(ctx.hwnd);
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.set_hover(None);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                let (x, y) = window::lparam_xy(lparam);
                if let Some(i) = self.item_at(x as f32 / self.scale, y as f32 / self.scale) {
                    if self.items[i].enabled {
                        let id = self.items[i].id;
                        self.finish(Some(id));
                    }
                }
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let vk = wparam.0 as u16;
                if vk == VK_ESCAPE.0 {
                    self.finish(None);
                } else if vk == VK_DOWN.0 {
                    self.move_hover(1);
                } else if vk == VK_UP.0 {
                    self.move_hover(-1);
                } else if vk == VK_RETURN.0 || vk == VK_SPACE.0 {
                    if let Some(h) = self.hover {
                        if self.items[h].enabled {
                            let id = self.items[h].id;
                            self.finish(Some(id));
                        }
                    }
                }
                Some(LRESULT(0))
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    self.finish(None);
                }
                Some(LRESULT(0))
            }
            WM_NCHITTEST => {
                let (sx, sy) = window::lparam_xy(lparam);
                let (cx, cy) = window::screen_to_client(ctx.hwnd, sx, sy);
                let inside = Rect::new(MARGIN, MARGIN, self.w, self.h).contains(cx as f32 / self.scale, cy as f32 / self.scale);
                Some(LRESULT(if inside { HTCLIENT as isize } else { HTTRANSPARENT as isize }))
            }
            WM_SETCURSOR => {
                window::set_arrow_cursor();
                Some(LRESULT(1))
            }
            WM_DESTROY => {
                self.win = None;
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}
