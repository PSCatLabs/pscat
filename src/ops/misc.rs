//! Output and interpreter-control operators: the pieces that make a REPL
//! and `--eval` mode actually demonstrable.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{
    Dict, ERROR_NAME_LIMIT, OPERATOR_LIMITS, OPERATOR_STACK_BYTES, Object, TRUNCATED, Value,
    cap_text,
};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "=", print_text);
    op(dict, "==", print_syntactic);
    op(dict, "stack", stack_dump);
    op(dict, "pstack", pstack_dump);
    op(dict, "print", print_string);
    op(dict, "quit", quit);
    op(dict, "handleerror", handleerror);
    // Access is modelled for arrays only, and only the write half (#1189):
    // `readonly`, `executeonly` and `noaccess` make an array reference
    // unwritable (`put`, `putinterval`, `astore` and `copy` then raise
    // `invalidaccess`, and `bind` leaves it alone). Reading is never
    // refused, and every other type passes through unchanged, as every
    // real Type 1 font needs: it says `executeonly`/`noaccess` on its way in.
    op(dict, "executeonly", access_restrict);
    op(dict, "readonly", access_restrict);
    op(dict, "noaccess", access_restrict);
    op(dict, "rcheck", access_check);
    op(dict, "wcheck", wcheck);
}

/// Fill `errordict` (issue #372). Ghostscript's has a handler per error
/// name, and found code re-raises a caught error with
/// `errordict /rangecheck get exec`. Without the entries that lookup is
/// itself an `undefined` error, so the re-raise reported `undefined`
/// where Ghostscript keeps the name.
///
/// Only names the interpreter can raise get an entry: Ghostscript has
/// 13 more (`stackoverflow`, `VMerror`, ...), but a
/// program can only meet them by catching one, and pscat never raises
/// them.
pub(crate) fn install_errordict(dict: &mut Dict) {
    use super::op;
    macro_rules! handlers {
        ($($f:ident => $name:literal),* $(,)?) => {$(
            fn $f(it: &mut Interp) -> Result<(), PsError> {
                reraise(it, $name)
            }
            op(dict, $name, $f);
        )*};
    }
    handlers! {
        h_stackunderflow => "stackunderflow",
        h_execstackoverflow => "execstackoverflow",
        h_typecheck => "typecheck",
        h_rangecheck => "rangecheck",
        h_undefined => "undefined",
        h_undefinedresult => "undefinedresult",
        h_unmatchedmark => "unmatchedmark",
        h_nocurrentpoint => "nocurrentpoint",
        h_invalidexit => "invalidexit",
        h_dictstackunderflow => "dictstackunderflow",
        h_invalidfont => "invalidfont",
        h_invalidaccess => "invalidaccess",
        h_invalidfileaccess => "invalidfileaccess",
        h_undefinedfilename => "undefinedfilename",
        h_syntaxerror => "syntaxerror",
        h_limitcheck => "limitcheck",
        h_ioerror => "ioerror",
        h_invalidrestore => "invalidrestore",
        h_undefinedresource => "undefinedresource",
        h_configurationerror => "configurationerror",
    }
    op(dict, "handleerror", handleerror);
}

/// What an `errordict` handler does when run by hand: raise `name`
/// again, with the top operand as the offending command (the PLRM has
/// the interpreter push that object before running the handler). The
/// raised error is the real one, so an enclosing `stopped` records it in
/// `$error` and an uncaught one is reported, both under `name`.
///
/// Deviation, on an empty operand stack: Ghostscript dies with an
/// unrecoverable `stackunderflow`. pscat does not restore an operator's
/// operands when it fails (docs/dev/HANDOFF.md), so after a catch the stack is
/// often empty where Ghostscript's holds the failed operands. The
/// handler then falls back to the command `$error` already holds, which
/// keeps the error name, the thing a re-raise exists to preserve.
fn reraise(it: &mut Interp, name: &'static str) -> Result<(), PsError> {
    let command = it.ostack.pop().or_else(|| {
        it.load("$error").and_then(|e| match &e.value {
            Value::Dict(d) => d.borrow().get("command"),
            _ => None,
        })
    });
    let text = command.as_ref().map(|c| c.text());
    if let Some(c) = command {
        it.set_error_command(c);
    }
    Err(PsError::from_name(name, text).unwrap_or(PsError::Undefined(name.to_string())))
}


/// `readonly`, `executeonly`, `noaccess`: the reference loses write
/// access if it is an array and is unchanged otherwise.
fn access_restrict(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.push(obj.into_readonly());
    Ok(())
}

/// `rcheck` is always true (reads are never refused). `wcheck` is false for
/// a read-only array and true for everything else; Ghostscript raises
/// `typecheck` on a name or an operator, pscat answers true.
fn access_check(it: &mut Interp) -> Result<(), PsError> {
    it.pop()?;
    it.push(Object::bool(true));
    Ok(())
}

fn wcheck(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    it.push(Object::bool(!obj.is_readonly()));
    Ok(())
}

/// The default error reporter: prints from $error the way the CLI does,
/// then clears newerror so the next error is distinguishable.
fn handleerror(it: &mut Interp) -> Result<(), PsError> {
    let Some(obj) = it.load("$error") else {
        return Ok(());
    };
    let Value::Dict(d) = &obj.value else {
        return Ok(());
    };
    let (name, command) = {
        let d = d.borrow();
        (
            d.get("errorname")
                .map(|o| cap_text(&o.text(), ERROR_NAME_LIMIT)),
            d.get("command")
                .map(|o| cap_text(&o.text(), ERROR_NAME_LIMIT)),
        )
    };
    let line = format!(
        "%%[ Error: {}; OffendingCommand: {} ]%%\n",
        name.unwrap_or_else(|| "--none--".to_string()),
        command.unwrap_or_else(|| "--none--".to_string())
    );
    let _ = it.write_stderr(line.as_bytes(), false);
    let d = d.clone();
    it.journal_dict(&d);
    d.borrow_mut().put("newerror".into(), Object::bool(false));
    Ok(())
}

fn print_text(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let line = format!("{}\n", obj.text());
    out(it, line.as_bytes(), false)
}

fn print_syntactic(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let line = format!("{}\n", obj.repr_bounded(&OPERATOR_LIMITS));
    out(it, line.as_bytes(), false)
}

/// Print the whole stack, top first, without disturbing it (PLRM `stack`).
/// One item at a time: a stack of many references to one big string must
/// not be expanded into a single buffer first. Bounded (#745): see
/// `print_stack_bounded`.
fn stack_dump(it: &mut Interp) -> Result<(), PsError> {
    print_stack_bounded(it, |o| o.text_bounded(OPERATOR_LIMITS.max_string))
}

fn pstack_dump(it: &mut Interp) -> Result<(), PsError> {
    print_stack_bounded(it, |o| o.repr_bounded(&OPERATOR_LIMITS))
}

/// One line per operand, top first, each rendered by `render` (already
/// bounded per item). Stops after `OPERATOR_STACK_BYTES` with one
/// `... truncated` line for the items below.
fn print_stack_bounded(it: &mut Interp, render: fn(&Object) -> String) -> Result<(), PsError> {
    let mut used = 0usize;
    for i in (0..it.ostack.len()).rev() {
        if used >= OPERATOR_STACK_BYTES {
            let line = format!("{TRUNCATED}\n");
            return out(it, line.as_bytes(), false);
        }
        let line = format!("{}\n", render(&it.ostack[i]));
        used += line.len();
        out(it, line.as_bytes(), false)?;
    }
    Ok(())
}

fn print_string(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::String(s) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let bytes = s.borrow_bytes().to_vec();
    out(it, &bytes, true)
}

/// Program output goes through the interpreter, which sends it to stdout
/// or, in the browser build, to a buffer the page reads (issue #252).
fn out(it: &mut Interp, bytes: &[u8], flush: bool) -> Result<(), PsError> {
    it.write_stdout(bytes, flush).map_err(|_| PsError::Io)
}

fn quit(it: &mut Interp) -> Result<(), PsError> {
    // Buffered disk files go out now (issue #762); a failure is kept on the
    // file and reported by its own `closefile`.
    let _ = it.flush_files();
    it.quit_requested = true;
    Ok(())
}
