//! Orquestación: carga de datos, instancia única, barra, ventana de mensajes, residente,
//! ajustes y actualizaciones.

pub mod instance;
pub mod lifecycle;

use crate::gfx::Gfx;
use crate::i18n::Lang;
use crate::icons::cache::IconCache;
use crate::model::LoadOutcome;
use crate::model::entries::Entries;
use crate::model::legacy_import;
use crate::model::settings::{Hotkey, Settings};
use crate::theme;
use crate::ui::dialog::Dialog;
use crate::ui::settings::{self as settings_ui, Ids, UpdateState};
use crate::ui::toolbar::Toolbar;
use crate::ui::window::{self, Handler, WindowCtx, WindowHandler};
use crate::{fsx, i18n};
use lifecycle::Lifecycle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static PERF: OnceLock<Instant> = OnceLock::new();
static OPEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
static RESIDENT: AtomicBool = AtomicBool::new(false);
static BAR_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Con `--perf`, anota en el log el tiempo desde el arranque hasta esta fase.
pub fn perf_mark(label: &str) {
    if let Some(t) = PERF.get() {
        log_info!("perf {:>7.1} ms  {label}", t.elapsed().as_secs_f64() * 1000.0);
    }
}

pub fn window_opened() {
    OPEN_WINDOWS.fetch_add(1, Ordering::SeqCst);
}

pub fn window_closed() {
    OPEN_WINDOWS.fetch_sub(1, Ordering::SeqCst);
    maybe_exit();
}

pub fn set_resident(on: bool) {
    RESIDENT.store(on, Ordering::SeqCst);
}

pub fn set_bar_visible(on: bool) {
    BAR_VISIBLE.store(on, Ordering::SeqCst);
    if !on {
        maybe_exit();
    }
}

/// Sale del proceso cuando no es residente y no queda ninguna ventana abierta.
pub fn maybe_exit() {
    if !RESIDENT.load(Ordering::SeqCst) && !BAR_VISIBLE.load(Ordering::SeqCst) && OPEN_WINDOWS.load(Ordering::SeqCst) == 0 {
        window::post_quit();
    }
}

pub fn load_settings() -> Settings {
    let path = fsx::settings_path();
    match Settings::load(&path) {
        LoadOutcome::Loaded(s) => s,
        LoadOutcome::Missing => Settings::default(),
        LoadOutcome::Corrupt(e) => {
            log_warn!("settings.json corrupt ({e}); quarantined");
            let _ = fsx::quarantine_corrupt(&path);
            Settings::default()
        }
    }
}

pub fn load_entries() -> Entries {
    let path = fsx::entries_path();
    let legacy = fsx::legacy_dir();
    let import = |why: &str| -> Option<Entries> {
        if !legacy_import::should_import(&path, &legacy) {
            return None;
        }
        match legacy_import::run(&legacy, &fsx::icons_dir()) {
            Ok(es) => {
                log_info!("imported {} entries from BatLauncher ({why})", es.entries.len());
                if let Err(e) = es.save(&path) {
                    log_warn!("entries.json save failed after import: {e}");
                }
                Some(es)
            }
            Err(e) => {
                log_warn!("legacy import failed: {e}");
                None
            }
        }
    };
    match Entries::load(&path) {
        LoadOutcome::Loaded(es) => es,
        LoadOutcome::Missing => import("first run").unwrap_or_default(),
        LoadOutcome::Corrupt(e) => {
            log_warn!("entries.json corrupt ({e}); quarantined");
            let _ = fsx::quarantine_corrupt(&path);
            import("after corrupt file").unwrap_or_default()
        }
    }
}

// ---- estado global de la app (solo hilo de interfaz) ----

struct App {
    gfx: Rc<Gfx>,
    settings: Rc<RefCell<Settings>>,
    bar: Rc<RefCell<Toolbar>>,
    lifecycle: Lifecycle,
    msg_hwnd: HWND,
    settings_dlg: Option<(Rc<RefCell<Dialog>>, Rc<Ids>)>,
    update_state: UpdateState,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|a| a.try_borrow_mut().ok().and_then(|mut a| a.as_mut().map(f)))
}

pub fn with_settings(f: impl FnOnce(&mut Settings)) {
    let s = with_app(|a| a.settings.clone());
    if let Some(s) = s {
        f(&mut s.borrow_mut());
        save_settings();
    }
}

pub fn save_settings() {
    APP.with(|a| {
        if let Ok(a) = a.try_borrow() {
            if let Some(app) = a.as_ref() {
                if let Err(e) = app.settings.borrow().save(&fsx::settings_path()) {
                    log_warn!("settings.json save failed: {e}");
                }
            }
        }
    });
}

pub fn toggle_bar() {
    let bar = with_app(|a| a.bar.clone()).flatten_opt();
    if let Some(bar) = bar {
        if let Ok(mut b) = bar.try_borrow_mut() {
            b.toggle();
        }
    }
}

trait FlattenOpt<T> {
    fn flatten_opt(self) -> Option<T>;
}
impl<T> FlattenOpt<T> for Option<T> {
    fn flatten_opt(self) -> Option<T> {
        self
    }
}

pub fn apply_resident(on: bool, settings: &mut Settings) {
    with_app(|a| a.lifecycle.set_resident(on, settings));
}

pub fn apply_startup(on: bool) -> std::result::Result<(), String> {
    with_app(|a| a.lifecycle.set_startup(on)).unwrap_or(Err("app".into()))
}

pub fn apply_hotkey(hk: Option<&Hotkey>) -> std::result::Result<(), ()> {
    with_app(|a| a.lifecycle.set_hotkey(hk)).unwrap_or(Err(()))
}

pub fn update_state() -> UpdateState {
    with_app(|a| a.update_state.clone()).unwrap_or(UpdateState::Idle)
}

pub fn set_update_state(state: UpdateState) {
    let refresh = with_app(|a| {
        a.update_state = state.clone();
        let avail = settings_ui::available_version(&a.settings.borrow(), &a.update_state).is_some();
        if let Ok(mut b) = a.bar.try_borrow_mut() {
            b.set_update_dot(avail);
        }
        a.settings_dlg.clone().map(|d| (d, a.settings.clone()))
    })
    .flatten();
    if let Some(((dlg, ids), settings)) = refresh {
        settings_ui::refresh(&dlg, &ids, &settings.borrow(), &state);
    }
}

pub fn theme_changed() {
    let (bar, hwnd) = match with_app(|a| (a.bar.clone(), a.msg_hwnd)) {
        Some(v) => v,
        None => return,
    };
    let _ = hwnd;
    if let Ok(mut b) = bar.try_borrow_mut() {
        b.theme_changed();
    }
    let dlg = with_app(|a| a.settings_dlg.clone()).flatten();
    if let Some((dlg, _)) = dlg {
        if let Ok(mut d) = dlg.try_borrow_mut() {
            let s = with_app(|a| a.settings.borrow().theme).unwrap_or_default();
            d.set_palette(theme::palette(s));
        }
    }
}

pub fn language_changed(lang: Lang) {
    i18n::set_lang(lang);
    save_settings();
    let (bar, dlg) = match with_app(|a| (a.bar.clone(), a.settings_dlg.take())) {
        Some(v) => v,
        None => return,
    };
    if let Ok(mut b) = bar.try_borrow_mut() {
        b.theme_changed();
    }
    // La ventana de ajustes se reconstruye con los textos nuevos.
    if let Some((old, _)) = dlg {
        if let Ok(mut d) = old.try_borrow_mut() {
            d.close();
        }
    }
    open_settings();
}

pub fn open_settings() {
    let existing = with_app(|a| a.settings_dlg.clone()).flatten();
    if let Some((dlg, _)) = existing {
        if let Ok(d) = dlg.try_borrow() {
            if d.is_open() {
                d.activate();
                return;
            }
        }
    }
    let ctx = with_app(|a| (a.gfx.clone(), a.settings.clone(), a.update_state.clone(), theme::palette(a.settings.borrow().theme)));
    let Some((gfx, settings, state, pal)) = ctx else { return };
    match settings_ui::open(&gfx, &pal, settings, state) {
        Ok((dlg, ids)) => {
            with_app(|a| a.settings_dlg = Some((dlg, ids)));
        }
        Err(e) => log_warn!("settings window failed: {e}"),
    }
}

pub fn exit_app() {
    with_app(|a| a.lifecycle.set_resident(false, &mut Settings::default()));
    window::post_quit();
}

pub fn check_update_now() {
    let hwnd = with_app(|a| a.msg_hwnd);
    if let Some(h) = hwnd {
        crate::update::apply::spawn_check(h.0 as isize);
    }
}

pub fn apply_update() {
    let ctx = with_app(|a| (a.msg_hwnd, settings_ui::available_version(&a.settings.borrow(), &a.update_state)));
    if let Some((h, Some(v))) = ctx {
        crate::update::apply::spawn_apply(h.0 as isize, v);
    }
}

fn maybe_auto_check() {
    let ctx = with_app(|a| {
        let s = a.settings.borrow();
        (a.msg_hwnd, s.update_check && crate::update::version::should_check(s.last_update_check.as_deref(), &fsx::now_iso()))
    });
    if let Some((h, true)) = ctx {
        unsafe {
            SetTimer(Some(h), window::TIMER_UPDATE_CHECK, 1000, None);
        }
    }
}

/// Manejador de la ventana de mensajes: alternar, atajo, bandeja, tema y actualizaciones.
struct MessageHandler;

impl WindowHandler for MessageHandler {
    fn on_message(&mut self, ctx: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        if msg == instance::toggle_msg() {
            toggle_bar();
            return Some(LRESULT(0));
        }
        match msg {
            WM_HOTKEY => {
                toggle_bar();
                Some(LRESULT(0))
            }
            window::WM_TRAY => {
                let event = (lparam.0 & 0xFFFF) as u32;
                let (x, y) = ((wparam.0 & 0xFFFF) as u16 as i16 as i32, ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32);
                match event {
                    WM_LBUTTONUP => toggle_bar(),
                    WM_CONTEXTMENU | WM_RBUTTONUP => tray_menu(x, y),
                    _ => {}
                }
                Some(LRESULT(0))
            }
            WM_SETTINGCHANGE | WM_DWMCOLORIZATIONCOLORCHANGED | WM_THEMECHANGED => {
                theme_changed();
                None
            }
            WM_TIMER if wparam.0 == window::TIMER_UPDATE_CHECK => {
                unsafe {
                    let _ = KillTimer(Some(ctx.hwnd), window::TIMER_UPDATE_CHECK);
                }
                check_update_now();
                Some(LRESULT(0))
            }
            window::WM_UPDATE_EVENT => {
                let ev = unsafe { Box::from_raw(lparam.0 as *mut crate::update::apply::UpdateEvent) };
                crate::update::apply::on_event(*ev);
                Some(LRESULT(0))
            }
            _ => None,
        }
    }
}

fn tray_menu(x: i32, y: i32) {
    use crate::gfx::draw::Glyph;
    use crate::ui::menu::{Menu, MenuItem};
    let ctx = with_app(|a| (a.gfx.clone(), theme::palette(a.settings.borrow().theme), a.settings.borrow().animations, a.msg_hwnd));
    let Some((gfx, pal, animations, hwnd)) = ctx else { return };
    unsafe {
        let _ = SetForegroundWindow(hwnd);
    }
    let items = vec![
        MenuItem::new(1, i18n::t("tray.open"), Some(Glyph::Play)),
        MenuItem::new(2, i18n::t("menu.settings"), Some(Glyph::Gear)),
        MenuItem::new(3, i18n::t("menu.exit"), Some(Glyph::Close)).separated(),
    ];
    let animate = theme::animations_enabled(animations);
    let _ = Menu::open(&gfx, &pal, items, x, y - 8, animate, Box::new(|pick| match pick {
        Some(1) => toggle_bar(),
        Some(2) => open_settings(),
        Some(3) => exit_app(),
        _ => {}
    }));
}

pub fn run(args: Vec<String>) -> i32 {
    let start = Instant::now();
    crate::log::init();
    crate::gfx::device::co_init();
    let perf = args.iter().any(|a| a == "--perf");
    if perf {
        let _ = PERF.set(start);
    }
    let background = args.iter().any(|a| a == "--background");
    let updated_old = args.iter().position(|a| a == "--updated").and_then(|i| args.get(i + 1).cloned());
    perf_mark("main");

    // Instancia única: la segunda solo avisa a la primera. Tras una actualización, el proceso
    // nuevo arranca mientras el viejo aún está saliendo: se le da hasta 5 s para soltar el mutex.
    let mut attempts = if updated_old.is_some() { 50 } else { 1 };
    let _mutex = loop {
        match instance::acquire() {
            instance::Instance::Primary(h) => break h,
            instance::Instance::Secondary => {
                attempts -= 1;
                if attempts > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                let ok = instance::signal_toggle();
                perf_mark(if ok { "toggle sent" } else { "toggle failed" });
                return 0;
            }
        }
    };

    let gfx = Rc::new(Gfx::start());
    let settings = load_settings();
    i18n::set_lang(settings.effective_language());
    let entries = load_entries();
    fsx::clear_dir(&fsx::update_dir());
    if let Some(old) = &updated_old {
        crate::update::apply::cleanup_old(std::path::Path::new(old));
    }
    perf_mark("data loaded");

    let cache = IconCache::load(&fsx::cache_path());
    let settings = Rc::new(RefCell::new(settings));
    let entries = Rc::new(RefCell::new(entries));

    let msg_handler: Handler = Rc::new(RefCell::new(MessageHandler));
    let msg_hwnd = match instance::create_message_window(msg_handler) {
        Ok(h) => h,
        Err(e) => {
            log_error!("message window failed: {e}");
            HWND::default()
        }
    };
    let mut lifecycle = Lifecycle::new(msg_hwnd);
    let resident = settings.borrow().resident;
    if background && !resident {
        let _ = lifecycle.set_startup(false);
        log_info!("--background without resident mode: exiting");
        return 0;
    }
    if resident {
        let mut s = settings.borrow_mut();
        lifecycle.set_resident(true, &mut s);
        lifecycle.refresh_startup_path(&s);
    }

    let bar = Toolbar::new(gfx.clone(), entries, settings.clone(), cache, Box::new(|| set_bar_visible(false)), perf.then_some(start));
    APP.with(|a| {
        *a.borrow_mut() = Some(App { gfx, settings, bar: bar.clone(), lifecycle, msg_hwnd, settings_dlg: None, update_state: UpdateState::Idle });
    });

    if updated_old.is_some() {
        set_update_state(UpdateState::Updated(crate::update::version::Version::current()));
        open_settings();
    } else if !background {
        bar.borrow_mut().show();
    }
    maybe_auto_check();
    let code = window::run_message_loop();
    APP.with(|a| *a.borrow_mut() = None);
    log_info!("exit {code}");
    code
}
