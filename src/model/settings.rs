//! Ajustes y su persistencia en `settings.json`.

use super::LoadOutcome;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hotkey {
    pub mods: Vec<String>,
    pub key: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LanguageSetting {
    #[default]
    System,
    Es,
    En,
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeSetting {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub language: LanguageSetting,
    #[serde(default)]
    pub resident: bool,
    #[serde(default)]
    pub start_with_windows: bool,
    #[serde(default)]
    pub hotkey: Option<Hotkey>,
    #[serde(default)]
    pub theme: ThemeSetting,
    #[serde(default = "yes")]
    pub animations: bool,
    #[serde(default = "yes")]
    pub update_check: bool,
    #[serde(default)]
    pub last_update_check: Option<String>,
    #[serde(default)]
    pub known_latest: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings::parse("{}").expect("default settings")
    }
}

impl Settings {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn load(path: &Path) -> LoadOutcome<Settings> {
        super::load_json(path)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::fsx::write_atomic(path, self.to_json().as_bytes())
    }

    pub fn effective_language(&self) -> crate::i18n::Lang {
        match self.language {
            LanguageSetting::System => crate::i18n::Lang::system(),
            LanguageSetting::Es => crate::i18n::Lang::Es,
            LanguageSetting::En => crate::i18n::Lang::En,
        }
    }
}

const NAMED_KEYS: &[(&str, u16)] = &[
    ("Space", 0x20),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Escape", 0x1B),
    ("Backspace", 0x08),
    ("Delete", 0x2E),
    ("Insert", 0x2D),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("Left", 0x25),
    ("Right", 0x27),
    ("Up", 0x26),
    ("Down", 0x28),
];

pub fn vk_name(vk: u16) -> String {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => (vk as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        0x60..=0x69 => format!("Numpad{}", vk - 0x60),
        _ => match NAMED_KEYS.iter().find(|(_, c)| *c == vk) {
            Some((n, _)) => n.to_string(),
            None => format!("Vk{vk:02X}"),
        },
    }
}

pub fn vk_from_name(name: &str) -> Option<u16> {
    if name.len() == 1 {
        let c = name.chars().next()?;
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Some(c as u16);
        }
        if c.is_ascii_lowercase() {
            return Some(c.to_ascii_uppercase() as u16);
        }
    }
    if let Some(n) = name.strip_prefix('F') {
        if let Ok(f) = n.parse::<u16>() {
            if (1..=24).contains(&f) {
                return Some(0x70 + f - 1);
            }
        }
    }
    if let Some(n) = name.strip_prefix("Numpad") {
        if let Ok(d) = n.parse::<u16>() {
            if d <= 9 {
                return Some(0x60 + d);
            }
        }
    }
    if let Some(h) = name.strip_prefix("Vk") {
        return u16::from_str_radix(h, 16).ok();
    }
    NAMED_KEYS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

fn is_modifier_vk(vk: u16) -> bool {
    matches!(vk, 0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

impl Hotkey {
    pub fn to_display(&self) -> String {
        let mut parts: Vec<String> = self
            .mods
            .iter()
            .map(|m| match m.as_str() {
                "win" => "Win".to_string(),
                "ctrl" => "Ctrl".to_string(),
                "alt" => "Alt".to_string(),
                "shift" => "Shift".to_string(),
                other => other.to_string(),
            })
            .collect();
        parts.push(self.key.clone());
        parts.join(" + ")
    }

    pub fn modifiers_flags(&self) -> u32 {
        self.mods.iter().fold(0, |acc, m| {
            acc | match m.as_str() {
                "win" => MOD_WIN,
                "ctrl" => MOD_CONTROL,
                "alt" => MOD_ALT,
                "shift" => MOD_SHIFT,
                _ => 0,
            }
        })
    }

    pub fn vk(&self) -> Option<u16> {
        vk_from_name(&self.key)
    }

    pub fn from_vk(mods: u32, vk: u16) -> Option<Hotkey> {
        if mods == 0 || is_modifier_vk(vk) {
            return None;
        }
        let mut names = Vec::new();
        if mods & MOD_WIN != 0 {
            names.push("win".to_string());
        }
        if mods & MOD_CONTROL != 0 {
            names.push("ctrl".to_string());
        }
        if mods & MOD_ALT != 0 {
            names.push("alt".to_string());
        }
        if mods & MOD_SHIFT != 0 {
            names.push("shift".to_string());
        }
        Some(Hotkey { mods: names, key: vk_name(vk) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty_object() {
        let s = Settings::parse("{}").unwrap();
        assert_eq!(s.language, LanguageSetting::System);
        assert!(!s.resident);
        assert!(s.animations);
        assert!(s.update_check);
        assert!(s.hotkey.is_none());
        assert_eq!(s.theme, ThemeSetting::System);
    }

    #[test]
    fn unknown_fields_survive() {
        let s = Settings::parse(r#"{"future":1}"#).unwrap();
        assert!(s.to_json().contains("\"future\": 1"));
        let back = Settings::parse(&s.to_json()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn hotkey_display_and_flags() {
        let h = Hotkey { mods: vec!["win".into(), "alt".into()], key: "Space".into() };
        assert_eq!(h.to_display(), "Win + Alt + Space");
        assert_eq!(h.modifiers_flags(), MOD_WIN | MOD_ALT);
        assert_eq!(h.vk(), Some(0x20));
    }

    #[test]
    fn hotkey_from_vk_rejects_bare_modifier() {
        assert!(Hotkey::from_vk(MOD_ALT, 0x12).is_none());
        assert!(Hotkey::from_vk(0, 0x4B).is_none());
        let h = Hotkey::from_vk(MOD_CONTROL | MOD_SHIFT, 0x4B).unwrap();
        assert_eq!(h.mods, vec!["ctrl", "shift"]);
        assert_eq!(h.key, "K");
    }

    #[test]
    fn vk_names_roundtrip() {
        for vk in [0x20u16, 0x41, 0x39, 0x70, 0x87, 0x60, 0x2E, 0xBA] {
            assert_eq!(vk_from_name(&vk_name(vk)), Some(vk), "vk {vk:#x}");
        }
        assert_eq!(vk_name(0xBA), "VkBA");
    }
}
