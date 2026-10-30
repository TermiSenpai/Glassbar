//! Extracción de iconos: imágenes con WIC, iconos de archivos con el shell, PNG y el hilo de trabajo.

use crate::gfx::wide;
use crate::model::entries::IconSource;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::IPropertyBag2;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::*;
use windows::core::*;

/// Premultiplica el alfa de un búfer BGRA.
pub fn premultiply(bgra: &mut [u8]) {
    for px in bgra.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 255 {
            px[0] = (px[0] as u32 * a / 255) as u8;
            px[1] = (px[1] as u32 * a / 255) as u8;
            px[2] = (px[2] as u32 * a / 255) as u8;
        }
    }
}

pub fn unpremultiply(bgra: &mut [u8]) {
    for px in bgra.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            px[0] = (px[0] as u32 * 255 / a).min(255) as u8;
            px[1] = (px[1] as u32 * 255 / a).min(255) as u8;
            px[2] = (px[2] as u32 * 255 / a).min(255) as u8;
        }
    }
}

/// Copia `src` (sw×sh) centrado en un lienzo transparente de px×px, sin escalar.
pub fn fit_center(src: &[u8], sw: u32, sh: u32, px: u32) -> Vec<u8> {
    let mut out = vec![0u8; (px * px * 4) as usize];
    let ox = (px as i64 - sw as i64) / 2;
    let oy = (px as i64 - sh as i64) / 2;
    for y in 0..sh as i64 {
        let dy = y + oy;
        if dy < 0 || dy >= px as i64 {
            continue;
        }
        for x in 0..sw as i64 {
            let dx = x + ox;
            if dx < 0 || dx >= px as i64 {
                continue;
            }
            let si = ((y as u32 * sw + x as u32) * 4) as usize;
            let di = ((dy as u32 * px + dx as u32) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// Decodifica una imagen y la ajusta a px×px manteniendo la proporción. BGRA premultiplicado.
pub fn load_image(wic: &IWICImagingFactory, path: &Path, px: u32) -> Option<Vec<u8>> {
    unsafe {
        let p = wide(&path.to_string_lossy());
        let decoder = wic.CreateDecoderFromFilename(PCWSTR(p.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = decoder.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        let (sw, sh) = if w >= h { (px, ((h as f32 / w as f32) * px as f32).round().max(1.0) as u32) } else { (((w as f32 / h as f32) * px as f32).round().max(1.0) as u32, px) };
        let scaler = wic.CreateBitmapScaler().ok()?;
        scaler.Initialize(&frame, sw, sh, WICBitmapInterpolationModeFant).ok()?;
        let conv = wic.CreateFormatConverter().ok()?;
        conv.Initialize(&scaler, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let mut buf = vec![0u8; (sw * sh * 4) as usize];
        conv.CopyPixels(std::ptr::null(), sw * 4, &mut buf).ok()?;
        Some(fit_center(&buf, sw, sh, px))
    }
}

/// Icono de un archivo (exe, lnk, cualquier tipo) vía el shell. BGRA premultiplicado de px×px.
pub fn extract_file_icon(path: &Path, px: u32) -> Option<Vec<u8>> {
    unsafe {
        let p = wide(&path.to_string_lossy());
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(p.as_ptr()), None::<&IBindCtx>).ok()?;
        let hbm = factory.GetImage(SIZE { cx: px as i32, cy: px as i32 }, SIIGBF_ICONONLY).ok()?;
        let out = hbitmap_to_bgra(hbm, px);
        let _ = DeleteObject(hbm.into());
        out
    }
}

unsafe fn hbitmap_to_bgra(hbm: HBITMAP, px: u32) -> Option<Vec<u8>> {
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(hbm.into(), size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut c_void)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth as u32, bm.bmHeight.unsigned_abs());
        if w == 0 || h == 0 {
            return None;
        }
        let screen = GetDC(None);
        let mut bmi = BITMAPINFO {
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
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let lines = GetDIBits(screen, hbm, 0, h, Some(buf.as_mut_ptr() as *mut c_void), &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(None, screen);
        if lines == 0 {
            return None;
        }
        if buf.chunks_exact(4).all(|p| p[3] == 0) {
            for p in buf.chunks_exact_mut(4) {
                p[3] = 255;
            }
        }
        premultiply(&mut buf);
        Some(if w == px && h == px { buf } else { fit_center(&buf, w, h, px) })
    }
}

/// Guarda BGRA premultiplicado de px×px como PNG.
pub fn encode_png(wic: &IWICImagingFactory, px: u32, bgra_premul: &[u8], dest: &Path) -> std::result::Result<(), String> {
    unsafe {
        let mut buf = bgra_premul.to_vec();
        unpremultiply(&mut buf);
        let p = wide(&dest.to_string_lossy());
        let stream = wic.CreateStream().map_err(|e| e.to_string())?;
        stream.InitializeFromFilename(PCWSTR(p.as_ptr()), GENERIC_WRITE.0).map_err(|e| e.to_string())?;
        let encoder = wic.CreateEncoder(&GUID_ContainerFormatPng, std::ptr::null()).map_err(|e| e.to_string())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).map_err(|e| e.to_string())?;
        let mut frame: Option<IWICBitmapFrameEncode> = None;
        let mut props: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut props).map_err(|e| e.to_string())?;
        let frame = frame.ok_or("frame")?;
        frame.Initialize(props.as_ref()).map_err(|e| e.to_string())?;
        frame.SetSize(px, px).map_err(|e| e.to_string())?;
        let mut fmt = GUID_WICPixelFormat32bppBGRA;
        frame.SetPixelFormat(&mut fmt).map_err(|e| e.to_string())?;
        frame.WritePixels(px, px * 4, &buf).map_err(|e| e.to_string())?;
        frame.Commit().map_err(|e| e.to_string())?;
        encoder.Commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub struct Job {
    pub index: usize,
    pub id: String,
    pub source: PathBuf,
    pub kind: IconSource,
    pub px: u32,
}

pub struct IconResult {
    pub index: usize,
    pub id: String,
    pub px: u32,
    pub fingerprint: u64,
    pub bgra: Option<Vec<u8>>,
}

/// Hilo de trabajo: procesa los trabajos en orden y envía cada resultado con `WM_ICON_READY`.
pub fn spawn_worker(jobs: Vec<Job>, hwnd_raw: isize) {
    std::thread::Builder::new()
        .name("glassbar-icons".into())
        .spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            let wic: Option<IWICImagingFactory> = unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None::<&IUnknown>, CLSCTX_INPROC_SERVER).ok() };
            for job in jobs {
                let fingerprint = crate::icons::cache::fingerprint_for(&job.source).unwrap_or(0);
                let bgra = match (&job.kind, &wic) {
                    (IconSource::Custom { .. }, Some(w)) => load_image(w, &job.source, job.px),
                    (IconSource::File, _) => extract_file_icon(&job.source, job.px),
                    _ => None,
                };
                let r = Box::new(IconResult { index: job.index, id: job.id, px: job.px, fingerprint, bgra });
                crate::ui::window::post(hwnd_raw, crate::ui::window::WM_ICON_READY, job.index, Box::into_raw(r) as isize);
            }
            unsafe { CoUninitialize() };
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiply_halves_half_alpha() {
        let mut p = [200, 100, 50, 128];
        premultiply(&mut p);
        assert_eq!(p, [100, 50, 25, 128]);
        unpremultiply(&mut p);
        assert!(p[0] >= 198 && p[0] <= 200);
    }

    #[test]
    fn fit_center_places_small_in_middle() {
        let src = vec![255; 4];
        let out = fit_center(&src, 1, 1, 3);
        let i = (1 * 3 + 1) * 4;
        assert_eq!(&out[i..i + 4], &[255, 255, 255, 255]);
        assert_eq!(out[0], 0);
        let wide_src = vec![7u8; 4 * 4];
        let out2 = fit_center(&wide_src, 4, 1, 2);
        assert_eq!(out2.len(), 16);
        assert_eq!(out2[0], 7); // fila 0: columnas centrales del origen
        assert_eq!(out2[8], 0); // fila 1: vacía
    }
}
