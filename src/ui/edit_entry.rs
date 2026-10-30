//! Diálogo de nueva entrada y editar: nombre, archivo e icono.

use crate::gfx::Gfx;
use crate::gfx::draw::Glyph;
use crate::i18n::t;
use crate::icons::extract;
use crate::model::entries::{self, Entry, IconSource};
use crate::theme::Palette;
use crate::ui::dialog::{Dialog, DialogAction};
use crate::ui::widgets::{Event, Form, Kind, TextState};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use windows::Win32::Foundation::HWND;
use windows::core::Result;

#[derive(Clone, Debug)]
pub struct EditResult {
    pub name: String,
    pub path: PathBuf,
    /// Icono resultante: `Custom` con `file` vacío significa "copiar desde `custom_src`".
    pub icon: IconSource,
    pub custom_src: Option<PathBuf>,
}

const PREVIEW_PX: f32 = 46.0;

pub fn is_image(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "ico" | "bmp" | "gif"))
}

fn preview_px(scale: f32) -> u32 {
    (PREVIEW_PX * scale).round() as u32
}

struct State {
    gfx: Rc<Gfx>,
    entry: Entry,
    icon_src: Option<PathBuf>,
    icon_custom_existing: Option<PathBuf>,
    use_file_icon: bool,
    scale: f32,
    hwnd: HWND,
}

impl State {
    fn current_icon_source(&self) -> IconSource {
        if let Some(_) = &self.icon_src {
            return IconSource::Custom { file: String::new() };
        }
        if self.icon_custom_existing.is_some() {
            return self.entry.icon.clone();
        }
        if self.use_file_icon { IconSource::File } else { IconSource::None }
    }

    fn preview(&self) -> Option<Vec<u8>> {
        let px = preview_px(self.scale);
        let wic = self.gfx.wic().ok()?;
        if let Some(src) = &self.icon_src {
            return if is_image(src) { extract::load_image(wic, src, px) } else { extract::extract_file_icon(src, px) };
        }
        if let Some(existing) = &self.icon_custom_existing {
            return extract::load_image(wic, existing, px);
        }
        if self.use_file_icon && self.entry.path.exists() {
            return extract::extract_file_icon(&self.entry.path, px);
        }
        None
    }
}

/// Abre el diálogo. `existing` = editar; `None` = nueva entrada. `on_done` recibe el resultado o `None` si se cancela.
pub fn open(gfx: &Rc<Gfx>, pal: &Palette, existing: Option<Entry>, on_done: Box<dyn FnOnce(Option<EditResult>)>) -> Result<()> {
    let is_new = existing.is_none();
    let entry = existing.unwrap_or_else(|| Entry { id: entries::new_id(), name: String::new(), path: PathBuf::new(), icon: IconSource::None, created_at: crate::fsx::now_iso(), extra: Default::default() });
    let title = if is_new { t("dialog.new_entry") } else { t("dialog.edit_entry") };
    let icon_custom_existing = match &entry.icon {
        IconSource::Custom { file } => Some(crate::fsx::icons_dir().join(file)),
        _ => None,
    };
    let use_file_icon = matches!(entry.icon, IconSource::File);
    let mon = crate::ui::window::cursor_monitor();
    let state = Rc::new(RefCell::new(State { gfx: gfx.clone(), entry: entry.clone(), icon_src: None, icon_custom_existing, use_file_icon, scale: mon.scale, hwnd: HWND::default() }));

    let mut form = Form::new(440.0);
    form.add(Kind::Header { title: title.to_string() });
    let preview = form.add_child(Kind::IconPreview { bitmap: None, bitmap_px: preview_px(mon.scale), glyph: Glyph::Terminal });
    let change = form.add_child(Kind::Button { text: t("dialog.change_icon").to_string(), accent: false, glyph: None, width: 118.0 });
    let use_file = form.add_child(Kind::Button { text: t("dialog.use_file_icon").to_string(), accent: false, glyph: None, width: 128.0 });
    let remove = form.add_child(Kind::Button { text: t("dialog.remove_icon").to_string(), accent: false, glyph: None, width: 70.0 });
    form.add(Kind::Row { children: vec![preview, change, use_file, remove], align_right: false });
    form.add(Kind::Spacer { h: 28.0 });
    form.add(Kind::Label { text: t("dialog.icon_hint").to_string(), dim: true, size: 11.0, color: None });
    form.add(Kind::Label { text: t("dialog.name").to_string(), dim: true, size: 12.0, color: None });
    let name = form.add(Kind::TextField(TextState::with_max(&entry.name, 64)));
    form.add(Kind::Label { text: t("dialog.file").to_string(), dim: true, size: 12.0, color: None });
    let mut path_state = TextState::new(&entry.path.to_string_lossy());
    path_state.max_len = 1024;
    let path = form.add_child(Kind::TextField(path_state));
    let browse = form.add_child(Kind::Button { text: t("dialog.browse").to_string(), accent: false, glyph: None, width: 100.0 });
    form.add(Kind::Row { children: vec![path, browse], align_right: false });
    form.add(Kind::Spacer { h: 10.0 });
    let cancel = form.add_child(Kind::Button { text: t("dialog.cancel").to_string(), accent: false, glyph: None, width: 96.0 });
    let save = form.add_child(Kind::Button { text: t("dialog.save").to_string(), accent: true, glyph: None, width: 96.0 });
    form.add(Kind::Row { children: vec![cancel, save], align_right: true });
    form.focus = Some(name);

    let st = state.clone();
    let gfx2 = gfx.clone();
    let mut done = Some(on_done);
    let refresh = move |form: &mut Form, st: &State| {
        let px = preview_px(st.scale);
        let bmp = st.preview().and_then(|b| gfx2.bitmap_from_bgra(px, &b).ok());
        form.set_icon_preview(preview, bmp, px);
        let p = PathBuf::from(form.text(path).trim());
        let exe_like = matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("exe" | "lnk"));
        form.set_enabled(use_file, exe_like);
        form.set_enabled(remove, st.icon_src.is_some() || st.icon_custom_existing.is_some() || st.use_file_icon);
        let missing = !p.as_os_str().is_empty() && !p.exists();
        form.set_field_error(path, if missing { Some(t("dialog.file_missing").to_string()) } else { None });
        let valid = !form.text(name).trim().is_empty() && !p.as_os_str().is_empty();
        form.set_enabled(save, valid);
    };
    let refresh = Rc::new(refresh);
    let refresh2 = refresh.clone();

    let dlg = Dialog::open(gfx, pal, title, 440.0, form, Box::new(move |form, ev| {
        let mut s = st.borrow_mut();
        match ev {
            Event::Clicked(id) if id == browse => {
                let filters = [(t("picker.files"), "*.bat;*.cmd;*.exe;*.lnk"), (t("picker.all"), "*.*")];
                if let Some(p) = crate::launch::pick_file(s.hwnd, &filters) {
                    let prev_default = entries::default_name(&s.entry.path);
                    let current_name = form.text(name);
                    s.entry.path = p.clone();
                    form.set_text(path, &p.to_string_lossy());
                    if current_name.trim().is_empty() || current_name == prev_default {
                        form.set_text(name, &entries::default_name(&p));
                    }
                    if s.icon_src.is_none() && s.icon_custom_existing.is_none() {
                        s.use_file_icon = matches!(entries::default_icon(&p), IconSource::File);
                    }
                }
                refresh2(form, &s);
                DialogAction::Relayout
            }
            Event::Clicked(id) if id == change => {
                let filters = [(t("picker.images"), "*.png;*.jpg;*.jpeg;*.ico;*.bmp;*.gif;*.exe;*.dll"), (t("picker.all"), "*.*")];
                if let Some(p) = crate::launch::pick_file(s.hwnd, &filters) {
                    s.icon_src = Some(p);
                    s.icon_custom_existing = None;
                    s.use_file_icon = false;
                }
                refresh2(form, &s);
                DialogAction::Repaint
            }
            Event::Clicked(id) if id == use_file => {
                s.icon_src = None;
                s.icon_custom_existing = None;
                s.use_file_icon = true;
                s.entry.path = PathBuf::from(form.text(path).trim());
                refresh2(form, &s);
                DialogAction::Repaint
            }
            Event::Clicked(id) if id == remove => {
                s.icon_src = None;
                s.icon_custom_existing = None;
                s.use_file_icon = false;
                refresh2(form, &s);
                DialogAction::Repaint
            }
            Event::TextChanged(id) => {
                if id == path {
                    s.entry.path = PathBuf::from(form.text(path).trim());
                }
                refresh2(form, &s);
                DialogAction::Relayout
            }
            Event::Clicked(id) if id == save => finish(form, &s, &mut done, name, path),
            Event::Submit => finish(form, &s, &mut done, name, path),
            Event::Clicked(id) if id == cancel => {
                if let Some(f) = done.take() {
                    f(None);
                }
                DialogAction::Close
            }
            Event::Cancel | Event::Close => {
                if let Some(f) = done.take() {
                    f(None);
                }
                DialogAction::Close
            }
            _ => DialogAction::Repaint,
        }
    }))?;
    {
        let hwnd = dlg.borrow().hwnd();
        state.borrow_mut().hwnd = hwnd;
        let mut d = dlg.borrow_mut();
        let s = state.borrow();
        refresh(&mut d.form, &s);
        drop(s);
        let _ = d.relayout();
    }
    Ok(())
}

fn finish(form: &mut Form, s: &State, done: &mut Option<Box<dyn FnOnce(Option<EditResult>)>>, name: u32, path: u32) -> DialogAction {
    let n = form.text(name).trim().to_string();
    let p = PathBuf::from(form.text(path).trim());
    if n.is_empty() || p.as_os_str().is_empty() {
        return DialogAction::Repaint;
    }
    let result = EditResult { name: n, path: p, icon: s.current_icon_source(), custom_src: s.icon_src.clone() };
    if let Some(f) = done.take() {
        f(Some(result));
    }
    DialogAction::Close
}
