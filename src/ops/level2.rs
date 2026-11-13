//! Level 2 odds and ends — Stage 8 task 5: packedarray, the clocks,
//! languagelevel, and resource-category basics; plus the PLRM's
//! interpreter-identity operators `product`, `version`, `revision` and
//! `serialnumber` (issue #1048).
//!
//! Honest fictions, documented: packed arrays are ordinary arrays
//! (access control is identities throughout — AGENTS/NOTES), the
//! packing flag is tracked but changes nothing about how procedures
//! are built, and the resource machinery covers the Font category
//! (delegating to findfont/definefont, sharing FontDirectory) plus a
//! generic per-category registry for everything else.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "packedarray", packedarray);
    op(dict, "setpacking", setpacking);
    op(dict, "currentpacking", currentpacking);
    op(dict, "usertime", usertime);
    op(dict, "realtime", realtime);
    op(dict, "languagelevel", languagelevel);
    op(dict, "product", product);
    op(dict, "version", version);
    op(dict, "revision", revision);
    op(dict, "serialnumber", serialnumber);
    op(dict, "defineresource", defineresource);
    op(dict, "findresource", findresource);
    op(dict, "resourcestatus", resourcestatus);
    op(dict, "undefineresource", undefineresource);
}

/// any0..anyn-1 n packedarray → array. Packed arrays here are plain
/// (read-only in spirit; access control is not modeled).
fn packedarray(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    // The operands come off the stack, so a count past its depth is an
    // underflow; checked first, `with_capacity` below would otherwise try
    // to reserve `n` slots (found by fuzzing, issue #604: a huge `n` panicked
    // with "capacity overflow").
    if n as usize > it.operand_stack().len() {
        return Err(PsError::StackUnderflow);
    }
    let mut items = Vec::with_capacity(n as usize);
    for _ in 0..n {
        items.push(it.pop()?);
    }
    items.reverse();
    // A packed array is read-only, as in Ghostscript (#1189).
    it.push(Object::array(items).into_readonly());
    Ok(())
}

fn setpacking(it: &mut Interp) -> Result<(), PsError> {
    let Value::Boolean(b) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    it.packing = b;
    Ok(())
}

fn currentpacking(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::bool(it.packing));
    Ok(())
}

/// Milliseconds of interpreter run time, per the PLRM. (On wasm32
/// there is no clock; both timers read 0 — see `src/clock.rs`.)
fn usertime(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.clock.elapsed_ms()));
    Ok(())
}

/// Milliseconds since an arbitrary epoch (we use the Unix epoch).
fn realtime(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(crate::clock::Clock::wall_ms()));
    Ok(())
}

fn languagelevel(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(2));
    Ok(())
}

/// – product → `(pscat)`: the interpreter's name.
fn product(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::string(b"pscat".to_vec()));
    Ok(())
}

/// – version → the crate version, e.g. `(0.7.0)` or `(0.7.0-rc.4)`. The
/// PLRM only says "a string"; pscat's own version is the useful one,
/// since it is what a program gates a pscat feature on.
fn version(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::string(
        env!("CARGO_PKG_VERSION").as_bytes().to_vec(),
    ));
    Ok(())
}

/// The crate version as one comparable integer, `major*10000 +
/// minor*100 + patch` (0.7.0 → 700, 1.2.3 → 10203). A pre-release tag is
/// dropped, so 0.7.0-rc.4 is 700 like 0.7.0: test `revision 700 ge` for
/// "0.7.0 or later". Two digits each for minor and patch, which holds
/// while both stay under 100.
const REVISION: i64 = {
    const fn parse(s: &str) -> i64 {
        let b = s.as_bytes();
        let mut n = 0;
        let mut i = 0;
        while i < b.len() {
            n = n * 10 + (b[i] - b'0') as i64;
            i += 1;
        }
        n
    }
    parse(env!("CARGO_PKG_VERSION_MAJOR")) * 10000
        + parse(env!("CARGO_PKG_VERSION_MINOR")) * 100
        + parse(env!("CARGO_PKG_VERSION_PATCH"))
};

/// – revision → int: `REVISION`.
fn revision(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(REVISION));
    Ok(())
}

/// – serialnumber → 0. There is no device serial number to report; the
/// PLRM allows any integer, and 0 says "none".
fn serialnumber(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(0));
    Ok(())
}

fn pop_category(it: &mut Interp) -> Result<Rc<str>, PsError> {
    match &it.pop()?.value {
        Value::Name(n) => Ok(n.text_rc()),
        _ => Err(PsError::Typecheck),
    }
}

/// key instance category defineresource → instance.
fn defineresource(it: &mut Interp) -> Result<(), PsError> {
    let category = pop_category(it)?;
    if &*category == "Font" {
        // Same registry, same FID plumbing as definefont.
        return super::font::definefont(it);
    }
    let instance = it.pop()?;
    let key = it.pop()?;
    let d = it.resource_category(&category);
    it.journal_dict(&d);
    d.borrow_mut().put_obj(&key, instance.clone())?;
    it.push(instance);
    Ok(())
}

/// key category findresource → instance.
fn findresource(it: &mut Interp) -> Result<(), PsError> {
    let category = pop_category(it)?;
    if &*category == "Font" {
        // findfont semantics, substitution included.
        return super::font::findfont(it);
    }
    let key = it.pop()?;
    let d = it.resource_category(&category);
    let found = d.borrow().get_obj(&key)?;
    match found {
        Some(o) => {
            it.push(o);
            Ok(())
        }
        None => Err(PsError::UndefinedResource),
    }
}

/// key category resourcestatus → status size true | false. Status and
/// size are fictions (0 0), like vmstatus's byte counts.
fn resourcestatus(it: &mut Interp) -> Result<(), PsError> {
    let category = pop_category(it)?;
    let key = it.pop()?;
    let known = if &*category == "Font" {
        let dir = super::font::font_directory(it)?;
        let in_dir = dir.borrow().known(&key)?;
        // Builtin faces materialize on demand — report them present.
        in_dir
            || match &key.value {
                Value::Name(n) => crate::font::builtin_index(n).is_some(),
                Value::String(s) => crate::font::builtin_index(&s.text()).is_some(),
                _ => false,
            }
    } else {
        let d = it.resource_category(&category);
        d.borrow().known(&key)?
    };
    if known {
        it.push(Object::int(0));
        it.push(Object::int(0));
        it.push(Object::bool(true));
    } else {
        it.push(Object::bool(false));
    }
    Ok(())
}

/// key category undefineresource —
fn undefineresource(it: &mut Interp) -> Result<(), PsError> {
    let category = pop_category(it)?;
    let key = it.pop()?;
    let d = if &*category == "Font" {
        super::font::font_directory(it)?
    } else {
        it.resource_category(&category)
    };
    it.journal_dict(&d);
    d.borrow_mut().undef(&key)
}

/// Shared with `Interp::resource_category` — the per-category dicts
/// live on the interpreter so they survive dict-stack churn.
pub(crate) type CategoryMap = std::collections::HashMap<Rc<str>, Rc<RefCell<Dict>>>;
