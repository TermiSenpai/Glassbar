//! Accesos directos `.lnk`: se resuelven a su archivo original al añadirlos, con sus argumentos
//! y directorio de trabajo, para que el acceso directo pueda borrarse y la entrada siga funcionando.

use crate::gfx::wide;
use std::path::{Path, PathBuf};
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::core::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ShortcutTarget {
    pub path: PathBuf,
    pub args: String,
    pub working_dir: Option<PathBuf>,
}

pub fn is_shortcut(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false)
}

fn from_buf(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// `None` si no es un `.lnk`, no se puede leer, o su destino no es un archivo.
pub fn resolve_shortcut(lnk: &Path) -> Option<ShortcutTarget> {
    if !is_shortcut(lnk) {
        return None;
    }
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        let p = wide(&lnk.to_string_lossy());
        pf.Load(PCWSTR(p.as_ptr()), STGM_READ).ok()?;
        let mut path_buf = vec![0u16; 32768];
        link.GetPath(&mut path_buf, std::ptr::null_mut(), 0).ok()?;
        let target = from_buf(&path_buf);
        if target.is_empty() {
            return None;
        }
        let mut args_buf = vec![0u16; 32768];
        let args = if link.GetArguments(&mut args_buf).is_ok() { from_buf(&args_buf) } else { String::new() };
        let mut dir_buf = vec![0u16; 32768];
        let dir = if link.GetWorkingDirectory(&mut dir_buf).is_ok() { from_buf(&dir_buf) } else { String::new() };
        Some(ShortcutTarget { path: PathBuf::from(target), args, working_dir: if dir.is_empty() { None } else { Some(PathBuf::from(dir)) } })
    }
}

/// Destino de un `.lnk` si existe como archivo; si no, el propio archivo sin cambios.
pub fn effective_target(path: &Path) -> (PathBuf, String, Option<PathBuf>) {
    match resolve_shortcut(path) {
        Some(t) if t.path.is_file() => (t.path, t.args, t.working_dir),
        _ => (path.to_path_buf(), String::new(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_shortcut(lnk: &Path, target: &str, args: &str, dir: &str) {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None::<&IUnknown>, CLSCTX_INPROC_SERVER).unwrap();
            let t = wide(target);
            link.SetPath(PCWSTR(t.as_ptr())).unwrap();
            let a = wide(args);
            link.SetArguments(PCWSTR(a.as_ptr())).unwrap();
            let d = wide(dir);
            link.SetWorkingDirectory(PCWSTR(d.as_ptr())).unwrap();
            let pf: IPersistFile = link.cast().unwrap();
            let p = wide(&lnk.to_string_lossy());
            pf.Save(PCWSTR(p.as_ptr()), true).unwrap();
        }
    }

    #[test]
    fn resolves_a_real_shortcut_with_arguments() {
        let d = std::env::temp_dir().join(format!("glassbar-lnk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let lnk = d.join("Mi consola.lnk");
        let target = "C:\\Windows\\System32\\cmd.exe";
        create_shortcut(&lnk, target, "/k echo hola", "C:\\Windows");
        let r = resolve_shortcut(&lnk).expect("resolved");
        assert!(r.path.to_string_lossy().eq_ignore_ascii_case(target));
        assert_eq!(r.args, "/k echo hola");
        assert_eq!(r.working_dir.as_deref().map(|p| p.to_string_lossy().to_string()), Some("C:\\Windows".to_string()));
        let (p, a, _) = effective_target(&lnk);
        assert!(p.to_string_lossy().eq_ignore_ascii_case(target));
        assert_eq!(a, "/k echo hola");
        assert!(resolve_shortcut(Path::new("C:\\no\\es.exe")).is_none());
        let (p2, a2, _) = effective_target(Path::new("C:\\no\\es.exe"));
        assert_eq!(p2, PathBuf::from("C:\\no\\es.exe"));
        assert!(a2.is_empty());
    }
}
