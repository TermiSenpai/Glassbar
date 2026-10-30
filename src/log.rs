//! Registro mínimo en `log.txt` con rotación y captura de pánicos.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const MAX_BYTES: usize = 1024 * 1024;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init() {
    let p = crate::fsx::log_path();
    rotate_if_needed(&p);
    if let Ok(mut f) = FILE.lock() {
        *f = Some(p);
    }
    std::panic::set_hook(Box::new(|info| {
        let msg = match info.payload().downcast_ref::<&str>() {
            Some(s) => s.to_string(),
            None => info.payload().downcast_ref::<String>().cloned().unwrap_or_default(),
        };
        let loc = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        write("PANIC", &format!("{msg} at {loc}"));
        crate::ui::window::destroy_all_windows();
    }));
}

pub fn rotate_if_needed(p: &Path) {
    if let Ok(m) = std::fs::metadata(p) {
        if m.len() as usize > MAX_BYTES {
            let rotated = p.with_file_name("log.1.txt");
            let _ = std::fs::remove_file(&rotated);
            let _ = std::fs::rename(p, rotated);
        }
    }
}

pub fn format_line(level: &str, msg: &str) -> String {
    format!("{} {level} {msg}\n", crate::fsx::timestamp_now())
}

pub fn write(level: &str, msg: &str) {
    let path = match FILE.lock() {
        Ok(f) => f.clone(),
        Err(_) => None,
    };
    if let Some(p) = path {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = f.write_all(format_line(level, msg).as_bytes());
        }
    }
}

#[macro_export]
macro_rules! log_info { ($($a:tt)*) => { $crate::log::write("INFO", &format!($($a)*)) } }
#[macro_export]
macro_rules! log_warn { ($($a:tt)*) => { $crate::log::write("WARN", &format!($($a)*)) } }
#[macro_export]
macro_rules! log_error { ($($a:tt)*) => { $crate::log::write("ERROR", &format!($($a)*)) } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_when_over_limit() {
        let d = std::env::temp_dir().join(format!("glassbar-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("log.txt");
        std::fs::write(&p, vec![b'x'; MAX_BYTES + 1]).unwrap();
        rotate_if_needed(&p);
        assert!(d.join("log.1.txt").exists());
        assert!(!p.exists());
    }

    #[test]
    fn format_line_has_level_and_message() {
        let l = format_line("INFO", "hola");
        assert!(l.contains(" INFO hola"));
        assert!(l.ends_with('\n'));
    }
}
