//! Instancia única: mutex con nombre y ventana de mensajes. Una segunda instancia envía
//! `Glassbar.Toggle` a la primera y sale.

use crate::gfx::wide;
use crate::ui::window::Handler;
use std::ffi::c_void;
use std::sync::OnceLock;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

pub const MUTEX_NAME: &str = "Local\\Glassbar.SingleInstance";
pub const MSG_CLASS: &str = "Glassbar.Messages";

static TOGGLE_MSG: OnceLock<u32> = OnceLock::new();

pub fn toggle_msg() -> u32 {
    *TOGGLE_MSG.get_or_init(|| unsafe { RegisterWindowMessageW(w!("Glassbar.Toggle")) })
}

pub enum Instance {
    Primary(HANDLE),
    Secondary,
}

pub fn acquire() -> Instance {
    unsafe {
        let name = wide(MUTEX_NAME);
        match CreateMutexW(None, true, PCWSTR(name.as_ptr())) {
            Ok(h) => {
                if GetLastError() == ERROR_ALREADY_EXISTS {
                    let _ = CloseHandle(h);
                    Instance::Secondary
                } else {
                    Instance::Primary(h)
                }
            }
            Err(_) => Instance::Secondary,
        }
    }
}

/// Busca la ventana de mensajes de la instancia principal y le envía "alterna".
pub fn signal_toggle() -> bool {
    unsafe {
        let class = wide(MSG_CLASS);
        let hwnd = FindWindowExW(Some(HWND_MESSAGE), None, PCWSTR(class.as_ptr()), None);
        let Ok(hwnd) = hwnd else { return false };
        if hwnd.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            let _ = AllowSetForegroundWindow(pid);
        }
        PostMessageW(Some(hwnd), toggle_msg(), WPARAM(0), LPARAM(0)).is_ok()
    }
}

/// Ventana de solo mensajes de clase `MSG_CLASS`, con el mismo despacho que las demás ventanas.
pub fn create_message_window(handler: Handler) -> Result<HWND> {
    unsafe {
        let class_w = wide(MSG_CLASS);
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(crate::ui::window::wndproc_export),
            hInstance: hinst,
            lpszClassName: PCWSTR(class_w.as_ptr()),
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let boxed = Box::new(handler);
        let param = Box::into_raw(boxed) as *const c_void;
        CreateWindowExW(WINDOW_EX_STYLE(0), PCWSTR(class_w.as_ptr()), w!("Glassbar"), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(hinst), Some(param))
    }
}
