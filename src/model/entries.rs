//! Entradas de la barra y su persistencia en `entries.json`.

use super::LoadOutcome;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum IconSource {
    Custom { file: String },
    File,
    None,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub icon: IconSource,
    #[serde(default)]
    pub created_at: String,
    /// Argumentos heredados de un acceso directo resuelto. No se editan en la interfaz.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub args: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Entry {
    /// Entrada nueva para un archivo. Un `.lnk` se resuelve a su destino, con argumentos y directorio.
    pub fn for_file(path: &Path) -> Entry {
        let (target, args, working_dir) = crate::shortcut::effective_target(path);
        Entry {
            id: new_id(),
            name: default_name(path),
            icon: default_icon(&target),
            path: target,
            created_at: crate::fsx::now_iso(),
            args,
            working_dir,
            extra: Default::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entries {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub entries: Vec<Entry>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn one() -> u32 {
    1
}

pub const SUPPORTED_EXT: [&str; 4] = ["bat", "cmd", "exe", "lnk"];

impl Default for Entries {
    fn default() -> Self {
        Self::new()
    }
}

impl Entries {
    pub fn new() -> Self {
        Entries { version: 1, entries: Vec::new(), extra: Default::default() }
    }

    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn load(path: &Path) -> LoadOutcome<Entries> {
        super::load_json(path)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::fsx::write_atomic(path, self.to_json().as_bytes())
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn add(&mut self, e: Entry) {
        self.entries.push(e);
    }

    pub fn remove(&mut self, id: &str) -> Option<Entry> {
        let i = self.index_of(id)?;
        Some(self.entries.remove(i))
    }

    pub fn move_left(&mut self, id: &str) -> bool {
        match self.index_of(id) {
            Some(i) if i > 0 => {
                self.entries.swap(i, i - 1);
                true
            }
            _ => false,
        }
    }

    pub fn move_right(&mut self, id: &str) -> bool {
        match self.index_of(id) {
            Some(i) if i + 1 < self.entries.len() => {
                self.entries.swap(i, i + 1);
                true
            }
            _ => false,
        }
    }

    pub fn ids(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.id.as_str()).collect()
    }
}

fn ext_lower(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default()
}

pub fn is_supported(path: &Path) -> bool {
    SUPPORTED_EXT.contains(&ext_lower(path).as_str())
}

pub fn default_name(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string()
}

pub fn default_icon(path: &Path) -> IconSource {
    match ext_lower(path).as_str() {
        "exe" | "lnk" => IconSource::File,
        _ => IconSource::None,
    }
}

/// UUID v4 en minúsculas con guiones.
pub fn new_id() -> String {
    use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};
    let mut b = [0u8; 16];
    let ok = unsafe { BCryptGenRandom(None, &mut b, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.is_ok();
    if !ok {
        // Respaldo sin criptografía: suficiente para un identificador local.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let p = std::process::id() as u128;
        let v = t ^ (p << 64) ^ (&b as *const _ as u128);
        b = v.to_le_bytes();
    }
    b[6] = (b[6] & 0x0F) | 0x40;
    b[8] = (b[8] & 0x3F) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}{}{}{}-{}{}-{}{}-{}{}-{}{}{}{}{}{}", h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8], h[9], h[10], h[11], h[12], h[13], h[14], h[15])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(id: &str) -> Entry {
        Entry { id: id.into(), name: id.into(), path: PathBuf::from(format!("C:\\{id}.bat")), icon: IconSource::None, created_at: String::new(), args: String::new(), working_dir: None, extra: Default::default() }
    }

    #[test]
    fn roundtrip_keeps_unknown_fields() {
        let json = r#"{"version":1,"future":true,"entries":[{"id":"a","name":"A","path":"C:\\a.exe","icon":{"kind":"file"},"created_at":"2026-01-01T00:00:00Z","color":"red"}]}"#;
        let es = Entries::parse(json).unwrap();
        assert_eq!(es.entries[0].icon, IconSource::File);
        let back = es.to_json();
        assert!(back.contains("\"future\": true"));
        assert!(back.contains("\"color\": \"red\""));
    }

    #[test]
    fn custom_icon_serializes_with_kind_and_file() {
        let mut es = Entries::new();
        let mut x = e("a");
        x.icon = IconSource::Custom { file: "a.png".into() };
        es.add(x);
        let j = es.to_json();
        assert!(j.contains("\"kind\": \"custom\"") && j.contains("\"file\": \"a.png\""));
        let back = Entries::parse(&j).unwrap();
        assert_eq!(back.entries[0].icon, IconSource::Custom { file: "a.png".into() });
    }

    #[test]
    fn move_left_and_right_respect_bounds() {
        let mut es = Entries::new();
        for id in ["a", "b", "c"] {
            es.add(e(id));
        }
        assert!(!es.move_left("a"));
        assert!(es.move_right("a"));
        assert_eq!(es.ids(), ["b", "a", "c"]);
        assert!(!es.move_right("c"));
        assert!(es.move_left("c"));
        assert_eq!(es.ids(), ["b", "c", "a"]);
    }

    #[test]
    fn remove_returns_entry() {
        let mut es = Entries::new();
        es.add(e("a"));
        assert_eq!(es.remove("a").unwrap().id, "a");
        assert!(es.remove("a").is_none());
    }

    #[test]
    fn defaults_by_extension() {
        assert!(is_supported(Path::new("x.BAT")) && is_supported(Path::new("x.lnk")));
        assert!(!is_supported(Path::new("x.txt")));
        assert_eq!(default_name(Path::new("C:\\dir\\Mi Script.cmd")), "Mi Script");
        assert_eq!(default_icon(Path::new("a.exe")), IconSource::File);
        assert_eq!(default_icon(Path::new("a.cmd")), IconSource::None);
    }

    #[test]
    fn new_id_is_uuid_shaped_and_unique() {
        let a = new_id();
        let b = new_id();
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "4");
        assert_ne!(a, b);
    }

    #[test]
    fn load_reports_missing_and_corrupt() {
        let d = std::env::temp_dir().join(format!("glassbar-entries-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(matches!(Entries::load(&d.join("none.json")), LoadOutcome::Missing));
        std::fs::write(d.join("bad.json"), "{").unwrap();
        assert!(matches!(Entries::load(&d.join("bad.json")), LoadOutcome::Corrupt(_)));
    }
}
