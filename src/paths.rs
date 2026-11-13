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
//! ## What a disk candidate may be (issue #1177)
//!
//! A relative name's candidates in the search order must be regular
//! files: a FIFO, device, socket or directory at one of those places is
//! "not here", and the search goes on to the next place and finally the
//! embedded copy. (A FIFO with no writer would otherwise block the open
//! itself, before any interpreter limit applies.) The check is made on
//! the opened handle (`Probe::open`: opened once, `O_NONBLOCK` on Unix so
//! the open itself cannot block), and the read goes through that same
//! handle, so a swap between the check and the read cannot slip a FIFO in.
//! This holds for `run`, `file` and `status` too, with or without execution
//! limits (their readers take the handle, issue #702's limit-aware one
//! included). A `lib/` file larger than `MAX_LIB_BYTES` is skipped as well,
//! silently, and [`open`] refuses (with a note on stderr) any other resource
//! larger than `MAX_RESOURCE_BYTES`. An absolute name is read as written,
//! exactly as before, FIFOs and devices included.
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
    /// (a parameter so tests need no real files). `read` is handed every
    /// candidate path as it is, with no screening of what it is: the
    /// screening of issue #1177 is [`Roots::open_probed`]'s.
    pub fn open_with(
        &self,
        name: &str,
        read: &dyn Fn(&Path) -> Option<Vec<u8>>,
    ) -> Option<Resource> {
        self.open_probed(name, &|p: &Path, _: Probe<'_>| read(p))
    }

    /// [`Roots::open_with`] telling `read` what kind of candidate it has
    /// (a [`Probe`]), so that it can open a search-order candidate once, with
    /// [`Probe::open`], and read the handle it gets (issue #1177).
    pub(crate) fn open_probed(&self, name: &str, read: &Reader<'_>) -> Option<Resource> {
        let from_disk = |search: bool| {
            move |(backend, path): (Backend, PathBuf)| {
                read(&path, Probe { name, search }).map(|bytes| Resource {
                    bytes: Cow::Owned(bytes),
                    backend,
                    path: Some(path),
                })
            }
        };
        if Path::new(name).is_absolute() {
            return from_disk(false)((Backend::Direct, PathBuf::from(name)));
        }
        self.disk_candidates(name)
            .into_iter()
            .find_map(from_disk(true))
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
                    .find_map(from_disk(true))
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
///
/// `PSCAT_NO_CHECKOUT=1` turns the heuristics off in those builds too,
/// so a test can run against what a bare install has (no checkout
/// `lib/` or `fonts/catalog/`): `tests/skill_blocks.rs`. Release builds
/// ignore it, since they have no heuristics to turn off.
#[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
fn checkout_roots(exe: Option<&Path>) -> Vec<PathBuf> {
    let off = std::env::var_os("PSCAT_NO_CHECKOUT").is_some_and(|v| !v.is_empty() && v != "0");
    checkout_roots_unless(exe, off)
}

#[cfg(all(not(target_arch = "wasm32"), debug_assertions))]
fn checkout_roots_unless(exe: Option<&Path>, off: bool) -> Vec<PathBuf> {
    if off {
        return Vec::new();
    }
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

/// The largest `lib/` source file a candidate may be before it is skipped
/// (issue #1177). The largest real one, `lib/paintkit.ps`, is 0.6 MB, so this
/// is two orders of magnitude of headroom while still far below the memory a
/// multi-GB file at a search-order place would take.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_LIB_BYTES: u64 = 64 << 20;

/// The largest file [`open`] reads for any other relative name: data a
/// program loads, and font files (`.ttf`/`.otf`/`.ttc`; the largest in the
/// catalog is 10 MB, a CJK collection can be several times that).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_RESOURCE_BYTES: u64 = 512 << 20;

/// The size cap [`open`] applies to a resource named `name`:
/// [`MAX_LIB_BYTES`] for a `lib/` name, [`MAX_RESOURCE_BYTES`] otherwise.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn size_cap(name: &str) -> u64 {
    if is_lib_name(name) {
        MAX_LIB_BYTES
    } else {
        MAX_RESOURCE_BYTES
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn is_lib_name(name: &str) -> bool {
    name.strip_prefix("./").unwrap_or(name).starts_with("lib/")
}

/// What a reader is told about the candidate it is handed (issue #1177).
#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct Probe<'a> {
    /// The name the program (or library) asked for.
    pub name: &'a str,
    /// Whether the candidate is a place the search order tries for a relative
    /// name, rather than an absolute name read as written.
    pub search: bool,
}

/// A disk reader for [`Roots::open_probed`]: the candidate path and what it
/// is, `None` for "not here, try the next place".
pub(crate) type Reader<'a> = dyn Fn(&Path, Probe<'_>) -> Option<Vec<u8>> + 'a;

/// What [`Probe::open`] made of a candidate.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) enum Opened {
    /// A search-order candidate, opened once and found to be a regular file
    /// of acceptable size: read this handle, never the path again.
    Handle(std::fs::File),
    /// An absolute name: nothing was opened; read it by path as before.
    AsWritten,
}

#[cfg(not(target_arch = "wasm32"))]
impl Probe<'_> {
    /// Open a search-order candidate for reading, once. `None` means "not
    /// here": the path is missing or unreadable, or is not a regular file
    /// (a FIFO, device, socket or directory), or is a `lib/` file over
    /// [`MAX_LIB_BYTES`]. The decision is made on the opened handle, which
    /// is `O_NONBLOCK` on Unix so that a FIFO with no writer opens at once
    /// instead of blocking in `open`, so there is no window between the
    /// check and the read for a swap. The skip is silent: a reader under
    /// read roots calls this only after admitting the path, but a note
    /// naming it would still be a message about the disk. An absolute name
    /// is [`Opened::AsWritten`].
    pub(crate) fn open(&self, path: &Path) -> Option<Opened> {
        if !self.search {
            return Some(Opened::AsWritten);
        }
        let file = open_regular(path)?;
        if is_lib_name(self.name) && file.metadata().ok()?.len() > MAX_LIB_BYTES {
            return None;
        }
        Some(Opened::Handle(file))
    }

    /// The most bytes a read of this candidate may take: [`MAX_LIB_BYTES`]
    /// for a `lib/` name, no limit otherwise (a program's own data files
    /// were never capped; [`open`] has its own cap).
    pub(crate) fn read_cap(&self) -> u64 {
        if is_lib_name(self.name) {
            MAX_LIB_BYTES
        } else {
            u64::MAX
        }
    }
}

/// Open `path` for reading only if it is a regular file, deciding from the
/// opened handle (issue #1177). On Unix the open is `O_NONBLOCK`, so a FIFO
/// with no writer opens at once (and is then refused) instead of blocking in
/// `open`; the flag does nothing to reads of a regular file.
#[cfg(not(target_arch = "wasm32"))]
fn open_regular(path: &Path) -> Option<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NONBLOCK);
    }
    let file = opts.open(path).ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}

/// Read the rest of `file`, an opened regular file, `None` when it cannot be
/// read or is larger than `cap` bytes (that last with a note on stderr
/// naming `path`: the file is real, so the refusal must not look like a
/// missing one). The length is checked on the handle, and the read stops one
/// byte past `cap`, so a file that grows after the check cannot get past it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_handle(file: std::fs::File, path: &Path, cap: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    let len = file.metadata().ok()?.len();
    if len > cap {
        too_large(path, cap);
        return None;
    }
    let mut data = Vec::new();
    data.try_reserve_exact(usize::try_from(len).ok()?.checked_add(1)?)
        .ok()?;
    file.take(cap.saturating_add(1))
        .read_to_end(&mut data)
        .ok()?;
    if data.len() as u64 > cap {
        too_large(path, cap);
        return None;
    }
    Some(data)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn too_large(path: &Path, cap: u64) {
    eprintln!(
        "pscat: {} is larger than the {} MiB a resource of its kind may be; skipped",
        path.display(),
        cap >> 20
    );
}

/// Resolve and read a program's `run`/`file` name through the search
/// order (module doc).
pub fn open(name: &str) -> Option<Resource> {
    #[cfg(target_arch = "wasm32")]
    let read = |_: &Path, _: Probe<'_>| None;
    // A search-order candidate is opened once and read through that handle
    // (`Probe::open`); an absolute name is read as written, as it always was.
    #[cfg(not(target_arch = "wasm32"))]
    let read = |p: &Path, probe: Probe<'_>| match probe.open(p)? {
        Opened::Handle(f) => read_handle(f, p, size_cap(name)),
        Opened::AsWritten => std::fs::read(p).ok(),
    };
    open_with_reader(name, &read)
}

/// [`open`] reading disk candidates through `read` instead of
/// `std::fs::read` (issue #702: the file operators read under the
/// interpreter's execution limits when it has any). `read` returning `None`
/// means "not here, try the next place", so a reader that stops for a limit
/// must remember why and make the caller discard whatever this returns.
pub(crate) fn open_with_reader(name: &str, read: &Reader<'_>) -> Option<Resource> {
    let found = roots().open_probed(name, read);
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

    #[cfg(debug_assertions)]
    #[test]
    fn no_checkout_switch_empties_the_heuristics() {
        let exe = Some(Path::new("/c/target/release/pscat"));
        assert!(checkout_roots_unless(exe, true).is_empty());
        assert!(!checkout_roots_unless(exe, false).is_empty());
    }

    /// Open `path` as a search-order candidate for `name` and read it, the
    /// way `paths::open` does (issue #1177).
    #[cfg(unix)]
    fn regular(name: &str, path: &Path, cap: u64) -> Option<Vec<u8>> {
        let probe = Probe { name, search: true };
        match probe.open(path)? {
            Opened::Handle(f) => read_handle(f, path, cap),
            Opened::AsWritten => unreachable!("a search candidate"),
        }
    }

    /// Run `f` on a thread and give up after a deadline: a regression in the
    /// non-blocking open is a hang, which must fail the test, not stall it.
    #[cfg(unix)]
    fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(std::time::Duration::from_secs(20))
            .expect("opening a FIFO blocked")
    }

    #[test]
    fn the_cap_is_per_kind() {
        assert_eq!(size_cap("lib/artkit.ps"), MAX_LIB_BYTES);
        assert_eq!(size_cap("./lib/artkit.ps"), MAX_LIB_BYTES);
        assert_eq!(size_cap("fonts/catalog/X/X.ttf"), MAX_RESOURCE_BYTES);
        assert_eq!(size_cap("data.bin"), MAX_RESOURCE_BYTES);
        // Real data the numbers are chosen from: the largest lib file and
        // the largest catalog font.
        const { assert!(MAX_LIB_BYTES > 100 * 598_400) };
        const { assert!(MAX_RESOURCE_BYTES > 40 * 10_414_588) };
        let search = |name| Probe { name, search: true };
        assert_eq!(search("lib/x.ps").read_cap(), MAX_LIB_BYTES);
        assert_eq!(search("data.bin").read_cap(), u64::MAX);
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_name_is_read_as_written() {
        let probe = Probe {
            name: "/dev/null",
            search: false,
        };
        assert!(matches!(
            probe.open(Path::new("/dev/null")),
            Some(Opened::AsWritten)
        ));
        // And `open` returns what `std::fs::read` does for a device node.
        assert_eq!(open("/dev/null").unwrap().bytes.len(), 0);
    }

    /// The handle-level reader itself, with a small cap so a regression
    /// cannot allocate much: tests/resource_candidates.rs covers the search
    /// order around it. Every open of a FIFO is behind a deadline.
    #[cfg(unix)]
    #[test]
    fn a_search_candidate_that_is_not_a_small_regular_file_is_refused_without_blocking() {
        let dir = std::env::temp_dir().join(format!("pscat-paths-regular-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("ok.ps");
        std::fs::write(&ok, b"12345").unwrap();
        assert_eq!(regular("x.ps", &ok, 5).as_deref(), Some(&b"12345"[..]));
        assert_eq!(regular("x.ps", &ok, 4), None);
        assert_eq!(regular("x.ps", &dir, 1 << 20), None, "a directory");
        assert_eq!(regular("x.ps", &dir.join("missing"), 1 << 20), None);
        let fifo = dir.join("stuck.ps");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        // O_NONBLOCK: these return at once instead of waiting for a writer.
        let f = fifo.clone();
        assert_eq!(within(move || regular("x.ps", &f, 1 << 20)), None);
        // A link to a FIFO is a FIFO.
        let link = dir.join("link.ps");
        std::os::unix::fs::symlink(&fifo, &link).unwrap();
        assert_eq!(within(move || regular("x.ps", &link, 1 << 20)), None);
        // A `lib/` file over the cap is skipped before it is read.
        let big = dir.join("big.ps");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_LIB_BYTES + 1)
            .unwrap();
        assert_eq!(regular("lib/big.ps", &big, MAX_LIB_BYTES), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
