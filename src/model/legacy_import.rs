//! Importación automática de los datos de BatLauncher en el primer arranque.

use super::entries::{Entries, Entry, IconSource, default_icon, new_id};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct LegacyData {
    #[serde(default)]
    entries: Vec<LegacyEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    file_path: String,
    #[serde(default)]
    icon_path: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

pub struct Converted {
    pub entry: Entry,
    pub copy_icon_from: Option<PathBuf>,
}

fn normalize_date(s: Option<String>) -> String {
    match s {
        Some(d) if d.len() >= 19 => {
            let base = &d[..19];
            format!("{base}Z")
        }
        _ => crate::fsx::now_iso(),
    }
}

/// Convierte el JSON de BatLauncher. `icon_exists` permite testear sin disco.
pub fn convert(data_json: &str, icon_exists: &dyn Fn(&Path) -> bool) -> Result<Vec<Converted>, String> {
    let data: LegacyData = serde_json::from_str(data_json).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for le in data.entries {
        if le.file_path.is_empty() {
            continue;
        }
        let id = if le.id.is_empty() { new_id() } else { le.id };
        let path = PathBuf::from(&le.file_path);
        let name = if le.name.is_empty() { super::entries::default_name(&path) } else { le.name };
        let (icon, copy_icon_from) = match le.icon_path {
            Some(ip) if !ip.is_empty() && icon_exists(Path::new(&ip)) => {
                let ext = Path::new(&ip).extension().and_then(|e| e.to_str()).unwrap_or("png").to_ascii_lowercase();
                (IconSource::Custom { file: format!("{id}.{ext}") }, Some(PathBuf::from(ip)))
            }
            _ => (default_icon(&path), None),
        };
        out.push(Converted {
            entry: Entry { id, name, path, icon, created_at: normalize_date(le.created_at), extra: Default::default() },
            copy_icon_from,
        });
    }
    Ok(out)
}

/// Lee `<legacy_dir>/data.json`, copia los iconos a `<icons_dir>/<id>.<ext>` y devuelve las entradas.
pub fn run(legacy_dir: &Path, icons_dir: &Path) -> Result<Entries, String> {
    let json = std::fs::read_to_string(legacy_dir.join("data.json")).map_err(|e| e.to_string())?;
    let converted = convert(&json, &|p| p.exists())?;
    let _ = std::fs::create_dir_all(icons_dir);
    let mut es = Entries::new();
    for c in converted {
        let mut entry = c.entry;
        if let (Some(src), IconSource::Custom { file }) = (&c.copy_icon_from, &entry.icon) {
            if std::fs::copy(src, icons_dir.join(file)).is_err() {
                entry.icon = default_icon(&entry.path);
            }
        }
        es.add(entry);
    }
    Ok(es)
}

pub fn should_import(entries_path: &Path, legacy_dir: &Path) -> bool {
    !entries_path.exists() && legacy_dir.join("data.json").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../tests/fixtures/legacy_data.json");

    #[test]
    fn convert_maps_fields_and_icon_kinds() {
        let exists = |p: &Path| p.ends_with("11111111-1111-4111-8111-111111111111.png");
        let out = convert(FIXTURE, &exists).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].entry.icon, IconSource::Custom { file: "11111111-1111-4111-8111-111111111111.png".into() });
        assert!(out[0].copy_icon_from.is_some());
        assert_eq!(out[0].entry.created_at, "2024-11-14T07:57:00Z");
        assert_eq!(out[1].entry.icon, IconSource::File);
        assert_eq!(out[2].entry.icon, IconSource::None);
        assert_eq!(out[2].entry.path, PathBuf::from("F:\\apps\\iniciar.bat"));
    }

    #[test]
    fn run_copies_icons_and_leaves_legacy_untouched() {
        let d = std::env::temp_dir().join(format!("glassbar-legacy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let legacy = d.join("BatLauncher");
        let icons = d.join("icons");
        std::fs::create_dir_all(legacy.join("icons")).unwrap();
        std::fs::create_dir_all(&icons).unwrap();
        let src_icon = legacy.join("icons").join("11111111-1111-4111-8111-111111111111.png");
        std::fs::write(&src_icon, [1, 2, 3]).unwrap();
        let prefix = legacy.join("icons").to_string_lossy().replace('\\', "\\\\") + "\\\\";
        let json = FIXTURE.replace("ICONS\\\\", &prefix);
        std::fs::write(legacy.join("data.json"), json).unwrap();
        let es = run(&legacy, &icons).unwrap();
        assert_eq!(es.entries.len(), 3);
        assert_eq!(std::fs::read(icons.join("11111111-1111-4111-8111-111111111111.png")).unwrap(), [1, 2, 3]);
        assert!(src_icon.exists());
        assert_eq!(es.entries[2].icon, IconSource::None);
    }

    #[test]
    fn should_import_only_when_entries_missing_and_legacy_present() {
        let d = std::env::temp_dir().join(format!("glassbar-legacy2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let legacy = d.join("BatLauncher");
        std::fs::create_dir_all(&legacy).unwrap();
        let entries = d.join("entries.json");
        assert!(!should_import(&entries, &legacy));
        std::fs::write(legacy.join("data.json"), "{}").unwrap();
        assert!(should_import(&entries, &legacy));
        std::fs::write(&entries, "{}").unwrap();
        assert!(!should_import(&entries, &legacy));
    }
}
