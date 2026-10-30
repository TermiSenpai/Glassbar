//! Paleta, tema del sistema, color de acento y "reducir movimiento".

use crate::model::settings::ThemeSetting;
use std::ffi::c_void;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba8(r: u8, g: u8, b: u8, a: f32) -> Color {
        Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a }
    }
    pub const fn white(a: f32) -> Color {
        Color { r: 1.0, g: 1.0, b: 1.0, a }
    }
    pub const fn black(a: f32) -> Color {
        Color { r: 0.0, g: 0.0, b: 0.0, a }
    }
    pub fn with_alpha(self, a: f32) -> Color {
        Color { a, ..self }
    }
    pub fn to_d2d(&self) -> D2D1_COLOR_F {
        D2D1_COLOR_F { r: self.r, g: self.g, b: self.b, a: self.a }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub dark: bool,
    pub plate_tint: Color,
    pub plate_border: Color,
    pub cell_hover: Color,
    pub cell_press: Color,
    pub separator: Color,
    pub text: Color,
    pub text_dim: Color,
    pub accent: Color,
    pub glyph: Color,
    pub missing_badge: Color,
    pub error: Color,
    pub menu_bg: Color,
    pub tooltip_bg: Color,
    pub card_bg: Color,
    pub card_border: Color,
    pub field_bg: Color,
    pub field_border: Color,
    pub window_solid: Color,
    pub toggle_knob_on: Color,
    pub toggle_off_border: Color,
    pub toggle_knob_off: Color,
    pub button_bg: Color,
    pub button_border: Color,
}

pub fn palette_for(dark: bool, accent: Color) -> Palette {
    if dark {
        Palette {
            dark,
            plate_tint: Color::rgba8(40, 40, 46, 0.62),
            plate_border: Color::white(0.09),
            cell_hover: Color::white(0.10),
            cell_press: Color::white(0.05),
            separator: Color::white(0.14),
            text: Color::rgba8(0xF0, 0xF0, 0xF4, 1.0),
            text_dim: Color::rgba8(0x9A, 0x9A, 0xA3, 1.0),
            accent,
            glyph: Color::rgba8(0xE8, 0xE8, 0xEE, 1.0),
            missing_badge: Color::rgba8(0xFF, 0x6B, 0x57, 1.0),
            error: Color::rgba8(0xFF, 0x8A, 0x7A, 1.0),
            menu_bg: Color::rgba8(40, 40, 46, 0.72),
            tooltip_bg: Color::rgba8(32, 32, 38, 0.86),
            card_bg: Color::white(0.045),
            card_border: Color::white(0.06),
            field_bg: Color::black(0.25),
            field_border: Color::white(0.09),
            window_solid: Color::rgba8(0x20, 0x20, 0x24, 1.0),
            toggle_knob_on: Color::rgba8(0x1B, 0x1B, 0x1F, 1.0),
            toggle_off_border: Color::rgba8(0x8A, 0x8A, 0x93, 1.0),
            toggle_knob_off: Color::rgba8(0xCF, 0xCF, 0xD4, 1.0),
            button_bg: Color::white(0.06),
            button_border: Color::white(0.09),
        }
    } else {
        Palette {
            dark,
            plate_tint: Color::rgba8(249, 249, 249, 0.70),
            plate_border: Color::black(0.08),
            cell_hover: Color::black(0.06),
            cell_press: Color::black(0.04),
            separator: Color::black(0.12),
            text: Color::rgba8(0x1D, 0x1D, 0x1F, 1.0),
            text_dim: Color::rgba8(0x6E, 0x6E, 0x73, 1.0),
            accent,
            glyph: Color::rgba8(0x2B, 0x2B, 0x30, 1.0),
            missing_badge: Color::rgba8(0xE0, 0x4E, 0x3A, 1.0),
            error: Color::rgba8(0xC4, 0x2B, 0x1C, 1.0),
            menu_bg: Color::rgba8(249, 249, 249, 0.82),
            tooltip_bg: Color::rgba8(252, 252, 252, 0.90),
            card_bg: Color::black(0.03),
            card_border: Color::black(0.06),
            field_bg: Color::black(0.05),
            field_border: Color::black(0.10),
            window_solid: Color::rgba8(0xF3, 0xF3, 0xF6, 1.0),
            toggle_knob_on: Color::white(1.0),
            toggle_off_border: Color::rgba8(0x86, 0x86, 0x8B, 1.0),
            toggle_knob_off: Color::rgba8(0x5D, 0x5D, 0x62, 1.0),
            button_bg: Color::black(0.04),
            button_border: Color::black(0.08),
        }
    }
}

/// `AppsUseLightTheme == 0` → oscuro. Sin valor → oscuro.
pub fn system_dark() -> bool {
    use windows::Win32::System::Registry::*;
    use windows::core::w;
    let mut val: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut val as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    if r.is_ok() { val == 0 } else { true }
}

/// Color de acento de Windows, con respaldo.
pub fn accent(dark: bool) -> Color {
    use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
    let mut argb: u32 = 0;
    let mut opaque = windows::core::BOOL(0);
    if unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) }.is_ok() && argb != 0 {
        let c = Color::rgba8(((argb >> 16) & 0xFF) as u8, ((argb >> 8) & 0xFF) as u8, (argb & 0xFF) as u8, 1.0);
        // En oscuro, el acento se aclara para que funcione como relleno de interruptores.
        return if dark { lighten(c, 0.35) } else { c };
    }
    if dark { Color::rgba8(0x60, 0xCD, 0xFF, 1.0) } else { Color::rgba8(0x00, 0x67, 0xC0, 1.0) }
}

pub fn lighten(c: Color, t: f32) -> Color {
    Color { r: c.r + (1.0 - c.r) * t, g: c.g + (1.0 - c.g) * t, b: c.b + (1.0 - c.b) * t, a: c.a }
}

/// `SPI_GETCLIENTAREAANIMATION == FALSE` → el usuario pidió reducir movimiento.
pub fn reduce_motion() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let mut on = windows::core::BOOL(1);
    let r = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, Some(&mut on as *mut _ as *mut c_void), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    r.is_ok() && !on.as_bool()
}

pub fn palette(setting: ThemeSetting) -> Palette {
    let dark = match setting {
        ThemeSetting::System => system_dark(),
        ThemeSetting::Light => false,
        ThemeSetting::Dark => true,
    };
    palette_for(dark, accent(dark))
}

pub fn animations_enabled(settings_animations: bool) -> bool {
    settings_animations && !reduce_motion()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_follow_setting() {
        let acc = Color::rgba8(0, 0, 255, 1.0);
        assert!(palette_for(true, acc).dark);
        assert!(!palette_for(false, acc).dark);
        assert_eq!(palette_for(true, acc).accent, acc);
    }

    #[test]
    fn color_conversion() {
        let c = Color::rgba8(255, 0, 0, 1.0).to_d2d();
        assert_eq!(c.r, 1.0);
        assert_eq!(c.g, 0.0);
        let l = lighten(Color::black(1.0), 0.5);
        assert!((l.r - 0.5).abs() < 1e-6);
    }
}
