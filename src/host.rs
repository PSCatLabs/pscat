//! Host integration: what an application that embeds the interpreter uses to
//! extend it and to exchange data with the programs it runs.
//!
//! The rest of the supported API runs a program and reads its pages. This
//! module is for a host that does more than that: an animation engine that
//! calls one procedure per frame, an editor that feeds a program live input,
//! a server that gives programs a few operators of its own. It has five
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use thiserror::Error;

use crate::error::PsError;
use crate::gfx::PathElement;
use crate::interp::Interp;
use crate::object::{Dict, Object, OpFn, Operator, Value as Raw};

/// The most operators one interpreter can have registered with
/// [`Interp::register_operator`].
pub const MAX_HOST_OPERATORS: usize = 256;

/// How deep [`Interp::pop_value`] and [`Interp::lookup`] follow nested arrays
/// and dictionaries before giving the rest back as [`Value::Other`]. A
/// structure that contains itself stops here too.
pub const MAX_VALUE_DEPTH: usize = 32;

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
    /// key is the text `==` would print for it. The order is the
    /// interpreter's `forall` order, stable from run to run.
    Dict(Vec<(String, Value)>),
    /// Any other object, kept as it is.
    Other(Opaque),
}

/// An object [`Value`] has no plain-data form for: a procedure, a mark, a
/// file, a save object, a graphics state or an operator. It can be pushed
/// back with [`Interp::push_value`].
#[derive(Clone)]
pub struct Opaque(Object);

impl Opaque {
    /// The object's PostScript type name, as the `type` operator gives it
    /// (`arraytype` for a procedure, `filetype`, `marktype`, ...).
    pub fn type_name(&self) -> &'static str {
        self.0.type_name()
    }

    /// Whether the object is a procedure (an executable array).
    pub fn is_procedure(&self) -> bool {
        self.0.executable && matches!(self.0.value, Raw::Array(_))
    }
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

    fn from_object(obj: &Object, depth: usize) -> Value {
        match &obj.value {
            Raw::Null => Value::Null,
            Raw::Boolean(b) => Value::Bool(*b),
            Raw::Integer(i) => Value::Int(*i),
            Raw::Real(r) => Value::Real(*r),
            Raw::Name(_) if !obj.executable => Value::Name(obj.text()),
            Raw::String(s) if !obj.executable => Value::String(s.to_vec()),
            Raw::Array(a) if !obj.executable && depth < MAX_VALUE_DEPTH => Value::Array(
                a.snapshot()
                    .iter()
                    .map(|o| Value::from_object(o, depth + 1))
                    .collect(),
            ),
            Raw::Dict(d) if depth < MAX_VALUE_DEPTH => {
                let pairs = d.borrow().pairs();
                Value::Dict(
                    pairs
                        .iter()
                        .map(|(k, v)| {
                            let key = match &k.value {
                                Raw::Name(_) | Raw::String(_) => k.text(),
                                _ => k.repr(),
                            };
                            (key, Value::from_object(v, depth + 1))
                        })
                        .collect(),
                )
            }
            _ => Value::Other(Opaque(obj.clone())),
        }
    }

    fn to_object(&self) -> Object {
        match self {
            Value::Null => Object::lit(Raw::Null),
            Value::Bool(b) => Object::bool(*b),
            Value::Int(i) => Object::int(*i),
            Value::Real(r) => Object::real(*r),
            Value::Name(n) => Object::name(n),
            Value::String(s) => Object::string(s.clone()),
            Value::Array(items) => Object::array(items.iter().map(Value::to_object).collect()),
            Value::Dict(entries) => {
                let mut d = Dict::new();
                for (k, v) in entries {
                    d.put(k.as_str().into(), v.to_object());
                }
                Object::lit(Raw::Dict(Rc::new(std::cell::RefCell::new(d))))
            }
            Value::Other(o) => o.0.clone(),
        }
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
/// The interpreter does not record which line of the source a procedure's
/// body came from, so a frame locates the error in the procedure itself:
/// the element that was running, shown among its neighbours.
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
    pub fn pop_value(&mut self) -> Result<Value, PsError> {
        let obj = self.pop()?;
        Ok(Value::from_object(&obj, 0))
    }

    /// Push a [`Value`] onto the operand stack. An array or dictionary is a
    /// new object each time.
    pub fn push_value(&mut self, value: &Value) {
        self.ostack.push(value.to_object());
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
    /// it; `None` if it is not defined.
    ///
    /// ```
    /// let mut interp = pscat::Interp::new();
    /// interp.run_str("/speed 2.5 def")?;
    /// assert_eq!(interp.lookup("speed").and_then(|v| v.as_f64()), Some(2.5));
    /// assert!(interp.lookup("nosuchname").is_none());
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn lookup(&self, name: &str) -> Option<Value> {
        self.load(name).map(|o| Value::from_object(&o, 0))
    }

    /// Define `name` as `value` in the current dictionary (userdict unless
    /// the program left another one open), as `def` would.
    ///
    /// ```
    /// use pscat::host::Value;
    /// let mut interp = pscat::Interp::new();
    /// interp.define_value("palette", &Value::Array(vec![0.1.into(), 0.5.into()]));
    /// interp.run_str("palette 1 get")?;
    /// assert_eq!(interp.pop_f64()?, 0.5);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn define_value(&mut self, name: &str, value: &Value) {
        self.define(name, value.to_object());
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
