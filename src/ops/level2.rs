//! Level 2 odds and ends — Stage 8 task 5: packedarray, the clocks,
//! languagelevel, and resource-category basics; plus the PLRM's
//! interpreter-identity operators `product`, `version`, `revision` and
//! `serialnumber` (issue #1048).
//!
//! The user-object operators (`defineuserobject`, `execuserobject`,
//! `undefineuserobject`, issue #1265) follow Ghostscript's own PostScript
//! definitions (`gs_dps2.ps`), which the tests pin run for run.
//!
//! Honest fictions, documented: packed arrays are ordinary arrays
//! (access control is identities throughout — AGENTS/NOTES), the
//! packing flag is tracked but changes nothing about how procedures
//! are built, and the resource machinery covers the Font category
//! (delegating to findfont/definefont, sharing FontDirectory) plus a
//! generic per-category registry for everything else. Resource status
//! and size follow Ghostscript (`resourcestatus`).

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
    op(dict, "defineuserobject", defineuserobject);
    op(dict, "execuserobject", execuserobject);
    op(dict, "undefineuserobject", undefineuserobject);
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
    let mut packed = Object::array(items).into_readonly();
    packed.packed = true;
    it.push(packed);
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
    // A Form is a dictionary (`/InstanceType /dicttype`, #1271). The
    // operands are put back by the ledger.
    if &*category == "Form" && !matches!(instance.value, Value::Dict(_)) {
        return Err(PsError::Typecheck);
    }
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

/// key category resourcestatus → status size true | false.
///
/// `status` follows Ghostscript, whose answers the tests pin: 0 for an
/// instance in VM, 2 for a Font that is not in VM but that `findfont` can
/// load (a built-in, standard-35 or catalog/pack face), and `size` -1,
/// "unknown" per the PLRM. A face asked for by name that nothing
/// provides, or whose font pack is not installed (`findfont` would hand
/// back a stand-in), is `false`, so a program can test for a face before
/// it sets text (issue #1214). Deviation: Ghostscript reports a face
/// `findfont` has already loaded as status 1 with its file size; here it
/// is in `FontDirectory`, so status 0, size -1.
fn resourcestatus(it: &mut Interp) -> Result<(), PsError> {
    let category = pop_category(it)?;
    let key = it.pop()?;
    let status = if &*category == "Font" {
        let dir = super::font::font_directory(it)?;
        if dir.borrow().known(&key)? {
            Some(0)
        } else {
            // Not defined yet: available when findfont would load it. The
            // same lookup findfont uses (`font::locate`), so the two agree.
            let name = match &key.value {
                Value::Name(n) => Some(n.to_string()),
                Value::String(s) => Some(s.text()),
                _ => None,
            };
            name.and_then(|n| crate::font::locate(&n)).map(|_| 2)
        }
    } else {
        let d = it.resource_category(&category);
        d.borrow().known(&key)?.then_some(0)
    };
    match status {
        Some(status) => {
            it.push(Object::int(status));
            it.push(Object::int(-1));
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
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

// --- user objects (#1265) ------------------------------------------------
//
// `UserObjects` is an array in `userdict` that a program fills by index.
// The behaviour below is Ghostscript 10.07.1's (`gs_dps2.ps`, run for run
// in `tests/user_objects.rs`), which duplicates Adobe's array growth.

/// Ghostscript's cap: a larger index is `rangecheck` (its source says the
/// test suite wants `rangecheck` where `.localvmarray` would give
/// `limitcheck`). It also bounds the array at 131070 slots, so a hostile
/// index cannot ask for a huge allocation.
const MAX_USER_OBJECT_INDEX: i64 = 65535;

/// The array starts with at least this many slots.
const MIN_USER_OBJECTS: usize = 10;

/// Elements copied per step charged: growing the array copies it all.
const COPY_PER_STEP: usize = 64;

const USER_OBJECTS: &str = "UserObjects";

/// The reference `/UserObjects` holds (its write flag is on it) and the
/// array under it, or the error Ghostscript gives for a missing or
/// non-array one (`undefined`, `typecheck`).
fn user_objects(it: &Interp) -> Result<(Object, crate::object::PsArray), PsError> {
    let found = it.userdict().borrow().get(USER_OBJECTS);
    match found {
        None => Err(PsError::Undefined(USER_OBJECTS.to_string())),
        Some(o) if matches!(o.value, Value::Array(_)) => {
            let Value::Array(a) = o.value.clone() else {
                return Err(PsError::Typecheck);
            };
            Ok((o, a))
        }
        // Ghostscript reads a string here too (`get` takes one); a
        // program that replaced /UserObjects with a non-array is the
        // only way to see it, so it is a `typecheck` here (notes/1265).
        Some(_) => Err(PsError::Typecheck),
    }
}

/// The slot index of an integer operand: a non-integer is `typecheck`,
/// checked before `UserObjects` is looked at, as Ghostscript does.
fn slot_index(idx: &Object) -> Result<i64, PsError> {
    match idx.value {
        Value::Integer(i) => Ok(i),
        _ => Err(PsError::Typecheck),
    }
}

/// What the memory limit is asked for each table slot of a userdict
/// snapshot: the entry, its key and a control byte, doubled for the load
/// factor. An estimate, as the snapshot is a `HashMap` clone.
const DICT_SLOT_BYTES: usize = 2 * (std::mem::size_of::<Object>() + 8 + 1);

/// Refuse work before any of it is done: `bytes` of new heap against
/// `--max-memory` and `slots` slots copied or initialised against
/// `--max-steps` (one step per 64) and `--max-time`. It runs ahead of every
/// mutation and install, so a refusal leaves `UserObjects`, userdict and
/// (through the operand ledger) the operand stack as they were.
fn preflight(it: &mut Interp, bytes: usize, slots: usize) -> Result<(), PsError> {
    if bytes == 0 && slots == 0 {
        return Ok(());
    }
    it.check_alloc(bytes)?;
    it.poll_limits((slots / COPY_PER_STEP + 1) as u64)
}

/// An array of `n` nulls with `slot` set to `value` when it is in range,
/// holding the first `n` slots of `old` in front. The memory it takes, the
/// first snapshot of userdict that installing it makes inside a `save`, and
/// the slots copied and initialised are all refused here, before the array
/// is made. It is made at its final capacity in one allocation (a copy
/// that was then resized would take up to twice the room it was checked for).
fn new_user_objects(
    it: &mut Interp,
    old: Option<&crate::object::PsArray>,
    n: usize,
    slot: i64,
    value: &Object,
) -> Result<Object, PsError> {
    let dict_slots = it.dict_snapshot_slots(&it.userdict());
    preflight(
        it,
        n.saturating_mul(std::mem::size_of::<Object>())
            .saturating_add(dict_slots.saturating_mul(DICT_SLOT_BYTES)),
        n.saturating_add(dict_slots),
    )?;
    let mut items = Vec::with_capacity(n);
    if let Some(a) = old {
        a.with_slice(|s| items.extend_from_slice(&s[..s.len().min(n)]));
    }
    items.resize(n, Object::lit(Value::Null));
    if let Ok(s) = usize::try_from(slot)
        && s < n
    {
        items[s] = value.clone();
    }
    Ok(Object::array(items))
}

/// Replace `/UserObjects` in userdict (a growth, or the first creation).
fn install_user_objects(it: &mut Interp, array: Object) {
    let userdict = it.userdict();
    it.journal_dict(&userdict);
    userdict.borrow_mut().put(USER_OBJECTS.into(), array);
}

/// The first write to the array's backing store at a save level
/// snapshots all of it (the journal's design), however small the view the
/// program holds: up to 655350 slots, copied again by every
/// `save ... restore` round. Refused against the limits before it is made;
/// free with no save live and for a store already journalled at this level.
fn charge_snapshot(it: &mut Interp, a: &crate::object::PsArray) -> Result<(), PsError> {
    let slots = it.array_snapshot_slots(a);
    if slots == 0 {
        return Ok(());
    }
    preflight(
        it,
        slots.saturating_mul(std::mem::size_of::<Object>()),
        slots,
    )
}

/// index obj defineuserobject → –
///
/// Stores `obj` at `index` of userdict's `UserObjects` array, creating the
/// array (at least 10 slots) or growing it as Ghostscript does: to twice
/// the index when the index is the old length, otherwise to index + 1.
/// An index over 65535 is `rangecheck`; any operand error leaves the
/// operands on the stack.
fn defineuserobject(it: &mut Interp) -> Result<(), PsError> {
    let value = it.pop()?;
    let idx = it.pop()?;
    // Ghostscript compares before anything else, so a real index past the
    // cap is `rangecheck` too and any other non-integer is `typecheck`.
    let slot = match idx.value {
        Value::Integer(i) if i > MAX_USER_OBJECT_INDEX => return Err(PsError::Rangecheck),
        Value::Real(r) if r > MAX_USER_OBJECT_INDEX as f64 => {
            return Err(PsError::Rangecheck);
        }
        Value::Integer(i) => Some(i),
        Value::Real(_) => None,
        _ => return Err(PsError::Typecheck),
    };
    let existing = it.userdict().borrow().get(USER_OBJECTS);
    let Some(existing) = existing else {
        // First use. A real index is `typecheck` once its array is made
        // (it is made only when index + 1 does not pass 10, so that
        // Ghostscript's `max` hands back an integer).
        let size = match (slot, &idx.value) {
            (Some(i), _) => usize::try_from(i + 1).unwrap_or(0).max(MIN_USER_OBJECTS),
            (None, Value::Real(r)) if *r + 1.0 <= MIN_USER_OBJECTS as f64 => MIN_USER_OBJECTS,
            _ => return Err(PsError::Typecheck),
        };
        let array = new_user_objects(it, None, size, slot.unwrap_or(-1), &value)?;
        install_user_objects(it, array);
        // A negative or real index leaves the new array behind, as in
        // Ghostscript, whose `put` is what fails.
        return match slot {
            Some(i) if i >= 0 => Ok(()),
            Some(_) => Err(PsError::Rangecheck),
            None => Err(PsError::Typecheck),
        };
    };
    let Value::Array(a) = &existing.value else {
        return Err(PsError::Typecheck);
    };
    let Some(i) = slot else {
        // A real index is a `typecheck`, the array untouched, whether it
        // would grow or put. The one exception is a put into a read-only
        // array, which Ghostscript refuses first: `0.0`, `-1.0` and `2.0`
        // on three slots are `invalidaccess`, `3.0` (a growth) `typecheck`.
        if let Value::Real(r) = idx.value
            && r < a.len() as f64
        {
            existing.check_writable()?;
        }
        return Err(PsError::Typecheck);
    };
    let len = a.len() as i64;
    if len <= i {
        // Grow: copy into a new array, as Ghostscript's `.localvmarray` +
        // `copy`, and put the slot in before it is installed (nothing in
        // the journal beyond the userdict entry).
        let size = usize::try_from(if i == len { i * 2 } else { i + 1 }).unwrap_or(0);
        let array = new_user_objects(it, Some(a), size, i, &value)?;
        install_user_objects(it, array);
        return if usize::try_from(i).is_ok_and(|i| i < size) {
            Ok(())
        } else {
            // Only an empty array and index 0 (twice nothing is nothing).
            Err(PsError::Rangecheck)
        };
    }
    existing.check_writable()?;
    let Ok(s) = usize::try_from(i) else {
        return Err(PsError::Rangecheck);
    };
    charge_snapshot(it, a)?;
    it.journal_array(a);
    a.put(s, value)
}

/// The object at `index`, for `execuserobject`: `undefined` with no
/// `UserObjects`, `rangecheck` outside it.
fn user_object_at(it: &Interp, index: i64) -> Result<Object, PsError> {
    let (_, a) = user_objects(it)?;
    usize::try_from(index)
        .ok()
        .and_then(|i| a.get(i))
        .ok_or(PsError::Rangecheck)
}

/// index execuserobject → …
///
/// Runs the object `defineuserobject` stored at `index`, as `exec` would:
/// a procedure runs, a literal is pushed. A slot never defined holds
/// `null`, which is pushed.
fn execuserobject(it: &mut Interp) -> Result<(), PsError> {
    let idx = it.pop()?;
    let index = slot_index(&idx)?;
    let obj = user_object_at(it, index)?;
    // The refusals `exec` makes before the object moves to the execution
    // stack, so the index stays on the operand stack for them.
    if obj.executable && obj.noaccess {
        return Err(PsError::InvalidAccess);
    }
    it.exec_room(&obj)?;
    it.exec_object(obj)
}

/// index undefineuserobject → –
///
/// Empties the slot (it holds `null` again). The array does not shrink.
fn undefineuserobject(it: &mut Interp) -> Result<(), PsError> {
    let idx = it.pop()?;
    let index = slot_index(&idx)?;
    let (flagged, a) = user_objects(it)?;
    // Ghostscript's `put` checks the write flag before the index.
    flagged.check_writable()?;
    let slot = usize::try_from(index)
        .ok()
        .filter(|&i| i < a.len())
        .ok_or(PsError::Rangecheck)?;
    charge_snapshot(it, &a)?;
    it.journal_array(&a);
    a.put(slot, Object::lit(Value::Null))
}
