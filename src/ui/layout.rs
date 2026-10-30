//! Geometría pura de la barra: celdas, carrusel, hit-test y posición. Sin Win32.

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
    pub fn inset(&self, d: f32) -> Rect {
        Rect::new(self.x + d, self.y + d, self.w - 2.0 * d, self.h - 2.0 * d)
    }
}

pub struct Metrics;

impl Metrics {
    pub const CELL: f32 = 48.0;
    pub const ICON: f32 = 34.0;
    pub const GAP: f32 = 4.0;
    pub const PAD_X: f32 = 8.0;
    pub const PAD_Y: f32 = 6.0;
    pub const RADIUS: f32 = 12.0;
    pub const CELL_RADIUS: f32 = 9.0;
    pub const SEP_W: f32 = 1.0;
    pub const SEP_H: f32 = 26.0;
    pub const SEP_MARGIN: f32 = 4.0;
    pub const ARROW_W: f32 = 20.0;
    pub const FADE_W: f32 = 22.0;
    pub const MARGIN: f32 = 32.0;
    pub const EDGE_GAP: f32 = 8.0;
    pub const MAX_FRACTION: f32 = 0.8;
    pub const EMPTY_TEXT_W: f32 = 200.0;
    pub const STEP: f32 = Self::CELL + Self::GAP;
    pub const HEIGHT: f32 = Self::PAD_Y * 2.0 + Self::CELL;
}

#[derive(Clone, PartialEq, Debug)]
pub struct Scroll {
    pub viewport: Rect,
    pub content_w: f32,
    pub offset: f32,
    pub max_offset: f32,
    pub left_arrow: Rect,
    pub right_arrow: Rect,
    pub fade_left: bool,
    pub fade_right: bool,
}

#[derive(Clone, PartialEq, Debug)]
pub struct BarLayout {
    pub plate: Rect,
    pub window: (f32, f32),
    pub cells: Vec<Rect>,
    pub sep: Option<Rect>,
    pub add: Rect,
    pub settings: Rect,
    pub scroll: Option<Scroll>,
    pub empty_text: Option<Rect>,
}

pub fn clamp_scroll(offset: f32, max: f32) -> f32 {
    offset.max(0.0).min(max.max(0.0))
}

fn fixed_width(n: usize) -> f32 {
    if n > 0 {
        Metrics::SEP_MARGIN * 2.0 + Metrics::SEP_W + Metrics::CELL * 2.0 + Metrics::GAP
    } else {
        Metrics::CELL * 2.0 + Metrics::GAP
    }
}

pub fn compute(n: usize, work_w: f32, scroll_offset: f32) -> BarLayout {
    let m = Metrics::MARGIN;
    let fixed = fixed_width(n);
    let natural = if n > 0 { n as f32 * Metrics::CELL + (n as f32 - 1.0) * Metrics::GAP } else { 0.0 };
    let max_icons = work_w * Metrics::MAX_FRACTION - Metrics::PAD_X * 2.0 - fixed;

    let mut scroll = None;
    let icons_w;
    let cells_x0;
    if n == 0 {
        icons_w = Metrics::EMPTY_TEXT_W;
        cells_x0 = m + Metrics::PAD_X;
    } else if natural <= max_icons {
        icons_w = natural;
        cells_x0 = m + Metrics::PAD_X;
    } else {
        let avail = max_icons - Metrics::ARROW_W * 2.0;
        let cells_fit = ((avail + Metrics::GAP) / Metrics::STEP).floor().max(1.0);
        let viewport_w = cells_fit * Metrics::STEP - Metrics::GAP;
        let max_offset = (natural - viewport_w).max(0.0);
        let offset = clamp_scroll(scroll_offset, max_offset);
        icons_w = Metrics::ARROW_W * 2.0 + viewport_w;
        let vx = m + Metrics::PAD_X + Metrics::ARROW_W;
        cells_x0 = vx - offset;
        let cy = m + Metrics::PAD_Y;
        scroll = Some(Scroll {
            viewport: Rect::new(vx, cy, viewport_w, Metrics::CELL),
            content_w: natural,
            offset,
            max_offset,
            left_arrow: Rect::new(m + Metrics::PAD_X, cy, Metrics::ARROW_W, Metrics::CELL),
            right_arrow: Rect::new(vx + viewport_w, cy, Metrics::ARROW_W, Metrics::CELL),
            fade_left: offset > 0.5,
            fade_right: offset < max_offset - 0.5,
        });
    }

    let plate_w = Metrics::PAD_X * 2.0 + icons_w + fixed;
    let plate = Rect::new(m, m, plate_w, Metrics::HEIGHT);
    let cy = m + Metrics::PAD_Y;
    let cells: Vec<Rect> = (0..n).map(|i| Rect::new(cells_x0 + i as f32 * Metrics::STEP, cy, Metrics::CELL, Metrics::CELL)).collect();

    let mut x = m + Metrics::PAD_X + icons_w;
    let sep = if n > 0 {
        x += Metrics::SEP_MARGIN;
        let r = Rect::new(x, m + (Metrics::HEIGHT - Metrics::SEP_H) / 2.0, Metrics::SEP_W, Metrics::SEP_H);
        x += Metrics::SEP_W + Metrics::SEP_MARGIN;
        Some(r)
    } else {
        None
    };
    let add = Rect::new(x, cy, Metrics::CELL, Metrics::CELL);
    x += Metrics::CELL + Metrics::GAP;
    let settings = Rect::new(x, cy, Metrics::CELL, Metrics::CELL);
    let empty_text = if n == 0 { Some(Rect::new(m + Metrics::PAD_X, cy, Metrics::EMPTY_TEXT_W, Metrics::CELL)) } else { None };

    BarLayout { plate, window: (plate_w + 2.0 * m, Metrics::HEIGHT + 2.0 * m), cells, sep, add, settings, scroll, empty_text, }
}

/// Índices de celdas enteras dentro del viewport; todas si no hay carrusel.
pub fn visible_cells(l: &BarLayout) -> Vec<usize> {
    match &l.scroll {
        None => (0..l.cells.len()).collect(),
        Some(s) => l
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.x >= s.viewport.x - 0.5 && c.right() <= s.viewport.right() + 0.5)
            .map(|(i, _)| i)
            .collect(),
    }
}

/// Destino de la inercia y duración en ms. `velocity` en px/s hacia la derecha (positiva = contenido hacia la derecha).
pub fn inertia_target(offset: f32, velocity_px_per_s: f32, max: f32) -> (f32, u32) {
    if velocity_px_per_s.abs() < 1.0 {
        return (clamp_scroll(offset, max), 0);
    }
    let target = clamp_scroll(offset - velocity_px_per_s * 0.25, max);
    let dist = (target - offset).abs();
    if dist < 0.5 {
        return (target, 0);
    }
    let ms = (dist / velocity_px_per_s.abs() * 1000.0 * 2.0).clamp(120.0, 400.0);
    (target, ms as u32)
}

/// Desplazamiento mínimo para que la celda `idx` quede entera en el viewport.
pub fn scroll_to_reveal(l: &BarLayout, idx: usize) -> f32 {
    let Some(s) = &l.scroll else { return 0.0 };
    let Some(c) = l.cells.get(idx) else { return s.offset };
    let content_x = c.x + s.offset - s.viewport.x; // posición de la celda dentro del contenido
    if content_x < s.offset {
        clamp_scroll(content_x, s.max_offset)
    } else if content_x + c.w > s.offset + s.viewport.w {
        clamp_scroll(content_x + c.w - s.viewport.w, s.max_offset)
    } else {
        s.offset
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Cell(usize),
    Add,
    Settings,
    ArrowLeft,
    ArrowRight,
    Plate,
    Outside,
}

pub fn hit_test(l: &BarLayout, x: f32, y: f32) -> Hit {
    if !l.plate.contains(x, y) {
        return Hit::Outside;
    }
    if l.add.contains(x, y) {
        return Hit::Add;
    }
    if l.settings.contains(x, y) {
        return Hit::Settings;
    }
    if let Some(s) = &l.scroll {
        if s.left_arrow.contains(x, y) {
            return Hit::ArrowLeft;
        }
        if s.right_arrow.contains(x, y) {
            return Hit::ArrowRight;
        }
        if !s.viewport.contains(x, y) {
            return Hit::Plate;
        }
    }
    for (i, c) in l.cells.iter().enumerate() {
        if c.contains(x, y) {
            return Hit::Cell(i);
        }
    }
    Hit::Plate
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorkArea {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub taskbar_top: bool,
}

/// Esquina superior izquierda de la ventana (con margen) para un área de trabajo.
pub fn window_position(l: &BarLayout, wa: WorkArea) -> (f32, f32) {
    let m = Metrics::MARGIN;
    let x = wa.x + (wa.w - l.plate.w) / 2.0 - m;
    let y = if wa.taskbar_top { wa.y + Metrics::EDGE_GAP - m } else { wa.y + wa.h - Metrics::EDGE_GAP - l.plate.h - m };
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_entries_fit_on_fullhd() {
        let l = compute(10, 1920.0, 0.0);
        assert!(l.scroll.is_none());
        assert_eq!(l.cells.len(), 10);
        assert_eq!(l.plate.h, 60.0);
        assert_eq!(l.cells[1].x - l.cells[0].x, 52.0);
        assert!(l.sep.is_some());
        assert!((l.settings.right() + Metrics::PAD_X - l.plate.right()).abs() < 0.01);
        assert_eq!(l.window, (l.plate.w + 64.0, 124.0));
    }

    #[test]
    fn empty_bar_has_text_and_no_separator() {
        let l = compute(0, 1920.0, 0.0);
        assert!(l.sep.is_none() && l.empty_text.is_some() && l.cells.is_empty());
    }

    #[test]
    fn many_entries_activate_carousel_capped_at_80_percent() {
        let l = compute(60, 1920.0, 0.0);
        let s = l.scroll.as_ref().unwrap();
        assert!(l.plate.w <= 1920.0 * 0.8 + 0.01);
        assert!(s.max_offset > 0.0 && !s.fade_left && s.fade_right);
        assert_eq!(s.left_arrow.w, 20.0);
        let l2 = compute(60, 1920.0, s.max_offset);
        let s2 = l2.scroll.as_ref().unwrap();
        assert!(s2.fade_left && !s2.fade_right);
        assert!((s.viewport.w + Metrics::GAP) % Metrics::STEP < 0.01);
    }

    #[test]
    fn clamp_and_inertia() {
        assert_eq!(clamp_scroll(-5.0, 100.0), 0.0);
        assert_eq!(clamp_scroll(500.0, 100.0), 100.0);
        let (t, d) = inertia_target(50.0, -400.0, 300.0);
        assert_eq!(t, 150.0);
        assert!(d > 0 && d <= 400);
        let (t2, _) = inertia_target(50.0, 4000.0, 300.0);
        assert_eq!(t2, 0.0);
        assert_eq!(inertia_target(50.0, 0.0, 300.0), (50.0, 0));
    }

    #[test]
    fn hit_test_finds_cells_and_controls() {
        let l = compute(3, 1920.0, 0.0);
        let (cx, cy) = l.cells[1].center();
        assert_eq!(hit_test(&l, cx, cy), Hit::Cell(1));
        let (ax, ay) = l.add.center();
        assert_eq!(hit_test(&l, ax, ay), Hit::Add);
        let (sx, sy) = l.settings.center();
        assert_eq!(hit_test(&l, sx, sy), Hit::Settings);
        assert_eq!(hit_test(&l, l.plate.x + 1.0, l.plate.y + 1.0), Hit::Plate);
        assert_eq!(hit_test(&l, 1.0, 1.0), Hit::Outside);
    }

    #[test]
    fn hidden_cells_are_not_hit_in_carousel() {
        let l = compute(60, 1920.0, 0.0);
        let last = l.cells[59];
        assert_eq!(hit_test(&l, last.center().0, last.center().1), Hit::Outside);
        assert!(!visible_cells(&l).contains(&59));
        assert!(visible_cells(&l).contains(&0));
        let s = l.scroll.as_ref().unwrap();
        assert_eq!(hit_test(&l, s.left_arrow.center().0, s.left_arrow.center().1), Hit::ArrowLeft);
    }

    #[test]
    fn scroll_to_reveal_moves_minimally() {
        let l = compute(60, 1920.0, 0.0);
        let off = scroll_to_reveal(&l, 59);
        let l2 = compute(60, 1920.0, off);
        assert!(visible_cells(&l2).contains(&59));
        assert_eq!(scroll_to_reveal(&l, 0), 0.0);
        let back = scroll_to_reveal(&l2, 0);
        assert_eq!(back, 0.0);
    }

    #[test]
    fn window_position_bottom_and_top_taskbar() {
        let l = compute(10, 1920.0, 0.0);
        let wa = WorkArea { x: 0.0, y: 0.0, w: 1920.0, h: 1032.0, taskbar_top: false };
        let (x, y) = window_position(&l, wa);
        assert!((x + Metrics::MARGIN + l.plate.w / 2.0 - 960.0).abs() < 0.01);
        assert_eq!(y + Metrics::MARGIN + 60.0, 1032.0 - 8.0);
        let wa2 = WorkArea { x: 0.0, y: 48.0, w: 1920.0, h: 1032.0, taskbar_top: true };
        let (_, y2) = window_position(&l, wa2);
        assert_eq!(y2 + Metrics::MARGIN, 48.0 + 8.0);
    }
}
