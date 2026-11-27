//! Control-flow operators. The looping ones don't recurse into the
//! interpreter — they push loop frames onto the execution stack, which is
//! what keeps deep iteration steppable (for live rendering) and immune to
//! Rust stack depth.

use crate::error::PsError;
use crate::interp::{ForControl, Interp};
use crate::object::{Dict, Object, PsArray, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "if", ps_if);
    op(dict, "ifelse", ifelse);
    op(dict, "exec", exec);
    op(dict, "repeat", repeat);
    op(dict, "loop", ps_loop);
    op(dict, "for", ps_for);
    op(dict, "exit", exit);
    op(dict, "stop", stop);
    op(dict, "stopped", stopped);
    op(dict, "countexecstack", countexecstack);
    op(dict, "execstack", execstack);
}

/// The loop operators require a procedure body specifically (not any
/// executable object) because the body runs repeatedly from a frame.
pub(crate) fn pop_proc(it: &mut Interp) -> Result<PsArray, PsError> {
    let obj = it.pop()?;
    match (&obj.value, obj.executable) {
        (Value::Array(body), true) => Ok(body.clone()),
        _ => Err(PsError::Typecheck),
    }
}

/// A procedure: what `if` and `ifelse` run. Anything else is a
/// `typecheck`, a literal array or an executable name included, as in
/// Ghostscript.
fn is_proc(o: &Object) -> bool {
    o.executable && matches!(o.value, Value::Array(_))
}

fn ps_if(it: &mut Interp) -> Result<(), PsError> {
    // Checked on the stack, in Ghostscript's order (the procedure's type,
    // then the depth, then the boolean), before anything is popped: a
    // failure leaves the operands where they were, and since nothing can
    // fail after, they skip the ledger (#1047).
    let cond = match it.operand_stack() {
        [] => return Err(PsError::StackUnderflow),
        [.., p] if !is_proc(p) => return Err(PsError::Typecheck),
        [_] => return Err(PsError::StackUnderflow),
        [.., c, _] => match c.value {
            Value::Boolean(b) => b,
            _ => return Err(PsError::Typecheck),
        },
    };
    if cond && let [.., p] = it.operand_stack() {
        it.exec_room(p)?;
    }
    let proc = it.pop_settled();
    it.pop_settled();
    match proc {
        Some(proc) if cond => it.exec_object(proc),
        _ => Ok(()),
    }
}

fn ifelse(it: &mut Interp) -> Result<(), PsError> {
    let cond = match it.operand_stack() {
        [] => return Err(PsError::StackUnderflow),
        [.., f] if !is_proc(f) => return Err(PsError::Typecheck),
        [_] => return Err(PsError::StackUnderflow),
        [.., t, _] if !is_proc(t) => return Err(PsError::Typecheck),
        [_, _] => return Err(PsError::StackUnderflow),
        [.., c, _, _] => match c.value {
            Value::Boolean(b) => b,
            _ => return Err(PsError::Typecheck),
        },
    };
    if let [.., t, f] = it.operand_stack() {
        it.exec_room(if cond { t } else { f })?;
    }
    let proc_false = it.pop_settled();
    let proc_true = it.pop_settled();
    it.pop_settled();
    match if cond { proc_true } else { proc_false } {
        Some(proc) => it.exec_object(proc),
        None => Ok(()),
    }
}

pub(crate) fn exec(it: &mut Interp) -> Result<(), PsError> {
    // Nothing can fail before the object runs: no ledger (#1047).
    if let Some(o) = it.operand_stack().last() {
        // Refused before it moves to the execution stack, so it stays on
        // the operand stack as in Ghostscript (#1321).
        if o.executable && o.noaccess {
            return Err(PsError::InvalidAccess);
        }
        it.exec_room(o)?;
    }
    let obj = it.pop_settled().ok_or(PsError::StackUnderflow)?;
    it.exec_object(obj)
}

fn repeat(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    it.begin_repeat(body, n)
}

fn ps_loop(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    it.begin_loop(body)
}

fn ps_for(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    let limit = it.pop_num()?;
    let increment = it.pop_num()?;
    let initial = it.pop_num()?;
    it.begin_for(body, ForControl::new(initial, increment, limit))
}

fn exit(it: &mut Interp) -> Result<(), PsError> {
    it.exit_loop()
}

fn stop(it: &mut Interp) -> Result<(), PsError> {
    if it.do_stop() {
        it.push(Object::bool(true));
        return Ok(());
    }
    // No enclosing stopped: do_stop drained the exec stack, which ends
    // the program — the PLRM's outermost job-server context would do the
    // same. Nothing is pushed.
    //
    // But if an error is still pending in `$error`, ending *silently*
    // loses it (issue #142). Ghostscript's outermost wrapper reports it
    // instead, which is what makes the standard cleanup idiom usable:
    //
    //     { userproc } stopped { restore-my-state stop } if
    //
    // Without this, re-raising turned a hard error into no error at all
    // for every caller that did not itself catch, so a library wrapping
    // a caller's procedure had to choose between leaking its own state
    // and swallowing the failure. `lib/paintkit.ps`'s `pkwet` hit
    // exactly that.
    match it.take_top_level_stop_error() {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn stopped(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.begin_stopped()?;
    it.exec_object(obj)
}

/// `- countexecstack int` (#1468): the length of the array `execstack`
/// would fill, so `countexecstack array execstack` always fits.
fn countexecstack(it: &mut Interp) -> Result<(), PsError> {
    let (entries, _) = it.exec_stack_shape()?;
    it.push(Object::int(entries as i64));
    Ok(())
}

/// `array execstack subarray` (#1468): the execution stack, bottom first,
/// in the first slots of `array` (`Interp::write_exec_stack` says what each
/// frame shows). Everything that can fail is checked, and the copies and
/// the per-entry steps charged, before the operand is taken, so an error
/// or a stop leaves the stack and the array as they were.
fn execstack(it: &mut Interp) -> Result<(), PsError> {
    let (a, target) = stack_copy_target(it)?;
    let (entries, bytes) = it.exec_stack_shape()?;
    check_room(&target, &a, entries)?;
    it.check_alloc(bytes)?;
    it.poll_limits(entries as u64)?;
    it.pop_settled();
    it.journal_array(&a);
    it.forget_positions(&a);
    it.write_exec_stack(&a, entries)?;
    push_filled(it, &a, target, entries)
}

/// The array operand of `execstack` and `dictstack`, left on the stack:
/// `stackunderflow`, or `typecheck` for anything but an array.
pub(crate) fn stack_copy_target(it: &Interp) -> Result<(PsArray, Object), PsError> {
    let target = it.operand_stack().last().ok_or(PsError::StackUnderflow)?;
    match &target.value {
        Value::Array(a) => Ok((a.clone(), target.clone())),
        _ => Err(PsError::Typecheck),
    }
}

/// gs 10.07.1's order: a short array is a `rangecheck` even when it is
/// also read-only (`1 array readonly execstack`); then `invalidaccess`.
pub(crate) fn check_room(target: &Object, a: &PsArray, entries: usize) -> Result<(), PsError> {
    if entries > a.len() {
        return Err(PsError::Rangecheck);
    }
    target.check_writable()
}

/// Push the filled part of `a`, with the operand's attributes.
pub(crate) fn push_filled(
    it: &mut Interp,
    a: &PsArray,
    target: Object,
    entries: usize,
) -> Result<(), PsError> {
    let filled = a.interval(0, entries)?;
    it.push(Object {
        value: Value::Array(filled),
        ..target
    });
    Ok(())
}
