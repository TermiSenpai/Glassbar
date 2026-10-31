//! Comprobación, descarga, verificación y reemplazo del exe. Los hilos informan con `WM_UPDATE_EVENT`.

use super::net::Http;
use super::version::{self, Version};
use crate::ui::settings::UpdateState;
use crate::ui::window;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Checking,
    UpToDate,
    Available(Version),
    Offline,
    Error(String),
    Downloading(u8),
    VerifyFailed,
    NotWritable,
    Applying,
    /// El exe ya está reemplazado: el hilo de interfaz relanza y sale.
    Swapped { exe: PathBuf, old: PathBuf },
}

fn post(hwnd: isize, ev: UpdateEvent) {
    window::post(hwnd, window::WM_UPDATE_EVENT, 0, Box::into_raw(Box::new(ev)) as isize);
}

fn is_offline(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("12007") || m.contains("12029") || m.contains("12002") || m.contains("12152") || m.contains("resolve") || m.contains("connect")
}

pub fn spawn_check(hwnd: isize) {
    post(hwnd, UpdateEvent::Checking);
    let _ = std::thread::Builder::new().name("glassbar-update-check".into()).spawn(move || {
        let r = Http::new().and_then(|h| h.head_location(&version::latest_url()));
        match r {
            Ok(Some(loc)) => match version::parse_latest_location(&loc) {
                Some(v) if v > Version::current() => post(hwnd, UpdateEvent::Available(v)),
                Some(_) => post(hwnd, UpdateEvent::UpToDate),
                None => post(hwnd, UpdateEvent::Error(format!("location: {loc}"))),
            },
            Ok(None) => post(hwnd, UpdateEvent::Error("no redirect".into())),
            Err(e) => post(hwnd, if is_offline(&e) { UpdateEvent::Offline } else { UpdateEvent::Error(e) }),
        }
    });
}

pub fn spawn_apply(hwnd: isize, v: Version) {
    post(hwnd, UpdateEvent::Downloading(0));
    let _ = std::thread::Builder::new().name("glassbar-update-apply".into()).spawn(move || {
        let dir = crate::fsx::update_dir();
        let exe_tmp = dir.join(version::ASSET);
        let sums_tmp = dir.join(version::SUMS);
        let run = || -> Result<(), UpdateEvent> {
            let http = Http::new().map_err(UpdateEvent::Error)?;
            http.get_to_file(&version::asset_url(v, version::SUMS), &sums_tmp, &mut |_, _| {}).map_err(|e| if is_offline(&e) { UpdateEvent::Offline } else { UpdateEvent::Error(e) })?;
            let mut last = 0u8;
            http.get_to_file(&version::asset_url(v, version::ASSET), &exe_tmp, &mut |done, total| {
                if let Some(t) = total {
                    let p = ((done as f64 / t.max(1) as f64) * 100.0) as u8;
                    if p != last {
                        last = p;
                        post(hwnd, UpdateEvent::Downloading(p));
                    }
                }
            })
            .map_err(|e| if is_offline(&e) { UpdateEvent::Offline } else { UpdateEvent::Error(e) })?;
            let sums = std::fs::read_to_string(&sums_tmp).map_err(|e| UpdateEvent::Error(e.to_string()))?;
            let expected = version::parse_sha256sums(&sums, version::ASSET).ok_or(UpdateEvent::VerifyFailed)?;
            let data = std::fs::read(&exe_tmp).map_err(|e| UpdateEvent::Error(e.to_string()))?;
            if version::sha256(&data) != expected {
                let _ = std::fs::remove_file(&exe_tmp);
                return Err(UpdateEvent::VerifyFailed);
            }
            post(hwnd, UpdateEvent::Applying);
            let (exe, old) = swap(&exe_tmp).map_err(|e| match e {
                SwapError::NotWritable => UpdateEvent::NotWritable,
                SwapError::Other(m) => UpdateEvent::Error(m),
            })?;
            post(hwnd, UpdateEvent::Swapped { exe, old });
            Ok(())
        };
        if let Err(ev) = run() {
            post(hwnd, ev);
        }
    });
}

pub enum SwapError {
    NotWritable,
    Other(String),
}

/// Renombra el exe actual a `.old` y coloca el nuevo. Devuelve (exe, old) para relanzar desde el hilo de interfaz.
pub fn swap(new_exe: &Path) -> Result<(PathBuf, PathBuf), SwapError> {
    let exe = std::env::current_exe().map_err(|e| SwapError::Other(e.to_string()))?;
    let dir = exe.parent().ok_or(SwapError::Other("exe dir".into()))?;
    let probe = dir.join(".glassbar-write-test");
    if std::fs::write(&probe, b"x").is_err() {
        return Err(SwapError::NotWritable);
    }
    let _ = std::fs::remove_file(&probe);
    let old = PathBuf::from(format!("{}.old", exe.display()));
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| SwapError::Other(format!("rename current: {e}")))?;
    if let Err(e) = std::fs::rename(new_exe, &exe).or_else(|_| std::fs::copy(new_exe, &exe).map(|_| ())) {
        let _ = std::fs::rename(&old, &exe);
        return Err(SwapError::Other(format!("place new: {e}")));
    }
    Ok((exe, old))
}

/// En el hilo de interfaz: lanza el exe nuevo con `--updated` y termina este proceso.
pub fn relaunch_and_quit(exe: &Path, old: &Path) {
    match std::process::Command::new(exe).arg("--updated").arg(old).spawn() {
        Ok(_) => {
            log_info!("update applied; relaunching {}", exe.display());
            window::post_quit();
        }
        Err(e) => log_warn!("relaunch failed: {e}"),
    }
}

/// Borra el `.old` que dejó la actualización anterior, con reintentos.
pub fn cleanup_old(old: &Path) {
    for _ in 0..10 {
        if !old.exists() || std::fs::remove_file(old).is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

/// Recibido en el hilo de interfaz: actualiza ajustes y estado.
pub fn on_event(ev: UpdateEvent) {
    let now = crate::fsx::now_iso();
    let state = match ev {
        UpdateEvent::Checking => UpdateState::Checking,
        UpdateEvent::UpToDate => {
            crate::app::with_settings(|s| {
                s.last_update_check = Some(now.clone());
                s.known_latest = Some(Version::current().to_string());
            });
            UpdateState::UpToDate
        }
        UpdateEvent::Available(v) => {
            crate::app::with_settings(|s| {
                s.last_update_check = Some(now.clone());
                s.known_latest = Some(v.to_string());
            });
            UpdateState::Available(v)
        }
        UpdateEvent::Offline => UpdateState::Offline,
        UpdateEvent::Error(e) => {
            log_warn!("update: {e}");
            UpdateState::Error(e)
        }
        UpdateEvent::Downloading(p) => UpdateState::Downloading(p),
        UpdateEvent::VerifyFailed => UpdateState::VerifyFailed,
        UpdateEvent::NotWritable => UpdateState::NotWritable,
        UpdateEvent::Applying => UpdateState::Restarting,
        UpdateEvent::Swapped { exe, old } => {
            crate::app::set_update_state(UpdateState::Restarting);
            relaunch_and_quit(&exe, &old);
            return;
        }
    };
    crate::app::set_update_state(state);
}
