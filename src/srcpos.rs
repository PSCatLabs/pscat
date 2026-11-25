//! Where procedure bodies were written (#1405, #1338).
//!
//! The interpreter tags no object with a source position: an `Object` has no
//! room for one, and a position on every number and name would cost every
//! program. What an error report, a host and the lint want is narrower: the
//! source line (and file) of an element *of a procedure body that was scanned
//! from program text*. So the scanner keeps a side table, [`BodyLines`], with
//! one entry per such body, keyed by the address of the body's storage.
//!
//! ## Shape
//!
//! An entry holds the body's lines as runs: the first element's line, then
//! `(element, line)` for each element that starts a new line. Source is
//! mostly several elements to a line, so this is about a byte per element
//! (a body written on one line has no table at all), where a line per
//! element is four. A lookup is a binary search; it happens only when an
//! error is reported, so the table costs a running program nothing but what
//! it takes to build and hold it.
//!
//! The entry holds a `Weak` handle on the storage, which keeps the address
//! from being given to another array while the entry exists. A body that has
//! been freed leaves an entry that is swept (see [`MAX_HELD`], `sweep`) when
//! the table has doubled, so a program that reads a file again and again
//! does not grow it.
//!
//! ## What a position means
//!
//! - It is where the element's token was *written*. `bind`, `cvx`, `cvlit`
//!   and `getinterval` keep the storage, so they keep the lines. `copy`,
//!   `astore`, `array`, `aload` and every procedure made at run time have new
//!   storage and no lines. An element written into a scanned body with `put`
//!   keeps the line of the token it replaced.
//! - Only the main program and `run` files have lines
//!   ([`Lexer::line`](crate::lexer::Lexer)): an executable string, an `eexec`
//!   stream or a file object run directly does not, as in the error report.
//! - A `run` file's name is the name `run` was given. The main program has a
//!   name only when the host set one ([`Interp::set_main_file_name`]) while
//!   it was scanned, and then the entry outlives the program. An unnamed main
//!   program's bodies are forgotten when the next program begins
//!   ([`BodyLines::begin_program`]): its line 1 is not the next one's, so a
//!   line would be a wrong answer, and none is the right one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::{Rc, Weak};

use crate::object::{Object, PsArray};
use crate::{PsError, Value};

/// Most held entries and runs in one table (named or unnamed). A program
/// scans each procedure once, so this is reached only by a source file with
/// millions of procedure elements; past it a body gets no lines (an error
/// falls back to the line that ran it).
const MAX_HELD: usize = 1 << 22;

/// A table smaller than this is not swept: dead entries cost a few dozen
/// bytes each.
const SWEEP_FLOOR: usize = 1 << 14;

/// What one visited entry of a sweep is charged, in the bytes the decode
/// budget turns into steps (a step per KiB): a step per 16 entries.
const SWEEP_CHARGE: usize = 64;

/// The line of each element of a body being scanned, built as the scanner
/// goes: runs of equal lines, in element order.
#[derive(Default)]
pub(crate) struct Lines {
    first: u32,
    last: u32,
    /// Elements pushed so far.
    count: u32,
    /// `(element, line)` for each element whose line differs from the one
    /// before it.
    rest: Vec<(u32, u32)>,
}

impl Lines {
    /// The next element is on `line`.
    #[inline]
    pub(crate) fn push(&mut self, line: u32) {
        if self.count == 0 {
            self.first = line;
            self.last = line;
        } else if line != self.last {
            self.rest.push((self.count, line));
            self.last = line;
        }
        self.count = self.count.saturating_add(1);
    }

    /// Elements pushed.
    pub(crate) fn len(&self) -> usize {
        self.count as usize
    }
}

/// Where a body was scanned from.
#[derive(Clone, Default)]
pub(crate) struct Source {
    /// A `run` file's name, or the main program's when the host named it;
    /// `None` for an unnamed main program.
    pub(crate) name: Option<Rc<str>>,
    /// Whether `name` is a `run` file (`false` for the main program): what
    /// the lint calls the file.
    pub(crate) is_run: bool,
}

/// What only the lint needs of a body (#1338): where it is in the text and
/// how it ends. Kept only while `--lint` traces the run.
struct LintInfo {
    /// Byte offsets in the source of the body's `{` and of the end of its
    /// `}`, so the body that sits right before a token can be found.
    span: (usize, usize),
    /// How the body ends, kept because the body itself may be gone by the
    /// time its `def` runs (a branch of `{ ... } if` is freed as it finishes).
    tail: Tail,
}

/// How a procedure body ends (#1338).
#[derive(Clone, PartialEq, Eq)]
enum Tail {
    /// In `def` or `store`; the literal names the body spells before it.
    Def(Vec<u32>),
    /// In a call to this name.
    Call(u32),
    /// In anything else (a literal, or nothing).
    Other,
}

/// The addresses an entry is keyed by: a multiplicative hash, where the
/// default (SipHash) was a measurable share of scanning a procedure-heavy
/// file. The keys are the interpreter's own pointers, not program data, so
/// they need no protection against collisions.
#[derive(Default)]
struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u64(u64::from(b));
        }
    }
    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }
    fn write_u64(&mut self, n: u64) {
        let h = (n >> 3).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = h ^ (h >> 29);
    }
}

type Map = HashMap<usize, BodyEntry, BuildHasherDefault<AddrHasher>>;

/// `BodyEntry::source` of an entry with no name.
const NO_SOURCE: u32 = u32::MAX;

/// Most distinct source names held at once.
const MAX_SOURCES: usize = 256;

/// One body: a fixed-size record, so a table of them is a flat array. Its
/// runs of lines are in the table's `runs`, not behind a pointer of their
/// own, which saves an allocation per multi-line body.
struct BodyEntry {
    _storage: Weak<RefCell<Vec<Object>>>,
    first: u32,
    /// An index into [`BodyLines::sources`], or [`NO_SOURCE`].
    source: u32,
    /// Where the body's runs start in [`Table::runs`], and how many.
    runs_at: u32,
    runs_len: u32,
}

/// One table of entries, with the work counted that bounds it.
#[derive(Default)]
struct Table {
    entries: Map,
    /// The runs of every entry, `(element, line)` for each element whose
    /// line differs from the one before it, one entry's after another's.
    runs: Vec<(u32, u32)>,
    /// What only the lint needs, by the same key; empty unless `--lint`.
    lint: HashMap<usize, LintInfo, BuildHasherDefault<AddrHasher>>,
    /// Entries and runs held (at most [`MAX_HELD`], so every index fits a u32).
    held: usize,
    /// The size at which the next sweep runs.
    sweep_at: usize,
}

impl Table {
    fn runs_of(&self, e: &BodyEntry) -> &[(u32, u32)] {
        let at = e.runs_at as usize;
        self.runs
            .get(at..at + e.runs_len as usize)
            .unwrap_or_default()
    }

    /// The line of the element at `at` in the storage.
    fn line(&self, e: &BodyEntry, at: usize) -> u32 {
        let runs = self.runs_of(e);
        let k = runs.partition_point(|&(start, _)| (start as usize) <= at);
        match k.checked_sub(1) {
            Some(k) => runs[k].1,
            None => e.first,
        }
    }

    /// The line of the last element of the body.
    fn last_line(&self, e: &BodyEntry) -> u32 {
        self.runs_of(e).last().map_or(e.first, |r| r.1)
    }

    /// Drop the entries whose body is gone, and the runs with them; returns
    /// how many entries it looked at.
    fn sweep(&mut self) -> usize {
        let visited = self.entries.len();
        self.entries.retain(|_, e| e._storage.strong_count() > 0);
        let mut runs = Vec::with_capacity(self.held.saturating_sub(self.entries.len()));
        for e in self.entries.values_mut() {
            let at = e.runs_at as usize;
            let kept = self
                .runs
                .get(at..at + e.runs_len as usize)
                .unwrap_or_default();
            e.runs_at = runs.len() as u32;
            runs.extend_from_slice(kept);
        }
        runs.shrink_to_fit();
        self.runs = runs;
        let entries = &self.entries;
        self.lint.retain(|k, _| entries.contains_key(k));
        self.held = self.entries.len() + self.runs.len();
        self.sweep_at = (self.entries.len() * 4).max(SWEEP_FLOOR);
        visited
    }
}

/// The side table (module docs). `named` holds bodies whose source has a
/// name, which outlive the program that scanned them; `anon` the main
/// program's, unnamed, which end with it.
#[derive(Default)]
pub(crate) struct BodyLines {
    named: Table,
    anon: Table,
    /// The names `named`'s entries refer to, by index: a handful of files
    /// shared by every body in them.
    sources: Vec<Source>,
}

/// The line and file of a body's element: `file` as the host reports it
/// (see [`Source::name`]).
pub(crate) struct Position {
    pub(crate) line: u32,
    pub(crate) file: Option<Rc<str>>,
}

impl BodyLines {
    /// Keep `lines` (one per element) for the freshly scanned `body`.
    ///
    /// `span` is given only for the lint, which keeps what it needs besides
    /// the lines. `check_alloc` is asked before the table takes memory it
    /// cannot do without (the hash map doubling is the one large step), so a
    /// memory cap stops the scan instead of being passed. Returns the work
    /// done beyond the scan, in the bytes the decode budget counts, for the
    /// caller to charge.
    pub(crate) fn register(
        &mut self,
        body: &PsArray,
        lines: Lines,
        name: Option<&Rc<str>>,
        is_run: bool,
        span: Option<(usize, usize)>,
        check_alloc: &dyn Fn(usize) -> Result<(), PsError>,
    ) -> Result<usize, PsError> {
        // An empty body has no element to ask about, except to the lint,
        // which counts the bodies written before a token.
        if lines.len() != body.len() || (span.is_none() && body.is_empty()) {
            return Ok(0);
        }
        let mut work = 0;
        let named = name.is_some();
        let table = if named {
            &mut self.named
        } else {
            &mut self.anon
        };
        if table.entries.len() >= table.sweep_at.max(SWEEP_FLOOR) {
            work = table.sweep().saturating_mul(SWEEP_CHARGE);
            if named {
                self.collect_sources();
            }
        }
        let source_id = if named {
            match self.source_id(name, is_run) {
                Some(id) => id,
                None => return Ok(work),
            }
        } else {
            NO_SOURCE
        };
        let table = if named {
            &mut self.named
        } else {
            &mut self.anon
        };
        let cost = 1 + lines.rest.len();
        if table.held + cost > MAX_HELD {
            return Ok(work);
        }
        // The table's two growing buffers double, and each doubling is asked
        // of the cap first; the lint's records are too few to ask about.
        let entry_bytes = std::mem::size_of::<(usize, BodyEntry)>() + 1;
        let mut grows = 0;
        if table.entries.len() == table.entries.capacity() {
            grows += (table.entries.capacity() * 2).max(4) * entry_bytes;
        }
        if table.runs.len() + lines.rest.len() > table.runs.capacity() {
            grows += table.runs.capacity() * std::mem::size_of::<(u32, u32)>();
        }
        if grows > 0 {
            check_alloc(grows)?;
        }
        table.held += cost;
        let key = body.storage_site(0).0;
        let runs_at = table.runs.len() as u32;
        let runs_len = lines.rest.len() as u32;
        table.runs.extend_from_slice(&lines.rest);
        if let Some(span) = span {
            table.lint.insert(
                key,
                LintInfo {
                    span,
                    tail: body_tail(body),
                },
            );
        }
        table.entries.insert(
            key,
            BodyEntry {
                _storage: body.storage_weak(),
                first: lines.first,
                source: source_id,
                runs_at,
                runs_len,
            },
        );
        Ok(work)
    }

    /// The index of `source` in the table of names, adding it; `None` when
    /// the names held are already as many as [`MAX_SOURCES`].
    fn source_id(&mut self, name: Option<&Rc<str>>, is_run: bool) -> Option<u32> {
        // Every body of a file carries the very same name, so the last one
        // asked for is nearly always the one asked for again.
        let same = |s: &Source| {
            s.is_run == is_run
                && match (&s.name, name) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b) || a == b,
                    _ => false,
                }
        };
        if let Some(i) = self.sources.iter().rposition(same) {
            return u32::try_from(i).ok();
        }
        if self.sources.len() >= MAX_SOURCES {
            return None;
        }
        self.sources.push(Source {
            name: name.cloned(),
            is_run,
        });
        u32::try_from(self.sources.len() - 1).ok()
    }

    /// Forget the names no entry of `named` refers to, renumbering the rest.
    fn collect_sources(&mut self) {
        let mut used = vec![false; self.sources.len()];
        for e in self.named.entries.values() {
            if let Some(u) = used.get_mut(e.source as usize) {
                *u = true;
            }
        }
        if used.iter().all(|&u| u) {
            return;
        }
        let mut renumber = vec![NO_SOURCE; used.len()];
        let mut kept = Vec::new();
        for (i, source) in self.sources.drain(..).enumerate() {
            if used[i] {
                renumber[i] = kept.len() as u32;
                kept.push(source);
            }
        }
        self.sources = kept;
        for e in self.named.entries.values_mut() {
            e.source = renumber
                .get(e.source as usize)
                .copied()
                .unwrap_or(NO_SOURCE);
        }
    }

    /// A program begins: forget the bodies of the last one that had no
    /// name, whose lines start again from 1. `forget_all` also forgets the
    /// named ones: what the lint wants, which matches positions in one
    /// text.
    pub(crate) fn begin_program(&mut self, forget_all: bool) {
        // Replaced, not cleared: a map that once held a large program keeps
        // its capacity, which `clear` would memset on every program.
        if !self.anon.entries.is_empty() {
            self.anon = Table::default();
        }
        if forget_all && !self.named.entries.is_empty() {
            self.named = Table::default();
            self.sources.clear();
        }
    }

    /// The table that holds `body`'s entry, the entry, and the index of
    /// element `i` in the storage.
    fn entry(&self, body: &PsArray, i: usize) -> Option<(&Table, &BodyEntry, usize)> {
        let (key, at) = body.storage_site(i);
        [&self.anon, &self.named]
            .into_iter()
            .find_map(|t| t.entries.get(&key).map(|e| (t, e, at)))
    }

    fn source_of(&self, e: &BodyEntry) -> Option<&Source> {
        self.sources.get(e.source as usize)
    }

    /// The file as the lint knows it: a `run` file's name.
    fn lint_file(&self, e: &BodyEntry) -> Option<&Rc<str>> {
        self.source_of(e)
            .filter(|s| s.is_run)
            .and_then(|s| s.name.as_ref())
    }

    /// Where element `i` of `body` was written, if `body` was scanned from
    /// text that has lines.
    pub(crate) fn position(&self, body: &PsArray, i: usize) -> Option<Position> {
        let (t, e, at) = self.entry(body, i)?;
        Some(Position {
            line: t.line(e, at),
            file: self.source_of(e).and_then(|s| s.name.clone()),
        })
    }

    /// The line (and `run` file) element `i` of `body` was written on, as
    /// the lint reports it.
    pub(crate) fn line_of(&self, body: &PsArray, i: usize) -> Option<(u32, Option<Rc<str>>)> {
        let (t, e, at) = self.entry(body, i)?;
        Some((t.line(e, at), self.lint_file(e).cloned()))
    }

    /// The line of the `def` of `name_id` that ends the literal body (or, for
    /// `ifelse`, the one of the `count` bodies) written right before the token
    /// the scanner is running at `token_at`: `{ ... } if` in the program text,
    /// whose branch no frame holds any more (#1338). The bodies are found by
    /// position, `blank(from, to)` telling that only white space and comments
    /// lie between them and the token, so the bodies are the operands the
    /// operator takes and not some other body spelling the same name.
    ///
    /// Answers only when exactly one of them ends in a `def` of the name and
    /// each of the others cannot have run one: it ends in a literal, or in a
    /// `def` of other names, or in a call that `inert` says is an operator
    /// that runs nothing. Otherwise `None`, and the caller keeps the line that
    /// ran it.
    pub(crate) fn before_token(
        &self,
        file: Option<&Rc<str>>,
        token_at: usize,
        count: usize,
        name_id: u32,
        blank: &dyn Fn(usize, usize) -> bool,
        inert: &dyn Fn(u32) -> bool,
    ) -> Option<(u32, Option<Rc<str>>)> {
        let mut found: Option<(u32, Option<Rc<str>>)> = None;
        let mut before = token_at;
        // Only the lint's entries have a span.
        let traced = || {
            [&self.anon, &self.named].into_iter().flat_map(|t| {
                t.lint
                    .iter()
                    .filter_map(move |(k, l)| t.entries.get(k).map(|e| (t, e, l)))
            })
        };
        for _ in 0..count {
            // The body whose `}` ends just before `before`. Two entries can
            // share an end only when one file was scanned twice: the same body.
            let mut same = traced().filter(|(_, e, l)| {
                self.lint_file(e) == file && l.span.1 <= before && blank(l.span.1, before)
            });
            let (table, entry, info) = same.next()?;
            if same.any(|(t, e, l)| {
                l.span != info.span
                    || l.tail != info.tail
                    || e.first != entry.first
                    || t.runs_of(e) != table.runs_of(entry)
            }) {
                return None;
            }
            match &info.tail {
                Tail::Def(names) if names.contains(&name_id) => {
                    // Two bodies that both define it: not a guess to make.
                    if found.is_some() {
                        return None;
                    }
                    found = Some((table.last_line(entry), self.lint_file(entry).cloned()));
                }
                Tail::Def(_) | Tail::Other => {}
                Tail::Call(id) if inert(*id) => {}
                Tail::Call(_) => return None,
            }
            before = info.span.0;
        }
        found
    }

    /// Entries held (tests).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.named.entries.len() + self.anon.entries.len()
    }

    /// Names held (tests).
    #[cfg(test)]
    pub(crate) fn source_count(&self) -> usize {
        self.sources.len()
    }
}

/// How `body` ends, for the lint.
fn body_tail(body: &PsArray) -> Tail {
    body.with_slice(|els| match els.split_last() {
        Some((last, before)) if crate::lint::is_binding_op(last) => Tail::Def(
            before
                .iter()
                .filter_map(|o| match &o.value {
                    Value::Name(n) if !o.executable => Some(n.id()),
                    _ => None,
                })
                .collect(),
        ),
        Some((last, _)) => match &last.value {
            Value::Name(n) if last.executable => Tail::Call(n.id()),
            _ => Tail::Other,
        },
        None => Tail::Other,
    })
}
