//! Versiones, análisis de la redirección de `latest`, `SHA256SUMS` y SHA‑256 puro.

pub const OWNER: &str = "TermiSenpai";
pub const REPO: &str = "Glassbar";
pub const ASSET: &str = "Glassbar.exe";
pub const SUMS: &str = "SHA256SUMS";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Version(pub u64, pub u64, pub u64);

impl Version {
    /// Acepta `1.2.3` y `v1.2.3`. Rechaza prerreleases y metadatos.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        if s.contains(['-', '+']) {
            return None;
        }
        let mut it = s.split('.');
        let a = it.next()?.parse().ok()?;
        let b = it.next()?.parse().ok()?;
        let c = it.next()?.parse().ok()?;
        if it.next().is_some() {
            return None;
        }
        Some(Version(a, b, c))
    }

    pub fn current() -> Version {
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version(0, 0, 0))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

pub fn latest_url() -> String {
    format!("https://github.com/{OWNER}/{REPO}/releases/latest")
}

pub fn asset_url(v: Version, name: &str) -> String {
    format!("https://github.com/{OWNER}/{REPO}/releases/download/v{v}/{name}")
}

pub fn release_page_url(v: Version) -> String {
    format!("https://github.com/{OWNER}/{REPO}/releases/tag/v{v}")
}

/// `.../releases/tag/v1.2.3` → 1.2.3
pub fn parse_latest_location(location: &str) -> Option<Version> {
    let idx = location.find("/releases/tag/")?;
    let tag = &location[idx + "/releases/tag/".len()..];
    let tag = tag.split(['/', '?', '#']).next()?;
    Version::parse(tag)
}

/// Líneas `hex  nombre` o `hex *nombre`.
pub fn parse_sha256sums(text: &str, filename: &str) -> Option<[u8; 32]> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let hex = parts.next()?;
        let name = parts.next().unwrap_or("").trim_start_matches('*');
        if name.eq_ignore_ascii_case(filename) && hex.len() == 64 {
            let mut out = [0u8; 32];
            for i in 0..32 {
                out[i] = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()?;
            }
            return Some(out);
        }
    }
    None
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA‑256 (FIPS 180-4), implementación pura.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut tail = Vec::with_capacity(128);
    let full = data.len() / 64 * 64;
    tail.extend_from_slice(&data[full..]);
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&bit_len.to_be_bytes());

    let mut process = |chunk: &[u8]| {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let big_s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(big_s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let big_s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = big_s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hv, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *hv = hv.wrapping_add(v);
        }
    };
    for chunk in data[..full].chunks_exact(64) {
        process(chunk);
    }
    for chunk in tail.chunks_exact(64) {
        process(chunk);
    }
    let mut out = [0u8; 32];
    for (i, v) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// `AAAA-MM-DDTHH:MM:SSZ` → segundos Unix.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 19 {
        return None;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |a: usize, e: usize| s[a..e].parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0, 4)?, num(5, 7)?, num(8, 10)?, num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se)
}

pub fn hours_since(last_iso: &str, now_iso: &str) -> Option<i64> {
    Some((parse_iso(now_iso)? - parse_iso(last_iso)?) / 3600)
}

/// Sin comprobación previa o fecha ilegible → comprobar. Si no, cada 24 h.
pub fn should_check(last_check_iso: Option<&str>, now_iso: &str) -> bool {
    match last_check_iso.and_then(|l| hours_since(l, now_iso)) {
        Some(h) => h >= 24 || h < 0,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_and_order() {
        assert_eq!(Version::parse("v1.2.3"), Some(Version(1, 2, 3)));
        assert_eq!(Version::parse("1.10.0"), Some(Version(1, 10, 0)));
        assert!(Version::parse("1.2").is_none() && Version::parse("1.2.3-beta").is_none());
        assert!(Version(1, 10, 0) > Version(1, 9, 9));
        assert_eq!(Version(1, 2, 3).to_string(), "1.2.3");
        assert_eq!(Version::current(), Version(2, 0, 5));
    }

    #[test]
    fn location_parsing() {
        assert_eq!(parse_latest_location("https://github.com/TermiSenpai/Glassbar/releases/tag/v1.4.0"), Some(Version(1, 4, 0)));
        assert!(parse_latest_location("https://github.com/TermiSenpai/Glassbar/releases").is_none());
    }

    #[test]
    fn sha256sums_parsing() {
        let t = "aaaa  other.exe\n0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef *Glassbar.exe\n";
        assert_eq!(parse_sha256sums(t, "Glassbar.exe").unwrap()[0], 0x01);
        assert!(parse_sha256sums(t, "nope").is_none());
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = vec![b'a'; 1_000_000];
        assert_eq!(hex(&sha256(&long)), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
        let exactly_64 = [b'x'; 64];
        assert_eq!(hex(&sha256(&exactly_64)).len(), 64);
    }

    #[test]
    fn should_check_every_24h() {
        assert!(should_check(None, "2026-10-30T10:00:00Z"));
        assert!(!should_check(Some("2026-10-30T00:00:00Z"), "2026-10-30T10:00:00Z"));
        assert!(should_check(Some("2026-10-29T09:00:00Z"), "2026-10-30T10:00:00Z"));
        assert!(should_check(Some("garbage"), "2026-10-30T10:00:00Z"));
        assert_eq!(hours_since("2026-10-29T09:00:00Z", "2026-10-30T10:00:00Z"), Some(25));
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2000-03-01T00:00:00Z"), Some(951868800));
    }

    #[test]
    fn urls() {
        assert_eq!(asset_url(Version(1, 2, 3), ASSET), "https://github.com/TermiSenpai/Glassbar/releases/download/v1.2.3/Glassbar.exe");
        assert_eq!(latest_url(), "https://github.com/TermiSenpai/Glassbar/releases/latest");
        assert_eq!(release_page_url(Version(2, 0, 0)), "https://github.com/TermiSenpai/Glassbar/releases/tag/v2.0.0");
    }
}
