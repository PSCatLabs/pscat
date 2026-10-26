//! Where a program may read files from (issue #763).
//!
//! Reading is open by default: an [`Interp`] starts with no read roots, so
//! `(path) (r) file`, `run`, `readreals` and `filenameforall` read and list
//! wherever the search order (`src/paths.rs`) finds a file, as they always
//! have. The CLI keeps that. An embedder that runs untrusted programs
//! confines them with [`Interp::set_read_roots`] (the CLI's `--read-root DIR`
//! and `--confine-reads`); `pscat-mcp` always does, with its working
//! directory, the pscat data directories and its `--allow-read` flags.
//!
//! Under read roots:
//! - a `..` component anywhere in a name or a `filenameforall` template is
//!   `invalidfileaccess`, before any lookup (as for the write roots);
//! - every disk candidate the search order tries is canonicalised (a symlink
//!   is followed to where it really points) and read only if it lands inside
//!   a canonicalised root; the canonical path is what is read. A candidate
//!   outside is skipped, like a missing one, so the next place (the embedded
//!   `lib/` copy, which is not a disk read) can still answer;
//! - an absolute name outside the roots is `invalidfileaccess` whether or
//!   not the file exists, so the error never tells a program what exists
//!   there; a relative name is `invalidfileaccess` when a candidate inside a
//!   root leads out (a symlink), and a search-order place outside every root
//!   is skipped as if it were empty (`undefinedfilename` if nothing else
//!   answers), for the same reason;
//! - `filenameforall` lists only directories inside the roots and yields
//!   only names that would open, so an absolute template outside them
//!   yields nothing (Ghostscript's `-dSAFER` skips files it may not open the
//!   same way; the PLRM lists no `invalidfileaccess` for `filenameforall`).
//!
//! Special files inside a root (a FIFO in the working directory) are the
//! user's own and read as they would without roots, under the execution
//! limits (issue #702); `/dev/zero` and friends are outside every root
//! unless someone names `/dev`. The write side's hard-link refusal does not
//! apply: a program cannot make links, and reading through one somebody else
//! made inside a root is that person's choice.
//!
//! Font lookup (`findfont`) is not a program-chosen path: it matches the
//! requested name against file stems in the font directories and never
//! joins a program's string into a path, so it is not checked here.
//!
//! [`Interp`]: crate::Interp
//! [`Interp::set_read_roots`]: crate::Interp::set_read_roots

use std::io;
use std::path::{Path, PathBuf};

use crate::error::PsError;
use crate::write_roots::{canonical_dirs, has_parent_dir};

/// The directories a program may read from, canonicalised. Present but
/// empty means nothing on disk at all (only the embedded `lib/`).
#[derive(Debug, Default, Clone)]
pub struct ReadRoots {
    roots: Vec<PathBuf>,
}

/// What [`ReadRoots::admit`] decided about one disk candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admit {
    /// Read this canonical path: it lies inside a root.
    Open(PathBuf),
    /// Nothing there (or a symlink pointing nowhere). `inside` says
    /// whether the place it would be is inside the roots.
    Missing { inside: bool },
    /// It exists, but resolves outside every root.
    Refused,
}

impl ReadRoots {
    /// Canonicalise each directory. Fails, naming the first bad one, if a
    /// directory doesn't exist or isn't a directory.
    pub fn new<P: AsRef<Path>>(dirs: &[P]) -> Result<ReadRoots, String> {
        Ok(ReadRoots {
            roots: canonical_dirs(dirs, "read root")?,
        })
    }

    /// The canonical roots.
    pub fn dirs(&self) -> &[PathBuf] {
        &self.roots
    }

    /// The checks on the name itself, before any lookup: no `..`, no NUL.
    pub fn check_name(&self, name: &str) -> Result<(), PsError> {
        if has_parent_dir(name) || name.contains('\0') {
            return Err(PsError::InvalidFileAccess);
        }
        Ok(())
    }

    /// Whether a canonical path is a root or inside one. `starts_with`
    /// compares whole components, so `/in` does not contain `/inside`.
    pub fn contains(&self, canon: &Path) -> bool {
        self.roots.iter().any(|r| canon.starts_with(r))
    }

    /// Whether `path`, canonicalised, is inside the roots (false when it
    /// can't be canonicalised).
    pub fn contains_path(&self, path: &Path) -> bool {
        std::fs::canonicalize(path).is_ok_and(|c| self.contains(&c))
    }

    /// Decide one disk candidate (module doc).
    pub fn admit(&self, candidate: &Path) -> Admit {
        match std::fs::canonicalize(candidate) {
            Ok(canon) if self.contains(&canon) => Admit::Open(canon),
            Ok(_) => Admit::Refused,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Admit::Missing {
                inside: self.nearest_ancestor_inside(candidate),
            },
            Err(_) => Admit::Refused,
        }
    }

    /// Whether the deepest existing ancestor of a missing `path` is inside
    /// the roots: where the file would be if it existed.
    fn nearest_ancestor_inside(&self, path: &Path) -> bool {
        let abs = if path.is_absolute() {
            path.to_path_buf()
        } else {
            match std::env::current_dir() {
                Ok(cwd) => cwd.join(path),
                Err(_) => return false,
            }
        };
        abs.ancestors()
            .skip(1)
            .find_map(|a| std::fs::canonicalize(a).ok())
            .is_some_and(|c| self.contains(&c))
    }
}

/// Resolve and read `name` through the search order
/// (`crate::paths::open_with_reader`), reading disk candidates through
/// `read`, under `roots` when there are any (module doc). `Ok(None)` is
/// "not found" (`undefinedfilename`); a name that only resolved outside
/// the roots is `invalidfileaccess`.
pub(crate) fn open_confined(
    roots: Option<&ReadRoots>,
    name: &str,
    read: &dyn Fn(&Path) -> Option<Vec<u8>>,
) -> Result<Option<crate::paths::Resource>, PsError> {
    let Some(roots) = roots else {
        return Ok(crate::paths::open_with_reader(name, read));
    };
    roots.check_name(name)?;
    let absolute = Path::new(name).is_absolute();
    let refused = std::cell::Cell::new(false);
    let gated = |p: &Path| match roots.admit(p) {
        Admit::Open(canon) => read(&canon),
        Admit::Refused => {
            // A search-order place outside the roots altogether (the
            // executable's directory, or the working directory when that
            // isn't a root) is skipped as if empty, so a relative name
            // can't probe what exists there; a link inside a root that
            // leads out is a refusal.
            let parent = match p.parent() {
                Some(d) if d.as_os_str().is_empty() => Path::new("."),
                Some(d) => d,
                None => Path::new("/"),
            };
            if absolute || roots.contains_path(parent) {
                refused.set(true);
            }
            None
        }
        // A missing file outside is refused only when the program named
        // it outright: a search-order place that doesn't have the file
        // just isn't the place.
        Admit::Missing { inside } => {
            if absolute && !inside {
                refused.set(true);
            }
            None
        }
    };
    let found = crate::paths::open_with_reader(name, &gated);
    if found.is_none() && refused.get() {
        return Err(PsError::InvalidFileAccess);
    }
    Ok(found)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    struct Tree {
        base: PathBuf,
        root: PathBuf,
        outside: PathBuf,
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn tree(tag: &str) -> Tree {
        let base =
            std::env::temp_dir().join(format!("pscat-read-roots-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("root/sub")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        fs::write(base.join("root/sub/in.txt"), b"in").unwrap();
        fs::write(base.join("outside/secret.txt"), b"secret").unwrap();
        Tree {
            root: base.join("root"),
            outside: base.join("outside"),
            base,
        }
    }

    fn roots(t: &Tree) -> ReadRoots {
        ReadRoots::new(&[&t.root]).unwrap()
    }

    #[test]
    fn inside_opens_at_its_canonical_path() {
        let t = tree("inside");
        let r = roots(&t);
        match r.admit(&t.root.join("sub/in.txt")) {
            Admit::Open(p) => assert_eq!(fs::read(p).unwrap(), b"in"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn outside_is_refused_and_missing_outside_says_so() {
        let t = tree("outside");
        let r = roots(&t);
        assert_eq!(r.admit(&t.outside.join("secret.txt")), Admit::Refused);
        assert_eq!(
            r.admit(&t.outside.join("nope.txt")),
            Admit::Missing { inside: false }
        );
        assert_eq!(
            r.admit(&t.root.join("nope.txt")),
            Admit::Missing { inside: true }
        );
        assert_eq!(
            r.admit(&t.root.join("no/such/dir/x")),
            Admit::Missing { inside: true }
        );
    }

    #[test]
    fn dotdot_names_are_refused_before_lookup() {
        let t = tree("dotdot");
        let r = roots(&t);
        for name in ["../x", "sub/../in.txt", "/a/../b"] {
            assert_eq!(
                r.check_name(name),
                Err(PsError::InvalidFileAccess),
                "{name}"
            );
        }
        assert_eq!(r.check_name("a\0b"), Err(PsError::InvalidFileAccess));
        assert_eq!(r.check_name("sub/in.txt"), Ok(()));
    }

    #[test]
    fn symlinks_out_are_refused_and_in_are_followed() {
        let t = tree("links");
        symlink(t.outside.join("secret.txt"), t.root.join("leak.txt")).unwrap();
        symlink(&t.outside, t.root.join("door")).unwrap();
        symlink(t.root.join("sub/in.txt"), t.root.join("alias.txt")).unwrap();
        symlink("/dev/zero", t.root.join("zero")).unwrap();
        let r = roots(&t);
        assert_eq!(r.admit(&t.root.join("leak.txt")), Admit::Refused);
        assert_eq!(r.admit(&t.root.join("door/secret.txt")), Admit::Refused);
        assert_eq!(r.admit(&t.root.join("zero")), Admit::Refused);
        assert!(matches!(r.admit(&t.root.join("alias.txt")), Admit::Open(_)));
    }

    #[test]
    fn device_files_and_sibling_prefixes_are_outside() {
        let t = tree("dev");
        let sibling = t.root.with_file_name("rootx");
        fs::create_dir_all(&sibling).unwrap();
        fs::write(sibling.join("f"), b"").unwrap();
        let r = roots(&t);
        assert_eq!(r.admit(Path::new("/dev/zero")), Admit::Refused);
        assert_eq!(r.admit(&sibling.join("f")), Admit::Refused);
    }

    #[test]
    fn empty_roots_admit_nothing() {
        let t = tree("empty");
        let r = ReadRoots::default();
        assert_eq!(r.admit(&t.root.join("sub/in.txt")), Admit::Refused);
    }

    #[test]
    fn a_root_named_through_a_symlink_works() {
        let t = tree("linkedroot");
        let alias = t.base.join("alias");
        symlink(&t.root, &alias).unwrap();
        let r = ReadRoots::new(&[&alias]).unwrap();
        assert!(matches!(r.admit(&alias.join("sub/in.txt")), Admit::Open(_)));
        assert!(matches!(
            r.admit(&t.root.join("sub/in.txt")),
            Admit::Open(_)
        ));
        assert!(ReadRoots::new(&[t.base.join("missing")]).is_err());
    }
}
