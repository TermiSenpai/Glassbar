//! Textos de la interfaz en español e inglés.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

pub const ES: &str = include_str!("../assets/strings/es.json");
pub const EN: &str = include_str!("../assets/strings/en.json");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub fn from_primary_lang_id(primary: u16) -> Lang {
        if primary == 0x0A { Lang::Es } else { Lang::En }
    }

    pub fn system() -> Lang {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        let id = unsafe { GetUserDefaultUILanguage() };
        Lang::from_primary_lang_id(id & 0x3FF)
    }
}

static TABLES: OnceLock<HashMap<Lang, HashMap<String, String>>> = OnceLock::new();
static CURRENT: RwLock<Lang> = RwLock::new(Lang::En);

fn tables() -> &'static HashMap<Lang, HashMap<String, String>> {
    TABLES.get_or_init(|| {
        let parse = |s: &str| serde_json::from_str::<HashMap<String, String>>(s).expect("strings json");
        HashMap::from([(Lang::Es, parse(ES)), (Lang::En, parse(EN))])
    })
}

pub fn set_lang(l: Lang) {
    if let Ok(mut c) = CURRENT.write() {
        *c = l;
    }
}

pub fn lang() -> Lang {
    CURRENT.read().map(|c| *c).unwrap_or(Lang::En)
}

/// Texto para la clave en el idioma activo. Devuelve la clave si no existe.
pub fn t(key: &str) -> &'static str {
    let l = lang();
    match tables().get(&l).and_then(|m| m.get(key)) {
        Some(s) => s.as_str(),
        None => Box::leak(key.to_string().into_boxed_str()),
    }
}

/// Como `t`, sustituyendo `{0}`, `{1}`… por los argumentos.
pub fn tf(key: &str, args: &[&str]) -> String {
    let mut s = t(key).to_string();
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), a);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// El idioma activo es global; los tests que lo cambian se serializan.
    static LANG_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn all_languages_share_keys_and_have_no_empty_values() {
        let es: serde_json::Map<String, serde_json::Value> = serde_json::from_str(ES).unwrap();
        let en: serde_json::Map<String, serde_json::Value> = serde_json::from_str(EN).unwrap();
        let mut ks: Vec<_> = es.keys().collect();
        let mut kn: Vec<_> = en.keys().collect();
        ks.sort();
        kn.sort();
        assert_eq!(ks, kn, "key sets differ");
        for (k, v) in es.iter().chain(en.iter()) {
            assert!(!v.as_str().unwrap().is_empty(), "empty value for {k}");
        }
    }

    #[test]
    fn t_returns_key_when_missing() {
        let _g = LANG_LOCK.lock().unwrap();
        set_lang(Lang::En);
        assert_eq!(t("nope.missing"), "nope.missing");
        assert_eq!(t("settings.title"), "Settings");
    }

    #[test]
    fn tf_substitutes_placeholders() {
        let _g = LANG_LOCK.lock().unwrap();
        set_lang(Lang::Es);
        assert_eq!(tf("settings.version", &["1.2.3"]), "Versión 1.2.3");
        set_lang(Lang::En);
    }

    #[test]
    fn lang_from_lcid_prefers_spanish_for_es() {
        assert_eq!(Lang::from_primary_lang_id(0x0A), Lang::Es);
        assert_eq!(Lang::from_primary_lang_id(0x09), Lang::En);
        assert_eq!(Lang::from_primary_lang_id(0x07), Lang::En);
    }
}
