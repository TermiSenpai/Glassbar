//! Modelo de datos: entradas, ajustes e importación de BatLauncher.

pub mod entries;
pub mod legacy_import;
pub mod settings;

use serde::de::DeserializeOwned;
use std::path::Path;

pub enum LoadOutcome<T> {
    Loaded(T),
    Missing,
    Corrupt(String),
}

pub fn load_json<T: DeserializeOwned>(path: &Path) -> LoadOutcome<T> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => LoadOutcome::Missing,
        Err(e) => LoadOutcome::Corrupt(e.to_string()),
        Ok(text) => match serde_json::from_str::<T>(&text) {
            Ok(v) => LoadOutcome::Loaded(v),
            Err(e) => LoadOutcome::Corrupt(e.to_string()),
        },
    }
}
