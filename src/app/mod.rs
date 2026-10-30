//! Orquestación: carga de datos, instancia, barra y bucle de mensajes.

use crate::gfx::Gfx;
use crate::icons::cache::IconCache;
use crate::model::LoadOutcome;
use crate::model::entries::Entries;
use crate::model::legacy_import;
use crate::model::settings::Settings;
use crate::ui::toolbar::Toolbar;
use crate::ui::window;
use crate::{fsx, i18n};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Instant;

static PERF: OnceLock<Instant> = OnceLock::new();

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static OPEN_WINDOWS: AtomicUsize = AtomicUsize::new(0);
static RESIDENT: AtomicBool = AtomicBool::new(false);
static BAR_VISIBLE: AtomicBool = AtomicBool::new(false);

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

/// Con `--perf`, anota en el log el tiempo desde el arranque hasta esta fase.
pub fn perf_mark(label: &str) {
    if let Some(t) = PERF.get() {
        log_info!("perf {:>7.1} ms  {label}", t.elapsed().as_secs_f64() * 1000.0);
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

pub fn run(args: Vec<String>) -> i32 {
    let start = Instant::now();
    crate::log::init();
    crate::gfx::device::co_init();
    let perf = args.iter().any(|a| a == "--perf");
    if perf {
        let _ = PERF.set(start);
    }
    perf_mark("main");
    let gfx = Rc::new(Gfx::start());

    let settings = load_settings();
    i18n::set_lang(settings.effective_language());
    let entries = load_entries();
    fsx::clear_dir(&fsx::update_dir());
    perf_mark("data loaded");

    let cache = IconCache::load(&fsx::cache_path());
    perf_mark("cache loaded");
    let settings = Rc::new(RefCell::new(settings));
    let entries = Rc::new(RefCell::new(entries));

    let bar = Toolbar::new(gfx, entries, settings, cache, Box::new(|| set_bar_visible(false)), perf.then_some(start));
    bar.borrow_mut().show();
    let code = window::run_message_loop();
    log_info!("exit {code}");
    code
}
