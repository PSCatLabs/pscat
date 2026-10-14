//! Output and interpreter-control operators: the pieces that make a REPL
//! and `--eval` mode actually demonstrable.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "=", print_text);
    op(dict, "==", print_syntactic);
    op(dict, "stack", stack_dump);
    op(dict, "pstack", pstack_dump);
    op(dict, "print", print_string);
    op(dict, "quit", quit);
    op(dict, "handleerror", handleerror);
    // Access-control attributes are not modeled (nothing enforces
    // them), but the operators must exist as identities: every real
    // Type 1 font says `executeonly`/`noaccess` on its way in.
    op(dict, "executeonly", access_noop);
    op(dict, "readonly", access_noop);
    op(dict, "noaccess", access_noop);
    op(dict, "rcheck", access_check);
    op(dict, "wcheck", access_check);
}

/// Fill `errordict` (issue #372). Ghostscript's has a handler per error
/// name, and found code re-raises a caught error with
/// `errordict /rangecheck get exec`. Without the entries that lookup is
/// itself an `undefined` error, so the re-raise reported `undefined`
/// where Ghostscript keeps the name.
///
/// Only names the interpreter can raise get an entry: Ghostscript has
/// 14 more (`stackoverflow`, `VMerror`, `invalidaccess`, ...), but a
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
        h_invalidfileaccess => "invalidfileaccess",
        h_undefinedfilename => "undefinedfilename",
        h_syntaxerror => "syntaxerror",
        h_limitcheck => "limitcheck",
        h_ioerror => "ioerror",
        h_invalidrestore => "invalidrestore",
        h_undefinedresource => "undefinedresource",
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
/// operands when it fails (HANDOFF.md), so after a catch the stack is
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

fn access_noop(it: &mut Interp) -> Result<(), PsError> {
    // operand ... operand: identity, but stackunderflow still applies.
    let obj = it.pop()?;
    it.push(obj);
    Ok(())
}

/// Everything reads/writes as permitted here; report true.
fn access_check(it: &mut Interp) -> Result<(), PsError> {
    it.pop()?;
    it.push(Object::bool(true));
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
            d.get("errorname").map(|o| o.text()),
            d.get("command").map(|o| o.text()),
        )
    };
    eprintln!(
        "%%[ Error: {}; OffendingCommand: {} ]%%",
        name.unwrap_or_else(|| "--none--".to_string()),
        command.unwrap_or_else(|| "--none--".to_string())
    );
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
    let line = format!("{}\n", obj.repr());
    out(it, line.as_bytes(), false)
}

/// Print the whole stack, top first, without disturbing it (PLRM `stack`).
/// One item at a time: a stack of many references to one big string must
/// not be expanded into a single buffer first.
fn stack_dump(it: &mut Interp) -> Result<(), PsError> {
    for i in (0..it.ostack.len()).rev() {
        let line = format!("{}\n", it.ostack[i].text());
        out(it, line.as_bytes(), false)?;
    }
    Ok(())
}

fn pstack_dump(it: &mut Interp) -> Result<(), PsError> {
    for i in (0..it.ostack.len()).rev() {
        let line = format!("{}\n", it.ostack[i].repr());
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
    it.quit_requested = true;
    Ok(())
}
