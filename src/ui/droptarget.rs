//! Destino OLE de arrastrar y soltar.
//!
//! El Explorador arrastra con `DoDragDrop`. Hasta 2.0.4 las ventanas dependían de la ruta heredada
//! (`DragAcceptFiles` + `WM_DROPFILES`), que OLE resuelve por su cuenta y que un usuario vio fallar
//! con el cursor de prohibido. Desde 2.0.5 cada ventana registra un `IDropTarget` propio, el
//! mecanismo estándar: acepta `CF_HDROP` y lo convierte en un `WM_DROPFILES` hacia la propia
//! ventana, de modo que los manejadores existentes no cambian.

use std::cell::Cell;
use std::ffi::c_void;

use windows::Win32::Foundation::{GlobalFree, HGLOBAL, HWND, LPARAM, POINTL, WPARAM};
use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop,
    ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_DROPFILES};
use windows::core::{Ref, Result, implement};

thread_local! {
    static OLE_READY: Cell<bool> = const { Cell::new(false) };
}

/// Registra la ventana como destino de arrastre OLE. Inicializa OLE en el hilo la primera vez.
pub fn register(hwnd: HWND) {
    OLE_READY.with(|ready| {
        if !ready.get() {
            // S_FALSE si COM ya estaba iniciado en apartamento: OLE se inicializa igualmente.
            if let Err(e) = unsafe { OleInitialize(None) } {
                log_warn!("OleInitialize: {e}");
            }
            ready.set(true);
        }
    });
    let target: IDropTarget = DropTarget { hwnd, accepts: Cell::new(false) }.into();
    unsafe {
        match RegisterDragDrop(hwnd, &target) {
            Ok(()) => log_info!("drop target registered for {:?}", hwnd),
            Err(e) => log_warn!("RegisterDragDrop: {e}"),
        }
    }
}

/// Quita el registro antes de destruir la ventana. Inocuo si no estaba registrada.
pub fn revoke(hwnd: HWND) {
    unsafe {
        let _ = RevokeDragDrop(hwnd);
    }
}

#[implement(IDropTarget)]
struct DropTarget {
    hwnd: HWND,
    accepts: Cell<bool>,
}

fn hdrop_format() -> FORMATETC {
    FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 }
}

/// Copia o enlace según lo que permita el origen; nada si no hay archivos.
fn effect_for(accepts: bool, allowed: DROPEFFECT) -> DROPEFFECT {
    if !accepts {
        DROPEFFECT_NONE
    } else if allowed.0 & DROPEFFECT_COPY.0 != 0 {
        DROPEFFECT_COPY
    } else if allowed.0 & DROPEFFECT_LINK.0 != 0 {
        DROPEFFECT_LINK
    } else {
        DROPEFFECT_NONE
    }
}

/// Duplica el bloque `DROPFILES` del origen: el que recibe `WM_DROPFILES` lo libera con `DragFinish`.
unsafe fn clone_hglobal(src: HGLOBAL) -> Option<HGLOBAL> {
    unsafe {
        let size = GlobalSize(src);
        if size == 0 {
            return None;
        }
        let from = GlobalLock(src);
        if from.is_null() {
            return None;
        }
        let dst = match GlobalAlloc(GMEM_MOVEABLE, size) {
            Ok(h) => h,
            Err(_) => {
                let _ = GlobalUnlock(src);
                return None;
            }
        };
        let to = GlobalLock(dst);
        if to.is_null() {
            let _ = GlobalFree(Some(dst));
            let _ = GlobalUnlock(src);
            return None;
        }
        std::ptr::copy_nonoverlapping(from as *const u8, to as *mut u8, size);
        let _ = GlobalUnlock(dst);
        let _ = GlobalUnlock(src);
        Some(dst)
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let accepts = data.as_ref().map(|d| unsafe { d.QueryGetData(&hdrop_format()).is_ok() }).unwrap_or(false);
        self.accepts.set(accepts);
        unsafe {
            let chosen = effect_for(accepts, *effect);
            log_info!("drag enter: files={accepts} allowed={:#x} effect={:#x}", (*effect).0, chosen.0);
            *effect = chosen;
        }
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = effect_for(self.accepts.get(), *effect);
        }
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.accepts.set(false);
        Ok(())
    }

    fn Drop(&self, data: Ref<IDataObject>, _keys: MODIFIERKEYS_FLAGS, _pt: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        let mut delivered = false;
        if let Some(d) = data.as_ref() {
            if let Ok(mut medium) = unsafe { d.GetData(&hdrop_format()) } {
                if medium.tymed == TYMED_HGLOBAL.0 as u32 {
                    if let Some(copy) = unsafe { clone_hglobal(medium.u.hGlobal) } {
                        unsafe {
                            // Mismo proceso: el HGLOBAL llega intacto y el manejador lo libera con DragFinish.
                            if PostMessageW(Some(self.hwnd), WM_DROPFILES, WPARAM(copy.0 as usize), LPARAM(0)).is_ok() {
                                delivered = true;
                            } else {
                                let _ = GlobalFree(Some(copy));
                            }
                        }
                    }
                }
                unsafe { ReleaseStgMedium(&mut medium) };
            }
        }
        self.accepts.set(false);
        log_info!("drop: delivered={delivered}");
        unsafe {
            *effect = if delivered { effect_for(true, *effect) } else { DROPEFFECT_NONE };
        }
        Ok(())
    }
}

#[allow(dead_code)]
fn _assert_pointer_size(_: *const c_void) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_prefers_copy_then_link() {
        assert_eq!(effect_for(true, DROPEFFECT(DROPEFFECT_COPY.0 | DROPEFFECT_LINK.0)), DROPEFFECT_COPY);
        assert_eq!(effect_for(true, DROPEFFECT_LINK), DROPEFFECT_LINK);
        assert_eq!(effect_for(true, DROPEFFECT_NONE), DROPEFFECT_NONE);
        assert_eq!(effect_for(false, DROPEFFECT_COPY), DROPEFFECT_NONE);
    }

    #[test]
    fn clone_hglobal_copies_bytes() {
        unsafe {
            let src = GlobalAlloc(GMEM_MOVEABLE, 8).unwrap();
            let p = GlobalLock(src) as *mut u8;
            for i in 0..8 {
                *p.add(i) = i as u8 * 3;
            }
            let _ = GlobalUnlock(src);
            let dst = clone_hglobal(src).unwrap();
            assert_ne!(dst.0, src.0);
            let q = GlobalLock(dst) as *const u8;
            for i in 0..8 {
                assert_eq!(*q.add(i), i as u8 * 3);
            }
            let _ = GlobalUnlock(dst);
            let _ = GlobalFree(Some(dst));
            let _ = GlobalFree(Some(src));
        }
    }
}
