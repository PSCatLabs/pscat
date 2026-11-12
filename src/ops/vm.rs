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

/// `- vmstatus level used maximum`. The level is real; the byte
/// counts are fixed fictions (found code wants "is there free VM",
/// and 15MB of headroom says yes) — real accounting would be theater.
fn vmstatus(it: &mut Interp) -> Result<(), PsError> {
    it.push(Object::int(it.save_level() as i64));
    it.push(Object::int(1_000_000));
    it.push(Object::int(16_000_000));
    Ok(())
}
