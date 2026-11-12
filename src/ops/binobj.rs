//! Level 2 binary object operators (issue #733).
//!
//! ```text
//! int setobjectformat -            % 0 off, 1 to 4 the byte order
//! - currentobjectformat int
//! obj tag printobject -            % to stdout
//! file obj tag writeobject -       % to an output file
//! ```
//!
//! The format is 0 (the operators refuse), 1 (IEEE, high byte first),
//! 2 (IEEE, low byte first), 3 and 4 (the same, "native"; a pscat host is
//! always little-endian IEEE, so they write exactly what 1 and 2 do, as
//! Ghostscript does). The default is 2 on every platform, which is what
//! Ghostscript starts with on a little-endian host. Anything else is
//! `rangecheck`. The format survives `save`/`restore`.
//!
//! `printobject` and `writeobject` write the binary object sequence of one
//! object; the tag (0 to 255) goes in the top-level object's tag byte. The
//! layout and the limits are described in `crate::binobj`. `writeobject`
//! needs an output file from `file` (so `--allow-write`, except for
//! `%stdout` and `%stderr`).
//!
//! Errors:
//! - `undefined`: the format is 0, as in Ghostscript.
//! - `typecheck`: a wrong operand type, a tag that is not an integer, or an
//!   object that has no binary form (a dictionary, a file, an operator...).
//! - `rangecheck`: a tag outside 0 to 255.
//! - `limitcheck`: an integer outside 32 bits, a real that binary32 cannot
//!   hold, a string, name or array over 65535 long, an empty name, nesting
//!   over 500 deep, or a sequence over 64 MiB (this ends a cyclic array).
//! - `ioerror`: the file is an input file or closed, or the OS refused.
//!
//! A real is written as binary32, so it loses precision: `0.1` comes back
//! as `0.10000000149011612`. Numbers such as `2.5` and `0.25` are exact.
//! A sequence holds at most 65535 elements per array, so a long numeric
//! array is better saved with `writereals` (#735).

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::Value;

pub fn install(dict: &mut crate::object::Dict) {
    super::op(dict, "setobjectformat", setobjectformat);
    super::op(dict, "currentobjectformat", currentobjectformat);
    super::op(dict, "printobject", printobject);
    super::op(dict, "writeobject", writeobject);
}

/// int setobjectformat -
fn setobjectformat(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    if !(0..=4).contains(&n) {
        return Err(PsError::Rangecheck);
    }
    it.object_format = n as u8;
    Ok(())
}

/// - currentobjectformat int
fn currentobjectformat(it: &mut Interp) -> Result<(), PsError> {
    let n = i64::from(it.object_format);
    it.push(crate::object::Object::int(n));
    Ok(())
}

/// The tag operand: an integer 0 to 255.
fn pop_tag(it: &mut Interp) -> Result<u8, PsError> {
    let tag = it.pop_int()?;
    u8::try_from(tag).map_err(|_| PsError::Rangecheck)
}

/// Pop `obj tag` and encode them in the current format.
fn encoded(it: &mut Interp, op: &str) -> Result<Vec<u8>, PsError> {
    if it.object_format == 0 {
        return Err(PsError::Undefined(op.to_string()));
    }
    let tag = pop_tag(it)?;
    let obj = it.pop()?;
    let fmt = it.object_format;
    crate::binobj::encode(&obj, tag, fmt, &mut |bytes| charge(it, bytes))
}

/// Tell the interpreter's limits about `bytes` more of the sequence.
fn charge(it: &mut Interp, bytes: usize) -> Result<(), PsError> {
    #[cfg(not(target_arch = "wasm32"))]
    if !it.limits().is_none() {
        return it.poll_read(bytes.div_ceil(1024), bytes);
    }
    let _ = (it, bytes);
    Ok(())
}

/// obj tag printobject -
fn printobject(it: &mut Interp) -> Result<(), PsError> {
    let bytes = encoded(it, "printobject")?;
    it.write_stdout(&bytes, false).map_err(|_| PsError::Io)
}

/// file obj tag writeobject -
fn writeobject(it: &mut Interp) -> Result<(), PsError> {
    if it.object_format == 0 {
        return Err(PsError::Undefined("writeobject".to_string()));
    }
    // The file is the deepest operand; look before taking the others so a
    // stack that is too short is `stackunderflow` and not a type error.
    let depth = it.operand_stack().len();
    if depth < 3 {
        return Err(PsError::StackUnderflow);
    }
    if !matches!(it.operand_stack()[depth - 3].value, Value::File(_)) {
        return Err(PsError::Typecheck);
    }
    let bytes = encoded(it, "writeobject")?;
    let file = super::file::pop_file(it)?;
    super::file::write_bytes(it, &file, &bytes)
}
