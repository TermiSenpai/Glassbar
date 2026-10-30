//! Rutas de datos, escritura atómica y cuarentena de archivos corruptos.

use std::path::{Path, PathBuf};
use std::{fs, io};

pub const APP_DIR_NAME: &str = "Glassbar";
pub const LEGACY_DIR_NAME: &str = "BatLauncher";

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    let d = local_app_data().join(APP_DIR_NAME);
    let _ = fs::create_dir_all(&d);
    d
}

pub fn icons_dir() -> PathBuf {
    let d = data_dir().join("icons");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn update_dir() -> PathBuf {
    let d = data_dir().join("update");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn entries_path() -> PathBuf {
    data_dir().join("entries.json")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn cache_path() -> PathBuf {
    data_dir().join("icons.cache")
}

pub fn log_path() -> PathBuf {
    data_dir().join("log.txt")
}

pub fn legacy_dir() -> PathBuf {
    local_app_data().join(LEGACY_DIR_NAME)
}

/// Escribe `archivo.ext.tmp` y lo renombra sobre el destino.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let tmp = if ext.is_empty() { path.with_extension("tmp") } else { path.with_extension(format!("{ext}.tmp")) };
    fs::write(&tmp, bytes)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path)
        }
    }
}

/// Fecha y hora local como `AAAAMMDD-HHMMSS`.
pub fn timestamp_now() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Fecha y hora UTC como `AAAA-MM-DDTHH:MM:SSZ`.
pub fn now_iso() -> String {
    use windows::Win32::System::SystemInformation::GetSystemTime;
    let t = unsafe { GetSystemTime() };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

/// Renombra un archivo ilegible a `nombre.corrupt-AAAAMMDD-HHMMSS`.
pub fn quarantine_corrupt(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let dest = path.with_file_name(format!("{name}.corrupt-{}", timestamp_now()));
    fs::rename(path, &dest)?;
    Ok(dest)
}

/// Borra el contenido de una carpeta sin borrar la carpeta.
pub fn clear_dir(dir: &Path) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                let _ = fs::remove_dir_all(&p);
            } else {
                let _ = fs::remove_file(&p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("glassbar-fsx-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_atomic_creates_file_and_leaves_no_tmp() {
        let d = tmp("create");
        let p = d.join("a.json");
        write_atomic(&p, b"{}").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"{}");
        assert!(!d.join("a.json.tmp").exists());
    }

    #[test]
    fn write_atomic_replaces_existing() {
        let d = tmp("replace");
        let p = d.join("b.json");
        write_atomic(&p, b"1").unwrap();
        write_atomic(&p, b"22").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"22");
    }

    #[test]
    fn quarantine_renames_with_corrupt_suffix() {
        let d = tmp("quarantine");
        let p = d.join("entries.json");
        fs::write(&p, b"garbage").unwrap();
        let moved = quarantine_corrupt(&p).unwrap();
        assert!(!p.exists());
        assert!(moved.file_name().unwrap().to_string_lossy().starts_with("entries.json.corrupt-"));
    }

    #[test]
    fn timestamp_has_expected_shape() {
        let t = timestamp_now();
        assert_eq!(t.len(), 15);
        assert_eq!(&t[8..9], "-");
        let n = now_iso();
        assert_eq!(n.len(), 20);
        assert!(n.ends_with('Z'));
    }

    #[test]
    fn clear_dir_removes_children_only() {
        let d = tmp("clear");
        let sub = d.join("x");
        fs::create_dir_all(&sub).unwrap();
        fs::write(d.join("f.txt"), b"1").unwrap();
        clear_dir(&d);
        assert!(d.exists() && !sub.exists() && !d.join("f.txt").exists());
    }
}
