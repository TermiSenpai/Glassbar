//! La barra: ventana, visuales, iconos, hover, lanzamiento, apertura y cierre animados.

use crate::gfx::anim::{AnimSpec, Ease, apply_offset_x, apply_offset_y, apply_opacity, apply_scale, set_opacity};
use crate::gfx::backdrop::{self, Capture, PlateSpec};
use crate::gfx::draw::{self, Glyph, Surface, TextStyle, create_surface};
use crate::gfx::{Gfx, pt};
use crate::icons::cache::{IconCache, Record, fingerprint_for};
use crate::icons::extract::{IconResult, Job, spawn_worker};
use crate::model::entries::{Entries, IconSource};
use crate::model::settings::Settings;
use crate::theme::{self, Palette};
use crate::ui::layout::{self, BarLayout, Hit, Metrics, Rect};
use crate::ui::window::{self, Handler, PopupStyle, Window, WindowCtx, WindowHandler};
use crate::fsx;
use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::{Rc, Weak};
use std::time::Instant;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::{D2D_RECT_F, D2D1_GRADIENT_STOP};
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::DragAcceptFiles;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::Result;

pub const OPEN_PLATE_MS: u32 = 260;
pub const OPEN_ICON_MS: u32 = 220;
pub const OPEN_STAGGER_MS: u32 = 38;
pub const OPEN_FIRST_DELAY_MS: u32 = 30;
pub const CLOSE_MS: u32 = 110;
pub const HOVER_MS: u32 = 120;
pub const PRESS_MS: u32 = 80;
pub const ICON_FADE_MS: u32 = 150;
pub const SCROLL_WHEEL_MS: u32 = 150;
pub const SCROLL_ARROW_MS: u32 = 220;
pub const SLIDE_PX: f32 = 18.0;
pub const ICON_SLIDE_PX: f32 = 8.0;
pub const CLOSE_SLIDE_PX: f32 = 10.0;

struct Visual {
    visual: IDCompositionVisual2,
    surface: Surface,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Focus {
    Cell(usize),
    Add,
    Settings,
}

pub struct Toolbar {
    gfx: Rc<Gfx>,
    entries: Rc<RefCell<Entries>>,
    settings: Rc<RefCell<Settings>>,
    cache: IconCache,
    pal: Palette,
    self_weak: Weak<RefCell<Toolbar>>,
    win: Option<Window>,
    scale: f32,
    work_w: f32,
    layout: BarLayout,
    scroll_offset: f32,
    group: Option<IDCompositionVisual2>,
    content: Option<IDCompositionVisual2>,
    plate: Option<Visual>,
    cells: Vec<Visual>,
    add: Option<Visual>,
    settings_btn: Option<Visual>,
    arrows: Option<(Visual, Visual)>,
    fades: Option<(Visual, Visual)>,
    others: Vec<Visual>,
    bitmaps: Vec<Option<ID2D1Bitmap1>>,
    missing: Vec<bool>,
    icon_px: u32,
    generation: u32,
    pending: usize,
    hover: Hit,
    pressed: Option<Hit>,
    focus: Option<Focus>,
    visible: bool,
    animate: bool,
    update_dot: bool,
    on_hidden: Box<dyn Fn()>,
    perf_start: Option<Instant>,
}

fn add_child(parent: &IDCompositionVisual2, child: &IDCompositionVisual2, last: &mut Option<IDCompositionVisual2>) -> Result<()> {
    unsafe {
        match last.as_ref() {
            Some(prev) => parent.AddVisual(child, true, prev)?,
            None => parent.AddVisual(child, true, None)?,
        }
    }
    *last = Some(child.clone());
    Ok(())
}

impl Toolbar {
    pub fn new(gfx: Rc<Gfx>, entries: Rc<RefCell<Entries>>, settings: Rc<RefCell<Settings>>, cache: IconCache, on_hidden: Box<dyn Fn()>, perf_start: Option<Instant>) -> Rc<RefCell<Toolbar>> {
        let pal = theme::palette(settings.borrow().theme);
        let rc = Rc::new(RefCell::new(Toolbar {
            gfx,
            entries,
            settings,
            cache,
            pal,
            self_weak: Weak::new(),
            win: None,
            scale: 1.0,
            work_w: 1920.0,
            layout: layout::compute(0, 1920.0, 0.0),
            scroll_offset: 0.0,
            group: None,
            content: None,
            plate: None,
            cells: Vec::new(),
            add: None,
            settings_btn: None,
            arrows: None,
            fades: None,
            others: Vec::new(),
            bitmaps: Vec::new(),
            missing: Vec::new(),
            icon_px: 34,
            generation: 0,
            pending: 0,
            hover: Hit::Outside,
            pressed: None,
            focus: None,
            visible: false,
            animate: true,
            update_dot: false,
            on_hidden,
            perf_start,
        }));
        rc.borrow_mut().self_weak = Rc::downgrade(&rc);
        rc
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_update_dot(&mut self, on: bool) {
        if self.update_dot != on {
            self.update_dot = on;
            if self.visible {
                let _ = self.paint_settings_button();
                self.commit();
            }
        }
    }

    pub fn toggle(&mut self) {
        if self.visible { self.hide() } else { self.show() }
    }

    fn phys(&self, v: f32) -> f32 {
        (v * self.scale).round()
    }

    fn commit(&self) {
        if let Some(w) = &self.win {
            let _ = w.commit();
        }
    }

    // ---- ventana ----

    fn create_window(&mut self) -> Result<()> {
        if let Some(w) = self.win.take() {
            w.destroy();
        }
        let handler: Handler = self.self_weak.upgrade().expect("toolbar alive");
        let win = Window::create(&self.gfx, "Glassbar.Bar", PopupStyle::BAR, 10, 10, handler)?;
        unsafe { DragAcceptFiles(win.hwnd, true) };
        self.win = Some(win);
        Ok(())
    }

    pub fn show(&mut self) {
        if self.visible {
            return;
        }
        if let Err(e) = self.show_inner() {
            log_error!("show failed: {e}");
            if Gfx::is_device_lost(&e) {
                log_warn!("device lost; the window will be recreated on next show");
                if let Some(w) = self.win.take() {
                    w.destroy();
                }
            }
        }
    }

    fn show_inner(&mut self) -> Result<()> {
        let mon = window::cursor_monitor();
        let scale_changed = (mon.scale - self.scale).abs() > 0.001;
        self.scale = mon.scale;
        self.work_w = mon.work.w;
        self.icon_px = (Metrics::ICON * self.scale).round() as u32;
        self.animate = theme::animations_enabled(self.settings.borrow().animations);
        self.pal = theme::palette(self.settings.borrow().theme);
        if self.win.is_none() || scale_changed {
            self.create_window()?;
        }
        self.hover = Hit::Outside;
        self.pressed = None;
        self.focus = None;
        self.relayout();
        let (lx, ly) = layout::window_position(&self.layout, mon.work);
        let (wx, wy) = (self.phys(lx) as i32, self.phys(ly) as i32);
        let (ww, wh) = (self.phys(self.layout.window.0) as i32, self.phys(self.layout.window.1) as i32);
        let win = self.win.as_ref().expect("window");
        win.move_to(wx, wy, ww, wh);
        let plate = self.layout.plate;
        let cap = if backdrop::output_is_hdr(mon.handle) {
            None
        } else {
            backdrop::capture_screen(wx + self.phys(plate.x) as i32, wy + self.phys(plate.y) as i32, self.phys(plate.w) as u32, self.phys(plate.h) as u32)
        };
        self.load_icons();
        self.build_visuals(cap.as_ref())?;
        self.animate_open()?;
        let win = self.win.as_ref().expect("window");
        win.show(true);
        win.commit()?;
        self.visible = true;
        if let Some(t) = self.perf_start.take() {
            log_info!("perf first-frame {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(())
    }

    pub fn hide(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        let Some(win) = &self.win else { return };
        if self.animate {
            if let Some(g) = &self.group {
                let d = &self.gfx.dcomp;
                let _ = apply_opacity(d, g, &AnimSpec::new(1.0, 0.0, CLOSE_MS, 0, Ease::InCubic), true);
                let _ = apply_offset_y(d, g, &AnimSpec::new(0.0, self.phys(CLOSE_SLIDE_PX), CLOSE_MS, 0, Ease::InCubic), true);
                let c = self.plate_center();
                let _ = apply_scale(d, g, &AnimSpec::new(1.0, 0.985, CLOSE_MS, 0, Ease::InCubic), c, true);
            }
            let _ = win.commit();
            win.set_timer(window::TIMER_HIDE, CLOSE_MS + 10);
        } else {
            self.finish_hide();
        }
    }

    fn finish_hide(&mut self) {
        if let Some(win) = &self.win {
            win.kill_timer(window::TIMER_HIDE);
            win.hide();
        }
        (self.on_hidden)();
    }

    fn plate_center(&self) -> (f32, f32) {
        let (cx, cy) = self.layout.plate.center();
        (self.phys(cx), self.phys(cy))
    }

    // ---- layout y datos ----

    fn relayout(&mut self) {
        let n = self.entries.borrow().entries.len();
        self.layout = layout::compute(n, self.work_w, self.scroll_offset);
        if let Some(s) = &self.layout.scroll {
            self.scroll_offset = s.offset;
        } else {
            self.scroll_offset = 0.0;
        }
        self.missing = self.entries.borrow().entries.iter().map(|e| !e.path.exists()).collect();
    }

    /// Bitmaps desde la caché; lo que falte se pide al hilo de extracción.
    fn load_icons(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        let entries = self.entries.borrow();
        let px = self.icon_px;
        let mut jobs = Vec::new();
        self.bitmaps = Vec::with_capacity(entries.entries.len());
        for (i, e) in entries.entries.iter().enumerate() {
            let source = match &e.icon {
                IconSource::Custom { file } => Some(fsx::icons_dir().join(file)),
                IconSource::File => Some(e.path.clone()),
                IconSource::None => None,
            };
            let mut bmp = None;
            if let Some(src) = source {
                let fp = fingerprint_for(&src);
                if let Some(fp) = fp {
                    if let Some(rec) = self.cache.get(&e.id, fp, px as u16) {
                        bmp = self.gfx.bitmap_from_bgra(px, &rec.bgra).ok();
                    }
                }
                if bmp.is_none() && fp.is_some() {
                    jobs.push(Job { generation: self.generation, index: i, id: e.id.clone(), source: src, kind: e.icon.clone(), px });
                }
            }
            self.bitmaps.push(bmp);
        }
        drop(entries);
        self.pending = jobs.len();
        if !jobs.is_empty() {
            if let Some(w) = &self.win {
                spawn_worker(jobs, w.hwnd.0 as isize);
            }
        }
    }

    fn on_icon_ready(&mut self, r: Box<IconResult>) {
        if r.generation != self.generation {
            return;
        }
        let id_ok = self.entries.borrow().entries.get(r.index).map(|e| e.id == r.id).unwrap_or(false);
        if id_ok {
            if let Some(bgra) = &r.bgra {
                self.cache.insert(Record { id: r.id.clone(), fingerprint: r.fingerprint, px: r.px as u16, bgra: bgra.clone() });
                if r.px == self.icon_px {
                    if let Ok(b) = self.gfx.bitmap_from_bgra(r.px, bgra) {
                        if r.index < self.bitmaps.len() {
                            self.bitmaps[r.index] = Some(b);
                        }
                        if r.index < self.cells.len() {
                            let _ = self.paint_cell(r.index);
                            let v = self.cells[r.index].visual.clone();
                            let _ = apply_opacity(&self.gfx.dcomp, &v, &AnimSpec::new(0.3, 1.0, ICON_FADE_MS, 0, Ease::OutCubic), self.animate);
                            self.commit();
                        }
                    }
                }
            }
        }
        self.pending = self.pending.saturating_sub(1);
        if self.pending == 0 {
            let entries = self.entries.borrow();
            let ids: Vec<&str> = entries.ids();
            self.cache.prune(&ids);
            if let Err(e) = self.cache.save(&fsx::cache_path()) {
                log_warn!("icons.cache save failed: {e}");
            }
        }
    }

    // ---- visuales ----

    fn make_visual(&self, parent: &IDCompositionVisual2, r: Rect, last: &mut Option<IDCompositionVisual2>, f: impl FnOnce(&ID2D1DeviceContext) -> Result<()>) -> Result<Visual> {
        let surface = create_surface(&self.gfx, self.phys(r.w) as u32, self.phys(r.h) as u32)?;
        draw::draw(&surface, self.scale, f)?;
        let visual = unsafe { self.gfx.dcomp.CreateVisual()? };
        unsafe {
            visual.SetContent(&surface.surface)?;
            visual.SetOffsetX2(self.phys(r.x))?;
            visual.SetOffsetY2(self.phys(r.y))?;
        }
        add_child(parent, &visual, last)?;
        Ok(Visual { visual, surface })
    }

    fn build_visuals(&mut self, cap: Option<&Capture>) -> Result<()> {
        let win = self.win.as_ref().expect("window");
        let dcomp = self.gfx.dcomp.clone();
        unsafe { win.root.RemoveAllVisuals()? };
        self.cells.clear();
        self.others.clear();
        self.arrows = None;
        self.fades = None;

        let group = unsafe { dcomp.CreateVisual()? };
        let mut root_last = None;
        add_child(&win.root, &group, &mut root_last)?;
        let mut last = None;

        // Plato
        let (ww, wh) = (self.phys(self.layout.window.0) as u32, self.phys(self.layout.window.1) as u32);
        let plate_surface = create_surface(&self.gfx, ww, wh)?;
        let spec = PlateSpec { plate: self.layout.plate, radius: Metrics::RADIUS, shadow: true, tint: self.pal.plate_tint, border: self.pal.plate_border };
        backdrop::render_plate(&self.gfx, &plate_surface, self.scale, &spec, cap, &self.pal)?;
        let plate_visual = unsafe { dcomp.CreateVisual()? };
        unsafe { plate_visual.SetContent(&plate_surface.surface)? };
        add_child(&group, &plate_visual, &mut last)?;
        self.plate = Some(Visual { visual: plate_visual, surface: plate_surface });

        // Celdas dentro de clip/content
        let n = self.layout.cells.len();
        let row_y = Metrics::MARGIN + Metrics::PAD_Y;
        let clip = unsafe { dcomp.CreateVisual()? };
        let content = unsafe { dcomp.CreateVisual()? };
        let origin_x = match &self.layout.scroll {
            Some(s) => s.viewport.x,
            None => self.layout.cells.first().map(|c| c.x).unwrap_or(Metrics::MARGIN + Metrics::PAD_X),
        };
        unsafe {
            clip.SetOffsetX2(self.phys(origin_x))?;
            clip.SetOffsetY2(self.phys(row_y))?;
            if let Some(s) = &self.layout.scroll {
                clip.SetClip2(&D2D_RECT_F { left: 0.0, top: -4.0 * self.scale, right: self.phys(s.viewport.w), bottom: self.phys(s.viewport.h) + 4.0 * self.scale })?;
            }
            content.SetOffsetX2(-self.phys(self.scroll_offset))?;
            clip.AddVisual(&content, true, None)?;
        }
        add_child(&group, &clip, &mut last)?;
        let mut cell_last = None;
        for i in 0..n {
            let px = self.phys(Metrics::CELL) as u32;
            let surface = create_surface(&self.gfx, px, px)?;
            let visual = unsafe { dcomp.CreateVisual()? };
            unsafe {
                visual.SetContent(&surface.surface)?;
                visual.SetOffsetX2(self.phys(i as f32 * Metrics::STEP))?;
            }
            add_child(&content, &visual, &mut cell_last)?;
            self.cells.push(Visual { visual, surface });
        }
        for i in 0..n {
            self.paint_cell(i)?;
        }

        // Bordes desvanecidos y flechas
        if let Some(s) = self.layout.scroll.clone() {
            let fade_color = self.pal.plate_tint.with_alpha(1.0);
            let lf = self.make_visual(&group, Rect::new(s.viewport.x, row_y, Metrics::FADE_W, Metrics::CELL), &mut last, |dc| gradient(dc, Metrics::FADE_W, Metrics::CELL, fade_color, true))?;
            let rf = self.make_visual(&group, Rect::new(s.viewport.right() - Metrics::FADE_W, row_y, Metrics::FADE_W, Metrics::CELL), &mut last, |dc| gradient(dc, Metrics::FADE_W, Metrics::CELL, fade_color, false))?;
            set_opacity(&lf.visual, if s.fade_left { 1.0 } else { 0.0 })?;
            set_opacity(&rf.visual, if s.fade_right { 1.0 } else { 0.0 })?;
            self.fades = Some((lf, rf));
            let gfx = self.gfx.clone();
            let glyph_c = self.pal.glyph;
            let la = self.make_visual(&group, s.left_arrow, &mut last, |dc| draw::glyph(&gfx, dc, Glyph::ChevronLeft, Rect::new(2.0, 16.0, 16.0, 16.0), glyph_c))?;
            let ra = self.make_visual(&group, s.right_arrow, &mut last, |dc| draw::glyph(&gfx, dc, Glyph::ChevronRight, Rect::new(2.0, 16.0, 16.0, 16.0), glyph_c))?;
            set_opacity(&la.visual, if s.fade_left { 1.0 } else { 0.35 })?;
            set_opacity(&ra.visual, if s.fade_right { 1.0 } else { 0.35 })?;
            self.arrows = Some((la, ra));
        }

        // Texto de barra vacía
        if let Some(r) = self.layout.empty_text {
            let gfx = self.gfx.clone();
            let c = self.pal.text_dim;
            let v = self.make_visual(&group, r, &mut last, |dc| {
                let l = draw::text_layout(&gfx, crate::i18n::t("bar.empty"), &TextStyle::normal(12.5), r.w, r.h)?;
                draw::draw_text(dc, &l, 4.0, 0.0, c)
            })?;
            self.others.push(v);
        }

        // Separador
        if let Some(sep) = self.layout.sep {
            let c = self.pal.separator;
            let v = self.make_visual(&group, sep, &mut last, |dc| draw::fill_rect(dc, Rect::new(0.0, 0.0, sep.w, sep.h), c))?;
            self.others.push(v);
        }

        // Botones fijos
        let add_r = self.layout.add;
        let add = self.make_visual(&group, add_r, &mut last, |_| Ok(()))?;
        self.add = Some(add);
        let set_r = self.layout.settings;
        let set = self.make_visual(&group, set_r, &mut last, |_| Ok(()))?;
        self.settings_btn = Some(set);
        self.paint_add_button()?;
        self.paint_settings_button()?;

        self.group = Some(group);
        self.content = Some(content);
        Ok(())
    }

    fn paint_cell(&self, i: usize) -> Result<()> {
        let Some(cv) = self.cells.get(i) else { return Ok(()) };
        let pal = self.pal;
        let hovered = self.hover == Hit::Cell(i);
        let pressed = self.pressed == Some(Hit::Cell(i));
        let focused = self.focus == Some(Focus::Cell(i));
        let missing = self.missing.get(i).copied().unwrap_or(false);
        let bmp = self.bitmaps.get(i).cloned().flatten();
        let gfx = self.gfx.clone();
        let icon_px = self.icon_px;
        draw::draw(&cv.surface, self.scale, |dc| {
            let r = Rect::new(0.0, 0.0, Metrics::CELL, Metrics::CELL);
            if pressed {
                draw::fill_round_rect(dc, r, Metrics::CELL_RADIUS, pal.cell_press)?;
            } else if hovered {
                draw::fill_round_rect(dc, r, Metrics::CELL_RADIUS, pal.cell_hover)?;
            }
            let pad = (Metrics::CELL - Metrics::ICON) / 2.0;
            let ir = Rect::new(pad, pad, Metrics::ICON, Metrics::ICON);
            let opacity = if missing { 0.35 } else { 1.0 };
            match &bmp {
                Some(b) => draw::draw_bitmap(dc, b, icon_px, ir, 7.0, opacity)?,
                None => draw::glyph(&gfx, dc, Glyph::Terminal, ir.inset(1.0), pal.accent.with_alpha(opacity))?,
            }
            if missing {
                draw::fill_circle(dc, Metrics::CELL - 9.5, Metrics::CELL - 9.5, 4.5, pal.missing_badge)?;
            }
            if focused {
                draw::stroke_round_rect(dc, r, Metrics::CELL_RADIUS, pal.accent, 2.0)?;
            }
            Ok(())
        })
    }

    fn paint_button(&self, v: &Visual, hit: Hit, focus: Focus, g: Glyph, dot: bool) -> Result<()> {
        let pal = self.pal;
        let hovered = self.hover == hit;
        let pressed = self.pressed == Some(hit);
        let focused = self.focus == Some(focus);
        let gfx = self.gfx.clone();
        draw::draw(&v.surface, self.scale, |dc| {
            let r = Rect::new(0.0, 0.0, Metrics::CELL, Metrics::CELL);
            if pressed {
                draw::fill_round_rect(dc, r, Metrics::CELL_RADIUS, pal.cell_press)?;
            } else if hovered {
                draw::fill_round_rect(dc, r, Metrics::CELL_RADIUS, pal.cell_hover)?;
            }
            draw::glyph(&gfx, dc, g, Rect::new(13.0, 13.0, 22.0, 22.0), pal.glyph)?;
            if dot {
                draw::fill_circle(dc, 36.0, 12.0, 3.5, pal.accent)?;
            }
            if focused {
                draw::stroke_round_rect(dc, r, Metrics::CELL_RADIUS, pal.accent, 2.0)?;
            }
            Ok(())
        })
    }

    fn paint_add_button(&self) -> Result<()> {
        match &self.add {
            Some(v) => self.paint_button(v, Hit::Add, Focus::Add, Glyph::Plus, false),
            None => Ok(()),
        }
    }

    fn paint_settings_button(&self) -> Result<()> {
        match &self.settings_btn {
            Some(v) => self.paint_button(v, Hit::Settings, Focus::Settings, Glyph::Gear, self.update_dot),
            None => Ok(()),
        }
    }

    fn repaint_hit(&self, h: Hit) {
        let _ = match h {
            Hit::Cell(i) => self.paint_cell(i),
            Hit::Add => self.paint_add_button(),
            Hit::Settings => self.paint_settings_button(),
            _ => Ok(()),
        };
    }

    fn visual_for(&self, h: Hit) -> Option<&IDCompositionVisual2> {
        match h {
            Hit::Cell(i) => self.cells.get(i).map(|c| &c.visual),
            Hit::Add => self.add.as_ref().map(|c| &c.visual),
            Hit::Settings => self.settings_btn.as_ref().map(|c| &c.visual),
            _ => None,
        }
    }

    fn set_hover_anim(&self, h: Hit, scale_to: f32, lift: f32) {
        if let Some(v) = self.visual_for(h) {
            let c = (self.phys(Metrics::CELL / 2.0), self.phys(Metrics::CELL / 2.0));
            let _ = apply_scale(&self.gfx.dcomp, v, &AnimSpec::new(if scale_to > 1.0 { 1.0 } else { 1.06 }, scale_to, HOVER_MS, 0, Ease::OutCubic), c, self.animate);
            let base_y = match h {
                Hit::Cell(_) => 0.0,
                _ => self.phys(Metrics::MARGIN + Metrics::PAD_Y),
            };
            let _ = apply_offset_y(&self.gfx.dcomp, v, &AnimSpec::new(base_y, base_y - self.phys(lift), HOVER_MS, 0, Ease::OutCubic), self.animate);
        }
    }

    fn animate_open(&mut self) -> Result<()> {
        let Some(g) = &self.group else { return Ok(()) };
        let d = &self.gfx.dcomp;
        let c = self.plate_center();
        apply_opacity(d, g, &AnimSpec::new(0.0, 1.0, OPEN_PLATE_MS, 0, Ease::OutCubic), self.animate)?;
        apply_offset_y(d, g, &AnimSpec::new(self.phys(SLIDE_PX), 0.0, OPEN_PLATE_MS, 0, Ease::OutCubic), self.animate)?;
        apply_scale(d, g, &AnimSpec::new(0.98, 1.0, OPEN_PLATE_MS, 0, Ease::OutCubic), c, self.animate)?;
        let visible = layout::visible_cells(&self.layout);
        for (k, i) in visible.iter().enumerate() {
            if let Some(cv) = self.cells.get(*i) {
                let delay = OPEN_FIRST_DELAY_MS + OPEN_STAGGER_MS * k as u32;
                apply_opacity(d, &cv.visual, &AnimSpec::new(0.0, 1.0, OPEN_ICON_MS, delay, Ease::OutCubic), self.animate)?;
                apply_offset_y(d, &cv.visual, &AnimSpec::new(self.phys(ICON_SLIDE_PX), 0.0, OPEN_ICON_MS, delay, Ease::OutCubic), self.animate)?;
            }
        }
        Ok(())
    }

    // ---- interacción ----

    fn hit_at(&self, lparam: LPARAM) -> Hit {
        let (x, y) = window::lparam_xy(lparam);
        layout::hit_test(&self.layout, x as f32 / self.scale, y as f32 / self.scale)
    }

    fn set_hover(&mut self, h: Hit) {
        if h == self.hover {
            return;
        }
        let old = self.hover;
        self.hover = h;
        if matches!(old, Hit::Cell(_) | Hit::Add | Hit::Settings) {
            self.repaint_hit(old);
            self.set_hover_anim(old, 1.0, 0.0);
        }
        if matches!(h, Hit::Cell(_) | Hit::Add | Hit::Settings) {
            self.repaint_hit(h);
            self.set_hover_anim(h, 1.06, 1.0);
        }
        self.commit();
    }

    fn set_pressed(&mut self, p: Option<Hit>) {
        if p == self.pressed {
            return;
        }
        let old = self.pressed;
        self.pressed = p;
        if let Some(o) = old {
            self.repaint_hit(o);
            if let Some(v) = self.visual_for(o) {
                let c = (self.phys(Metrics::CELL / 2.0), self.phys(Metrics::CELL / 2.0));
                let to = if self.hover == o { 1.06 } else { 1.0 };
                let _ = apply_scale(&self.gfx.dcomp, v, &AnimSpec::new(0.94, to, PRESS_MS, 0, Ease::OutCubic), c, self.animate);
            }
        }
        if let Some(n) = p {
            self.repaint_hit(n);
            if let Some(v) = self.visual_for(n) {
                let c = (self.phys(Metrics::CELL / 2.0), self.phys(Metrics::CELL / 2.0));
                let _ = apply_scale(&self.gfx.dcomp, v, &AnimSpec::new(1.06, 0.94, PRESS_MS, 0, Ease::OutCubic), c, self.animate);
            }
        }
        self.commit();
    }

    fn on_click(&mut self, h: Hit) {
        match h {
            Hit::Cell(i) => self.launch_entry(i),
            Hit::Add => log_info!("add clicked (pending task)"),
            Hit::Settings => log_info!("settings clicked (pending task)"),
            Hit::ArrowLeft => self.scroll_by(-(self.viewport_w() - Metrics::CELL), SCROLL_ARROW_MS),
            Hit::ArrowRight => self.scroll_by(self.viewport_w() - Metrics::CELL, SCROLL_ARROW_MS),
            _ => {}
        }
    }

    fn launch_entry(&mut self, i: usize) {
        let path = match self.entries.borrow().entries.get(i) {
            Some(e) => e.path.clone(),
            None => return,
        };
        if !path.exists() {
            if i < self.missing.len() && !self.missing[i] {
                self.missing[i] = true;
                let _ = self.paint_cell(i);
                self.commit();
            }
            return;
        }
        match crate::launch::launch(&path) {
            Ok(()) => {
                log_info!("launched {}", path.display());
                self.hide();
            }
            Err(msg) => log_warn!("launch failed for {}: {msg}", path.display()),
        }
    }

    fn viewport_w(&self) -> f32 {
        self.layout.scroll.as_ref().map(|s| s.viewport.w).unwrap_or(0.0)
    }

    pub fn scroll_by(&mut self, delta: f32, ms: u32) {
        let Some(s) = self.layout.scroll.clone() else { return };
        let target = layout::clamp_scroll(self.scroll_offset + delta, s.max_offset);
        self.scroll_to(target, ms);
    }

    pub fn scroll_to(&mut self, target: f32, ms: u32) {
        let Some(s) = self.layout.scroll.clone() else { return };
        let target = layout::clamp_scroll(target, s.max_offset);
        if (target - self.scroll_offset).abs() < 0.01 {
            return;
        }
        let from = self.scroll_offset;
        self.scroll_offset = target;
        self.relayout();
        if let Some(c) = &self.content {
            let animate = self.animate && ms > 0;
            let _ = apply_offset_x(&self.gfx.dcomp, c, &AnimSpec::new(-self.phys(from), -self.phys(target), ms.max(1), 0, Ease::OutCubic), animate);
        }
        self.update_scroll_chrome();
        self.commit();
    }

    fn update_scroll_chrome(&self) {
        let Some(s) = &self.layout.scroll else { return };
        let d = &self.gfx.dcomp;
        if let Some((lf, rf)) = &self.fades {
            let _ = apply_opacity(d, &lf.visual, &AnimSpec::new(if s.fade_left { 0.0 } else { 1.0 }, if s.fade_left { 1.0 } else { 0.0 }, 120, 0, Ease::OutCubic), self.animate);
            let _ = apply_opacity(d, &rf.visual, &AnimSpec::new(if s.fade_right { 0.0 } else { 1.0 }, if s.fade_right { 1.0 } else { 0.0 }, 120, 0, Ease::OutCubic), self.animate);
        }
        if let Some((la, ra)) = &self.arrows {
            let _ = set_opacity(&la.visual, if s.fade_left { 1.0 } else { 0.35 });
            let _ = set_opacity(&ra.visual, if s.fade_right { 1.0 } else { 0.35 });
        }
    }

    fn on_deactivate(&mut self, other: HWND) {
        if self.visible && !window::is_ours(other) {
            self.hide();
        }
    }
}

/// Degradado horizontal de `color` opaco a transparente (o al revés si `!left_to_right`).
fn gradient(dc: &ID2D1DeviceContext, w: f32, h: f32, color: crate::theme::Color, left_solid: bool) -> Result<()> {
    unsafe {
        let rt: ID2D1RenderTarget = windows::core::Interface::cast(dc)?;
        let stops = [
            D2D1_GRADIENT_STOP { position: 0.0, color: color.with_alpha(if left_solid { 0.95 } else { 0.0 }).to_d2d() },
            D2D1_GRADIENT_STOP { position: 1.0, color: color.with_alpha(if left_solid { 0.0 } else { 0.95 }).to_d2d() },
        ];
        let coll = rt.CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(0.0, 0.0), endPoint: pt(w, 0.0) };
        let b = rt.CreateLinearGradientBrush(&props, None, &coll)?;
        rt.FillRectangle(&D2D_RECT_F { left: 0.0, top: 0.0, right: w, bottom: h }, &b);
        Ok(())
    }
}

impl WindowHandler for Toolbar {
    fn on_message(&mut self, ctx: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match msg {
            WM_MOUSEMOVE => {
                let h = self.hit_at(lparam);
                self.set_hover(h);
                window::invalidate_tracking(ctx.hwnd);
                Some(LRESULT(0))
            }
            WM_MOUSELEAVE => {
                self.set_hover(Hit::Outside);
                Some(LRESULT(0))
            }
            WM_LBUTTONDOWN => {
                let h = self.hit_at(lparam);
                if matches!(h, Hit::Cell(_) | Hit::Add | Hit::Settings | Hit::ArrowLeft | Hit::ArrowRight) {
                    self.set_pressed(Some(h));
                    window::capture_mouse(ctx.hwnd);
                }
                Some(LRESULT(0))
            }
            WM_LBUTTONUP => {
                window::release_mouse();
                let h = self.hit_at(lparam);
                let pressed = self.pressed;
                self.set_pressed(None);
                if pressed == Some(h) {
                    self.on_click(h);
                }
                Some(LRESULT(0))
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                let delta = window::wheel_delta(wparam);
                let steps = if msg == WM_MOUSEWHEEL { -delta } else { delta } as f32 / 120.0;
                self.scroll_by(steps * Metrics::STEP, SCROLL_WHEEL_MS);
                Some(LRESULT(0))
            }
            WM_KEYDOWN => {
                if wparam.0 as u16 == VK_ESCAPE.0 {
                    self.hide();
                    return Some(LRESULT(0));
                }
                None
            }
            WM_ACTIVATE => {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    self.on_deactivate(HWND(lparam.0 as *mut c_void));
                }
                Some(LRESULT(0))
            }
            WM_NCHITTEST => {
                let (sx, sy) = window::lparam_xy(lparam);
                let (cx, cy) = window::screen_to_client(ctx.hwnd, sx, sy);
                let inside = self.layout.plate.contains(cx as f32 / self.scale, cy as f32 / self.scale);
                Some(LRESULT(if inside { HTCLIENT as isize } else { HTTRANSPARENT as isize }))
            }
            WM_SETCURSOR => {
                window::set_arrow_cursor();
                Some(LRESULT(1))
            }
            WM_TIMER => {
                if wparam.0 == window::TIMER_HIDE {
                    self.finish_hide();
                    return Some(LRESULT(0));
                }
                None
            }
            window::WM_ICON_READY => {
                let r = unsafe { Box::from_raw(lparam.0 as *mut IconResult) };
                self.on_icon_ready(r);
                Some(LRESULT(0))
            }
            window::WM_THEME_CHANGED | window::WM_ENTRIES_CHANGED => {
                if self.visible {
                    self.pal = theme::palette(self.settings.borrow().theme);
                    self.relayout();
                    self.load_icons();
                    let _ = self.build_visuals(None);
                    self.commit();
                }
                Some(LRESULT(0))
            }
            WM_CLOSE => {
                self.hide();
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}
