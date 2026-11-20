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
       pscat lib remove NAME [--force]

install  copy DIR's files into lib/NAME/ in the per-user data directory (or in
         $PSCAT_HOME, if set; `pscat fonts list` shows which), so that
         (lib/NAME/x.ps) run finds them from any directory, also in pscat mcp.
         NAME defaults to DIR's own name. Running it again replaces the
         package: files of the old install that DIR no longer has are removed.
         Refused: a NAME that matches a library built into pscat (artkit,
         qr, ...), so an install can never shadow one; a directory that pscat
         lib did not create. Dotfiles and symlinks in DIR are not copied.
list     the installed packages and where they are
remove   delete lib/NAME/, a package that install put there, and say how many
         files went. Refused: a directory that pscat lib did not create (see
         `pscat lib list`), unless --force; a name that is not a directory.

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
        "remove" => {
            let mut name: Option<String> = None;
            let mut force = false;
            while let Some(a) = args.next() {
                match a.as_str() {
                    "-h" | "--help" => {
                        println!("{USAGE}");
                        return Ok(());
                    }
                    "--force" => force = true,
                    s if s.starts_with("--") => {
                        return Err(Failure::Usage(format!("unknown option {s}"), USAGE));
                    }
                    _ if name.is_none() => name = Some(a),
                    _ => {
                        return Err(Failure::Usage(format!("unexpected argument {a}"), USAGE));
                    }
                }
            }
            let Some(name) = name else {
                return Err(Failure::Usage("remove needs NAME".into(), USAGE));
            };
            remove(&lib_dir()?, &name, force)
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

/// Create a scratch directory that must not already exist. A leftover from an
/// interrupted run (or one made by hand) is not ours to delete, so it is a
/// refusal with the way out, never a `remove_dir_all`.
fn make_scratch(path: &Path) -> Result<(), String> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(format!(
            "{} already exists, and this run did not make it (left by an interrupted \
             install, or made by hand); move or remove it and run again",
            path.display()
        )),
        Err(e) => Err(format!("cannot create {}: {e}", path.display())),
    }
}

/// Open `path` for reading without following a symbolic link in its last
/// component, and only if what was opened is a regular file. The check is on
/// the handle, so a file swapped for a link after [`collect`] looked at it is
/// refused instead of followed.
fn open_regular(path: &Path) -> std::io::Result<fs::File> {
    let mut opts = fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NONBLOCK: a FIFO swapped in cannot hold the open forever.
        opts.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = opts.open(path)?;
    let md = file.metadata()?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if reparse || !md.is_file() {
        return Err(std::io::Error::other("is not a regular file any more"));
    }
    Ok(file)
}

/// Copy one source file to `to` (which must not exist), reading from a single
/// handle opened by [`open_regular`] and never past the byte caps, so a file
/// that grows after [`collect`] sized it cannot take more than the limits
/// allow. `spent` is the bytes copied so far in this install.
fn copy_item(root: &Path, item: &Item, to: &Path, spent: &mut u64) -> std::io::Result<()> {
    let named = |e: std::io::Error| std::io::Error::other(format!("{}: {e}", item.rel));
    // An intermediate directory swapped for a link is caught here when it
    // happened before this call; the window between this check and the open
    // is not closed (that would take openat, which std does not offer).
    if let Some(parent) = item.from.parent() {
        let real = fs::canonicalize(parent).map_err(named)?;
        if !real.starts_with(root) {
            return Err(named(std::io::Error::other("is now outside the package")));
        }
    }
    let mut src = open_regular(&item.from).map_err(named)?;
    let md = src.metadata().map_err(named)?;
    let room = MAX_FILE_BYTES.min(MAX_TOTAL_BYTES.saturating_sub(*spent));
    if md.len() > room {
        return Err(named(std::io::Error::other(format!(
            "is {} bytes, over the {room} bytes still allowed",
            md.len()
        ))));
    }
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    let n = copy_limited(&mut src, &mut dst, room).map_err(named)?;
    dst.set_permissions(md.permissions())?;
    *spent += n;
    Ok(())
}

/// Copy at most `room` bytes: reading one byte more than that is an error,
/// and no more than `room + 1` bytes are ever written.
fn copy_limited(
    from: &mut impl std::io::Read,
    to: &mut impl std::io::Write,
    room: u64,
) -> std::io::Result<u64> {
    use std::io::Read;
    let n = std::io::copy(&mut from.take(room + 1), to)?;
    if n > room {
        return Err(std::io::Error::other(format!(
            "grew past the {room} bytes still allowed while it was copied"
        )));
    }
    Ok(n)
}

/// `fs::rename`, as a plain function so `swap_in` can take a stand-in for it.
fn rename_path(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

/// Put `stage` where `dest` is. With an old package (`had_old`) it is first
/// renamed into `holder` (a directory made here, exclusively), then `stage` is
/// renamed to `dest`. Two renames are not atomic: between them `dest` does not
/// exist, so a program loading `lib/NAME/...` at that instant finds nothing.
/// The window is two back-to-back renames, and a failure of the second puts
/// the old package back. If even that fails, the error names where the old
/// package is, and `holder` is left alone.
fn swap_in(
    stage: &Path,
    dest: &Path,
    holder: &Path,
    had_old: bool,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let old = holder.join("old");
    if had_old {
        make_scratch(holder)?;
        if let Err(e) = rename(dest, &old) {
            let _ = fs::remove_dir(holder);
            return Err(format!("cannot replace {}: {e}", dest.display()));
        }
    }
    if let Err(e) = rename(stage, dest) {
        if !had_old {
            return Err(format!("cannot install {}: {e}", dest.display()));
        }
        return match rename(&old, dest) {
            Ok(()) => {
                let _ = fs::remove_dir(holder);
                Err(format!(
                    "cannot install {}: {e} (the previous install is back in place)",
                    dest.display()
                ))
            }
            Err(e2) => Err(format!(
                "cannot install {}: {e}; and the previous install could not be put back \
                 ({e2}): it is at {}",
                dest.display(),
                old.display()
            )),
        };
    }
    if had_old {
        let _ = fs::remove_dir_all(holder);
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

    // Build the new package beside the old one, then swap. Both scratch
    // directories are made exclusively, so only a directory this run created
    // is ever deleted (a hand-made `.NAME.new-<pid>` is refused, not removed).
    let pid = std::process::id();
    let stage = lib.join(format!(".{name}.new-{pid}"));
    let holder = lib.join(format!(".{name}.old-{pid}"));
    make_scratch(&stage).map_err(fail)?;
    let mut spent = 0u64;
    let built = (|| -> std::io::Result<()> {
        for i in &items {
            let to = stage.join(&i.rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            copy_item(&src, i, &to, &mut spent)?;
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
    if let Err(e) = swap_in(&stage, &dest, &holder, replaced.is_some(), &rename_path) {
        // The stage is ours; the holder is removed by `swap_in` unless it
        // still holds the only copy of the old package.
        let _ = fs::remove_dir_all(&stage);
        return Err(fail(e));
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

/// How many files, symbolic links and the like (everything that is not a
/// directory) are below `dir`, hidden ones included and links not followed,
/// not counting `skip` (the marker). What `remove` reports as gone.
fn count_all(dir: &Path, skip: &Path) -> std::io::Result<usize> {
    let mut n = 0;
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                todo.push(e.path());
            } else if e.path() != skip {
                n += 1;
            }
        }
    }
    Ok(n)
}

/// Delete everything under `dir` but the marker, then the marker, then `dir`.
/// Removing the marker last keeps an interrupted removal re-runnable without
/// `--force`: until the last step the directory is still one `install` made.
/// (`remove_dir_all` does not follow a symbolic link, at the top or below,
/// and has been safe against a swap during the walk since Rust 1.58.1.)
fn remove_tree(dir: &Path, marker: &Path) -> std::io::Result<()> {
    for e in fs::read_dir(dir)? {
        let e = e?;
        let path = e.path();
        let is_dir = e.file_type()?.is_dir();
        if path == marker && !is_dir {
            continue;
        }
        if is_dir {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    if fs::symlink_metadata(marker).is_ok() {
        fs::remove_file(marker)?;
    }
    fs::remove_dir(dir)
}

/// `pscat lib remove NAME [--force]`: delete `lib/NAME/` from the install
/// directory. Only a directory `install` made (one holding a `.pscat-lib`
/// marker that is a regular file) goes without `--force`; a symbolic link or a
/// plain file named NAME is never removed, `--force` or not. NAME is one plain
/// component, so nothing outside `lib/` can be named.
fn remove(lib: &Path, name: &str, force: bool) -> Result<(), Failure> {
    let plain = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !plain {
        return Err(fail(format!(
            "{name:?} is not a package name: use letters, digits, - and _ (at most 64); \
             `pscat lib list` shows what is installed"
        )));
    }
    let dest = lib.join(name);
    let meta = match fs::symlink_metadata(&dest) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(fail(format!(
                "lib/{name} is not installed (looked in {}); `pscat lib list` shows what is",
                lib.display()
            )));
        }
        Err(e) => return Err(fail(format!("cannot read {}: {e}", dest.display()))),
    };
    if !meta.is_dir() {
        return Err(fail(format!(
            "{} is not a directory (a file or a symbolic link); remove it by hand",
            dest.display()
        )));
    }
    let marker = dest.join(MARKER);
    let ours = fs::symlink_metadata(&marker).is_ok_and(|m| m.is_file());
    if !ours && !force {
        return Err(fail(format!(
            "{} was not made by pscat lib install, so it is not removed; \
             pass --force to delete it anyway",
            dest.display()
        )));
    }
    let files = count_all(&dest, &marker)
        .map_err(|e| fail(format!("cannot read {}: {e}", dest.display())))?;
    remove_tree(&dest, &marker)
        .map_err(|e| fail(format!("cannot remove {}: {e}", dest.display())))?;
    println!(
        "removed lib/{name}: {files} file{} from {}",
        plural(files),
        dest.display()
    );
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

    /// A fresh directory for one test (src tests cannot reach
    /// `tests/common`; same naming as the other unit tests here).
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pscat-libpkg-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::canonicalize(&dir).unwrap()
    }

    /// The message of a refused install (`Failure` has no `Debug`).
    fn failure_text(r: Result<(), Failure>) -> String {
        match r {
            Err(Failure::Msg(m)) => m,
            Err(_) => "not a message failure".into(),
            Ok(()) => panic!("expected a refusal"),
        }
    }

    fn item(from: PathBuf, rel: &str) -> Item {
        Item {
            from,
            rel: rel.to_string(),
            len: 0,
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_file_swapped_for_a_link_after_the_scan_is_not_followed() {
        let base = scratch("toctou-link");
        let root = base.join("pkg");
        fs::create_dir_all(&root).unwrap();
        let secret = base.join("secret.txt");
        fs::write(&secret, "private").unwrap();
        // collect saw a regular file here; now it is a link to a file outside.
        let victim = root.join("a.ps");
        fs::write(&victim, "ok").unwrap();
        let mut items = Vec::new();
        collect(&root, 0, "", &mut items).unwrap();
        assert_eq!(items.len(), 1);
        fs::remove_file(&victim).unwrap();
        std::os::unix::fs::symlink(&secret, &victim).unwrap();
        let to = base.join("out.ps");
        let e = copy_item(&root, &items[0], &to, &mut 0).expect_err("followed a link");
        assert!(e.to_string().contains("a.ps"), "{e}");
        assert!(!to.exists() || fs::read_to_string(&to).unwrap().is_empty());
        // A directory swapped for a link to a directory outside is refused too.
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub/b.ps"), "ok").unwrap();
        let mut items = Vec::new();
        collect(&root, 0, "", &mut items).unwrap();
        let b = items.iter().find(|i| i.rel == "sub/b.ps").unwrap();
        let outside = base.join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("b.ps"), "stolen").unwrap();
        fs::remove_dir_all(root.join("sub")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("sub")).unwrap();
        let to = base.join("out2.ps");
        let e = copy_item(&root, b, &to, &mut 0).expect_err("followed a directory link");
        assert!(e.to_string().contains("outside"), "{e}");
        assert!(!to.exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_swapped_in_does_not_hang_or_copy() {
        let base = scratch("toctou-fifo");
        let fifo = base.join("f.ps");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let to = base.join("out.ps");
        assert!(copy_item(&base, &item(fifo, "f.ps"), &to, &mut 0).is_err());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_file_that_grew_is_cut_off_at_the_cap() {
        let base = scratch("toctou-grow");
        let from = base.join("a.ps");
        fs::write(&from, vec![b'x'; 100]).unwrap();
        let to = base.join("out.ps");
        // collect sized it small; the total allowance left is 40 bytes.
        let mut spent = MAX_TOTAL_BYTES - 40;
        let e = copy_item(&base, &item(from.clone(), "a.ps"), &to, &mut spent)
            .expect_err("copied past the cap");
        assert!(e.to_string().contains("a.ps"), "{e}");
        // The copy never wrote more than the allowance plus the probe byte.
        assert!(!to.exists() || fs::metadata(&to).unwrap().len() <= 41);
        // Within the allowance it copies, and counts what it copied.
        let mut spent = 0;
        let to2 = base.join("out2.ps");
        copy_item(&base, &item(from, "a.ps"), &to2, &mut spent).unwrap();
        assert_eq!(spent, 100);
        assert_eq!(fs::metadata(&to2).unwrap().len(), 100);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn the_copy_stops_one_byte_past_its_room() {
        // A reader that holds more than `room`, as a file that grew after
        // the scan would: it errors, and never writes more than room + 1.
        let room = 64u64;
        let data = vec![b'x'; room as usize + 10];
        let mut out = Vec::new();
        let e = copy_limited(&mut std::io::Cursor::new(&data), &mut out, room)
            .expect_err("copied past the room");
        assert!(e.to_string().contains("grew past"), "{e}");
        assert_eq!(out.len() as u64, room + 1);
        let mut out = Vec::new();
        let n = copy_limited(&mut std::io::Cursor::new(&data[..64]), &mut out, room).unwrap();
        assert_eq!((n, out.len()), (64, 64));
    }

    #[test]
    fn a_scratch_directory_that_exists_is_refused_and_left_alone() {
        let base = scratch("scratch-collide");
        let src = base.join("mypkg");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("a.ps"), "1").unwrap();
        let lib = base.join("lib");
        fs::create_dir_all(&lib).unwrap();
        let pid = std::process::id();
        // Hand-made directories with the names this run would use.
        for leftover in [format!(".mypkg.new-{pid}"), format!(".mypkg.old-{pid}")] {
            let hand = lib.join(&leftover);
            fs::create_dir_all(&hand).unwrap();
            fs::write(hand.join("precious.txt"), "mine").unwrap();
            // The .old holder is only made when replacing, so install once first.
            let first = install(&lib, &src, None, &[]);
            if leftover.contains(".new-") {
                let e = failure_text(first);
                assert!(e.contains("already exists"), "{e}");
                assert!(!lib.join("mypkg").exists());
            } else {
                assert!(first.is_ok());
                let e = failure_text(install(&lib, &src, None, &[]));
                assert!(e.contains("already exists"), "{e}");
                // The installed package is untouched by the refused re-install.
                assert!(lib.join("mypkg/a.ps").is_file());
            }
            assert_eq!(
                fs::read_to_string(hand.join("precious.txt")).unwrap(),
                "mine",
                "{leftover} was deleted"
            );
            assert!(
                fs::read_dir(&lib)
                    .unwrap()
                    .filter_map(Result::ok)
                    .all(|e| !e.file_name().to_string_lossy().contains(".new-")
                        || e.file_name().to_string_lossy() == leftover),
                "a stage of this run was left behind"
            );
            fs::remove_dir_all(&hand).unwrap();
            let _ = fs::remove_dir_all(lib.join("mypkg"));
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failed_second_rename_puts_the_old_package_back() {
        let base = scratch("swap-fail");
        let dest = base.join("pkg");
        let stage = base.join(".pkg.new");
        let holder = base.join(".pkg.old");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("old.ps"), "old").unwrap();
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("new.ps"), "new").unwrap();
        // Fail the rename of the stage into place, pass the others.
        let from_stage = stage.clone();
        let failing = move |a: &Path, b: &Path| {
            if a == from_stage {
                Err(std::io::Error::other("injected"))
            } else {
                fs::rename(a, b)
            }
        };
        let e = swap_in(&stage, &dest, &holder, true, &failing).expect_err("must fail");
        assert!(e.contains("injected") && e.contains("back in place"), "{e}");
        assert_eq!(fs::read_to_string(dest.join("old.ps")).unwrap(), "old");
        assert!(!holder.exists(), "holder not cleaned up");
        assert!(stage.is_dir(), "swap_in leaves the stage to its caller");
        // If putting it back fails too, the error says where the old one is.
        let all_fail = |a: &Path, b: &Path| {
            if a.ends_with("old") || a == stage {
                Err(std::io::Error::other("injected"))
            } else {
                fs::rename(a, b)
            }
        };
        let e = swap_in(&stage, &dest, &holder, true, &all_fail).expect_err("must fail");
        assert!(e.contains("could not be put back"), "{e}");
        assert!(e.contains(holder.to_str().unwrap()), "{e}");
        // Put the old package back by hand, then success replaces it and
        // removes the holder.
        fs::rename(holder.join("old"), &dest).unwrap();
        fs::remove_dir(&holder).unwrap();
        swap_in(&stage, &dest, &holder, true, &rename_path).unwrap();
        assert!(dest.join("new.ps").is_file() && !dest.join("old.ps").exists());
        assert!(!holder.exists());
        let _ = fs::remove_dir_all(&base);
    }
}
