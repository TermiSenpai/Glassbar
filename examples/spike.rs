//! Spike: ventana sin bitmap de redirección, Acrylic por instantánea,
//! animación de apertura con DirectComposition y medición de primer frame.
//! Ejecutar: cargo run --release --example spike

use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::ptr::null_mut;
use std::time::Instant;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;
use windows_numerics::{Matrix3x2, Vector2};

struct Capture {
    w: u32,
    h: u32,
    bgra: Vec<u8>,
}

unsafe fn capture_screen(x: i32, y: i32, w: u32, h: u32) -> Option<Capture> {
    unsafe {
        let screen = GetDC(None);
        if screen.is_invalid() {
            return None;
        }
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = null_mut();
        let hbm = match CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(b) => b,
            Err(_) => {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return None;
            }
        };
        let old = SelectObject(mem, hbm.into());
        let ok = BitBlt(mem, 0, 0, w as i32, h as i32, Some(screen), x, y, SRCCOPY).is_ok();
        let mut out = Vec::new();
        if ok && !bits.is_null() {
            out = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize).to_vec();
        }
        SelectObject(mem, old);
        let _ = DeleteObject(hbm.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !ok || out.iter().all(|&b| b == 0) {
            return None;
        }
        for px in out.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Some(Capture { w, h, bgra: out })
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_KEYDOWN if wp.0 == VK_ESCAPE.0 as usize => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_LBUTTONDOWN | WM_DESTROY | WM_TIMER => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}

fn color(r: u8, g: u8, b: u8, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
}

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h }, radiusX: r, radiusY: r }
}

fn bitmap_props(target: bool) -> D2D1_BITMAP_PROPERTIES1 {
    D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: if target { D2D1_BITMAP_OPTIONS_TARGET } else { D2D1_BITMAP_OPTIONS_NONE },
        colorContext: ManuallyDrop::new(None),
    }
}

/// Polinomio cúbico que lleva de `from` a `to` en `dur_ms`, tras `delay_ms`.
unsafe fn anim(dcomp: &IDCompositionDesktopDevice, from: f32, to: f32, dur_ms: u32, delay_ms: u32, ease_out: bool) -> Result<IDCompositionAnimation> {
    unsafe {
        let a = dcomp.CreateAnimation()?;
        let d = dur_ms as f32 / 1000.0;
        let delay = delay_ms as f64 / 1000.0;
        if delay_ms > 0 {
            a.AddCubic(0.0, from, 0.0, 0.0, 0.0)?;
        }
        let r = to - from;
        if ease_out {
            a.AddCubic(delay, from, 3.0 * r / d, -3.0 * r / (d * d), r / (d * d * d))?;
        } else {
            a.AddCubic(delay, from, 0.0, 0.0, r / (d * d * d))?;
        }
        a.End(delay + d as f64, to)?;
        Ok(a)
    }
}

fn main() -> Result<()> {
    let start = Instant::now();
    unsafe {
        // Monitor del cursor y DPI.
        let mut pt = POINT::default();
        GetCursorPos(&mut pt)?;
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        GetMonitorInfoW(hmon, &mut mi).ok()?;
        let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
        GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y)?;
        let s = dpi_x as f32 / 96.0;

        // Geometría: plato 600x60 lógico, margen 32 para sombra y deslizamiento.
        let plate_w = (600.0 * s).round() as i32;
        let plate_h = (60.0 * s).round() as i32;
        let m = (32.0 * s).round() as i32;
        let win_w = plate_w + 2 * m;
        let win_h = plate_h + 2 * m;
        let work = mi.rcWork;
        let plate_x = work.left + (work.right - work.left - plate_w) / 2;
        let plate_y = work.bottom - (8.0 * s).round() as i32 - plate_h;

        // Captura de lo que hay detrás del plato (antes de mostrar la ventana).
        let t_cap = Instant::now();
        let cap = capture_screen(plate_x, plate_y, plate_w as u32, plate_h as u32);
        let cap_ms = t_cap.elapsed().as_secs_f64() * 1000.0;

        let t_win = Instant::now();
        // Ventana.
        let hinst: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = w!("GlassbarSpike");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!(""),
            WS_POPUP,
            plate_x - m,
            plate_y - m,
            win_w,
            win_h,
            None,
            None,
            Some(hinst),
            None,
        )?;
        let disable = BOOL(1);
        DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, &disable as *const BOOL as *const c_void, size_of::<BOOL>() as u32)?;

        let win_ms = t_win.elapsed().as_secs_f64() * 1000.0;
        // Dispositivos.
        let t_dev = Instant::now();
        let args: Vec<String> = std::env::args().collect();
        let warp = args.iter().any(|a| a == "--warp");
        let driver = if warp { D3D_DRIVER_TYPE_WARP } else { D3D_DRIVER_TYPE_HARDWARE };
        let mut d3d: Option<ID3D11Device> = None;
        D3D11CreateDevice(None, driver, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut d3d), None, None)?;
        let d3d = d3d.expect("d3d device");
        let d3d_ms = t_dev.elapsed().as_secs_f64() * 1000.0;
        let dxgi: IDXGIDevice = d3d.cast()?;
        let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
        let d2d_device = factory.CreateDevice(&dxgi)?;
        let odc = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
        let d2d_ms = t_dev.elapsed().as_secs_f64() * 1000.0;
        let dcomp: IDCompositionDesktopDevice = DCompositionCreateDevice2(&d2d_device)?;
        let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
        let root = dcomp.CreateVisual()?;
        target.SetRoot(&root)?;
        let group = dcomp.CreateVisual()?;
        root.AddVisual(&group, true, None)?;
        let dcomp_ms = t_dev.elapsed().as_secs_f64() * 1000.0;

        // Plato: superficie del tamaño de la ventana.
        let t_blur = Instant::now();
        let plate_surface = dcomp.CreateSurface(win_w as u32, win_h as u32, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
        {
            let mf = m as f32;
            let pw = plate_w as f32;
            let ph = plate_h as f32;
            let radius = 12.0 * s;
            let plate = rrect(mf, mf, pw, ph, radius);
            let full = D2D_SIZE_U { width: win_w as u32, height: win_h as u32 };

            // Offscreen 1: fondo desenfocado colocado en la posición del plato.
            let blurred = odc.CreateBitmap(full, None, 0, &bitmap_props(true))?;
            odc.SetTarget(&blurred);
            odc.BeginDraw();
            odc.Clear(Some(&color(0, 0, 0, 0.0)));
            if let Some(c) = &cap {
                let src = odc.CreateBitmap(D2D_SIZE_U { width: c.w, height: c.h }, Some(c.bgra.as_ptr() as *const c_void), c.w * 4, &bitmap_props(false))?;
                let blur = odc.CreateEffect(&CLSID_D2D1GaussianBlur)?;
                blur.SetInput(0, &src, true);
                let sd: f32 = 20.0 * s;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &sd.to_le_bytes())?;
                let bm: u32 = D2D1_BORDER_MODE_HARD.0 as u32;
                blur.SetValue(D2D1_GAUSSIANBLUR_PROP_BORDER_MODE.0 as u32, D2D1_PROPERTY_TYPE_ENUM, &bm.to_le_bytes())?;
                let out = blur.GetOutput()?;
                let at = Vector2 { X: mf, Y: mf };
                odc.DrawImage(&out, Some(&at), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
            }
            odc.EndDraw(None, None)?;

            // Offscreen 2: silueta del plato para la sombra.
            let silhouette = odc.CreateBitmap(full, None, 0, &bitmap_props(true))?;
            odc.SetTarget(&silhouette);
            odc.BeginDraw();
            odc.Clear(Some(&color(0, 0, 0, 0.0)));
            let black = odc.CreateSolidColorBrush(&color(0, 0, 0, 0.42), None)?;
            odc.FillRoundedRectangle(&plate, &black);
            odc.EndDraw(None, None)?;

            // Superficie final.
            let mut off = POINT::default();
            let dc: ID2D1DeviceContext = plate_surface.BeginDraw(None, &mut off)?;
            dc.SetTransform(&Matrix3x2::translation(off.x as f32, off.y as f32));
            dc.Clear(Some(&color(0, 0, 0, 0.0)));

            let shadow = dc.CreateEffect(&CLSID_D2D1Shadow)?;
            shadow.SetInput(0, &silhouette, true);
            let ssd: f32 = 10.0 * s;
            shadow.SetValue(D2D1_SHADOW_PROP_BLUR_STANDARD_DEVIATION.0 as u32, D2D1_PROPERTY_TYPE_FLOAT, &ssd.to_le_bytes())?;
            let sout = shadow.GetOutput()?;
            let sat = Vector2 { X: 0.0, Y: 6.0 * s };
            dc.DrawImage(&sout, Some(&sat), None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);

            let brush = dc.CreateBitmapBrush(&blurred, None, None)?;
            dc.FillRoundedRectangle(&plate, &brush);
            let tint = dc.CreateSolidColorBrush(&color(40, 40, 46, 0.62), None)?;
            dc.FillRoundedRectangle(&plate, &tint);
            let border = dc.CreateSolidColorBrush(&color(255, 255, 255, 0.09), None)?;
            dc.DrawRoundedRectangle(&plate, &border, 1.0 * s, None);
            plate_surface.EndDraw()?;
        }
        let blur_ms = t_blur.elapsed().as_secs_f64() * 1000.0;

        let plate_visual = dcomp.CreateVisual()?;
        plate_visual.SetContent(&plate_surface)?;
        group.AddVisual(&plate_visual, true, None)?;

        // Diez "iconos": círculos de colores.
        let icon_px = (34.0 * s).round() as u32;
        let cell = 48.0 * s;
        let step = 52.0 * s;
        let pad_x = 8.0 * s;
        let pad_y = 6.0 * s;
        let mut keep: Vec<(IDCompositionVisual2, IDCompositionSurface)> = Vec::new();
        let palette = [
            (96, 165, 250), (248, 113, 113), (52, 211, 153), (251, 191, 36), (167, 139, 250),
            (244, 114, 182), (45, 212, 191), (251, 146, 60), (163, 230, 53), (148, 163, 184),
        ];
        for (i, (r, g, b)) in palette.iter().enumerate() {
            let surf = dcomp.CreateSurface(icon_px, icon_px, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_ALPHA_MODE_PREMULTIPLIED)?;
            let mut off = POINT::default();
            let dc: ID2D1DeviceContext = surf.BeginDraw(None, &mut off)?;
            dc.SetTransform(&Matrix3x2::translation(off.x as f32, off.y as f32));
            dc.Clear(Some(&color(0, 0, 0, 0.0)));
            let fill = dc.CreateSolidColorBrush(&color(*r, *g, *b, 1.0), None)?;
            let half = icon_px as f32 / 2.0;
            dc.FillEllipse(&D2D1_ELLIPSE { point: Vector2 { X: half, Y: half }, radiusX: half, radiusY: half }, &fill);
            surf.EndDraw()?;

            let v = dcomp.CreateVisual()?;
            v.SetContent(&surf)?;
            let x = m as f32 + pad_x + i as f32 * step + (cell - icon_px as f32) / 2.0;
            let y = m as f32 + pad_y + (cell - icon_px as f32) / 2.0;
            v.SetOffsetX2(x)?;
            let delay = 30 + 38 * i as u32;
            v.SetOffsetY(&anim(&dcomp, y + 8.0 * s, y, 220, delay, true)?)?;
            let v3: IDCompositionVisual3 = v.cast()?;
            v3.SetOpacity(&anim(&dcomp, 0.0, 1.0, 220, delay, true)?)?;
            group.AddVisual(&v, true, &plate_visual)?;
            keep.push((v, surf));
        }

        // Animación del grupo: opacidad, deslizamiento y escala.
        let g3: IDCompositionVisual3 = group.cast()?;
        g3.SetOpacity(&anim(&dcomp, 0.0, 1.0, 260, 0, true)?)?;
        group.SetOffsetY(&anim(&dcomp, 18.0 * s, 0.0, 260, 0, true)?)?;
        let scale = dcomp.CreateScaleTransform()?;
        scale.SetCenterX2(m as f32 + plate_w as f32 / 2.0)?;
        scale.SetCenterY2(m as f32 + plate_h as f32 / 2.0)?;
        let sa = anim(&dcomp, 0.98, 1.0, 260, 0, true)?;
        scale.SetScaleX(&sa)?;
        scale.SetScaleY(&sa)?;
        group.SetTransform(&scale)?;

        // `--exit-after <ms>` cierra solo, para pruebas sin intervención.
        if let Some(i) = args.iter().position(|a| a == "--exit-after") {
            if let Some(ms) = args.get(i + 1).and_then(|v| v.parse::<u32>().ok()) {
                SetTimer(Some(hwnd), 1, ms, None);
            }
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        dcomp.Commit()?;
        let first_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!("driver: {}", if warp { "WARP" } else { "hardware" });
        println!("first-frame: {first_ms:.1} ms");
        println!("phases: window {win_ms:.1} ms | d3d {d3d_ms:.1} ms | +d2d {d2d_ms:.1} ms | +dcomp {dcomp_ms:.1} ms (cumulative from device start)");
        println!("capture: {cap_ms:.2} ms (captured: {})", cap.is_some());
        println!("plate-render (blur+shadow): {blur_ms:.2} ms");
        println!("Escape o clic para salir.");

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        drop(keep);
        drop(plate_visual);
        drop(target);
    }
    Ok(())
}
