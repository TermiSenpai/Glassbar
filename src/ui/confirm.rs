//! Diálogo de confirmación de cristal.

use crate::gfx::Gfx;
use crate::theme::Palette;
use crate::ui::dialog::{Dialog, DialogAction};
use crate::ui::widgets::{Event, Form, Kind};
use std::rc::Rc;
use windows::core::Result;

pub fn open(gfx: &Rc<Gfx>, pal: &Palette, title: &str, text: &str, confirm_label: &str, on_done: Box<dyn FnOnce(bool)>) -> Result<()> {
    let mut form = Form::new(360.0);
    form.add(Kind::Header { title: title.to_string() });
    form.add(Kind::Label { text: text.to_string(), dim: false, size: 13.0, color: None });
    form.add(Kind::Spacer { h: 6.0 });
    let cancel = form.add_child(Kind::Button { text: crate::i18n::t("dialog.cancel").to_string(), accent: false, glyph: None, width: 96.0 });
    let ok = form.add_child(Kind::Button { text: confirm_label.to_string(), accent: true, glyph: None, width: 96.0 });
    form.add(Kind::Row { children: vec![cancel, ok], align_right: true });
    form.focus = Some(ok);
    let mut done = Some(on_done);
    let _ = Dialog::open(gfx, pal, title, 360.0, form, Box::new(move |_form, ev| match ev {
        Event::Clicked(id) if id == ok => {
            if let Some(f) = done.take() {
                f(true);
            }
            DialogAction::Close
        }
        Event::Submit => {
            if let Some(f) = done.take() {
                f(true);
            }
            DialogAction::Close
        }
        Event::Clicked(id) if id == cancel => {
            if let Some(f) = done.take() {
                f(false);
            }
            DialogAction::Close
        }
        Event::Cancel | Event::Close => {
            if let Some(f) = done.take() {
                f(false);
            }
            DialogAction::Close
        }
        _ => DialogAction::Repaint,
    }))?;
    Ok(())
}
