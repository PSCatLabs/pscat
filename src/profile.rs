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

/// The caller of a call made from the top level (no procedure open).
const ROOT: u32 = u32::MAX;
/// `Active::edge` when the call graph is off.
const NO_EDGE: usize = usize::MAX;

/// One caller -> callee pair's running totals (`--profile-graph`).
struct EdgeStat {
    caller: u32,
    callee: u32,
    calls: u64,
    /// Calls along this edge now open; time is added when the outermost
    /// closes, so a recursive edge (`fib` -> `fib`) is not counted twice.
    depth: u32,
    incl_ns: u64,
}

/// A call in progress (one per marker frame on the exec stack, same order).
struct Active {
    id: u32,
    start: u64,
    /// Inclusive time of the calls this one made, to subtract for exclusive.
    child_ns: u64,
    /// Index into `Profiler::edges` of the edge this call runs along.
    edge: usize,
    /// `Profiler::current` when this call opened, to put back when it closes.
    caller: u32,
}

/// `--profile-ops`: time inside each built-in operator, by function address
/// (the loop sees operators as bare `fn` pointers, not names).
#[derive(Default)]
struct OpTimes {
    /// Address -> the names it is bound to in systemdict, joined with `/`.
    names: HashMap<usize, String>,
    /// Address -> (calls, nanoseconds).
    stats: HashMap<usize, (u64, u64)>,
}

#[derive(Default)]
pub(crate) struct Profiler {
    ops: Option<OpTimes>,
    stats: HashMap<u32, Stat>,
    active: Vec<Active>,
    /// `--profile-graph`: record who called what. Off, the profiler does
    /// no edge work at all.
    graph: bool,
    edge_ix: HashMap<(u32, u32), usize>,
    edges: Vec<EdgeStat>,
    /// The procedure whose body is running now (`ROOT` at the top level).
    /// Not the same as the innermost marker: a tail call into a name that
    /// already has a marker reuses it, so with `ping` <-> `pong` the marker
    /// on top says `pong` while `ping` runs. Kept only with the graph on.
    current: u32,
}

impl Profiler {
    pub(crate) fn new() -> Self {
        // Start the clock's zero point before the first reading.
        crate::clock::Clock::mono_ns();
        Profiler {
            current: ROOT,
            ..Self::default()
        }
    }

    /// Record caller -> callee edges too (before the program runs).
    pub(crate) fn set_graph(&mut self) {
        self.graph = true;
    }

    /// Time operators too (before the program runs). `bound` is every
    /// (name, function) pair systemdict holds, for labelling the report.
    pub(crate) fn set_ops(&mut self, bound: impl Iterator<Item = (String, usize)>) {
        let mut by_addr: HashMap<usize, Vec<String>> = HashMap::new();
        for (name, addr) in bound {
            by_addr.entry(addr).or_default().push(name);
        }
        let names = by_addr
            .into_iter()
            .map(|(addr, mut v)| {
                v.sort();
                v.dedup();
                (addr, v.join("/"))
            })
            .collect();
        self.ops = Some(OpTimes {
            names,
            stats: HashMap::new(),
        });
    }

    #[inline]
    pub(crate) fn ops_on(&self) -> bool {
        self.ops.is_some()
    }

    /// One finished operator call.
    pub(crate) fn record_op(&mut self, addr: usize, ns: u64) {
        if let Some(o) = self.ops.as_mut() {
            let s = o.stats.entry(addr).or_default();
            s.0 += 1;
            s.1 += ns;
        }
    }

    /// The edge from the procedure now running (or the top level) to `callee`.
    fn edge_to(&mut self, callee: u32) -> usize {
        let caller = self.current;
        let edges = &mut self.edges;
        *self.edge_ix.entry((caller, callee)).or_insert_with(|| {
            edges.push(EdgeStat {
                caller,
                callee,
                calls: 0,
                depth: 0,
                incl_ns: 0,
            });
            edges.len() - 1
        })
    }

    /// Does one of the top `k` open calls belong to `id`? Those are the
    /// procedures currently in tail position (see `Interp::prof_call`).
    pub(crate) fn tail_has(&self, k: usize, id: u32) -> bool {
        self.active[self.active.len().saturating_sub(k)..]
            .iter()
            .any(|a| a.id == id)
    }

    /// Count a call that reuses an open marker (a tail call).
    /// The edge counts the call too, with no time of its own: the time is
    /// part of the open call it continues.
    pub(crate) fn count_only(&mut self, id: u32) {
        self.stats.entry(id).or_default().calls += 1;
        if self.graph {
            let e = self.edge_to(id);
            self.edges[e].calls += 1;
            self.current = id;
        }
    }

    pub(crate) fn enter(&mut self, id: u32) {
        let s = self.stats.entry(id).or_default();
        s.calls += 1;
        s.depth += 1;
        let caller = self.current;
        let edge = if self.graph {
            let e = self.edge_to(id);
            self.edges[e].calls += 1;
            self.edges[e].depth += 1;
            self.current = id;
            e
        } else {
            NO_EDGE
        };
        self.active.push(Active {
            id,
            start: crate::clock::Clock::mono_ns(),
            child_ns: 0,
            edge,
            caller,
        });
    }

    /// Take back an `enter` whose frame could not be pushed.
    pub(crate) fn abort_enter(&mut self) {
        if let Some(a) = self.active.pop() {
            if let Some(s) = self.stats.get_mut(&a.id) {
                s.depth -= 1;
            }
            if let Some(e) = self.edges.get_mut(a.edge) {
                e.depth -= 1;
                self.current = a.caller;
            }
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
        if let Some(e) = self.edges.get_mut(a.edge) {
            e.depth = e.depth.saturating_sub(1);
            if e.depth == 0 {
                e.incl_ns += dur;
            }
            self.current = a.caller;
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
        let edges = self.graph.then(|| {
            let mut edges: Vec<EdgeRow> = self
                .edges
                .iter()
                .map(|e| EdgeRow {
                    caller: (e.caller != ROOT).then(|| crate::name::resolve(e.caller).to_string()),
                    callee: crate::name::resolve(e.callee).to_string(),
                    calls: e.calls,
                    inclusive: Duration::from_nanos(e.incl_ns),
                })
                .collect();
            // Most time first; ties by calls, then names, never hash order.
            edges.sort_by(|a, b| {
                b.inclusive
                    .cmp(&a.inclusive)
                    .then(b.calls.cmp(&a.calls))
                    .then_with(|| a.caller.cmp(&b.caller))
                    .then_with(|| a.callee.cmp(&b.callee))
            });
            edges
        });
        let operators = self.ops.as_ref().map(|o| {
            let mut ops: Vec<OpRow> = o
                .stats
                .iter()
                .map(|(addr, &(calls, ns))| OpRow {
                    name: o
                        .names
                        .get(addr)
                        .cloned()
                        .unwrap_or_else(|| UNNAMED_OPERATOR.to_string()),
                    calls,
                    time: Duration::from_nanos(ns),
                })
                .collect();
            ops.sort_by(|a, b| {
                b.time
                    .cmp(&a.time)
                    .then(b.calls.cmp(&a.calls))
                    .then_with(|| a.name.cmp(&b.name))
            });
            ops
        });
        ProfileReport {
            rows,
            edges,
            operators,
        }
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

/// One caller -> callee line of the call graph (`--profile-graph`).
#[derive(Clone, Debug)]
pub struct EdgeRow {
    /// The procedure that made the call, or `None` for the top level.
    pub caller: Option<String>,
    pub callee: String,
    /// Calls along this edge. Summed over a callee's incoming edges this is
    /// the callee's [`ProfileRow::calls`].
    pub calls: u64,
    /// Time the callee ran when called from `caller`, children included,
    /// counted once per outermost recursive frame of the edge. A tail call
    /// adds a call and no time: its time is in the call it continues.
    pub inclusive: Duration,
}

/// One built-in operator's line of the `--profile-ops` table.
#[derive(Clone, Debug)]
pub struct OpRow {
    /// The operator's name in systemdict (`a/b` if several names are bound
    /// to the same function).
    pub name: String,
    pub calls: u64,
    /// Time inside the operator's own code, summed over its calls. Includes
    /// the two clock reads around each call (tens of nanoseconds).
    pub time: Duration,
}

/// The label for an operator whose function no systemdict name is bound to.
const UNNAMED_OPERATOR: &str = "(unnamed operator)";

/// The finished profile, sorted by exclusive time, longest first.
#[derive(Clone, Debug)]
pub struct ProfileReport {
    pub rows: Vec<ProfileRow>,
    /// The call graph, longest edge first; `None` unless it was asked for.
    pub edges: Option<Vec<EdgeRow>>,
    /// Time per built-in operator, longest first; `None` unless asked for.
    pub operators: Option<Vec<OpRow>>,
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
        if let Some(edges) = &self.edges {
            let shown = top.map_or(edges.len(), |n| n.min(edges.len()));
            out.push_str(&format!(
                "pscat: call graph, {} of {} edges by inclusive time\n",
                shown,
                edges.len()
            ));
            out.push_str(&format!(
                "  {:>10}  {:>12}  {}\n",
                "calls", "inclusive", "caller -> callee"
            ));
            for e in &edges[..shown] {
                out.push_str(&format!(
                    "  {:>10}  {:>12}  {} -> {}\n",
                    e.calls,
                    fmt_ms(e.inclusive),
                    e.caller.as_deref().unwrap_or(TOP_LEVEL),
                    e.callee
                ));
            }
            if shown < edges.len() {
                out.push_str(&format!(
                    "  ... {} more (--profile-top N, 0 = all)\n",
                    edges.len() - shown
                ));
            }
        }
        if let Some(ops) = &self.operators {
            let shown = top.map_or(ops.len(), |n| n.min(ops.len()));
            out.push_str(&format!(
                "pscat: operators, {} of {} by time\n",
                shown,
                ops.len()
            ));
            out.push_str(&format!(
                "  {:>10}  {:>12}  {:>9}  {}\n",
                "calls", "time", "per call", "operator"
            ));
            for o in &ops[..shown] {
                let per_call = o.time.as_nanos() / u128::from(o.calls.max(1));
                out.push_str(&format!(
                    "  {:>10}  {:>12}  {:>6} ns  {}\n",
                    o.calls,
                    fmt_ms(o.time),
                    per_call,
                    o.name
                ));
            }
            if shown < ops.len() {
                out.push_str(&format!(
                    "  ... {} more (--profile-top N, 0 = all)\n",
                    ops.len() - shown
                ));
            }
        }
        out
    }

    /// One JSON object: `{"procedures":[{"name","calls","inclusive_ms","exclusive_ms"}...],
    /// "total_procedures":N}`, plus `"edges":[{"caller" (null at the top level),"callee",
    /// "calls","inclusive_ms"}...],"total_edges":N` with the call graph, and
    /// `"operators":[{"name","calls","time_ms"}...],"total_operators":N` with
    /// the operator times.
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
        out.push_str(&format!("],\"total_procedures\":{}", self.rows.len()));
        if let Some(edges) = &self.edges {
            let shown = top.map_or(edges.len(), |n| n.min(edges.len()));
            out.push_str(",\"edges\":[");
            for (i, e) in edges[..shown].iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let caller = e.caller.as_deref().map_or("null".to_string(), json_str);
                out.push_str(&format!(
                    "{{\"caller\":{},\"callee\":{},\"calls\":{},\"inclusive_ms\":{:.3}}}",
                    caller,
                    json_str(&e.callee),
                    e.calls,
                    e.inclusive.as_secs_f64() * 1e3
                ));
            }
            out.push_str(&format!("],\"total_edges\":{}", edges.len()));
        }
        if let Some(ops) = &self.operators {
            let shown = top.map_or(ops.len(), |n| n.min(ops.len()));
            out.push_str(",\"operators\":[");
            for (i, o) in ops[..shown].iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    "{{\"name\":{},\"calls\":{},\"time_ms\":{:.3}}}",
                    json_str(&o.name),
                    o.calls,
                    o.time.as_secs_f64() * 1e3
                ));
            }
            out.push_str(&format!("],\"total_operators\":{}", ops.len()));
        }
        out.push('}');
        out
    }
}

/// How the text view names a call made from outside every procedure.
const TOP_LEVEL: &str = "<top level>";

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
