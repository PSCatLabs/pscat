//! Downloadable font packs (issue #599): the font catalog ships as
//! separately versioned `.tar.zst` packs on their own `fonts-YYYY.MM`
//! release train. The standard 35 stay compiled into pscat: the twelve
//! Liberation faces and, since issue #757, the TeX Gyre and URW faces
//! that were the `texgyre` pack and part of `extras`.
//!
//! This module holds the parts every build needs: the pack table
//! (`fonts/packs.json`, compiled in, so a missing face can name the
//! pack that has it), the pinned index ([`pinned`]), and, on native
//! builds, the installed packs and the font directories font lookup
//! reads. `install` and `archive` do the downloading, unpacking and
//! checking behind `pscat fonts` (`cli`).
//!
//! ## Where fonts come from, in order
//!
//! 1. Every plain `fonts/catalog/` directory the resolver finds
//!    (`paths::Roots::catalog_dirs_with`): `$PSCAT_HOME`, the user data
//!    directory, the executable's directory, the checkout (debug builds
//!    only), the current directory. Each is an explicit filesystem
//!    choice, reported as an *override* and *user-selected*.
//! 2. Installed packs, `<root>/fonts/packs/<pack>/`, in
//!    `paths::Roots::pack_dirs` order (`$PSCAT_HOME`, user data, the
//!    executable's directory). The first root holding a pack name wins.
//!
//! The first directory holding a requested file serves it. A pack is
//! reported *pinned* when the index it was installed from has the tag
//! and SHA-256 this binary pins, whatever command installed it (the
//! bytes are what reproduce the pixels); anything else is
//! *user-selected*. The reproducibility contract is in
//! docs/guides/fonts.md ("Font packs").

pub mod pinned;

#[cfg(not(target_arch = "wasm32"))]
pub mod archive;
#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod install;

pub use pinned::{PINNED_INDEX, Pinned};

use std::path::Path;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

/// The highest `manifest_schema` (inside each pack) and
/// `index_schema` (the pack index) this build reads. Both are
/// versioned apart from the interpreter and the font train.
pub const MANIFEST_SCHEMA: u64 = 1;
pub const INDEX_SCHEMA: u64 = 1;

/// One pack in `fonts/packs.json`.
#[derive(Debug)]
pub struct PackDef {
    pub name: String,
    pub description: String,
    /// (family directory, font file names)
    pub families: Vec<(String, Vec<String>)>,
}

/// The compiled-in pack table. The file is this crate's own data and
/// a test parses it, so a malformed table is a build defect, not input.
pub fn packs() -> &'static [PackDef] {
    static TABLE: std::sync::OnceLock<Vec<PackDef>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| parse_table(include_str!("../../fonts/packs.json")))
}

fn parse_table(src: &str) -> Vec<PackDef> {
    let v: serde_json::Value = serde_json::from_str(src).expect("fonts/packs.json parses");
    let strs = |v: &serde_json::Value| -> Vec<String> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect()
    };
    v["packs"]
        .as_array()
        .expect("fonts/packs.json has packs")
        .iter()
        .map(|p| PackDef {
            name: p["name"].as_str().unwrap_or_default().to_string(),
            description: p["description"].as_str().unwrap_or_default().to_string(),
            families: p["families"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(fam, files)| (fam.clone(), strs(files)))
                .collect(),
        })
        .collect()
}

/// Packs that no longer exist, with the reason: their faces moved into
/// the binary. Older indexes (the pinned `fonts-2026.10` among them)
/// still list them; `pscat fonts install` skips them with this note.
pub const RETIRED_PACKS: &[(&str, &str)] = &[(
    "texgyre",
    "its faces (Palatino, Bookman, New Century Schoolbook, Avant Garde, \
     Zapf Chancery, Helvetica-Narrow) are built into pscat since issue #757",
)];

/// The line `pscat fonts install` prints for a retired pack, or `None`
/// for any other name.
pub fn retired_note(name: &str) -> Option<String> {
    RETIRED_PACKS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(n, why)| format!("pscat: the {n} font pack is retired: {why}; nothing to install"))
}

/// The pack shipping the font file whose stem is `stem`, matched
/// case-insensitively like font lookup's own stem match.
pub fn pack_for_stem(stem: &str) -> Option<&'static str> {
    packs().iter().find_map(|p| {
        p.families
            .iter()
            .flat_map(|(_, files)| files)
            .any(|f| {
                Path::new(f)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem))
            })
            .then_some(p.name.as_str())
    })
}

/// Whether a pack is the one this binary pins, or one the user chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Pinned,
    UserSelected,
}

impl Selection {
    pub fn as_str(self) -> &'static str {
        match self {
            Selection::Pinned => "pinned",
            Selection::UserSelected => "user-selected",
        }
    }
}

/// A pack installed under some `<root>/fonts/packs/`, as recorded in its
/// `install.json` by `pscat fonts install`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct Installed {
    pub name: String,
    pub dir: PathBuf,
    pub tag: String,
    pub pack_sha256: String,
    /// SHA-256 of the index the pack was installed through; `None` for
    /// a lone pack file (`--from x.tar.zst`), which has no index.
    pub index_sha256: Option<String>,
    /// The URL or file it came from.
    pub source: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Installed {
    pub fn selection(&self, pin: Option<Pinned>) -> Selection {
        match (pin, &self.index_sha256) {
            (Some(p), Some(sha)) if p.tag == self.tag && p.sha256 == sha => Selection::Pinned,
            _ => Selection::UserSelected,
        }
    }

    /// Read `<dir>/install.json`; `None` for anything that is not a
    /// completed install (a staging directory, a stray folder).
    pub fn read(dir: &Path) -> Option<Installed> {
        let bytes = std::fs::read(dir.join("install.json")).ok()?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let s = |k: &str| v[k].as_str().map(str::to_string);
        Some(Installed {
            name: s("pack")?,
            dir: dir.to_path_buf(),
            tag: s("tag")?,
            pack_sha256: s("pack_sha256")?,
            index_sha256: s("index_sha256"),
            source: s("source").unwrap_or_default(),
        })
    }
}

/// The packs installed in one `fonts/packs/` directory, by name.
#[cfg(not(target_arch = "wasm32"))]
pub fn installed_in(packs_dir: &Path) -> Vec<Installed> {
    let mut out: Vec<Installed> = std::fs::read_dir(packs_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| Installed::read(&e.path()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Installed packs across `dirs` (search order); the first directory
/// holding a pack name wins, the rest are shadowed.
#[cfg(not(target_arch = "wasm32"))]
pub fn installed_across(dirs: &[PathBuf]) -> Vec<Installed> {
    let mut seen = std::collections::HashSet::new();
    dirs.iter()
        .flat_map(|d| installed_in(d))
        .filter(|p| seen.insert(p.name.clone()))
        .collect()
}

/// The packs this process's font lookup reads.
#[cfg(not(target_arch = "wasm32"))]
pub fn installed() -> Vec<Installed> {
    installed_across(&crate::paths::process_roots().pack_dirs())
}

/// One directory font lookup scans for `<Family>/<file>`.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct FontDir {
    pub path: PathBuf,
    /// `None` for a plain `fonts/catalog/` override, else the pack.
    pub pack: Option<String>,
}

/// Font lookup's directories, in order (module doc).
#[cfg(not(target_arch = "wasm32"))]
pub fn font_dirs() -> Vec<FontDir> {
    crate::paths::catalog_dirs()
        .into_iter()
        .map(|path| FontDir { path, pack: None })
        .chain(installed().into_iter().map(|p| FontDir {
            path: p.dir,
            pack: Some(p.name),
        }))
        .collect()
}

/// The `font_packs` section of `--capabilities`: the pin, the installed
/// packs with pinned/user-selected status, and the override
/// directories that take precedence over them.
pub fn status_json() -> serde_json::Value {
    use serde_json::json;
    let pinned = PINNED_INDEX.map(|p| json!({ "tag": p.tag, "sha256": p.sha256 }));
    #[cfg(not(target_arch = "wasm32"))]
    let (installed, overrides): (Vec<serde_json::Value>, Vec<serde_json::Value>) = (
        installed()
            .iter()
            .map(|p| {
                json!({
                    "name": p.name,
                    "tag": p.tag,
                    "status": p.selection(PINNED_INDEX).as_str(),
                    "path": p.dir.display().to_string(),
                    "index_sha256": p.index_sha256,
                    "pack_sha256": p.pack_sha256,
                })
            })
            .collect(),
        crate::paths::catalog_dirs()
            .iter()
            .map(|d| json!({ "path": d.display().to_string(), "status": "user-selected" }))
            .collect(),
    );
    #[cfg(target_arch = "wasm32")]
    let (installed, overrides): (Vec<serde_json::Value>, Vec<serde_json::Value>) =
        (Vec::new(), Vec::new());
    json!({
        "pinned_index": pinned,
        "installed": installed,
        "overrides": overrides,
        "available": packs().iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
    })
}

/// `--fonts`' trailer: one `#` line per override and installed pack,
/// so a pasted `--fonts` says whether renders should reproduce.
#[cfg(not(target_arch = "wasm32"))]
pub fn status_lines() -> Vec<String> {
    let mut out = Vec::new();
    match PINNED_INDEX {
        Some(p) => out.push(format!(
            "# font-pack index pinned: {} ({})",
            p.tag, p.sha256
        )),
        None => {
            out.push("# font-pack index pinned: none (this build pins no fonts-* release)".into())
        }
    }
    for d in crate::paths::catalog_dirs() {
        out.push(format!("# font-override {} user-selected", d.display()));
    }
    for p in installed() {
        out.push(format!(
            "# font-pack {} {} {} {}",
            p.name,
            p.tag,
            p.selection(PINNED_INDEX).as_str(),
            p.dir.display()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_the_catalog_on_disk() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fonts/catalog");
        let mut on_disk: Vec<(String, Vec<String>)> = Vec::new();
        for e in std::fs::read_dir(&root).unwrap().flatten() {
            if !e.path().is_dir() {
                continue;
            }
            let mut files: Vec<String> = std::fs::read_dir(e.path())
                .unwrap()
                .flatten()
                .map(|f| f.file_name().to_string_lossy().into_owned())
                .filter(|f| {
                    let l = f.to_ascii_lowercase();
                    l.ends_with(".ttf") || l.ends_with(".otf") || l.ends_with(".ttc")
                })
                .collect();
            files.sort();
            on_disk.push((e.file_name().to_string_lossy().into_owned(), files));
        }
        on_disk.sort();
        let mut listed: Vec<(String, Vec<String>)> = packs()
            .iter()
            .flat_map(|p| p.families.iter().cloned())
            .map(|(f, mut files)| {
                files.sort();
                (f, files)
            })
            .collect();
        listed.sort();
        assert_eq!(
            listed, on_disk,
            "fonts/packs.json is out of step with fonts/catalog/"
        );
        let names: Vec<&str> = packs().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["extras", "cjk-thai"]);
        for (retired, _) in RETIRED_PACKS {
            assert!(!names.contains(retired), "{retired} is retired");
        }
    }

    #[test]
    fn faces_map_to_their_packs() {
        assert_eq!(pack_for_stem("SpecialElite-Regular"), Some("extras"));
        // Compiled in since issue #757: no pack.
        assert_eq!(pack_for_stem("StandardSymbolsPS"), None);
        assert_eq!(pack_for_stem("d050000l"), None);
        assert_eq!(pack_for_stem("texgyrepagella-regular"), None);
        assert_eq!(pack_for_stem("NotoSansJP-Regular"), Some("cjk-thai"));
        assert_eq!(pack_for_stem("Helvetica"), None);
        assert_eq!(pack_for_stem("NoSuchFace"), None);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_is_pinned_only_for_the_pinned_index_bytes() {
        let pin = Pinned {
            tag: "fonts-2026.10",
            sha256: "aa",
        };
        let mut p = Installed {
            name: "extras".into(),
            dir: PathBuf::from("/x"),
            tag: "fonts-2026.10".into(),
            pack_sha256: "bb".into(),
            index_sha256: Some("aa".into()),
            source: String::new(),
        };
        assert_eq!(p.selection(Some(pin)), Selection::Pinned);
        assert_eq!(p.selection(None), Selection::UserSelected);
        p.index_sha256 = Some("cc".into());
        assert_eq!(p.selection(Some(pin)), Selection::UserSelected);
        p.index_sha256 = None;
        assert_eq!(p.selection(Some(pin)), Selection::UserSelected);
        p.index_sha256 = Some("aa".into());
        p.tag = "fonts-2026.11".into();
        assert_eq!(p.selection(Some(pin)), Selection::UserSelected);
    }
}
