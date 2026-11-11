//! `filenameforall`'s listing (issue #740): which file names match a
//! template, looked up where `run` and `(r) file` would look
//! (`crate::paths`, whose module doc has the search order).

use std::path::{Path, PathBuf};

use crate::error::PsError;

/// The names `filenameforall` yields for `template` (issue #740): file
/// names a program could open with `run` or `(r) file`, sorted, without
/// duplicates. An absolute template lists that directory; a relative one
/// lists the directory in every place the search order would look (module
/// doc), the embedded `lib/` copy included, and yields the relative name.
/// Wildcards (`*`, `?`, `\` to quote) match within the last component;
/// a template with one in a directory part matches nothing.
///
/// **Windows.** `\` is always the escape character, on every platform
/// (PLRM), so it cannot also separate directories: write `C:/dir/*.ps`, not
/// `C:\dir\*.ps` (each `\` quotes the next character, and the whole is one
/// name with no directory part, which matches nothing). `/` is the separator everywhere: the template is split
/// at its `/`s, Windows accepts them in paths, and the names come back with
/// the `/` they were given (a drive letter and a UNC `//server/share/` form
/// work as the directory part). Names match case-sensitively on every
/// platform. Patterns across several directories (`*/*.ps`) are not
/// supported, on any platform.
///
/// `poll` is called once per directory entry looked at, with the bytes the
/// entry adds to the result (0 when it doesn't match), so an interpreter
/// under execution limits can stop a listing of a huge directory.
///
/// Under read roots (issue #763, `crate::read_roots`) only directories
/// inside them are listed and only names that would open there are
/// yielded: a template with `..`, or an absolute one outside the roots,
/// yields nothing. That is no error: the PLRM gives `filenameforall` no
/// `invalidfileaccess`, and Ghostscript's `-dSAFER` skips files it may not
/// open the same way.
pub fn matching_names(
    template: &[u8],
    roots: Option<&crate::read_roots::ReadRoots>,
    poll: &mut dyn FnMut(usize) -> Result<(), PsError>,
) -> Result<Vec<Vec<u8>>, PsError> {
    let Ok(template) = std::str::from_utf8(template) else {
        return Ok(Vec::new());
    };
    let first_wild = template.find(['*', '?', '\\']).unwrap_or(template.len());
    let dir_end = template[..first_wild].rfind('/').map_or(0, |i| i + 1);
    let (dir, pattern) = template.split_at(dir_end);
    if pattern.is_empty() || pattern.contains('/') {
        return Ok(Vec::new());
    }
    if roots.is_some_and(|r| r.check_name(dir).is_err()) {
        return Ok(Vec::new());
    }
    let mut found = std::collections::BTreeSet::new();
    // Every entry looked at goes through here, skipped ones as `None`, so
    // each one is polled.
    let mut take = |file: Option<&str>| {
        let Some(file) = file else {
            return poll(0);
        };
        let hit = glob_match(pattern.as_bytes(), file.as_bytes());
        poll(if hit { dir.len() + file.len() } else { 0 })?;
        if hit {
            found.insert(format!("{dir}{file}").into_bytes());
        }
        Ok::<(), PsError>(())
    };
    let dirs: Vec<PathBuf> = if Path::new(dir).is_absolute() {
        vec![PathBuf::from(dir)]
    } else {
        let name = if dir.is_empty() { "." } else { dir };
        let r = crate::paths::process_roots();
        r.disk_candidates(name)
            .into_iter()
            .chain(r.checkout_candidates(name))
            .map(|(_, p)| p)
            .collect()
    };
    for d in dirs {
        if roots.is_some_and(|r| !r.contains_path(&d)) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries {
            // Regular files, through symlinks, as `run` would read them,
            // and under read roots only where they would let it.
            let opens = |p: &Path| match roots {
                None => true,
                Some(r) => matches!(r.admit(p), crate::read_roots::Admit::Open(_)),
            };
            let name = e
                .ok()
                .filter(|e| std::fs::metadata(e.path()).is_ok_and(|m| m.is_file()))
                .filter(|e| opens(&e.path()))
                .map(|e| e.file_name());
            take(name.as_ref().and_then(|n| n.to_str()))?;
        }
    }
    if !Path::new(dir).is_absolute() {
        for name in crate::paths::embedded_names() {
            if let Some(file) = name.strip_prefix(dir).filter(|f| !f.contains('/')) {
                take(Some(file))?;
            }
        }
    }
    Ok(found.into_iter().collect())
}

/// PostScript filename matching: `*` any run, `?` one byte, `\\` quotes
/// the next byte.
fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    // Where to resume after the last `*`: (pattern index, name index).
    let mut star: Option<(usize, usize)> = None;
    while n < name.len() {
        match pattern.get(p) {
            Some(b'*') => {
                star = Some((p + 1, n));
                p += 1;
                continue;
            }
            Some(b'?') => {
                p += 1;
                n += 1;
                continue;
            }
            Some(&c) => {
                let (lit, width) = if c == b'\\' {
                    (pattern.get(p + 1).copied(), 2)
                } else {
                    (Some(c), 1)
                };
                if lit == Some(name[n]) {
                    p += width;
                    n += 1;
                    continue;
                }
            }
            None => {}
        }
        match star {
            Some((sp, sn)) => {
                p = sp;
                n = sn + 1;
                star = Some((sp, sn + 1));
            }
            None => return false,
        }
    }
    pattern[p..].iter().all(|&c| c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_match_follows_postscript_rules() {
        let m = |p: &str, n: &str| glob_match(p.as_bytes(), n.as_bytes());
        assert!(m("*", "") && m("*", "abc") && m("a*c", "abbbc") && m("a*c", "ac"));
        assert!(m("?.ps", "a.ps") && !m("?.ps", "ab.ps"));
        assert!(m("*.ps", "x.y.ps") && !m("*.ps", "x.psx"));
        assert!(m("a*b*c", "aXbYbZc") && !m("a*b*c", "aXbY"));
        assert!(m("\\*", "*") && !m("\\*", "x") && !m("a\\", "a"));
        assert!(m("", "") && !m("", "a"));
    }

    /// On Windows a template uses `/` between directories, drive letter
    /// included, and the names come back with it (issue #762).
    #[cfg(windows)]
    #[test]
    fn windows_templates_use_forward_slashes() {
        let dir = std::env::temp_dir().join(format!("pscat-fnfa-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        std::fs::write(dir.join("b.txt"), b"x").unwrap();
        let base = dir.to_str().unwrap().replace('\\', "/");
        let names =
            matching_names(format!("{base}/*.txt").as_bytes(), None, &mut |_| Ok(())).unwrap();
        let want: Vec<Vec<u8>> = ["a.txt", "b.txt"]
            .iter()
            .map(|n| format!("{base}/{n}").into_bytes())
            .collect();
        assert_eq!(names, want);
        // The backslash form is the escape, so it names nothing.
        let bs = format!("{}\\*.txt", dir.to_str().unwrap());
        assert!(
            matching_names(bs.as_bytes(), None, &mut |_| Ok(()))
                .unwrap()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn matching_names_needs_wildcards_in_the_last_component_only() {
        let names = |t: &[u8]| matching_names(t, None, &mut |_| Ok(())).unwrap();
        assert!(names(b"li*/artkit.ps").is_empty());
        assert!(names(b"lib/").is_empty());
        assert!(names(b"\xff*").is_empty());
        assert_eq!(names(b"lib/artkit.ps"), vec![b"lib/artkit.ps".to_vec()]);
        // A poll that refuses stops the listing with its error.
        let stopped = matching_names(b"lib/*.ps", None, &mut |_| Err(PsError::StepsExceeded));
        assert_eq!(stopped, Err(PsError::StepsExceeded));
    }
}
