//! Ventana de ajustes: tarjetas estilo Ajustes de Windows 11, cambios aplicados al instante.

use crate::gfx::Gfx;
use crate::gfx::draw::Glyph;
use crate::i18n::{t, tf};
use crate::model::settings::{LanguageSetting, Settings, ThemeSetting};
use crate::theme::Palette;
use crate::ui::dialog::{Dialog, DialogAction};
use crate::ui::widgets::{Event, Form, Kind, WidgetId};
use crate::update::version::Version;
use std::cell::RefCell;
use std::rc::Rc;
use windows::core::Result;

pub const WIDTH: f32 = 520.0;

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateState {
    Idle,
    Checking,
    Available(Version),
    UpToDate,
    Offline,
    Error(String),
    Downloading(u8),
    VerifyFailed,
    NotWritable,
    Updated(Version),
    Restarting,
}

pub struct Ids {
    pub language: WidgetId,
    pub resident: WidgetId,
    pub startup: WidgetId,
    pub startup_card: WidgetId,
    pub hotkey: WidgetId,
    pub hotkey_card: WidgetId,
    pub theme: WidgetId,
    pub animations: WidgetId,
    pub update_check: WidgetId,
    pub version_card: WidgetId,
    pub changes_link: WidgetId,
    pub check_now: WidgetId,
    pub apply_update: WidgetId,
    pub open_folder: WidgetId,
    pub repo_link: WidgetId,
}

fn hours_text(settings: &Settings) -> String {
    match &settings.last_update_check {
        Some(last) => match crate::update::version::hours_since(last, &crate::fsx::now_iso()) {
            Some(h) if h >= 1 => tf("settings.update.last", &[&tf("time.hours", &[&h.to_string()])]),
            Some(_) => tf("settings.update.last", &[t("time.moments")]),
            None => t("settings.update.never").to_string(),
        },
        None => t("settings.update.never").to_string(),
    }
}

pub fn status_text(settings: &Settings, state: &UpdateState) -> String {
    match state {
        UpdateState::Idle => match settings.known_latest.as_deref().and_then(Version::parse) {
            Some(v) if v > Version::current() => tf("settings.update.available", &[&v.to_string()]),
            _ => hours_text(settings),
        },
        UpdateState::Checking => t("settings.update.checking").to_string(),
        UpdateState::Available(v) => tf("settings.update.available", &[&v.to_string()]),
        UpdateState::UpToDate => t("settings.update.uptodate").to_string(),
        UpdateState::Offline => t("settings.update.offline").to_string(),
        UpdateState::Error(e) => tf("settings.update.error", &[e]),
        UpdateState::Downloading(p) => tf("settings.update.downloading", &[&p.to_string()]),
        UpdateState::VerifyFailed => t("settings.update.verify_failed").to_string(),
        UpdateState::NotWritable => t("settings.update.not_writable").to_string(),
        UpdateState::Updated(v) => tf("settings.update.updated", &[&v.to_string()]),
        UpdateState::Restarting => t("settings.update.restarting").to_string(),
    }
}

pub fn available_version(settings: &Settings, state: &UpdateState) -> Option<Version> {
    match state {
        UpdateState::Available(v) => Some(*v),
        UpdateState::Idle => settings.known_latest.as_deref().and_then(Version::parse).filter(|v| *v > Version::current()),
        _ => None,
    }
}

fn build(settings: &Settings, state: &UpdateState) -> (Form, Ids) {
    let mut f = Form::new(WIDTH);
    f.add(Kind::Header { title: t("settings.title").to_string() });

    f.add(Kind::SectionTitle { text: t("settings.general").to_string() });
    let language = f.add_child(Kind::Dropdown {
        options: vec![t("settings.language.system").to_string(), t("settings.language.es").to_string(), t("settings.language.en").to_string()],
        selected: match settings.language {
            LanguageSetting::System => 0,
            LanguageSetting::Es => 1,
            LanguageSetting::En => 2,
        },
        open: false,
    });
    f.add(Kind::Card { title: t("settings.language").to_string(), subtitle: None, glyph: Some(Glyph::Globe), child: Some(language) });
    let resident = f.add_child(Kind::Toggle { on: settings.resident });
    f.add(Kind::Card { title: t("settings.resident").to_string(), subtitle: Some(t("settings.resident.hint").to_string()), glyph: Some(Glyph::Monitor), child: Some(resident) });
    let startup = f.add_child(Kind::Toggle { on: settings.start_with_windows });
    let startup_card = f.add(Kind::Card { title: t("settings.startup").to_string(), subtitle: Some(t("settings.startup.hint").to_string()), glyph: Some(Glyph::Play), child: Some(startup) });
    let hotkey = f.add_child(Kind::HotkeyField { value: settings.hotkey.clone(), capturing: false, error: None });
    let hotkey_card = f.add(Kind::Card { title: t("settings.hotkey").to_string(), subtitle: Some(t("settings.hotkey.hint").to_string()), glyph: Some(Glyph::Keyboard), child: Some(hotkey) });

    f.add(Kind::SectionTitle { text: t("settings.appearance").to_string() });
    let theme = f.add_child(Kind::Dropdown {
        options: vec![t("settings.theme.system").to_string(), t("settings.theme.light").to_string(), t("settings.theme.dark").to_string()],
        selected: match settings.theme {
            ThemeSetting::System => 0,
            ThemeSetting::Light => 1,
            ThemeSetting::Dark => 2,
        },
        open: false,
    });
    f.add(Kind::Card { title: t("settings.theme").to_string(), subtitle: None, glyph: Some(Glyph::Theme), child: Some(theme) });
    let animations = f.add_child(Kind::Toggle { on: settings.animations });
    f.add(Kind::Card { title: t("settings.animations").to_string(), subtitle: Some(t("settings.animations.hint").to_string()), glyph: Some(Glyph::Bolt), child: Some(animations) });

    f.add(Kind::SectionTitle { text: t("settings.updates").to_string() });
    let update_check = f.add_child(Kind::Toggle { on: settings.update_check });
    f.add(Kind::Card { title: t("settings.update_check").to_string(), subtitle: Some(t("settings.update_check.hint").to_string()), glyph: Some(Glyph::Refresh), child: Some(update_check) });
    let changes_link = f.add_child(Kind::Link { text: t("settings.update.changes").to_string() });
    let check_now = f.add_child(Kind::Button { text: t("settings.update.now").to_string(), accent: false, glyph: None, width: 110.0 });
    let apply_update = f.add_child(Kind::Button { text: t("settings.update.apply").to_string(), accent: true, glyph: None, width: 100.0 });
    let actions = f.add_child(Kind::Row { children: vec![changes_link, check_now, apply_update], align_right: true });
    let version_card = f.add(Kind::Card { title: tf("settings.version", &[&Version::current().to_string()]), subtitle: Some(status_text(settings, state)), glyph: Some(Glyph::Download), child: Some(actions) });

    f.add(Kind::SectionTitle { text: t("settings.data").to_string() });
    let open_folder = f.add_child(Kind::Button { text: t("settings.open_folder").to_string(), accent: false, glyph: None, width: 120.0 });
    f.add(Kind::Card { title: t("settings.data_folder").to_string(), subtitle: Some(crate::fsx::data_dir().to_string_lossy().to_string()), glyph: Some(Glyph::Folder), child: Some(open_folder) });

    f.add(Kind::Spacer { h: 6.0 });
    let footer = f.add_child(Kind::Label { text: tf("settings.footer", &[&Version::current().to_string()]), dim: true, size: 11.5, color: None });
    let repo_link = f.add_child(Kind::Link { text: format!("github.com/{}/{}", crate::update::version::OWNER, crate::update::version::REPO) });
    f.add(Kind::Row { children: vec![footer, repo_link], align_right: false });

    let ids = Ids { language, resident, startup, startup_card, hotkey, hotkey_card, theme, animations, update_check, version_card, changes_link, check_now, apply_update, open_folder, repo_link };
    sync_enabled(&mut f, &ids, settings, state);
    (f, ids)
}

fn sync_enabled(f: &mut Form, ids: &Ids, settings: &Settings, state: &UpdateState) {
    f.set_enabled(ids.startup, settings.resident);
    f.set_enabled(ids.hotkey, settings.resident);
    let avail = available_version(settings, state).is_some();
    f.set_visible(ids.apply_update, avail);
    f.set_visible(ids.changes_link, avail);
    let busy = matches!(state, UpdateState::Checking | UpdateState::Downloading(_) | UpdateState::Restarting);
    f.set_enabled(ids.check_now, !busy);
    f.set_enabled(ids.apply_update, !busy);
    f.set_label(ids.version_card, &status_text(settings, state));
}

/// Abre la ventana de ajustes. Devuelve el diálogo para que `app` pueda refrescar el estado.
pub fn open(gfx: &Rc<Gfx>, pal: &Palette, settings: Rc<RefCell<Settings>>, state: UpdateState) -> Result<(Rc<RefCell<Dialog>>, Rc<Ids>)> {
    let (form, ids) = build(&settings.borrow(), &state);
    let ids = Rc::new(ids);
    let ids2 = ids.clone();
    let settings2 = settings.clone();
    let dlg = Dialog::open(gfx, pal, t("settings.title"), WIDTH, form, Box::new(move |form, ev| {
        let ids = &ids2;
        let mut s = settings2.borrow_mut();
        let mut changed = true;
        let mut relayout = false;
        match ev {
            Event::Selected(id, i) if id == ids.language => {
                s.language = match i {
                    1 => LanguageSetting::Es,
                    2 => LanguageSetting::En,
                    _ => LanguageSetting::System,
                };
                let lang = s.effective_language();
                drop(s);
                crate::app::language_changed(lang);
                return DialogAction::None;
            }
            Event::Toggled(id, on) if id == ids.resident => {
                s.resident = on;
                crate::app::apply_resident(on, &mut s);
                form.set_toggle(ids.startup, s.start_with_windows);
                form.set_hotkey(ids.hotkey, s.hotkey.clone(), None);
            }
            Event::Toggled(id, on) if id == ids.startup => match crate::app::apply_startup(on) {
                Ok(()) => {
                    s.start_with_windows = on;
                    form.set_label(ids.startup_card, t("settings.startup.hint"));
                }
                Err(e) => {
                    form.set_toggle(ids.startup, false);
                    s.start_with_windows = false;
                    form.set_label(ids.startup_card, &format!("{} ({e})", t("settings.startup.error")));
                }
            },
            Event::HotkeyChanged(id, hk) if id == ids.hotkey => match crate::app::apply_hotkey(hk.as_ref()) {
                Ok(()) => {
                    s.hotkey = hk;
                    form.set_hotkey(ids.hotkey, s.hotkey.clone(), None);
                }
                Err(()) => {
                    form.set_hotkey(ids.hotkey, s.hotkey.clone(), Some(t("settings.hotkey.conflict").to_string()));
                    changed = false;
                }
            },
            Event::Selected(id, i) if id == ids.theme => {
                s.theme = match i {
                    1 => ThemeSetting::Light,
                    2 => ThemeSetting::Dark,
                    _ => ThemeSetting::System,
                };
                drop(s);
                crate::app::save_settings();
                crate::app::theme_changed();
                return DialogAction::Repaint;
            }
            Event::Toggled(id, on) if id == ids.animations => s.animations = on,
            Event::Toggled(id, on) if id == ids.update_check => s.update_check = on,
            Event::Clicked(id) if id == ids.check_now => {
                changed = false;
                drop(s);
                crate::app::check_update_now();
                return DialogAction::Repaint;
            }
            Event::Clicked(id) if id == ids.apply_update => {
                changed = false;
                drop(s);
                crate::app::apply_update();
                return DialogAction::Repaint;
            }
            Event::Clicked(id) if id == ids.changes_link => {
                changed = false;
                if let Some(v) = available_version(&s, &crate::app::update_state()) {
                    crate::launch::open_url(&crate::update::version::release_page_url(v));
                }
            }
            Event::Clicked(id) if id == ids.open_folder => {
                changed = false;
                crate::launch::open_folder(&crate::fsx::data_dir());
            }
            Event::Clicked(id) if id == ids.repo_link => {
                changed = false;
                crate::launch::open_url(&format!("https://github.com/{}/{}", crate::update::version::OWNER, crate::update::version::REPO));
            }
            Event::Cancel | Event::Close => return DialogAction::Close,
            Event::Submit => {
                changed = false;
            }
            _ => {
                changed = false;
            }
        }
        if changed {
            let state = crate::app::update_state();
            sync_enabled(form, ids, &s, &state);
            drop(s);
            crate::app::save_settings();
            relayout = true;
        }
        if relayout { DialogAction::Relayout } else { DialogAction::Repaint }
    }))?;
    Ok((dlg, ids))
}

/// Refresca el estado de actualización y los controles dependientes en una ventana abierta.
pub fn refresh(dlg: &Rc<RefCell<Dialog>>, ids: &Ids, settings: &Settings, state: &UpdateState) {
    if let Ok(mut d) = dlg.try_borrow_mut() {
        sync_enabled(&mut d.form, ids, settings, state);
        let _ = d.relayout();
    }
}
