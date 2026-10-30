//! Popover de cristal anclado a una celda: un texto y botones. Mismo plato que el menú.

use crate::gfx::anim::{AnimSpec, Ease, apply_offset_y, apply_opacity};
use crate::gfx::backdrop::{self, PlateSpec};
use crate::gfx::draw::{self, Surface, TextStyle, create_surface};
use crate::gfx::Gfx;
use crate::theme::Palette;
use crate::ui::layout::Rect;
use crate::ui::widgets::{self, Event, Form, Kind, WidgetId};
use crate::ui::window::{self, Handler, Monitor, PopupStyle, Window, WindowCtx, WindowHandler};
use std::cell::RefCell;
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

pub const MARGIN: f32 = 16.0;
pub const RADIUS: f32 = 10.0;
pub const GAP: f32 = 10.0;
pub const MIN_W: f32 = 280.0;

pub struct PopoverButton {
    pub label: String,
    pub accent: bool,
}

pub type PickFn = Box<dyn FnMut(Option<usize>)>;

pub struct Popover {
    gfx: Rc<Gfx>,
    pal: Palette,
    form: Form,
    buttons: Vec<WidgetId>,
    win: Option<Window>,
    _plate: Option<Surface>,
    content: Option<Surface>,
    scale: f32,
    w: f32,
    h: f32,
    on_pick: Option<PickFn>,
    done: bool,
}

impl Popover {
    /// `anchor_cx`, `anchor_top`: centro horizontal y borde superior de la celda, en píxeles físicos.
    /// Se abre encima de la celda; si no cabe, debajo. `on_pick` recibe el índice del botón o `None`.
    pub fn open(gfx: &Rc<Gfx>, pal: &Palette, text: &str, buttons: Vec<PopoverButton>, anchor_cx: i32, anchor_top: i32, anchor_bottom: i32, mon: &Monitor, animate: bool, on_pick: PickFn) -> Result<()> {
        let scale = mon.scale;
        let text_w = draw::text_width(gfx, text, &TextStyle::normal(13.0));
        let w = (text_w + 2.0 * widgets::MARGIN + 8.0).max(MIN_W).min(420.0).ceil();

        let mut form = Form::new(w);
        form.add(Kind::Spacer { h: 4.0 });
        form.add(Kind::Label { text: text.to_string(), dim: false, size: 13.0, color: None });
        let ids: Vec<WidgetId> = buttons.iter().map(|b| form.add_child(Kind::Button { text: b.label.clone(), accent: b.accent, glyph: None, width: (draw::text_width(gfx, &b.label, &TextStyle::normal(12.5)) + 32.0).max(90.0).ceil() })).collect();
        form.add(Kind::Row { children: ids.clone(), align_right: true });
        form.layout();
        form.focus = ids.last().copied();
        let h = form.content_h;

        let phys = |v: f32| (v * scale).round() as i32;
        let (win_w, win_h) = (phys(w + 2.0 * MARGIN), phys(h + 2.0 * MARGIN));
        let mut x = anchor_cx - win_w / 2;
        x = x.clamp(mon.work_phys.left - phys(MARGIN), (mon.work_phys.right - win_w + phys(MARGIN)).max(mon.work_phys.left - phys(MARGIN)));
        let above = anchor_top - phys(GAP) - phys(h) - phys(MARGIN);
        let y = if above >= mon.work_phys.top - phys(MARGIN) { above } else { anchor_bottom + phys(GAP) - phys(MARGIN) };

        let rc = Rc::new(RefCell::new(Popover { gfx: gfx.clone(), pal: *pal, form, buttons: ids, win: None, _plate: None, content: None, scale, w, h, on_pick: Some(on_pick), done: false }));
        let handler: Handler = rc.clone();
        let win = Window::create(gfx, "Glassbar.Popover", PopupStyle::MENU, win_w, win_h, handler)?;
        win.move_to(x, y, win_w, win_h);

        let cap = backdrop::capture_screen_div(x + phys(MARGIN), y + phys(MARGIN), phys(w) as u32, phys(h) as u32, 2);
        let plate = create_surface(gfx, win_w as u32, win_h as u32)?;
        let spec = PlateSpec { plate: Rect::new(MARGIN, MARGIN, w, h), radius: RADIUS, shadow: true, tint: pal.menu_bg, border: pal.plate_border };
        backdrop::render_plate(gfx, &plate, scale, &spec, cap.as_ref(), pal)?;
        let content = create_surface(gfx, win_w as u32, win_h as u32)?;

        let dcomp = gfx.dcomp()?;
        let root = win.root()?;
        let group = unsafe { dcomp.CreateVisual()? };
        let pv = unsafe { dcomp.CreateVisual()? };
        let cv = unsafe { dcomp.CreateVisual()? };
        unsafe {
            pv.SetContent(&plate.surface)?;
            cv.SetContent(&content.surface)?;
            group.AddVisual(&pv, true, None)?;
            group.AddVisual(&cv, true, &pv)?;
            root.AddVisual(&group, true, None)?;
        }
        apply_opacity(dcomp, &group, &AnimSpec::new(0.0, 1.0, 120, 0, Ease::OutCubic), animate)?;
        apply_offset_y(dcomp, &group, &AnimSpec::new(4.0 * scale, 0.0, 120, 0, Ease::OutCubic), animate)?;
        {
            let mut p = rc.borrow_mut();
            p.win = Some(win);
            p._plate = Some(plate);
            p.content = Some(content);
            p.repaint()?;
        }
        if let Some(w) = &rc.borrow().win {
            w.show(true);
            w.commit()?;
        }
        Ok(())
    }

    fn repaint(&mut self) -> Result<()> {
        let Some(s) = &self.content else { return Ok(()) };
        let gfx = self.gfx.clone();
        let pal = self.pal;
        let form = &self.form;
        draw::draw(s, self.scale, |dc| draw::with_translation(dc, MARGIN, MARGIN, |dc| form.draw(&gfx, dc, &pal)))?;
        if let Some(w) = &self.win {
            w.commit()?;
        }
        Ok(())
    }

    fn local(&self, lparam: LPARAM) -> (f32, f32) {
        let (x, y) = window::lparam_xy(lparam);
        (x as f32 / self.scale - MARGIN, y as f32 / self.scale - MARGIN)
    }

    fn finish(&mut self, pick: Option<usize>) {
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

    fn handle(&mut self, ev: Option<Event>) {
        match ev {
            Some(Event::Clicked(id)) => {
                let idx = self.buttons.iter().position(|b| *b == id);
                self.finish(idx);
            }
            Some(Event::Submit) => {
                let idx = self.buttons.len().checked_sub(1);
                self.finish(idx);
            }
            Some(Event::Cancel) | Some(Event::Close) => self.finish(None),
            Some(_) => {
                let _ = self.repaint();
            }
            None => {}
        }
    }
}

impl WindowHandler for Popover {
    fn on_message(&mut self, ctx: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.win.is_none() {
            return None;
        }
        match msg {
            WM_MOUSEMOVE => {
                let (x, y) = self.local(lparam);
                if self.form.on_mouse_move(x, y) {
                    let _ = self.repaint();
                }
                window::invalidate_tracking(ctx.hwnd);
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                if self.form.on_mouse_move(-1.0, -1.0) {
                    let _ = self.repaint();
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let (x, y) = self.local(lparam);
                window::capture_mouse(ctx.hwnd);
                let ev = self.form.on_mouse_down(x, y);
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                window::release_mouse();
                let (x, y) = self.local(lparam);
                let ev = self.form.on_mouse_up(x, y);
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let shift = window::key_down(VK_SHIFT.0);
                let ctrl = window::key_down(VK_CONTROL.0);
                let ev = self.form.on_key(wparam.0 as u16, shift, ctrl);
                self.handle(ev);
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
