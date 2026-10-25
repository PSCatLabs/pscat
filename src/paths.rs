//! The resource resolver: one place that answers "where do pscat's
//! runtime assets come from" (issue #598, replacing Stage 21's
//! root-only lookup). Used by the `run`/`file` operators
//! (`ops/file.rs`), the font catalog (`font.rs`, so `--fonts` and
//! `--capabilities` too), `selftest`'s `% @requires:` loading, and
//! `pscat-mcp`.
//!
//! The shared abstraction is a *name* (`lib/artkit.ps`) and the bytes
//! it opens to, not a filesystem path: a resource can come from disk
//! or from the copy of `lib/**/*.ps` compiled into the binary
//! (`build.rs` writes the index). The wasm build has only that
//! embedded backend; native builds search disk first.
//!
//! ## Search order
//!
//! Written down once, for users, in `docs/guides/getting-started.md`
//! ("Where pscat finds its files"); keep the two in step. In short, a
//! relative `run`/`file` name tries `$PSCAT_LIB` (lib/ names only),
//! `$PSCAT_HOME`/`$PSCAT_ROOT`, the current directory, the per-user data
//! directory, the executable's directory, the embedded copy, and, in
//! debug-assertion builds only, the checkout heuristics. Those
//! heuristics (three levels above the executable, and the compile-time
//! crate root) never run in a release build, so an install test cannot
//! pass by finding a checkout or the crates.io source tree that
//! `CARGO_MANIFEST_DIR` points into. The font catalog skips the steps
//! that make no sense for a directory and checks the current directory
//! last.
//!
//! `PSCAT_TRACE_RESOURCES=1` prints one stderr line per resolved
//! resource naming the backend that served it
//! (`scripts/clean_install_test.sh` asserts on it).

use std::borrow::Cow;
use std::path::{Path, PathBuf};

include!(concat!(env!("OUT_DIR"), "/embedded_lib.rs"));

/// Which step of the search order served a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    LibEnv,
    Home,
    Direct,
    UserData,
    ExeDir,
    Embedded,
    Checkout,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::LibEnv => "pscat-lib",
            Backend::Home => "pscat-home",
            Backend::Direct => "cwd",
            Backend::UserData => "user-data",
            Backend::ExeDir => "exe-dir",
            Backend::Embedded => "embedded",
            Backend::Checkout => "checkout",
        }
    }
}

/// A resolved resource: its bytes and where they came from.
#[derive(Debug)]
pub struct Resource {
    pub bytes: Cow<'static, [u8]>,
    pub backend: Backend,
    /// The file read, for disk backends; `None` for the embedded copy.
    pub path: Option<PathBuf>,
}

/// The embedded copy of `name` (`lib/artkit.ps`), if there is one. A
/// leading `./` is tolerated; nothing else is normalized.
pub fn embedded(name: &str) -> Option<&'static [u8]> {
    let name = name.strip_prefix("./").unwrap_or(name);
    EMBEDDED_LIB
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| EMBEDDED_LIB[i].1)
}

/// Every embedded name, sorted.
pub fn embedded_names() -> impl Iterator<Item = &'static str> {
    EMBEDDED_LIB.iter().map(|(n, _)| *n)
}

/// The on-disk places the search order consults, gathered once from
/// the process environment, or built by hand in tests (`set_var` is
/// `unsafe` in edition 2024 and racy under a parallel test runner).
#[derive(Debug, Default, Clone)]
pub struct Roots {
    pub lib: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub user_data: Option<PathBuf>,
    pub exe_dir: Option<PathBuf>,
    pub checkout: Vec<PathBuf>,
}

impl Roots {
    /// The real environment. Empty on wasm, where there is no disk.
    pub fn from_process() -> Roots {
        #[cfg(target_arch = "wasm32")]
        {
            Roots::default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let var = |k: &str| {
                std::env::var_os(k)
                    .filter(|v| !v.is_empty())
                    .map(PathBuf::from)
            };
            // current_exe() may return a symlink's own location (one
            // in ~/.local/bin pointing at a bundle); canonicalize so
            // the exe-dir step looks beside the real binary.
            let exe = std::env::current_exe()
                .ok()
                .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
            Roots {
                lib: var("PSCAT_LIB"),
                home: var("PSCAT_HOME").or_else(|| var("PSCAT_ROOT")),
                user_data: user_data_dir(&var),
                exe_dir: exe.as_deref().and_then(Path::parent).map(Path::to_path_buf),
                checkout: checkout_roots(exe.as_deref()),
            }
        }
    }

    /// The disk candidates for a relative `name`, in search order
    /// (steps 1-5; the embedded copy is step 6 and
    /// [`Roots::checkout_candidates`] step 7).
    pub fn disk_candidates(&self, name: &str) -> Vec<(Backend, PathBuf)> {
        let mut out = Vec::new();
        if let (Some(lib), Some(rest)) = (&self.lib, name.strip_prefix("lib/")) {
            out.push((Backend::LibEnv, lib.join(rest)));
        }
        if let Some(home) = &self.home {
            out.push((Backend::Home, home.join(name)));
        }
        out.push((Backend::Direct, PathBuf::from(name)));
        if let Some(d) = &self.user_data {
            out.push((Backend::UserData, d.join(name)));
        }
        if let Some(d) = &self.exe_dir {
            out.push((Backend::ExeDir, d.join(name)));
        }
        out
    }

    /// Step 7's candidates for `name`.
    pub fn checkout_candidates(&self, name: &str) -> Vec<(Backend, PathBuf)> {
        self.checkout
            .iter()
            .map(|r| (Backend::Checkout, r.join(name)))
            .collect()
    }

    /// Resolve `name` against these roots, reading through `read`
    /// (a parameter so tests need no real files).
    pub fn open_with(
        &self,
        name: &str,
        read: &dyn Fn(&Path) -> Option<Vec<u8>>,
    ) -> Option<Resource> {
        let from_disk = |(backend, path): (Backend, PathBuf)| {
            read(&path).map(|bytes| Resource {
                bytes: Cow::Owned(bytes),
                backend,
                path: Some(path),
            })
        };
        if Path::new(name).is_absolute() {
            return from_disk((Backend::Direct, PathBuf::from(name)));
        }
        self.disk_candidates(name)
            .into_iter()
            .find_map(from_disk)
            .or_else(|| {
                embedded(name).map(|b| Resource {
                    bytes: Cow::Borrowed(b),
                    backend: Backend::Embedded,
                    path: None,
                })
            })
            .or_else(|| {
                self.checkout_candidates(name)
                    .into_iter()
                    .find_map(from_disk)
            })
    }

    /// The roots that hold whole pscat trees (`lib/`, `fonts/catalog/`,
    /// `scripts/`), in search order, the current directory excluded.
    pub fn install_roots(&self) -> Vec<(Backend, PathBuf)> {
        let mut out = Vec::new();
        if let Some(h) = &self.home {
            out.push((Backend::Home, h.clone()));
        }
        if let Some(d) = &self.user_data {
            out.push((Backend::UserData, d.clone()));
        }
        if let Some(d) = &self.exe_dir {
            out.push((Backend::ExeDir, d.clone()));
        }
        out.extend(self.checkout.iter().map(|r| (Backend::Checkout, r.clone())));
        out
    }

    /// The font catalog directory: the install roots, then the
    /// current directory.
    pub fn catalog_dir_with(&self, is_dir: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
        self.catalog_dirs_with(is_dir).into_iter().next()
    }

    /// Every plain `fonts/catalog/` directory, in [`Self::catalog_dir_with`]'s
    /// order. Each is an explicit filesystem choice (an override, in
    /// `pscat fonts`' terms), so all of them come before any installed
    /// font pack (issue #599).
    pub fn catalog_dirs_with(&self, is_dir: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
        self.install_roots()
            .into_iter()
            .map(|(_, r)| r)
            .chain(std::iter::once(PathBuf::from(".")))
            .map(|r| r.join("fonts/catalog"))
            .filter(|p| is_dir(p))
            .collect()
    }

    /// The directories `pscat fonts install` puts packs in and font
    /// lookup reads them from, in search order: `$PSCAT_HOME`, the
    /// per-user data directory, the executable's directory. Never the
    /// checkout or the current directory: an installed pack is a user
    /// install, not a source tree.
    pub fn pack_dirs(&self) -> Vec<PathBuf> {
        [&self.home, &self.user_data, &self.exe_dir]
            .into_iter()
            .flatten()
            .map(|r| r.join("fonts/packs"))
            .collect()
    }

    /// Where `pscat fonts install` writes: `$PSCAT_HOME` (or
    /// `$PSCAT_ROOT`), otherwise the per-user data directory.
    pub fn pack_install_dir(&self) -> Option<PathBuf> {
        self.home
            .as_ref()
            .or(self.user_data.as_ref())
            .map(|r| r.join("fonts/packs"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn user_data_dir(var: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    if cfg!(windows) {
        var("APPDATA").map(|d| d.join("pscat"))
    } else if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support/pscat"))
    } else {
        var("XDG_DATA_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("pscat"))
    }
}

/// Step 7, in debug-assertion builds only (`debug` and `--profile ci`,
/// which is how the test suite runs): a release binary must not find
/// resources by sitting inside a checkout or by remembering where its
/// source was compiled.
#[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
fn checkout_roots(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = exe
        .and_then(|p| p.ancestors().nth(3))
        .map(Path::to_path_buf)
        .into_iter()
        .collect();
    v.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    v
}

#[cfg(all(not(target_arch = "wasm32"), not(debug_assertions)))]
fn checkout_roots(_exe: Option<&Path>) -> Vec<PathBuf> {
    Vec::new()
}

fn trace_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        cfg!(not(target_arch = "wasm32"))
            && std::env::var_os("PSCAT_TRACE_RESOURCES").is_some_and(|v| !v.is_empty() && v != "0")
    })
}

fn roots() -> &'static Roots {
    static ROOTS: std::sync::OnceLock<Roots> = std::sync::OnceLock::new();
    ROOTS.get_or_init(Roots::from_process)
}

/// Resolve and read a program's `run`/`file` name through the search
/// order (module doc).
pub fn open(name: &str) -> Option<Resource> {
    #[cfg(target_arch = "wasm32")]
    let read = |_: &Path| None;
    #[cfg(not(target_arch = "wasm32"))]
    let read = |p: &Path| std::fs::read(p).ok();
    open_with_reader(name, &read)
}

/// [`open`] reading disk candidates through `read` instead of
/// `std::fs::read` (issue #702: the file operators read under the
/// interpreter's execution limits when it has any). `read` returning `None`
/// means "not here, try the next place", so a reader that stops for a limit
/// must remember why and make the caller discard whatever this returns.
pub(crate) fn open_with_reader(
    name: &str,
    read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Option<Resource> {
    let found = roots().open_with(name, read);
    if trace_enabled() {
        match &found {
            Some(r) => match &r.path {
                Some(p) => eprintln!(
                    "pscat: resource {name} <- {} ({})",
                    r.backend.as_str(),
                    p.display()
                ),
                None => eprintln!("pscat: resource {name} <- {}", r.backend.as_str()),
            },
            None => eprintln!("pscat: resource {name} <- not found"),
        }
    }
    found
}

/// The names `filenameforall` yields for `template` (issue #740): file
/// names a program could open with `run` or `(r) file`, sorted, without
/// duplicates. An absolute template lists that directory; a relative one
/// lists the directory in every place the search order would look (module
/// doc), the embedded `lib/` copy included, and yields the relative name.
/// Wildcards (`*`, `?`, `\` to quote) match within the last component;
/// a template with one in a directory part matches nothing.
///
/// `poll` is called once per directory entry looked at, with the bytes the
/// entry adds to the result (0 when it doesn't match), so an interpreter
/// under execution limits can stop a listing of a huge directory.
pub fn matching_names(
    template: &[u8],
    poll: &mut dyn FnMut(usize) -> Result<(), crate::error::PsError>,
) -> Result<Vec<Vec<u8>>, crate::error::PsError> {
    let Ok(template) = std::str::from_utf8(template) else {
        return Ok(Vec::new());
    };
    let first_wild = template.find(['*', '?', '\\']).unwrap_or(template.len());
    let dir_end = template[..first_wild].rfind('/').map_or(0, |i| i + 1);
    let (dir, pattern) = template.split_at(dir_end);
    if pattern.is_empty() || pattern.contains('/') {
        return Ok(Vec::new());
    }
    let mut found = std::collections::BTreeSet::new();
    let mut take = |file: &str| {
        let hit = glob_match(pattern.as_bytes(), file.as_bytes());
        poll(if hit { dir.len() + file.len() } else { 0 })?;
        if hit {
            found.insert(format!("{dir}{file}").into_bytes());
        }
        Ok::<(), crate::error::PsError>(())
    };
    let dirs: Vec<PathBuf> = if Path::new(dir).is_absolute() {
        vec![PathBuf::from(dir)]
    } else {
        let name = if dir.is_empty() { "." } else { dir };
        let r = roots();
        r.disk_candidates(name)
            .into_iter()
            .chain(r.checkout_candidates(name))
            .map(|(_, p)| p)
            .collect()
    };
    for d in dirs {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            // Regular files, through symlinks, as `run` would read them.
            if !std::fs::metadata(e.path()).is_ok_and(|m| m.is_file()) {
                continue;
            }
            if let Some(file) = e.file_name().to_str() {
                take(file)?;
            }
        }
    }
    if !Path::new(dir).is_absolute() {
        for name in embedded_names() {
            if let Some(file) = name.strip_prefix(dir).filter(|f| !f.contains('/')) {
                take(file)?;
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

/// The pscat trees on disk, in search order (see [`Roots::install_roots`]).
pub fn install_roots() -> Vec<(Backend, PathBuf)> {
    roots().install_roots()
}

/// Find the font catalog directory (module doc, last paragraph).
#[cfg(not(target_arch = "wasm32"))]
pub fn catalog_dir() -> Option<PathBuf> {
    roots().catalog_dir_with(&|p| p.is_dir())
}

#[cfg(target_arch = "wasm32")]
pub fn catalog_dir() -> Option<PathBuf> {
    None
}

/// Every `fonts/catalog/` directory on disk (see [`Roots::catalog_dirs_with`]).
#[cfg(not(target_arch = "wasm32"))]
pub fn catalog_dirs() -> Vec<PathBuf> {
    roots().catalog_dirs_with(&|p| p.is_dir())
}

/// The process's roots, for `pscat fonts` and the font-pack lookup.
pub fn process_roots() -> &'static Roots {
    roots()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn reader(files: &[(&str, &str)]) -> impl Fn(&Path) -> Option<Vec<u8>> {
        let map: HashMap<PathBuf, Vec<u8>> = files
            .iter()
            .map(|(p, b)| (PathBuf::from(p), b.as_bytes().to_vec()))
            .collect();
        move |p: &Path| map.get(p).cloned()
    }

    fn full_roots() -> Roots {
        Roots {
            lib: Some(PathBuf::from("/envlib")),
            home: Some(PathBuf::from("/home")),
            user_data: Some(PathBuf::from("/data")),
            exe_dir: Some(PathBuf::from("/exe")),
            checkout: vec![PathBuf::from("/checkout")],
        }
    }

    #[test]
    fn embedded_index_matches_every_lib_file_on_disk() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "ps") {
                    out.push(p);
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut on_disk = Vec::new();
        walk(&root.join("lib"), &mut on_disk);
        assert!(on_disk.len() > 20, "lib/ walk found too little");
        assert_eq!(on_disk.len(), embedded_names().count());
        for p in on_disk {
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_str()
                .unwrap()
                .replace('\\', "/");
            let bytes = std::fs::read(&p).unwrap();
            assert_eq!(embedded(&rel), Some(&bytes[..]), "{rel} differs from disk");
        }
        assert!(embedded("lib/fonts/neon.ps").is_some());
        assert!(embedded("./lib/artkit.ps").is_some());
        assert!(embedded("lib/styles/specimen_toon.png").is_none());
        assert!(embedded("artkit.ps").is_none());
    }

    #[test]
    fn embedded_serves_when_no_disk_copy_exists() {
        let r = Roots::default()
            .open_with("lib/artkit.ps", &|_: &Path| None)
            .unwrap();
        assert_eq!(r.backend, Backend::Embedded);
        assert_eq!(&r.bytes[..], embedded("lib/artkit.ps").unwrap());
        assert!(r.path.is_none());
    }

    #[test]
    fn on_disk_lib_overrides_embedded() {
        // A checkout's (or any directory's) own lib/ beside the program.
        let read = reader(&[("lib/artkit.ps", "% dev copy")]);
        let r = Roots::default().open_with("lib/artkit.ps", &read).unwrap();
        assert_eq!(r.backend, Backend::Direct);
        assert_eq!(&r.bytes[..], b"% dev copy");
    }

    #[test]
    fn pscat_lib_overrides_embedded_and_cwd() {
        let read = reader(&[
            ("/envlib/artkit.ps", "% PSCAT_LIB"),
            ("lib/artkit.ps", "% cwd"),
        ]);
        let r = full_roots().open_with("lib/artkit.ps", &read).unwrap();
        assert_eq!(r.backend, Backend::LibEnv);
        assert_eq!(&r.bytes[..], b"% PSCAT_LIB");
        // PSCAT_LIB stands in for lib/ only, never for other names.
        let read = reader(&[("/envlib/helper.ps", "x")]);
        assert!(full_roots().open_with("helper.ps", &read).is_none());
    }

    #[test]
    fn search_order_is_lib_home_cwd_data_exe_embedded_checkout() {
        let roots = full_roots();
        let order: Vec<_> = roots
            .disk_candidates("lib/x.ps")
            .into_iter()
            .chain(roots.checkout_candidates("lib/x.ps"))
            .collect();
        assert_eq!(
            order,
            vec![
                (Backend::LibEnv, PathBuf::from("/envlib/x.ps")),
                (Backend::Home, PathBuf::from("/home/lib/x.ps")),
                (Backend::Direct, PathBuf::from("lib/x.ps")),
                (Backend::UserData, PathBuf::from("/data/lib/x.ps")),
                (Backend::ExeDir, PathBuf::from("/exe/lib/x.ps")),
                (Backend::Checkout, PathBuf::from("/checkout/lib/x.ps")),
            ]
        );
        // The embedded copy sits between the exe dir and the checkout.
        let read = reader(&[("/checkout/lib/artkit.ps", "% checkout")]);
        let r = roots.open_with("lib/artkit.ps", &read).unwrap();
        assert_eq!(r.backend, Backend::Embedded);
        let read = reader(&[("/exe/lib/artkit.ps", "% bundle")]);
        assert_eq!(
            roots.open_with("lib/artkit.ps", &read).unwrap().backend,
            Backend::ExeDir
        );
    }

    #[test]
    fn non_lib_names_fall_through_to_checkout() {
        let read = reader(&[("/checkout/examples/a.ps", "a")]);
        let r = full_roots().open_with("examples/a.ps", &read).unwrap();
        assert_eq!(r.backend, Backend::Checkout);
        assert!(full_roots().open_with("nope.ps", &read).is_none());
    }

    #[test]
    fn absolute_names_ignore_every_root() {
        // `/abs/x.ps` has no drive letter, so it is not absolute on Windows.
        let (x, y) = if cfg!(windows) {
            ("C:/abs/x.ps", "C:/abs/y.ps")
        } else {
            ("/abs/x.ps", "/abs/y.ps")
        };
        let files = [(x, "yes")];
        let read = reader(&files);
        let r = full_roots().open_with(x, &read).unwrap();
        assert_eq!(r.backend, Backend::Direct);
        assert!(full_roots().open_with(y, &read).is_none());
    }

    #[test]
    fn catalog_checks_install_roots_before_cwd() {
        let roots = full_roots();
        let both =
            |p: &Path| p == Path::new("./fonts/catalog") || p == Path::new("/exe/fonts/catalog");
        assert_eq!(
            roots.catalog_dir_with(&both),
            Some(PathBuf::from("/exe/fonts/catalog"))
        );
        let only_cwd = |p: &Path| p == Path::new("./fonts/catalog");
        assert_eq!(
            roots.catalog_dir_with(&only_cwd),
            Some(PathBuf::from("./fonts/catalog"))
        );
        assert_eq!(Roots::default().catalog_dir_with(&|_| false), None);
    }

    #[test]
    fn every_catalog_dir_is_listed_and_packs_live_in_user_roots() {
        let roots = full_roots();
        let some = |p: &Path| p != Path::new("/data/fonts/catalog");
        assert_eq!(
            roots.catalog_dirs_with(&some),
            vec![
                PathBuf::from("/home/fonts/catalog"),
                PathBuf::from("/exe/fonts/catalog"),
                PathBuf::from("/checkout/fonts/catalog"),
                PathBuf::from("./fonts/catalog"),
            ]
        );
        assert_eq!(
            roots.pack_dirs(),
            vec![
                PathBuf::from("/home/fonts/packs"),
                PathBuf::from("/data/fonts/packs"),
                PathBuf::from("/exe/fonts/packs"),
            ]
        );
        assert_eq!(
            roots.pack_install_dir(),
            Some(PathBuf::from("/home/fonts/packs"))
        );
        let no_home = Roots {
            home: None,
            ..full_roots()
        };
        assert_eq!(
            no_home.pack_install_dir(),
            Some(PathBuf::from("/data/fonts/packs"))
        );
        assert_eq!(Roots::default().pack_install_dir(), None);
    }

    #[test]
    fn checkout_heuristics_follow_debug_assertions() {
        // The suite runs with debug assertions (debug or --profile ci),
        // so the heuristics are on here; a release build has none.
        let r = checkout_roots(Some(Path::new("/c/target/release/pscat")));
        if cfg!(debug_assertions) {
            assert_eq!(r[0], PathBuf::from("/c"));
            assert!(r.contains(&PathBuf::from(env!("CARGO_MANIFEST_DIR"))));
        } else {
            assert!(r.is_empty());
        }
    }

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

    #[test]
    fn matching_names_needs_wildcards_in_the_last_component_only() {
        let names = |t: &[u8]| matching_names(t, &mut |_| Ok(())).unwrap();
        assert!(names(b"li*/artkit.ps").is_empty());
        assert!(names(b"lib/").is_empty());
        assert!(names(b"\xff*").is_empty());
        assert_eq!(names(b"lib/artkit.ps"), vec![b"lib/artkit.ps".to_vec()]);
        // A poll that refuses stops the listing with its error.
        let stopped = matching_names(b"lib/*.ps", &mut |_| {
            Err(crate::error::PsError::StepsExceeded)
        });
        assert_eq!(stopped, Err(crate::error::PsError::StepsExceeded));
    }
}
