//! `pscat --profile` (issue #737): per-procedure call counts and time.
//!
//! A procedure is attributed to the name it was *executed through*: when the
//! exec loop resolves an executable name to a procedure body and profiling is
//! on, it plants a [`Frame::ProfReturn`](crate::interp) marker under the body's
//! frame. The marker is reached only after the body, and whatever its last
//! element tail-called, has finished (a `Proc` frame is dropped before its
//! last element runs, so the frame itself can't mark the return). Unwinding
//! (`stop`, a caught or uncaught error, `exit`, `quit`) pops the marker like
//! any other frame and records the exit there.
//!
//! A procedure run any other way (`exec`, `if`/`ifelse` branches, a `forall`
//! body, a Type 3 glyph) has no name of its own: its time is part of the
//! procedure that ran it.
//!
//! Off by default. Then a procedure call pays one test of `Interp::profile`
//! (in `execute_resolved`, only where a body is about to run), and nothing
//! else changes: the profiled work is in `#[cold]` functions the plain run
//! never reaches.

use std::collections::HashMap;
use std::time::Duration;

/// One name's running totals.
#[derive(Default)]
struct Stat {
    calls: u64,
    /// Frames of this name now open; inclusive time is added when the
    /// outermost one closes, so recursion is not counted twice.
    depth: u32,
    incl_ns: u64,
    excl_ns: u64,
}

/// A call in progress (one per marker frame on the exec stack, same order).
struct Active {
    id: u32,
    start: u64,
    /// Inclusive time of the calls this one made, to subtract for exclusive.
    child_ns: u64,
}

#[derive(Default)]
pub(crate) struct Profiler {
    stats: HashMap<u32, Stat>,
    active: Vec<Active>,
}

impl Profiler {
    pub(crate) fn new() -> Self {
        // Start the clock's zero point before the first reading.
        crate::clock::Clock::mono_ns();
        Self::default()
    }

    /// Does one of the top `k` open calls belong to `id`? Those are the
    /// procedures currently in tail position (see `Interp::prof_call`).
    pub(crate) fn tail_has(&self, k: usize, id: u32) -> bool {
        self.active[self.active.len().saturating_sub(k)..].iter().any(|a| a.id == id)
    }

    /// Count a call that reuses an open marker (a tail call).
    pub(crate) fn count_only(&mut self, id: u32) {
        self.stats.entry(id).or_default().calls += 1;
    }

    pub(crate) fn enter(&mut self, id: u32) {
        let s = self.stats.entry(id).or_default();
        s.calls += 1;
        s.depth += 1;
        self.active.push(Active {
            id,
            start: crate::clock::Clock::mono_ns(),
            child_ns: 0,
        });
    }

    /// Take back an `enter` whose frame could not be pushed.
    pub(crate) fn abort_enter(&mut self) {
        if let Some(a) = self.active.pop()
            && let Some(s) = self.stats.get_mut(&a.id)
        {
            s.depth -= 1;
        }
    }

    /// Close the innermost open call (its marker frame was popped, reached
    /// or unwound).
    pub(crate) fn exit(&mut self) {
        let Some(a) = self.active.pop() else { return };
        let dur = crate::clock::Clock::mono_ns().saturating_sub(a.start);
        let s = self.stats.entry(a.id).or_default();
        s.excl_ns += dur.saturating_sub(a.child_ns);
        s.depth = s.depth.saturating_sub(1);
        if s.depth == 0 {
            s.incl_ns += dur;
        }
        if let Some(parent) = self.active.last_mut() {
            parent.child_ns += dur;
        }
    }

    pub(crate) fn report(&self) -> ProfileReport {
        let mut rows: Vec<ProfileRow> = self
            .stats
            .iter()
            .map(|(&id, s)| ProfileRow {
                name: crate::name::resolve(id).to_string(),
                calls: s.calls,
                inclusive: Duration::from_nanos(s.incl_ns),
                exclusive: Duration::from_nanos(s.excl_ns),
            })
            .collect();
        // Longest exclusive time first; ties by calls, then name, so the
        // order never depends on hash order.
        rows.sort_by(|a, b| {
            b.exclusive
                .cmp(&a.exclusive)
                .then(b.calls.cmp(&a.calls))
                .then(a.name.cmp(&b.name))
        });
        ProfileReport { rows }
    }
}

/// One procedure's line of a [`ProfileReport`].
#[derive(Clone, Debug)]
pub struct ProfileRow {
    pub name: String,
    pub calls: u64,
    /// Wall time from call to return, counted once per outermost recursive
    /// frame, children included.
    pub inclusive: Duration,
    /// `inclusive` less the time spent in other named procedures it called.
    pub exclusive: Duration,
}

/// The finished profile, sorted by exclusive time, longest first.
#[derive(Clone, Debug)]
pub struct ProfileReport {
    pub rows: Vec<ProfileRow>,
}

impl ProfileReport {
    /// Readable text, the top `top` rows (all if `None`).
    pub fn to_text(&self, top: Option<usize>) -> String {
        let shown = top.map_or(self.rows.len(), |n| n.min(self.rows.len()));
        let rows = &self.rows[..shown];
        let mut out = format!(
            "pscat: profile, {} of {} procedures by exclusive time\n",
            shown,
            self.rows.len()
        );
        out.push_str(&format!(
            "  {:>10}  {:>12}  {:>12}  {}\n",
            "calls", "exclusive", "inclusive", "procedure"
        ));
        for r in rows {
            out.push_str(&format!(
                "  {:>10}  {:>12}  {:>12}  {}\n",
                r.calls,
                fmt_ms(r.exclusive),
                fmt_ms(r.inclusive),
                r.name
            ));
        }
        if shown < self.rows.len() {
            out.push_str(&format!(
                "  ... {} more (--profile-top N, 0 = all)\n",
                self.rows.len() - shown
            ));
        }
        out
    }

    /// One JSON object: `{"procedures":[{"name","calls","inclusive_ms","exclusive_ms"}...]}`.
    pub fn to_json(&self, top: Option<usize>) -> String {
        let shown = top.map_or(self.rows.len(), |n| n.min(self.rows.len()));
        let mut out = String::from("{\"procedures\":[");
        for (i, r) in self.rows[..shown].iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"name\":{},\"calls\":{},\"inclusive_ms\":{:.3},\"exclusive_ms\":{:.3}}}",
                json_str(&r.name),
                r.calls,
                r.inclusive.as_secs_f64() * 1e3,
                r.exclusive.as_secs_f64() * 1e3
            ));
        }
        out.push_str(&format!("],\"total_procedures\":{}}}", self.rows.len()));
        out
    }
}

fn fmt_ms(d: Duration) -> String {
    format!("{:.3} ms", d.as_secs_f64() * 1e3)
}

fn json_str(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}
