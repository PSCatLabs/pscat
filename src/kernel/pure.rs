//! Can a pixel run in the interpreter on its own? (#1093)
//!
//! A kernel with a branch that did not compile (a soft branch) retires the
//! lanes that take it. Those pixels can be interpreted one by one, after the
//! compiled ones ([`super::fallback_pixels`]), only if interpreting them
//! cannot change anything another pixel reads: the compiled pixels were
//! computed from the state as it was when `kernelimage` was called. Otherwise
//! the whole image runs in the interpreter.
//!
//! The question is decided over everything the procedure can reach, not over
//! the soft branches alone. A retired pixel runs the whole procedure again,
//! and past its soft branch it runs code the compiler only checked for the
//! paths it compiled: a soft branch may have rebound a name the rest reads.
//!
//! **The universe.** Start from the procedure and the operand stack under it
//! (a pixel can reach that with `index`). Add every element of every array or
//! procedure reached, every key and value of every dictionary reached, and,
//! for every executable name reached, what the dictionary stack binds it to
//! now. Every object a pixel can execute is in that set:
//!
//! - its procedure text is;
//! - a name resolves either to its binding now (in the set) or to a value the
//!   pixel itself defined, since nothing a pure pixel does can write any
//!   other dictionary. A value it defines is on its operand stack, so it is
//!   in the set, or is a fresh array or dictionary of such values, or a
//!   number;
//! - nothing it may run turns data into code (`cvx`, `token`, `run`).
//!
//! So the procedure is pure when every operator in the set is in [`PURE`]
//! and nothing in it executes without being scanned (an executable string or
//! file). [`PURE`] has no operator that writes into an existing object
//! (`put`, `putinterval`, `astore`, `copy`'s composite form, `store`, `undef`,
//! `cvs`, `bind`), touches the random state, the graphics state, files, the
//! dictionary stack (`begin`, `end`: without them every `def` lands in the
//! pixel's own dictionary), or the execution stack (`stop`, `stopped`,
//! `execstack`), or turns data into code.
//!
//! **Three forms are told from the text** (#1093 follow-up, from
//! fractal_clouds), each by the elements just before it in a procedure:
//!
//! - `n copy` with `n` an integer literal is the stack form;
//! - `n array astore` stores into the array just made;
//! - `/x load` reads `x`'s binding now, which is added to the universe as
//!   for an executable `x`;
//! - `b {..} if`, `b not {..} if` and `b {..} {..} ifelse` with `b` an
//!   executable name bound to a boolean now: the branch that is not taken
//!   is left out of the universe, so long as no literal `/b` is in it (a
//!   pure pixel can only rebind `b` with `def`, which needs one). If one
//!   turns up, the branches left out are scanned after all.
//!
//! Each holds unless a pixel fetches the `copy`, `astore`, `load` or branch out of
//! its procedure with `get` and runs it apart from what precedes it; a
//! procedure that does that is not one anyone writes for `kernelimage`.
//!
//! What a pure pixel can still do is reach under its own operands, or `exit`
//! out of the procedure. The per-pixel driver shows a pixel the operands the
//! whole-image driver does, so the first does the same in both, except when
//! it changes the column the whole-image driver stores the pixel in. That,
//! and an `exit`, the per-pixel driver checks for after each pixel: it then
//! puts the operand stack back and runs the whole image in the interpreter
//! instead (no pixel before changed anything else).

use std::collections::HashSet;
use std::rc::Rc;

use crate::interp::Interp;
use crate::object::{Object, PsArray, Value};

/// Operators a pure procedure may use: none writes into an object it did
/// not make, and none reaches state outside the operand stack and the
/// pixel's own dictionary. `copy` is not here: its composite form writes
/// into its second operand, and whether the stack form is meant cannot be
/// told from the text (a procedure's elements can be fetched with `get`).
const PURE: &[&str] = &[
    // stack (a reach under the pixel's own operands is caught at run time)
    "pop",
    "exch",
    "dup",
    "index",
    "roll",
    "clear",
    "count",
    "counttomark",
    "cleartomark",
    "]",
    ">>",
    // arithmetic
    "add",
    "sub",
    "mul",
    "div",
    "idiv",
    "mod",
    "neg",
    "abs",
    "ceiling",
    "floor",
    "round",
    "truncate",
    "sqrt",
    "exp",
    "ln",
    "log",
    "sin",
    "cos",
    "atan",
    "cvi",
    "cvr",
    "min",
    "max",
    ".min",
    ".max",
    // compare and logic
    "eq",
    "ne",
    "lt",
    "le",
    "gt",
    "ge",
    "and",
    "or",
    "xor",
    "not",
    "bitshift",
    // control
    "if",
    "ifelse",
    "exec",
    "for",
    "repeat",
    "loop",
    "forall",
    "exit",
    // the pixel's own dictionary, fresh objects, reads
    "def",
    "dict",
    "array",
    "known",
    "length",
    "get",
    "getinterval",
    "aload",
    // noise: reads of the table `noiseseed` set
    "gnoise2",
    "gnoise3",
    "gnoise2d",
    "vnoise2",
    "vnoise3",
    "fbm2",
    "fbm3",
    "vfbm2",
    "vfbm3",
    "ridged2",
    "ridged3",
];

/// Objects a scan may look at; past this the procedure counts as impure.
const MAX_WORK: usize = 1 << 22;
/// Reasons a note lists.
const MAX_REASONS: usize = 4;

/// Why `proc` is not pure, or `None` when it is (see the module docs).
/// `below` is the operand stack under the procedure's `i j`.
pub(crate) fn impurity(it: &Interp, proc: &PsArray, below: &[Object]) -> Option<String> {
    let mut s = Scan {
        it,
        arrays: HashSet::new(),
        dicts: HashSet::new(),
        names: HashSet::new(),
        seen: HashSet::new(),
        work: 0,
        todo: Vec::new(),
        found: Vec::new(),
        literals: HashSet::new(),
        deferred: Vec::new(),
    };
    let top: Option<Rc<str>> = None;
    s.todo
        .push((Object::exec(Value::Array(proc.clone())), top.clone()));
    for o in below {
        s.todo.push((o.clone(), top.clone()));
    }
    s.run();
    // A branch left out as not taken whose guard a pixel could rebind:
    // scan it after all, and what it reaches, until nothing changes.
    loop {
        let (back, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.deferred)
            .into_iter()
            .partition(|d| s.literals.contains(&d.0));
        s.deferred = keep;
        if back.is_empty() || !s.found.is_empty() {
            break;
        }
        for (_, o, at) in back {
            s.todo.push((o, at));
        }
        s.run();
    }
    // `PSCAT_KERNEL_TIMING` lists every reason (#1093 follow-up: the
    // "and N more" hid which ones a program had left to fix).
    let max = if std::env::var_os("PSCAT_KERNEL_TIMING").is_some() {
        usize::MAX
    } else {
        MAX_REASONS
    };
    match s.found.len() {
        0 => None,
        n if n > max => Some(format!(
            "{}; and {} more (PSCAT_KERNEL_TIMING=1 lists them)",
            s.found[..MAX_REASONS].join("; "),
            n - MAX_REASONS
        )),
        _ => Some(s.found.join("; ")),
    }
}

struct Scan<'a> {
    it: &'a Interp,
    /// Array storage already scanned, by address (a view is scanned as its
    /// whole storage: `getinterval` shares it).
    arrays: HashSet<usize>,
    dicts: HashSet<usize>,
    /// Executable names whose binding has been added.
    names: HashSet<u32>,
    work: usize,
    /// Objects to look at, each with the innermost named procedure it was
    /// reached through (for the reason).
    todo: Vec<(Object, Option<Rc<str>>)>,
    /// What makes it impure: one entry per operator (or kind of object).
    found: Vec<String>,
    seen: HashSet<String>,
    /// Literal names in the universe: a `def` can rebind only these.
    literals: HashSet<u32>,
    /// Branches left out as not taken: the guard's name, the branch, and
    /// where it was reached.
    deferred: Vec<(u32, Object, Option<Rc<str>>)>,
}

impl Scan<'_> {
    fn run(&mut self) {
        while let Some((o, at)) = self.todo.pop() {
            self.work += 1;
            if self.work > MAX_WORK {
                self.found.push("too much to check for side effects".into());
                return;
            }
            let mut flag = |what: String| {
                if self.seen.insert(what.clone()) {
                    self.found.push(match &at {
                        Some(n) => format!("{what} in {n}"),
                        None => what,
                    });
                }
            };
            match &o.value {
                Value::Integer(_)
                | Value::Real(_)
                | Value::Boolean(_)
                | Value::Mark
                | Value::Null => {}
                Value::Name(n) => {
                    if !o.executable {
                        self.literals.insert(n.id());
                    }
                    if o.executable
                        && self.names.insert(n.id())
                        && let Some(b) = self.it.load_id(n.id())
                    {
                        self.todo.push((b, Some(n.text_rc())));
                    }
                }
                Value::Operator(op) => {
                    if !PURE.contains(&op.name) {
                        flag(op.name.to_string());
                    }
                }
                Value::Array(a) => {
                    let id = Rc::as_ptr(a.data_rc()) as *const () as usize;
                    if self.arrays.insert(id) {
                        let items: Vec<Object> = a.data_rc().borrow().clone();
                        self.work += items.len();
                        let skip = self.told(&items, &at);
                        for (k, el) in items.into_iter().enumerate() {
                            if !is_scalar(&el) && !skip.contains(&k) {
                                self.todo.push((el, at.clone()));
                            }
                        }
                    }
                }
                Value::Dict(d) => {
                    let id = Rc::as_ptr(d) as *const () as usize;
                    if self.dicts.insert(id) {
                        let pairs = d.borrow().pairs();
                        self.work += pairs.len();
                        for (k, v) in pairs {
                            for el in [k, v] {
                                if !is_scalar(&el) {
                                    self.todo.push((el, at.clone()));
                                }
                            }
                        }
                    }
                }
                // Data a pure operator cannot use; run, it would be code no
                // scan has seen.
                Value::String(_) | Value::File(_) | Value::Save(_) | Value::GState(_) => {
                    if o.executable {
                        flag(format!("an executable {}", o.type_name()));
                    }
                }
            }
        }
    }
}

impl Scan<'_> {
    /// The operator `o` runs: itself, or an executable name's binding now.
    fn op_of(&self, o: &Object) -> Option<&'static str> {
        match &o.value {
            Value::Operator(op) => Some(op.name),
            Value::Name(n) if o.executable => match self.it.load_id(n.id())?.value {
                Value::Operator(op) => Some(op.name),
                _ => None,
            },
            _ => None,
        }
    }

    /// The elements of a procedure that the forms told from the text (see
    /// the module docs) leave out of the scan; a branch not taken is
    /// deferred, not dropped.
    fn told(&mut self, items: &[Object], at: &Option<Rc<str>>) -> HashSet<usize> {
        let mut skip = HashSet::new();
        let int = |o: &Object| matches!(o.value, Value::Integer(_));
        let proc = |o: &Object| o.executable && matches!(o.value, Value::Array(_));
        for (k, el) in items.iter().enumerate() {
            match self.op_of(el) {
                Some("copy") if k >= 1 && int(&items[k - 1]) => {
                    skip.insert(k);
                }
                Some("astore")
                    if k >= 2
                        && int(&items[k - 2])
                        && self.op_of(&items[k - 1]) == Some("array") =>
                {
                    skip.insert(k);
                }
                Some("load") if k >= 1 => {
                    if let Value::Name(n) = &items[k - 1].value
                        && !items[k - 1].executable
                    {
                        skip.insert(k);
                        if self.names.insert(n.id())
                            && let Some(b) = self.it.load_id(n.id())
                        {
                            self.todo.push((b, Some(n.text_rc())));
                        }
                    }
                }
                Some(op @ ("if" | "ifelse")) => {
                    let n = if op == "if" { 1 } else { 2 };
                    if k < n || !items[k - n..k].iter().all(proc) {
                        continue;
                    }
                    let mut g = k - n;
                    let negate = g >= 1 && self.op_of(&items[g - 1]) == Some("not");
                    if negate {
                        g -= 1;
                    }
                    let Some(Object {
                        value: Value::Name(name),
                        executable: true,
                        ..
                    }) = g.checked_sub(1).map(|g| &items[g])
                    else {
                        continue;
                    };
                    let Some(Value::Boolean(b)) = self.it.load_id(name.id()).map(|o| o.value)
                    else {
                        continue;
                    };
                    let taken = b != negate;
                    // `if`: the branch, run when taken. `ifelse`: the first
                    // when taken, else the second.
                    let dead = match (n, taken) {
                        (1, true) => continue,
                        (1, false) | (2, true) => k - 1,
                        _ => k - 2,
                    };
                    skip.insert(dead);
                    self.deferred
                        .push((name.id(), items[dead].clone(), at.clone()));
                }
                _ => {}
            }
        }
        skip
    }
}

fn is_scalar(o: &Object) -> bool {
    matches!(
        o.value,
        Value::Integer(_) | Value::Real(_) | Value::Boolean(_) | Value::Mark | Value::Null
    )
}
