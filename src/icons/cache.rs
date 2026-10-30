//! `icons.cache`: mapas de bits BGRA premultiplicados por entrada y tamaño en píxeles.

use std::path::Path;

pub const MAGIC: &[u8; 4] = b"GBIC";
pub const FORMAT_VERSION: u16 = 1;
const ID_LEN: usize = 36;

#[derive(Clone, PartialEq, Debug)]
pub struct Record {
    pub id: String,
    pub fingerprint: u64,
    pub px: u16,
    pub bgra: Vec<u8>,
}

#[derive(Default, Clone, Debug)]
pub struct IconCache {
    pub records: Vec<Record>,
}

fn fixed_id(id: &str) -> [u8; ID_LEN] {
    let mut out = [b' '; ID_LEN];
    for (i, b) in id.bytes().take(ID_LEN).enumerate() {
        out[i] = b;
    }
    out
}

impl IconCache {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(16 + self.records.iter().map(|r| ID_LEN + 10 + r.bgra.len()).sum::<usize>());
        v.extend_from_slice(MAGIC);
        v.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        v.extend_from_slice(&(self.records.len() as u32).to_le_bytes());
        for r in &self.records {
            v.extend_from_slice(&fixed_id(&r.id));
            v.extend_from_slice(&r.fingerprint.to_le_bytes());
            v.extend_from_slice(&r.px.to_le_bytes());
            let expected = r.px as usize * r.px as usize * 4;
            if r.bgra.len() == expected {
                v.extend_from_slice(&r.bgra);
            } else {
                v.extend(std::iter::repeat_n(0u8, expected));
            }
        }
        v
    }

    pub fn decode(bytes: &[u8]) -> Option<IconCache> {
        if bytes.len() < 10 || &bytes[..4] != MAGIC {
            return None;
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != FORMAT_VERSION {
            return None;
        }
        let count = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
        let mut pos = 10;
        let mut records = Vec::with_capacity(count.min(1024));
        for _ in 0..count {
            if bytes.len() < pos + ID_LEN + 10 {
                return None;
            }
            let id = String::from_utf8_lossy(&bytes[pos..pos + ID_LEN]).trim_end().to_string();
            pos += ID_LEN;
            let fingerprint = u64::from_le_bytes(bytes[pos..pos + 8].try_into().ok()?);
            pos += 8;
            let px = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
            pos += 2;
            let len = px as usize * px as usize * 4;
            if bytes.len() < pos + len {
                return None;
            }
            records.push(Record { id, fingerprint, px, bgra: bytes[pos..pos + len].to_vec() });
            pos += len;
        }
        Some(IconCache { records })
    }

    pub fn get(&self, id: &str, fingerprint: u64, px: u16) -> Option<&Record> {
        self.records.iter().find(|r| r.id == id && r.fingerprint == fingerprint && r.px == px)
    }

    pub fn insert(&mut self, r: Record) {
        if let Some(existing) = self.records.iter_mut().find(|x| x.id == r.id && x.px == r.px) {
            *existing = r;
        } else {
            self.records.push(r);
        }
    }

    pub fn prune(&mut self, keep_ids: &[&str]) {
        self.records.retain(|r| keep_ids.contains(&r.id.as_str()));
    }

    pub fn load(path: &Path) -> IconCache {
        match std::fs::read(path) {
            Ok(bytes) => IconCache::decode(&bytes).unwrap_or_default(),
            Err(_) => IconCache::default(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::fsx::write_atomic(path, &self.encode())
    }
}

pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Huella del archivo de origen: ruta en minúsculas, fecha de modificación y tamaño. `None` si no existe.
pub fn fingerprint_for(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    let mut buf = path.to_string_lossy().to_lowercase().into_bytes();
    buf.extend_from_slice(&mtime.to_le_bytes());
    buf.extend_from_slice(&meta.len().to_le_bytes());
    Some(fnv1a(&buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(id: char, fp: u64, px: u16, fill: u8) -> Record {
        Record { id: id.to_string().repeat(36), fingerprint: fp, px, bgra: vec![fill; px as usize * px as usize * 4] }
    }

    #[test]
    fn roundtrip() {
        let mut c = IconCache::default();
        c.insert(rec('a', 7, 2, 9));
        c.insert(rec('b', 8, 3, 1));
        let d = IconCache::decode(&c.encode()).unwrap();
        assert_eq!(d.records, c.records);
    }

    #[test]
    fn rejects_bad_magic_version_and_truncated() {
        assert!(IconCache::decode(b"NOPE").is_none());
        let mut c = IconCache::default();
        c.insert(rec('a', 1, 2, 0));
        let mut b = c.encode();
        b.truncate(b.len() - 1);
        assert!(IconCache::decode(&b).is_none());
        let mut v = c.encode();
        v[4] = 9;
        assert!(IconCache::decode(&v).is_none());
    }

    #[test]
    fn get_requires_matching_fingerprint_and_px() {
        let mut c = IconCache::default();
        c.insert(rec('a', 1, 2, 0));
        let id = "a".repeat(36);
        assert!(c.get(&id, 1, 2).is_some());
        assert!(c.get(&id, 2, 2).is_none());
        assert!(c.get(&id, 1, 3).is_none());
    }

    #[test]
    fn insert_replaces_same_id_px_and_prune_drops_others() {
        let mut c = IconCache::default();
        c.insert(rec('a', 1, 2, 0));
        c.insert(rec('a', 2, 2, 1));
        c.insert(rec('b', 1, 2, 0));
        assert_eq!(c.records.len(), 2);
        let keep = "a".repeat(36);
        c.prune(&[keep.as_str()]);
        assert_eq!(c.records.len(), 1);
        assert_eq!(c.records[0].fingerprint, 2);
    }

    #[test]
    fn fnv1a_known_vector() {
        assert_eq!(fnv1a(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a(b"a"), 0xaf63dc4c8601ec8c);
    }

    #[test]
    fn fingerprint_changes_with_content_size() {
        let d = std::env::temp_dir().join(format!("glassbar-fp-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("x.png");
        std::fs::write(&p, b"1").unwrap();
        let f1 = fingerprint_for(&p).unwrap();
        std::fs::write(&p, b"12").unwrap();
        let f2 = fingerprint_for(&p).unwrap();
        assert_ne!(f1, f2);
        assert!(fingerprint_for(&d.join("nope")).is_none());
    }

    #[test]
    fn load_missing_or_invalid_gives_empty() {
        let d = std::env::temp_dir().join(format!("glassbar-cache-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert!(IconCache::load(&d.join("none")).records.is_empty());
        std::fs::write(d.join("bad"), b"xx").unwrap();
        assert!(IconCache::load(&d.join("bad")).records.is_empty());
    }
}
