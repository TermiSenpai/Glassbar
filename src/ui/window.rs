//! Ventana popup base sobre DirectComposition y despacho de mensajes a un `WindowHandler`.

use crate::gfx::Gfx;
use crate::gfx::wide;
use crate::ui::layout::WorkArea;
use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

pub const WM_ICON_READY: u32 = WM_APP + 1;
pub const WM_UPDATE_EVENT: u32 = WM_APP + 2;
pub const WM_TRAY: u32 = WM_APP + 3;
pub const WM_THEME_CHANGED: u32 = WM_APP + 4;
pub const WM_ENTRIES_CHANGED: u32 = WM_APP + 5;
pub const WM_DIALOG_CLOSED: u32 = WM_APP + 6;

pub const TIMER_TOOLTIP: usize = 1;
pub const TIMER_CARET: usize = 2;
pub const TIMER_HIDE: usize = 3;
pub const TIMER_TOAST: usize = 4;
pub const TIMER_UPDATE_CHECK: usize = 5;
pub const TIMER_MENU_FADE: usize = 6;

pub struct WindowCtx {
    pub hwnd: HWND,
    pub scale: f32,
}

pub trait WindowHandler {
    /// `None` delega en `DefWindowProcW`.
    fn on_message(&mut self, win: &WindowCtx, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT>;
}

pub type Handler = Rc<RefCell<dyn WindowHandler>>;

#[derive(Clone, Copy, Default)]
pub struct PopupStyle {
    pub topmost: bool,
    pub tool: bool,
    pub no_activate: bool,
    pub app_window: bool,
}

impl PopupStyle {
    pub const BAR: PopupStyle = PopupStyle { topmost: true, tool: true, no_activate: false, app_window: false };
    pub const TOOLTIP: PopupStyle = PopupStyle { topmost: true, tool: true, no_activate: true, app_window: false };
    pub const MENU: PopupStyle = PopupStyle { topmost: true, tool: true, no_activate: false, app_window: false };
    pub const DIALOG: PopupStyle = PopupStyle { topmost: false, tool: false, no_activate: false, app_window: true };
}

pub struct Comp {
    pub target: IDCompositionTarget,
    pub root: IDCompositionVisual2,
    pub dcomp: IDCompositionDesktopDevice,
}

pub struct Window {
    pub hwnd: HWND,
    pub comp: Option<Comp>,
    pub scale: Cell<f32>,
}

static WINDOWS: Mutex<Vec<isize>> = Mutex::new(Vec::new());

fn register(hwnd: HWND) {
    if let Ok(mut w) = WINDOWS.lock() {
        w.push(hwnd.0 as isize);
    }
}

fn unregister(hwnd: HWND) {
    if let Ok(mut w) = WINDOWS.lock() {
        w.retain(|h| *h != hwnd.0 as isize);
    }
}

/// Destruye todas las ventanas creadas por este módulo. Pensado para el gancho de pánico.
pub fn destroy_all_windows() {
    let list: Vec<isize> = WINDOWS.lock().map(|w| w.clone()).unwrap_or_default();
    for h in list {
        crate::ui::droptarget::revoke(HWND(h as *mut c_void));
        unsafe {
            let _ = DestroyWindow(HWND(h as *mut c_void));
        }
    }
}

pub unsafe extern "system" fn wndproc_export(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { wndproc(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            register(hwnd);
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Handler;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let result = {
            let handler = &*ptr;
            let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
            let ctx = WindowCtx { hwnd, scale };
            match handler.try_borrow_mut() {
                Ok(mut h) => h.on_message(&ctx, msg, wparam, lparam),
                Err(_) => None,
            }
        };
        if msg == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            unregister(hwnd);
            drop(Box::from_raw(ptr as *mut Handler));
        }
        match result {
            Some(r) => r,
            None => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn ensure_class(name: &[u16]) {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).map(|m| m.into()).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: PCWSTR(name.as_ptr()),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        // Falla si ya existe; es lo esperado a partir de la segunda ventana de la clase.
        let _ = RegisterClassExW(&wc);
    }
}

impl Window {
    /// Crea la ventana y le acopla la composición. Para ventanas creadas cuando los dispositivos ya existen.
    pub fn create(gfx: &Gfx, class: &str, style: PopupStyle, w: i32, h: i32, handler: Handler) -> Result<Window> {
        let mut win = Window::create_hwnd(class, style, w, h, handler)?;
        win.attach(gfx)?;
        Ok(win)
    }

    /// Solo el HWND. La composición se acopla después con `attach`, cuando los dispositivos estén listos.
    pub fn create_hwnd(class: &str, style: PopupStyle, w: i32, h: i32, handler: Handler) -> Result<Window> {
        unsafe {
            let class_w = wide(class);
            ensure_class(&class_w);
            let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
            let mut ex = WS_EX_NOREDIRECTIONBITMAP;
            if style.topmost {
                ex |= WS_EX_TOPMOST;
            }
            if style.tool {
                ex |= WS_EX_TOOLWINDOW;
            }
            if style.no_activate {
                ex |= WS_EX_NOACTIVATE;
            }
            if style.app_window {
                ex |= WS_EX_APPWINDOW;
            }
            let boxed = Box::new(handler);
            let param = Box::into_raw(boxed) as *const c_void;
            let hwnd = CreateWindowExW(ex, PCWSTR(class_w.as_ptr()), w!("Glassbar"), WS_POPUP, 0, 0, w.max(1), h.max(1), None, None, Some(hinst), Some(param))?;
            if style.tool {
                let disable = BOOL(1);
                let _ = DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, &disable as *const BOOL as *const c_void, size_of::<BOOL>() as u32);
                let excluded = BOOL(1);
                let _ = DwmSetWindowAttribute(hwnd, DWMWA_EXCLUDED_FROM_PEEK, &excluded as *const BOOL as *const c_void, size_of::<BOOL>() as u32);
            }
            let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
            Ok(Window { hwnd, comp: None, scale: Cell::new(scale) })
        }
    }

    pub fn attach(&mut self, gfx: &Gfx) -> Result<()> {
        if self.comp.is_some() {
            return Ok(());
        }
        let dcomp = gfx.dcomp()?.clone();
        unsafe {
            let target = dcomp.CreateTargetForHwnd(self.hwnd, true)?;
            let root = dcomp.CreateVisual()?;
            target.SetRoot(&root)?;
            self.comp = Some(Comp { target, root, dcomp });
        }
        Ok(())
    }

    pub fn root(&self) -> Result<&IDCompositionVisual2> {
        self.comp.as_ref().map(|c| &c.root).ok_or_else(|| Error::from(E_FAIL))
    }

    pub fn move_to(&self, x: i32, y: i32, w: i32, h: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, w.max(1), h.max(1), SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOOWNERZORDER);
        }
    }

    pub fn show(&self, activate: bool) {
        unsafe {
            if activate {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetForegroundWindow(self.hwnd);
            } else {
                let _ = ShowWindow(self.hwnd, SW_SHOWNA);
            }
        }
    }

    pub fn hide(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_HIDE);
        }
    }

    pub fn is_visible(&self) -> bool {
        unsafe { IsWindowVisible(self.hwnd).as_bool() }
    }

    pub fn commit(&self) -> Result<()> {
        match &self.comp {
            Some(c) => unsafe { c.dcomp.Commit() },
            None => Ok(()),
        }
    }

    pub fn set_timer(&self, id: usize, ms: u32) {
        unsafe {
            SetTimer(Some(self.hwnd), id, ms, None);
        }
    }

    pub fn kill_timer(&self, id: usize) {
        unsafe {
            let _ = KillTimer(Some(self.hwnd), id);
        }
    }

    pub fn destroy(&self) {
        crate::ui::droptarget::revoke(self.hwnd);
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }

    pub fn set_title(&self, title: &str) {
        let t = wide(title);
        unsafe {
            let _ = SetWindowTextW(self.hwnd, PCWSTR(t.as_ptr()));
        }
    }

    /// Rectángulo de la ventana en píxeles físicos de pantalla.
    pub fn rect(&self) -> RECT {
        let mut r = RECT::default();
        unsafe {
            let _ = GetWindowRect(self.hwnd, &mut r);
        }
        r
    }
}

/// ¿La ventana pertenece a este hilo? Sirve para no ocultar la barra cuando el foco pasa a un menú o diálogo propio.
pub fn is_ours(hwnd: HWND) -> bool {
    if hwnd.0.is_null() {
        return false;
    }
    unsafe { GetWindowThreadProcessId(hwnd, None) == windows::Win32::System::Threading::GetCurrentThreadId() }
}

pub struct Monitor {
    pub handle: HMONITOR,
    pub scale: f32,
    /// Área de trabajo en píxeles lógicos (físicos / escala), en coordenadas absolutas de pantalla.
    pub work: WorkArea,
    pub work_phys: RECT,
    pub monitor_phys: RECT,
}

fn monitor_info(handle: HMONITOR) -> Monitor {
    unsafe {
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(handle, &mut mi);
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let scale = dx as f32 / 96.0;
        let w = mi.rcWork;
        let taskbar_top = w.top > mi.rcMonitor.top;
        Monitor {
            handle,
            scale,
            work: WorkArea { x: w.left as f32 / scale, y: w.top as f32 / scale, w: (w.right - w.left) as f32 / scale, h: (w.bottom - w.top) as f32 / scale, taskbar_top },
            work_phys: w,
            monitor_phys: mi.rcMonitor,
        }
    }
}

/// Monitor bajo el cursor.
pub fn cursor_monitor() -> Monitor {
    unsafe {
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        monitor_info(MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST))
    }
}

/// Monitor de una ventana o de un punto físico.
pub fn monitor_at(x: i32, y: i32) -> Monitor {
    unsafe { monitor_info(MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST)) }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

pub fn run_message_loop() -> i32 {
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        msg.wParam.0 as i32
    }
}

pub fn post_quit() {
    unsafe { PostQuitMessage(0) };
}

pub fn lparam_xy(lparam: LPARAM) -> (i32, i32) {
    let x = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    (x, y)
}

pub fn wheel_delta(wparam: WPARAM) -> i32 {
    ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

pub fn key_down(vk: u16) -> bool {
    unsafe { (windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

/// Mensaje para una ventana desde otro hilo.
pub fn post(hwnd_raw: isize, msg: u32, wparam: usize, lparam: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd_raw as *mut c_void)), msg, WPARAM(wparam), LPARAM(lparam));
    }
}

pub fn invalidate_tracking(hwnd: HWND) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
        let _ = TrackMouseEvent(&mut t);
    }
}

pub fn client_to_screen(hwnd: HWND, x: i32, y: i32) -> (i32, i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ClientToScreen(hwnd, &mut p);
    }
    (p.x, p.y)
}

pub fn screen_to_client(hwnd: HWND, x: i32, y: i32) -> (i32, i32) {
    let mut p = POINT { x, y };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut p);
    }
    (p.x, p.y)
}

pub fn capture_mouse(hwnd: HWND) {
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
    }
}

pub fn release_mouse() {
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
    }
}

pub fn set_arrow_cursor() {
    unsafe {
        if let Ok(c) = LoadCursorW(None, IDC_ARROW) {
            SetCursor(Some(c));
        }
    }
}

pub fn set_hand_cursor() {
    unsafe {
        if let Ok(c) = LoadCursorW(None, IDC_HAND) {
            SetCursor(Some(c));
        }
    }
}

pub fn set_ibeam_cursor() {
    unsafe {
        if let Ok(c) = LoadCursorW(None, IDC_IBEAM) {
            SetCursor(Some(c));
        }
    }
}

pub fn now_ms() -> u64 {
    unsafe { windows::Win32::System::SystemInformation::GetTickCount64() }
}

pub fn gdi_unused() -> HGDIOBJ {
    HGDIOBJ::default()
}

/// Acepta archivos arrastrados desde el Explorador mediante un destino OLE registrado. No se
/// activa `WS_EX_ACCEPTFILES` para que OLE consulte siempre el destino registrado y no la ruta heredada.
/// Si el proceso corre elevado, UIPI bloquea `WM_DROPFILES` y `WM_COPYGLOBALDATA`; el filtro los permite.
pub fn accept_drops(hwnd: HWND) {
    const WM_COPYGLOBALDATA: u32 = 0x0049;
    unsafe {
        for msg in [WM_DROPFILES, WM_COPYDATA, WM_COPYGLOBALDATA] {
            let _ = ChangeWindowMessageFilterEx(hwnd, msg, MSGFLT_ALLOW, None);
        }
    }
    crate::ui::droptarget::register(hwnd);
}
