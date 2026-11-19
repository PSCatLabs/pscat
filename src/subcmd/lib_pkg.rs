//! `pscat lib install DIR [--name NAME]` and `pscat lib list` (issue #1056):
//! put a library of your own where `(lib/NAME/x.ps) run` finds it from any
//! directory, in the CLI and in `pscat mcp`.
//!
//! The package is copied to `lib/NAME/` under `$PSCAT_HOME` if that is set,
//! else the per-user data directory (the place `pscat fonts list` prints
//! `fonts/packs` under). Nothing about the search order or the read roots
//! changes: that directory is already step 2 or 4 of the `lib/` search and
//! already a read root of the MCP server. What this module adds is the
//! guarantee that an install cannot *shadow* the compiled-in library, which
//! the search would otherwise prefer a file in the data directory over
//! (pscat-mcp#87 relies on a planted `lib/*.ps` never doing that):
//!
//! * `NAME` is `[A-Za-z0-9_-]+`, so it is one path component and cannot be
//!   `..`, hidden, or carry a separator;
//! * `NAME` may not match a compiled-in `lib/` entry (a file, its stem, or a
//!   subdirectory), compared without regard to case, since the data
//!   directory can be on a case-folding file system;
//! * no file of the package may land on a compiled-in name either.
//!
//! Only regular files are copied (a symlink in DIR could otherwise carry a
//! file from outside it into a directory the MCP server may read), and a
//! re-install builds the new copy beside the old one and swaps them, so
//! nothing of the old package survives into the new one.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Args, Failure};

const USAGE: &str = "usage: pscat lib install DIR [--name NAME]
       pscat lib list

install  copy DIR's files into lib/NAME/ in the per-user data directory (or in
         $PSCAT_HOME, if set; `pscat fonts list` shows which), so that
         (lib/NAME/x.ps) run finds them from any directory, also in pscat mcp.
         NAME defaults to DIR's own name. Running it again replaces the
         package: files of the old install that DIR no longer has are removed.
         Refused: a NAME that matches a library built into pscat (artkit,
         qr, ...), so an install can never shadow one; a directory that pscat
         lib did not create. Dotfiles and symlinks in DIR are not copied.
list     the installed packages and where they are

A library that loads files beside itself can ask where it was loaded from:
currentfilename returns the name it was run by, e.g. lib/NAME/x.ps.";

/// What marks a package directory as made by `pscat lib install`, so a
/// re-install replaces only what an install made. A dotfile: never copied
/// from DIR, and not a name a program loads.
const MARKER: &str = ".pscat-lib";

/// More than this many files, or this many bytes in all, is not a library;
/// a DIR that large is a mistake (`/`, a home directory), refused before a
/// byte is copied.
const MAX_FILES: usize = 2000;
const MAX_TOTAL_BYTES: u64 = 256 << 20;
/// The most a `lib/` file may hold before the resolver skips it, so that an
/// install never puts a file where a program then cannot find it.
const MAX_FILE_BYTES: u64 = 64 << 20;
const MAX_DEPTH: usize = 16;

pub(crate) fn run(mut args: Args) -> Result<(), Failure> {
    let Some(cmd) = args.next() else {
        return Err(Failure::Usage("missing command".into(), USAGE));
    };
    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "list" => {
            if let Some(a) = args.next() {
                return Err(Failure::Usage(format!("unexpected argument {a}"), USAGE));
            }
            list(&lib_dir()?)
        }
        "install" => {
            let mut dir: Option<String> = None;
            let mut name: Option<String> = None;
            while let Some(a) = args.next() {
                match a.as_str() {
                    "-h" | "--help" => {
                        println!("{USAGE}");
                        return Ok(());
                    }
                    "--name" => name = Some(args.value("--name").map_err(usage)?),
                    s if s.starts_with("--") => {
                        return Err(Failure::Usage(format!("unknown option {s}"), USAGE));
                    }
                    _ if dir.is_none() => dir = Some(a),
                    _ => {
                        return Err(Failure::Usage(format!("unexpected argument {a}"), USAGE));
                    }
                }
            }
            let Some(dir) = dir else {
                return Err(Failure::Usage("install needs DIR".into(), USAGE));
            };
            let builtins: Vec<&str> = pscat::paths::embedded_names().collect();
            install(&lib_dir()?, Path::new(&dir), name.as_deref(), &builtins)
        }
        other => Err(Failure::Usage(format!("unknown command {other:?}"), USAGE)),
    }
}

fn usage(msg: String) -> Failure {
    Failure::Usage(msg, USAGE)
}

fn fail(msg: impl Into<String>) -> Failure {
    Failure::Msg(msg.into())
}

/// `lib/` in the install directory (it may not exist yet).
fn lib_dir() -> Result<PathBuf, Failure> {
    pscat::paths::process_roots()
        .lib_install_dir()
        .ok_or_else(|| fail("no place to install: set PSCAT_HOME, or HOME (APPDATA on Windows)"))
}

/// The lower-cased first path component under `lib/` of every compiled-in
/// entry, with and without its extension: `artkit.ps` gives `artkit.ps` and
/// `artkit`; `fonts/neon.ps` gives `fonts`.
fn builtin_heads(builtins: &[&str]) -> BTreeSet<String> {
    let mut heads = BTreeSet::new();
    for n in builtins {
        let Some(rest) = n.strip_prefix("lib/") else {
            continue;
        };
        let head = rest.split('/').next().unwrap_or(rest).to_ascii_lowercase();
        if let Some((stem, _)) = head.split_once('.') {
            heads.insert(stem.to_string());
        }
        heads.insert(head);
    }
    heads
}

/// Whether `NAME` may name a package: one plain component that no
/// compiled-in `lib/` entry answers to.
fn check_name(name: &str, builtins: &[&str]) -> Result<(), String> {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !plain {
        return Err(format!(
            "{name:?} is not a usable package name: use letters, digits, - and _ (at most 64); \
             pass --name to pick one"
        ));
    }
    if builtin_heads(builtins).contains(&name.to_ascii_lowercase()) {
        return Err(format!(
            "{name:?} is the name of a library built into pscat (lib/{name}...); \
             a package may not shadow it. Pass --name with another name"
        ));
    }
    Ok(())
}

/// Whether `lib/NAME/REL` is, or differs only in case from, a compiled-in name.
fn is_builtin_path(path: &str, builtins: &[&str]) -> bool {
    builtins.iter().any(|b| b.eq_ignore_ascii_case(path))
}

/// One file to copy: where it is, and its `/`-joined name inside the package.
struct Item {
    from: PathBuf,
    rel: String,
    len: u64,
}

/// The regular files under `dir`, hidden entries and symlinks left out (each
/// skip noted on stderr), sorted by name.
fn collect(dir: &Path, depth: usize, prefix: &str, out: &mut Vec<Item>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err(format!(
            "{} is nested deeper than {MAX_DEPTH} levels",
            dir.display()
        ));
    }
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let fname = e.file_name();
        let Some(fname) = fname.to_str() else {
            return Err(format!(
                "{} holds a file name that is not UTF-8",
                dir.display()
            ));
        };
        let rel = format!("{prefix}{fname}");
        if fname.starts_with('.') {
            continue;
        }
        let path = e.path();
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        if kind.file_type().is_symlink() {
            eprintln!("pscat lib: not copied: {rel} is a symbolic link");
        } else if kind.is_dir() {
            collect(&path, depth + 1, &format!("{rel}/"), out)?;
        } else if kind.is_file() {
            if kind.len() > MAX_FILE_BYTES {
                return Err(format!(
                    "{rel} is {} bytes; pscat does not load a lib/ file over {MAX_FILE_BYTES}",
                    kind.len()
                ));
            }
            out.push(Item {
                from: path,
                rel,
                len: kind.len(),
            });
        } else {
            eprintln!("pscat lib: not copied: {rel} is not a regular file");
        }
        if out.len() > MAX_FILES {
            return Err(format!(
                "more than {MAX_FILES} files: {} is not a library directory",
                dir.display()
            ));
        }
    }
    Ok(())
}

/// How many regular files are below `dir`, hidden ones and symlinks not counted.
fn count_files(dir: &Path) -> usize {
    let mut files = Vec::new();
    let _ = collect_quiet(dir, 0, &mut files);
    files.len()
}

/// [`collect`] without the notes, for counting what an install holds.
fn collect_quiet(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for e in fs::read_dir(dir)? {
        let e = e?;
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let t = e.file_type()?;
        if t.is_dir() {
            collect_quiet(&e.path(), depth + 1, out)?;
        } else if t.is_file() {
            out.push(e.path());
        }
    }
    Ok(())
}

fn install(lib: &Path, dir: &Path, name: Option<&str>, builtins: &[&str]) -> Result<(), Failure> {
    let src =
        fs::canonicalize(dir).map_err(|e| fail(format!("cannot read {}: {e}", dir.display())))?;
    if !src.is_dir() {
        return Err(fail(format!("{} is not a directory", dir.display())));
    }
    let name = match name {
        Some(n) => n.to_string(),
        None => src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| fail("DIR has no name of its own to use; pass --name"))?
            .to_string(),
    };
    check_name(&name, builtins).map_err(fail)?;

    let mut items = Vec::new();
    collect(&src, 0, "", &mut items).map_err(fail)?;
    if items.is_empty() {
        return Err(fail(format!("{} holds no files to install", src.display())));
    }
    let total: u64 = items.iter().map(|i| i.len).sum();
    if total > MAX_TOTAL_BYTES {
        return Err(fail(format!(
            "{} holds {total} bytes; {} is not a library directory",
            src.display(),
            src.display()
        )));
    }
    // Nothing of the package may land on a compiled-in name.
    for i in &items {
        let path = format!("lib/{name}/{}", i.rel);
        if is_builtin_path(&path, builtins) {
            return Err(fail(format!(
                "{path} would shadow a library built into pscat"
            )));
        }
    }

    fs::create_dir_all(lib).map_err(|e| fail(format!("cannot create {}: {e}", lib.display())))?;
    let lib =
        fs::canonicalize(lib).map_err(|e| fail(format!("cannot read {}: {e}", lib.display())))?;
    // Installing from the install directory, or from a directory that holds
    // it, would copy a tree into itself or replace its own source.
    if src.starts_with(&lib) || lib.starts_with(&src) {
        return Err(fail(format!(
            "{} and the install directory {} overlap; copy the package elsewhere first",
            src.display(),
            lib.display()
        )));
    }

    let dest = lib.join(&name);
    let mut replaced: Option<BTreeSet<String>> = None;
    match fs::symlink_metadata(&dest) {
        Ok(m) if m.is_dir() && dest.join(MARKER).is_file() => {
            let mut old = Vec::new();
            collect_quiet(&dest, 0, &mut old).map_err(|e| fail(e.to_string()))?;
            replaced = Some(
                old.iter()
                    .filter_map(|p| p.strip_prefix(&dest).ok())
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .collect(),
            );
        }
        Ok(m) if m.is_dir() => {
            return Err(fail(format!(
                "{} exists and was not made by pscat lib install; \
                 move it away or pick another --name",
                dest.display()
            )));
        }
        Ok(_) => {
            return Err(fail(format!(
                "{} exists and is not a directory; move it away or pick another --name",
                dest.display()
            )));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(fail(format!("cannot read {}: {e}", dest.display()))),
    }

    // Build the new package beside the old one, then swap.
    let pid = std::process::id();
    let stage = lib.join(format!(".{name}.new-{pid}"));
    let aside = lib.join(format!(".{name}.old-{pid}"));
    let _ = fs::remove_dir_all(&stage);
    let built = (|| -> std::io::Result<()> {
        for i in &items {
            let to = stage.join(&i.rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&i.from, &to)?;
        }
        fs::write(
            stage.join(MARKER),
            format!("installed by pscat lib install from {}\n", src.display()),
        )
    })();
    if let Err(e) = built {
        let _ = fs::remove_dir_all(&stage);
        return Err(fail(format!("cannot copy into {}: {e}", stage.display())));
    }
    if replaced.is_some() {
        let _ = fs::remove_dir_all(&aside);
        if let Err(e) = fs::rename(&dest, &aside) {
            let _ = fs::remove_dir_all(&stage);
            return Err(fail(format!("cannot replace {}: {e}", dest.display())));
        }
    }
    if let Err(e) = fs::rename(&stage, &dest) {
        if replaced.is_some() {
            let _ = fs::rename(&aside, &dest);
        }
        let _ = fs::remove_dir_all(&stage);
        return Err(fail(format!("cannot install {}: {e}", dest.display())));
    }
    if replaced.is_some() {
        let _ = fs::remove_dir_all(&aside);
    }

    match replaced {
        None => println!(
            "installed lib/{name}: {} file{} in {}",
            items.len(),
            plural(items.len()),
            dest.display()
        ),
        Some(old) => {
            let new: BTreeSet<&str> = items.iter().map(|i| i.rel.as_str()).collect();
            let gone: Vec<&str> = old
                .iter()
                .map(String::as_str)
                .filter(|f| !new.contains(f))
                .collect();
            println!(
                "replaced lib/{name}: {} file{} in {} (the previous install's {} file{} are gone)",
                items.len(),
                plural(items.len()),
                dest.display(),
                old.len(),
                plural(old.len())
            );
            if !gone.is_empty() {
                println!(
                    "  no longer in the package, so removed: {}",
                    gone.join(", ")
                );
            }
        }
    }
    if let Some(first) = items.iter().find(|i| i.rel.ends_with(".ps")) {
        println!("load a file with: (lib/{name}/{}) run", first.rel);
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn list(lib: &Path) -> Result<(), Failure> {
    println!("install directory: {}", lib.display());
    let mut names: Vec<_> = match fs::read_dir(lib) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    if names.is_empty() {
        println!("no packages installed (pscat lib install DIR)");
        return Ok(());
    }
    for n in names {
        let path = lib.join(&n);
        let from = fs::read_to_string(path.join(MARKER))
            .ok()
            .and_then(|m| {
                m.trim()
                    .strip_prefix("installed by pscat lib install from ")
                    .map(str::to_string)
            })
            .map_or_else(
                || "not made by pscat lib install".to_string(),
                |s| format!("from {s}"),
            );
        let files = count_files(&path);
        println!(
            "lib/{n}: {files} file{} at {} ({from})",
            plural(files),
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTINS: &[&str] = &[
        "lib/artkit.ps",
        "lib/fonts/neon.ps",
        "lib/styles/a.ps",
        "lib/Mixed.PS",
    ];

    #[test]
    fn a_name_is_one_plain_component() {
        for ok in ["mypkg", "sheet_music", "a-b", "X9"] {
            assert!(check_name(ok, BUILTINS).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "..",
            ".hidden",
            "a/b",
            "a\\b",
            "a b",
            "sheet.music",
            "é",
        ] {
            assert!(check_name(bad, BUILTINS).is_err(), "{bad:?}");
        }
        assert!(check_name(&"a".repeat(65), BUILTINS).is_err());
    }

    #[test]
    fn a_name_may_not_shadow_a_compiled_in_entry() {
        // A file's stem, a file with its extension left off in other ways,
        // a directory, and the same in another case.
        for bad in [
            "artkit", "ARTKIT", "Artkit", "fonts", "Fonts", "styles", "mixed", "MIXED",
        ] {
            let err = check_name(bad, BUILTINS).expect_err(bad);
            assert!(err.contains("built into pscat"), "{bad}: {err}");
        }
        assert!(check_name("artkit2", BUILTINS).is_ok());
    }

    #[test]
    fn a_file_may_not_land_on_a_compiled_in_name() {
        assert!(is_builtin_path("lib/artkit.ps", BUILTINS));
        assert!(is_builtin_path("lib/ARTKIT.PS", BUILTINS));
        assert!(!is_builtin_path("lib/mypkg/artkit.ps", BUILTINS));
    }

    #[test]
    fn every_real_compiled_in_name_is_refused() {
        let real: Vec<&str> = pscat::paths::embedded_names().collect();
        assert!(real.len() > 20);
        for n in &real {
            let Some(rest) = n.strip_prefix("lib/") else {
                continue;
            };
            let head = rest.split('/').next().unwrap();
            let stem = head.split('.').next().unwrap();
            for candidate in [stem, &stem.to_ascii_uppercase()] {
                // A stem with a character a name cannot have is refused for
                // that, which is as good.
                assert!(check_name(candidate, &real).is_err(), "{candidate}");
            }
        }
    }
}
