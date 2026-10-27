//! Interned names — Stage 8 task 2.
//!
//! PostScript programs execute names constantly (every operator call
//! is a name lookup), and hashing the *text* of `Rc<str>` keys on
//! every dictionary probe was the measured bottleneck (`benches/`:
//! fib 27 at ~7× gs before this module). A [`PsName`] is the text
//! plus a process-wide (per-thread) integer id assigned on first
//! sight; dictionaries key their name entries by id, so a lookup
//! hashes one `u32` instead of a string.
//!
//! The interner is thread-local: the interpreter is single-threaded
//! by design (`Rc` composites make `Interp: !Send`), and multiple
//! `Interp`s on one thread share ids harmlessly. Names live for the
//! life of the thread, matching PostScript's own name semantics —
//! the table only grows, and that's correct.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::rc::Rc;

/// An interned name: id for identity/hashing, text for display and
/// the many call sites that match on `&*name`.
#[derive(Clone)]
pub struct PsName {
    id: u32,
    text: Rc<str>,
}

impl PsName {
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The shared text handle (cheap clone).
    pub fn text_rc(&self) -> Rc<str> {
        self.text.clone()
    }
}

impl Deref for PsName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.text
    }
}

impl PartialEq for PsName {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for PsName {}

impl Hash for PsName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Display for PsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

impl fmt::Debug for PsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", self.text)
    }
}

impl From<&str> for PsName {
    fn from(s: &str) -> Self {
        intern(s)
    }
}

#[derive(Default)]
struct Interner {
    by_text: HashMap<Rc<str>, u32>,
    by_id: Vec<Rc<str>>,
    /// Names made inside [`detached`], by `id - DETACHED_BASE`, with
    /// whether the program has interned them yet (`true` = not yet).
    detached: Vec<(Rc<str>, bool)>,
    /// Inside [`detached`]: new names take detached ids.
    detaching: bool,
}

/// Detached ids start here, far above any sequential id and far below the
/// `u32::MAX` sentinels other modules use.
const DETACHED_BASE: u32 = 1 << 31;
/// A preamble defines a handful of names; more is a bug.
const DETACHED_CAP: usize = 1 << 16;

thread_local! {
    static INTERNER: RefCell<Interner> = RefCell::new(Interner::default());
}

impl Interner {
    /// The name for an id `by_text` already holds. A detached name the
    /// program hasn't interned yet takes a sequential slot now, the one a
    /// run without the preamble would give it here, so every later name
    /// gets the id it would get in that run.
    fn existing(&mut self, id: u32) -> PsName {
        if id < DETACHED_BASE {
            return PsName {
                id,
                text: self.by_id[id as usize].clone(),
            };
        }
        let (text, pending) = &mut self.detached[(id - DETACHED_BASE) as usize];
        let text = text.clone();
        // Inside `detached` a repeat mention is still the preamble's.
        if !self.detaching && std::mem::take(pending) {
            self.by_id.push(text.clone());
        }
        PsName { id, text }
    }

    fn insert(&mut self, text: Rc<str>) -> PsName {
        let id = if self.detaching {
            assert!(
                self.detached.len() < DETACHED_CAP,
                "too many detached names"
            );
            self.detached.push((text.clone(), true));
            DETACHED_BASE + (self.detached.len() - 1) as u32
        } else {
            self.by_id.push(text.clone());
            (self.by_id.len() - 1) as u32
        };
        self.by_text.insert(text.clone(), id);
        PsName { id, text }
    }
}

/// Run `f` with name interning detached from the program's id sequence
/// (#868). A name first interned inside `f` gets an id from a separate
/// range and takes no sequential id, until the program itself first
/// interns it (scanner, `cvn`, ...): then it takes the sequential slot a
/// run without `f` would have given it there, keeping its detached id.
/// So the program's own names get the same ids, and `forall` over their
/// dictionaries the same order, as in a run without `f`. Names already
/// interned are unaffected. `--bands` defines the band names this way.
pub fn detached<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            INTERNER.with(|i| i.borrow_mut().detaching = self.0);
        }
    }
    let _restore =
        Restore(INTERNER.with(|i| std::mem::replace(&mut i.borrow_mut().detaching, true)));
    f()
}

pub fn intern(s: &str) -> PsName {
    INTERNER.with(|i| {
        let mut i = i.borrow_mut();
        if let Some(&id) = i.by_text.get(s) {
            return i.existing(id);
        }
        i.insert(s.into())
    })
}

/// Intern reusing an existing allocation.
pub fn intern_rc(s: Rc<str>) -> PsName {
    INTERNER.with(|i| {
        let mut i = i.borrow_mut();
        if let Some(&id) = i.by_text.get(&*s) {
            return i.existing(id);
        }
        i.insert(s)
    })
}

/// Id lookup **without** growing the table — the read-only probes
/// (`Dict::get(&str)`) must not accumulate garbage names.
pub fn lookup(s: &str) -> Option<u32> {
    INTERNER.with(|i| i.borrow().by_text.get(s).copied())
}

/// Resolve an id back to a full name (for `Dict::pairs`).
/// Never takes a detached name's sequential slot: resolving is not the
/// program interning a name.
pub fn resolve(id: u32) -> PsName {
    INTERNER.with(|i| {
        let i = i.borrow();
        let text = if id < DETACHED_BASE {
            i.by_id[id as usize].clone()
        } else {
            i.detached[(id - DETACHED_BASE) as usize].0.clone()
        };
        PsName { id, text }
    })
}

/// `BuildHasher` for id-keyed maps: one multiply instead of SipHash.
/// (Ids are small and sequential; the golden-ratio multiply spreads
/// them across the table.)
#[derive(Default, Clone)]
pub struct IdHashBuilder;

pub struct IdHasher(u64);

impl std::hash::BuildHasher for IdHashBuilder {
    type Hasher = IdHasher;
    fn build_hasher(&self) -> IdHasher {
        IdHasher(0)
    }
}

impl Hasher for IdHasher {
    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("IdHasher is for u32 name ids only");
    }
    fn write_u32(&mut self, n: u32) {
        self.0 = u64::from(n).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_id() {
        let a = intern("moveto");
        let b = intern("moveto");
        assert_eq!(a, b);
        assert_eq!(a.id(), b.id());
        assert!(Rc::ptr_eq(&a.text_rc(), &b.text_rc()), "one allocation");
    }

    #[test]
    fn lookup_does_not_grow() {
        assert_eq!(lookup("never-interned-xyzzy"), None);
        assert_eq!(lookup("never-interned-xyzzy"), None);
    }

    /// The interner is per thread; a fresh thread gives each test a fresh one.
    fn on_fresh_thread(f: impl FnOnce() + Send + 'static) {
        std::thread::spawn(f).join().expect("test thread");
    }

    #[test]
    fn detached_names_take_no_sequential_ids() {
        on_fresh_thread(|| {
            let base = intern("base-868").id();
            let band = detached(|| intern("BandIndex"));
            assert!(band.id() >= DETACHED_BASE);
            assert_eq!(&*resolve(band.id()), "BandIndex");
            assert_eq!(lookup("BandIndex"), Some(band.id()));
            // The preamble took no slot: the next name is base + 1.
            assert_eq!(intern("a-868").id(), base + 1);
            // The program's first mention takes the slot a plain run would
            // (base + 2), keeping its detached id; later mentions take none.
            assert_eq!(intern("BandIndex"), band);
            assert_eq!(intern("BandIndex"), band);
            assert_eq!(intern_rc("BandIndex".into()), band);
            assert_eq!(intern("c-868").id(), base + 3);
        });
    }

    #[test]
    fn a_repeat_mention_inside_detached_takes_no_slot() {
        on_fresh_thread(|| {
            let base = intern("base-868").id();
            detached(|| {
                intern("foo-868");
                intern("foo-868");
            });
            assert_eq!(intern("next-868").id(), base + 1);
        });
    }

    /// The id the next new name gets after `program` runs in a fresh
    /// interpreter on a fresh thread, with or without the band preamble.
    fn next_id_after(program: &'static str, preamble: bool) -> u32 {
        std::thread::spawn(move || {
            let mut interp = crate::Interp::new();
            if preamble {
                let band = b"/BandIndex 0 def /BandCount 1 def /BandBBox [0 0 612 792] def";
                detached(|| interp.run_source(band)).expect("preamble");
            }
            interp.run_source(program.as_bytes()).expect("program");
            intern("probe-868").id()
        })
        .join()
        .expect("test thread")
    }

    /// #868: a program's names get the same ids with the band preamble as
    /// without, whether it mentions a band name partway through or never.
    #[test]
    fn the_band_preamble_leaves_the_programs_ids_alone() {
        for program in [
            "/early 1 def /BandIndex where { pop } if /later 2 def (made) cvn pop",
            "/early 1 def /BandCount pop /BandBBox pop /BandIndex pop /later 2 def",
            "/early 1 def /later 2 def",
        ] {
            assert_eq!(
                next_id_after(program, true),
                next_id_after(program, false),
                "{program}"
            );
        }
    }

    #[test]
    fn detached_mode_ends_on_error_and_leaves_old_names_alone() {
        on_fresh_thread(|| {
            let old = intern("old-868");
            let r: Result<(), ()> = detached(|| {
                assert_eq!(intern("old-868"), old, "already interned: unchanged");
                Err(())
            });
            assert!(r.is_err());
            assert!(intern("new-868").id() < DETACHED_BASE);
        });
    }

    #[test]
    fn deref_matches_text() {
        let n = intern("show");
        assert_eq!(&*n, "show");
        assert_eq!(resolve(n.id()).text_rc(), n.text_rc());
    }
}
