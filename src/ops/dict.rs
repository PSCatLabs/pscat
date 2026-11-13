//! Dictionary and definition operators, plus `bind`.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, PsArray, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "def", def);
    op(dict, "dict", make_dict);
    op(dict, "begin", begin);
    op(dict, "end", end);
    op(dict, "load", load);
    op(dict, "bind", bind);
    op(dict, "known", known);
    op(dict, "where", where_op);
    op(dict, "store", store);
    op(dict, "undef", undef);
    op(dict, "currentdict", currentdict);
    op(dict, "countdictstack", countdictstack);
    op(dict, "cleardictstack", cleardictstack);
    op(dict, "maxlength", maxlength);
    op(dict, ">>", dict_from_mark);
    // `<<` is just a mark, installed as a constant in ops::install_all.
}

/// `key value def`: any non-null key, per the PLRM (`put` has always taken
/// the same keys). Names -- by far the common case -- go straight to the
/// interned-id path; every other key (integer, real, boolean, string, ...)
/// takes the out-of-line general path in `Interp::define_key` so it adds no
/// work to the hot one (#323).
fn def(it: &mut Interp) -> Result<(), PsError> {
    let value = it.pop()?;
    let key = it.pop()?;
    if let Value::Name(n) = &key.value {
        it.define_name(n, value);
        return Ok(());
    }
    it.define_key(&key, value)
}

fn make_dict(it: &mut Interp) -> Result<(), PsError> {
    let capacity = it.pop_int()?;
    if capacity < 0 {
        return Err(PsError::Rangecheck);
    }
    // Capacity is a Level 1 fiction we don't enforce; Level 2 made
    // dictionaries grow automatically anyway.
    it.push(Object::lit(Value::Dict(Rc::new(RefCell::new(Dict::new())))));
    Ok(())
}

fn pop_dict_operand(it: &mut Interp) -> Result<Rc<RefCell<Dict>>, PsError> {
    match &it.pop()?.value {
        Value::Dict(d) => Ok(d.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn begin(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict_operand(it)?;
    it.push_dict(d);
    Ok(())
}

fn end(it: &mut Interp) -> Result<(), PsError> {
    it.pop_dict()
}

fn load(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let found = match &key.value {
        Value::Name(n) => it.load_id(n.id()),
        _ => it.load_key(&key)?,
    };
    let obj = found.ok_or_else(|| PsError::Undefined(key.text()))?;
    it.push(obj);
    Ok(())
}

fn known(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let d = pop_dict_operand(it)?;
    let found = d.borrow().known(&key)?;
    it.push(Object::bool(found));
    Ok(())
}

/// key where -> dict true (topmost defining dict) | false
fn where_op(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    match it.find_defining_dict(&key)? {
        Some(d) => {
            it.push(Object::lit(Value::Dict(d)));
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
    }
    Ok(())
}

/// key value store: replace in the topmost dict that defines key, else
/// define in the current dict.
fn store(it: &mut Interp) -> Result<(), PsError> {
    let value = it.pop()?;
    let key = it.pop()?;
    let d = match it.find_defining_dict(&key)? {
        Some(d) => d,
        None => it.current_dict(),
    };
    it.journal_dict(&d);
    d.borrow_mut().put_obj(&key, value)?;
    Ok(())
}

fn undef(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let d = pop_dict_operand(it)?;
    it.journal_dict(&d);
    d.borrow_mut().undef(&key)
}

fn currentdict(it: &mut Interp) -> Result<(), PsError> {
    let d = it.current_dict();
    it.push(Object::lit(Value::Dict(d)));
    Ok(())
}

fn countdictstack(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.dict_stack_len() as i64));
    Ok(())
}

fn cleardictstack(it: &mut Interp) -> Result<(), PsError> {
    it.clear_dict_stack();
    Ok(())
}

fn maxlength(it: &mut Interp) -> Result<(), PsError> {
    let d = pop_dict_operand(it)?;
    // Level 2: dictionaries grow, so maxlength just reports capacity
    // "at least current length"; length + slack matches Ghostscript's
    // spirit without modeling capacity.
    let n = d.borrow().len() as i64;
    it.push(Object::int(n.max(1)));
    Ok(())
}

/// `>>`: collect key/value pairs down to the mark into a new dict.
fn dict_from_mark(it: &mut Interp) -> Result<(), PsError> {
    let mut items = Vec::new();
    loop {
        let obj = it.pop().map_err(|_| PsError::UnmatchedMark)?;
        if matches!(obj.value, Value::Mark) {
            break;
        }
        items.push(obj);
    }
    if items.len() % 2 != 0 {
        return Err(PsError::Rangecheck);
    }
    let mut d = Dict::new();
    // `items` holds the operands top-of-stack-first, so popping walks
    // them in original order: key, value, key, value, ...
    while let (Some(key), Some(value)) = (items.pop(), items.pop()) {
        d.put_obj(&key, value)?;
    }
    it.push(Object::lit(Value::Dict(Rc::new(RefCell::new(d)))));
    Ok(())
}

fn bind(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    match (&obj.value, obj.executable) {
        (Value::Array(body), true) => {
            // A read-only procedure is left alone, names and all (PLRM
            // `bind`; Ghostscript agrees). The procedure itself stays as
            // writable as it was: only the nested procedures `bind`
            // walks into are made read-only (#1189).
            if !obj.is_readonly() {
                bind_proc(it, body, 0, &mut BindWalk::default())?;
            }
            it.push(obj);
            Ok(())
        }
        _ => Err(PsError::Typecheck),
    }
}

/// Units of work (`BindWalk::walked`) `bind` does between looks at the
/// limits (#1176).
const BIND_POLL: usize = 1 << 16;

/// One `bind` call's walk (#1176). `seen` is each procedure view already
/// bound, with the shallowest depth it was reached at: a procedure two
/// others share, or one that contains itself, is bound once instead of
/// once per path to it, which made `bind` exponential (a cycle of two
/// references walked 2^100 views). Binding is idempotent while the walk
/// runs (the dictionary stack does not change), so skipping a view seen at
/// the same or a shallower depth binds exactly what walking it again would.
#[derive(Default)]
struct BindWalk {
    seen: std::collections::HashMap<(usize, usize, usize), usize>,
    /// Work (elements, and dictionaries searched for names) since the last
    /// look at the limits.
    walked: usize,
}

/// Replace executable names that currently resolve to operators with the
/// operators themselves, recursing into nested procedures — the classic
/// "immune to later redefinition, and faster" transform.
fn bind_proc(
    it: &mut Interp,
    body: &PsArray,
    depth: usize,
    walk: &mut BindWalk,
) -> Result<(), PsError> {
    if depth > 100 {
        return Ok(());
    }
    match walk.seen.entry(body.identity()) {
        std::collections::hash_map::Entry::Occupied(mut e) => {
            if *e.get() <= depth {
                return Ok(());
            }
            e.insert(depth);
        }
        std::collections::hash_map::Entry::Vacant(e) => {
            e.insert(depth);
        }
    }
    // bind mutates VM contents like any put — journaled so restore
    // un-binds, matching Adobe's VM semantics.
    it.journal_array(body);
    let mut nested: Vec<PsArray> = Vec::new();
    // Distinct views can still overlap (every suffix of one array), and each
    // name is looked up through the whole dictionary stack, so the walk is
    // not linear in the VM in every case. It looks at the limits as it goes:
    // an element is one unit of work and a lookup one more per dictionary on
    // the stack, a step per 1024 units, as a file read is charged per KiB. A
    // limit stops the walk where it is (the elements after it are skipped).
    let lookup = 1 + it.dict_stack_len();
    let mut stopped: Option<PsError> = None;
    // for_each_mut declines (harmlessly) if the storage is already
    // borrowed — the self-referential case.
    body.for_each_mut(|el| {
        if stopped.is_some() {
            return;
        }
        walk.walked += 1;
        match (&el.value, el.executable) {
            (Value::Name(n), true) => {
                walk.walked += lookup;
                if let Some(resolved) = it.load(n)
                    && resolved.executable
                    && matches!(resolved.value, Value::Operator(_))
                {
                    *el = resolved;
                }
            }
            (Value::Array(inner), true) => {
                // A nested procedure is made read-only through this slot,
                // and bound, unless the slot already was read-only: `bind`
                // does not enter those (#1189). One this walk has bound
                // already is entered again (the `seen` check decides if it
                // needs a second look), since the walk itself set the flag.
                if !el.readonly || walk.seen.contains_key(&inner.identity()) {
                    nested.push(inner.clone());
                    el.readonly = true;
                }
            }
            _ => {}
        }
        if walk.walked >= BIND_POLL {
            if let Err(e) = it.poll_limits((walk.walked / 1024) as u64) {
                stopped = Some(e);
            }
            walk.walked %= 1024;
        }
    });
    if let Some(e) = stopped {
        return Err(e);
    }
    for inner in nested {
        bind_proc(it, &inner, depth + 1, walk)?;
    }
    Ok(())
}
