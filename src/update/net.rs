//! Cliente HTTP mínimo con WinHTTP: HEAD sin redirecciones y GET a archivo con progreso.

use crate::gfx::wide;
use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use windows::Win32::Networking::WinHttp::*;
use windows::core::*;

pub struct Http {
    session: *mut c_void,
}

impl Drop for Http {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.session);
        }
    }
}

struct Handle(*mut c_void);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = WinHttpCloseHandle(self.0);
        }
    }
}

fn err(ctx: &str) -> String {
    let e = Error::from_thread();
    format!("{ctx}: {} ({})", e.message(), e.code().0 as u32 & 0xFFFF)
}

struct Url {
    host: String,
    path: String,
    port: u16,
}

fn parse_url(url: &str) -> std::result::Result<Url, String> {
    let rest = url.strip_prefix("https://").ok_or_else(|| "only https".to_string())?;
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    Ok(Url { host: host.to_string(), path: path.to_string(), port: 443 })
}

impl Http {
    pub fn new() -> std::result::Result<Http, String> {
        unsafe {
            let agent = wide(&format!("Glassbar/{}", env!("CARGO_PKG_VERSION")));
            let session = WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, None, None, 0);
            if session.is_null() {
                return Err(err("WinHttpOpen"));
            }
            let _ = WinHttpSetTimeouts(session, 5000, 5000, 5000, 15000);
            Ok(Http { session })
        }
    }

    fn open(&self, url: &Url, verb: PCWSTR) -> std::result::Result<(Handle, Handle), String> {
        unsafe {
            let host = wide(&url.host);
            let conn = WinHttpConnect(self.session, PCWSTR(host.as_ptr()), url.port, 0);
            if conn.is_null() {
                return Err(err("WinHttpConnect"));
            }
            let conn = Handle(conn);
            let path = wide(&url.path);
            let req = WinHttpOpenRequest(conn.0, verb, PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);
            if req.is_null() {
                return Err(err("WinHttpOpenRequest"));
            }
            Ok((conn, Handle(req)))
        }
    }

    /// HEAD sin seguir redirecciones. Devuelve la cabecera `Location` si la hay.
    pub fn head_location(&self, url: &str) -> std::result::Result<Option<String>, String> {
        let u = parse_url(url)?;
        unsafe {
            let (_conn, req) = self.open(&u, w!("HEAD"))?;
            let mut disable: u32 = WINHTTP_DISABLE_REDIRECTS;
            let _ = WinHttpSetOption(Some(req.0), WINHTTP_OPTION_DISABLE_FEATURE, Some(std::slice::from_raw_parts(&mut disable as *mut u32 as *const u8, 4)));
            WinHttpSendRequest(req.0, None, None, 0, 0, 0).map_err(|_| err("send"))?;
            WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|_| err("receive"))?;
            let mut buf = vec![0u16; 2048];
            let mut len = (buf.len() * 2) as u32;
            let r = WinHttpQueryHeaders(req.0, WINHTTP_QUERY_LOCATION, None, Some(buf.as_mut_ptr() as *mut c_void), &mut len, std::ptr::null_mut());
            if r.is_err() {
                let mut status: u32 = 0;
                let mut slen = 4u32;
                let _ = WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, None, Some(&mut status as *mut u32 as *mut c_void), &mut slen, std::ptr::null_mut());
                if status != 0 && !(300..400).contains(&status) {
                    return Err(format!("HTTP {status}"));
                }
                return Ok(None);
            }
            let n = (len as usize / 2).min(buf.len());
            Ok(Some(String::from_utf16_lossy(&buf[..n])))
        }
    }

    /// GET con redirecciones, escribiendo en `dest`. `progress(bytes, total)`.
    pub fn get_to_file(&self, url: &str, dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> std::result::Result<(), String> {
        let u = parse_url(url)?;
        unsafe {
            let (_conn, req) = self.open(&u, w!("GET"))?;
            WinHttpSendRequest(req.0, None, None, 0, 0, 0).map_err(|_| err("send"))?;
            WinHttpReceiveResponse(req.0, std::ptr::null_mut()).map_err(|_| err("receive"))?;
            let mut status: u32 = 0;
            let mut slen = 4u32;
            let _ = WinHttpQueryHeaders(req.0, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, None, Some(&mut status as *mut u32 as *mut c_void), &mut slen, std::ptr::null_mut());
            if status != 200 {
                return Err(format!("HTTP {status}"));
            }
            let mut total: u64 = 0;
            let mut tlen = 8u32;
            let total = if WinHttpQueryHeaders(req.0, WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER64, None, Some(&mut total as *mut u64 as *mut c_void), &mut tlen, std::ptr::null_mut()).is_ok() { Some(total) } else { None };
            let mut file = std::fs::File::create(dest).map_err(|e| e.to_string())?;
            let mut done: u64 = 0;
            let mut chunk = vec![0u8; 64 * 1024];
            loop {
                let mut avail: u32 = 0;
                WinHttpQueryDataAvailable(req.0, &mut avail).map_err(|_| err("available"))?;
                if avail == 0 {
                    break;
                }
                let want = (avail as usize).min(chunk.len());
                let mut read: u32 = 0;
                WinHttpReadData(req.0, chunk.as_mut_ptr() as *mut c_void, want as u32, &mut read).map_err(|_| err("read"))?;
                if read == 0 {
                    break;
                }
                file.write_all(&chunk[..read as usize]).map_err(|e| e.to_string())?;
                done += read as u64;
                progress(done, total);
            }
            file.flush().map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}
