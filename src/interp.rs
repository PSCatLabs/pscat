//! The interpreter: [`Interp`].
//!
//! Internally, the execution machine: operand stack, dictionary stack, and
//! an explicit execution stack.
//!
//! Execution uses an explicit stack of frames rather than host recursion,
//! for three reasons that all pay off in later stages:
//!
//! 1. Deeply recursive PostScript programs must not consume Rust call
//!    stack — depth is bounded by our own limit, and overflow becomes a
//!    catchable `execstackoverflow` instead of a process abort.
//! 2. The machine can be stepped one object at a time, which is the hook
//!    Stage 2's live rendering will drive (run N steps, present a frame).
//! 3. A procedure frame that has yielded its last element is popped
//!    *before* that element executes, so tail-recursive PostScript runs in
//!    constant execution-stack space for free.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::PsError;
use crate::font::{ShowCtx, ShowStep};
use crate::gfx::{DEFAULT_PAGE, Gfx, GraphicsState};
use crate::image::{ImageCtx, ImageStep};
use crate::lexer::{Lexer, Token};
use crate::limits::{Limits, LimitsError};
use crate::object::{
    Dict, ERROR_NAME_LIMIT, Num, Object, PsArray, PsString, SaveHandle, Value, cap_text,
};
use crate::ops;

/// Generous enough for any sane program (procedure calls in tail position
/// don't consume frames at all), small enough that runaway recursion is an
/// error rather than unbounded memory growth.
const EXEC_STACK_LIMIT: usize = 10_000;

/// Nesting cap for `{ { { ... } } }` at scan time; the scanner recurses per
/// nesting level, so this bounds Rust stack use on malicious input.
const PROC_NESTING_LIMIT: usize = 500;

/// `execute_resolved`'s `via` for an object that was not reached through a
/// name. No interned name has this id.
const NO_NAME: u32 = u32::MAX;
/// A `run` file's name as an error report shows it (issue #1043): capped
/// like an offending command, with `;` and `]` percent-escaped so a name
/// cannot forge the report's `; ` field separator or its ` ]%%` end
/// (`pscat-mcp` splits on both, #745), and CR/LF escaped so the report
/// stays one line (consumers read the first stderr line).
fn report_name(name: &str) -> String {
    cap_text(name, ERROR_NAME_LIMIT)
        .replace(';', "%3B")
        .replace(']', "%5D")
        .replace('\n', "%0A")
        .replace('\r', "%0D")
}

/// How many innermost `run` frames an error report's `Run chain` names
/// before eliding to the outermost one (issue #1043).
const ERROR_CHAIN_LIMIT: usize = 8;

/// What a procedure literal (an executable array met in a body) runs as,
/// so the step loop dispatches it like any operator (#864).
#[inline(never)]
fn proc_literal_op(it: &mut Interp) -> Result<(), PsError> {
    it.proc_literal()
}

/// A procedure literal (an executable array met in a body).
#[inline(always)]
fn is_proc_literal(o: &Object) -> bool {
    o.executable && matches!(o.value, Value::Array(_))
}

/// The body of procedure literal `o`, for a fused conditional to run.
#[inline(always)]
fn proc_body(o: &Object) -> Option<PsArray> {
    match &o.value {
        Value::Array(a) => Some(a.clone()),
        _ => None,
    }
}

/// Whether procedure element `o` runs the operator named `op` (#864): `Some(
/// NO_NAME)` if it is that operator, `Some(id)` if it is the executable name
/// `name_id` and that name resolves to the operator in `dstack` right now,
/// `None` otherwise. Only the operator's own name is looked up, so other
/// names cost one compare.
#[inline(always)]
fn runs_operator(o: &Object, dstack: &[Rc<RefCell<Dict>>], op: &str, name_id: u32) -> Option<u32> {
    if !o.executable {
        return None;
    }
    // By the operator's name, not its function's address: the same
    // function can have several addresses, and two functions one. Every
    // operator object carries the name it is registered under in
    // systemdict. `op` is the name systemdict's own object carries, and
    // any copy of that object holds the same `&'static str`: equal
    // pointers (and lengths) mean equal names, so the fast check is never
    // wrong, and the text compare covers any other operator object.
    let is_op = |o: &Object| {
        o.executable
            && matches!(&o.value, Value::Operator(f) if std::ptr::eq(f.name, op) || f.name == op)
    };
    match &o.value {
        Value::Operator(_) => is_op(o).then_some(NO_NAME),
        Value::Name(n) if n.id() == name_id => dstack
            .iter()
            .rev()
            .find_map(|d| d.borrow().get_id_ref(name_id).map(is_op))
            .is_some_and(|b| b)
            .then_some(name_id),
        _ => None,
    }
}

enum Frame {
    /// Tokens being scanned incrementally from program source.
    Scanner(Lexer),
    /// A procedure (executable array) being executed element by element.
    Proc { body: PsArray, pc: usize },
    /// `repeat`: run the body `remaining` more times.
    Repeat { body: PsArray, remaining: i64 },
    /// `loop`: run the body until `exit` (or an error) unwinds it.
    Loop { body: PsArray },
    /// `for`: push the control value, run the body, advance, repeat.
    For { body: PsArray, control: ForControl },
    /// `forall`: push element(s), run the body, advance.
    Forall { body: PsArray, src: ForallSrc },
    /// `pathforall`: one path element per step — push its user-space
    /// coordinates, run the matching proc (move/line/curve/close).
    /// The path is snapshotted at operator time; holds no external
    /// state, so unwinding needs no cleanup.
    PathForall {
        elems: Vec<PathForallEl>,
        idx: usize,
        /// moveto, lineto, curveto, closepath procs, in that order.
        procs: Box<[Object; 4]>,
    },
    /// The boundary a `stopped` context plants: reached normally, it pops
    /// and pushes `false`; `stop` (or a recoverable error) unwinds to it
    /// and pushes `true`.
    StopMark,
    /// The show family: one glyph per step; Type 3 glyphs and kshow
    /// procs run as ordinary frames above this one (see `crate::font`).
    Show(Box<ShowCtx>),
    /// image/imagemask/colorimage accumulating sample data; procedure
    /// data sources run as frames above (see `crate::image`).
    Image(Box<ImageCtx>),
    /// A one-shot continuation: an operator that must run a PostScript
    /// procedure mid-flight (a Separation tint transform) parks its
    /// completion here and pushes the procedure above; when the
    /// procedure finishes, the continuation consumes its results.
    /// Holds no external state, so unwinding needs no cleanup.
    PostOp(PostOp),
    /// `--profile` (issue #737): planted under the body of a procedure
    /// called through a name. Reached, or unwound, it closes that call in
    /// the profiler. Only exists while profiling.
    ProfReturn,
}

/// What `next_item` hands the step loop.
enum Item {
    /// An object scanned from source, to execute (or push).
    Obj(Object),
    /// A procedure frame with an element left is on top: the step loop
    /// runs that element with [`Interp::proc_step`], in this same step
    /// (#863). Procedure elements never pass through `next_item`'s
    /// `Action` machinery. Also returned, with a loop frame on top, when a
    /// run of `EMPTY_RUN` empty-body iterations pauses (#900); `proc_step`
    /// then finds no procedure and the step does nothing more.
    Proc,
}

/// What [`Interp::proc_step`] did with the top of the execution stack.
enum ProcStep {
    /// A procedure element that is an operator: call it. In the plain loop
    /// a procedure literal comes as `Op(proc_literal_op)` too (#864).
    Op(crate::object::OpFn),
    /// A procedure element that is an executable name: look it up and run it.
    Name(u32),
    /// A procedure literal, to push with [`Interp::proc_literal`]: what
    /// `proc_step::<true>` returns for one, so `--profile-ops` can tell it
    /// from an operator. The plain loop gets it as
    /// `Op(proc_literal_op)` (#864), so it has no arm of its own there.
    Lit,
    /// A procedure element that is pushed (a literal other than a
    /// procedure body): already on the operand stack.
    Pushed,
    /// The top frame is not a procedure: take the general path.
    Other,
}

pub(crate) enum PostOp {
    /// A Separation tint transform finished: pop the alt-space
    /// components it produced and make them the current color (the
    /// color space itself stays Separation).
    SeparationColor { alt_ncomp: u32 },
}

/// One `pathforall` element, coordinates already in user space.
pub(crate) enum PathForallEl {
    Move(f64, f64),
    Line(f64, f64),
    Curve([f64; 6]),
    Close,
}

pub(crate) enum ForallSrc {
    /// Elements are fetched live, so mutation mid-loop is visible —
    /// matching PostScript.
    Array(PsArray, usize),
    /// Bytes pushed as integers.
    Str(PsString, usize),
    /// Dict pairs are snapshotted when `forall` starts; the PLRM leaves
    /// order and mid-loop mutation behavior unspecified.
    Pairs(Vec<(Object, Object)>, usize),
}

pub(crate) struct ForControl {
    current: f64,
    increment: f64,
    limit: f64,
    /// All three operands were integers, so the pushed control values are
    /// integers too, per the PLRM.
    integral: bool,
}

impl ForControl {
    pub(crate) fn new(initial: Num, increment: Num, limit: Num) -> Self {
        let integral = matches!(
            (initial, increment, limit),
            (Num::Int(_), Num::Int(_), Num::Int(_))
        );
        ForControl {
            current: initial.to_f64(),
            increment: increment.to_f64(),
            limit: limit.to_f64(),
            integral,
        }
    }

    /// A zero increment never finishes — that matches PostScript, where
    /// such a loop runs until `exit`.
    fn finished(&self) -> bool {
        if self.increment >= 0.0 {
            self.current > self.limit
        } else {
            self.current < self.limit
        }
    }

    fn value(&self) -> Object {
        if self.integral {
            Object::int(self.current as i64)
        } else {
            Object::real(self.current)
        }
    }
}

/// One undo entry in the save/restore journal: the whole backing store
/// of an array or dict as it was when first mutated at the current save
/// level (object-granularity copy-on-write — see `docs/dev/VM.md`). Strings are
/// exempt from restore per PLRM §3.7.3.2, so they never appear here.
enum JEntry {
    Array {
        data: Rc<RefCell<Vec<Object>>>,
        old: Vec<Object>,
    },
    Dict {
        dict: Rc<RefCell<Dict>>,
        old: Dict,
    },
    /// A `gstate` object overwritten by `currentgstate` or `copy` (#394).
    GState {
        gs: Rc<RefCell<crate::gfx::GraphicsState>>,
        old: Box<crate::gfx::GraphicsState>,
    },
}

/// The interpreter-side record behind a `save` object.
struct SaveRecord {
    handle: Rc<SaveHandle>,
    /// Journal length at save time; restore undoes back to here.
    journal_mark: usize,
    /// `setpacking` is VM state: `restore` brings the old mode back.
    packing: bool,
    /// Backing-store pointers already journaled at this level. The
    /// journal entry keeps the `Rc` alive, so pointer reuse can't alias.
    seen: HashSet<usize>,
    /// Graphics snapshot (same mechanism as Type 3 glyph contexts):
    /// restore = grestoreall to the save point.
    gfx_depth: usize,
    gfx_state: Box<GraphicsState>,
    /// `setobjectformat` is VM state: `restore` brings the old value back.
    object_format: u8,
}

/// A PostScript interpreter with its own page, stacks and virtual memory.
///
/// Run a program with [`run_str`](Self::run_str) or
/// [`run_source`](Self::run_source), then read what it drew with
/// [`pages`](Self::pages) (or [`svg_pages`](Self::svg_pages) /
/// [`pdf_document`](Self::pdf_document) when those were enabled first).
/// State persists between runs, the way it does in a REPL: a second
/// `run_str` sees what the first one defined, and draws on the same page.
///
/// ```
/// let mut interp = pscat::Interp::new(); // US Letter, 612x792 points, 72 dpi
/// interp.run_str("/square { dup mul } def")?;
/// interp.capture_stdout();
/// interp.run_str("7 square ==")?;
/// assert_eq!(interp.captured_stdout(), b"49\n");
/// # Ok::<(), pscat::PsError>(())
/// ```
///
/// Not `Send`: see the [crate docs](crate#threads).
pub struct Interp {
    pub(crate) ostack: Vec<Object>,
    dstack: Vec<Rc<RefCell<Dict>>>,
    estack: Vec<Frame>,
    /// Mutation journal + live save records. Empty save stack = no
    /// journaling (the common, zero-overhead path).
    journal: Vec<JEntry>,
    save_stack: Vec<SaveRecord>,
    /// `setglobal`'s allocation mode and the modes `save` recorded (#1146).
    pub(crate) vm_mode: crate::ops::vm::VmMode,
    pub(crate) quit_requested: bool,
    /// The most recently executed name (interned id — resolving to
    /// text only happens at error time), for `OffendingCommand`.
    last_name: Option<u32>,
    /// The interned ids of the names `if` and `ifelse` (#864's fused
    /// conditionals).
    if_id: u32,
    ifelse_id: u32,
    /// The names systemdict's `if` and `ifelse` operator objects carry:
    /// every copy of them (a bound body's) holds the same `&'static str`,
    /// so [`runs_operator`] matches them by pointer before comparing text.
    if_name: &'static str,
    ifelse_name: &'static str,
    /// Fused steps run (#864); see [`Interp::fused_steps`].
    fused_steps: u64,
    /// Steps left in the running slice after the current one (#864): a
    /// fused run of several elements is charged here. Each step loop sets
    /// it on entry (`step_n_timed` to zero, so it never fuses); nothing
    /// runs a step loop from inside an operator, so it is not reentered.
    slice_left: usize,
    /// The error most recently caught by a `stopped`, kept so that a
    /// `stop` which reaches the top level can report it the way
    /// Ghostscript's outermost job wrapper does (issue #142).
    ///
    /// A payload cache, not the authority on which error is pending:
    /// `$error` is that, since it is a VM dict a `restore` rolls back
    /// while this field is not. This supplies only what `$error` has
    /// nowhere to keep — `syntaxerror`'s detail — and only when the two
    /// still agree on the error's name.
    caught_error: Option<PsError>,
    /// The offending object an `errordict` handler was run on (issue
    /// #372), waiting for `recover`/`record_error` to put it in
    /// `$error /command` as is. Ghostscript keeps the object itself
    /// (`(abc)`, `/foo`, `2`), not a name. Consumed by the next error,
    /// caught or not, so a REPL cannot carry it into an unrelated one.
    error_command: Option<Object>,
    /// The line most recently scanned from the main program source or a
    /// `run` file (issues #17, #1043) — `None` when no line is known.
    /// Scanning an eexec stream or executable string leaves it as it was
    /// (see `Lexer::line`). Sticky across procedure calls: an error deep
    /// inside a previously-defined procedure is attributed to the line
    /// that most recently touched real source text, not the procedure's
    /// original definition site — this interpreter doesn't tag objects
    /// with a source position.
    last_line: Option<usize>,
    /// The `run` file `last_line` belongs to (issue #1043); `None` when it
    /// is a line of the main program.
    last_file: Option<Rc<str>>,
    /// The `run` frames open when the last uncaught error left the machine
    /// (issue #1043), innermost first: `(file, line)`, `None` for the
    /// main program. Taken in `step_n` before it unwinds the execution
    /// stack (limit errors included); its innermost entry is the location
    /// `error_report` gives. Cleared by `begin_source` and by a top-level
    /// `stop`, so it never outlives the error it describes.
    error_chain: Vec<(Option<Rc<str>>, usize)>,
    /// The font names `findfont` has already printed a stand-in note for
    /// (issue #728), so a `save`/`restore` loop warns once per name per
    /// interpreter, not once per call.
    pub(crate) font_notes_shown: std::collections::HashSet<String>,
    /// The notes `findfont` raised for substituted pack faces this run,
    /// one per name, newline-joined. The wasm build has no stderr, so
    /// the npm package reads them here (`Renderer.warnings`, issue #750).
    pub(crate) font_warnings: String,
    /// State for `rand`/`srand`/`rrand`. Deterministic by default —
    /// reproducible art is a feature here, not a bug.
    pub(crate) rand_state: i64,
    /// When set, every `srand` call ignores its operand and reseeds
    /// with this value instead (issue #21's `--sweep-seed`) — lets a
    /// sweep drive the RNG on found art that hardcodes its own
    /// `N srand` line, with no source edit needed. The exception is an
    /// `srand` of a value `rrand` handed out under the override (a
    /// saved stream coming back, issue #202), which restores it exactly.
    /// `seed_override_fired`
    /// records whether an override actually intercepted a call (or
    /// sampled the override-seeded noise table, see `noise_from_override`),
    /// so a sweep over a script that never calls `srand` or samples noise
    /// can be flagged instead of silently producing identical frames.
    pub(crate) seed_override: Option<i64>,
    /// The permutation table the noise operators sample (`noiseseed`,
    /// issue #734). Separate from `rand_state`, so noise never moves a
    /// `rand` sequence.
    pub(crate) noise_perm: [u8; 256],
    /// `noise_perm` was built from the seed override (issue #753), and the
    /// program has not called `noiseseed` since. A program's own
    /// `noiseseed` clears it and keeps its own table; sampling the table
    /// while it is set counts as the override firing.
    pub(crate) noise_from_override: bool,
    pub(crate) seed_override_fired: bool,
    /// `setpacking` flag — tracked, but packed arrays are ordinary
    /// arrays here (ops/level2.rs).
    pub(crate) packing: bool,
    /// `setobjectformat` (#733): 0 off, 1 to 4 the binary object sequence
    /// byte order. Starts at 2 (IEEE, low byte first) on every platform,
    /// which is Ghostscript's default on a little-endian host.
    pub(crate) object_format: u8,
    /// `usertime`'s zero point.
    pub(crate) clock: crate::clock::Clock,
    /// Non-Font resource categories (ops/level2.rs). Font shares
    /// FontDirectory instead.
    resources: crate::ops::level2::CategoryMap,
    /// Set by the CLI's single-run path: dropping this `Interp` leaves the
    /// dictionary graph alone (see `Drop for Interp`).
    keep_dicts_on_drop: bool,
    /// When set, `=`, `==`, `stack`, `pstack` and `print` append here
    /// instead of writing to the process's stdout (issue #252). The
    /// browser build turns it on, since a wasm32 module has no stdout,
    /// so the playground can show what a program prints. Off by default:
    /// the CLI, REPL and MCP server keep writing straight to stdout.
    stdout_capture: Option<Vec<u8>>,
    /// Set once the capture buffer has dropped output and carries the
    /// truncation note; later writes are discarded.
    stdout_truncated: bool,
    /// When set, what the program writes to stderr (`(%stderr)` files,
    /// `handleerror`, `findfont`'s stand-in notes) is dropped; the notes
    /// still collect in `font_warnings`. `pscat --bands` sets it on every
    /// band but the top one (#877).
    quiet_stderr: bool,
    /// Caps on the running program (issue #604); `Limits::none()` unless
    /// `set_limits` was called, and then every limit check is skipped.
    limits: Limits,
    /// The most device pixels a page `setpagedevice` makes may have
    /// (`--max-pixels`, #909); `None` leaves only the 8000-a-side ceiling.
    pub(crate) max_page_pixels: Option<u64>,
    /// `setpagedevice` keys already warned about as unsupported, and
    /// whether an explicit page's refusal was noted (#909): each once.
    pub(crate) page_device_notes: std::collections::HashSet<String>,
    /// How `kernelimage` runs its procedure (`.kernelmode`, #742).
    pub(crate) kernel_mode: crate::kernel::KernelMode,
    /// `.kernelmath /fast` (#1100): `sin`, `cos` and `atan` vectorised
    /// in `kernelimage`'s compiled kernels. On unless `PSCAT_KERNEL_MATH`
    /// is `exact` ([`crate::kernel::default_fast_math`]).
    pub(crate) kernel_fast_math: bool,
    /// `.kernelshare`: when the kernel compiler shares procedure bodies.
    pub(crate) kernel_share: crate::kernel::compile::ShareMode,
    /// `.kernelcaps`: the kernel compiler's size caps (a test hook).
    pub(crate) kernel_caps: [usize; 3],
    /// Reasons a `kernelimage` procedure did not compile, each noted once.
    pub(crate) kernel_notes: std::collections::HashSet<String>,
    /// `kernelimage` calls finished by the compiled path, by it with some
    /// pixels left to the interpreter, and wholly by the interpreter
    /// (`.kernelstats`).
    pub(crate) kernel_stats: (i64, i64, i64),
    /// Steps run by the current program (`begin_source` zeroes it).
    steps_used: u64,
    /// Starts when the current program does; read only when `limits` has a
    /// time cap.
    run_clock: crate::clock::Clock,
    /// Process heap bytes when `set_limits` ran: the memory cap counts
    /// growth from here.
    mem_base: usize,
    /// Loop iterations until the next `poll_slow` (see `tick`).
    poll_in: u32,
    /// The limits as the decode filters this interpreter makes see them,
    /// and the bytes they decoded since the last look (#1187). Every
    /// `filter` and `eexec` gets a handle; `sync_decode_budget` takes the
    /// bytes as steps and refreshes the probe wherever the limits are read.
    decode_budget: Rc<crate::limits::DecodeBudget>,
    /// Directories the program may write files in (issue #740); empty
    /// unless `set_write_roots` was called.
    write_roots: crate::write_roots::WriteRoots,
    /// Directories the program may read files from (issue #763); `None`
    /// (the default) reads wherever the search order finds a file.
    read_roots: Option<crate::read_roots::ReadRoots>,
    /// Disk files the program has open for writing (issue #762), weakly:
    /// a file nothing refers to any more drops, closing its descriptor and
    /// flushing itself. Counted against [`MAX_OPEN_OUTPUT_FILES`] and
    /// flushed together by [`Interp::flush_outputs`].
    open_outputs: Vec<std::rc::Weak<RefCell<crate::file::PsFile>>>,
    /// Set by a disk file dropped with bytes it could not write; shared
    /// with each [`DiskSink`](crate::file::DiskSink) this interpreter opens.
    lost_write: Rc<Cell<bool>>,
    /// Bytes the program has handed to disk files so far (issue #762). An
    /// `Arc` so the bands of a `--bands` render can count against one
    /// budget ([`Interp::share_write_count`], #1045).
    bytes_written: Arc<AtomicU64>,
    /// Set when a write to the process's stdout failed because the reader
    /// had gone (a closed pipe, `| head`), so the CLI can end quietly
    /// instead of reporting the `ioerror` that follows (#1059). Cleared when
    /// a `stopped` catches an error, so a later, unrelated `ioerror` is not
    /// taken for it.
    stdout_epipe_error: bool,
    /// The most bytes it may hand to disk files; `None` for no cap.
    max_write_bytes: Option<u64>,
    /// `--profile` (issue #737): `Some` while profiling. A program that
    /// is not profiled never looks at it per step (see `step_n`).
    profile: Option<Box<crate::profile::Profiler>>,
    /// `--profile-ops`: the name of the operator `proc_step::<true>` last
    /// returned as `ProcStep::Op`, for `step_n_timed` to label its row with
    /// (the step itself carries only the function). Written only there.
    op_name: &'static str,
    pub(crate) gfx: Gfx,
}

/// The most a captured stdout keeps (see [`Interp::capture_stdout`]): a
/// runaway `{ 1 == } loop` in a browser tab must not grow without limit.
pub const STDOUT_CAPTURE_LIMIT: usize = 64 * 1024;

/// Steps per `step_n` call in `run_source` when a time or memory limit is
/// set: small enough to notice a runaway within milliseconds, large enough
/// that the clock read is lost in the noise.
const LIMITED_SLICE: usize = 512;

/// The most disk files a program may have open for writing at once
/// (issue #762); one more is `limitcheck`.
pub const MAX_OPEN_OUTPUT_FILES: usize = 64;

/// Loop iterations between looks at the limits from inside `next_item`.
const POLL_PERIOD: u32 = 1024;

/// The most iterations of an empty loop body (`{ } loop`, `1 1 1e9 { } for`)
/// one step runs; they are free up to there, as they always were, so a short
/// empty loop keeps its step count (#900).
const EMPTY_RUN: u32 = 1024;
const STDOUT_TRUNCATED: &[u8] = b"\n%% output truncated: more than 64 KB\n";

/// Whether `scale` (device pixels per point, dpi / 72) can size a canvas:
/// finite and above zero. A NaN or 0 would otherwise round to a 1x1 page
/// and a negative one would mirror the page (#1133).
fn usable_scale(scale: f32) -> bool {
    scale.is_finite() && scale > 0.0
}

impl Interp {
    /// An interpreter with a US Letter page (612x792 points) at 72 dpi.
    /// Nothing chose that size, so a program's `setpagedevice /PageSize`
    /// may replace it. Equivalent to [`new_scaled(1.0)`](Self::new_scaled).
    pub fn new() -> Self {
        // A scale of 1 gives the 612x792-pixel default page; this cannot fail.
        Self::new_scaled(1.0).expect("default page size is valid")
    }

    /// The same interpreter as [`new`](Self::new), rendered at `scale`
    /// device pixels per point (`scale` is dpi / 72, so 2.0 renders at
    /// 144 dpi). The page is US Letter (612x792 points) until something
    /// chooses another size: a program's `setpagedevice /PageSize` may
    /// replace it, and the replacement page keeps `scale`. This is the
    /// constructor for a host that picks a resolution but leaves the page
    /// size to the program; [`with_page_scaled`](Self::with_page_scaled)
    /// fixes the size instead, so `setpagedevice` cannot change it.
    ///
    /// `None` if `scale` is not a positive finite number, or if the
    /// page is more than 8000 pixels on a side at that scale (about 10.1
    /// for Letter). A program that then asks for a larger page at that
    /// scale gets `configurationerror` from `setpagedevice`, as under `new`.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new_scaled(2.0).expect("valid scale");
    /// interp.run_str("<< /PageSize [200 100] >> setpagedevice showpage")?;
    /// let page = &interp.pages()[0];
    /// assert_eq!((page.width(), page.height()), (400, 200));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn new_scaled(scale: f32) -> Option<Self> {
        if !(scale.is_finite() && scale > 0.0) {
            return None;
        }
        let mut interp = Self::with_page_scaled(DEFAULT_PAGE.0, DEFAULT_PAGE.1, scale)?;
        interp
            .gfx
            .set_page_size_source(crate::gfx::PageSizeSource::Default);
        Some(interp)
    }

    /// An interpreter with a `width`x`height`-point page at 72 dpi (one
    /// pixel per point). `None` if the size is unusable: zero, or more than
    /// 8000 pixels on a side. The caller chose the size, so it is
    /// [`PageSizeSource::Explicit`](crate::gfx::PageSizeSource): a program's
    /// `setpagedevice` cannot change it (#909).
    pub fn with_page(width: u32, height: u32) -> Option<Self> {
        Self::with_page_scaled(width, height, 1.0)
    }

    /// A page of `width`x`height` *points* rendered at `scale` device
    /// pixels per point: `scale` is dpi / 72, so 2.0 renders at 144 dpi.
    /// `None` if `scale` is not a positive finite number, or if the page is
    /// more than 8000 pixels on a side at that scale.
    /// Explicit, like [`with_page`](Self::with_page); a host that wants the
    /// program's `setpagedevice` to size the page uses
    /// [`new_scaled`](Self::new_scaled) instead.
    pub fn with_page_scaled(width: u32, height: u32, scale: f32) -> Option<Self> {
        if !usable_scale(scale) {
            return None;
        }
        let px = |points: u32| ((points as f32 * scale).round() as u32).max(1);
        let (pw, ph) = (px(width), px(height));
        // Same ceiling as --page: keeps a huge dpi from attempting a
        // multi-gigabyte canvas.
        if pw > 8000 || ph > 8000 {
            return None;
        }
        let mut gfx = Gfx::with_scale(pw, ph, scale)?;
        gfx.set_page_points(f64::from(width), f64::from(height));
        gfx.set_page_size_source(crate::gfx::PageSizeSource::Explicit);
        let mut system = Dict::new();
        ops::install_all(&mut system);
        let op_name = |name: &'static str| match system.get(name).map(|o| o.value) {
            Some(Value::Operator(op)) => op.name,
            _ => name,
        };
        let (if_name, ifelse_name) = (op_name("if"), op_name("ifelse"));
        // Error machinery: $error is where recovered errors are recorded;
        // errordict holds a handler per error name, so a program can
        // re-raise a caught error with `errordict /name get exec` (issue
        // #372). A handler installed over one of them is not yet
        // consulted when an error happens — see docs/archive/NOTES.md.
        let mut errordict = Dict::new();
        ops::misc::install_errordict(&mut errordict);
        system.put(
            "errordict".into(),
            Object::lit(Value::Dict(Rc::new(RefCell::new(errordict)))),
        );
        let mut error_state = Dict::new();
        error_state.put("newerror".into(), Object::bool(false));
        system.put(
            "$error".into(),
            Object::lit(Value::Dict(Rc::new(RefCell::new(error_state)))),
        );
        let system = Rc::new(RefCell::new(system));
        let user = Rc::new(RefCell::new(Dict::new()));
        // The permanent dicts are reachable by name, per the PLRM —
        // found code writes `userdict /x get` and `systemdict begin`
        // routinely. (systemdict referencing itself is an Rc cycle;
        // `Drop for Interp` empties systemdict to break it.)
        system.borrow_mut().put(
            "systemdict".into(),
            Object::lit(Value::Dict(system.clone())),
        );
        system
            .borrow_mut()
            .put("userdict".into(), Object::lit(Value::Dict(user.clone())));
        Some(Interp {
            ostack: Vec::new(),
            dstack: vec![system, user],
            estack: Vec::new(),
            journal: Vec::new(),
            save_stack: Vec::new(),
            vm_mode: Default::default(),
            quit_requested: false,
            last_name: None,
            if_id: crate::name::intern("if").id(),
            if_name,
            ifelse_name,
            ifelse_id: crate::name::intern("ifelse").id(),
            fused_steps: 0,
            slice_left: 0,
            caught_error: None,
            error_command: None,
            last_line: None,
            last_file: None,
            error_chain: Vec::new(),
            font_notes_shown: std::collections::HashSet::new(),
            font_warnings: String::new(),
            rand_state: 1,
            seed_override: None,
            noise_perm: crate::ops::noise::perm_from_seed(0),
            noise_from_override: false,
            seed_override_fired: false,
            packing: false,
            object_format: 2,
            clock: crate::clock::Clock::start(),
            resources: Default::default(),
            keep_dicts_on_drop: false,
            stdout_capture: None,
            stdout_truncated: false,
            quiet_stderr: false,
            limits: Limits::none(),
            max_page_pixels: None,
            page_device_notes: std::collections::HashSet::new(),
            kernel_mode: crate::kernel::KernelMode::Auto,
            kernel_fast_math: crate::kernel::default_fast_math(),
            kernel_share: crate::kernel::compile::ShareMode::Auto,
            kernel_caps: crate::kernel::compile::DEFAULT_CAPS,
            kernel_notes: std::collections::HashSet::new(),
            kernel_stats: (0, 0, 0),
            steps_used: 0,
            run_clock: crate::clock::Clock::start(),
            mem_base: 0,
            poll_in: POLL_PERIOD,
            decode_budget: Rc::default(),
            write_roots: Default::default(),
            read_roots: None,
            open_outputs: Vec::new(),
            lost_write: Rc::new(Cell::new(false)),
            bytes_written: Arc::new(AtomicU64::new(0)),
            stdout_epipe_error: false,
            max_write_bytes: None,
            profile: None,
            op_name: "",
            gfx,
        })
    }

    /// Run a whole program to completion. On error the program is aborted
    /// and the error returned; the page and operand stack keep what it had
    /// done so far (see [`error_report`](Self::error_report)). Text that
    /// is not UTF-8 goes through [`run_source`](Self::run_source).
    pub fn run_str(&mut self, src: &str) -> Result<(), PsError> {
        self.run_source(src.as_bytes())
    }

    /// [`run_str`](Self::run_str) for program text as bytes, which is what
    /// PostScript files are: binary data and Latin-1 strings are fine.
    pub fn run_source(&mut self, src: &[u8]) -> Result<(), PsError> {
        self.begin_source(src);
        // Time and memory are looked at once per `step_n` call, so a run
        // with either limit takes smaller slices to notice sooner.
        let slice = if self.limits.max_time.is_some() || self.limits.max_memory.is_some() {
            LIMITED_SLICE
        } else {
            4096
        };
        while self.step_n(slice)? {}
        Ok(())
    }

    /// Put [`Limits`] on the programs this interpreter runs from now on.
    /// `Limits::none()` takes them off again. The step count and the clock
    /// restart with each program ([`begin_source`](Self::begin_source),
    /// [`run_str`](Self::run_str)); the memory cap counts heap growth from
    /// this call. Fails, and changes nothing, if `max_memory` is set and
    /// [`TrackingAllocator`](crate::TrackingAllocator) is not installed.
    ///
    /// ```
    /// use pscat::{Interp, Limits};
    /// let mut interp = Interp::new();
    /// interp.set_limits(Limits::none().with_max_steps(1_000))?;
    /// assert!(interp.run_str("1 2 add pop").is_ok());
    /// # Ok::<(), pscat::LimitsError>(())
    /// ```
    pub fn set_limits(&mut self, limits: Limits) -> Result<(), LimitsError> {
        if limits.max_memory.is_some() && !crate::memtrack::installed() {
            return Err(LimitsError::AllocatorNotInstalled);
        }
        self.limits = limits;
        self.mem_base = crate::memtrack::live_bytes();
        self.gfx
            .set_memory_budget(limits.max_memory.map(|max| (self.mem_base, max)));
        self.steps_used = 0;
        self.poll_in = POLL_PERIOD;
        self.run_clock = crate::clock::Clock::start();
        self.decode_budget.reset(self.limit_probe());
        Ok(())
    }

    /// Cap the pages `setpagedevice` may make at `max` device pixels
    /// (`pscat --max-pixels`, #909); `None` (the default) leaves only the
    /// 8000-pixels-a-side ceiling every page has. A larger `/PageSize` is a
    /// `configurationerror`, with a stderr line saying why. The page the
    /// interpreter was made with is the host's to check.
    pub fn set_max_page_pixels(&mut self, max: Option<u64>) {
        self.max_page_pixels = max;
    }

    /// Count the memory cap's heap growth from `base` bytes (a
    /// [`memtrack::live_bytes`](crate::memtrack::live_bytes) level) instead
    /// of the level when [`set_limits`](Self::set_limits) ran. For a host
    /// that runs several interpreters under one budget: `pscat --bands`
    /// gives every band the same base, taken before any band starts (#844).
    /// Call it after `set_limits`, which resets the base.
    pub fn set_memory_base(&mut self, base: usize) {
        self.mem_base = base;
        self.gfx
            .set_memory_budget(self.limits.max_memory.map(|max| (base, max)));
        self.decode_budget.set_probe(self.limit_probe());
    }

    /// The limits in force ([`Limits::none`] by default).
    pub fn limits(&self) -> Limits {
        self.limits
    }

    /// Let programs write files inside `dirs` (issue #740; the CLI's
    /// `--allow-write DIR`): `(path) (w) file`, `(a) file`, `renamefile`
    /// and `deletefile` work on paths that resolve inside one of them, and
    /// refuse every other disk path with `invalidfileaccess`, `..` and
    /// symlink escapes included (`src/write_roots.rs`). None by default;
    /// an empty slice takes them away again. Fails, changing nothing, if a
    /// directory doesn't exist.
    ///
    /// ```no_run
    /// let mut interp = pscat::Interp::new();
    /// interp.set_write_roots(&["out"])?;
    /// interp.run_str("(out/hello.txt) (w) file dup (hi) writestring closefile").unwrap();
    /// # Ok::<(), String>(())
    /// ```
    pub fn set_write_roots<P: AsRef<std::path::Path>>(&mut self, dirs: &[P]) -> Result<(), String> {
        self.write_roots = crate::write_roots::WriteRoots::new(dirs)?;
        Ok(())
    }

    /// The canonical write roots (empty by default).
    pub fn write_roots(&self) -> &[std::path::PathBuf] {
        self.write_roots.dirs()
    }

    /// The write policy, for the operators that write to disk.
    pub(crate) fn write_policy(&self) -> &crate::write_roots::WriteRoots {
        &self.write_roots
    }

    /// Cap the bytes a program may write to disk files (issue #762; the
    /// CLI's `--max-write-bytes`); `None` removes the cap, and it is off by
    /// default. The count is for the interpreter's whole life, starts at
    /// zero, and is the bytes handed to `write`, `writestring`,
    /// `writehexstring` and `writereals`, so overwriting a file after
    /// `setfileposition` counts again. A write that would pass the cap
    /// writes nothing and raises `limitcheck`. Calling this again also
    /// starts the count again (on a fresh count of its own, so it leaves
    /// alone one shared with other interpreters).
    pub fn set_max_write_bytes(&mut self, max: Option<u64>) {
        self.max_write_bytes = max;
        self.bytes_written = Arc::new(AtomicU64::new(0));
    }

    /// Count this interpreter's writes in `count`, which other
    /// interpreters may hold too, so they all spend one
    /// [`Interp::set_max_write_bytes`] budget (`--bands`, #1045). Call it
    /// after `set_max_write_bytes`, which starts a count of its own.
    pub fn share_write_count(&mut self, count: Arc<AtomicU64>) {
        self.bytes_written = count;
    }

    /// Whether the last error raised failed a write to stdout because its
    /// reader had gone (`EPIPE`), and nothing has caught it: an `ioerror`
    /// that is that, not a fault in the program.
    pub fn stdout_epipe_error(&self) -> bool {
        self.stdout_epipe_error
    }

    /// The write-bytes cap ([`Interp::set_max_write_bytes`]).
    pub fn max_write_bytes(&self) -> Option<u64> {
        self.max_write_bytes
    }

    /// Whether `n` more bytes fit under the write cap.
    pub(crate) fn check_write_bytes(&self, n: u64) -> Result<(), PsError> {
        match self.max_write_bytes {
            Some(max) if self.bytes_written.load(Ordering::Relaxed).saturating_add(n) > max => {
                Err(PsError::Limitcheck)
            }
            _ => Ok(()),
        }
    }

    /// Count `n` bytes against the write cap, or `limitcheck`.
    pub(crate) fn charge_write_bytes(&mut self, n: u64) -> Result<(), PsError> {
        let Some(max) = self.max_write_bytes else {
            self.bytes_written.fetch_add(n, Ordering::Relaxed);
            return Ok(());
        };
        // One atomic check-and-add: bands sharing the count must not both
        // pass a check that only one of them fits under. A compare-exchange
        // loop, not `fetch_update`, which newer toolchains rename.
        let mut used = self.bytes_written.load(Ordering::Relaxed);
        loop {
            let total = used.saturating_add(n);
            if total > max {
                return Err(PsError::Limitcheck);
            }
            match self.bytes_written.compare_exchange_weak(
                used,
                total,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(()),
                Err(now) => used = now,
            }
        }
    }

    /// Forget output files that are closed or gone, and refuse (with
    /// `limitcheck`) another one when [`MAX_OPEN_OUTPUT_FILES`] are open.
    /// Called before a disk file is opened, so a refusal truncates nothing.
    pub(crate) fn check_output_slot(&mut self) -> Result<(), PsError> {
        self.open_outputs.retain(|w| {
            w.upgrade()
                .is_some_and(|f| f.try_borrow().map_or(true, |f| f.is_open()))
        });
        if self.open_outputs.len() >= MAX_OPEN_OUTPUT_FILES {
            return Err(PsError::Limitcheck);
        }
        Ok(())
    }

    /// The note a [`DiskSink`](crate::file::DiskSink) sets when it is dropped
    /// with bytes it could not write.
    pub(crate) fn lost_write_note(&self) -> Rc<Cell<bool>> {
        self.lost_write.clone()
    }

    /// Track a disk output file just opened (after `check_output_slot`).
    pub(crate) fn register_output(&mut self, f: &crate::file::FileHandle) {
        self.open_outputs.push(Rc::downgrade(f));
    }

    /// Write out the buffered bytes of every open disk file, and report a
    /// failure: a file whose write failed, or one that was dropped without
    /// being closed and could not write (disk full), is `ioerror`. Every
    /// file is flushed regardless. The `pscat` binary calls this on every
    /// exit path, so a program that lost data exits non-zero; an embedder
    /// that leaves files open should call it before it drops the `Interp`
    /// (the drop flushes too, but cannot tell anyone). The note about a
    /// dropped file is cleared by this call.
    pub fn flush_outputs(&mut self) -> Result<(), PsError> {
        let flushed = self.flush_files();
        if self.lost_write.replace(false) {
            return Err(PsError::Io);
        }
        flushed
    }

    /// [`Interp::flush_outputs`] for the places that cannot raise (`showpage`,
    /// `quit`, `flush`, before a read): the bytes go out, and a failure stays
    /// on the file (or in the dropped-file note) for the exit flush to report.
    pub(crate) fn flush_files(&mut self) -> Result<(), PsError> {
        let mut result = Ok(());
        for w in &self.open_outputs {
            if let Some(f) = w.upgrade()
                && let Ok(mut f) = f.try_borrow_mut()
                && let Err(e) = f.flush_disk()
            {
                result = Err(e);
            }
        }
        result
    }

    /// Confine reads to `dirs` (issue #763; the CLI's `--read-root DIR`,
    /// and what `pscat-mcp` always does): `(path) (r) file`, `run`,
    /// `readreals` and `filenameforall` read and list only files that
    /// resolve inside one of them, and refuse the rest with
    /// `invalidfileaccess`, `..` and symlink escapes included
    /// (`src/read_roots.rs`). The embedded copy of `lib/` stays available.
    /// An empty slice confines reads to that embedded copy alone. Fails,
    /// changing nothing, if a directory doesn't exist.
    ///
    /// ```no_run
    /// let mut interp = pscat::Interp::new();
    /// interp.set_read_roots(&["."])?;
    /// assert!(interp.run_str("(/etc/hosts) (r) file").is_err());
    /// # Ok::<(), String>(())
    /// ```
    pub fn set_read_roots<P: AsRef<std::path::Path>>(&mut self, dirs: &[P]) -> Result<(), String> {
        self.read_roots = Some(crate::read_roots::ReadRoots::new(dirs)?);
        Ok(())
    }

    /// Lift the read confinement again: reads are open, the default.
    pub fn clear_read_roots(&mut self) {
        self.read_roots = None;
    }

    /// The canonical read roots, or `None` when reads are open (the
    /// default).
    pub fn read_roots(&self) -> Option<&[std::path::PathBuf]> {
        self.read_roots.as_ref().map(|r| r.dirs())
    }

    /// The read policy, for the operators that read from disk.
    pub(crate) fn read_policy(&self) -> Option<&crate::read_roots::ReadRoots> {
        self.read_roots.as_ref()
    }

    /// Where writing to `name` would go: a path inside the write roots, or
    /// the PostScript error the program gets (`invalidfileaccess` outside
    /// them or with none set). Operators that open files for writing use
    /// this check, through [`crate::write_roots::WriteRoots::open`].
    pub fn writable_path(&self, name: &str) -> Result<std::path::PathBuf, PsError> {
        self.write_roots.resolve_open(name)
    }

    /// What is left of the step and time limits, for work done off the
    /// machine (a compiled kernel, #742): steps still allowed, and the
    /// deadline as a `Clock::mono_ns` instant.
    pub(crate) fn remaining_budget(&self) -> (Option<u64>, Option<u64>) {
        let steps = self
            .limits
            .max_steps
            .map(|m| m.saturating_sub(self.steps_used));
        let deadline = self.limits.max_time.map(|t| {
            let left = (t.as_millis() as i64 - self.run_clock.elapsed_ms()).max(0) as u64;
            crate::clock::Clock::mono_ns().saturating_add(left.saturating_mul(1_000_000))
        });
        (steps, deadline)
    }

    /// The limits for a loop that cannot borrow the interpreter (#1176,
    /// [`LimitProbe`](crate::limits::LimitProbe)).
    pub(crate) fn limit_probe(&self) -> crate::limits::LimitProbe {
        if self.limits.is_none() {
            return crate::limits::LimitProbe::NONE;
        }
        let (steps_left, deadline_ns) = self.remaining_budget();
        crate::limits::LimitProbe {
            steps_left,
            deadline_ns,
            memory: self.limits.max_memory.map(|max| (self.mem_base, max)),
        }
    }

    /// Count `n` steps done off the machine.
    pub(crate) fn charge_steps(&mut self, n: u64) {
        self.steps_used = self.steps_used.saturating_add(n);
    }

    /// The handle every decode filter this interpreter makes holds (#1187).
    pub(crate) fn decode_budget(&self) -> Rc<crate::limits::DecodeBudget> {
        self.decode_budget.clone()
    }

    /// Take what the decode filters decoded since the last look as steps
    /// (a step per KiB, as for a whole-file read), then give them the
    /// limits as they stand now. Called wherever the limits are read; only
    /// under limits, so the clock reads it makes cost nothing otherwise.
    fn sync_decode_budget(&mut self) {
        if self.limits.is_none() {
            return;
        }
        let steps = self.decode_budget.take_steps();
        self.steps_used = self.steps_used.saturating_add(steps);
        self.decode_budget.set_probe(self.limit_probe());
    }

    /// The permanent systemdict.
    pub(crate) fn systemdict(&self) -> Rc<RefCell<Dict>> {
        self.dstack[0].clone()
    }

    /// Interpreter steps the current program has run, counted only while
    /// some [`Limits`] are set (otherwise 0). A KiB read by `file` or `run`
    /// counts as one step. The count moves in slices (up
    /// to 4096 steps, 512 with a time or memory limit), so it can trail the
    /// true figure by a slice.
    pub fn steps_used(&self) -> u64 {
        self.steps_used
    }

    /// Queue a program for execution without running it; drive it with
    /// [`step_n`](Self::step_n). This is the live-rendering entry point:
    /// run a slice, look at the page, run the next.
    pub fn begin_source(&mut self, src: &[u8]) {
        self.quit_requested = false;
        // A canvas the last program could not restore within the memory cap
        // stopped that program; it does not stop this one (#909).
        self.gfx.clear_memory_refused();
        self.last_name = None;
        self.last_line = None;
        self.last_file = None;
        self.error_chain.clear();
        if !self.limits.is_none() {
            self.steps_used = 0;
            self.poll_in = POLL_PERIOD;
            self.run_clock = crate::clock::Clock::start();
            self.decode_budget.reset(self.limit_probe());
        }
        self.estack
            .push(Frame::Scanner(Lexer::main_program(src.to_vec())));
    }

    /// Execute up to `budget` objects. A step that executes no object, such
    /// as a run of up to `EMPTY_RUN` iterations of an empty loop body,
    /// counts as one, so the call always returns after bounded work (#900).
    /// `Ok(true)` means work remains. On
    /// error (or `quit`) the execution stack is cleared — the program is
    /// aborted — but the operand stack and canvas are left for inspection.
    pub fn step_n(&mut self, budget: usize) -> Result<bool, PsError> {
        // One call site for `step_n_inner`, so the compiler inlines the hot
        // loop here as it always did; the limits are all in the cold
        // `limit_budget` and `after_slice` (issue #604).
        let limited = !self.limits.is_none();
        let budget = if limited {
            match self.limit_budget(budget) {
                Ok(n) => n,
                Err(e) => {
                    self.snapshot_error_chain();
                    self.unwind_all();
                    return Err(e);
                }
            }
        } else {
            budget
        };
        // `--profile-ops` times every operator, in its own copy of the loop
        // (checked once per slice, so the plain loop carries no per-step test).
        let mut result = if self.profile.as_deref().is_some_and(|p| p.ops_on()) {
            self.step_n_timed(budget)
        } else {
            self.step_n_inner(budget)
        };
        if limited {
            result = self.after_slice(result, budget);
        }
        if result.is_err() || self.quit_requested {
            // Every uncaught error, limits included, passes here with the
            // execution stack still whole: the one place to record the
            // open `run` frames before they are dropped (#1043).
            if result.is_err() {
                self.snapshot_error_chain();
            }
            self.unwind_all();
        }
        result
    }

    /// Before a slice under `Limits`: refuse if one is already passed, and
    /// return the budget to run, no bigger than the steps the program has
    /// left. The step cap costs nothing per step: the inner loop gets a
    /// smaller budget and `after_slice` adds it to the count afterwards.
    #[cold]
    #[inline(never)]
    fn limit_budget(&mut self, budget: usize) -> Result<usize, PsError> {
        self.sync_decode_budget();
        self.check_time_and_memory()?;
        let mut n = budget;
        if let Some(max) = self.limits.max_steps {
            // One step past the cap is allowed, so a program of exactly
            // `max` steps is not refused for the pass that finds it done.
            if self.steps_used > max {
                return Err(PsError::StepsExceeded);
            }
            let left = (max - self.steps_used).saturating_add(1);
            n = n.min(usize::try_from(left).unwrap_or(usize::MAX));
        }
        Ok(n)
    }

    /// After a slice under `Limits`: count the loop iterations `tick` has not
    /// charged yet, so a program that finishes inside a poll period is held
    /// to the step cap too; add the slice's steps; and look at the clock and
    /// the heap again, so a program that passes the time or memory cap in its
    /// last slice is reported, not let through as a clean run.
    #[cold]
    #[inline(never)]
    fn after_slice(&mut self, result: Result<bool, PsError>, n: usize) -> Result<bool, PsError> {
        let more = result?;
        let pending = u64::from(POLL_PERIOD - self.poll_in);
        self.poll_in = POLL_PERIOD;
        self.steps_used = self.steps_used.saturating_add(pending);
        self.sync_decode_budget();
        if let Some(max) = self.limits.max_steps
            && self.steps_used > max
        {
            return Err(PsError::StepsExceeded);
        }
        if more {
            self.steps_used = self.steps_used.saturating_add(n as u64);
        }
        self.check_time_and_memory()?;
        Ok(more)
    }

    fn check_time_and_memory(&self) -> Result<(), PsError> {
        // A page device a restore could not bring back within the memory
        // cap (#909): the canvas was refused, and the run stops here.
        if self.gfx.memory_refused() {
            return Err(PsError::MemoryExceeded);
        }
        if let Some(max) = self.limits.max_time
            && self.run_clock.elapsed_ms() as u128 >= max.as_millis()
        {
            return Err(PsError::TimeExceeded);
        }
        if let Some(max) = self.limits.max_memory
            && self.heap_growth() > max
        {
            return Err(PsError::MemoryExceeded);
        }
        Ok(())
    }

    /// Called once per loop iteration, from inside `next_item`. A loop whose
    /// body is empty (`{ } loop`, `1 1 1000000000 { } for`) yields no object,
    /// so it runs up to `EMPTY_RUN` iterations before it returns to `step_n`
    /// where the limits are read; this counts the iterations itself. One decrement and a branch per
    /// iteration; the slow part runs once per `POLL_PERIOD`.
    #[inline]
    fn tick(&mut self) -> Result<(), PsError> {
        self.poll_in -= 1;
        if self.poll_in == 0 {
            self.poll_slow()
        } else {
            Ok(())
        }
    }

    #[cold]
    #[inline(never)]
    fn poll_slow(&mut self) -> Result<(), PsError> {
        self.poll_in = POLL_PERIOD;
        if self.limits.is_none() {
            return Ok(());
        }
        // A loop iteration counts as a step, on top of the objects its body
        // executes.
        self.steps_used = self.steps_used.saturating_add(u64::from(POLL_PERIOD));
        self.sync_decode_budget();
        if let Some(max) = self.limits.max_steps
            && self.steps_used > max
        {
            return Err(PsError::StepsExceeded);
        }
        self.check_time_and_memory()
    }

    /// Heap bytes the process has gained since `set_limits`.
    fn heap_growth(&self) -> usize {
        crate::memtrack::live_bytes().saturating_sub(self.mem_base)
    }

    /// The heap has grown past `--max-memory` (the kernel compiler's look,
    /// between interpreter steps).
    pub(crate) fn memory_exceeded(&self) -> bool {
        self.limits
            .max_memory
            .is_some_and(|max| self.heap_growth() > max)
    }

    /// The limits' look during a whole-file read (issue #702, `file::
    /// read_path_limited`), once per chunk and while waiting on a stalled
    /// source. Each of the `kib` whole KiB read since the last look is one
    /// step, so a step cap bounds a read too; `will_alloc` is heap the buffer
    /// is about to take, refused before it is made.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn poll_read(&mut self, kib: usize, will_alloc: usize) -> Result<(), PsError> {
        self.steps_used = self.steps_used.saturating_add(kib as u64);
        self.sync_decode_budget();
        if let Some(max) = self.limits.max_steps
            && self.steps_used > max
        {
            return Err(PsError::StepsExceeded);
        }
        if will_alloc > 0 {
            self.check_alloc(will_alloc)?;
        }
        self.check_time_and_memory()
    }

    /// The limits' look from inside one long operator (#1176): the same
    /// checks a slice ends with, raising the same uncatchable
    /// `limitexceeded`. `steps` is the work done since the last look, in
    /// the operator's own unit (a KiB read, 1024 path segments); 0 charges
    /// nothing and only reads the clock and the heap. Free when no limit is
    /// set, so an operator can call it every few thousand items of its loop.
    #[inline]
    pub(crate) fn poll_limits(&mut self, steps: u64) -> Result<(), PsError> {
        if self.limits.is_none() {
            return Ok(());
        }
        self.poll_limits_slow(steps)
    }

    #[cold]
    #[inline(never)]
    fn poll_limits_slow(&mut self, steps: u64) -> Result<(), PsError> {
        self.steps_used = self.steps_used.saturating_add(steps);
        self.sync_decode_budget();
        if let Some(max) = self.limits.max_steps
            && self.steps_used > max
        {
            return Err(PsError::StepsExceeded);
        }
        self.check_time_and_memory()
    }

    /// Refuse an allocation of `bytes` that would pass the memory cap,
    /// before making it. For the operators that can ask for megabytes in
    /// one step (`array`, `string`); everything else is caught by the
    /// per-slice check in `step_n_limited`.
    pub(crate) fn check_alloc(&self, bytes: usize) -> Result<(), PsError> {
        if let Some(max) = self.limits.max_memory
            && self.heap_growth().saturating_add(bytes) > max
        {
            return Err(PsError::MemoryExceeded);
        }
        Ok(())
    }

    /// Per-frame teardown shared by every unwind path — `unwind_all`
    /// (abort: an unhandled error or `quit`) and `do_stop` (recoverable:
    /// `stop`, or an error caught by an enclosing `stopped`). Any frame
    /// kind that holds state needing explicit cleanup when it's dropped
    /// mid-flight belongs here, not duplicated in both callers — missing
    /// one path is exactly how the eexec case (below) went unnoticed
    /// until issue #17's lint mode made it visible (round 5 of that
    /// PR's cross-model review: the first fix only covered
    /// `unwind_all`, missing that `stopped` catching an error/`stop`
    /// inside an eexec stream goes through `do_stop` instead — worse
    /// than the abort case, since execution *continues* afterward with
    /// a phantom systemdict on top of the dict stack, so a subsequent
    /// `def` lands in the wrong dictionary).
    fn cleanup_unwound_frame(&mut self, frame: Frame) {
        match frame {
            // Type 3 glyph contexts' graphics-state snapshot and paint
            // suppression must not leak past the show that created them.
            Frame::Show(mut ctx) => {
                ctx.cleanup(&mut self.gfx);
                self.after_device_change();
            }
            // A profiled call unwound by `stop`, an error, `exit` or `quit`
            // ends here.
            Frame::ProfReturn => self.prof_exit(),
            // `begin_eexec` pushes systemdict for the encrypted stream's
            // duration, popped on normal completion via
            // `Action::PopScannerAndDict`; a scanner dropped mid-flight
            // needs the same pop or systemdict ends up pushed twice.
            // Only when the *exact* dict `begin_eexec` pushed (by
            // identity, not merely "whatever's on top") is still there,
            // though: PostScript running inside the encrypted stream is
            // free to manage the dict stack itself (a Type 1 font's own
            // `currentdict end ... Private begin` is exactly this) —
            // `end`-ing the injected copy and `begin`-ning a
            // program-owned dict of its own before stopping/erroring
            // must not have that dict popped out from under it here
            // (round 6 of PR #59's review: an unconditional pop did).
            Frame::Scanner(lexer) if lexer.pop_systemdict && self.dstack.len() > 2 => {
                if let Some(top) = self.dstack.last()
                    && Rc::ptr_eq(top, &self.dstack[0])
                {
                    self.dstack.pop();
                }
            }
            _ => {}
        }
    }

    /// Abort the program: drop every frame (see `cleanup_unwound_frame`
    /// for what each kind needs).
    fn unwind_all(&mut self) {
        while let Some(frame) = self.estack.pop() {
            self.cleanup_unwound_frame(frame);
        }
    }

    /// [`step_n_inner`](Self::step_n_inner) with a clock read around each
    /// operator (`--profile-ops`). Kept as a copy of the loop, not a generic
    /// parameter of it: the plain loop must stay exactly as the compiler
    /// had it (#737 measured a generic version 19% slower on `fib`). The
    /// helpers it shares with that loop are `#[inline(always)]` so the
    /// second call site does not cost them their inlining.
    ///
    /// Time is taken only for operators the loop reaches by pointer or by
    /// name. An operator's own work is what is timed: `for`, `if` and
    /// `repeat` only push a frame (their bodies run as later steps, and
    /// are timed there), and `show` and `image` do their work in later
    /// frame steps of `next_item`, so they show only their set-up here.
    #[cold]
    #[inline(never)]
    fn step_n_timed(&mut self, budget: usize) -> Result<bool, PsError> {
        // No steps to spare: a procedure literal is pushed alone, never
        // fused, so every operator is still timed and counted on its own.
        self.slice_left = 0;
        for _ in 0..budget {
            if self.quit_requested {
                return Ok(false);
            }
            let mut step = self.proc_step::<true>();
            if let ProcStep::Other = step {
                match self.next_item() {
                    Ok(None) => return Ok(false),
                    Ok(Some(Item::Proc)) => step = self.proc_step::<true>(),
                    Ok(Some(Item::Obj(obj))) => {
                        let done = match (&obj.value, obj.executable) {
                            (Value::Name(n), true) => self.execute_name_timed(n.id()),
                            (Value::Operator(op), true) => self.timed_op(op.name, op.func),
                            _ => self.execute_element(obj),
                        };
                        if let Err(e) = done {
                            self.recover(e)?;
                        }
                        continue;
                    }
                    Err(e) => {
                        self.sync_scan_line();
                        self.recover(e)?;
                        continue;
                    }
                }
            }
            let done = match step {
                // A procedure literal is not an operator: not timed
                // (and never fused here, `slice_left` being zero).
                ProcStep::Lit => self.proc_literal(),
                ProcStep::Op(func) => self.timed_op(self.op_name, func),
                ProcStep::Name(id) => self.execute_name_timed(id),
                ProcStep::Pushed | ProcStep::Other => continue,
            };
            if let Err(e) = done {
                self.recover(e)?;
            }
        }
        Ok(!self.estack.is_empty())
    }

    #[inline(never)]
    fn timed_op(&mut self, name: &'static str, func: crate::object::OpFn) -> Result<(), PsError> {
        let t0 = crate::clock::Clock::mono_ns();
        let r = func(self);
        let ns = crate::clock::Clock::mono_ns().saturating_sub(t0);
        if let Some(p) = self.profile.as_mut() {
            p.record_op(name, ns);
        }
        r
    }

    /// `execute_name`, timing the operator if the name resolves to one.
    fn execute_name_timed(&mut self, id: u32) -> Result<(), PsError> {
        self.last_name = Some(id);
        let resolved = self
            .load_id(id)
            .ok_or_else(|| PsError::Undefined(crate::name::resolve(id).to_string()))?;
        if resolved.executable
            && let Value::Operator(op) = &resolved.value
        {
            return self.timed_op(op.name, op.func);
        }
        self.execute_resolved(resolved, id)
    }

    // `step_n_timed` is a copy of this loop (`--profile-ops`): change both.
    fn step_n_inner(&mut self, budget: usize) -> Result<bool, PsError> {
        // An explicit count of the steps left, so a fused run of several
        // elements (#864) can charge each of them.
        self.slice_left = budget;
        while self.slice_left > 0 {
            self.slice_left -= 1;
            if self.quit_requested {
                return Ok(false);
            }
            // A procedure element (nearly every step of a bound library)
            // is dispatched straight from the frame. Anything else goes
            // through `next_item`, which hands back `Item::Proc` when a
            // loop iteration (or a frame finishing) leaves a procedure on
            // top: its first element runs in this same step, as it always
            // has.
            let done = match self.proc_step::<false>() {
                ProcStep::Op(func) => func(self),
                ProcStep::Name(id) => self.execute_name(id),
                ProcStep::Pushed | ProcStep::Lit => continue,
                ProcStep::Other => match self.next_item() {
                    Ok(None) => return Ok(false),
                    Ok(Some(Item::Obj(obj))) => self.execute_element(obj),
                    Ok(Some(Item::Proc)) => match self.proc_step::<false>() {
                        ProcStep::Op(func) => func(self),
                        ProcStep::Name(id) => self.execute_name(id),
                        // `Other`: a run of empty loop iterations paused
                        // with no procedure on top (#900), a step that
                        // did nothing.
                        ProcStep::Pushed | ProcStep::Lit | ProcStep::Other => continue,
                    },
                    Err(e) => {
                        // A scan/syntax error short-circuits out of
                        // next_item before any Action ran, so the failing
                        // Scanner frame is still on top with its line set.
                        self.sync_scan_line();
                        self.recover(e)?;
                        continue;
                    }
                },
            };
            if let Err(e) = done {
                self.recover(e)?;
            }
        }
        Ok(!self.estack.is_empty())
    }

    /// One step of a procedure frame on top of the execution stack, taken
    /// without going through `next_item` (#863): the element is looked at
    /// in place (one momentary borrow, released before anything executes,
    /// so a `put` into the running procedure is still seen, #323). A
    /// literal is pushed right here, numbers copied without the
    /// out-of-line `clone`; an operator or executable name comes back for
    /// the caller to run. As in `next_item`, the frame is popped as it
    /// yields its last element, before that element runs (tail calls).
    /// `sync_scan_line` is not needed: the top frame is not a scanner.
    #[inline(always)]
    fn proc_step<const TIMED: bool>(&mut self) -> ProcStep {
        let Interp {
            estack,
            ostack,
            op_name,
            ..
        } = self;
        let Some(Frame::Proc { body, pc }) = estack.last_mut() else {
            return ProcStep::Other;
        };
        let i = *pc;
        // `pc` is advanced in here, except for a procedure literal, which
        // `proc_literal` takes from where it is; `pc` then still points
        // inside the body, so the tail pop below is not taken.
        let Some(step) = body.peek(i, |o| {
            if o.executable {
                match &o.value {
                    Value::Operator(op) => {
                        *pc = i + 1;
                        if TIMED {
                            *op_name = op.name;
                        }
                        return ProcStep::Op(op.func);
                    }
                    Value::Name(n) => {
                        *pc = i + 1;
                        return ProcStep::Name(n.id());
                    }
                    Value::Array(_) => {
                        return if TIMED {
                            ProcStep::Lit
                        } else {
                            ProcStep::Op(proc_literal_op)
                        };
                    }
                    _ => {}
                }
            }
            ostack.push(o.clone_inline());
            *pc = i + 1;
            ProcStep::Pushed
        }) else {
            // Not reached (an exhausted frame is popped as it yields its
            // last element), but `next_item` knows what to do with one.
            return ProcStep::Other;
        };
        if *pc >= body.len() {
            // Known to be this `Proc` frame: drop just its body, not
            // through the general frame drop glue.
            if let Some(Frame::Proc { body, .. }) = self.estack.pop() {
                drop(body);
            }
        }
        step
    }

    /// The procedure literal at the top frame's `pc` (#864): push it, or
    /// run it fused with the elements after it, as one step, when they are
    /// `{ } if` or `{ } { } ifelse` and the operand under the literal is a
    /// boolean ([`fused_conditional`](Self::fused_conditional)); a fused
    /// run's extra steps are charged to `slice_left`. Never fails: a fused
    /// run is only taken where it cannot.
    fn proc_literal(&mut self) -> Result<(), PsError> {
        let left = self.slice_left;
        if left > 0
            && let Some(Object {
                value: Value::Boolean(c),
                ..
            }) = self.ostack.last()
            && let Some(extra) = self.fused_conditional(left, *c)
        {
            self.slice_left -= extra;
            return Ok(());
        }
        let Some(Frame::Proc { body, pc }) = self.estack.last_mut() else {
            unreachable!("a procedure literal with no procedure frame on top")
        };
        let i = *pc;
        // The body's handle copied in place, not through `Value::clone`.
        if let Some(Some(lit)) = body.peek(i, proc_body) {
            self.ostack.push(Object::exec(Value::Array(lit)));
        }
        *pc = i + 1;
        if *pc >= body.len() {
            // Known to be this `Proc` frame: drop just its body.
            if let Some(Frame::Proc { body, .. }) = self.estack.pop() {
                drop(body);
            }
        }
        Ok(())
    }

    /// The procedure literal at the top frame's `pc` and the elements after
    /// it, as one step, if they are `{ } if` or `{ } { } ifelse`; `c` is the
    /// boolean on top of the operand stack. A fused run of `n` elements
    /// takes `n - 1` of the `left` steps, so it runs only when that many
    /// are left, and the step count is the same either way. Returns those
    /// extra steps, or `None` if nothing fuses (nothing is done then).
    ///
    /// Nothing is compiled or remembered: the elements are read from the
    /// body as it is now, under one borrow. That is the same as reading
    /// them a step apart, since pushing a literal runs no code that could
    /// change the body or the dictionaries in between. A run fuses only
    /// where the conditional cannot fail (a boolean and procedure literals
    /// as its operands, and room on the execution stack for the branch), so
    /// every error is raised by unfused elements, exactly as before.
    /// The conditional counts when it is the operator itself (a bound body)
    /// or the name `if` or `ifelse` resolving to it now (an unbound one);
    /// then `last_name` is set as running the name would set it.
    #[inline(never)]
    fn fused_conditional(&mut self, left: usize, c: bool) -> Option<usize> {
        let Interp {
            estack,
            ostack,
            dstack,
            if_id,
            ifelse_id,
            if_name,
            ifelse_name,
            ..
        } = self;
        let depth = estack.len();
        let Some(Frame::Proc { body, pc }) = estack.last_mut() else {
            return None;
        };
        let i = *pc;
        // (elements fused, the branch to run, the name it ran through)
        let (n, branch, via) = body.with_slice(|els| {
            let lit = els.get(i)?;
            let next = els.get(i + 1)?;
            if let Some(via) = runs_operator(next, dstack, if_name, *if_id) {
                return Some((2, if c { proc_body(lit) } else { None }, via));
            }
            if left < 2 || !is_proc_literal(next) {
                return None;
            }
            let via = runs_operator(els.get(i + 2)?, dstack, ifelse_name, *ifelse_id)?;
            Some((3, proc_body(if c { lit } else { next }), via))
        })?;
        let tail = i + n >= body.len();
        // The branch's frame goes where `if` would put it, after the tail
        // pop. If that would overflow the execution stack, nothing fuses:
        // the elements run one at a time and `if` raises the error.
        let branch = branch.filter(|b| !b.is_empty());
        if branch.is_some() && depth - usize::from(tail) >= EXEC_STACK_LIMIT {
            return None;
        }
        ostack.pop();
        *pc = i + n;
        if tail {
            self.estack.pop();
        }
        if via != NO_NAME {
            self.last_name = Some(via);
        }
        self.fused_steps += 1;
        if let Some(b) = branch {
            self.estack.push(Frame::Proc { body: b, pc: 0 });
        }
        Some(n - 1)
    }

    /// The number of fused steps run so far (#864): a test's check that a
    /// fused path was taken at all.
    #[doc(hidden)]
    pub fn fused_steps(&self) -> u64 {
        self.fused_steps
    }

    /// If a `stopped` context encloses the error, record it in `$error`,
    /// unwind to the context, and push `true` — errors are catchable.
    /// Otherwise propagate; the front end reports it.
    #[inline(always)]
    fn recover(&mut self, e: PsError) -> Result<(), PsError> {
        let command = self.error_command.take();
        // A limit error is never catchable: `stopped` would otherwise let a
        // runaway program swallow it and carry on (issue #604).
        if e.limit().is_some() || !self.estack.iter().any(|f| matches!(f, Frame::StopMark)) {
            return Err(e);
        }
        self.record_error(&e, command);
        self.stdout_epipe_error = false;
        self.caught_error = Some(e);
        self.do_stop();
        self.push(Object::bool(true));
        Ok(())
    }

    /// The error a top-level `stop` should report, if any (issue #142).
    ///
    /// Gated on `$error /newerror`, which is what Ghostscript's outermost
    /// job wrapper keys off — verified against it directly, including the
    /// quirk that a `stop` used for plain control flow *after* an earlier
    /// caught-and-handled error reports that stale error, while a `stop`
    /// with no error ever recorded reports nothing at all.
    ///
    /// `$error` is the authority on *which* error, not the `caught_error`
    /// cache. `$error` is a VM dict, so a `restore` rolls it back like
    /// anything else and can leave it naming an earlier error than the
    /// one last caught — the cache would then report the wrong one
    /// (Codex review of PR #138). The cache is consulted only to supply
    /// a payload `$error` has nowhere to keep, and only when it agrees
    /// with `$error` about the error's name.
    ///
    /// `last_name` is restored from `$error /command` for the same
    /// reason: by the time `stop` runs, the caller's cleanup and the
    /// `stop` itself have overwritten it, so `error_report` would name
    /// `stop` as the offending command rather than the `div` or `add`
    /// that actually failed. That write is why this takes `&mut self`
    /// and is named `take_` — it is only reached once `do_stop` has
    /// already drained the execution stack, so nothing but the report
    /// that immediately follows can observe it.
    ///
    /// `last_line` is deliberately *not* restored alongside it, so the
    /// report pairs the original operator with the `stop`'s own line.
    /// `$error` has nowhere to keep a line number, and the only other
    /// source is a Rust-side cache — which is exactly what a `restore`
    /// desynchronizes, the bug this method's `$error`-first design
    /// exists to avoid. A slightly wrong line beats a confidently wrong
    /// error (review of PR #138).
    pub(crate) fn take_top_level_stop_error(&mut self) -> Option<PsError> {
        let (newerror, errorname, command) = match self.load("$error") {
            Some(obj) => match &obj.value {
                Value::Dict(d) => {
                    let d = d.borrow();
                    let flag = matches!(
                        d.get("newerror").map(|o| o.value.clone()),
                        Some(Value::Boolean(true))
                    );
                    let name = match d.get("errorname").map(|o| o.value.clone()) {
                        Some(Value::Name(n)) => Some(n.to_string()),
                        _ => None,
                    };
                    // Any object can be there: an errordict handler
                    // stores the one it was run on (issue #372).
                    let cmd = d.get("command").map(|o| o.text());
                    (flag, name, cmd)
                }
                _ => (false, None, None),
            },
            None => (false, None, None),
        };
        // The machine has unwound; whatever chain an earlier uncaught
        // error left no longer describes this report.
        self.error_chain.clear();
        if !newerror {
            return None;
        }
        let name = errorname?;
        let rebuilt = PsError::from_name(&name, command.clone())?;
        // Rebuilt from `$error` for everything `$error` can express,
        // which is everything but `syntaxerror`'s detail — matching the
        // cache on the name alone is not enough, since two different
        // `undefined`s share it and a restore can leave `$error` naming
        // the earlier one (Codex review of PR #138).
        let err = match (&rebuilt, &self.caught_error) {
            (PsError::Syntax(_), Some(cached @ PsError::Syntax(_))) => cached.clone(),
            _ => rebuilt,
        };
        if let Some(c) = command {
            self.last_name = Some(crate::name::intern(&c).id());
        }
        Some(err)
    }

    /// Called by an `errordict` handler (issue #372): the object it was
    /// run on is the offending command, for `$error /command` and for
    /// the report of an error nothing catches.
    pub(crate) fn set_error_command(&mut self, obj: Object) {
        self.last_name = Some(crate::name::intern(&obj.text()).id());
        self.error_command = Some(obj);
    }

    fn record_error(&mut self, e: &PsError, command: Option<Object>) {
        let command = match command {
            Some(obj) => Some(obj),
            None => match e {
                PsError::Undefined(n) => Some(Object::exec(Value::Name(crate::name::intern(n)))),
                _ => self
                    .last_name
                    .map(|id| Object::exec(Value::Name(crate::name::resolve(id)))),
            },
        };
        let error_dict = match self.load("$error") {
            Some(obj) => match &obj.value {
                Value::Dict(d) => Some(d.clone()),
                _ => None,
            },
            None => None,
        };
        if let Some(d) = error_dict {
            // $error is a VM dict; its writes roll back like any other.
            self.journal_dict(&d);
            let mut d = d.borrow_mut();
            d.put("newerror".into(), Object::bool(true));
            d.put(
                "errorname".into(),
                Object::lit(Value::Name(e.name().into())),
            );
            if let Some(c) = command {
                d.put("command".into(), c);
            }
        }
    }

    /// Refresh `last_line` from the innermost frame if it's a
    /// main-program `Scanner` — a no-op (line stays sticky) otherwise.
    /// Called both when a token is successfully yielded *and* when
    /// scanning it failed (`next_token` sets its line before dispatch
    /// succeeds or fails, so the frame reflects the failing token's
    /// line either way — it just hasn't been popped yet, since the
    /// error propagated before any `Action` was applied).
    fn sync_scan_line(&mut self) {
        if let Some(Frame::Scanner(lexer)) = self.estack.last()
            && let Some(line) = lexer.line()
        {
            self.last_line = Some(line);
            // Per token: compare pointers so the common case (same file
            // as the last token) touches no reference count.
            let name = lexer.run_name();
            let same = match (name, &self.last_file) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                self.last_file = name.cloned();
            }
        }
    }

    /// Point `last_line`/`last_file` back at the innermost source still
    /// on the execution stack (#1043). Without this, an error right after
    /// `run` returns inside a procedure (`/f { (lib.ps) run 1 0 div } def`)
    /// would name the finished file, since no token is scanned between.
    fn restore_scan_location(&mut self) {
        for frame in self.estack.iter().rev() {
            if let Frame::Scanner(lexer) = frame
                && let Some(line) = lexer.line()
            {
                self.last_line = Some(line);
                self.last_file = lexer.run_name().cloned();
                return;
            }
        }
    }

    /// Record the open `run` frames for `error_report` (issue #1043).
    /// Called by `step_n` only for an error nothing caught, before it
    /// unwinds, so a `stopped` loop used as control flow pays nothing.
    fn snapshot_error_chain(&mut self) {
        self.error_chain.clear();
        for frame in self.estack.iter().rev() {
            if let Frame::Scanner(lexer) = frame
                && let Some(line) = lexer.line()
            {
                self.error_chain.push((lexer.run_name().cloned(), line));
            }
        }
    }

    /// `N`, `N (file)` or `N (file); Run chain: file:N <- outer:M <- line K`
    /// (issue #1043). A main-program line stays a bare number, so the
    /// report for a single-file program reads exactly as before.
    fn error_location(&self, line: usize) -> String {
        let file = match self.error_chain.first() {
            Some((f, _)) => f,
            None => &self.last_file,
        };
        let Some(file) = file else {
            return line.to_string();
        };
        let mut out = format!("{line} ({})", report_name(file));
        if self.error_chain.len() > 1 {
            let link = |(f, l): &(Option<Rc<str>>, usize)| match f {
                Some(f) => format!("{}:{l}", report_name(f)),
                None => format!("line {l}"),
            };
            let chain = &self.error_chain;
            let mut links: Vec<String>;
            if chain.len() > ERROR_CHAIN_LIMIT + 1 {
                // Deep `run` recursion: keep the innermost frames and the
                // outermost one, elide the middle.
                links = chain[..ERROR_CHAIN_LIMIT].iter().map(link).collect();
                links.push("...".to_string());
                links.extend(chain.last().map(link));
            } else {
                links = chain.iter().map(link).collect();
            }
            out.push_str("; Run chain: ");
            out.push_str(&links.join(" <- "));
        }
        out
    }

    /// LaserWriter-style error report, e.g.
    /// `%%[ Error: undefined; OffendingCommand: frobnicate ]%%`. Appends
    /// `; Line: N` (issue #17) when the line of the most recent token
    /// scanned from the program or a `run` file is known — see
    /// `last_line`'s doc comment for what "known" excludes. A `run` file's
    /// line names the file, `Line: 5 (inner.ps)`, followed by the chain of
    /// open `run` frames when there is one (issue #1043,
    /// `error_location`).
    pub fn error_report(&self, err: &PsError) -> String {
        let kind = match err {
            PsError::Syntax(detail) => format!("syntaxerror ({detail})"),
            _ => match err.limit() {
                Some(limit) => format!("limitexceeded ({limit})"),
                None => err.name().to_string(),
            },
        };
        let command = match err {
            PsError::Undefined(name) => cap_text(name, ERROR_NAME_LIMIT),
            _ => self.last_executed_name().map_or_else(
                || "--none--".to_string(),
                |n| cap_text(&n, ERROR_NAME_LIMIT),
            ),
        };
        // The chain's innermost entry is the source still open when the
        // error left the machine, which beats the sticky location: a
        // `stop` that unwound a `run` file mid-procedure scans no token
        // to correct it. The sticky one serves a top-level `stop`, whose
        // stack is already drained (no chain).
        let line = self.error_chain.first().map(|(_, l)| *l).or(self.last_line);
        match line {
            Some(line) => {
                let at = self.error_location(line);
                format!("%%[ Error: {kind}; OffendingCommand: {command}; Line: {at} ]%%")
            }
            None => format!("%%[ Error: {kind}; OffendingCommand: {command} ]%%"),
        }
    }

    /// Pull the next object to execute off the execution stack, popping
    /// exhausted frames and iterating loop frames as needed.
    #[inline(always)]
    fn next_item(&mut self) -> Result<Option<Item>, PsError> {
        // Frame inspection and stack mutation can't overlap borrows, so
        // each pass decides on an action first, then applies it.
        enum Action {
            Yield(Item),
            Pop,
            PopThenPush(Object),
            PopThenPush2(Object, Object),
            Iterate(PsArray),
            IterateWith(Object, PsArray),
            IterateWith2(Object, Object, PsArray),
            /// Show frame: push operands, then execute the target with
            /// procedure-call semantics (BuildChar, kshow proc).
            ExecWith(Vec<Object>, Object),
            /// Show frame did synchronous work; go around again.
            Nothing,
            /// An eexec source finished: it carried a systemdict push
            /// that ends with it.
            PopScannerAndDict,
            /// A binary object sequence was scanned (raw bytes): decode it
            /// and run its top-level objects in order, as if each had been
            /// a token (#733).
            RunSequence(Vec<u8>),
            /// A `run` file's scanner is exhausted (#1043): pop it and give
            /// the error location back to the source that ran it.
            PopRunScanner,
            /// A profiled call returned (`--profile`).
            ProfExit,
        }
        // Iterations of an empty body yield nothing, so they run here, in a
        // row, inside one step. After `EMPTY_RUN` of them the call returns
        // `Item::Proc` with no procedure on top, which the step loops take
        // as a step that did nothing; so `step_n` always gets back control
        // and every slice is bounded (#900).
        let mut empty_run = 0u32;
        loop {
            // A Type 3 glyph's end restores its graphics state, which can
            // bring an older page device back (#909).
            let mut stepped_show = false;
            let action = {
                // Split borrow: the scanner needs the dictionary stack
                // (for //immediate names), the show frame the graphics
                // state, while the frame itself is borrowed.
                let Interp {
                    estack,
                    dstack,
                    gfx,
                    ostack,
                    object_format,
                    ..
                } = self;
                let Some(frame) = estack.last_mut() else {
                    return Ok(None);
                };
                match frame {
                    Frame::Scanner(lexer) => match lexer.next_token_in(*object_format != 0)? {
                        None => {
                            if lexer.pop_systemdict {
                                Action::PopScannerAndDict
                            } else if lexer.run_name().is_some() {
                                Action::PopRunScanner
                            } else {
                                Action::Pop
                            }
                        }
                        Some(Token::RBrace) => {
                            return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
                        }
                        Some(Token::LBrace) => {
                            Action::Yield(Item::Obj(scan_procedure(lexer, 0, dstack)?))
                        }
                        Some(Token::ImmediateName(n)) => {
                            Action::Yield(Item::Obj(resolve_immediate(&n, dstack)?))
                        }
                        Some(Token::BinarySeq(bytes)) => Action::RunSequence(bytes),
                        Some(tok) => Action::Yield(Item::Obj(token_to_object(tok))),
                    },
                    // The step loop runs procedure elements itself
                    // (`proc_step`, #863). An exhausted frame is popped
                    // as it yields its last element, so this `Pop` is
                    // only a safeguard.
                    Frame::Proc { body, pc } => {
                        if *pc < body.len() {
                            return Ok(Some(Item::Proc));
                        }
                        Action::Pop
                    }
                    Frame::Repeat { body, remaining } => {
                        if *remaining <= 0 {
                            Action::Pop
                        } else {
                            *remaining -= 1;
                            Action::Iterate(body.clone())
                        }
                    }
                    Frame::Loop { body } => Action::Iterate(body.clone()),
                    Frame::PathForall { elems, idx, procs } => match elems.get(*idx) {
                        None => Action::Pop,
                        Some(el) => {
                            let (coords, proc) = match el {
                                PathForallEl::Move(x, y) => {
                                    (vec![Object::real(*x), Object::real(*y)], procs[0].clone())
                                }
                                PathForallEl::Line(x, y) => {
                                    (vec![Object::real(*x), Object::real(*y)], procs[1].clone())
                                }
                                PathForallEl::Curve(c) => (
                                    c.iter().map(|v| Object::real(*v)).collect(),
                                    procs[2].clone(),
                                ),
                                PathForallEl::Close => (Vec::new(), procs[3].clone()),
                            };
                            *idx += 1;
                            Action::ExecWith(coords, proc)
                        }
                    },
                    Frame::For { body, control } => {
                        if control.finished() {
                            Action::Pop
                        } else {
                            let v = control.value();
                            control.current += control.increment;
                            Action::IterateWith(v, body.clone())
                        }
                    }
                    Frame::Forall { body, src } => match src {
                        ForallSrc::Array(a, i) => match a.get(*i) {
                            None => Action::Pop,
                            Some(el) => {
                                *i += 1;
                                Action::IterateWith(el, body.clone())
                            }
                        },
                        ForallSrc::Str(s, i) => match s.byte(*i) {
                            None => Action::Pop,
                            Some(b) => {
                                *i += 1;
                                Action::IterateWith(Object::int(i64::from(b)), body.clone())
                            }
                        },
                        ForallSrc::Pairs(pairs, i) => match pairs.get(*i) {
                            None => Action::Pop,
                            Some((k, v)) => {
                                let (k, v) = (k.clone(), v.clone());
                                *i += 1;
                                Action::IterateWith2(k, v, body.clone())
                            }
                        },
                    },
                    // Reached from above means the stopped procedure ran
                    // to completion without stopping.
                    Frame::StopMark => Action::PopThenPush(Object::bool(false)),
                    Frame::ProfReturn => Action::ProfExit,
                    Frame::Show(ctx) => {
                        stepped_show = true;
                        let step = ctx.step(gfx)?;
                        match step {
                            ShowStep::Pop => Action::Pop,
                            ShowStep::PopPushWidth(wx, wy) => {
                                Action::PopThenPush2(Object::real(wx), Object::real(wy))
                            }
                            ShowStep::Exec { operands, target } => {
                                Action::ExecWith(operands, target)
                            }
                            ShowStep::Again => Action::Nothing,
                        }
                    }
                    Frame::PostOp(op) => {
                        match op {
                            PostOp::SeparationColor { alt_ncomp } => {
                                let n = *alt_ncomp as usize;
                                let mut comps = [0f64; 4];
                                for slot in comps[..n].iter_mut().rev() {
                                    let o = ostack.pop().ok_or(PsError::StackUnderflow)?;
                                    *slot = match o.value {
                                        Value::Integer(i) => i as f64,
                                        Value::Real(r) => r,
                                        _ => return Err(PsError::Typecheck),
                                    };
                                }
                                match n {
                                    1 => gfx.set_rgb(comps[0], comps[0], comps[0]),
                                    3 => gfx.set_rgb(comps[0], comps[1], comps[2]),
                                    _ => gfx.set_cmyk(comps[0], comps[1], comps[2], comps[3]),
                                }
                            }
                        }
                        Action::Pop
                    }
                    Frame::Image(ctx) => {
                        if ctx.waiting() {
                            // The data-source procedure's result.
                            let s = ostack.pop().ok_or(PsError::StackUnderflow)?;
                            ctx.supply(s)?;
                        }
                        match ctx.step(gfx)? {
                            ImageStep::Exec(p) => Action::ExecWith(Vec::new(), p),
                            ImageStep::Done => Action::Pop,
                        }
                    }
                }
            };
            if stepped_show {
                self.after_device_change();
                self.device_restore_result()?;
            }
            match action {
                Action::Yield(o) => {
                    self.sync_scan_line();
                    return Ok(Some(o));
                }
                Action::Pop => {
                    self.estack.pop();
                }
                Action::PopThenPush(o) => {
                    self.estack.pop();
                    self.push(o);
                }
                Action::PopThenPush2(a, b) => {
                    self.estack.pop();
                    self.push(a);
                    self.push(b);
                }
                Action::Iterate(body) => {
                    self.tick()?;
                    if body.is_empty() {
                        empty_run += 1;
                        if empty_run == EMPTY_RUN {
                            return Ok(Some(Item::Proc));
                        }
                        continue;
                    }
                    self.push_proc_frame(body)?;
                }
                Action::IterateWith(v, body) => {
                    self.tick()?;
                    self.push(v);
                    if body.is_empty() {
                        empty_run += 1;
                        if empty_run == EMPTY_RUN {
                            return Ok(Some(Item::Proc));
                        }
                        continue;
                    }
                    self.push_proc_frame(body)?;
                }
                Action::IterateWith2(a, b, body) => {
                    self.tick()?;
                    self.push(a);
                    self.push(b);
                    if body.is_empty() {
                        empty_run += 1;
                        if empty_run == EMPTY_RUN {
                            return Ok(Some(Item::Proc));
                        }
                        continue;
                    }
                    self.push_proc_frame(body)?;
                }
                Action::ExecWith(operands, target) => {
                    for o in operands {
                        self.push(o);
                    }
                    let empty = target.executable
                        && matches!(&target.value, Value::Array(a) if a.is_empty());
                    self.exec_object(target)?;
                    // An empty procedure yields nothing, like an empty loop
                    // body: count it toward the same run (#903).
                    if empty {
                        empty_run += 1;
                        if empty_run == EMPTY_RUN {
                            return Ok(Some(Item::Proc));
                        }
                    }
                }
                // A show frame that painted a glyph itself (#903).
                Action::Nothing => {
                    empty_run += 1;
                    if empty_run == EMPTY_RUN {
                        return Ok(Some(Item::Proc));
                    }
                }
                Action::PopScannerAndDict => {
                    if let Some(frame) = self.estack.pop() {
                        self.cleanup_unwound_frame(frame);
                    }
                }
                Action::RunSequence(bytes) => {
                    // The scanner is still the top frame: errors from running
                    // the sequence belong to its line, as for a plain token.
                    self.sync_scan_line();
                    let items = self.decode_sequence(&bytes)?;
                    // A scanned executable string runs, as a token read from
                    // a file would; a procedure body only pushes it. So each
                    // one is followed by `exec` (the operator itself, not a
                    // lookup a program could have redefined).
                    let mut run = Vec::with_capacity(items.len());
                    for o in items {
                        let exec_string = o.executable && matches!(o.value, Value::String(_));
                        run.push(o);
                        if exec_string {
                            run.push(Object::exec(Value::Operator(crate::object::Operator {
                                name: "exec",
                                func: crate::ops::control::exec,
                            })));
                        }
                    }
                    self.push_proc_frame(PsArray::from_vec(run))?;
                }
                Action::PopRunScanner => {
                    self.estack.pop();
                    self.restore_scan_location();
                }
                Action::ProfExit => {
                    self.estack.pop();
                    self.prof_exit();
                }
            }
        }
    }

    /// Execute one object encountered in a token stream or procedure body.
    /// Only executable names and operators actually execute here; an
    /// executable array in this position is *pushed* (that's what makes
    /// `{...}` a deferred procedure), as is everything literal.
    #[inline(always)]
    fn execute_element(&mut self, obj: Object) -> Result<(), PsError> {
        match (&obj.value, obj.executable) {
            (Value::Name(n), true) => self.execute_name(n.id()),
            (Value::Operator(op), true) => (op.func)(self),
            _ => {
                self.push(obj);
                Ok(())
            }
        }
    }

    /// Run procedure `body` as a call of `id`: plant a [`Frame::ProfReturn`]
    /// below its frame and open the call in the profiler.
    ///
    /// A procedure whose last element calls another (or itself) drops its
    /// frame first, so a chain of tail calls leaves a run of markers on top
    /// of the exec stack, one per procedure in tail position. A call to a
    /// name already in that run counts but plants no new marker, which keeps
    /// tail recursion at constant depth, as it is without `--profile`. (The
    /// time then belongs to the innermost open call, the last procedure of
    /// the chain to plant a marker: with mutual tail recursion `ping` <->
    /// `pong`, the body of both counts as `pong`'s exclusive time.)
    ///
    /// Each open call holds two exec-stack slots (marker and body frame),
    /// so non-tail recursion runs out at about half the depth it does in a
    /// plain run (`execstackoverflow`, about 5,000 levels instead of 10,000).
    #[cold]
    #[inline(never)]
    fn prof_call(&mut self, id: u32, body: PsArray) -> Result<(), PsError> {
        let run = self
            .estack
            .iter()
            .rev()
            .take_while(|f| matches!(f, Frame::ProfReturn))
            .count();
        let Some(p) = self.profile.as_mut() else {
            return self.push_proc_frame(body);
        };
        if run > 0 && p.tail_has(run, id) {
            p.count_only(id);
            return self.push_proc_frame(body);
        }
        self.push_frame(Frame::ProfReturn)?;
        if let Some(p) = self.profile.as_mut() {
            p.enter(id);
        }
        let pushed = self.push_proc_frame(body);
        if pushed.is_err() {
            self.estack.pop();
            if let Some(p) = self.profile.as_mut() {
                p.abort_enter();
            }
        }
        pushed
    }

    fn prof_exit(&mut self) {
        if let Some(p) = self.profile.as_mut() {
            p.exit();
        }
    }

    /// Turn on the per-procedure profile (`pscat --profile`, issue #737)
    /// for the programs this interpreter runs from now on. Call it before
    /// the program starts: a call already running is not counted.
    #[doc(hidden)]
    pub fn enable_profile(&mut self) {
        self.profile = Some(Box::new(crate::profile::Profiler::new()));
    }

    /// Also record who called whom (`--profile-graph`); call it right
    /// after [`enable_profile`](Self::enable_profile).
    #[doc(hidden)]
    pub fn enable_profile_graph(&mut self) {
        if let Some(p) = self.profile.as_mut() {
            p.set_graph();
        }
    }

    /// Also time each built-in operator (`--profile-ops`); call it right
    /// after [`enable_profile`](Self::enable_profile). Slows the run more
    /// than `--profile` alone: two clock reads per operator. Rows are
    /// keyed by `Operator::name`, as operator identity is (#881, #885).
    #[doc(hidden)]
    pub fn enable_profile_ops(&mut self) {
        if let Some(p) = self.profile.as_mut() {
            p.set_ops();
        }
    }

    /// What the profile has recorded so far (finished calls only), or
    /// `None` if [`enable_profile`](Self::enable_profile) was not called.
    #[doc(hidden)]
    pub fn profile_report(&self) -> Option<crate::profile::ProfileReport> {
        self.profile.as_ref().map(|p| p.report())
    }

    /// Execute the executable name with interned id `id`: look it up on
    /// the dictionary stack and run what it resolves to. Numbers come out
    /// of the dictionary copied inline (#863).
    #[inline]
    fn execute_name(&mut self, id: u32) -> Result<(), PsError> {
        self.last_name = Some(id);
        let resolved = self
            .dstack
            .iter()
            .rev()
            .find_map(|d| d.borrow().get_id_ref(id).map(Object::clone_inline))
            .ok_or_else(|| undefined(id))?;
        self.execute_resolved(resolved, id)
    }

    /// Execute the object a name resolved to. Unlike `execute_element`,
    /// an executable array here *runs* — this is a procedure call.
    ///
    /// `via` is the name the object was reached through, or [`NO_NAME`]
    /// (`exec`, a conditional's branch): `--profile` attributes a
    /// procedure body to the name it ran through (#737).
    #[inline(always)]
    fn execute_resolved(&mut self, mut obj: Object, via: u32) -> Result<(), PsError> {
        // A name may resolve to another executable name; follow the chain
        // with a budget so `a -> b -> a` cycles error out instead of
        // spinning forever.
        let mut hops = 0;
        loop {
            if !obj.executable {
                self.push(obj);
                return Ok(());
            }
            match &obj.value {
                Value::Operator(op) => return (op.func)(self),
                Value::Array(_) => {
                    // Move the body out rather than cloning it (and then
                    // dropping `obj`): one refcount round trip fewer.
                    let Value::Array(body) = obj.value else {
                        unreachable!()
                    };
                    if via != NO_NAME && self.profile.is_some() {
                        return self.prof_call(via, body);
                    }
                    return self.push_proc_frame(body);
                }
                // An executable string runs as source — the `(...) cvx
                // exec` idiom found code uses constantly.
                Value::String(s) => {
                    return self.push_frame(Frame::Scanner(Lexer::new(s.to_vec())));
                }
                // An executable file runs in place, sharing its position
                // with everything else holding the handle.
                Value::File(f) => {
                    return self.push_frame(Frame::Scanner(Lexer::from_file(f.clone())));
                }
                Value::Name(n) => {
                    hops += 1;
                    if hops > 100 {
                        return Err(PsError::Limitcheck);
                    }
                    let id = n.id();
                    self.last_name = Some(id);
                    let next = self
                        .load_id(id)
                        .ok_or_else(|| PsError::Undefined(n.to_string()))?;
                    obj = next;
                }
                _ => {
                    self.push(obj);
                    return Ok(());
                }
            }
        }
    }

    #[inline(always)]
    fn push_proc_frame(&mut self, body: PsArray) -> Result<(), PsError> {
        if body.is_empty() {
            return Ok(());
        }
        if self.estack.len() >= EXEC_STACK_LIMIT {
            return Err(PsError::ExecStackOverflow);
        }
        self.estack.push(Frame::Proc { body, pc: 0 });
        Ok(())
    }

    /// Park `post` as a continuation, put `operands` on the operand
    /// stack, and run `proc` above it — the pattern for any operator
    /// that needs a PostScript procedure's result (never recurse).
    pub(crate) fn begin_postop(
        &mut self,
        post: PostOp,
        operands: Vec<Object>,
        proc: &Object,
    ) -> Result<(), PsError> {
        let Value::Array(body) = &proc.value else {
            return Err(PsError::Typecheck);
        };
        if !proc.executable {
            return Err(PsError::Typecheck);
        }
        self.push_frame(Frame::PostOp(post))?;
        for o in operands {
            self.push(o);
        }
        self.push_proc_frame(body.clone())
    }

    fn push_frame(&mut self, frame: Frame) -> Result<(), PsError> {
        if self.estack.len() >= EXEC_STACK_LIMIT {
            return Err(PsError::ExecStackOverflow);
        }
        self.estack.push(frame);
        Ok(())
    }

    /// `exec` and the conditional operators: run an object with full
    /// executable semantics (procedures run rather than being pushed).
    pub(crate) fn exec_object(&mut self, obj: Object) -> Result<(), PsError> {
        self.execute_resolved(obj, NO_NAME)
    }

    /// Run `file` as program source loaded by `run` under `name`, so an
    /// error in it reports its own file and line (issue #1043).
    pub(crate) fn run_file(
        &mut self,
        file: crate::file::FileHandle,
        name: Rc<str>,
    ) -> Result<(), PsError> {
        self.push_frame(Frame::Scanner(Lexer::run_file(file, name)))
    }

    /// The `token` operator on a file: one token scanned in place — the
    /// file's shared position advances exactly past it.
    pub(crate) fn scan_token_from_file(
        &self,
        file: crate::file::FileHandle,
    ) -> Result<Option<Object>, PsError> {
        let mut lexer = Lexer::from_file(file);
        Ok(self.scan_one(&mut lexer)?.map(|(obj, _)| obj))
    }

    /// The `token` operator: scan one token from raw bytes, returning the
    /// object and how many bytes the scanner consumed.
    pub(crate) fn scan_token_from(
        &self,
        bytes: Vec<u8>,
    ) -> Result<Option<(Object, usize)>, PsError> {
        let mut lexer = Lexer::new(bytes);
        self.scan_one(&mut lexer)
    }

    /// The top-level objects of a scanned binary object sequence (#733).
    /// A sequence decodes to at most one object per 8 bytes, so the memory
    /// limit is asked about that much before anything is built.
    fn decode_sequence(&self, bytes: &[u8]) -> Result<Vec<Object>, PsError> {
        self.check_alloc(bytes.len() / 8 * std::mem::size_of::<Object>())?;
        decode_sequence(bytes, &self.dstack)
    }

    fn scan_one(&self, lexer: &mut Lexer) -> Result<Option<(Object, usize)>, PsError> {
        lexer.binary_tokens = self.object_format != 0;
        let obj = match lexer.next_token()? {
            None => return Ok(None),
            Some(Token::RBrace) => {
                return Err(PsError::Syntax("'}' with no matching '{'".to_string()));
            }
            Some(Token::LBrace) => scan_procedure(lexer, 0, &self.dstack)?,
            Some(Token::ImmediateName(n)) => resolve_immediate(&n, &self.dstack)?,
            Some(Token::BinarySeq(bytes)) => Object::procedure(self.decode_sequence(&bytes)?),
            Some(tok) => token_to_object(tok),
        };
        Ok(Some((obj, lexer.pos())))
    }

    pub(crate) fn begin_repeat(&mut self, body: PsArray, count: i64) -> Result<(), PsError> {
        self.push_frame(Frame::Repeat {
            body,
            remaining: count,
        })
    }

    pub(crate) fn begin_loop(&mut self, body: PsArray) -> Result<(), PsError> {
        self.push_frame(Frame::Loop { body })
    }

    pub(crate) fn begin_for(&mut self, body: PsArray, control: ForControl) -> Result<(), PsError> {
        self.push_frame(Frame::For { body, control })
    }

    pub(crate) fn begin_pathforall(
        &mut self,
        elems: Vec<PathForallEl>,
        procs: [Object; 4],
    ) -> Result<(), PsError> {
        self.push_frame(Frame::PathForall {
            elems,
            idx: 0,
            procs: Box::new(procs),
        })
    }

    pub(crate) fn begin_forall(&mut self, body: PsArray, src: ForallSrc) -> Result<(), PsError> {
        self.push_frame(Frame::Forall { body, src })
    }

    /// Plant the boundary for a `stopped` context.
    pub(crate) fn begin_stopped(&mut self) -> Result<(), PsError> {
        self.push_frame(Frame::StopMark)
    }

    /// Queue a show (any of the family) as an execution-stack frame.
    pub(crate) fn begin_show(&mut self, ctx: ShowCtx) -> Result<(), PsError> {
        self.push_frame(Frame::Show(Box::new(ctx)))
    }

    /// Queue an image (image/imagemask/colorimage) frame.
    pub(crate) fn begin_image(&mut self, ctx: ImageCtx) -> Result<(), PsError> {
        // The SVG and PDF recorders decode the whole image into a buffer of
        // their own, width x height pixels whatever data the source gives
        // (`30000 30000 8 [...] {()} image` is 3.6 GB and 9e8 pixels in one
        // step, #1176), so that buffer is refused against a memory cap
        // before the image starts, as `array` and `string` are.
        let bytes = ctx.recorder_bytes(self.gfx.svg_wanted(), self.gfx.pdf_wanted());
        if bytes > 0 {
            self.check_alloc(bytes)?;
        }
        self.push_frame(Frame::Image(Box::new(ctx)))
    }

    /// `currentfile`: the topmost *file* being executed. Executable
    /// strings scan through the same machinery but aren't files, per
    /// the PLRM, so they're skipped.
    pub(crate) fn current_file(&self) -> Option<crate::file::FileHandle> {
        self.estack.iter().rev().find_map(|f| match f {
            Frame::Scanner(lx) => lx.file().cloned(),
            _ => None,
        })
    }

    /// `eexec`: run a decrypting view of `file` as program source, with
    /// systemdict pushed for its duration (Type 1 fonts assume it).
    pub(crate) fn begin_eexec(&mut self, file: crate::file::FileHandle) -> Result<(), PsError> {
        let eexec_file = crate::file::PsFile::filtered_limited(
            file,
            crate::file::Decoder::eexec(),
            self.decode_budget(),
        );
        let mut lexer = Lexer::from_file(eexec_file);
        lexer.pop_systemdict = true;
        self.push_dict(self.dstack[0].clone());
        self.push_frame(Frame::Scanner(lexer))
    }

    /// setcachedevice/setcharwidth: hand the width to the innermost show
    /// whose glyph procedure is actually running. False if there is none
    /// — the operators are meaningless outside BuildChar.
    pub(crate) fn set_type3_glyph_width(&mut self, w: (f64, f64)) -> bool {
        for frame in self.estack.iter_mut().rev() {
            if let Frame::Show(ctx) = frame
                && ctx.has_pending()
            {
                ctx.set_glyph_width(w);
                return true;
            }
        }
        false
    }

    /// Unwind to (and including) the nearest StopMark. Returns whether a
    /// context existed; with none, the whole program is aborted, which is
    /// what the PLRM's outermost job-server `stopped` would do.
    pub(crate) fn do_stop(&mut self) -> bool {
        let mut left_run_file = false;
        while let Some(frame) = self.estack.pop() {
            if matches!(frame, Frame::StopMark) {
                // A `stopped` that abandoned a `run` file resumes in the
                // source under it, which scans no token to say so (#1043).
                if left_run_file {
                    self.restore_scan_location();
                }
                return true;
            }
            if let Frame::Scanner(lexer) = &frame
                && lexer.run_name().is_some()
            {
                left_run_file = true;
            }
            self.cleanup_unwound_frame(frame);
        }
        false
    }

    /// `exit`: unwind to (and including) the innermost loop frame.
    /// Stops at a source boundary, a `stopped` boundary, or a show in
    /// progress (a BuildChar/kshow proc can't `exit` the show) — per the
    /// PLRM's `invalidexit`.
    pub(crate) fn exit_loop(&mut self) -> Result<(), PsError> {
        loop {
            match self.estack.last() {
                None
                | Some(
                    Frame::Scanner(_)
                    | Frame::StopMark
                    | Frame::Show(_)
                    | Frame::Image(_)
                    | Frame::PostOp(_),
                ) => {
                    return Err(PsError::InvalidExit);
                }
                Some(
                    Frame::Repeat { .. }
                    | Frame::Loop { .. }
                    | Frame::For { .. }
                    | Frame::Forall { .. }
                    | Frame::PathForall { .. },
                ) => {
                    self.estack.pop();
                    return Ok(());
                }
                Some(Frame::Proc { .. }) => {
                    self.estack.pop();
                }
                // A profiled call `exit` leaves: close it.
                Some(Frame::ProfReturn) => {
                    self.estack.pop();
                    self.prof_exit();
                }
            }
        }
    }

    /// The instance dict for a non-Font resource category, created on
    /// first touch.
    pub(crate) fn resource_category(&mut self, category: &Rc<str>) -> Rc<RefCell<Dict>> {
        self.resources
            .entry(category.clone())
            .or_insert_with(|| Rc::new(RefCell::new(Dict::new())))
            .clone()
    }

    // --- dictionary-stack access for the dict operators -----------------

    pub(crate) fn current_dict(&self) -> Rc<RefCell<Dict>> {
        // The dict stack never drops below systemdict/userdict.
        self.dstack
            .last()
            .cloned()
            .unwrap_or_else(|| unreachable!("dict stack is never empty"))
    }

    #[doc(hidden)]
    pub fn dict_stack_len(&self) -> usize {
        self.dstack.len()
    }

    pub(crate) fn clear_dict_stack(&mut self) {
        self.dstack.truncate(2);
    }

    /// `where`: the topmost dictionary defining `key`, if any.
    pub(crate) fn find_defining_dict(
        &self,
        key: &Object,
    ) -> Result<Option<Rc<RefCell<Dict>>>, PsError> {
        for d in self.dstack.iter().rev() {
            if d.borrow().known(key)? {
                return Ok(Some(d.clone()));
            }
        }
        Ok(None)
    }

    pub(crate) fn push_dict(&mut self, dict: Rc<RefCell<Dict>>) {
        self.dstack.push(dict);
    }

    /// `end`: systemdict and userdict are permanent, per the PLRM.
    pub(crate) fn pop_dict(&mut self) -> Result<(), PsError> {
        if self.dstack.len() <= 2 {
            return Err(PsError::DictStackUnderflow);
        }
        self.dstack.pop();
        Ok(())
    }

    /// Look a name up through the dictionary stack, top down.
    #[doc(hidden)]
    pub fn load(&self, name: &str) -> Option<Object> {
        self.dstack.iter().rev().find_map(|d| d.borrow().get(name))
    }

    /// The interned fast path for the same walk — what name execution
    /// uses (one integer hash per dictionary probed, no Rc traffic).
    #[inline]
    pub(crate) fn load_id(&self, id: u32) -> Option<Object> {
        self.dstack.iter().rev().find_map(|d| d.borrow().get_id(id))
    }

    /// Define a name in the topmost dictionary (what `def` will do once
    /// control flow lands; already used by tests and available to embedders).
    #[doc(hidden)]
    pub fn define(&mut self, name: &str, obj: Object) {
        if let Some(top) = self.dstack.last().cloned() {
            self.journal_dict(&top);
            top.borrow_mut().put(name.into(), obj);
        }
    }

    /// [`define`](Self::define) for a name that is already interned —
    /// what `def` uses. Going through `&str` would allocate an
    /// `Rc<str>` and re-intern it (a SipHash probe and a free) on every
    /// `def`, which profiled at ~15% of a bound drykit run (#323).
    pub(crate) fn define_name(&mut self, name: &crate::name::PsName, obj: Object) {
        // With no save open there is nothing to journal, so the top dict
        // is borrowed where it is, without an `Rc` clone (#863).
        if self.save_stack.is_empty() {
            if let Some(top) = self.dstack.last() {
                top.borrow_mut().put_name(name, obj);
            }
            return;
        }
        if let Some(top) = self.dstack.last().cloned() {
            self.journal_dict(&top);
            top.borrow_mut().put_name(name, obj);
        }
    }

    /// `def` for any non-name key (integer, real, boolean, string, ...):
    /// the general half of [`define_name`](Self::define_name), kept out of
    /// line so the name-key hot path stays small. `Dict::put_obj` applies
    /// the PLRM key rules (whole reals become integers, strings become
    /// names, null is a typecheck).
    #[inline(never)]
    pub(crate) fn define_key(&mut self, key: &Object, obj: Object) -> Result<(), PsError> {
        if let Some(top) = self.dstack.last().cloned() {
            self.journal_dict(&top);
            top.borrow_mut().put_obj(key, obj)?;
        }
        Ok(())
    }

    /// `load` for a non-name key: the same top-down walk as
    /// [`load_id`](Self::load_id), through the general key lookup.
    #[inline(never)]
    pub(crate) fn load_key(&self, key: &Object) -> Result<Option<Object>, PsError> {
        for d in self.dstack.iter().rev() {
            if let Some(obj) = d.borrow().get_obj(key)? {
                return Ok(Some(obj));
            }
        }
        Ok(None)
    }

    // --- save/restore (design: docs/dev/VM.md) -----------------------------------

    /// Copy-on-write barrier: call *before* mutating an array's contents.
    /// First touch at the current save level snapshots the whole backing
    /// store; later touches are one hash lookup. No-op with no live save.
    pub(crate) fn journal_array(&mut self, a: &PsArray) {
        let Some(rec) = self.save_stack.last_mut() else {
            return;
        };
        let data = a.data_rc();
        if rec.seen.insert(Rc::as_ptr(data) as usize) {
            self.journal.push(JEntry::Array {
                data: data.clone(),
                old: data.borrow().clone(),
            });
        }
    }

    /// The dict counterpart of [`journal_array`].
    pub(crate) fn journal_dict(&mut self, d: &Rc<RefCell<Dict>>) {
        let Some(rec) = self.save_stack.last_mut() else {
            return;
        };
        if rec.seen.insert(Rc::as_ptr(d) as usize) {
            self.journal.push(JEntry::Dict {
                dict: d.clone(),
                old: d.borrow().clone(),
            });
        }
    }

    /// The `gstate` counterpart of [`journal_array`]: `currentgstate`
    /// and `copy` overwrite a gstate object's contents, which `restore`
    /// rolls back, as gs does.
    pub(crate) fn journal_gstate(&mut self, g: &Rc<RefCell<crate::gfx::GraphicsState>>) {
        let Some(rec) = self.save_stack.last_mut() else {
            return;
        };
        if rec.seen.insert(Rc::as_ptr(g) as usize) {
            self.journal.push(JEntry::GState {
                gs: g.clone(),
                old: Box::new(g.borrow().clone()),
            });
        }
    }

    pub(crate) fn do_save(&mut self) -> Object {
        let (gfx_depth, gfx_state) = self.gfx.glyph_snapshot();
        self.gfx.clear_clip_stack();
        let handle = Rc::new(SaveHandle {
            valid: Cell::new(true),
        });
        self.save_stack.push(SaveRecord {
            handle: handle.clone(),
            journal_mark: self.journal.len(),
            packing: self.packing,
            seen: HashSet::new(),
            gfx_depth,
            gfx_state,
            object_format: self.object_format,
        });
        Object::lit(Value::Save(handle))
    }

    pub(crate) fn do_restore(&mut self, obj: &Object) -> Result<(), PsError> {
        let Value::Save(h) = &obj.value else {
            return Err(PsError::Typecheck);
        };
        if !h.valid.get() {
            return Err(PsError::InvalidRestore);
        }
        let idx = self
            .save_stack
            .iter()
            .position(|r| Rc::ptr_eq(&r.handle, h))
            .ok_or(PsError::InvalidRestore)?;
        // Restoring this save discards any newer ones; their objects go
        // stale (a later restore on them is invalidrestore, per PLRM).
        for rec in &self.save_stack[idx..] {
            rec.handle.valid.set(false);
        }
        self.save_stack.truncate(idx + 1);
        let rec = self.save_stack.pop().expect("record at idx exists");
        self.packing = rec.packing;
        while self.journal.len() > rec.journal_mark {
            match self.journal.pop().expect("length checked") {
                // Replacing contents while holding the borrow is safe
                // even for self-referential structures: the displaced
                // objects' Drop skips anything still multiply-owned,
                // and this cell is kept alive by `data`/`dict` itself.
                JEntry::Array { data, old } => *data.borrow_mut() = old,
                JEntry::Dict { dict, old } => *dict.borrow_mut() = old,
                JEntry::GState { gs, old } => *gs.borrow_mut() = *old,
            }
        }
        self.object_format = rec.object_format;
        // grestoreall to the save point, then drop the saved state —
        // pinned against gs (`save gsave gsave 5 setlinewidth restore`).
        self.gfx
            .restore_glyph_snapshot(rec.gfx_depth, rec.gfx_state);
        self.after_device_change();
        self.device_restore_result()
    }

    /// `vmstatus`'s save-nesting level.
    pub(crate) fn save_level(&self) -> usize {
        self.save_stack.len()
    }

    /// `grestoreall`: pop to the innermost save's boundary (keeping the
    /// boundary state available for its restore), or to the bottom.
    pub(crate) fn do_grestoreall(&mut self) {
        match self.save_stack.last() {
            Some(rec) => {
                let state = rec.gfx_state.clone();
                let depth = rec.gfx_depth;
                self.gfx.restore_glyph_snapshot(depth, state);
            }
            None => self.gfx.grestore_all_bottom(),
        }
        // gs's grestoreall ends in a fresh gsave level, so no clipsave
        // entries survive it (#494).
        self.gfx.clear_clip_stack();
        self.after_device_change();
    }

    /// `grestore` (#505): at the innermost save's boundary it restores
    /// the save's state without popping; see [`Gfx::grestore_at`].
    pub(crate) fn do_grestore(&mut self) {
        let boundary = self
            .save_stack
            .last()
            .map(|rec| (rec.gfx_depth, &*rec.gfx_state));
        self.gfx.grestore_at(boundary);
        self.after_device_change();
    }

    /// After anything that can install a page device (`setpagedevice`, or
    /// a restore that brings an older one back, #909): a `--bands` run
    /// redefines `/BandBBox` for the new page, so a band-aware program
    /// reads the area its band keeps of the page it now has.
    /// `limitexceeded (memory)` when a restore could not bring its page
    /// device back within `--max-memory` (#909): raised by the operator
    /// that restored, before the program goes on.
    pub(crate) fn device_restore_result(&self) -> Result<(), PsError> {
        if self.gfx.memory_refused() {
            Err(PsError::MemoryExceeded)
        } else {
            Ok(())
        }
    }

    pub(crate) fn after_device_change(&mut self) {
        if !self.gfx.take_device_changed() {
            return;
        }
        let Some((index, count)) = self.gfx.band() else {
            return;
        };
        let (w, h) = (self.gfx.pixmap.width(), self.gfx.pixmap.height());
        let (top, bottom) = crate::gfx::band_rows_of(index, count, h);
        let scale = self.gfx.scale();
        let (llx, lly) = self.gfx.page_origin();
        // The same f32 arithmetic, and the same numbers once printed and
        // scanned, as the `/BandBBox` `pscat --bands` defines first.
        let user_y = |row: u32| lly + (h - row) as f32 / scale;
        let number = |v: f32| {
            let text = format!("{v}");
            match text.parse::<i64>() {
                Ok(i) => Object::int(i),
                Err(_) => Object::real(text.parse().unwrap_or(f64::from(v))),
            }
        };
        let bbox = Object::array(vec![
            number(llx),
            number(user_y(bottom)),
            number(llx + w as f32 / scale),
            number(user_y(top)),
        ]);
        let user = self.dstack[1].clone();
        self.journal_dict(&user);
        // Detached, as the band names were first defined (#868).
        let key = crate::name::detached(|| crate::name::intern("BandBBox"));
        user.borrow_mut().put_name(&key, bbox);
    }

    /// `cliprestore` (#494): this needs the innermost save's snapshot,
    /// the clip a level opened by `save` started with.
    pub(crate) fn do_cliprestore(&mut self) {
        let boundary = self
            .save_stack
            .last()
            .map(|rec| (rec.gfx_depth, &*rec.gfx_state));
        self.gfx.cliprestore(boundary);
    }

    #[doc(hidden)]
    pub fn operand_stack(&self) -> &[Object] {
        &self.ostack
    }

    /// Self-check/lint mode (issue #17): heuristic checks for common
    /// silent-failure mistakes — see `crate::lint` for what's checked
    /// and why `render_checks` exists.
    #[doc(hidden)]
    pub fn lint(&self, render_checks: bool) -> Vec<crate::lint::LintFinding> {
        crate::lint::check(self, render_checks)
    }

    /// [`lint`](Self::lint) plus the program's own `%%Pages:`
    /// declaration (issue #95), which
    /// `crate::lint::scan_declared_pages` reads from the source. A
    /// separate method rather than an extra argument, so `lint`'s
    /// existing public signature keeps working.
    #[doc(hidden)]
    pub fn lint_with_pages(
        &self,
        render_checks: bool,
        declared_pages: &crate::lint::DeclaredPages,
    ) -> Vec<crate::lint::LintFinding> {
        crate::lint::check_with_pages(self, render_checks, declared_pages)
    }

    /// Whether the program ran `quit`. [`run_str`](Self::run_str) returns
    /// `Ok` either way; this tells the two apart.
    pub fn quit_requested(&self) -> bool {
        self.quit_requested
    }

    /// Force every `srand` call to reseed with `seed` instead of its
    /// operand (issue #21's `--sweep-seed`), except one that restores a
    /// value saved with `rrand` (issue #202). `None` (the default)
    /// leaves `srand` and `rrand` alone. Also clears the fired flag, so a caller
    /// re-arming this on a fresh `Interp` starts from a clean read of
    /// [`seed_override_fired`](Self::seed_override_fired).
    ///
    /// The seed also builds the noise table (issue #753), as `seed
    /// noiseseed` would, so art built on the native noise operators
    /// varies with the sweep too. A program that calls `noiseseed`
    /// itself keeps its own table. `None` puts the default table back,
    /// but only while the table is still the override's.
    pub fn set_seed_override(&mut self, seed: Option<i64>) {
        self.seed_override = seed;
        self.seed_override_fired = false;
        match seed {
            Some(s) => {
                self.noise_perm = crate::ops::noise::perm_from_seed(s);
                self.noise_from_override = true;
            }
            None if self.noise_from_override => {
                self.noise_perm = crate::ops::noise::perm_from_seed(0);
                self.noise_from_override = false;
            }
            None => {}
        }
    }

    /// Whether [`set_seed_override`](Self::set_seed_override)'s
    /// override has actually taken effect since it was set: it
    /// intercepted a `srand` call, or a noise operator sampled the table
    /// it built (issue #753). `false` means the source never called
    /// `srand` and never sampled override-seeded noise, so the override
    /// had no effect on this run.
    pub fn seed_override_fired(&self) -> bool {
        self.seed_override_fired
    }

    /// Make dropping this `Interp` skip emptying its dictionaries, as it
    /// did before issue #625. For a process that exits right after the run
    /// (the `pscat` CLI): the OS reclaims the memory, and walking a large
    /// graph first only delays the exit. Not for a long-lived process.
    #[doc(hidden)]
    pub fn leave_dicts_to_process_exit(&mut self) {
        self.keep_dicts_on_drop = true;
    }

    #[doc(hidden)]
    pub fn last_executed_name(&self) -> Option<String> {
        self.last_name
            .map(|id| crate::name::resolve(id).to_string())
    }

    #[doc(hidden)]
    pub fn push(&mut self, obj: Object) {
        self.ostack.push(obj);
    }

    #[doc(hidden)]
    pub fn pop(&mut self) -> Result<Object, PsError> {
        self.ostack.pop().ok_or(PsError::StackUnderflow)
    }

    /// The top two operands, if both are numbers (deeper one first). The
    /// arithmetic and comparison operators' fast path (#863): it looks
    /// without popping, so when it says no, the operator's general path
    /// runs on an untouched stack and fails exactly as it always has.
    #[inline(always)]
    pub(crate) fn top2_nums(&self) -> Option<(Num, Num)> {
        let [.., a, b] = self.ostack.as_slice() else {
            return None;
        };
        Some((a.num()?, b.num()?))
    }

    /// Replace the top two operands with the literal `v`. For the fast
    /// paths above, once `top2_nums` has said both are numbers.
    #[inline(always)]
    pub(crate) fn replace_top2(&mut self, v: Value) {
        if let Some(b) = self.ostack.pop() {
            b.drop_inline();
        }
        if let Some(a) = self.ostack.last_mut() {
            std::mem::replace(a, Object::lit(v)).drop_inline();
        }
    }

    #[doc(hidden)]
    pub fn pop_num(&mut self) -> Result<Num, PsError> {
        match self.pop()?.value {
            Value::Integer(i) => Ok(Num::Int(i)),
            Value::Real(r) => Ok(Num::Real(r)),
            _ => Err(PsError::Typecheck),
        }
    }

    #[doc(hidden)]
    pub fn pop_int(&mut self) -> Result<i64, PsError> {
        match self.pop()?.value {
            Value::Integer(i) => Ok(i),
            _ => Err(PsError::Typecheck),
        }
    }

    #[doc(hidden)]
    pub fn pop_f64(&mut self) -> Result<f64, PsError> {
        Ok(self.pop_num()?.to_f64())
    }

    /// Collect what the program prints (`=`, `==`, `stack`, `pstack`,
    /// `print`) in a buffer instead of writing it to stdout, starting
    /// empty. Kept to [`STDOUT_CAPTURE_LIMIT`] bytes plus a note saying
    /// the rest was dropped.
    pub fn capture_stdout(&mut self) {
        self.stdout_capture = Some(Vec::new());
        self.stdout_truncated = false;
    }

    /// Drop what the program writes to stderr from now on: `(%stderr)`
    /// files, `handleerror` and `findfont`'s stand-in notes, which
    /// [`font_warnings`](Self::font_warnings) still collects. For a host
    /// running several copies of one program (`pscat --bands`, #877).
    pub fn quiet_stderr(&mut self) {
        self.quiet_stderr = true;
    }

    /// Program output to stderr, unless [`quiet_stderr`](Self::quiet_stderr)
    /// is on.
    pub(crate) fn write_stderr(&self, bytes: &[u8], flush: bool) -> std::io::Result<()> {
        use std::io::Write;
        if self.quiet_stderr {
            return Ok(());
        }
        let mut err = std::io::stderr();
        err.write_all(bytes)?;
        if flush { err.flush() } else { Ok(()) }
    }

    /// What has been printed since [`Interp::capture_stdout`]; empty when
    /// capture is off.
    pub fn captured_stdout(&self) -> &[u8] {
        self.stdout_capture.as_deref().unwrap_or(&[])
    }

    /// The one-line notes `findfont` raised for pack faces it replaced
    /// with a built-in one, newline-joined, once per name (the lines the
    /// native binary prints on stderr); empty when nothing was replaced.
    pub fn font_warnings(&self) -> &str {
        &self.font_warnings
    }

    /// Program output: to the capture buffer when one is on, else stdout
    /// (flushed when `flush` is set, as `print` does: a prompt with no
    /// newline must still appear).
    pub(crate) fn write_stdout(&mut self, bytes: &[u8], flush: bool) -> std::io::Result<()> {
        use std::io::Write;
        let Some(buf) = self.stdout_capture.as_mut() else {
            let mut out = std::io::stdout();
            let written = out
                .write_all(bytes)
                .and_then(|()| if flush { out.flush() } else { Ok(()) });
            if let Err(e) = &written
                && e.kind() == std::io::ErrorKind::BrokenPipe
            {
                self.stdout_epipe_error = true;
            }
            return written;
        };
        if self.stdout_truncated || bytes.is_empty() {
            return Ok(());
        }
        // Until truncation the buffer never exceeds the limit, so a write
        // that finds it exactly full still adds the note below.
        let room = STDOUT_CAPTURE_LIMIT - buf.len();
        if bytes.len() <= room {
            buf.extend_from_slice(bytes);
        } else {
            buf.extend_from_slice(&bytes[..room]);
            buf.extend_from_slice(STDOUT_TRUNCATED);
            self.stdout_truncated = true;
        }
        Ok(())
    }

    /// The pages the program has produced so far, oldest first: one per
    /// `showpage` or `copypage`, plus the canvas as it stands if anything
    /// was painted after the last `showpage` (or if there was none), as
    /// `pscat --png` writes them.
    pub fn pages(&self) -> Vec<crate::Page> {
        let mut pages: Vec<crate::Page> = self
            .gfx
            .pages()
            .iter()
            .cloned()
            .map(|pixmap| crate::Page::new(pixmap, self.gfx.scale()))
            .collect();
        if pages.is_empty() || self.gfx.has_trailing_art() {
            pages.push(crate::Page::new(self.gfx.pixmap.clone(), self.gfx.scale()));
        }
        pages
    }

    /// Start recording SVG. Call before running the program: only what is
    /// drawn afterwards is recorded. Read the result with
    /// [`svg_pages`](Self::svg_pages).
    pub fn enable_svg(&mut self) {
        self.gfx.enable_svg();
    }

    /// How SVG output writes text set in the standard fonts:
    /// [`SvgText::Text`](crate::SvgText::Text) writes selectable `<text>`
    /// elements instead of glyph outlines (`pscat --svg-text`, #1119).
    /// The default is [`SvgText::Paths`](crate::SvgText::Paths). Call
    /// before running the program; PNG and PDF output are unaffected.
    pub fn set_svg_text(&mut self, mode: crate::SvgText) {
        self.gfx.set_svg_text(mode);
    }

    /// How PDF output writes text: [`PdfText::Text`](crate::PdfText::Text)
    /// writes selectable, searchable text objects in embedded subset fonts
    /// instead of glyph outlines (`pscat --pdf-text`, #1193). The default
    /// is [`PdfText::Paths`](crate::PdfText::Paths). Call before running
    /// the program; PNG and SVG output are unaffected.
    pub fn set_pdf_text(&mut self, mode: crate::PdfText) {
        self.gfx.set_pdf_text(mode);
    }

    /// One SVG document per page (as [`pages`](Self::pages) counts them),
    /// or `None` if [`enable_svg`](Self::enable_svg) was not called.
    pub fn svg_pages(&self) -> Option<Vec<String>> {
        self.gfx.svg_pages()
    }

    /// Start recording PDF. Call before running the program. Read the
    /// result with [`pdf_document`](Self::pdf_document).
    pub fn enable_pdf(&mut self) {
        self.gfx.enable_pdf();
    }

    /// The whole document as one PDF file, a page per page, or `None` if
    /// [`enable_pdf`](Self::enable_pdf) was not called.
    pub fn pdf_document(&self) -> Option<Vec<u8>> {
        self.gfx.pdf_document()
    }

    #[doc(hidden)]
    pub fn gfx(&self) -> &Gfx {
        &self.gfx
    }

    #[doc(hidden)]
    pub fn gfx_mut(&mut self) -> &mut Gfx {
        &mut self.gfx
    }
}

/// systemdict holds a strong `Rc` to itself and to `userdict` (the PLRM's
/// `systemdict begin` / `userdict /x get` idiom), and the crate has no cycle
/// collector, so without this the dictionaries, and everything a program
/// stored in them, would outlive every dropped `Interp` (issue #21 found it
/// for the sweep loop; issue #625 for library users, who build one `Interp`
/// per document). Emptying both permanent dictionaries breaks those
/// references, and also a program's own `userdict /me userdict put`.
///
/// This is a `Drop` on `Interp`, not on `Object`: `Object` must stay
/// `Drop`-free (docs/dev/HANDOFF.md, the `PsArray` note). The cost is one
/// teardown of the dictionary graph when an `Interp` goes out of scope.
///
/// An `Object` that a caller took out of the interpreter (`pop`) and that
/// is systemdict or userdict itself (`systemdict` then `pop`) is emptied by
/// this too: it is the same dictionary. The `pscat` CLI opts out with
/// [`leave_dicts_to_process_exit`](Interp::leave_dicts_to_process_exit).
impl Drop for Interp {
    fn drop(&mut self) {
        // Before the early return below: a file kept alive by the leaked
        // dictionary graph would otherwise never be flushed.
        let _ = self.flush_files();
        if self.keep_dicts_on_drop {
            return;
        }
        // Take each permanent dictionary's contents out first, so the graph
        // is freed after the `RefMut` is released (a destructor can then
        // never find one still borrowed). A deep chain a program built is
        // freed iteratively by `Drop for Dict`, wherever it hangs from (the
        // operand stack, `begin`, the journal, a resource category...).
        // `try_borrow_mut`: a caller that popped `userdict` and still holds
        // a `borrow()` of it must not turn a drop into a panic (a second
        // panic while unwinding aborts the process). That dictionary is
        // simply left alone, as it was before #625.
        for dict in self.dstack.iter().take(2) {
            let contents = dict.try_borrow_mut().map(|mut d| std::mem::take(&mut *d));
            drop(contents);
        }
    }
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

/// The `undefined` error for name `id`, out of the name lookup's way.
#[cold]
#[inline(never)]
fn undefined(id: u32) -> PsError {
    PsError::Undefined(crate::name::resolve(id).to_string())
}

fn token_to_object(tok: Token) -> Object {
    match tok {
        Token::Integer(i) => Object::int(i),
        Token::Real(r) => Object::real(r),
        Token::String(bytes) => Object::string(bytes),
        Token::Name(n) => Object::exec(Value::Name(crate::name::intern(&n))),
        Token::LiteralName(n) => Object::lit(Value::Name(crate::name::intern(&n))),
        Token::NumberArray(nums) => Object::array(nums.into_iter().map(token_to_object).collect()),
        // All callers consume brace and immediate-name tokens before
        // converting, so these cannot reach here.
        Token::LBrace | Token::RBrace | Token::ImmediateName(_) | Token::BinarySeq(_) => {
            unreachable!("structural tokens handled by the scanner")
        }
    }
}

/// `//name`: substituted with its current value at scan time — the value
/// is *not* executed, even if it's a procedure.
fn resolve_immediate(name: &str, dicts: &[Rc<RefCell<Dict>>]) -> Result<Object, PsError> {
    for d in dicts.iter().rev() {
        if let Some(v) = d.borrow().get(name) {
            return Ok(v);
        }
    }
    Err(PsError::Undefined(name.to_string()))
}

/// The top-level objects of a scanned binary object sequence (#733).
fn decode_sequence(bytes: &[u8], dicts: &[Rc<RefCell<Dict>>]) -> Result<Vec<Object>, PsError> {
    crate::binobj::decode(bytes, &mut |n| resolve_immediate(n, dicts))
}

/// Collect tokens up to the matching `}` into a procedure object. Called
/// with the opening `{` already consumed.
fn scan_procedure(
    lexer: &mut Lexer,
    depth: usize,
    dicts: &[Rc<RefCell<Dict>>],
) -> Result<Object, PsError> {
    if depth >= PROC_NESTING_LIMIT {
        return Err(PsError::Limitcheck);
    }
    let mut items = Vec::new();
    loop {
        match lexer.next_token()? {
            None => {
                return Err(PsError::Syntax(
                    "unterminated procedure: missing '}'".to_string(),
                ));
            }
            Some(Token::RBrace) => break,
            Some(Token::LBrace) => items.push(scan_procedure(lexer, depth + 1, dicts)?),
            Some(Token::ImmediateName(n)) => items.push(resolve_immediate(&n, dicts)?),
            Some(Token::BinarySeq(bytes)) => {
                items.push(Object::procedure(decode_sequence(&bytes, dicts)?));
            }
            Some(tok) => items.push(token_to_object(tok)),
        }
    }
    Ok(Object::procedure(items))
}

#[cfg(test)]
mod tests {

    use super::*;

    type WeakDict = (&'static str, std::rc::Weak<RefCell<Dict>>);

    /// Dropping an `Interp` must free its dictionaries (issue #625):
    /// systemdict holds strong `Rc`s to itself and to `userdict`, and a
    /// program can hang more graph off both. `Weak` handles prove it,
    /// rather than reasoning about the cycle: after a plain `drop` none
    /// of them may upgrade. The program touches the places that could
    /// keep a dict alive: user dicts, a restored and a still-open `save`
    /// (journal), a caught error (`$error`, `errordict`), the font
    /// machinery (`FontDirectory`), and `systemdict begin` left open.
    fn weak_dicts_after_program() -> (Interp, Vec<WeakDict>) {
        let mut it = Interp::new();
        it.run_str(
            "userdict /held 3 dict put
             userdict /nested << /a << /b 1 >> >> put
             save userdict /tmp 1 put restore
             save pop userdict /open 2 put
             { 1 0 div } stopped pop
             /Helvetica findfont 12 scalefont setfont 10 10 moveto (hi) show
             systemdict begin
             userdict /held get
             userdict /nested get /a get
             FontDirectory
             errordict
             $error",
        )
        .expect("probe program runs");
        let mut weaks = vec![
            ("systemdict", std::rc::Rc::downgrade(&it.dstack[0])),
            ("userdict", std::rc::Rc::downgrade(&it.dstack[1])),
        ];
        for name in ["$error", "errordict", "FontDirectory", "nested /a", "held"] {
            match it.pop().expect("dict on the operand stack").value {
                Value::Dict(d) => weaks.push((name, std::rc::Rc::downgrade(&d))),
                _ => panic!("{name}: expected a dict on the operand stack"),
            }
        }
        (it, weaks)
    }

    #[test]
    fn dropping_an_interp_frees_its_dictionaries() {
        for _ in 0..20 {
            let (interp, weaks) = weak_dicts_after_program();
            assert!(
                weaks.iter().all(|(_, w)| w.upgrade().is_some()),
                "dicts must stay alive while the Interp does"
            );
            drop(interp);
            for (name, w) in &weaks {
                assert!(w.upgrade().is_none(), "{name} leaked past the Interp");
            }
        }
    }

    /// Freeing the graph must not recurse once per nesting level: a
    /// program can nest as deep as it likes, and a library user's thread
    /// (this test's is 2 MB) would otherwise abort in `drop` (Codex review
    /// of #625). 100000 levels of dict, of array, and of both alternating,
    /// chains in userdict, left on the operand stack, left open by `begin`,
    /// registered with `defineresource`, and held only by the save journal
    /// (a `save` then an overwrite).
    #[test]
    fn dropping_an_interp_with_deeply_nested_dicts_does_not_overflow_the_stack() {
        let mut it = Interp::new();
        it.run_str(
            "0 1 1 100000 { pop 1 dict dup /n 4 -1 roll put } for userdict /dicts 3 -1 roll put
             0 1 1 100000 { pop 1 array dup 0 4 -1 roll put } for userdict /arrays 3 -1 roll put
             0 1 1 100000 { pop 1 dict dup /n 4 -1 roll put 1 array dup 0 4 -1 roll put } for
             userdict /mixed 3 -1 roll put
             0 1 1 100000 { pop 1 dict dup /n 4 -1 roll put } for
             /deep userdict /dicts get /Generic defineresource pop
             userdict /dicts get begin
             0 1 1 100000 { pop 1 dict dup /n 4 -1 roll put } for
             save pop userdict /arrays 0 put userdict /kept 3 -1 roll put",
        )
        .expect("build the chains");
        drop(it);
    }

    /// The same guard covers a chain freed mid-run (issue #631): no
    /// `Interp` drop is involved, only `undef` releasing the last reference.
    #[test]
    fn freeing_a_deeply_nested_dict_chain_mid_run_does_not_overflow_the_stack() {
        let mut it = Interp::new();
        it.run_str(
            "0 1 1 100000 { pop 1 dict dup /n 4 -1 roll put } for userdict /deep 3 -1 roll put
             userdict /deep undef",
        )
        .expect("build and free the chain");
    }

    /// A caller that popped `userdict` and still holds a borrow of it can
    /// drop the `Interp` without a panic (Codex review of #625).
    #[test]
    fn dropping_an_interp_whose_userdict_is_borrowed_does_not_panic() {
        let mut it = Interp::new();
        it.run_str("userdict").expect("push userdict");
        let Value::Dict(user) = it.pop().expect("userdict").value else {
            panic!("userdict is a dict");
        };
        let guard = user.borrow();
        drop(it);
        drop(guard);
    }

    /// A chain can also run through gstates, each holding the current
    /// font's dictionary: font -> /prev gstate -> font -> ... (Codex review
    /// of #625).
    #[test]
    fn dropping_an_interp_with_a_chain_through_gstates_does_not_overflow_the_stack() {
        let mut it = Interp::new();
        it.run_str(
            "/Helvetica findfont 12 scalefont setfont
             1 1 100000 { pop
               currentfont dup length 2 add dict copy
               gstate 1 index /prev 3 -1 roll put
               setfont
             } for
             userdict /last currentfont put
             /Helvetica findfont setfont",
        )
        .expect("build the chain");
        drop(it);
    }

    /// The same through a gstate's colour space: `Separation` keeps its
    /// colorant operand (Codex review of #625).
    #[test]
    fn dropping_an_interp_with_a_chain_through_a_colour_space_does_not_overflow_the_stack() {
        let mut it = Interp::new();
        it.run_str(
            "userdict /chain 1 dict put
             1 1 100000 { pop
               [/Separation userdict /chain get /DeviceGray { pop 0 }] setcolorspace
               userdict /chain 1 dict dup /g gstate put put
             } for
             /DeviceGray setcolorspace",
        )
        .expect("build the chain");
        drop(it);
    }

    #[test]
    fn dropping_an_interp_with_no_program_frees_both_permanent_dicts() {
        let interp = Interp::new();
        let (system, user) = (
            std::rc::Rc::downgrade(&interp.dstack[0]),
            std::rc::Rc::downgrade(&interp.dstack[1]),
        );
        drop(interp);
        assert!(system.upgrade().is_none(), "systemdict must be freed");
        assert!(user.upgrade().is_none(), "userdict must be freed");
    }

    #[test]
    fn procedure_is_deferred_and_calls_work_via_names() {
        let mut it = Interp::new();
        it.run_str("{2 mul}").expect("scan proc");
        assert_eq!(it.operand_stack().len(), 1);
        let proc = it.pop().expect("proc on stack");
        assert_eq!(proc.repr(), "{2 mul}");

        it.define("double", proc);
        it.run_str("5 double double").expect("call proc");
        assert_eq!(it.pop().expect("result").repr(), "20");
    }

    #[test]
    fn name_resolution_cycles_are_limitcheck() {
        let mut it = Interp::new();
        it.define("a", Object::exec(Value::Name("b".into())));
        it.define("b", Object::exec(Value::Name("a".into())));
        assert_eq!(it.run_str("a"), Err(PsError::Limitcheck));
    }

    #[test]
    fn deep_brace_nesting_is_limitcheck_not_stack_overflow() {
        let src = "{".repeat(2000);
        let mut it = Interp::new();
        assert_eq!(it.run_str(&src), Err(PsError::Limitcheck));
    }

    #[test]
    fn literal_name_pushes_itself() {
        let mut it = Interp::new();
        it.run_str("/foo").expect("literal name");
        assert_eq!(it.pop().expect("name").repr(), "/foo");
    }

    #[test]
    fn error_report_names_the_line_the_error_happened_on() {
        let mut it = Interp::new();
        // A trailing newline is the case that catches an off-by-one: the
        // token `div` finishes and consumes that newline as its
        // delimiter (see lexer.rs's `eat_token_delimiter`) before `div`
        // actually runs and fails, so a naive "read the file's current
        // line" would report line 2 for an error that happened on line 1.
        let err = it.run_str("1 0 div\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 1"), "report: {report}");
    }

    #[test]
    fn error_report_line_advances_across_newlines() {
        let mut it = Interp::new();
        let err = it.run_str("1 1 add pop\n1 0 div\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_line_stays_with_the_main_program_across_a_string() {
        let mut it = Interp::new();
        // "exec" is the last token scanned directly from the main
        // program (line 2); the executable string's own "div" isn't
        // main-program source and must not overwrite that with a
        // spurious line of its own (it would always be line 1).
        let err = it.run_str("42 pop\n(1 0 div) cvx exec").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_lines_a_syntax_error_correctly() {
        // Regression test (Codex review, PR #59): a scan/syntax error
        // propagates out of next_item via `?` before any Action ever
        // runs, so the old Yield-only line sync never saw it — the
        // report kept whatever line the previous good token was on.
        // "1 pop" is line 1; the stray ')' that has no matching '(' is
        // on line 2.
        let mut it = Interp::new();
        let err = it.run_str("1 pop\n)").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_counts_lone_cr_as_a_line_ending() {
        // Regression test (Codex review, PR #59): the file-level line
        // counter only advanced on '\n', missing old-Mac-style
        // lone-CR line endings (PostScript accepts CR, LF, and CRLF).
        let mut it = Interp::new();
        let err = it.run_str("1 pop\r1 0 div\r").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_counts_a_crlf_pair_as_one_line_ending() {
        let mut it = Interp::new();
        let err = it.run_str("1 pop\r\n1 0 div\r\n").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 2"), "report: {report}");
    }

    #[test]
    fn error_report_lines_a_malformed_first_token() {
        // Same gap, worst case: the very first token is malformed, so
        // there's no earlier successfully-scanned token to fall back
        // on at all.
        let mut it = Interp::new();
        let err = it.run_str(")").unwrap_err();
        let report = it.error_report(&err);
        assert!(report.contains("Line: 1"), "report: {report}");
    }

    /// The eexec cipher (Type 1 spec, C1) — the inverse of
    /// `crate::file`'s decoder, duplicated here (as `tests/type1.rs`
    /// already does) rather than shared, since it's ten lines and this
    /// is the only other place that needs to construct an encrypted
    /// stream rather than decode one.
    fn eexec_encrypt(plain: &[u8]) -> Vec<u8> {
        let mut r: u16 = 55665;
        b"XXXX"
            .iter()
            .chain(plain)
            .map(|&p| {
                let c = p ^ (r >> 8) as u8;
                r = (u16::from(c).wrapping_add(r))
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                c
            })
            .collect()
    }

    #[test]
    fn an_error_inside_eexec_does_not_leave_a_phantom_dict_stack_entry() {
        // Regression test (Codex review round 4, PR #59): begin_eexec
        // pushes systemdict for the encrypted stream's duration,
        // popped when the scanner exhausts normally
        // (Action::PopScannerAndDict) -- but unwind_all (what runs on
        // an unhandled error or quit) used to drop the scanner frame
        // without the matching dict-stack pop, leaving systemdict
        // pushed a second time. Nothing about that is visible from
        // outside the crate except dict_stack_len(), which issue #17's
        // lint mode is now the first real consumer of -- it used to
        // misreport this as a "missing end" dict-leak.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"1 0 div");
        let mut src = b"currentfile eexec ".to_vec();
        src.extend(encrypted);
        it.run_source(&src).expect_err("1 0 div still errors");
        assert_eq!(
            it.dict_stack_len(),
            2,
            "systemdict/userdict only -- eexec's push must not survive an aborted run"
        );
    }

    #[test]
    fn a_caught_stop_inside_eexec_does_not_leave_a_phantom_dict_stack_entry() {
        // Regression test (Codex review round 5, PR #59): round 4 only
        // fixed unwind_all (the abort path); `stop` inside an eexec
        // stream -- or an error caught by an *enclosing* `stopped` --
        // unwinds via do_stop instead, which had the identical gap.
        // Worse than the abort case: execution continues afterward, so
        // a leftover systemdict push doesn't just mislead a diagnostic,
        // it silently redirects the next `def`.
        let mut it = Interp::new();
        // Two things this can't be built from: a `{...}` procedure
        // literal (parsed as an ordinary token stream up front, before
        // `eexec` ever runs, so the raw encrypted bytes would be
        // (mis)scanned as literal source) or an executable *string*
        // (`currentfile` explicitly skips non-file scanners per the
        // PLRM, so it would resolve to whatever real file happens to be
        // running further down the stack instead of this one). A `Value
        // ::File` pushed and executed directly — what `run` builds
        // internally — is a real file frame, so `currentfile` inside it
        // resolves to itself, same as a `run`-loaded program would see.
        //
        // A trailing "\n" in the plaintext, not just "stop": the scanner
        // peeks one byte past a token to consume its delimiter
        // (`eat_token_delimiter`) *before* the token is even returned
        // for execution, so without real plaintext there to satisfy
        // that peek it decodes whatever raw byte follows next using the
        // ongoing cipher stream — garbage, not this test's concern.
        // Real Type 1 fonts avoid this the same way, via trailing zero
        // padding after the encrypted section.
        let encrypted = eexec_encrypt(b"stop\n");
        let mut inner = b"currentfile eexec ".to_vec();
        inner.extend(encrypted);
        it.push(Object::exec(Value::File(crate::file::PsFile::from_bytes(
            inner,
        ))));
        it.run_str("stopped pop /probe 1 def")
            .expect("stopped catches it; overall run succeeds");
        assert_eq!(
            it.dict_stack_len(),
            2,
            "systemdict/userdict only -- eexec's push must not survive a caught stop"
        );
        assert_eq!(
            it.load("probe").expect("defined").repr(),
            "1",
            "def after the stopped block must land in userdict, not a phantom systemdict"
        );
    }

    #[test]
    fn a_program_owned_dict_left_open_by_eexec_survives_the_cleanup() {
        // Regression test (Codex review round 6, PR #59): round 5's fix
        // popped whatever was on top of the dict stack unconditionally,
        // assuming it must be eexec's injected systemdict copy — but
        // PostScript inside the encrypted stream can `end` that copy
        // itself and `begin` a dict of its own (a real Type 1 font's
        // `currentdict end ... Private begin` does exactly this) before
        // stopping. That program-owned dict must not be popped out from
        // under it just because *a* pop_systemdict scanner unwound —
        // only the exact dict `begin_eexec` pushed, by identity, should
        // ever be removed here.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"end 10 dict begin stop\n");
        let mut inner = b"currentfile eexec ".to_vec();
        inner.extend(encrypted);
        it.push(Object::exec(Value::File(crate::file::PsFile::from_bytes(
            inner,
        ))));
        it.run_str("stopped pop")
            .expect("stopped catches it; overall run succeeds");
        assert_eq!(
            it.dict_stack_len(),
            3,
            "systemdict/userdict + the program's own still-open dict, per the PLRM's \
             stopped-doesn't-restore-the-dict-stack rule -- not eexec's copy, which the \
             program's own `end` already removed, and not popped again by cleanup"
        );
    }

    #[test]
    fn eexec_completing_normally_pops_its_injected_dict_by_identity_not_position() {
        // Follow-up to round 6: that fix only landed in
        // cleanup_unwound_frame, the abort/stop path. The far more
        // common path -- an eexec stream simply running out of bytes
        // and completing normally -- goes through
        // Action::PopScannerAndDict instead, which still had the
        // original unconditional "pop whatever's on top" logic. A real
        // Type 1 font's `currentdict end ... Private begin` before
        // falling off the end of its encrypted section would lose its
        // own dict on the single most common eexec path of all.
        // Action::PopScannerAndDict now delegates to
        // cleanup_unwound_frame so both paths share one identity check.
        let mut it = Interp::new();
        let encrypted = eexec_encrypt(b"end 10 dict begin\n");
        let mut src = b"currentfile eexec ".to_vec();
        src.extend(encrypted);
        it.run_source(&src)
            .expect("plain completion, nothing errors");
        assert_eq!(
            it.dict_stack_len(),
            3,
            "systemdict/userdict + the program's own dict left open across eexec's natural \
             end -- not eexec's injected copy, which the program's own `end` already removed"
        );
    }

    // --- #323: procedure fetch without a clone, def without re-intern ---

    fn run_top(src: &str) -> String {
        let mut it = Interp::new();
        it.run_str(src).expect("runs");
        it.pop().expect("a result").repr()
    }

    #[test]
    fn put_into_a_running_procedure_is_seen_by_the_same_run() {
        // Element 10 (`sub`) is replaced by `add` before the fetch
        // reaches it, so the running procedure sees the new element.
        let src = "/p { /p load 10 {add} bind 0 get put 10 3 sub } def p";
        assert_eq!(run_top(src), "13");
        assert_eq!(run_top(&format!("{src} pop p")), "13");
    }

    #[test]
    fn a_procedure_executes_itself_recursively() {
        assert_eq!(run_top("/f { dup 0 gt { 1 sub f } if } def 5 f"), "0");
        assert_eq!(
            run_top("/q { dup 0 gt { 1 sub /q load exec } if } def 7 q"),
            "0"
        );
    }

    #[test]
    fn def_inside_save_is_rolled_back_by_restore() {
        assert_eq!(run_top("/x 1 def save /x 2 def x exch restore x add"), "3");
        assert_eq!(run_top("(k) 5 def k /k load add"), "10");
    }

    #[test]
    fn undefined_name_inside_a_procedure_is_reported_by_name() {
        let mut it = Interp::new();
        let err = it.run_str("/p { 1 nosuchname } def p").unwrap_err();
        assert_eq!(err, PsError::Undefined("nosuchname".to_string()));
        let report = it.error_report(&err);
        assert!(report.contains("OffendingCommand: nosuchname"), "{report}");
    }
}
