//! Host integration: what an application that embeds the interpreter uses to
//! extend it and to exchange data with the programs it runs.
//!
//! The rest of the supported API runs a program and reads its pages. This
//! module is for a host that does more than that: an animation engine that
//! calls one procedure per frame, an editor that feeds a program live input,
//! a server that gives programs a few operators of its own. It has nine
//! parts, all methods on [`Interp`]:
//!
//! - **Operators.** [`Interp::register_operator`] adds an operator backed by
//!   a Rust closure. To a program it is an ordinary operator: `bind` resolves
//!   it, `type` says `operatortype`, and an error it returns names it as the
//!   offending command.
//! - **Values.** [`Value`] is a PostScript object as plain Rust data.
//!   [`Interp::pop_value`] and [`Interp::push_value`] move one across the
//!   operand stack, [`Interp::lookup`] and [`Interp::define_value`] read and
//!   write names, and the typed `pop_*`/`push_*` methods cover the common
//!   cases without building a [`Value`].
//! - **The current path.** [`Interp::current_path`] reads it in user space as
//!   [`PathSegment`]s and [`Interp::set_current_path`] replaces it, so a host
//!   operator can measure, resample or rewrite a path the program built.
//! - **Errors as data.** [`Interp::error_details`] gives an error's name,
//!   offending command and line as fields, with a backtrace of the
//!   procedures that were running.
//! - **Interruption.** [`Interp::set_interrupt`] gives the interpreter a flag
//!   another thread can raise to stop a running program with
//!   [`PsError::Interrupted`].
//! - **Virtual files.** [`Interp::add_virtual_file`] serves a named source
//!   from memory to `run` and `(name) (r) file`, so a program can load
//!   `components/foo.ps` where no disk (or no right to read it) exists.
//! - **A raster under the transformation.** [`Interp::draw_page`] paints a
//!   [`Page`](crate::Page) the host holds through the current transformation,
//!   clip, `setalpha` and `setblendmode`, as `image` paints a program's own
//!   samples.
//! - **A check for text in the wrong font mode.**
//!   [`Interp::set_utf8_text_check`] turns on the `[utf8-in-byte-font]` lint
//!   for a host that runs programs itself, and [`Interp::utf8_text_findings`]
//!   reads what it found, with the file and line.
//! - **Open transparency groups.** [`Interp::open_group_count`] says how many
//!   `begingroup`s are open on the page, by whatever route they were opened,
//!   and [`Interp::innermost_group_serial`] names the innermost one, so a host
//!   that composes its own layers with
//!   [`swap_canvas`](crate::Interp::swap_canvas) can tell when the canvas is
//!   not the page, and whether it is still the same group.
//!
//! Like the rest of the crate API, all of it is Experimental
//! ([`docs/STABILITY.md`](https://github.com/PSCatLabs/pscat/blob/main/docs/STABILITY.md)):
//! it can change in any release.
//!
//! The canvas itself is reached through [`Page`](crate::Page):
//! [`Interp::swap_canvas`](crate::Interp::swap_canvas) exchanges the raster a
//! program paints on, and [`Page::premultiplied_rgba8`](crate::Page::premultiplied_rgba8)
//! reads its pixels without a copy.
//!
//! ```
//! use std::cell::Cell;
//! use std::rc::Rc;
//! use pscat::Interp;
//!
//! // A host clock the program reads with `now`.
//! let clock = Rc::new(Cell::new(0.0_f64));
//! let mut interp = Interp::with_page(100, 100).expect("valid page size");
//! let c = clock.clone();
//! interp.register_operator("now", move |i| {
//!     i.push_real(c.get());
//!     Ok(())
//! })?;
//! interp.run_str("/spin { now 90 mul } bind def")?;
//! clock.set(0.5);
//! interp.run_str("spin")?;
//! assert_eq!(interp.pop_f64()?, 45.0);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use thiserror::Error;

use crate::error::PsError;
use crate::gfx::PathElement;
use crate::interp::Interp;
use crate::object::{Dict, Object, OpFn, Operator, Value as Raw};

/// The most operators one interpreter can have registered with
/// [`Interp::register_operator`].
pub const MAX_HOST_OPERATORS: usize = 256;

pub use crate::virtual_files::{
    MAX_VIRTUAL_FILE_BYTES, MAX_VIRTUAL_FILES, MAX_VIRTUAL_NAME_BYTES, MAX_VIRTUAL_TOTAL_BYTES,
};

/// The largest raster (in pixels) [`Interp::draw_page`] embeds in SVG or PDF
/// output. PNG output paints the host's page in place and needs no copy, so
/// it has no such limit; the recorders keep a straight-alpha copy of the
/// pixels (and, for SVG, its PNG encoding), which this bounds.
pub const MAX_EMBEDDED_RASTER_PIXELS: usize = 16 << 20;

/// How [`Interp::draw_page`] reads a raster where it is scaled.
///
/// `Nearest` takes the pixel that contains each device pixel's centre, as
/// `image` does; it keeps hard pixel edges and is what a 1:1 placement
/// reproduces exactly. `Bilinear` blends the four nearest pixels, which is
/// what a rotated or enlarged layer of soft art wants. Both leave the edge of
/// the raster anti-aliased.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum Sampling {
    /// The nearest pixel.
    #[default]
    Nearest,
    /// A bilinear blend of the four nearest pixels.
    Bilinear,
}

/// How deep [`Interp::pop_value`] and [`Interp::lookup`] follow nested arrays
/// and dictionaries before giving the rest back as [`Value::Other`]. A
/// structure that contains itself stops here too, and [`MAX_VALUE_NODES`]
/// bounds how much of it is copied on the way.
pub const MAX_VALUE_DEPTH: usize = 32;

/// The most a [`Value`] that [`Interp::pop_value`] or [`Interp::lookup`]
/// copies out may hold, counted in nodes: one for each array element and
/// dictionary entry, however deep, and one for each 32 bytes of a string.
/// It bounds the copy of a structure that holds itself more than once,
/// which within [`MAX_VALUE_DEPTH`] alone could be billions of nodes
/// (`/a 2 array def a 0 a put a 1 a put`). A larger value is `limitcheck`
/// from `pop_value`, and comes back whole as [`Value::Other`] from `lookup`.
/// The empty slots a dictionary keeps after `undef` count as steps, and past
/// 64 times this many in one copy it is a `limitcheck` too.
pub const MAX_VALUE_NODES: usize = 1 << 20;

/// How a dictionary key that is not a name or a string is spelled in a
/// [`Value::Dict`]: as `==` would, shortened past these bounds.
const KEY_LIMITS: crate::object::DumpLimits = crate::object::DumpLimits {
    max_depth: 4,
    max_elements: 16,
    max_string: 128,
    max_bytes: 256,
};

/// String bytes that count as one node.
const STRING_BYTES_PER_NODE: usize = 32;

/// Nodes copied between looks at the limits; also the work one step
/// stands for.
const NODES_PER_STEP: usize = 1024;

/// Empty dictionary slots passed over that cost as much as one node. A
/// dictionary keeps the table of its largest size after `undef`, and a copy
/// walks all of it, so the copy is charged for the empty slots too (#1422).
const SLOTS_PER_NODE: usize = 4;

/// The most empty dictionary slots one conversion may pass over. Slots count
/// as steps, which bound a run but not a copy made between runs, and a
/// dictionary that holds itself twice is copied again for every path to it
/// (up to [`MAX_VALUE_NODES`] nodes): each visit walks the whole retained
/// table. No one dictionary keeps anything near this many slots, so a copy
/// that reaches it is `limitcheck`, like one past the node cap.
const MAX_SCAN_SLOTS: usize = 64 * MAX_VALUE_NODES;

/// One conversion's running count, held to the interpreter's limits.
struct Walk<'a> {
    interp: &'a Interp,
    nodes: usize,
    /// Empty dictionary slots passed over; they count as steps, not nodes, so
    /// they never push a small dictionary past [`MAX_VALUE_NODES`].
    slots: usize,
}

impl Walk<'_> {
    /// Count `n` more nodes, about to take `bytes` of heap. Past the cap it
    /// is `limitcheck`; every [`NODES_PER_STEP`] nodes, and before a large
    /// allocation, the limits are looked at as a running operator's are
    /// (outside a run only the memory cap).
    fn take(&mut self, n: usize, bytes: usize) -> Result<(), PsError> {
        let before = self.nodes;
        self.nodes = self.nodes.saturating_add(n);
        if self.nodes > MAX_VALUE_NODES {
            return Err(PsError::Limitcheck);
        }
        if self.nodes / NODES_PER_STEP != before / NODES_PER_STEP || bytes >= 4096 {
            self.interp.host_look(self.steps(), bytes)?;
        }
        Ok(())
    }

    /// Count `n` empty slots about to be passed over, and look at the
    /// limits when they add up to another step.
    fn scan(&mut self, n: usize) -> Result<(), PsError> {
        let before = self.steps();
        self.slots = self.slots.saturating_add(n);
        if self.slots > MAX_SCAN_SLOTS {
            return Err(PsError::Limitcheck);
        }
        if self.steps() != before {
            self.interp.host_look(self.steps(), 0)?;
        }
        Ok(())
    }

    /// The steps the copy so far stands for.
    fn steps(&self) -> u64 {
        (self.nodes / NODES_PER_STEP + self.slots / (SLOTS_PER_NODE * NODES_PER_STEP)) as u64
    }
}

/// A host operator's closure. `Rc`, not `Box`: the call clones the handle so
/// the closure can borrow the interpreter mutably while it runs.
pub(crate) type HostOp = Rc<dyn Fn(&mut Interp) -> Result<(), PsError>>;

/// Why [`Interp::register_operator`] refused a registration.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum RegisterError {
    /// The interpreter already has [`MAX_HOST_OPERATORS`] host operators.
    #[error("too many host operators (the limit is {MAX_HOST_OPERATORS})")]
    TooMany,
    /// The name is empty, longer than 127 bytes, or has a character a
    /// PostScript name token cannot hold (white space or one of
    /// `()<>[]{}/%`), so no program could call it.
    #[error("'{0}' is not usable as an operator name")]
    BadName(String),
}

/// A PostScript object as plain Rust data, for moving values between a
/// program and its host.
///
/// Composite values are copied: a [`Value::Array`] is the array's elements at
/// the time it was read, and pushing one makes a new array. An object with no
/// plain-data form (a procedure, a file, a save object, an operator) comes
/// back as [`Value::Other`], which can be pushed again unchanged; so a host
/// operator can take a procedure as an operand and hand it back to the
/// program to run.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// An integer.
    Int(i64),
    /// A real.
    Real(f64),
    /// A name, without its slash: `/Helvetica` is `Name("Helvetica")`.
    Name(String),
    /// A string's bytes. PostScript strings are bytes, not text.
    String(Vec<u8>),
    /// A literal array's elements.
    Array(Vec<Value>),
    /// A dictionary's entries. A name or string key is its text; any other
    /// key is the text `==` would print for it, shortened past a few hundred
    /// bytes (a long array key ends in `...`). The order is the
    /// interpreter's `forall` order, stable from run to run.
    Dict(Vec<(String, Value)>),
    /// Any other object, kept as it is.
    Other(Opaque),
}

/// An object [`Value`] has no plain-data form for: a procedure, a mark, a
/// file, a save object, a graphics state or an operator. It can be pushed
/// back with [`Interp::push_value`], into the interpreter it came from.
///
/// It belongs to that interpreter. Its host operators are slots of that
/// interpreter's registrations (a procedure `bind` resolved holds them
/// too), and a save object is one of its save levels, so in another
/// interpreter it would run that one's operators under this one's names, or
/// restore the wrong VM. [`Interp::push_value`] and
/// [`Interp::define_value`] refuse one from another interpreter with
/// `invalidaccess`. (An `Opaque` holds an `Rc`, so it cannot leave its
/// thread: interpreters on separate threads never meet this.)
#[derive(Clone)]
pub struct Opaque {
    obj: Object,
    /// The interpreter it came from ([`Interp`]'s `host_id`).
    owner: u64,
}

impl Opaque {
    /// The object's PostScript type name, as the `type` operator gives it
    /// (`arraytype` for a procedure, `filetype`, `marktype`, ...).
    pub fn type_name(&self) -> &'static str {
        self.obj.type_name()
    }

    /// Whether the object is a procedure (an executable array).
    pub fn is_procedure(&self) -> bool {
        self.obj.executable && matches!(self.obj.value, Raw::Array(_))
    }
}

/// A new interpreter's `host_id`: unique in the process, never reused.
pub(crate) fn next_interp_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl std::fmt::Debug for Opaque {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Opaque({})", self.type_name())
    }
}

impl Value {
    /// The number this value holds, an integer converted to a real; `None`
    /// for anything else.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }

    /// The integer this value holds; `None` for a real or anything else.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The boolean this value holds.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The text of a name, or of a string that is valid UTF-8.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Name(n) => Some(n),
            Value::String(s) => std::str::from_utf8(s).ok(),
            _ => None,
        }
    }

    /// The elements of an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// An array of numbers as reals; `None` if this is not an array or an
    /// element is not a number.
    pub fn as_numbers(&self) -> Option<Vec<f64>> {
        self.as_array()?.iter().map(Value::as_f64).collect()
    }

    /// The value under `key` in a dictionary.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// `obj` as a `Value`, its whole copy counted in `walk`. The caller
    /// has counted `obj` itself; a container counts its members before it
    /// copies them, so nothing is allocated past the cap.
    fn from_object(obj: &Object, depth: usize, walk: &mut Walk<'_>) -> Result<Value, PsError> {
        use std::mem::size_of;
        Ok(match &obj.value {
            Raw::Null => Value::Null,
            Raw::Boolean(b) => Value::Bool(*b),
            Raw::Integer(i) => Value::Int(*i),
            Raw::Real(r) => Value::Real(*r),
            Raw::Name(n) if !obj.executable => {
                walk.take(n.len() / STRING_BYTES_PER_NODE, n.len())?;
                Value::Name(obj.text())
            }
            Raw::String(s) if !obj.executable => {
                walk.take(s.len() / STRING_BYTES_PER_NODE, s.len())?;
                Value::String(s.to_vec())
            }
            Raw::Array(a) if !obj.executable && depth < MAX_VALUE_DEPTH => {
                let len = a.len();
                walk.take(
                    len,
                    len.saturating_mul(size_of::<Value>() + size_of::<Object>()),
                )?;
                let items = a.snapshot();
                let mut out = Vec::with_capacity(items.len());
                for o in items.iter() {
                    out.push(Value::from_object(o, depth + 1, walk)?);
                }
                Value::Array(out)
            }
            Raw::Dict(d) if depth < MAX_VALUE_DEPTH => {
                let len = d.borrow().len();
                walk.take(
                    len,
                    len.saturating_mul(size_of::<(String, Value)>() + 2 * size_of::<Object>()),
                )?;
                // `pairs` walks every slot the tables retain, live or not.
                walk.scan(d.borrow().slot_overhead())?;
                // Copied out first: the borrow must end before a member
                // that is this dictionary is read.
                let pairs = d.borrow().pairs();
                let mut out = Vec::with_capacity(pairs.len());
                for (k, v) in &pairs {
                    out.push((
                        Value::key_text(k, walk)?,
                        Value::from_object(v, depth + 1, walk)?,
                    ));
                }
                Value::Dict(out)
            }
            _ => Value::Other(Opaque {
                obj: obj.clone(),
                owner: walk.interp.host_id,
            }),
        })
    }

    /// A dictionary key's text, counted in `walk` like a value: a name or
    /// string by its length, any other key spelled as `==` would, but
    /// within [`KEY_LIMITS`] (an array key can hold itself).
    fn key_text(k: &Object, walk: &mut Walk<'_>) -> Result<String, PsError> {
        let len = match &k.value {
            Raw::Name(n) => n.len(),
            Raw::String(s) => s.len(),
            _ => {
                walk.take(KEY_LIMITS.max_bytes / STRING_BYTES_PER_NODE, 0)?;
                return Ok(k.repr_bounded(&KEY_LIMITS));
            }
        };
        walk.take(len / STRING_BYTES_PER_NODE, len)?;
        Ok(k.text())
    }

    /// The value as an object of the interpreter `owner`; `invalidaccess`
    /// for an [`Opaque`] anywhere in it that came from another one.
    fn to_object(&self, owner: u64) -> Result<Object, PsError> {
        Ok(match self {
            Value::Null => Object::lit(Raw::Null),
            Value::Bool(b) => Object::bool(*b),
            Value::Int(i) => Object::int(*i),
            Value::Real(r) => Object::real(*r),
            Value::Name(n) => Object::name(n),
            Value::String(s) => Object::string(s.clone()),
            Value::Array(items) => Object::array(
                items
                    .iter()
                    .map(|v| v.to_object(owner))
                    .collect::<Result<_, _>>()?,
            ),
            Value::Dict(entries) => {
                let mut d = Dict::new();
                for (k, v) in entries {
                    d.put(k.as_str().into(), v.to_object(owner)?);
                }
                Object::lit(Raw::Dict(Rc::new(std::cell::RefCell::new(d))))
            }
            Value::Other(o) if o.owner == owner => o.obj.clone(),
            Value::Other(_) => return Err(PsError::InvalidAccess),
        })
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Real(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    /// A string, not a name: [`Value::Name`] is spelled out.
    fn from(v: &str) -> Self {
        Value::String(v.as_bytes().to_vec())
    }
}

/// One segment of a path in user space: what `pathforall` would hand a
/// program, as data. Coordinates are in the user space that is current when
/// the path is read ([`Interp::current_path`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    /// `x y moveto`: start a new subpath.
    MoveTo(f64, f64),
    /// `x y lineto`.
    LineTo(f64, f64),
    /// `x1 y1 x2 y2 x3 y3 curveto`: a cubic Bézier to the last point.
    CurveTo(f64, f64, f64, f64, f64, f64),
    /// `closepath`.
    Close,
}

/// What is known about an error, field by field: the data
/// [`Interp::error_report`] formats into one line. From
/// [`Interp::error_details`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ErrorDetails {
    /// The PLRM error name ([`PsError::name`]): `undefined`, `typecheck`,
    /// `limitexceeded`, `interrupt`, ...
    pub name: &'static str,
    /// The offending command: the undefined name, or the operator or name
    /// being executed when the error happened. `None` when nothing had run.
    pub command: Option<String>,
    /// The 1-based line of the most recent token scanned from program text,
    /// when one is known. It is where execution was, which for an error
    /// inside a procedure is the line that called it, not the line that
    /// defined it.
    pub line: Option<usize>,
    /// The `run` file `line` is in; `None` for a line of the main program.
    pub file: Option<String>,
    /// Which limit was passed, for a limit error.
    pub limit: Option<crate::Limit>,
    /// A syntax error's detail message.
    pub detail: Option<String>,
    /// The procedures that were running, innermost first: where in the
    /// program's own code the error happened. Empty for an error at the top
    /// level of the program text, which `line` locates. At most
    /// [`MAX_TRACE_FRAMES`] frames, the innermost ones.
    pub backtrace: Vec<TraceFrame>,
}

/// The most frames [`ErrorDetails::backtrace`] holds.
pub const MAX_TRACE_FRAMES: usize = 16;

/// One procedure that was running when an error happened.
///
/// A frame locates the error in the procedure itself: the element that was
/// running, shown among its neighbours, and, when the procedure was written
/// in program text, the `line` and `file` of that element.
///
/// A procedure is on the execution stack only until it issues its last
/// element (that is what makes tail calls free), so a procedure whose
/// *last* element made the failing call is not in the trace: the frames
/// are the procedures that still had something left to do.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TraceFrame {
    /// The name the procedure is defined under in a dictionary on the
    /// dictionary stack; `None` for a procedure with no name of its own (a
    /// loop body, a procedure passed as an operand).
    pub procedure: Option<String>,
    /// The index, in the procedure's body, of the element that was running.
    pub element: usize,
    /// That element between `>>>` and `<<<`, with up to five elements
    /// before it and three after, as `==` would print them (long values
    /// shortened): `0 0 moveto >>> show <<< grestore`. A leading or
    /// trailing `...` means the body goes on.
    pub excerpt: String,
    /// The 1-based line of the element that was running, where it was
    /// written: in the main program or in a `run` file. `None` when the
    /// procedure has no source line: it was made at run time (`array`,
    /// `copy`, `astore`, `aload`), came from an executable string, a `token`
    /// or an `eexec` stream (whose lines are not the program's), was read by
    /// an earlier program that had no name (see `file`), or the interpreter
    /// holds too many procedures to keep every position. `bind`, `cvx`,
    /// `cvlit` and `getinterval` keep the line; an element changed with `put`
    /// keeps the line of the token it replaced; a procedure written into with
    /// `copy`, `astore` or `putinterval` has none. Unlike
    /// [`ErrorDetails::line`], which is where execution was, this is where
    /// the procedure is *written*.
    pub line: Option<usize>,
    /// The file `line` is in: the name a `run` file was run under, or the
    /// name the host gave the main program with
    /// [`Interp::set_main_file_name`] while the procedure was read. `None`
    /// for the main program of the run that failed when no name was set, so
    /// a host that runs several programs on one interpreter (a library
    /// loaded by one, called from the next) names the one that defines
    /// procedures before running it, then clears the name:
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.set_main_file_name(Some("scene.ps"));
    /// interp.run_str("/label {\n  10 20 moveto\n  show\n} def")?;
    /// interp.set_main_file_name(None);
    /// let err = interp.run_str("1 label").unwrap_err();
    /// let frame = &interp.error_details(&err).backtrace[0];
    /// assert_eq!((frame.file.as_deref(), frame.line), (Some("scene.ps"), Some(3)));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub file: Option<String>,
}

/// Names leaked for [`Operator::name`], which is `&'static str`. Each
/// distinct name is leaked once per process, however many interpreters
/// register it.
fn static_name(name: &str) -> &'static str {
    static NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(s) = names.get(name) {
        return s;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    names.insert(name.to_string(), leaked);
    leaked
}

/// Operator `N`'s entry point. An [`Operator`] is a plain function pointer
/// (the step loop calls it with no indirection through a closure), so each
/// host slot gets its own monomorphized function that looks its closure up.
fn trampoline<const N: usize>(interp: &mut Interp) -> Result<(), PsError> {
    let op = interp
        .host_ops
        .get(N)
        .cloned()
        .ok_or_else(|| PsError::Undefined(format!("host operator {N}")))?;
    op(interp)
}

macro_rules! trampoline_row {
    ($b:literal) => {
        [
            trampoline::<{ $b * 16 }> as OpFn,
            trampoline::<{ $b * 16 + 1 }>,
            trampoline::<{ $b * 16 + 2 }>,
            trampoline::<{ $b * 16 + 3 }>,
            trampoline::<{ $b * 16 + 4 }>,
            trampoline::<{ $b * 16 + 5 }>,
            trampoline::<{ $b * 16 + 6 }>,
            trampoline::<{ $b * 16 + 7 }>,
            trampoline::<{ $b * 16 + 8 }>,
            trampoline::<{ $b * 16 + 9 }>,
            trampoline::<{ $b * 16 + 10 }>,
            trampoline::<{ $b * 16 + 11 }>,
            trampoline::<{ $b * 16 + 12 }>,
            trampoline::<{ $b * 16 + 13 }>,
            trampoline::<{ $b * 16 + 14 }>,
            trampoline::<{ $b * 16 + 15 }>,
        ]
    };
}

static TRAMPOLINES: [[OpFn; 16]; MAX_HOST_OPERATORS / 16] = [
    trampoline_row!(0),
    trampoline_row!(1),
    trampoline_row!(2),
    trampoline_row!(3),
    trampoline_row!(4),
    trampoline_row!(5),
    trampoline_row!(6),
    trampoline_row!(7),
    trampoline_row!(8),
    trampoline_row!(9),
    trampoline_row!(10),
    trampoline_row!(11),
    trampoline_row!(12),
    trampoline_row!(13),
    trampoline_row!(14),
    trampoline_row!(15),
];

fn usable_operator_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 127
        && !name
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b == 0 || b"()<>[]{}/%".contains(&b))
}

impl Interp {
    /// Add an operator that runs `op`, a Rust closure, under `name` in
    /// systemdict. A program calls it like any built-in operator. The
    /// closure takes its operands from the interpreter with the `pop_*`
    /// methods and leaves its results with the `push_*` ones; if it returns
    /// an error, the operand stack goes back to what it was before the call
    /// and the error names the operator as its offending command, as for a
    /// built-in.
    ///
    /// Registering a name a second time replaces the closure in place:
    /// procedures already bound to the operator call the new one.
    /// Registering the name of a built-in operator replaces the built-in in
    /// systemdict for everything looked up from then on (a procedure that
    /// `bind` already resolved keeps the built-in), which is how a host
    /// takes over, say, `showpage`.
    ///
    /// The closure may capture state, typically an `Rc` the host also
    /// holds. It must not run a program on the interpreter it is given
    /// ([`run_str`](Self::run_str) from inside an operator is not
    /// supported); to have the program run something, push it and let the
    /// program `exec` it.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.register_operator("hypot", |i| {
    ///     let (y, x) = (i.pop_f64()?, i.pop_f64()?);
    ///     i.push_real(x.hypot(y));
    ///     Ok(())
    /// })?;
    /// interp.run_str("/h { 3 4 hypot } bind def h")?;
    /// assert_eq!(interp.pop_f64()?, 5.0);
    /// // An error is the operator's own, and leaves the operands in place.
    /// let err = interp.run_str("(three) 4 hypot").unwrap_err();
    /// assert_eq!(interp.error_details(&err).command.as_deref(), Some("hypot"));
    /// assert_eq!(interp.operand_count(), 2);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn register_operator<F>(&mut self, name: &str, op: F) -> Result<(), RegisterError>
    where
        F: Fn(&mut Interp) -> Result<(), PsError> + 'static,
    {
        if !usable_operator_name(name) {
            return Err(RegisterError::BadName(name.to_string()));
        }
        let name = static_name(name);
        let slot = match self.host_op_names.iter().position(|n| *n == name) {
            Some(slot) => {
                self.host_ops[slot] = Rc::new(op);
                slot
            }
            None => {
                if self.host_ops.len() >= MAX_HOST_OPERATORS {
                    return Err(RegisterError::TooMany);
                }
                self.host_ops.push(Rc::new(op));
                self.host_op_names.push(name);
                self.host_ops.len() - 1
            }
        };
        let func = TRAMPOLINES[slot / 16][slot % 16];
        self.systemdict().borrow_mut().put(
            name.into(),
            Object::exec(Raw::Operator(Operator { name, func })),
        );
        Ok(())
    }

    /// How many objects are on the operand stack.
    pub fn operand_count(&self) -> usize {
        self.ostack.len()
    }

    /// Empty the operand stack, as `clear` does.
    ///
    /// Inside a host operator the caller's operands go the way of any other
    /// pop: if the operator then fails, they are put back, as the
    /// registration promises. From the host between runs, and in an
    /// operator that succeeds, the stack is simply empty afterwards.
    pub fn clear_operands(&mut self) {
        // Outside an operator (`op_low` is 0) nothing is recorded, so this
        // is `clear`; inside one, each pop below the operator's own pushes
        // goes to the operand ledger.
        if self.op_low == 0 {
            self.ostack.clear();
        } else {
            while self.pop().is_ok() {}
        }
    }

    /// Pop the top operand as a [`Value`]. `stackunderflow` if the stack is
    /// empty.
    ///
    /// ```
    /// use pscat::host::Value;
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("<< /Size [640 360] /Title (Dawn) /Loop true >>")?;
    /// let config = interp.pop_value()?;
    /// assert_eq!(config.get("Size").and_then(Value::as_numbers), Some(vec![640.0, 360.0]));
    /// assert_eq!(config.get("Title").and_then(Value::as_text), Some("Dawn"));
    /// assert_eq!(config.get("Loop").and_then(Value::as_bool), Some(true));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    ///
    /// The copy is bounded by [`MAX_VALUE_NODES`] (`limitcheck` past it) and,
    /// from inside a host operator, counted against the running program's
    /// limits and the interrupt flag like any long operator
    /// ([`check_limits`](Self::check_limits)). On an error the operand is
    /// left where it was.
    pub fn pop_value(&mut self) -> Result<Value, PsError> {
        let obj = self.ostack.last().cloned().ok_or(PsError::StackUnderflow)?;
        let (value, steps) = self.value_of(&obj);
        match value {
            Ok(value) => {
                self.charge_host_steps(steps)?;
                self.pop()?;
                Ok(value)
            }
            Err(e) => {
                // The work up to the refusal is charged too; the refusal is
                // what is reported (a step cap it passes is seen next look).
                let _ = self.charge_host_steps(steps);
                Err(e)
            }
        }
    }

    /// `obj` copied out as a [`Value`], and the steps the copy stood for,
    /// whether it finished or was refused part way.
    fn value_of(&self, obj: &Object) -> (Result<Value, PsError>, u64) {
        let mut walk = Walk {
            interp: self,
            nodes: 0,
            slots: 0,
        };
        let value = walk
            .take(1, 0)
            .and_then(|()| Value::from_object(obj, 0, &mut walk));
        (value, walk.steps())
    }

    /// Push a [`Value`] onto the operand stack. An array or dictionary is a
    /// new object each time. `invalidaccess`, and nothing pushed, if the
    /// value holds a [`Value::Other`] from another interpreter (see
    /// [`Opaque`]).
    pub fn push_value(&mut self, value: &Value) -> Result<(), PsError> {
        let obj = value.to_object(self.host_id)?;
        self.ostack.push(obj);
        Ok(())
    }

    /// Push an integer.
    pub fn push_int(&mut self, v: i64) {
        self.ostack.push(Object::int(v));
    }

    /// Push a real.
    pub fn push_real(&mut self, v: f64) {
        self.ostack.push(Object::real(v));
    }

    /// Push a boolean.
    pub fn push_bool(&mut self, v: bool) {
        self.ostack.push(Object::bool(v));
    }

    /// Push a literal name: `push_name("butt")` pushes `/butt`.
    pub fn push_name(&mut self, name: &str) {
        self.ostack.push(Object::name(name));
    }

    /// Push a new string holding `bytes`.
    pub fn push_string(&mut self, bytes: &[u8]) {
        self.ostack.push(Object::string(bytes.to_vec()));
    }

    /// Push a new array of reals.
    pub fn push_numbers(&mut self, values: &[f64]) {
        self.ostack.push(Object::array(
            values.iter().map(|v| Object::real(*v)).collect(),
        ));
    }

    /// Pop a boolean; `typecheck` for anything else.
    pub fn pop_bool(&mut self) -> Result<bool, PsError> {
        match self.ostack.last().map(|o| &o.value) {
            None => Err(PsError::StackUnderflow),
            Some(Raw::Boolean(_)) => match self.pop()?.value {
                Raw::Boolean(b) => Ok(b),
                _ => Err(PsError::Typecheck),
            },
            Some(_) => Err(PsError::Typecheck),
        }
    }

    /// Pop a string and return its bytes; `typecheck` for anything else.
    pub fn pop_string(&mut self) -> Result<Vec<u8>, PsError> {
        match self.ostack.last().map(|o| &o.value) {
            None => Err(PsError::StackUnderflow),
            Some(Raw::String(_)) => match self.pop()?.value {
                Raw::String(s) => Ok(s.to_vec()),
                _ => Err(PsError::Typecheck),
            },
            Some(_) => Err(PsError::Typecheck),
        }
    }

    /// Pop a name and return its text, without the slash; `typecheck` for
    /// anything else.
    pub fn pop_name(&mut self) -> Result<String, PsError> {
        match self.ostack.last().map(|o| &o.value) {
            None => Err(PsError::StackUnderflow),
            Some(Raw::Name(_)) => Ok(self.pop()?.text()),
            Some(_) => Err(PsError::Typecheck),
        }
    }

    /// Pop an array of numbers as reals; `typecheck` if the operand is not
    /// an array or an element is not a number.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("[1 2.5 3]")?;
    /// assert_eq!(interp.pop_numbers()?, vec![1.0, 2.5, 3.0]);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn pop_numbers(&mut self) -> Result<Vec<f64>, PsError> {
        let nums: Option<Vec<f64>> = match self.ostack.last().map(|o| &o.value) {
            None => return Err(PsError::StackUnderflow),
            Some(Raw::Array(a)) => a
                .snapshot()
                .iter()
                .map(|o| o.num().map(|n| n.to_f64()))
                .collect(),
            Some(_) => None,
        };
        let nums = nums.ok_or(PsError::Typecheck)?;
        self.pop()?;
        Ok(nums)
    }

    /// The value `name` has on the dictionary stack, as `load` would find
    /// it; `None` if it is not defined. A value [`pop_value`](Self::pop_value)
    /// would refuse (one past [`MAX_VALUE_NODES`], or whose copy a running
    /// program's limits stop) comes back whole as [`Value::Other`]. From
    /// inside a host operator the copy counts against the step cap as
    /// `pop_value`'s does, charged at the program's next look at its limits
    /// (the next [`check_limits`](Self::check_limits), or the end of the
    /// slice) rather than at once; between runs it is free.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("/speed 2.5 def")?;
    /// assert_eq!(interp.lookup("speed").and_then(|v| v.as_f64()), Some(2.5));
    /// assert!(interp.lookup("nosuchname").is_none());
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn lookup(&self, name: &str) -> Option<Value> {
        self.load(name).map(|o| {
            let (value, steps) = self.value_of(&o);
            // `&self`: the copy, finished or refused, is charged at the
            // program's next look at its limits, as `pop_value` charges at
            // once. Between runs a copy is free.
            if self.running() {
                let pending = &self.host_steps_pending;
                pending.set(pending.get().saturating_add(steps));
            }
            value.unwrap_or_else(|_| {
                Value::Other(Opaque {
                    obj: o.clone(),
                    owner: self.host_id,
                })
            })
        })
    }

    /// Define `name` as `value` in the current dictionary (userdict unless
    /// the program left another one open), as `def` would. `invalidaccess`,
    /// and nothing defined, if the value holds a [`Value::Other`] from
    /// another interpreter (see [`Opaque`]).
    ///
    /// ```
    /// use pscat::host::Value;
    /// let mut interp = pscat::Interp::new();
    /// interp.define_value("palette", &Value::Array(vec![0.1.into(), 0.5.into()]))?;
    /// interp.run_str("palette 1 get")?;
    /// assert_eq!(interp.pop_f64()?, 0.5);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn define_value(&mut self, name: &str, value: &Value) -> Result<(), PsError> {
        let obj = value.to_object(self.host_id)?;
        self.define(name, obj);
        Ok(())
    }

    /// The current path in user space, segment by segment, in the order it
    /// was built. Empty when there is no current path. `undefinedresult` if
    /// the current transformation cannot be inverted (`0 0 scale`).
    ///
    /// The interpreter keeps paths in device space, in single precision, so
    /// a coordinate comes back within a small fraction of a device pixel of
    /// what the program wrote, not bit for bit.
    ///
    /// ```
    /// use pscat::host::PathSegment;
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("2 2 scale 10 10 moveto 30 10 lineto closepath")?;
    /// let path = interp.current_path()?;
    /// assert_eq!(path.len(), 3);
    /// assert_eq!(path[1], PathSegment::LineTo(30.0, 10.0));
    /// assert_eq!(path[2], PathSegment::Close);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn current_path(&self) -> Result<Vec<PathSegment>, PsError> {
        let gfx = self.gfx();
        let elements = gfx.state().path.elements();
        if elements.is_empty() {
            return Ok(Vec::new());
        }
        // A singular matrix has no inverse; one so close to singular that
        // the inverse overflows is no more usable.
        let inv = gfx
            .ctm()
            .invert()
            .filter(|m| {
                [m.sx, m.kx, m.ky, m.sy, m.tx, m.ty]
                    .iter()
                    .all(|v| v.is_finite())
            })
            .ok_or(PsError::UndefinedResult)?;
        let user = |p: crate::gfx::DevPoint| {
            let (x, y) = (f64::from(p.x), f64::from(p.y));
            (
                f64::from(inv.sx) * x + f64::from(inv.kx) * y + f64::from(inv.tx),
                f64::from(inv.ky) * x + f64::from(inv.sy) * y + f64::from(inv.ty),
            )
        };
        Ok(elements
            .into_iter()
            .map(|e| match e {
                PathElement::Move(p) => {
                    let (x, y) = user(p);
                    PathSegment::MoveTo(x, y)
                }
                PathElement::Line(p) => {
                    let (x, y) = user(p);
                    PathSegment::LineTo(x, y)
                }
                PathElement::Curve(c1, c2, p) => {
                    let ((x1, y1), (x2, y2), (x3, y3)) = (user(c1), user(c2), user(p));
                    PathSegment::CurveTo(x1, y1, x2, y2, x3, y3)
                }
                PathElement::Close => PathSegment::Close,
            })
            .collect())
    }

    /// Replace the current path with `segments`, given in the current user
    /// space, as `newpath` followed by the matching path operators would.
    /// `nocurrentpoint` if a subpath does not start with a
    /// [`PathSegment::MoveTo`]; the path is then left empty.
    ///
    /// ```
    /// use pscat::host::PathSegment::{Close, LineTo, MoveTo};
    /// let mut interp = pscat::Interp::with_page(20, 20).expect("valid page size");
    /// interp.set_current_path(&[
    ///     MoveTo(0.0, 0.0), LineTo(20.0, 0.0), LineTo(20.0, 20.0), LineTo(0.0, 20.0), Close,
    /// ])?;
    /// interp.run_str("1 0 0 setrgbcolor fill")?;
    /// assert_eq!(&interp.pages()[0].rgba8()[..4], &[255, 0, 0, 255]);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn set_current_path(&mut self, segments: &[PathSegment]) -> Result<(), PsError> {
        let gfx = self.gfx_mut();
        gfx.newpath();
        let built = segments.iter().try_for_each(|s| match *s {
            PathSegment::MoveTo(x, y) => {
                gfx.moveto(x, y);
                Ok(())
            }
            PathSegment::LineTo(x, y) => gfx.lineto(x, y),
            PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) => gfx.curveto(x1, y1, x2, y2, x3, y3),
            PathSegment::Close => {
                gfx.closepath();
                Ok(())
            }
        });
        if built.is_err() {
            gfx.newpath();
        }
        built
    }

    /// What is known about `err`, the error the last program run on this
    /// interpreter returned, as separate fields: the same facts
    /// [`error_report`](Self::error_report) puts in one line, for a host
    /// that reports errors as data.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// let err = interp.run_str("1 2 add\n(a) (b) add").unwrap_err();
    /// let d = interp.error_details(&err);
    /// assert_eq!((d.name, d.command.as_deref(), d.line), ("typecheck", Some("add"), Some(2)));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    ///
    /// For an error inside a procedure, [`ErrorDetails::backtrace`] says
    /// which procedures were running and where in each:
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("/label { 10 20 moveto show } def
    ///                 /page  { gsave 42 label grestore } def")?;
    /// let err = interp.run_str("page").unwrap_err();
    /// let trace = interp.error_details(&err).backtrace;
    /// assert_eq!(trace[0].procedure.as_deref(), Some("label"));
    /// assert_eq!(trace[0].excerpt, "10 20 moveto >>> show <<<");
    /// assert_eq!(trace[1].procedure.as_deref(), Some("page"));
    /// assert_eq!(trace[1].excerpt, "gsave 42 >>> label <<< grestore");
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn error_details(&self, err: &PsError) -> ErrorDetails {
        let command = match err {
            PsError::Undefined(name) => Some(name.clone()),
            _ => self.last_executed_name(),
        };
        let (line, file) = self.error_line_and_file();
        ErrorDetails {
            name: err.name(),
            command,
            line,
            file,
            limit: err.limit(),
            detail: match err {
                PsError::Syntax(detail) => Some(detail.clone()),
                _ => None,
            },
            backtrace: self.error_trace.clone(),
        }
    }

    /// Give the interpreter a flag that stops a running program. While the
    /// flag is `true`, [`run_str`](Self::run_str) and
    /// [`run_source`](Self::run_source) return [`PsError::Interrupted`] at
    /// the next look: before the program starts, every few thousand steps,
    /// and once more when it ends, so a run during which the flag was up
    /// fails however short it is (a program that finished keeps its page
    /// and operand stack, as after any error). The program cannot catch it
    /// with `stopped`, nor an `Interrupted` a host operator returns. The
    /// host owns the flag: it stays raised until the host lowers it, so
    /// every later run stops too, before it runs anything. `None` takes the
    /// flag away.
    ///
    /// Like the step limit, the flag is looked at between objects, so one
    /// long-running operator finishes first unless it calls
    /// [`check_limits`](Self::check_limits) as it goes. A host that drives the machine
    /// itself with [`step_n`](Self::step_n) reads its own flag between
    /// slices; this one is not consulted there.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::sync::atomic::{AtomicBool, Ordering};
    /// use pscat::{Interp, PsError};
    ///
    /// let stop = Arc::new(AtomicBool::new(false));
    /// let mut interp = Interp::new();
    /// interp.set_interrupt(Some(stop.clone()));
    /// // Another thread would raise it; here it is raised up front.
    /// stop.store(true, Ordering::Relaxed);
    /// assert_eq!(interp.run_str("{ } loop").unwrap_err(), PsError::Interrupted);
    /// stop.store(false, Ordering::Relaxed);
    /// interp.run_str("1 1 add pop")?;
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn set_interrupt(&mut self, flag: Option<Arc<AtomicBool>>) {
        self.interrupt = flag;
    }

    /// For a host operator that does a lot of work in one call: count
    /// `steps` steps of it against the step cap, and look at the time and
    /// memory caps and the interrupt flag, as the interpreter does between
    /// objects. The interpreter looks only between objects, so an operator
    /// whose work grows with its operands (a loop whose count comes from
    /// the program, a large copy) should call this as it goes, every few
    /// thousand items, and return its error with `?`.
    ///
    /// The error is [`PsError::Interrupted`] for a raised flag, else the
    /// limit's (`StepsExceeded`, `TimeExceeded`, `MemoryExceeded`); like
    /// those, the program cannot catch it with `stopped`. A step is about
    /// what one cheap operator costs (the interpreter counts a KiB read as
    /// one), so charge in proportion to the work.
    ///
    /// It costs a branch and a load when no limits are set and no flag is
    /// raised. Called from the host between runs, it looks only at the
    /// flag and charges nothing: the limits belong to a running program.
    ///
    /// ```
    /// use pscat::{Interp, Limits, PsError};
    /// let mut interp = Interp::new();
    /// // `n spin`: n units of host work, charged as it goes.
    /// interp.register_operator("spin", |i| {
    ///     let n = i.pop_int()?;
    ///     for k in 0..n {
    ///         // ... one unit of work ...
    ///         if k % 1000 == 999 {
    ///             i.check_limits(1000)?;
    ///         }
    ///     }
    ///     Ok(())
    /// })?;
    /// interp.set_limits(Limits::none().with_max_steps(100_000))?;
    /// interp.run_str("10000 spin")?;
    /// let err = interp.run_str("{ 1000000000000 spin } stopped").unwrap_err();
    /// assert_eq!(err, PsError::StepsExceeded);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn check_limits(&mut self, steps: u64) -> Result<(), PsError> {
        self.host_poll(steps)
    }

    /// Whether the host's interrupt flag is raised.
    #[inline]
    pub(crate) fn interrupted(&self) -> bool {
        self.interrupt
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Relaxed))
    }
}

impl Interp {
    /// Serve `bytes` to programs under `name`: `(name) run`, `(name) (r) file`,
    /// `readreals` and `status` then read these bytes, with no disk involved.
    /// A second registration of the same name replaces the first. A program
    /// that was already running a file under the name keeps its own copy.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.add_virtual_file("components/dot.ps", b"/dot { 3 3 moveto } def")?;
    /// interp.run_str("(components/dot.ps) run dot currentpoint")?;
    /// assert_eq!((interp.pop_f64()?, interp.pop_f64()?), (3.0, 3.0));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    ///
    /// `bytes` is an owned `Vec<u8>` (moved in, never copied) or a borrowed
    /// slice (copied once, after every check below has passed, so a refused
    /// call allocates nothing). The sizes are looked at first: a borrowed
    /// copy is also held to `--max-memory` (`memoryexceeded` before it is made)
    /// and, called from a host operator, charged a step per KiB to the
    /// running program like any host work.
    ///
    /// The name is matched whole and exactly (one leading `./` is ignored),
    /// and wins over a disk file and the embedded `lib/` copy of the same
    /// name, so a host can replace a library. Any other name is resolved as
    /// before: registering files changes nothing else, and under read roots
    /// ([`set_read_roots`](Self::set_read_roots)) an unregistered name outside
    /// them is still `invalidfileaccess`. Virtual files are not disk reads, so
    /// the roots do not gate them: a program can read exactly the names the
    /// host registered, which is how a sandbox (`pscat-mcp`, `--allow-read`)
    /// stays closed, since only the host's own code registers a file. Only
    /// reading is served: opening one for writing, `deletefile`,
    /// `renamefile` and `filenameforall` do not see them. An error inside a
    /// file `run` this way is reported with the name (`error_details().file`,
    /// the backtrace frames, `currentfilename`).
    ///
    /// Refused, with nothing changed: an empty name or one with a NUL
    /// (`rangecheck`); a name that starts with `%` (a device) or has a `..`
    /// component (`invalidfileaccess`); a name over
    /// [`MAX_VIRTUAL_NAME_BYTES`], a file over [`MAX_VIRTUAL_FILE_BYTES`], a
    /// store that would pass [`MAX_VIRTUAL_FILES`] files or
    /// [`MAX_VIRTUAL_TOTAL_BYTES`] bytes (`limitcheck`). The store counts what
    /// it keeps: each file's name and its buffer's *capacity*, so
    /// `shrink_to_fit` a `Vec` that reserved more than it holds.
    ///
    /// Each read of a virtual file copies it into the program's file object,
    /// and is charged like reading that many bytes from disk (a step per KiB
    /// against `--max-steps`, the memory cap checked before the copy), so a
    /// program that loads a large file in a loop meets its limits.
    pub fn add_virtual_file<'a>(
        &mut self,
        name: &str,
        bytes: impl Into<std::borrow::Cow<'a, [u8]>>,
    ) -> Result<(), PsError> {
        use std::borrow::Cow;
        let bytes = bytes.into();
        let (len, capacity) = match &bytes {
            Cow::Borrowed(b) => (b.len(), b.len()),
            Cow::Owned(v) => (v.len(), v.capacity()),
        };
        self.virtual_files.check(name, len, capacity)?;
        if matches!(bytes, Cow::Borrowed(_)) {
            // A copy is about to be made: under the memory cap, and as host
            // work for the program that is running (a host operator).
            self.check_alloc(len)?;
            self.charge_host_steps((len / 1024) as u64 + 1)?;
        }
        self.virtual_files.add(name, bytes.into_owned())
    }

    /// Stop serving `name` (see [`add_virtual_file`](Self::add_virtual_file));
    /// whether it was registered. Reads of it fall back to the ordinary
    /// search order.
    pub fn remove_virtual_file(&mut self, name: &str) -> bool {
        self.virtual_files.remove(name)
    }

    /// The bytes of a registered virtual file, shared, for the readers of
    /// files (`run`, `file`, `readreals`, `status`). `None` when `name` is
    /// not registered; the caller charges the copy it makes.
    pub(crate) fn virtual_file(&self, name: &str) -> Option<std::rc::Rc<Vec<u8>>> {
        self.virtual_files.get(name)
    }
}

/// Fonts a [`Utf8TextReport`] names, the rest being counted in
/// [`Utf8TextReport::more_fonts`].
pub const MAX_UTF8_TEXT_FONTS: usize = crate::lint::UTF8_FONTS_SHOWN;

/// Distinct characters a [`Utf8TextFinding`] names.
pub const MAX_UTF8_TEXT_CHARS: usize = crate::lint::UTF8_CHARS_SHOWN;

/// The most fonts past [`MAX_UTF8_TEXT_FONTS`] that [`Utf8TextReport::more_fonts`]
/// counts, so it is a floor ("at least") for a program that uses more.
pub const MAX_UTF8_TEXT_MORE_FONTS: usize = crate::lint::UTF8_MORE_FONTS_COUNTED;

/// The longest [`Utf8TextFinding::font`], in bytes before a closing `...`
/// that marks a longer name as cut.
pub const MAX_UTF8_TEXT_NAME_BYTES: usize = crate::font::UTF8_FONT_NAME_MAX;

/// UTF-8 text that a program showed in a font that reads one character per
/// byte, the finding behind the `[utf8-in-byte-font]` lint. From
/// [`Interp::utf8_text_findings`].
///
/// `(aπb) show` in Helvetica reads the two bytes of `π` as two codes and
/// draws stray accents. That is correct PostScript, so nothing fails; a font
/// with `/PSCatUnicode true` reads the string as UTF-8 instead.
///
/// One finding per font, for the first show in it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Utf8TextFinding {
    /// The font dict's `FontName` (`?` when it has none), cut to
    /// [`MAX_UTF8_TEXT_NAME_BYTES`] bytes.
    pub font: String,
    /// The non-ASCII characters shown, in order of first appearance, at most
    /// [`MAX_UTF8_TEXT_CHARS`] distinct ones.
    pub characters: Vec<char>,
    /// More distinct characters were shown than `characters` holds.
    pub more_characters: bool,
    /// The 1-based line of the most recent token scanned from program text
    /// when the first such show ran: where execution was, as for
    /// [`ErrorDetails::line`], which for a show inside a procedure is the
    /// line that called it, not the line that wrote the show. `None` when
    /// no line is known.
    pub line: Option<usize>,
    /// The file `line` is in: the name a `run` file was run under, or, for a
    /// line of the main program, the name the host gave it with
    /// [`Interp::set_main_file_name`] when the show ran. `None` when `line`
    /// is `None` or the main program has no name.
    pub file: Option<String>,
}

/// What [`Interp::utf8_text_findings`] found: at most
/// [`MAX_UTF8_TEXT_FONTS`] findings, so the report is bounded however much
/// text a program shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Utf8TextReport {
    /// One finding per font, in the order the fonts were first used.
    pub fonts: Vec<Utf8TextFinding>,
    /// Distinct fonts past [`MAX_UTF8_TEXT_FONTS`] that were shown UTF-8 text,
    /// not named in `fonts` (counted up to [`MAX_UTF8_TEXT_MORE_FONTS`]).
    pub more_fonts: usize,
}

impl Interp {
    /// Turn the check for UTF-8 text shown in a byte-mode font on or off:
    /// the `[utf8-in-byte-font]` finding of `pscat --lint` (issue #1412), for
    /// a host that builds its own interpreter. Read what it found with
    /// [`utf8_text_findings`](Self::utf8_text_findings).
    ///
    /// Off by default. While off, a show costs one extra branch and nothing
    /// else changes; turning it on never changes what a program draws, what
    /// it leaves on its stacks or how many steps it takes. While on, a show
    /// of a string with a high byte in a font with no `/PSCatUnicode true`
    /// looks at the string (at most 4096 bytes, so the work per show is
    /// bounded; a longer string is not examined) and at the font's name (at
    /// most [`MAX_UTF8_TEXT_NAME_BYTES`] bytes). It is deliberately narrow, as a false report is worse than
    /// a miss: it needs a string that is wholly well-formed UTF-8 with a
    /// non-ASCII character, an outline face that is not Symbol or
    /// ZapfDingbats, and no custom `Encoding`; `kshow` is not checked, and a
    /// font with `/PSCatUnicode false` is taken to want bytes. The
    /// findings it keeps are bounded by [`MAX_UTF8_TEXT_FONTS`] and
    /// [`MAX_UTF8_TEXT_CHARS`].
    ///
    /// The findings belong to the program that ran last: starting the next one
    /// (`run_str`, `run_source`, `begin_source`) forgets them, so read them after
    /// each run, including a run that returned an error (the show before the
    /// error is still reported). Turning the check off forgets them too.
    ///
    /// Like the rest of the crate API this is Experimental
    /// (`docs/STABILITY.md`): it can change in any release.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(100, 100).expect("valid page size");
    /// interp.set_utf8_text_check(true);
    /// interp.set_main_file_name(Some("scene.ps"));
    /// interp.run_str("/Helvetica findfont 12 scalefont setfont 5 40 moveto\n(aπb) show")?;
    /// let report = interp.utf8_text_findings();
    /// assert_eq!(report.fonts.len(), 1);
    /// let finding = &report.fonts[0];
    /// assert_eq!((finding.font.as_str(), finding.characters.as_slice()), ("Helvetica", ['π'].as_slice()));
    /// assert_eq!((finding.file.as_deref(), finding.line), (Some("scene.ps"), Some(2)));
    ///
    /// // The same text in a font that reads UTF-8 is not reported.
    /// interp.run_str("/Helvetica findfont dup length dict copy dup /PSCatUnicode true put
    ///                 /Helvetica-U exch definefont 12 scalefont setfont 5 40 moveto (aπb) show")?;
    /// assert!(interp.utf8_text_findings().fonts.is_empty());
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn set_utf8_text_check(&mut self, on: bool) {
        self.set_utf8_text_trace(on);
    }

    /// What the check turned on by
    /// [`set_utf8_text_check`](Self::set_utf8_text_check) found in the
    /// program that ran last. Empty when the check is off.
    pub fn utf8_text_findings(&self) -> Utf8TextReport {
        self.utf8_text_trace()
            .map(crate::lint::Utf8TextTrace::report)
            .unwrap_or_default()
    }

    /// How many transparency groups (`begingroup`) are open on the page now,
    /// as the interpreter itself counts them (#1494). A group counts however
    /// it was opened (the operator named in a program, `systemdict /begingroup
    /// get exec`, a procedure that calls it) and stops counting the moment
    /// the interpreter closes it: at its `endgroup`, at `erasepage`,
    /// `showpage` or `copypage`, at a `restore` that unwinds past it, when
    /// the program ends, and when an uncaught error, a limit or the
    /// interrupt flag stops it. A caught error does not close a group:
    /// `{ ... begingroup ... error } stopped` leaves it open, as it leaves a
    /// `gsave` open. A program that a host runs in slices
    /// ([`begin_source`](Interp::begin_source), [`step_n`](Interp::step_n))
    /// keeps its groups open between slices until it finishes.
    ///
    /// This is the host's answer to "is the canvas the page?". With a group
    /// open, the raster the program paints on is the group's layer, and
    /// [`swap_canvas`](Interp::swap_canvas) exchanges the page *below* it,
    /// which the group still composites onto at its `endgroup`. So a host
    /// that paints its own layers with `swap_canvas` finds that a layer
    /// swapped in while the count is above 0 is not painted on until the
    /// group closes. One exception: a group opened while nothing paints
    /// (after `nulldevice`) has no layer, is counted, and displaces
    /// nothing.
    ///
    /// The count is the page's, from anywhere. Inside a pattern's
    /// `PaintProc` it is still the number of groups the page has open (the
    /// ones opened before the pattern was painted), and the `PaintProc`'s own
    /// groups are not counted: they live on the tile, which a host cannot
    /// reach with `swap_canvas`. A `PaintProc` cannot close the page's
    /// groups either: its `endgroup` with none of its own open does nothing,
    /// so the count does not change. With no page group open the count is 0
    /// inside a `PaintProc`, where `swap_canvas` hands the page back
    /// unchanged (there is no page to exchange).
    ///
    /// A read of the interpreter's state: it costs no steps, memory or time
    /// against the limits, and changes nothing. Like the rest of the crate
    /// API this is Experimental (`docs/STABILITY.md`): it can change in any
    /// release. [`innermost_group_serial`](Self::innermost_group_serial)
    /// says which group the innermost is.
    ///
    /// ```
    /// use std::{cell::RefCell, rc::Rc};
    /// let mut interp = pscat::Interp::with_page(2, 2).expect("valid page size");
    /// // A host operator reads the count while the program runs.
    /// let seen = Rc::new(RefCell::new(Vec::new()));
    /// let log = seen.clone();
    /// interp.register_operator("groups", move |i| {
    ///     log.borrow_mut().push(i.open_group_count());
    ///     Ok(())
    /// })?;
    /// interp.run_str("groups << >> begingroup groups
    ///                 << >> systemdict /begingroup get exec groups
    ///                 endgroup groups erasepage groups")?;
    /// // None, one, two (opened through `systemdict`), one again, and
    /// // `erasepage` closed the last.
    /// assert_eq!(*seen.borrow(), [0, 1, 2, 1, 0]);
    /// assert_eq!(interp.open_group_count(), 0); // and none after the run
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn open_group_count(&self) -> usize {
        self.gfx().page_group_count()
    }

    /// Which group is the innermost one open on the page: its serial, or
    /// `None` with no group open (#1494). A serial is the number of groups
    /// the interpreter has opened when this one began, counting from 1, so it
    /// is monotonic and never repeats for the life of the interpreter: not
    /// after the group is closed, not after a `restore` or a page boundary
    /// closed it, not across runs. Equal serials mean the same group.
    ///
    /// A count alone cannot say this. A procedure the host calls can close
    /// the group it is inside and open another at the same depth, and
    /// [`open_group_count`](Self::open_group_count) is the same before and
    /// after; the serial differs. Compare serials to ask "is the innermost
    /// group still the one I saw?".
    ///
    /// Like the count, it is the page's: inside a pattern's `PaintProc` it is
    /// the innermost group the page has open (the `PaintProc`'s own groups,
    /// which the tile holds, are not reported, though they take a serial from
    /// the same counter), and a `PaintProc`'s `endgroup` with none of its own
    /// open does not change it. A read: no steps, memory or time are charged.
    /// Experimental like the rest of the crate API.
    ///
    /// ```
    /// use std::{cell::RefCell, rc::Rc};
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// let seen = Rc::new(RefCell::new(Vec::new()));
    /// let log = seen.clone();
    /// interp.register_operator("serial", move |i| {
    ///     log.borrow_mut().push(i.innermost_group_serial());
    ///     Ok(())
    /// })?;
    /// // Close a group and open another at the same depth: the count is 1
    /// // both times, the serial is not.
    /// interp.run_str("serial << >> begingroup serial serial endgroup
    ///                 << >> begingroup serial")?;
    /// let s = seen.borrow();
    /// assert_eq!(s[0], None);
    /// assert_eq!(s[1], s[2]); // the same group twice
    /// assert!(s[3] > s[1]);   // a different one, later
    /// assert_eq!(interp.innermost_group_serial(), None); // closed with the run
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn innermost_group_serial(&self) -> Option<u64> {
        self.gfx().innermost_page_group_serial()
    }
}
