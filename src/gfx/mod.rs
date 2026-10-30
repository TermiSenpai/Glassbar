//! Gráficos: dispositivos, animaciones de DirectComposition, dibujo con Direct2D y cristal.

pub mod anim;
pub mod backdrop;
pub mod device;
pub mod draw;

pub use device::Gfx;

use windows_numerics::{Matrix3x2, Vector2};

pub fn pt(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

pub fn translation(x: f32, y: f32) -> Matrix3x2 {
    Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: x, M32: y }
}

pub fn scale_translate(sx: f32, sy: f32, x: f32, y: f32) -> Matrix3x2 {
    Matrix3x2 { M11: sx, M12: 0.0, M21: 0.0, M22: sy, M31: x, M32: y }
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
