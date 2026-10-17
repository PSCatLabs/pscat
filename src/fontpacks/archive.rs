//! Unpacking a font pack: zstd (pure-Rust `ruzstd`) around a plain
//! ustar archive. The tar reader is a deliberately small one rather
//! than the `tar` crate: a pack holds only regular files, so anything
//! else (links, devices, pax/GNU extension headers, absolute or `..`
//! paths, duplicates) is refused outright instead of being interpreted.

use std::io::Read;

/// Caps that keep a hostile or corrupt pack from exhausting memory.
/// The largest real pack (`cjk-thai`) unpacks to about 24 MB.
pub const MAX_UNPACKED: u64 = 512 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 10_000;

/// One regular file from a pack.
#[derive(Debug)]
pub struct Entry {
    pub path: String,
    pub data: Vec<u8>,
}

/// Decompress a `.zst` stream, refusing output past [`MAX_UNPACKED`].
pub fn unzstd(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let dec = ruzstd::decoding::StreamingDecoder::new(bytes)
        .map_err(|e| format!("not a zstd stream: {e}"))?;
    let mut out = Vec::new();
    dec.take(MAX_UNPACKED + 1)
        .read_to_end(&mut out)
        .map_err(|e| format!("zstd decode failed: {e}"))?;
    if out.len() as u64 > MAX_UNPACKED {
        return Err(format!("pack unpacks to more than {MAX_UNPACKED} bytes"));
    }
    Ok(out)
}

/// A path inside a pack is safe to join under the install directory:
/// relative, `/`-separated, no empty, `.` or `..` components, no
/// backslashes, colons or NULs (Windows drive and stream syntax).
pub fn safe_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains(['\\', ':', '\0'])
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

fn octal(field: &[u8]) -> Result<u64, String> {
    let s: String = field
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).map_err(|_| format!("bad octal field {s:?} in tar header"))
}

fn cstr(field: &[u8]) -> Result<String, String> {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8(field[..end].to_vec()).map_err(|_| "non-UTF-8 name in tar header".into())
}

/// Read every entry of an uncompressed ustar archive.
pub fn untar(tar: &[u8]) -> Result<Vec<Entry>, String> {
    let mut out: Vec<Entry> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut total: u64 = 0;
    let mut pos = 0usize;
    loop {
        let header = tar
            .get(pos..pos + 512)
            .ok_or("tar archive ends without its end-of-archive marker")?;
        if header.iter().all(|&b| b == 0) {
            return Ok(out);
        }
        let stored = octal(&header[148..156])?;
        let sum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    u64::from(b)
                }
            })
            .sum();
        if sum != stored {
            return Err("tar header checksum mismatch".into());
        }
        if &header[257..262] != b"ustar" {
            return Err("not a ustar archive".into());
        }
        let name = cstr(&header[0..100])?;
        let prefix = cstr(&header[345..500])?;
        let path = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let kind = header[156];
        if kind != b'0' && kind != 0 {
            return Err(format!(
                "{path}: only regular files belong in a font pack (tar type {:?})",
                kind as char
            ));
        }
        if !safe_path(&path) {
            return Err(format!("unsafe path {path:?} in pack"));
        }
        if !seen.insert(path.clone()) {
            return Err(format!("{path} appears twice in the pack"));
        }
        let size = octal(&header[124..136])?;
        total = total.saturating_add(size);
        if total > MAX_UNPACKED || out.len() >= MAX_ENTRIES {
            return Err("pack is larger than any font pack should be".into());
        }
        let start = pos + 512;
        let end = start
            .checked_add(usize::try_from(size).map_err(|_| "tar entry too large")?)
            .ok_or("tar entry too large")?;
        let data = tar
            .get(start..end)
            .ok_or_else(|| format!("{path}: truncated in the archive"))?
            .to_vec();
        out.push(Entry { path, data });
        pos = start + (end - start).div_ceil(512) * 512;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Build a ustar archive the way scripts/font_packs.py does, for
    /// tests. `kind` lets a test plant a non-regular entry.
    pub(crate) fn tar_of(entries: &[(&str, &[u8], u8)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, data, kind) in entries {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..107].copy_from_slice(b"0000644");
            h[108..115].copy_from_slice(b"0000000");
            h[116..123].copy_from_slice(b"0000000");
            h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
            h[136..147].copy_from_slice(b"00000000000");
            h[156] = *kind;
            h[257..263].copy_from_slice(b"ustar\0");
            h[263..265].copy_from_slice(b"00");
            h[148..156].copy_from_slice(b"        ");
            let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
            h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(data);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.extend_from_slice(&[0u8; 1024]);
        out
    }

    pub(crate) fn zstd_of(bytes: &[u8]) -> Vec<u8> {
        ruzstd::encoding::compress_to_vec(bytes, ruzstd::encoding::CompressionLevel::Fastest)
    }

    #[test]
    fn round_trips_regular_files() {
        let tar = tar_of(&[
            ("A/a.ttf", b"font bytes", b'0'),
            ("manifest.json", b"{}", b'0'),
        ]);
        let got = untar(&unzstd(&zstd_of(&tar)).unwrap()).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].path, "A/a.ttf");
        assert_eq!(got[0].data, b"font bytes");
        assert_eq!(got[1].path, "manifest.json");
    }

    #[test]
    fn refuses_links_traversal_duplicates_and_garbage() {
        let link = tar_of(&[("A/a.ttf", b"", b'2')]);
        assert!(untar(&link).unwrap_err().contains("regular files"));
        let up = tar_of(&[("../evil", b"x", b'0')]);
        assert!(untar(&up).unwrap_err().contains("unsafe path"));
        let abs = tar_of(&[("/etc/x", b"x", b'0')]);
        assert!(untar(&abs).unwrap_err().contains("unsafe path"));
        let dup = tar_of(&[("a", b"x", b'0'), ("a", b"y", b'0')]);
        assert!(untar(&dup).unwrap_err().contains("twice"));
        let mut bad = tar_of(&[("a", b"x", b'0')]);
        bad[0] = b'b';
        assert!(untar(&bad).unwrap_err().contains("checksum"));
        let truncated = &tar_of(&[("a", &[7u8; 2000], b'0')])[..1024];
        assert!(untar(truncated).is_err());
        assert!(unzstd(b"definitely not zstd").is_err());
    }

    #[test]
    fn safe_path_rules() {
        assert!(safe_path("Bangers/Bangers-Regular.ttf"));
        assert!(safe_path("manifest.json"));
        for bad in ["", "/x", "a/../b", "a//b", "./a", "a\\b", "C:x", "a/"] {
            assert!(!safe_path(bad), "{bad:?} should be unsafe");
        }
    }
}
