//! Named in-memory files a host serves to `run`, `(name) (r) file`,
//! `readreals` and `status` (#1407).
//!
//! A host that embeds the interpreter (WebAssembly, a sandboxed service, an
//! application with several library files of its own) registers sources with
//! [`Interp::add_virtual_file`](crate::Interp::add_virtual_file) and a
//! program loads them as it would a file on disk: `(components/foo.ps) run`.
//!
//! What a name is. A virtual file is matched by its whole name, exactly
//! (after one leading `./` is dropped from both sides): no search order, no
//! directory, no wildcard. It is looked at before anything else, so a
//! registered name shadows a disk file and the embedded `lib/` copy of the
//! same name, which is how a host overrides a library. A name that is not
//! registered is resolved as it always was (disk, the embedded `lib/`,
//! `undefinedfilename`, and under read roots `invalidfileaccess` outside
//! them), so registering files never changes what any other name does.
//!
//! What it is not. It is not a disk read, so read roots (#763) do not gate it
//! (the host chose to put it there, as it chose the roots). Only reading is
//! served: `(name) (w) file`, `deletefile` and `renamefile` still go to the
//! write policy and the disk, and `filenameforall` lists disk and embedded
//! names only.
//!
//! Bounds. The store is limited on every axis a host-supplied thing can grow
//! ([`MAX_VIRTUAL_FILE_BYTES`], [`MAX_VIRTUAL_FILES`],
//! [`MAX_VIRTUAL_TOTAL_BYTES`]); a registration past one is `limitcheck` and
//! changes nothing. Serving a file copies it into the program's file object,
//! so each read is charged as a read of the same size is (a step per KiB) and
//! checked against the memory cap before the copy.

use std::collections::HashMap;
use std::rc::Rc;

use crate::error::PsError;

/// The longest one virtual file may be: the same bound a `lib/` file read
/// from disk has (`paths::MAX_LIB_BYTES`).
pub const MAX_VIRTUAL_FILE_BYTES: usize = crate::paths::MAX_LIB_BYTES as usize;

/// The most virtual files one interpreter holds.
pub const MAX_VIRTUAL_FILES: usize = 4096;

/// The most bytes all of one interpreter's virtual files may hold together:
/// the bound on any one resource read from disk (`paths::MAX_RESOURCE_BYTES`).
pub const MAX_VIRTUAL_TOTAL_BYTES: usize = crate::paths::MAX_RESOURCE_BYTES as usize;

/// The registered files.
#[derive(Debug)]
pub(crate) struct VirtualFiles {
    files: HashMap<String, Rc<[u8]>>,
    total: usize,
    /// [`MAX_VIRTUAL_TOTAL_BYTES`], lowered by the unit tests so that the
    /// bound can be reached without holding half a gigabyte.
    max_total: usize,
}

impl Default for VirtualFiles {
    fn default() -> Self {
        VirtualFiles {
            files: HashMap::new(),
            total: 0,
            max_total: MAX_VIRTUAL_TOTAL_BYTES,
        }
    }
}

/// `name` as stored and looked up: one leading `./` dropped.
fn key(name: &str) -> &str {
    name.strip_prefix("./").unwrap_or(name)
}

/// Whether `name` may be registered. Refused names could never be read
/// back (`..` is `invalidfileaccess` under read roots before any lookup, and
/// a `%` name is a device, not a file), or would only confuse (empty, NUL).
fn valid(name: &str) -> Result<(), PsError> {
    let key = key(name);
    if key.is_empty() || key.contains('\0') {
        return Err(PsError::Rangecheck);
    }
    if key.starts_with('%') || key.split(['/', '\\']).any(|part| part == "..") {
        return Err(PsError::InvalidFileAccess);
    }
    Ok(())
}

impl VirtualFiles {
    /// The bytes registered under `name`, shared (no copy).
    pub(crate) fn get(&self, name: &str) -> Option<Rc<[u8]>> {
        if self.files.is_empty() {
            return None;
        }
        self.files.get(key(name)).cloned()
    }

    /// Register or replace `name`. Nothing changes on an error.
    pub(crate) fn add(&mut self, name: &str, bytes: Vec<u8>) -> Result<(), PsError> {
        valid(name)?;
        if bytes.len() > MAX_VIRTUAL_FILE_BYTES {
            return Err(PsError::Limitcheck);
        }
        let key = key(name);
        let old = self.files.get(key).map(|old| old.len());
        if old.is_none() && self.files.len() >= MAX_VIRTUAL_FILES {
            return Err(PsError::Limitcheck);
        }
        let replaced = old.unwrap_or(0);
        let total = self.total - replaced + bytes.len();
        if total > self.max_total {
            return Err(PsError::Limitcheck);
        }
        self.files.insert(key.to_owned(), Rc::from(bytes));
        self.total = total;
        Ok(())
    }

    /// Forget `name`; whether it was registered.
    pub(crate) fn remove(&mut self, name: &str) -> bool {
        match self.files.remove(key(name)) {
            Some(old) => {
                self.total -= old.len();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_total_is_bounded_and_a_replacement_counts_net() {
        let mut v = VirtualFiles::default();
        v.max_total = 10;
        v.add("a", vec![0; 6]).unwrap();
        v.add("b", vec![0; 4]).unwrap();
        assert_eq!(v.add("c", vec![0]), Err(PsError::Limitcheck));
        // Replacing `a` with something larger passes the total; with
        // something that fits it does not.
        assert_eq!(v.add("a", vec![0; 7]), Err(PsError::Limitcheck));
        v.add("a", vec![0; 5]).unwrap();
        assert_eq!(v.total, 9);
        assert!(v.remove("b"));
        assert!(!v.remove("b"));
        assert_eq!(v.total, 5);
        v.add("c", vec![0; 5]).unwrap();
    }

    #[test]
    fn the_count_is_bounded_but_a_replacement_is_not_a_new_file() {
        let mut v = VirtualFiles::default();
        for i in 0..MAX_VIRTUAL_FILES {
            v.add(&format!("f{i}"), Vec::new()).unwrap();
        }
        assert_eq!(v.add("one-more", Vec::new()), Err(PsError::Limitcheck));
        v.add("f0", vec![1]).unwrap();
        assert_eq!(v.get("./f0").as_deref(), Some(&[1u8][..]));
    }
}
