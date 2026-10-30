//! Widgets de cristal para diálogos y ajustes: cabecera, tarjetas, interruptor, desplegable,
//! botón, campo de texto, captura de atajo, enlace y vista previa de icono.
//!
//! La disposición y el estado son puros; solo `draw` toca Direct2D.

use crate::gfx::draw::{self, Glyph, TextStyle};
use crate::gfx::Gfx;
use crate::model::settings::Hotkey;
use crate::theme::{Color, Palette};
use crate::ui::layout::Rect;
use windows::Win32::Graphics::Direct2D::*;
use windows::core::Result;

pub type WidgetId = u32;

pub const MARGIN: f32 = 18.0;
pub const HEADER_H: f32 = 44.0;
pub const SECTION_H: f32 = 28.0;
pub const CARD_H: f32 = 56.0;
pub const CARD_SUB_H: f32 = 72.0;
pub const LABEL_H: f32 = 20.0;
pub const CONTROL_H: f32 = 32.0;
pub const ROW_GAP: f32 = 8.0;
pub const CARD_GAP: f32 = 4.0;
pub const FIELD_GAP: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub struct TextState {
    pub text: String,
    pub caret: usize,
    pub sel_anchor: Option<usize>,
    pub max_len: usize,
    pub placeholder: String,
    pub error: Option<String>,
}

impl TextState {
    pub fn new(text: &str) -> TextState {
        TextState { text: text.to_string(), caret: text.chars().count(), sel_anchor: None, max_len: 4096, placeholder: String::new(), error: None }
    }
    pub fn with_max(text: &str, max_len: usize) -> TextState {
        TextState { max_len, ..TextState::new(text) }
    }
    fn len(&self) -> usize {
        self.text.chars().count()
    }
    fn byte_at(&self, ci: usize) -> usize {
        self.text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(self.text.len())
    }
    pub fn selected_range(&self) -> Option<(usize, usize)> {
        let a = self.sel_anchor?;
        if a == self.caret {
            return None;
        }
        Some((a.min(self.caret), a.max(self.caret)))
    }
    pub fn delete_selection(&mut self) -> bool {
        let Some((s, e)) = self.selected_range() else {
            self.sel_anchor = None;
            return false;
        };
        let (bs, be) = (self.byte_at(s), self.byte_at(e));
        self.text.replace_range(bs..be, "");
        self.caret = s;
        self.sel_anchor = None;
        true
    }
    pub fn insert_str(&mut self, s: &str) {
        self.delete_selection();
        let room = self.max_len.saturating_sub(self.len());
        let s: String = s.chars().filter(|c| !c.is_control()).take(room).collect();
        if s.is_empty() {
            return;
        }
        let b = self.byte_at(self.caret);
        self.text.insert_str(b, &s);
        self.caret += s.chars().count();
    }
    pub fn backspace(&mut self) {
        if self.delete_selection() || self.caret == 0 {
            return;
        }
        let (bs, be) = (self.byte_at(self.caret - 1), self.byte_at(self.caret));
        self.text.replace_range(bs..be, "");
        self.caret -= 1;
    }
    pub fn delete(&mut self) {
        if self.delete_selection() || self.caret >= self.len() {
            return;
        }
        let (bs, be) = (self.byte_at(self.caret), self.byte_at(self.caret + 1));
        self.text.replace_range(bs..be, "");
    }
    fn move_to(&mut self, pos: usize, shift: bool) {
        if shift {
            if self.sel_anchor.is_none() {
                self.sel_anchor = Some(self.caret);
            }
        } else {
            self.sel_anchor = None;
        }
        self.caret = pos.min(self.len());
    }
    pub fn move_left(&mut self, shift: bool) {
        if !shift {
            if let Some((s, _)) = self.selected_range() {
                self.caret = s;
                self.sel_anchor = None;
                return;
            }
        }
        let p = self.caret.saturating_sub(1);
        self.move_to(p, shift);
    }
    pub fn move_right(&mut self, shift: bool) {
        if !shift {
            if let Some((_, e)) = self.selected_range() {
                self.caret = e;
                self.sel_anchor = None;
                return;
            }
        }
        let p = self.caret + 1;
        self.move_to(p, shift);
    }
    pub fn home(&mut self, shift: bool) {
        self.move_to(0, shift);
    }
    pub fn end(&mut self, shift: bool) {
        let n = self.len();
        self.move_to(n, shift);
    }
    pub fn select_all(&mut self) {
        self.sel_anchor = Some(0);
        self.caret = self.len();
    }
    pub fn selected_text(&self) -> String {
        match self.selected_range() {
            Some((s, e)) => self.text.chars().skip(s).take(e - s).collect(),
            None => String::new(),
        }
    }
    pub fn set_text(&mut self, t: &str) {
        self.text = t.to_string();
        self.caret = self.len();
        self.sel_anchor = None;
    }
}

#[derive(Clone, Debug)]
pub enum Kind {
    Header { title: String },
    SectionTitle { text: String },
    Card { title: String, subtitle: Option<String>, glyph: Option<Glyph>, child: Option<WidgetId> },
    Label { text: String, dim: bool, size: f32, color: Option<Color> },
    Button { text: String, accent: bool, glyph: Option<Glyph>, width: f32 },
    Toggle { on: bool },
    Dropdown { options: Vec<String>, selected: usize, open: bool },
    TextField(TextState),
    HotkeyField { value: Option<Hotkey>, capturing: bool, error: Option<String> },
    Link { text: String },
    IconPreview { bitmap: Option<ID2D1Bitmap1>, bitmap_px: u32, glyph: Glyph },
    Row { children: Vec<WidgetId>, align_right: bool },
    Spacer { h: f32 },
}

#[derive(Clone, Debug)]
pub struct Widget {
    pub id: WidgetId,
    pub kind: Kind,
    pub rect: Rect,
    pub enabled: bool,
    pub top_level: bool,
    pub visible: bool,
}

impl Widget {
    pub fn focusable(&self) -> bool {
        self.enabled && self.visible && matches!(self.kind, Kind::Button { .. } | Kind::Toggle { .. } | Kind::Dropdown { .. } | Kind::TextField(_) | Kind::HotkeyField { .. } | Kind::Link { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Clicked(WidgetId),
    Toggled(WidgetId, bool),
    Selected(WidgetId, usize),
    TextChanged(WidgetId),
    HotkeyChanged(WidgetId, Option<Hotkey>),
    Close,
    Submit,
    Cancel,
    Repaint,
}

#[derive(Default)]
pub struct Form {
    pub widgets: Vec<Widget>,
    pub focus: Option<WidgetId>,
    pub hover: Option<WidgetId>,
    pub pressed: Option<WidgetId>,
    pub width: f32,
    pub content_h: f32,
    pub caret_visible: bool,
    pub close_hover: bool,
    pub default_button: Option<WidgetId>,
    next_id: WidgetId,
    dragging_text: Option<WidgetId>,
}

impl Form {
    pub fn new(width: f32) -> Form {
        Form { width, caret_visible: true, next_id: 1, ..Default::default() }
    }

    pub fn add(&mut self, kind: Kind) -> WidgetId {
        self.add_with(kind, true)
    }

    /// Widget hijo de una tarjeta o una fila; no participa en la disposición vertical.
    pub fn add_child(&mut self, kind: Kind) -> WidgetId {
        self.add_with(kind, false)
    }

    fn add_with(&mut self, kind: Kind, top_level: bool) -> WidgetId {
        let id = self.next_id;
        self.next_id += 1;
        self.widgets.push(Widget { id, kind, rect: Rect::default(), enabled: true, top_level, visible: true });
        id
    }

    pub fn get(&self, id: WidgetId) -> Option<&Widget> {
        self.widgets.iter().find(|w| w.id == id)
    }

    pub fn get_mut(&mut self, id: WidgetId) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|w| w.id == id)
    }

    pub fn set_enabled(&mut self, id: WidgetId, on: bool) {
        if let Some(w) = self.get_mut(id) {
            w.enabled = on;
        }
        if !on && self.focus == Some(id) {
            self.focus = None;
        }
    }

    pub fn set_visible(&mut self, id: WidgetId, on: bool) {
        if let Some(w) = self.get_mut(id) {
            w.visible = on;
        }
    }

    pub fn text(&self, id: WidgetId) -> String {
        match self.get(id).map(|w| &w.kind) {
            Some(Kind::TextField(t)) => t.text.clone(),
            _ => String::new(),
        }
    }

    pub fn set_text(&mut self, id: WidgetId, text: &str) {
        if let Some(Widget { kind: Kind::TextField(t), .. }) = self.get_mut(id) {
            t.set_text(text);
        }
    }

    pub fn set_label(&mut self, id: WidgetId, text: &str) {
        match self.get_mut(id).map(|w| &mut w.kind) {
            Some(Kind::Label { text: t, .. }) | Some(Kind::Button { text: t, .. }) | Some(Kind::Link { text: t }) => *t = text.to_string(),
            Some(Kind::Card { subtitle, .. }) => *subtitle = Some(text.to_string()),
            _ => {}
        }
    }

    pub fn set_label_color(&mut self, id: WidgetId, color: Option<Color>) {
        if let Some(Kind::Label { color: c, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
            *c = color;
        }
    }

    pub fn set_toggle(&mut self, id: WidgetId, on: bool) {
        if let Some(Kind::Toggle { on: o }) = self.get_mut(id).map(|w| &mut w.kind) {
            *o = on;
        }
    }

    pub fn set_selected(&mut self, id: WidgetId, idx: usize) {
        if let Some(Kind::Dropdown { selected, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
            *selected = idx;
        }
    }

    pub fn set_hotkey(&mut self, id: WidgetId, hk: Option<Hotkey>, error: Option<String>) {
        if let Some(Kind::HotkeyField { value, capturing, error: e }) = self.get_mut(id).map(|w| &mut w.kind) {
            *value = hk;
            *capturing = false;
            *e = error;
        }
    }

    pub fn set_field_error(&mut self, id: WidgetId, error: Option<String>) {
        if let Some(Kind::TextField(t)) = self.get_mut(id).map(|w| &mut w.kind) {
            t.error = error;
        }
    }

    pub fn set_icon_preview(&mut self, id: WidgetId, bitmap: Option<ID2D1Bitmap1>, bitmap_px: u32) {
        if let Some(Kind::IconPreview { bitmap: b, bitmap_px: p, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
            *b = bitmap;
            *p = bitmap_px;
        }
    }

    // ---- disposición ----

    fn height_of(&self, w: &Widget) -> f32 {
        match &w.kind {
            Kind::Header { .. } => HEADER_H,
            Kind::SectionTitle { .. } => SECTION_H,
            Kind::Card { subtitle, .. } => if subtitle.is_some() { CARD_SUB_H } else { CARD_H },
            Kind::Label { size, .. } => (size * 1.6).max(LABEL_H),
            Kind::Button { .. } | Kind::Dropdown { .. } | Kind::TextField(_) | Kind::HotkeyField { .. } | Kind::Row { .. } => CONTROL_H,
            Kind::Toggle { .. } => 20.0,
            Kind::Link { .. } => LABEL_H,
            Kind::IconPreview { .. } => 64.0,
            Kind::Spacer { h } => *h,
        }
    }

    fn child_width(&self, w: &Widget) -> f32 {
        match &w.kind {
            Kind::Toggle { .. } => 40.0,
            Kind::Dropdown { .. } => 150.0,
            Kind::HotkeyField { .. } => 170.0,
            Kind::Button { width, .. } => *width,
            Kind::Link { text } => text.chars().count() as f32 * 6.5 + 8.0,
            Kind::Label { text, size, .. } => text.chars().count() as f32 * size * 0.55 + 8.0,
            Kind::IconPreview { .. } => 64.0,
            Kind::TextField(_) => 200.0,
            Kind::Row { children, .. } => {
                let ws: f32 = children.iter().filter_map(|c| self.get(*c)).filter(|w| w.visible).map(|w| self.child_width(w)).sum();
                let n = children.iter().filter_map(|c| self.get(*c)).filter(|w| w.visible).count();
                ws + ROW_GAP * n.saturating_sub(1) as f32
            }
            _ => 100.0,
        }
    }

    pub fn layout(&mut self) {
        let mut y = 0.0;
        let width = self.width;
        let ids: Vec<WidgetId> = self.widgets.iter().filter(|w| w.top_level && w.visible).map(|w| w.id).collect();
        let mut prev_kind_card = false;
        for id in ids {
            let (h, is_card, is_field, is_section) = {
                let w = self.get(id).expect("widget");
                (self.height_of(w), matches!(w.kind, Kind::Card { .. }), matches!(w.kind, Kind::TextField(_) | Kind::Label { .. }), matches!(w.kind, Kind::SectionTitle { .. }))
            };
            if is_card && prev_kind_card {
                y += CARD_GAP;
            } else if is_section {
                y += 6.0;
            } else if !is_card && y > 0.0 {
                y += if is_field { 4.0 } else { FIELD_GAP };
            }
            let r = Rect::new(MARGIN, y, width - 2.0 * MARGIN, h);
            self.get_mut(id).expect("widget").rect = r;
            self.layout_children(id, r);
            y += h;
            prev_kind_card = is_card;
        }
        self.content_h = y + MARGIN;
    }

    fn layout_children(&mut self, id: WidgetId, r: Rect) {
        let kind = self.get(id).map(|w| w.kind.clone());
        match kind {
            Some(Kind::Card { child: Some(c), .. }) => {
                let cw = self.get(c).map(|w| self.child_width(w)).unwrap_or(40.0);
                let ch = self.get(c).map(|w| self.height_of(w)).unwrap_or(20.0);
                let cr = Rect::new(r.right() - 14.0 - cw, r.y + (r.h - ch) / 2.0, cw, ch);
                if let Some(w) = self.get_mut(c) {
                    w.rect = cr;
                }
                self.layout_children(c, cr);
            }
            Some(Kind::Row { children, align_right }) => {
                let visible: Vec<WidgetId> = children.iter().copied().filter(|c| self.get(*c).map(|w| w.visible).unwrap_or(false)).collect();
                let widths: Vec<f32> = visible.iter().map(|c| self.get(*c).map(|w| self.child_width(w)).unwrap_or(80.0)).collect();
                let total: f32 = widths.iter().sum::<f32>() + ROW_GAP * (visible.len().saturating_sub(1)) as f32;
                let mut x = if align_right { r.right() - total } else { r.x };
                for (c, cw) in visible.iter().zip(widths) {
                    let ch = self.get(*c).map(|w| self.height_of(w)).unwrap_or(CONTROL_H);
                    let cr = Rect::new(x, r.y + (r.h - ch) / 2.0, cw, ch);
                    if let Some(w) = self.get_mut(*c) {
                        w.rect = cr;
                    }
                    x += cw + ROW_GAP;
                }
            }
            _ => {}
        }
    }

    /// Rectángulo del botón de cerrar de la cabecera.
    fn close_rect(&self) -> Option<Rect> {
        self.widgets.iter().find(|w| matches!(w.kind, Kind::Header { .. })).map(|w| Rect::new(w.rect.right() - 28.0, w.rect.y + (w.rect.h - 28.0) / 2.0, 28.0, 28.0))
    }

    fn dropdown_popup_rect(&self, w: &Widget) -> Option<Rect> {
        if let Kind::Dropdown { options, open: true, .. } = &w.kind {
            let h = 6.0 + 30.0 * options.len() as f32;
            let below = Rect::new(w.rect.x, w.rect.bottom() + 4.0, w.rect.w, h);
            return Some(if below.bottom() > self.content_h - MARGIN { Rect::new(w.rect.x, w.rect.y - 4.0 - h, w.rect.w, h) } else { below });
        }
        None
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<WidgetId> {
        // Desplegable abierto primero.
        for w in &self.widgets {
            if let Some(r) = self.dropdown_popup_rect(w) {
                if r.contains(x, y) {
                    return Some(w.id);
                }
            }
        }
        // Hijos antes que contenedores.
        for w in self.widgets.iter().filter(|w| !w.top_level && w.visible && w.rect.contains(x, y)) {
            return Some(w.id);
        }
        for w in self.widgets.iter().filter(|w| w.top_level && w.visible && w.rect.contains(x, y)) {
            if let Kind::Card { child: Some(c), .. } = &w.kind {
                if self.get(*c).map(|cw| cw.focusable()).unwrap_or(false) {
                    return Some(*c);
                }
            }
            return Some(w.id);
        }
        None
    }

    // ---- entrada ----

    pub fn on_mouse_move(&mut self, x: f32, y: f32) -> bool {
        let h = self.hit(x, y);
        let ch = self.close_rect().map(|r| r.contains(x, y)).unwrap_or(false);
        let changed = h != self.hover || ch != self.close_hover;
        self.hover = h;
        self.close_hover = ch;
        if let Some(id) = self.dragging_text {
            if let Some(w) = self.get(id) {
                let rect = w.rect;
                if let Kind::TextField(t) = &w.kind {
                    let pos = caret_from_x(&t.text, x - rect.x - 10.0);
                    if let Some(Kind::TextField(t)) = self.get_mut(id).map(|w| &mut w.kind) {
                        if t.sel_anchor.is_none() {
                            t.sel_anchor = Some(t.caret);
                        }
                        t.caret = pos;
                    }
                    return true;
                }
            }
        }
        changed
    }

    pub fn on_mouse_down(&mut self, x: f32, y: f32) -> Option<Event> {
        if self.close_rect().map(|r| r.contains(x, y)).unwrap_or(false) {
            return Some(Event::Close);
        }
        // Cerrar desplegables abiertos si se pulsa fuera.
        let open_id = self.widgets.iter().find(|w| matches!(w.kind, Kind::Dropdown { open: true, .. })).map(|w| w.id);
        let h = self.hit(x, y);
        if let Some(oid) = open_id {
            if h != Some(oid) {
                if let Some(Kind::Dropdown { open, .. }) = self.get_mut(oid).map(|w| &mut w.kind) {
                    *open = false;
                }
                return Some(Event::Repaint);
            }
        }
        // Cancelar captura de atajo si se pulsa fuera.
        let capturing = self.widgets.iter().find(|w| matches!(w.kind, Kind::HotkeyField { capturing: true, .. })).map(|w| w.id);
        if let Some(cid) = capturing {
            if h != Some(cid) {
                if let Some(Kind::HotkeyField { capturing, .. }) = self.get_mut(cid).map(|w| &mut w.kind) {
                    *capturing = false;
                }
            }
        }
        let Some(id) = h else {
            return None;
        };
        let (enabled, kind, rect) = {
            let w = self.get(id)?;
            (w.enabled, w.kind.clone(), w.rect)
        };
        if !enabled {
            return None;
        }
        match kind {
            Kind::TextField(t) => {
                self.focus = Some(id);
                let pos = caret_from_x(&t.text, x - rect.x - 10.0);
                if let Some(Kind::TextField(t)) = self.get_mut(id).map(|w| &mut w.kind) {
                    t.caret = pos;
                    t.sel_anchor = None;
                }
                self.dragging_text = Some(id);
                self.caret_visible = true;
                Some(Event::Repaint)
            }
            Kind::Dropdown { options, open, .. } => {
                self.focus = Some(id);
                if open {
                    let popup = self.dropdown_popup_rect(self.get(id)?)?;
                    let idx = ((y - popup.y - 3.0) / 30.0).floor();
                    let mut ev = Event::Repaint;
                    if idx >= 0.0 && (idx as usize) < options.len() && popup.contains(x, y) {
                        if let Some(Kind::Dropdown { selected, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                            *selected = idx as usize;
                        }
                        ev = Event::Selected(id, idx as usize);
                    }
                    if let Some(Kind::Dropdown { open, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                        *open = false;
                    }
                    Some(ev)
                } else {
                    if let Some(Kind::Dropdown { open, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                        *open = true;
                    }
                    Some(Event::Repaint)
                }
            }
            Kind::HotkeyField { .. } => {
                self.focus = Some(id);
                if let Some(Kind::HotkeyField { capturing, error, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                    *capturing = true;
                    *error = None;
                }
                Some(Event::Repaint)
            }
            Kind::Button { .. } | Kind::Toggle { .. } | Kind::Link { .. } => {
                self.focus = Some(id);
                self.pressed = Some(id);
                Some(Event::Repaint)
            }
            _ => None,
        }
    }

    pub fn on_mouse_up(&mut self, x: f32, y: f32) -> Option<Event> {
        self.dragging_text = None;
        let pressed = self.pressed.take()?;
        if self.hit(x, y) != Some(pressed) {
            return Some(Event::Repaint);
        }
        self.activate(pressed)
    }

    fn activate(&mut self, id: WidgetId) -> Option<Event> {
        let w = self.get_mut(id)?;
        if !w.enabled {
            return None;
        }
        match &mut w.kind {
            Kind::Button { .. } | Kind::Link { .. } => Some(Event::Clicked(id)),
            Kind::Toggle { on } => {
                *on = !*on;
                Some(Event::Toggled(id, *on))
            }
            Kind::Dropdown { open, .. } => {
                *open = !*open;
                Some(Event::Repaint)
            }
            Kind::HotkeyField { capturing, error, .. } => {
                *capturing = true;
                *error = None;
                Some(Event::Repaint)
            }
            _ => None,
        }
    }

    fn focus_cycle(&mut self, delta: i32) {
        let ids: Vec<WidgetId> = self.widgets.iter().filter(|w| w.focusable()).map(|w| w.id).collect();
        if ids.is_empty() {
            self.focus = None;
            return;
        }
        let idx = self.focus.and_then(|f| ids.iter().position(|i| *i == f)).map(|p| p as i32).unwrap_or(-1);
        let next = if idx < 0 { if delta > 0 { 0 } else { ids.len() as i32 - 1 } } else { (idx + delta).rem_euclid(ids.len() as i32) };
        self.focus = Some(ids[next as usize]);
    }

    /// `vk` es un código de tecla virtual. Devuelve un evento si lo consumió.
    pub fn on_key(&mut self, vk: u16, shift: bool, ctrl: bool) -> Option<Event> {
        const TAB: u16 = 0x09;
        const ENTER: u16 = 0x0D;
        const ESC: u16 = 0x1B;
        const SPACE: u16 = 0x20;
        const LEFT: u16 = 0x25;
        const UP: u16 = 0x26;
        const RIGHT: u16 = 0x27;
        const DOWN: u16 = 0x28;
        const HOME: u16 = 0x24;
        const END: u16 = 0x23;
        const BACK: u16 = 0x08;
        const DEL: u16 = 0x2E;

        // Captura de atajo tiene prioridad absoluta.
        if let Some(id) = self.widgets.iter().find(|w| matches!(w.kind, Kind::HotkeyField { capturing: true, .. })).map(|w| w.id) {
            if matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5) {
                return Some(Event::Repaint);
            }
            if vk == ESC {
                if let Some(Kind::HotkeyField { capturing, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                    *capturing = false;
                }
                return Some(Event::Repaint);
            }
            if vk == BACK || vk == DEL {
                if let Some(Kind::HotkeyField { capturing, value, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                    *capturing = false;
                    *value = None;
                }
                return Some(Event::HotkeyChanged(id, None));
            }
            let mut mods = 0;
            if ctrl {
                mods |= crate::model::settings::MOD_CONTROL;
            }
            if shift {
                mods |= crate::model::settings::MOD_SHIFT;
            }
            if crate::ui::window::key_down(0x12) {
                mods |= crate::model::settings::MOD_ALT;
            }
            if crate::ui::window::key_down(0x5B) || crate::ui::window::key_down(0x5C) {
                mods |= crate::model::settings::MOD_WIN;
            }
            let hk = Hotkey::from_vk(mods, vk);
            if hk.is_some() {
                if let Some(Kind::HotkeyField { capturing, value, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                    *capturing = false;
                    *value = hk.clone();
                }
                return Some(Event::HotkeyChanged(id, hk));
            }
            return Some(Event::Repaint);
        }

        if vk == ESC {
            if let Some(id) = self.widgets.iter().find(|w| matches!(w.kind, Kind::Dropdown { open: true, .. })).map(|w| w.id) {
                if let Some(Kind::Dropdown { open, .. }) = self.get_mut(id).map(|w| &mut w.kind) {
                    *open = false;
                }
                return Some(Event::Repaint);
            }
            return Some(Event::Cancel);
        }
        if vk == TAB {
            self.focus_cycle(if shift { -1 } else { 1 });
            self.caret_visible = true;
            return Some(Event::Repaint);
        }
        let Some(fid) = self.focus else {
            return if vk == ENTER { Some(Event::Submit) } else { None };
        };
        let kind = self.get(fid)?.kind.clone();
        match kind {
            Kind::TextField(_) => {
                let mut ev = Event::Repaint;
                if let Some(Kind::TextField(t)) = self.get_mut(fid).map(|w| &mut w.kind) {
                    match vk {
                        LEFT => t.move_left(shift),
                        RIGHT => t.move_right(shift),
                        HOME => t.home(shift),
                        END => t.end(shift),
                        BACK => {
                            t.backspace();
                            ev = Event::TextChanged(fid);
                        }
                        DEL => {
                            t.delete();
                            ev = Event::TextChanged(fid);
                        }
                        ENTER => return Some(Event::Submit),
                        0x41 if ctrl => t.select_all(),
                        0x43 if ctrl => crate::ui::clipboard::set_text(&t.selected_text()),
                        0x58 if ctrl => {
                            crate::ui::clipboard::set_text(&t.selected_text());
                            t.delete_selection();
                            ev = Event::TextChanged(fid);
                        }
                        0x56 if ctrl => {
                            if let Some(s) = crate::ui::clipboard::get_text() {
                                t.insert_str(&s.replace(['\r', '\n'], " "));
                                ev = Event::TextChanged(fid);
                            }
                        }
                        _ => return None,
                    }
                }
                self.caret_visible = true;
                Some(ev)
            }
            Kind::Dropdown { options, selected, open } => match vk {
                UP | DOWN => {
                    let n = options.len();
                    if n == 0 {
                        return None;
                    }
                    let next = if vk == UP { (selected + n - 1) % n } else { (selected + 1) % n };
                    if let Some(Kind::Dropdown { selected, .. }) = self.get_mut(fid).map(|w| &mut w.kind) {
                        *selected = next;
                    }
                    Some(Event::Selected(fid, next))
                }
                ENTER | SPACE => {
                    if let Some(Kind::Dropdown { open: o, .. }) = self.get_mut(fid).map(|w| &mut w.kind) {
                        *o = !open;
                    }
                    Some(Event::Repaint)
                }
                _ => None,
            },
            Kind::Toggle { .. } | Kind::Button { .. } | Kind::Link { .. } | Kind::HotkeyField { .. } => match vk {
                SPACE => self.activate(fid),
                ENTER => {
                    if matches!(kind, Kind::Button { .. } | Kind::Link { .. }) { self.activate(fid) } else { Some(Event::Submit) }
                }
                _ => None,
            },
            _ => if vk == ENTER { Some(Event::Submit) } else { None },
        }
    }

    pub fn on_char(&mut self, ch: char) -> Option<Event> {
        let fid = self.focus?;
        if let Some(Kind::TextField(t)) = self.get_mut(fid).map(|w| &mut w.kind) {
            if ch.is_control() {
                return None;
            }
            t.insert_str(&ch.to_string());
            self.caret_visible = true;
            return Some(Event::TextChanged(fid));
        }
        None
    }

    pub fn has_text_focus(&self) -> bool {
        self.focus.and_then(|f| self.get(f)).map(|w| matches!(w.kind, Kind::TextField(_))).unwrap_or(false)
    }

    // ---- dibujo ----

    pub fn draw(&self, gfx: &Gfx, dc: &ID2D1DeviceContext, pal: &Palette) -> Result<()> {
        for w in self.widgets.iter().filter(|w| w.visible) {
            self.draw_widget(gfx, dc, pal, w)?;
        }
        // Desplegables abiertos encima de todo.
        for w in self.widgets.iter().filter(|w| w.visible) {
            if let (Kind::Dropdown { options, selected, open: true }, Some(pr)) = (&w.kind, self.dropdown_popup_rect(w)) {
                draw::fill_round_rect(dc, pr, 6.0, pal.window_solid)?;
                draw::fill_round_rect(dc, pr, 6.0, pal.card_bg)?;
                draw::stroke_round_rect(dc, pr, 6.0, pal.card_border, 1.0)?;
                for (i, o) in options.iter().enumerate() {
                    let ir = Rect::new(pr.x + 3.0, pr.y + 3.0 + 30.0 * i as f32, pr.w - 6.0, 30.0);
                    if i == *selected {
                        draw::fill_round_rect(dc, ir, 4.0, pal.cell_hover)?;
                        draw::fill_rect(dc, Rect::new(ir.x + 2.0, ir.y + 8.0, 3.0, 14.0), pal.accent)?;
                    }
                    let l = draw::text_layout(gfx, o, &TextStyle::normal(12.5), ir.w - 24.0, ir.h)?;
                    draw::draw_text(dc, &l, ir.x + 14.0, ir.y, pal.text)?;
                }
            }
        }
        Ok(())
    }

    fn draw_widget(&self, gfx: &Gfx, dc: &ID2D1DeviceContext, pal: &Palette, w: &Widget) -> Result<()> {
        let r = w.rect;
        let focused = self.focus == Some(w.id);
        let hovered = self.hover == Some(w.id) && w.enabled;
        let pressed = self.pressed == Some(w.id);
        let alpha = if w.enabled { 1.0 } else { 0.45 };
        let text_c = pal.text.with_alpha(alpha);
        let dim_c = pal.text_dim.with_alpha(alpha);
        match &w.kind {
            Kind::Header { title } => {
                let l = draw::text_layout(gfx, title, &TextStyle::bold(17.0), r.w - 40.0, r.h)?;
                draw::draw_text(dc, &l, r.x, r.y, pal.text)?;
                if let Some(cr) = self.close_rect() {
                    if self.close_hover {
                        draw::fill_round_rect(dc, cr, 6.0, pal.cell_hover)?;
                    }
                    draw::glyph(gfx, dc, Glyph::Close, cr.inset(7.0), pal.text_dim)?;
                }
            }
            Kind::SectionTitle { text } => {
                let l = draw::text_layout(gfx, text, &TextStyle::bold(12.0), r.w, r.h)?;
                draw::draw_text(dc, &l, r.x, r.y + 6.0, pal.text_dim)?;
            }
            Kind::Card { title, subtitle, glyph, .. } => {
                draw::fill_round_rect(dc, r, 6.0, pal.card_bg)?;
                draw::stroke_round_rect(dc, r, 6.0, pal.card_border, 1.0)?;
                let mut tx = r.x + 14.0;
                if let Some(g) = glyph {
                    draw::glyph(gfx, dc, *g, Rect::new(r.x + 14.0, r.y + (r.h - 18.0) / 2.0, 18.0, 18.0), pal.glyph.with_alpha(alpha))?;
                    tx += 30.0;
                }
                let child_w = match &w.kind {
                    Kind::Card { child: Some(c), .. } => self.get(*c).map(|cw| self.child_width(cw) + 28.0).unwrap_or(0.0),
                    _ => 0.0,
                };
                let tw = (r.w - (tx - r.x) - child_w).max(60.0);
                match subtitle {
                    Some(sub) => {
                        let l = draw::text_layout(gfx, title, &TextStyle::normal(13.0), tw, 20.0)?;
                        draw::draw_text(dc, &l, tx, r.y + 14.0, text_c)?;
                        let s = draw::text_layout(gfx, sub, &TextStyle::normal(11.5), tw, 18.0)?;
                        draw::draw_text(dc, &s, tx, r.y + 36.0, dim_c)?;
                    }
                    None => {
                        let l = draw::text_layout(gfx, title, &TextStyle::normal(13.0), tw, r.h)?;
                        draw::draw_text(dc, &l, tx, r.y, text_c)?;
                    }
                }
            }
            Kind::Label { text, dim, size, color } => {
                let l = draw::text_layout(gfx, text, &TextStyle::normal(*size), r.w, r.h)?;
                let c = color.unwrap_or(if *dim { pal.text_dim } else { pal.text }).with_alpha(alpha);
                draw::draw_text(dc, &l, r.x, r.y, c)?;
            }
            Kind::Button { text, accent, glyph, .. } => {
                let bg = if *accent { pal.accent } else { pal.button_bg };
                let bg = if pressed { bg.with_alpha(bg.a * 0.7) } else if hovered { if *accent { crate::theme::lighten(bg, 0.08) } else { pal.cell_hover } } else { bg };
                draw::fill_round_rect(dc, r, 5.0, bg.with_alpha(bg.a * alpha))?;
                if !*accent {
                    draw::stroke_round_rect(dc, r, 5.0, pal.button_border, 1.0)?;
                }
                let fg = if *accent { if pal.dark { Color::rgba8(0x0B, 0x0B, 0x0F, 1.0) } else { Color::white(1.0) } } else { pal.text };
                let mut tx = r.x;
                let mut tw = r.w;
                if let Some(g) = glyph {
                    draw::glyph(gfx, dc, *g, Rect::new(r.x + 10.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), fg.with_alpha(alpha))?;
                    tx += 22.0;
                    tw -= 22.0;
                }
                let l = draw::text_layout(gfx, text, &TextStyle::normal(12.5), tw, r.h)?;
                draw::set_text_align(&l, true);
                draw::draw_text(dc, &l, tx, r.y, fg.with_alpha(alpha))?;
                if focused {
                    draw::stroke_round_rect(dc, r.inset(-2.0), 7.0, pal.accent, 1.5)?;
                }
            }
            Kind::Toggle { on } => {
                if *on {
                    draw::fill_round_rect(dc, r, 10.0, pal.accent.with_alpha(alpha))?;
                    draw::fill_circle(dc, r.right() - 10.0, r.y + 10.0, 6.0, pal.toggle_knob_on)?;
                } else {
                    draw::stroke_round_rect(dc, r, 10.0, pal.toggle_off_border.with_alpha(alpha), 1.0)?;
                    draw::fill_circle(dc, r.x + 10.0, r.y + 10.0, 6.0, pal.toggle_knob_off.with_alpha(alpha))?;
                }
                if hovered && w.enabled {
                    draw::stroke_round_rect(dc, r, 10.0, pal.text.with_alpha(0.15), 1.0)?;
                }
                if focused {
                    draw::stroke_round_rect(dc, r.inset(-3.0), 13.0, pal.accent, 1.5)?;
                }
            }
            Kind::Dropdown { options, selected, .. } => {
                draw::fill_round_rect(dc, r, 5.0, if hovered { pal.cell_hover } else { pal.button_bg })?;
                draw::stroke_round_rect(dc, r, 5.0, pal.button_border, 1.0)?;
                let txt = options.get(*selected).cloned().unwrap_or_default();
                let l = draw::text_layout(gfx, &txt, &TextStyle::normal(12.5), r.w - 36.0, r.h)?;
                draw::draw_text(dc, &l, r.x + 10.0, r.y, text_c)?;
                draw::glyph(gfx, dc, Glyph::ChevronDown, Rect::new(r.right() - 24.0, r.y + (r.h - 16.0) / 2.0, 16.0, 16.0), dim_c)?;
                if focused {
                    draw::stroke_round_rect(dc, r.inset(-2.0), 7.0, pal.accent, 1.5)?;
                }
            }
            Kind::TextField(t) => {
                draw::fill_round_rect(dc, r, 5.0, pal.field_bg)?;
                draw::stroke_round_rect(dc, r, 5.0, pal.field_border, 1.0)?;
                let under = if focused { pal.accent } else { pal.text.with_alpha(0.35) };
                draw::fill_rect(dc, Rect::new(r.x + 5.0, r.bottom() - (if focused { 2.0 } else { 1.0 }), r.w - 10.0, if focused { 2.0 } else { 1.0 }), under)?;
                let style = TextStyle::normal(13.0);
                let inner = Rect::new(r.x + 10.0, r.y, r.w - 20.0, r.h);
                draw::push_clip(dc, inner);
                if t.text.is_empty() && !t.placeholder.is_empty() {
                    let l = draw::text_layout(gfx, &t.placeholder, &style, inner.w, inner.h)?;
                    draw::draw_text(dc, &l, inner.x, inner.y, pal.text_dim)?;
                }
                if let Some((s, e)) = t.selected_range() {
                    let x0 = prefix_width(gfx, &t.text, s, &style);
                    let x1 = prefix_width(gfx, &t.text, e, &style);
                    draw::fill_rect(dc, Rect::new(inner.x + x0, r.y + 7.0, x1 - x0, r.h - 14.0), pal.accent.with_alpha(0.35))?;
                }
                let l = draw::text_layout(gfx, &t.text, &style, 10_000.0, inner.h)?;
                draw::draw_text(dc, &l, inner.x, inner.y, text_c)?;
                if focused && self.caret_visible {
                    let cx = prefix_width(gfx, &t.text, t.caret, &style);
                    draw::fill_rect(dc, Rect::new(inner.x + cx, r.y + 8.0, 1.0, r.h - 16.0), pal.text)?;
                }
                draw::pop_clip(dc);
                if let Some(err) = &t.error {
                    let l = draw::text_layout(gfx, err, &TextStyle::normal(11.0), r.w, 16.0)?;
                    draw::draw_text(dc, &l, r.x + 2.0, r.bottom() + 2.0, pal.error)?;
                }
            }
            Kind::HotkeyField { value, capturing, error } => {
                let bg = if *capturing { pal.accent.with_alpha(0.18) } else if hovered { pal.cell_hover } else { pal.field_bg };
                draw::fill_round_rect(dc, r, 5.0, bg)?;
                draw::stroke_round_rect(dc, r, 5.0, if *capturing { pal.accent } else { pal.field_border }, 1.0)?;
                let txt = if *capturing {
                    crate::i18n::t("settings.hotkey.capture").to_string()
                } else {
                    value.as_ref().map(|h| h.to_display()).unwrap_or_else(|| crate::i18n::t("settings.hotkey.none").to_string())
                };
                let style = if value.is_some() && !*capturing { TextStyle::mono(12.0) } else { TextStyle::normal(12.0) };
                let l = draw::text_layout(gfx, &txt, &style, r.w - 16.0, r.h)?;
                draw::set_text_align(&l, true);
                draw::draw_text(dc, &l, r.x + 8.0, r.y, if value.is_some() || *capturing { text_c } else { dim_c })?;
                if let Some(err) = error {
                    let l = draw::text_layout(gfx, err, &TextStyle::normal(11.0), 260.0, 16.0)?;
                    draw::set_text_align(&l, true);
                    draw::draw_text(dc, &l, r.x + r.w - 260.0, r.bottom() + 2.0, pal.error)?;
                }
                if focused && !*capturing {
                    draw::stroke_round_rect(dc, r.inset(-2.0), 7.0, pal.accent, 1.5)?;
                }
            }
            Kind::Link { text } => {
                let l = draw::text_layout(gfx, text, &TextStyle::normal(12.0), r.w, r.h)?;
                let c = if hovered { pal.text } else { pal.accent };
                draw::draw_text(dc, &l, r.x, r.y, c)?;
                draw::fill_rect(dc, Rect::new(r.x, r.bottom() - 3.0, (text.chars().count() as f32 * 6.3).min(r.w), 1.0), c.with_alpha(0.6))?;
                if focused {
                    draw::stroke_round_rect(dc, r.inset(-2.0), 4.0, pal.accent, 1.0)?;
                }
            }
            Kind::IconPreview { bitmap, bitmap_px, glyph } => {
                draw::fill_round_rect(dc, r, 12.0, pal.card_bg)?;
                draw::stroke_round_rect(dc, r, 12.0, pal.card_border, 1.0)?;
                match bitmap {
                    Some(b) => draw::draw_bitmap(dc, b, *bitmap_px, r.inset(9.0), 9.0, 1.0)?,
                    None => draw::glyph(gfx, dc, *glyph, r.inset(16.0), pal.accent)?,
                }
            }
            Kind::Row { .. } | Kind::Spacer { .. } => {}
        }
        Ok(())
    }
}

/// Posición del cursor a partir de un desplazamiento horizontal, sin medir con DirectWrite:
/// aproximación con anchura media; se corrige con `prefix_width` al dibujar.
pub fn caret_from_x(text: &str, x: f32) -> usize {
    let n = text.chars().count();
    if n == 0 || x <= 0.0 {
        return 0;
    }
    let avg = 7.0;
    ((x / avg).round() as usize).min(n)
}

fn prefix_width(gfx: &Gfx, text: &str, chars: usize, style: &TextStyle) -> f32 {
    let prefix: String = text.chars().take(chars).collect();
    if prefix.is_empty() { 0.0 } else { draw::text_width(gfx, &prefix, style) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut f = TextState::new("ab");
        f.caret = 1;
        f.insert_str("X");
        assert_eq!(f.text, "aXb");
        assert_eq!(f.caret, 2);
        f.backspace();
        assert_eq!(f.text, "ab");
        assert_eq!(f.caret, 1);
    }

    #[test]
    fn selection_replace() {
        let mut f = TextState::new("hello");
        f.caret = 0;
        f.sel_anchor = Some(5);
        f.insert_str("y");
        assert_eq!(f.text, "y");
        assert!(f.sel_anchor.is_none());
        assert_eq!(f.caret, 1);
    }

    #[test]
    fn max_len_enforced() {
        let mut f = TextState::with_max("", 3);
        f.insert_str("abcd");
        assert_eq!(f.text, "abc");
    }

    #[test]
    fn unicode_boundaries() {
        let mut f = TextState::new("ñá");
        f.caret = 1;
        f.backspace();
        assert_eq!(f.text, "á");
        f.end(false);
        f.delete();
        assert_eq!(f.text, "á");
        f.home(true);
        assert_eq!(f.selected_text(), "á");
    }

    #[test]
    fn tab_cycles_focus_over_enabled_focusable() {
        let mut f = Form::new(400.0);
        let a = f.add(Kind::Button { text: "a".into(), accent: false, glyph: None, width: 80.0 });
        let b = f.add(Kind::Button { text: "b".into(), accent: false, glyph: None, width: 80.0 });
        let c = f.add(Kind::Toggle { on: false });
        f.set_enabled(b, false);
        f.on_key(0x09, false, false);
        assert_eq!(f.focus, Some(a));
        f.on_key(0x09, false, false);
        assert_eq!(f.focus, Some(c));
        f.on_key(0x09, false, false);
        assert_eq!(f.focus, Some(a));
        f.on_key(0x09, true, false);
        assert_eq!(f.focus, Some(c));
        assert_eq!(f.on_key(0x20, false, false), Some(Event::Toggled(c, true)));
    }

    #[test]
    fn layout_stacks_vertically_and_places_card_child() {
        let mut f = Form::new(500.0);
        f.add(Kind::Header { title: "T".into() });
        let t = f.add_child(Kind::Toggle { on: true });
        let card = f.add(Kind::Card { title: "x".into(), subtitle: None, glyph: None, child: Some(t) });
        f.layout();
        let cr = f.get(card).unwrap().rect;
        let tr = f.get(t).unwrap().rect;
        assert_eq!(cr.y, HEADER_H);
        assert!(tr.x > cr.x && tr.right() <= cr.right());
        assert!(f.content_h > HEADER_H + CARD_H);
        assert_eq!(f.hit(tr.center().0, tr.center().1), Some(t));
        assert_eq!(f.hit(cr.x + 5.0, cr.y + 5.0), Some(t));
    }

    #[test]
    fn hotkey_capture_flow() {
        let mut f = Form::new(400.0);
        let h = f.add(Kind::HotkeyField { value: None, capturing: false, error: None });
        f.layout();
        let r = f.get(h).unwrap().rect;
        assert_eq!(f.on_mouse_down(r.center().0, r.center().1), Some(Event::Repaint));
        assert_eq!(f.on_key(0x10, true, false), Some(Event::Repaint));
        match f.on_key(0x4B, false, true) {
            Some(Event::HotkeyChanged(id, Some(hk))) => {
                assert_eq!(id, h);
                assert_eq!(hk.key, "K");
                assert!(hk.mods.contains(&"ctrl".to_string()));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
