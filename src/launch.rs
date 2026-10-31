//! Lanzar entradas, abrir carpetas y URLs, y selectores de archivos nativos.

use crate::gfx::wide;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::*;

fn last_error_message() -> String {
    let e = Error::from_thread();
    let m = e.message();
    if m.is_empty() { format!("{:?}", e.code()) } else { m }
}

/// Abre el archivo con el verbo `open` del shell. Directorio de trabajo: el indicado o la carpeta del archivo.
pub fn launch(path: &Path, args: &str, working_dir: Option<&Path>) -> std::result::Result<(), String> {
    let file = wide(&path.to_string_lossy());
    let dir = working_dir.filter(|d| d.is_dir()).or_else(|| path.parent()).map(|d| wide(&d.to_string_lossy()));
    let params = if args.trim().is_empty() { None } else { Some(wide(args)) };
    let verb = wide("open");
    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: params.as_ref().map(|p| PCWSTR(p.as_ptr())).unwrap_or(PCWSTR::null()),
            lpDirectory: dir.as_ref().map(|d| PCWSTR(d.as_ptr())).unwrap_or(PCWSTR::null()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        ShellExecuteExW(&mut info).map_err(|_| last_error_message())
    }
}

pub fn open_location(path: &Path) {
    let args = wide(&format!("/select,\"{}\"", path.to_string_lossy()));
    unsafe {
        ShellExecuteW(None, w!("open"), w!("explorer.exe"), PCWSTR(args.as_ptr()), None, SW_SHOWNORMAL);
    }
}

pub fn open_folder(dir: &Path) {
    let d = wide(&dir.to_string_lossy());
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(d.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

pub fn open_url(url: &str) {
    let u = wide(url);
    unsafe {
        ShellExecuteW(None, w!("open"), PCWSTR(u.as_ptr()), None, None, SW_SHOWNORMAL);
    }
}

/// Selector de archivos nativo. `filters`: (nombre, patrón) como ("Imágenes", "*.png;*.jpg").
pub fn pick_file(owner: HWND, filters: &[(&str, &str)]) -> Option<PathBuf> {
    unsafe {
        let dlg: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok()?;
        let names: Vec<Vec<u16>> = filters.iter().map(|(n, _)| wide(n)).collect();
        let specs: Vec<Vec<u16>> = filters.iter().map(|(_, s)| wide(s)).collect();
        let fs: Vec<COMDLG_FILTERSPEC> = names.iter().zip(specs.iter()).map(|(n, s)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(s.as_ptr()) }).collect();
        dlg.SetFileTypes(&fs).ok()?;
        let opts = dlg.GetOptions().unwrap_or_default();
        dlg.SetOptions(opts | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM).ok()?;
        dlg.Show(Some(owner)).ok()?;
        let item = dlg.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = name.to_string().ok()?;
        CoTaskMemFree(Some(name.0 as *const c_void));
        Some(PathBuf::from(s))
    }
}
