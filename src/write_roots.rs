//! Where a program may write files (issue #740).
//!
//! Writing is an interpreter capability, off by default: an [`Interp`]
//! starts with no write roots, so `(path) (w) file`, `renamefile` and
//! `deletefile` refuse every disk path with `invalidfileaccess` until the
//! embedder names directories with [`Interp::set_write_roots`] (the CLI's
//! `--allow-write DIR`). `pscat-mcp` never names any. `%stdout` and
//! `%stderr` are not disk paths and need no root.
//!
//! The checks, in order, for a name a program writes to:
//! - a `..` component anywhere is refused outright, before any lookup;
//! - a relative name is relative to the current directory only, never the
//!   read search order (`src/paths.rs`);
//! - the path is canonicalised (an existing target in full, so a symlink
//!   is followed to where it really points; a new file through its parent
//!   directory) and must land strictly inside a canonicalised root;
//! - an existing symlink that points nowhere is refused (opening it would
//!   create its target, wherever that is);
//! - on Unix, an existing file with more than one hard link is refused for
//!   writing, since another of its names may be outside the roots.
//!
//! `renamefile`/`deletefile` act on a directory *entry* rather than on what
//! it points to ([`WriteRoots::resolve_entry`]): only the parent directory
//! is canonicalised, and the final name is used as is, because unlinking or
//! renaming a symlink touches the link, never its target.
//!
//! Threat model. The program being run is the adversary; it cannot create
//! symlinks or hard links (no operator does), so the links it could abuse
//! are ones already inside a root. The check and the open are not atomic:
//! a *different* process with write access to a root could swap a
//! directory for a symlink in between. New files are opened with
//! `create_new` (`O_EXCL`), which never follows a final-component symlink,
//! so the remaining window needs such a concurrent local writer.
//!
//! [`Interp`]: crate::Interp
//! [`Interp::set_write_roots`]: crate::Interp::set_write_roots

use std::io;
use std::path::{Component, Path, PathBuf};

use crate::error::PsError;

/// The directories a program may write in, canonicalised. Empty (the
/// default) means no disk writes at all.
#[derive(Debug, Default, Clone)]
pub struct WriteRoots {
    roots: Vec<PathBuf>,
}

/// How a file is opened for writing: `(w)` truncates, `(a)` appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteMode {
    Truncate,
    Append,
}

impl WriteRoots {
    /// Canonicalise each directory. Fails, naming the first bad one, if a
    /// directory doesn't exist or isn't a directory.
    pub fn new<P: AsRef<Path>>(dirs: &[P]) -> Result<WriteRoots, String> {
        let mut roots = Vec::with_capacity(dirs.len());
        for d in dirs {
            let d = d.as_ref();
            let canon =
                std::fs::canonicalize(d).map_err(|e| format!("write root {}: {e}", d.display()))?;
            if !canon.is_dir() {
                return Err(format!("write root {}: not a directory", d.display()));
            }
            roots.push(canon);
        }
        Ok(WriteRoots { roots })
    }

    /// The canonical roots.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    fn contains(&self, canon: &Path) -> bool {
        // `starts_with` compares whole components, so `/out` does not
        // contain `/outside`. Strictly inside: a root itself is a
        // directory, never a file to write or remove.
        self.roots
            .iter()
            .any(|r| canon != r.as_path() && canon.starts_with(r))
    }

    fn contains_dir(&self, canon_dir: &Path) -> bool {
        self.roots.iter().any(|r| canon_dir.starts_with(r))
    }

    /// The absolute, `..`-free form of a program's name, plus its final
    /// component.
    fn absolute(&self, name: &str) -> Result<(PathBuf, std::ffi::OsString), PsError> {
        if self.roots.is_empty() {
            return Err(PsError::InvalidFileAccess);
        }
        if name.is_empty() {
            return Err(PsError::UndefinedFilename);
        }
        if name.contains('\0') {
            return Err(PsError::InvalidFileAccess);
        }
        // `Path` drops a trailing `/` or `/.`, which would turn `(d/) (w)
        // file` into the file `d` (POSIX refuses it), or make the existence
        // test below look at a different entry than the OS would open.
        if name == "." || name.ends_with('/') || name.ends_with("/.") {
            return Err(PsError::InvalidFileAccess);
        }
        let p = Path::new(name);
        if p.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(PsError::InvalidFileAccess);
        }
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().map_err(|_| PsError::Io)?.join(p)
        };
        let file = abs
            .file_name()
            .ok_or(PsError::InvalidFileAccess)?
            .to_os_string();
        Ok((abs, file))
    }

    /// The canonical parent of `abs`, which must lie inside a root.
    fn checked_parent(&self, abs: &Path) -> Result<PathBuf, PsError> {
        let parent = abs.parent().ok_or(PsError::InvalidFileAccess)?;
        let canon = std::fs::canonicalize(parent).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => PsError::UndefinedFilename,
            _ => PsError::InvalidFileAccess,
        })?;
        if !self.contains_dir(&canon) {
            return Err(PsError::InvalidFileAccess);
        }
        Ok(canon)
    }

    /// The path to open for writing `name`: an existing file's canonical
    /// path, or a new file's canonical parent joined with its name.
    /// `invalidfileaccess` for anything outside the roots (module doc).
    /// This is the one place a writable path is decided; every operator
    /// that writes to disk goes through it or [`Self::open`].
    pub fn resolve_open(&self, name: &str) -> Result<PathBuf, PsError> {
        self.resolve(name).map(|(path, _)| path)
    }

    /// [`Self::resolve_open`], plus whether the target already existed.
    fn resolve(&self, name: &str) -> Result<(PathBuf, bool), PsError> {
        let (abs, file) = self.absolute(name)?;
        match std::fs::symlink_metadata(&abs) {
            Ok(_) => Ok((self.check_existing(&abs)?, true)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Ok((self.checked_parent(&abs)?.join(file), false))
            }
            Err(_) => Err(PsError::InvalidFileAccess),
        }
    }

    fn check_existing(&self, abs: &Path) -> Result<PathBuf, PsError> {
        // A dangling symlink fails to canonicalise: refused, since opening
        // it for writing would create whatever it points at.
        let canon = std::fs::canonicalize(abs).map_err(|_| PsError::InvalidFileAccess)?;
        if !self.contains(&canon) {
            return Err(PsError::InvalidFileAccess);
        }
        let meta = std::fs::metadata(&canon).map_err(|_| PsError::InvalidFileAccess)?;
        // Regular files only: a directory can't be opened for writing, and
        // a FIFO or device would block or reach outside the disk.
        if !meta.is_file() {
            return Err(PsError::InvalidFileAccess);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.nlink() > 1 {
                return Err(PsError::InvalidFileAccess);
            }
        }
        Ok(canon)
    }

    /// Open `name` for writing, checked by [`Self::resolve_open`]. A new
    /// file is created with `create_new` (`O_EXCL`), which never follows a
    /// final-component symlink; losing that race to a regular file
    /// re-checks it as an existing one. An existing file is re-checked
    /// right before it is opened, and the open never creates anything, so
    /// a symlink swapped in by another process is the module doc's only
    /// remaining window.
    pub fn open(&self, name: &str, mode: WriteMode) -> Result<std::fs::File, PsError> {
        let (path, existed) = self.resolve(name)?;
        let mut opts = std::fs::OpenOptions::new();
        match mode {
            WriteMode::Truncate => opts.write(true),
            WriteMode::Append => opts.append(true),
        };
        let opened = if existed {
            let path = self.check_existing(&path)?;
            if mode == WriteMode::Truncate {
                opts.truncate(true);
            }
            opts.open(path)
        } else {
            let mut fresh = opts.clone();
            fresh.create_new(true);
            match fresh.open(&path) {
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let path = self.check_existing(&path)?;
                    if mode == WriteMode::Truncate {
                        opts.truncate(true);
                    }
                    opts.open(path)
                }
                other => other,
            }
        };
        opened.map_err(io_error)
    }

    /// The directory entry `name` names, for `renamefile`/`deletefile`:
    /// its parent canonicalised and inside a root, its final component
    /// untouched (a symlink is the link itself). With `must_exist`, a
    /// missing entry is `undefinedfilename`.
    pub fn resolve_entry(&self, name: &str, must_exist: bool) -> Result<PathBuf, PsError> {
        let (abs, file) = self.absolute(name)?;
        let entry = self.checked_parent(&abs)?.join(file);
        if must_exist {
            match std::fs::symlink_metadata(&entry) {
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    return Err(PsError::UndefinedFilename);
                }
                Err(_) => return Err(PsError::InvalidFileAccess),
            }
        }
        Ok(entry)
    }

    /// `deletefile`: remove the file entry `name`.
    pub fn delete(&self, name: &str) -> Result<(), PsError> {
        let entry = self.resolve_entry(name, true)?;
        let meta = std::fs::symlink_metadata(&entry).map_err(io_error)?;
        if meta.is_dir() {
            return Err(PsError::InvalidFileAccess);
        }
        std::fs::remove_file(&entry).map_err(io_error)
    }

    /// `renamefile`: move the entry `old` to `new`, both inside the roots.
    pub fn rename(&self, old: &str, new: &str) -> Result<(), PsError> {
        let from = self.resolve_entry(old, true)?;
        let to = self.resolve_entry(new, false)?;
        // Files, as the PLRM says, and as `deletefile` treats them.
        let is_dir = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
        if is_dir(&from) || is_dir(&to) {
            return Err(PsError::InvalidFileAccess);
        }
        std::fs::rename(&from, &to).map_err(io_error)
    }
}

/// An I/O failure as a PostScript error: missing is `undefinedfilename`,
/// refused is `invalidfileaccess`, anything else `ioerror`.
pub fn io_error(e: io::Error) -> PsError {
    match e.kind() {
        io::ErrorKind::NotFound => PsError::UndefinedFilename,
        io::ErrorKind::PermissionDenied
        | io::ErrorKind::AlreadyExists
        | io::ErrorKind::IsADirectory => PsError::InvalidFileAccess,
        _ => PsError::Io,
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    /// A temp tree: `root/` (the write root) and `outside/` beside it.
    struct Tree {
        _dir: tempdir::Dir,
        root: PathBuf,
        outside: PathBuf,
    }

    mod tempdir {
        pub struct Dir(pub std::path::PathBuf);
        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn tree(tag: &str) -> Tree {
        let base =
            std::env::temp_dir().join(format!("pscat-write-roots-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root/sub")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        // macOS temp dirs sit behind the /var -> /private/var symlink, so
        // the roots are named through a symlinked path in every test.
        Tree {
            root: base.join("root"),
            outside: base.join("outside"),
            _dir: tempdir::Dir(base),
        }
    }

    fn roots(t: &Tree) -> WriteRoots {
        WriteRoots::new(&[&t.root]).unwrap()
    }

    fn s(p: &Path) -> String {
        p.to_str().unwrap().to_string()
    }

    #[test]
    fn empty_roots_refuse_everything() {
        let t = tree("empty");
        let w = WriteRoots::default();
        let target = s(&t.root.join("a.txt"));
        assert_eq!(w.resolve_open(&target), Err(PsError::InvalidFileAccess));
        assert_eq!(w.delete(&target), Err(PsError::InvalidFileAccess));
    }

    #[test]
    fn writes_inside_the_root() {
        let t = tree("inside");
        let w = roots(&t);
        use std::io::Write;
        let mut f = w
            .open(&s(&t.root.join("sub/a.txt")), WriteMode::Truncate)
            .unwrap();
        f.write_all(b"hi").unwrap();
        drop(f);
        let mut f = w
            .open(&s(&t.root.join("sub/a.txt")), WriteMode::Append)
            .unwrap();
        f.write_all(b"!").unwrap();
        drop(f);
        assert_eq!(fs::read(t.root.join("sub/a.txt")).unwrap(), b"hi!");
    }

    #[test]
    fn dotdot_is_refused_even_when_it_stays_inside() {
        let t = tree("dotdot");
        let w = roots(&t);
        for name in [
            s(&t.root.join("../outside/x.txt")),
            s(&t.root.join("sub/../a.txt")),
        ] {
            assert_eq!(
                w.resolve_open(&name),
                Err(PsError::InvalidFileAccess),
                "{name}"
            );
        }
        assert!(!t.outside.join("x.txt").exists());
    }

    #[test]
    fn absolute_path_outside_is_refused() {
        let t = tree("abs");
        let w = roots(&t);
        let name = s(&t.outside.join("x.txt"));
        assert_eq!(
            w.open(&name, WriteMode::Truncate).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert!(!t.outside.join("x.txt").exists());
    }

    #[test]
    fn sibling_prefix_is_not_inside() {
        let t = tree("prefix");
        let sibling = t.root.with_file_name("rootx");
        fs::create_dir_all(&sibling).unwrap();
        let w = roots(&t);
        assert_eq!(
            w.resolve_open(&s(&sibling.join("a"))),
            Err(PsError::InvalidFileAccess)
        );
    }

    #[test]
    fn file_symlink_pointing_outside_is_refused() {
        let t = tree("filelink");
        fs::write(t.outside.join("victim.txt"), b"keep").unwrap();
        symlink(t.outside.join("victim.txt"), t.root.join("link.txt")).unwrap();
        let w = roots(&t);
        let name = s(&t.root.join("link.txt"));
        assert_eq!(
            w.open(&name, WriteMode::Truncate).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert_eq!(
            w.open(&name, WriteMode::Append).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert_eq!(fs::read(t.outside.join("victim.txt")).unwrap(), b"keep");
    }

    #[test]
    fn directory_symlink_mid_path_pointing_outside_is_refused() {
        let t = tree("dirlink");
        symlink(&t.outside, t.root.join("door")).unwrap();
        let w = roots(&t);
        let name = s(&t.root.join("door/new.txt"));
        assert_eq!(
            w.open(&name, WriteMode::Truncate).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert!(!t.outside.join("new.txt").exists());
        // Rename/delete through it too.
        fs::write(t.outside.join("v.txt"), b"keep").unwrap();
        assert_eq!(
            w.delete(&s(&t.root.join("door/v.txt"))),
            Err(PsError::InvalidFileAccess)
        );
        assert!(t.outside.join("v.txt").exists());
    }

    #[test]
    fn dangling_symlink_does_not_create_its_target() {
        let t = tree("dangling");
        symlink(t.outside.join("created.txt"), t.root.join("dangle")).unwrap();
        let w = roots(&t);
        let name = s(&t.root.join("dangle"));
        assert_eq!(
            w.open(&name, WriteMode::Truncate).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert_eq!(
            w.open(&name, WriteMode::Append).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert!(!t.outside.join("created.txt").exists());
    }

    #[test]
    fn symlink_inside_pointing_inside_is_allowed() {
        let t = tree("innerlink");
        fs::write(t.root.join("sub/real.txt"), b"").unwrap();
        symlink(t.root.join("sub/real.txt"), t.root.join("alias.txt")).unwrap();
        let w = roots(&t);
        use std::io::Write;
        w.open(&s(&t.root.join("alias.txt")), WriteMode::Truncate)
            .unwrap()
            .write_all(b"ok")
            .unwrap();
        assert_eq!(fs::read(t.root.join("sub/real.txt")).unwrap(), b"ok");
    }

    #[test]
    fn hard_link_to_an_outside_file_is_refused() {
        let t = tree("hardlink");
        fs::write(t.outside.join("v.txt"), b"keep").unwrap();
        fs::hard_link(t.outside.join("v.txt"), t.root.join("h.txt")).unwrap();
        let w = roots(&t);
        let name = s(&t.root.join("h.txt"));
        assert_eq!(
            w.open(&name, WriteMode::Truncate).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert_eq!(fs::read(t.outside.join("v.txt")).unwrap(), b"keep");
        // Removing the in-root name is harmless and allowed.
        w.delete(&name).unwrap();
        assert!(t.outside.join("v.txt").exists());
    }

    #[test]
    fn the_root_itself_is_not_a_target() {
        let t = tree("rootself");
        let w = roots(&t);
        assert_eq!(w.resolve_open(&s(&t.root)), Err(PsError::InvalidFileAccess));
        assert_eq!(w.delete(&s(&t.root)), Err(PsError::InvalidFileAccess));
        assert_eq!(w.resolve_open("/"), Err(PsError::InvalidFileAccess));
        assert!(t.root.is_dir());
    }

    #[test]
    fn missing_parent_is_undefinedfilename() {
        let t = tree("noparent");
        let w = roots(&t);
        assert_eq!(
            w.resolve_open(&s(&t.root.join("nope/a.txt"))),
            Err(PsError::UndefinedFilename)
        );
    }

    #[test]
    fn rename_needs_both_ends_inside() {
        let t = tree("rename");
        let w = roots(&t);
        fs::write(t.root.join("a.txt"), b"a").unwrap();
        fs::write(t.outside.join("o.txt"), b"o").unwrap();
        assert_eq!(
            w.rename(&s(&t.root.join("a.txt")), &s(&t.outside.join("b.txt"))),
            Err(PsError::InvalidFileAccess)
        );
        assert_eq!(
            w.rename(&s(&t.outside.join("o.txt")), &s(&t.root.join("o.txt"))),
            Err(PsError::InvalidFileAccess)
        );
        assert!(t.root.join("a.txt").exists() && t.outside.join("o.txt").exists());
        assert_eq!(
            w.rename(&s(&t.root.join("missing")), &s(&t.root.join("b"))),
            Err(PsError::UndefinedFilename)
        );
        w.rename(&s(&t.root.join("a.txt")), &s(&t.root.join("sub/b.txt")))
            .unwrap();
        assert_eq!(fs::read(t.root.join("sub/b.txt")).unwrap(), b"a");
    }

    #[test]
    fn delete_outside_and_missing() {
        let t = tree("delete");
        let w = roots(&t);
        fs::write(t.outside.join("o.txt"), b"o").unwrap();
        assert_eq!(
            w.delete(&s(&t.outside.join("o.txt"))),
            Err(PsError::InvalidFileAccess)
        );
        assert!(t.outside.join("o.txt").exists());
        assert_eq!(
            w.delete(&s(&t.root.join("missing"))),
            Err(PsError::UndefinedFilename)
        );
        assert_eq!(
            w.delete(&s(&t.root.join("sub"))),
            Err(PsError::InvalidFileAccess)
        );
    }

    #[test]
    fn deleting_a_symlink_removes_the_link_not_the_target() {
        let t = tree("dellink");
        fs::write(t.outside.join("v.txt"), b"keep").unwrap();
        symlink(t.outside.join("v.txt"), t.root.join("l")).unwrap();
        let w = roots(&t);
        w.delete(&s(&t.root.join("l"))).unwrap();
        assert!(fs::symlink_metadata(t.root.join("l")).is_err());
        assert_eq!(fs::read(t.outside.join("v.txt")).unwrap(), b"keep");
    }

    #[test]
    fn root_named_through_a_symlink_still_works() {
        let t = tree("linkedroot");
        let alias = t.root.with_file_name("alias");
        symlink(&t.root, &alias).unwrap();
        let w = WriteRoots::new(&[&alias]).unwrap();
        w.open(&s(&alias.join("a.txt")), WriteMode::Truncate)
            .unwrap();
        w.open(&s(&t.root.join("b.txt")), WriteMode::Truncate)
            .unwrap();
        assert!(t.root.join("a.txt").exists() && t.root.join("b.txt").exists());
    }

    #[test]
    fn trailing_slash_or_dot_is_refused() {
        let t = tree("trailing");
        symlink(t.outside.join("new"), t.root.join("dangle")).unwrap();
        let w = roots(&t);
        for name in ["d/", "dotnew/.", "dangle/", "dangle/."] {
            let name = format!("{}/{name}", t.root.display());
            assert_eq!(
                w.open(&name, WriteMode::Truncate).err(),
                Some(PsError::InvalidFileAccess),
                "{name}"
            );
        }
        assert!(!t.root.join("d").exists() && !t.root.join("dotnew").exists());
        assert!(!t.outside.join("new").exists());
    }

    #[test]
    fn fifo_and_directory_targets_are_refused() {
        let t = tree("fifo");
        let fifo = t.root.join("fifo");
        let made = std::process::Command::new("mkfifo").arg(&fifo).status();
        let w = roots(&t);
        if made.is_ok_and(|s| s.success()) {
            assert_eq!(
                w.open(&s(&fifo), WriteMode::Truncate).err(),
                Some(PsError::InvalidFileAccess)
            );
        }
        assert_eq!(
            w.open(&s(&t.root.join("sub")), WriteMode::Append).err(),
            Some(PsError::InvalidFileAccess)
        );
        assert_eq!(
            w.rename(&s(&t.root.join("sub")), &s(&t.root.join("sub2"))),
            Err(PsError::InvalidFileAccess)
        );
        assert!(t.root.join("sub").is_dir());
    }

    #[test]
    fn nul_and_empty_names() {
        let t = tree("nul");
        let w = roots(&t);
        assert_eq!(w.resolve_open(""), Err(PsError::UndefinedFilename));
        let name = format!("{}\0/x", t.root.display());
        assert_eq!(w.resolve_open(&name), Err(PsError::InvalidFileAccess));
    }

    #[test]
    fn a_root_must_be_an_existing_directory() {
        let t = tree("badroot");
        fs::write(t.root.join("f"), b"").unwrap();
        assert!(WriteRoots::new(&[t.root.join("f")]).is_err());
        assert!(WriteRoots::new(&[t.root.join("missing")]).is_err());
    }
}
