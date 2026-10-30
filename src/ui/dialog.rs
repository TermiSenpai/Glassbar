//! Ventana secundaria de cristal: Mica del sistema o fondo sólido, con un `Form` dentro.

use crate::gfx::draw::{self, Surface, create_surface};
use crate::gfx::Gfx;
use crate::theme::Palette;
use crate::ui::layout::Rect;
use crate::ui::widgets::{Event, Form, Kind};
use crate::ui::window::{self, Handler, PopupStyle, Window, WindowCtx, WindowHandler};
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::UI::Controls::{MARGINS, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, Result};

pub enum DialogAction {
    None,
    Repaint,
    Relayout,
    Close,
}

pub type EventFn = Box<dyn FnMut(&mut Form, Event) -> DialogAction>;

pub struct Dialog {
    gfx: Rc<Gfx>,
    pub pal: Palette,
    pub form: Form,
    win: Option<Window>,
    surface: Option<Surface>,
    scale: f32,
    mica: bool,
    on_event: EventFn,
    closed: bool,
    caret_timer: bool,
    view_h: f32,
    scroll: f32,
    content_visual: Option<windows::Win32::Graphics::DirectComposition::IDCompositionVisual2>,
}

const DWMSBT_MAINWINDOW: u32 = 2;
const DWMWCP_ROUND: u32 = 2;

impl Dialog {
    /// Crea, coloca centrada en el monitor del cursor y muestra la ventana.
    pub fn open(gfx: &Rc<Gfx>, pal: &Palette, title: &str, width: f32, form: Form, on_event: EventFn) -> Result<Rc<RefCell<Dialog>>> {
        let rc = Rc::new(RefCell::new(Dialog { gfx: gfx.clone(), pal: *pal, form, win: None, surface: None, scale: 1.0, mica: false, on_event, closed: false, caret_timer: false, view_h: 0.0, scroll: 0.0, content_visual: None }));
        let handler: Handler = rc.clone();
        let mon = window::cursor_monitor();
        let scale = mon.scale;
        let form_h;
        {
            let mut d = rc.borrow_mut();
            d.form.width = width;
            d.form.layout();
            form_h = d.form.content_h;
            d.scale = scale;
        }
        let phys = |v: f32| (v * scale).round() as i32;
        let max_h = ((mon.work_phys.bottom - mon.work_phys.top) as f32 / scale - 80.0).max(200.0);
        let view_h = form_h.min(max_h);
        let (w, h) = (phys(width), phys(view_h));
        {
            let mut d = rc.borrow_mut();
            d.view_h = view_h;
        }
        let win = Window::create(gfx, "Glassbar.Dialog", PopupStyle::DIALOG, w, h, handler)?;
        win.set_title(title);
        let x = mon.work_phys.left + ((mon.work_phys.right - mon.work_phys.left) - w) / 2;
        let y = mon.work_phys.top + ((mon.work_phys.bottom - mon.work_phys.top) - h) / 2;
        win.move_to(x, y, w, h);
        let mica = apply_backdrop(win.hwnd, pal.dark);
        crate::app::window_opened();
        {
            let mut d = rc.borrow_mut();
            d.mica = mica;
            d.win = Some(win);
            d.rebuild_surface()?;
            d.repaint()?;
        }
        if let Some(w) = &rc.borrow().win {
            w.show(true);
            let _ = w.commit();
        }
        Ok(rc)
    }

    pub fn hwnd(&self) -> HWND {
        self.win.as_ref().map(|w| w.hwnd).unwrap_or_default()
    }

    pub fn is_open(&self) -> bool {
        !self.closed && self.win.is_some()
    }

    pub fn activate(&self) {
        if let Some(w) = &self.win {
            w.show(true);
        }
    }

    pub fn set_palette(&mut self, pal: Palette) {
        self.pal = pal;
        if let Some(w) = &self.win {
            self.mica = apply_backdrop(w.hwnd, pal.dark);
        }
        let _ = self.repaint();
    }

    fn phys(&self, v: f32) -> f32 {
        (v * self.scale).round()
    }

    fn rebuild_surface(&mut self) -> Result<()> {
        let Some(win) = &self.win else { return Ok(()) };
        let (w, h) = (self.phys(self.form.width) as u32, self.phys(self.form.content_h) as u32);
        let s = create_surface(&self.gfx, w, h)?;
        let root = win.root()?.clone();
        unsafe {
            root.RemoveAllVisuals()?;
            let v = self.gfx.dcomp()?.CreateVisual()?;
            v.SetContent(&s.surface)?;
            v.SetOffsetY2(-self.phys(self.scroll))?;
            root.AddVisual(&v, true, None)?;
            self.content_visual = Some(v);
        }
        self.surface = Some(s);
        Ok(())
    }

    /// Vuelve a calcular la disposición y, si cambia el alto, redimensiona la ventana.
    pub fn relayout(&mut self) -> Result<()> {
        let old_h = self.form.content_h;
        self.form.layout();
        if (self.form.content_h - old_h).abs() > 0.5 {
            let max_h = self.view_h.max(200.0);
            let view = self.form.content_h.min(max_h);
            if let Some(win) = &self.win {
                let r = win.rect();
                let (w, h) = (self.phys(self.form.width) as i32, self.phys(view) as i32);
                win.move_to(r.left, r.top, w, h);
            }
            self.scroll = self.scroll.min((self.form.content_h - view).max(0.0));
            self.rebuild_surface()?;
        }
        self.repaint()
    }

    pub fn repaint(&mut self) -> Result<()> {
        let Some(s) = &self.surface else { return Ok(()) };
        let gfx = self.gfx.clone();
        let pal = self.pal;
        let mica = self.mica;
        let (w, h) = (self.form.width, self.form.content_h);
        let form = &self.form;
        draw::draw(s, self.scale, |dc| {
            if !mica {
                draw::fill_round_rect(dc, Rect::new(0.0, 0.0, w, h), 10.0, pal.window_solid)?;
                draw::stroke_round_rect(dc, Rect::new(0.0, 0.0, w, h), 10.0, pal.plate_border, 1.0)?;
            }
            form.draw(&gfx, dc, &pal)
        })?;
        if let Some(win) = &self.win {
            win.commit()?;
        }
        Ok(())
    }

    /// Cierra la ventana. Se llama desde dentro del propio manejador de mensajes, así que
    /// `WM_DESTROY` no llega al manejador (está prestado): el contador se descuenta aquí.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        if let Some(win) = self.win.take() {
            win.destroy();
        }
        crate::app::window_closed();
    }

    fn handle(&mut self, ev: Option<Event>) {
        let Some(ev) = ev else { return };
        let action = match ev {
            Event::Repaint => DialogAction::Repaint,
            other => (self.on_event)(&mut self.form, other),
        };
        match action {
            DialogAction::None => {}
            DialogAction::Repaint => {
                let _ = self.repaint();
            }
            DialogAction::Relayout => {
                let _ = self.relayout();
            }
            DialogAction::Close => self.close(),
        }
    }

    fn update_caret_timer(&mut self) {
        let want = self.form.has_text_focus();
        if want != self.caret_timer {
            if let Some(w) = &self.win {
                if want {
                    let ms = unsafe { GetCaretBlinkTime() };
                    w.set_timer(window::TIMER_CARET, if ms == 0 || ms == u32::MAX { 530 } else { ms });
                } else {
                    w.kill_timer(window::TIMER_CARET);
                }
            }
            self.caret_timer = want;
        }
    }

    fn header_hit(&self, lx: f32, ly: f32) -> bool {
        let header = self.form.widgets.iter().find(|w| matches!(w.kind, Kind::Header { .. })).map(|w| w.rect);
        match header {
            Some(r) if r.contains(lx, ly) => {
                let close = Rect::new(r.right() - 28.0, r.y + (r.h - 28.0) / 2.0, 28.0, 28.0);
                !close.contains(lx, ly)
            }
            _ => false,
        }
    }
}

/// Mica del sistema con esquinas redondeadas. `false` si no está disponible (fondo sólido propio).
fn apply_backdrop(hwnd: HWND, dark: bool) -> bool {
    unsafe {
        let m = MARGINS { cxLeftWidth: -1, cxRightWidth: -1, cyTopHeight: -1, cyBottomHeight: -1 };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
        let d = BOOL(if dark { 1 } else { 0 });
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &d as *const BOOL as *const c_void, size_of::<BOOL>() as u32);
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const u32 as *const c_void, 4);
        let backdrop = DWMSBT_MAINWINDOW;
        DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop as *const u32 as *const c_void, 4).is_ok()
    }
}

impl WindowHandler for Dialog {
    fn on_message(&mut self, ctx: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if self.win.is_none() {
            return None;
        }
        match msg {
            WM_MOUSEWHEEL => {
                let delta = window::wheel_delta(wparam) as f32 / 120.0;
                let view = self.view_h.max(200.0).min(self.form.content_h);
                let max = (self.form.content_h - view).max(0.0);
                let target = (self.scroll - delta * 60.0).clamp(0.0, max);
                if (target - self.scroll).abs() > 0.1 {
                    self.scroll = target;
                    if let (Some(v), Ok(d)) = (&self.content_visual, self.gfx.dcomp()) {
                        let _ = crate::gfx::anim::apply_offset_y(d, v, &crate::gfx::anim::AnimSpec::new(0.0, -self.phys(self.scroll), 120, 0, crate::gfx::anim::Ease::OutCubic), false);
                    }
                    if let Some(w) = &self.win {
                        let _ = w.commit();
                    }
                }
                Some(LRESULT(0))
            }
            WM_MOUSEMOVE => {
                let (x, y) = window::lparam_xy(lparam);
                let y = y as f32 / self.scale + self.scroll;
                if self.form.on_mouse_move(x as f32 / self.scale, y) {
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
                let (x, y) = window::lparam_xy(lparam);
                let y = y as f32 / self.scale + self.scroll;
                window::capture_mouse(ctx.hwnd);
                let ev = self.form.on_mouse_down(x as f32 / self.scale, y);
                self.update_caret_timer();
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                window::release_mouse();
                let (x, y) = window::lparam_xy(lparam);
                let y = y as f32 / self.scale + self.scroll;
                let ev = self.form.on_mouse_up(x as f32 / self.scale, y);
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                let shift = window::key_down(VK_SHIFT.0);
                let ctrl = window::key_down(VK_CONTROL.0);
                let ev = self.form.on_key(wparam.0 as u16, shift, ctrl);
                self.update_caret_timer();
                if ev.is_none() {
                    return None;
                }
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_CHAR => {
                let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
                if ch.is_control() {
                    return Some(LRESULT(0));
                }
                let ev = self.form.on_char(ch);
                self.handle(ev);
                Some(LRESULT(0))
            }
            WM_SETCURSOR => {
                let (sx, sy) = window::cursor_pos();
                let (cx, cy) = window::screen_to_client(ctx.hwnd, sx, sy);
                let (lx, ly) = (cx as f32 / self.scale, cy as f32 / self.scale + self.scroll);
                let kind = self.form.hit(lx, ly).and_then(|id| self.form.get(id)).map(|w| (w.kind.clone(), w.enabled));
                match kind {
                    Some((Kind::TextField(_), true)) => window::set_ibeam_cursor(),
                    Some((Kind::Link { .. }, true)) => window::set_hand_cursor(),
                    _ => window::set_arrow_cursor(),
                }
                Some(LRESULT(1))
            }
            WM_NCHITTEST => {
                let (sx, sy) = window::lparam_xy(lparam);
                let (cx, cy) = window::screen_to_client(ctx.hwnd, sx, sy);
                if self.header_hit(cx as f32 / self.scale, cy as f32 / self.scale + self.scroll) { Some(LRESULT(HTCAPTION as isize)) } else { Some(LRESULT(HTCLIENT as isize)) }
            }
            WM_TIMER if wparam.0 == window::TIMER_CARET => {
                self.form.caret_visible = !self.form.caret_visible;
                let _ = self.repaint();
                Some(LRESULT(0))
            }
            WM_KILLFOCUS => {
                self.form.caret_visible = false;
                let _ = self.repaint();
                Some(LRESULT(0))
            }
            WM_SETFOCUS => {
                self.form.caret_visible = true;
                let _ = self.repaint();
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.handle(Some(Event::Cancel));
                Some(LRESULT(0))
            }
            window::WM_THEME_CHANGED => {
                let _ = self.repaint();
                Some(LRESULT(0))
            }
            WM_DESTROY => {
                if !self.closed {
                    self.closed = true;
                    self.win = None;
                    crate::app::window_closed();
                }
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}
