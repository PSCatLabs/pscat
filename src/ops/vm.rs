//! save / restore / vmstatus — VM snapshot semantics. The design (and
//! the gs pins behind every choice here) is `docs/dev/VM.md`; the machinery
//! lives on `Interp` (`do_save`/`do_restore`, the COW journal).
//!
//! `currentglobal`/`setglobal` (Level 2) keep the VM allocation mode as a
//! flag only: pscat has one VM, so nothing is allocated elsewhere, but the
//! bracketed idiom `currentglobal true setglobal ... setglobal` reads back
//! what it set, and `restore` treats the mode as Ghostscript does (see
//! `VmMode`).
//!
//! Documented deviation: no invalidrestore scan of the stacks for
//! composites created since the save — under `Rc` such objects simply
//! stay valid, so programs that would error on Adobe render here.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};
use std::rc::Rc;

/// The VM allocation mode and what `save` recorded of it (issue #1146).
///
/// Ghostscript's `restore` does not reset the mode to the saved value as
/// the PLRM describes: it only ever drops to local. After `restore` the
/// mode is global only if it is global now AND it was global at every
/// `save` the restore discards (measured with gs 10.07.1: `true setglobal
/// save false setglobal restore` leaves it false, `true setglobal save
/// true setglobal restore` true, `false setglobal save true setglobal
/// restore` false, and restoring an outer save over an inner one saved
/// while local gives local). That is what is implemented. Kept outside
/// `SaveRecord` so the mode needs no change to the save machinery.
#[derive(Default)]
pub(crate) struct VmMode {
    /// `currentglobal`: `false` (local) at start.
    global: bool,
    /// `(save level after the save, mode at the save)`, innermost last.
    /// Keyed by level so a save made by any other route cannot misalign it.
    saved: Vec<(usize, bool)>,
}

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "save", save);
    op(dict, "restore", restore);
    op(dict, "vmstatus", vmstatus);
    op(dict, "currentglobal", currentglobal);
    op(dict, "setglobal", setglobal);
    op(dict, "gcheck", gcheck);
    op(dict, "scheck", scheck);
    op(dict, "internaldict", internaldict);
}

fn save(it: &mut Interp) -> Result<(), PsError> {
    let s = it.do_save();
    let level = it.save_level();
    let mode = &mut it.vm_mode;
    mode.saved.retain(|&(l, _)| l < level);
    mode.saved.push((level, mode.global));
    it.push(s);
    Ok(())
}

fn restore(it: &mut Interp) -> Result<(), PsError> {
    let s = it.pop()?;
    it.do_restore(&s)?;
    // Only a restore that happened changes the mode (an `invalidrestore`
    // returned above leaves it alone).
    let level = it.save_level();
    let mode = &mut it.vm_mode;
    for &(l, was_global) in &mode.saved {
        if l > level {
            mode.global &= was_global;
        }
    }
    mode.saved.retain(|&(l, _)| l <= level);
    Ok(())
}

/// `- currentglobal bool`: the VM allocation mode, `false` (local) until
/// `setglobal` says otherwise.
fn currentglobal(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::bool(it.vm_mode.global));
    Ok(())
}

/// `bool setglobal -`: record the allocation mode. pscat has a single VM,
/// so this changes nothing about where objects live; programs that bracket
/// resource definitions with it get the answers they expect. `typecheck`
/// on a non-boolean leaves the mode as it was.
fn setglobal(it: &mut Interp) -> Result<(), PsError> {
    let flag = match it.pop()?.value {
        Value::Boolean(b) => b,
        _ => return Err(PsError::Typecheck),
    };
    it.vm_mode.global = flag;
    Ok(())
}

/// `any gcheck bool`: is the object in global VM, or simple (always true).
///
/// pscat has one VM, so a composite is not allocated anywhere in particular
/// and `setglobal` does not change that. The model: the composites that
/// stand in for global VM are `globaldict`, `systemdict` and
/// `GlobalFontDirectory` (true), every other string, array, dictionary and
/// gstate is local (false), whatever `currentglobal` said when it was made.
/// Ghostscript would answer `true` for one made while the mode was global;
/// that is the one place this differs (`tests/gcheck_globaldict.rs`). Simple
/// objects, names, operators, files and `save` objects are `true`, as in gs.
fn gcheck(it: &mut Interp) -> Result<(), PsError> {
    let o = it.pop()?;
    it.push(Object::bool(is_global(it, &o)));
    Ok(())
}

/// `any scheck bool` (#1469): is the object in shared VM. pscat has no
/// shared VM apart from the global one, and gs answers exactly as `gcheck`
/// does for every object (measured with 10.07.1), so it is `gcheck` under its
/// PLRM name.
fn scheck(it: &mut Interp) -> Result<(), PsError> {
    let o = it.pop()?;
    it.push(Object::bool(is_global(it, &o)));
    Ok(())
}

fn is_global(it: &Interp, o: &Object) -> bool {
    match &o.value {
        Value::String(_) | Value::Array(_) | Value::GState(_) => false,
        Value::Dict(d) => {
            Rc::ptr_eq(d, &it.globaldict)
                || Rc::ptr_eq(d, &it.systemdict())
                || Rc::ptr_eq(d, &it.plrm_dicts.global_fonts)
        }
        _ => true,
    }
}

/// The key `internaldict` opens for (PLRM 3rd ed. names no key; Adobe's
/// Type 1 font programs and gs use this one).
const INTERNALDICT_KEY: i64 = 1_183_615_869;

/// `int internaldict dict` (#1469): the dictionary a Type 1 font program
/// reaches with `1183615869 internaldict`. It is an empty local dictionary
/// that accepts `def`, the same one on every call, and runs nothing: gs
/// keeps a few font-procedure hooks in its own (`.execute_scheduled_inits`,
/// `CCRun`), pscat none. Any other key is `invalidaccess`, whatever its type
/// (not `typecheck`), and the key stays on the stack, as in gs. A real equal
/// to the key opens it too; gs also opens it for reals within single
/// precision of the key, which pscat does not.
fn internaldict(it: &mut Interp) -> Result<(), PsError> {
    let key = it.pop()?;
    let opens = match key.value {
        Value::Integer(i) => i == INTERNALDICT_KEY,
        Value::Real(r) => r == INTERNALDICT_KEY as f64,
        _ => false,
    };
    if !opens {
        return Err(PsError::InvalidAccess);
    }
    it.push(Object::lit(Value::Dict(it.plrm_dicts.internal.clone())));
    Ok(())
}

/// `- vmstatus level used maximum`. The level is real; the byte
/// counts are fixed fictions (found code wants "is there free VM",
/// and 15MB of headroom says yes) — real accounting would be theater.
fn vmstatus(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.save_level() as i64));
    it.push(Object::int(1_000_000));
    it.push(Object::int(16_000_000));
    Ok(())
}
