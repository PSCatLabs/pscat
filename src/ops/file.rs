//! File and filter operators. Files share their read position with the
//! scanner (see `src/file.rs`) — that's what `currentfile` is for.

use crate::error::PsError;
use crate::file::{Decoder, FileHandle, PsFile, Sink};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsString, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "file", file);
    op(dict, "currentfile", currentfile);
    op(dict, "read", read);
    op(dict, "readstring", readstring);
    op(dict, "readhexstring", readhexstring);
    op(dict, "readline", readline);
    op(dict, "closefile", closefile);
    op(dict, "bytesavailable", bytesavailable);
    op(dict, "flushfile", flushfile);
    op(dict, "status", status);
    op(dict, "filter", filter);
    op(dict, "eexec", eexec);
    op(dict, "run", run);
    op(dict, "write", write);
    op(dict, "writestring", writestring);
    op(dict, "writehexstring", writehexstring);
    op(dict, "flush", flush);
    op(dict, "renamefile", renamefile);
    op(dict, "deletefile", deletefile);
    op(dict, "filenameforall", filenameforall);
}

fn pop_file(it: &mut Interp) -> Result<FileHandle, PsError> {
    match &it.pop()?.value {
        Value::File(f) => Ok(f.clone()),
        _ => Err(PsError::Typecheck),
    }
}

fn pop_ps_string(it: &mut Interp) -> Result<PsString, PsError> {
    match &it.pop()?.value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(PsError::Typecheck),
    }
}

/// Read a filename the way a program wrote it: absolute, or relative
/// through the resource resolver (`crate::paths`, whose module doc has
/// the search order). A name that resolves relative to the current
/// directory reads exactly as it always did; the embedded `lib/` and
/// the install locations only answer what would otherwise be an
/// `undefinedfilename`.
///
/// Under execution limits (`Interp::set_limits`) the read looks at them as
/// the bytes arrive, so `(/dev/zero) (r) file` or a FIFO ends in a limit
/// error instead of running on inside this one operator (issue #702).
pub(super) fn read_resource(it: &mut Interp, name: &str) -> Result<Vec<u8>, PsError> {
    #[cfg(not(target_arch = "wasm32"))]
    if !it.limits().is_none() {
        return read_resource_limited(it, name);
    }
    // The wasm build has no disk to poll a read of.
    #[cfg(target_arch = "wasm32")]
    let _ = &it;
    crate::paths::open(name)
        .map(|r| r.bytes.into_owned())
        .ok_or(PsError::UndefinedFilename)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_resource_limited(it: &mut Interp, name: &str) -> Result<Vec<u8>, PsError> {
    use std::cell::RefCell;
    let it = RefCell::new(it);
    let stopped: RefCell<Option<PsError>> = RefCell::new(None);
    let read = |path: &std::path::Path| {
        if stopped.borrow().is_some() {
            return None;
        }
        let mut poll = |kib: usize, will_alloc: usize| it.borrow_mut().poll_read(kib, will_alloc);
        match crate::file::read_path_limited(path, &mut poll) {
            Ok(data) => data,
            Err(e) => {
                *stopped.borrow_mut() = Some(e);
                None
            }
        }
    };
    let found = crate::paths::open_with_reader(name, &read);
    // A limit wins over whatever the search found after it: a later place
    // (the embedded copy of `lib/`) must not turn a stopped read into a
    // successful one.
    if let Some(e) = stopped.take() {
        return Err(e);
    }
    found
        .map(|r| r.bytes.into_owned())
        .ok_or(PsError::UndefinedFilename)
}

/// filename access file → file. `(r)` reads a file through the resource
/// search order. `(w)` (truncate or create) and `(a)` (append or create)
/// write `%stdout`, `%stderr`, or a disk file inside the interpreter's
/// write roots (`Interp::set_write_roots`, the CLI's `--allow-write`;
/// none by default, so every disk path is `invalidfileaccess`). Other
/// modes (`r+`, `w+`, `a+`) are refused.
fn file(it: &mut Interp) -> Result<(), PsError> {
    let access = pop_ps_string(it)?.to_vec();
    let name = pop_ps_string(it)?;
    let mode = match access.as_slice() {
        b"r" => {
            let data = read_resource(it, &name.text())?;
            it.push(Object::lit(Value::File(PsFile::from_bytes(data))));
            return Ok(());
        }
        b"w" => crate::write_roots::WriteMode::Truncate,
        b"a" => crate::write_roots::WriteMode::Append,
        _ => return Err(PsError::InvalidFileAccess),
    };
    let name = name.to_vec();
    let sink = match name.as_slice() {
        b"%stdout" => Sink::Stdout,
        b"%stderr" => Sink::Stderr,
        // Other `%device` names (`%lineedit`, `%printer`) aren't files here.
        [b'%', ..] => return Err(PsError::UndefinedFilename),
        _ => {
            let name = writable_name(&name)?;
            Sink::Disk(it.write_policy().open(name, mode)?)
        }
    };
    it.push(Object::lit(Value::File(PsFile::output(sink))));
    Ok(())
}

/// A filename to write, rename or delete, as text. Only exact UTF-8: a
/// lossy conversion would act on a different name than the program gave.
fn writable_name(bytes: &[u8]) -> Result<&str, PsError> {
    std::str::from_utf8(bytes).map_err(|_| PsError::InvalidFileAccess)
}

/// Send bytes to an output file. Writing to an input file or a closed
/// one is an `ioerror`.
fn write_bytes(it: &mut Interp, f: &FileHandle, bytes: &[u8]) -> Result<(), PsError> {
    use std::io::Write;
    match f.borrow_mut().sink_mut() {
        None => return Err(PsError::Io),
        Some(Sink::Disk(d)) => return d.write_all(bytes).map_err(|_| PsError::Io),
        Some(Sink::Stderr) => {
            return std::io::stderr().write_all(bytes).map_err(|_| PsError::Io);
        }
        // Through the interpreter, outside the borrow.
        Some(Sink::Stdout) => {}
    }
    it.write_stdout(bytes, false).map_err(|_| PsError::Io)
}

/// Flush an output file (`flushfile` on one, and `closefile`).
fn flush_output(it: &mut Interp, f: &FileHandle) -> Result<(), PsError> {
    use std::io::Write;
    let mut file = f.borrow_mut();
    match file.sink_mut() {
        None => Ok(()),
        Some(Sink::Disk(d)) => d.flush().map_err(|_| PsError::Io),
        Some(Sink::Stderr) => std::io::stderr().flush().map_err(|_| PsError::Io),
        Some(Sink::Stdout) => {
            drop(file);
            it.write_stdout(&[], true).map_err(|_| PsError::Io)
        }
    }
}

/// file int write → (one byte, the integer's low 8 bits).
fn write(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    let f = pop_file(it)?;
    write_bytes(it, &f, &[n.rem_euclid(256) as u8])
}

/// file string writestring →
fn writestring(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    // Straight from the string's storage: no copy that the memory limit
    // wouldn't see.
    let bytes = s.borrow_bytes();
    write_bytes(it, &f, &bytes)
}

/// file string writehexstring → (two lowercase hex digits per byte, as
/// Ghostscript writes them).
fn writehexstring(it: &mut Interp) -> Result<(), PsError> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    // In fixed-size pieces, so a long string never needs a buffer twice
    // its size outside the memory limit's view.
    if s.is_empty() {
        // Still an error on a closed or input file.
        return write_bytes(it, &f, &[]);
    }
    let mut buf = [0u8; 8192];
    for chunk in s.borrow_bytes().chunks(buf.len() / 2) {
        for (i, &b) in chunk.iter().enumerate() {
            buf[2 * i] = HEX[usize::from(b >> 4)];
            buf[2 * i + 1] = HEX[usize::from(b & 15)];
        }
        write_bytes(it, &f, &buf[..2 * chunk.len()])?;
    }
    Ok(())
}

/// flush — push `print` and `%stdout` output out now.
fn flush(it: &mut Interp) -> Result<(), PsError> {
    it.write_stdout(&[], true).map_err(|_| PsError::Io)
}

/// oldname newname renamefile → (both inside the write roots).
fn renamefile(it: &mut Interp) -> Result<(), PsError> {
    let new = pop_ps_string(it)?.to_vec();
    let old = pop_ps_string(it)?.to_vec();
    it.write_policy()
        .rename(writable_name(&old)?, writable_name(&new)?)
}

/// filename deletefile → (inside the write roots).
fn deletefile(it: &mut Interp) -> Result<(), PsError> {
    let name = pop_ps_string(it)?.to_vec();
    it.write_policy().delete(writable_name(&name)?)
}

/// template proc scratch filenameforall → . Calls `proc` with each file
/// name matching `template` (`*` any run, `?` one character, `\` quotes
/// the next), copied into `scratch` (`rangecheck` when it doesn't fit).
/// Names are listed where `run` and `(r) file` would look
/// (`crate::filenames::matching_names`), so it can list what a program could
/// read, nothing more; wildcards match within the last path component.
fn filenameforall(it: &mut Interp) -> Result<(), PsError> {
    let scratch = it.pop()?;
    if !matches!(scratch.value, Value::String(_)) {
        return Err(PsError::Typecheck);
    }
    let proc = it.pop()?;
    if !(proc.executable && matches!(proc.value, Value::Array(_))) {
        return Err(PsError::Typecheck);
    }
    let template = pop_ps_string(it)?.to_vec();
    // Under execution limits each entry looked at is a step, and each
    // match's bytes are checked against the memory cap as it is added.
    #[cfg(not(target_arch = "wasm32"))]
    let limited = !it.limits().is_none();
    #[cfg(not(target_arch = "wasm32"))]
    let mut poll = |n: usize| if limited { it.poll_read(1, n) } else { Ok(()) };
    #[cfg(target_arch = "wasm32")]
    let mut poll = |_: usize| Ok(());
    let names = crate::filenames::matching_names(&template, &mut poll)?;
    let names: Vec<Object> = names.into_iter().map(Object::string).collect();
    // `{ scratch copy proc exec }` over the names, with the operators
    // themselves rather than their names, so a program that redefines
    // `copy` or `exec` doesn't change this one. `forall` gives `exit` its
    // usual meaning inside `proc`.
    let opr = |name, func| Object::exec(Value::Operator(crate::object::Operator { name, func }));
    let body = Object::procedure(vec![
        scratch,
        opr("copy", super::data::copy),
        proc,
        opr("exec", super::control::exec),
    ]);
    it.push(Object::array(names));
    it.push(body);
    super::data::forall(it)
}

/// The file the interpreter is currently executing — or the PLRM's
/// "invalid file object" (a closed file) when there isn't one.
fn currentfile(it: &mut Interp) -> Result<(), PsError> {
    let f = it.current_file().unwrap_or_else(PsFile::closed_file);
    it.push(Object::lit(Value::File(f)));
    Ok(())
}

/// file read → byte true | false
fn read(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    match f.borrow_mut().read_byte()? {
        Some(b) => {
            it.push(Object::int(i64::from(b)));
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
    }
    Ok(())
}

/// file string readstring → substring filled — the workhorse of image
/// data loops. The bool reports whether the string filled completely.
fn readstring(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    while n < s.len() {
        match f.borrow_mut().read_byte()? {
            Some(b) => {
                s.put_byte(n, b)?;
                n += 1;
            }
            None => break,
        }
    }
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(n > 0 && n == s.len()));
    Ok(())
}

/// file string readhexstring → substring filled. Non-hex bytes are
/// ignored, per the PLRM.
fn readhexstring(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    let mut hi: Option<u8> = None;
    loop {
        if n >= s.len() {
            break;
        }
        let Some(b) = f.borrow_mut().read_byte()? else {
            break;
        };
        if let Some(v) = (b as char).to_digit(16) {
            match hi.take() {
                Some(h) => {
                    s.put_byte(n, (h << 4) | v as u8)?;
                    n += 1;
                }
                None => hi = Some(v as u8),
            }
        }
    }
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(n > 0 && n == s.len()));
    Ok(())
}

/// file string readline → substring found — up to (not including) the
/// newline; rangecheck if the line doesn't fit.
fn readline(it: &mut Interp) -> Result<(), PsError> {
    let s = pop_ps_string(it)?;
    let f = pop_file(it)?;
    let mut n = 0;
    let found = loop {
        let Some(b) = f.borrow_mut().read_byte()? else {
            break false;
        };
        match b {
            b'\n' => break true,
            b'\r' => {
                if f.borrow_mut().peek_byte()? == Some(b'\n') {
                    f.borrow_mut().read_byte()?;
                }
                break true;
            }
            _ => {
                if n >= s.len() {
                    return Err(PsError::Rangecheck);
                }
                s.put_byte(n, b)?;
                n += 1;
            }
        }
    };
    it.push(Object::lit(Value::String(s.interval(0, n)?)));
    it.push(Object::bool(found));
    Ok(())
}

fn closefile(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    let flushed = flush_output(it, &f);
    f.borrow_mut().close();
    flushed
}

fn bytesavailable(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    let n = f.borrow().bytes_available();
    it.push(Object::int(n.map_or(-1, |n| n as i64)));
    Ok(())
}

/// For an input file, flushfile reads and discards to EOF, per the PLRM;
/// an output file's pending bytes go out.
fn flushfile(it: &mut Interp) -> Result<(), PsError> {
    let f = pop_file(it)?;
    if f.borrow().is_output() {
        return flush_output(it, &f);
    }
    while f.borrow_mut().read_byte()?.is_some() {}
    Ok(())
}

/// file status → bool | filename status → false (no filesystem stats).
fn status(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::File(f) => it.push(Object::bool(f.borrow().is_open())),
        Value::String(_) => it.push(Object::bool(false)),
        _ => return Err(PsError::Typecheck),
    }
    Ok(())
}

/// source [dict] name filter → file. Decode filters only; a parameter
/// dict operand is accepted and ignored (none of the supported filters
/// take parameters we honor yet). Sources: file or string.
fn filter(it: &mut Interp) -> Result<(), PsError> {
    let name = match &it.pop()?.value {
        Value::Name(n) => n.clone(),
        _ => return Err(PsError::Typecheck),
    };
    let dec = match &*name {
        "ASCIIHexDecode" => Decoder::ascii_hex(),
        "ASCII85Decode" => Decoder::ascii85(),
        "RunLengthDecode" => Decoder::RunLength,
        "FlateDecode" => Decoder::flate(),
        "LZWDecode" => Decoder::lzw(),
        "DCTDecode" => Decoder::dct(),
        // Encode filters (and the one decoder we don't have,
        // CCITTFaxDecode) are undefined.
        _ => return Err(PsError::Undefined(name.to_string())),
    };
    let mut src = it.pop()?;
    if matches!(src.value, Value::Dict(_)) {
        // A parameter dict between source and name: accept, ignore.
        src = it.pop()?;
    }
    let source = match &src.value {
        Value::File(f) => f.clone(),
        // A string source is a snapshot of its current contents.
        Value::String(s) => PsFile::from_bytes(s.to_vec()),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::lit(Value::File(PsFile::filtered(source, dec))));
    Ok(())
}

/// file eexec — execute the encrypted region as program source. The
/// canonical caller is an embedded Type 1 font: `currentfile eexec`.
fn eexec(it: &mut Interp) -> Result<(), PsError> {
    let src = it.pop()?;
    let file = match &src.value {
        Value::File(f) => f.clone(),
        // eexec on a string is legal (fonts are sometimes wrapped).
        Value::String(s) => PsFile::from_bytes(s.to_vec()),
        _ => return Err(PsError::Typecheck),
    };
    it.begin_eexec(file)
}

/// filename run — execute a program file from disk.
fn run(it: &mut Interp) -> Result<(), PsError> {
    let name = pop_ps_string(it)?.text();
    let data = read_resource(it, &name)?;
    let f = PsFile::from_bytes(data);
    it.exec_object(Object::exec(Value::File(f)))
}
