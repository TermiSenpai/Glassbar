//! Modo residente: icono de bandeja, atajo global y clave de inicio con Windows.

use crate::gfx::wide;
use crate::model::settings::{Hotkey, Settings};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

pub const HOTKEY_ID: i32 = 1;
const TRAY_ID: u32 = 1;
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Glassbar");

pub struct Lifecycle {
    msg_hwnd: HWND,
    tray: bool,
    hotkey: bool,
}

fn app_icon() -> HICON {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).map(|m| m.into()).unwrap_or_default();
        LoadIconW(Some(hinst), PCWSTR(1 as *const u16)).or_else(|_| LoadIconW(None, IDI_APPLICATION)).unwrap_or_default()
    }
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut d = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: TRAY_ID, ..Default::default() };
    d.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    d.uCallbackMessage = crate::ui::window::WM_TRAY;
    d.hIcon = app_icon();
    let tip = wide("Glassbar");
    for (i, c) in tip.iter().take(127).enumerate() {
        d.szTip[i] = *c;
    }
    d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
    d
}

impl Lifecycle {
    pub fn new(msg_hwnd: HWND) -> Lifecycle {
        Lifecycle { msg_hwnd, tray: false, hotkey: false }
    }

    /// Aplica el modo residente: bandeja y atajo si está encendido; sin ellos y sin inicio con Windows si no.
    pub fn set_resident(&mut self, on: bool, settings: &mut Settings) {
        crate::app::set_resident(on);
        if on {
            self.add_tray();
            let hk = settings.hotkey.clone();
            if self.set_hotkey(hk.as_ref()).is_err() {
                settings.hotkey = None;
            }
        } else {
            self.remove_tray();
            let _ = self.set_hotkey(None);
            if settings.start_with_windows {
                let _ = self.set_startup(false);
                settings.start_with_windows = false;
            }
        }
    }

    pub fn set_hotkey(&mut self, hk: Option<&Hotkey>) -> std::result::Result<(), ()> {
        unsafe {
            if self.hotkey {
                let _ = UnregisterHotKey(Some(self.msg_hwnd), HOTKEY_ID);
                self.hotkey = false;
            }
            let Some(hk) = hk else { return Ok(()) };
            let Some(vk) = hk.vk() else { return Err(()) };
            let mods = HOT_KEY_MODIFIERS(hk.modifiers_flags() | MOD_NOREPEAT.0);
            match RegisterHotKey(Some(self.msg_hwnd), HOTKEY_ID, mods, vk as u32) {
                Ok(()) => {
                    self.hotkey = true;
                    Ok(())
                }
                Err(_) => Err(()),
            }
        }
    }

    pub fn set_startup(&self, on: bool) -> std::result::Result<(), String> {
        unsafe {
            let mut key = HKEY::default();
            let r = RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, None, REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None);
            if r.is_err() {
                return Err(format!("{r:?}"));
            }
            let result = if on {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let cmd = wide(&format!("\"{}\" --background", exe.display()));
                let bytes: &[u8] = std::slice::from_raw_parts(cmd.as_ptr() as *const u8, cmd.len() * 2);
                let r = RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes));
                if r.is_err() { Err(format!("{r:?}")) } else { Ok(()) }
            } else {
                let r = RegDeleteValueW(key, RUN_VALUE);
                if r.is_err() && r != ERROR_FILE_NOT_FOUND { Err(format!("{r:?}")) } else { Ok(()) }
            };
            let _ = RegCloseKey(key);
            result
        }
    }

    /// Si el exe se movió, reescribe la clave de inicio con la ruta actual.
    pub fn refresh_startup_path(&self, settings: &Settings) {
        if settings.resident && settings.start_with_windows {
            let _ = self.set_startup(true);
        }
    }

    pub fn is_startup_registered() -> bool {
        unsafe {
            let mut size = 0u32;
            RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_ok()
        }
    }

    fn add_tray(&mut self) {
        if self.tray {
            return;
        }
        unsafe {
            let d = tray_data(self.msg_hwnd);
            if Shell_NotifyIconW(NIM_ADD, &d).as_bool() {
                let _ = Shell_NotifyIconW(NIM_SETVERSION, &d);
                self.tray = true;
                log_info!("tray icon added");
            } else {
                log_warn!("tray icon failed");
            }
        }
    }

    fn remove_tray(&mut self) {
        if !self.tray {
            return;
        }
        unsafe {
            let d = tray_data(self.msg_hwnd);
            let _ = Shell_NotifyIconW(NIM_DELETE, &d);
        }
        self.tray = false;
    }

    pub fn has_tray(&self) -> bool {
        self.tray
    }
}

impl Drop for Lifecycle {
    fn drop(&mut self) {
        self.remove_tray();
        let _ = self.set_hotkey(None);
    }
}
