//! `writereals` and `readreals`: save and load a numeric array as packed
//! little-endian binary (issue #735).
//!
//! ```text
//! array (path) writereals            -    % IEEE 754 binary64, 8 bytes each
//! array (path) /f32 writereals       -    % binary32, 4 bytes each
//! (path) n readreals          array       % n reals from the start of the file
//! (path) n /f32 readreals     array
//! ```
//!
//! Formats are `/f64` (the default) and `/f32`; any other name is
//! `rangecheck`. The byte order is little-endian on every platform.
//!
//! Writing follows the write policy of issue #740 (`--allow-write`; the
//! path goes through [`crate::write_roots::WriteRoots::open`], which is
//! `Interp::writable_path` plus the open itself) and truncates an existing
//! file. Reading follows the read policy of `(path) (r) file` (the same
//! search order, the same limit-aware read).
//!
//! Errors, in the order they are checked:
//! - `typecheck`: wrong operand type, or an array element that is not an
//!   integer or real. Every element is checked before the file is opened,
//!   so a bad array never truncates the target.
//! - `invalidfileaccess` / `undefinedfilename`: as for `(path) (w) file`
//!   (writing) or `(path) (r) file` (reading).
//! - `rangecheck`: `n` is negative, an unknown format name, or the file
//!   holds fewer than `n` values. The PLRM keeps `ioerror` for an I/O
//!   failure that is not end-of-file, and a short file is end-of-file, so
//!   it is not `ioerror`; asking for more values than exist is a range
//!   error, like `getinterval` past the end.
//! - `limitcheck`: `n` over the 16M-element `array` cap.
//! - `ioerror`: the OS refused a write.
//!
//! A limit that stops a long write leaves the file as far as it got (it was
//! truncated when opened): short, so a later `readreals` of the full count
//! is `rangecheck` rather than a silently wrong array.
//!
//! Bytes past the first `n` values (a longer file, or a trailing partial
//! value) are ignored. The values are preserved bit for bit: NaN, infinities,
//! `-0.0` and subnormals round-trip through `/f64`. Integers are written as
//! reals (an integer above 2^53 rounds as `cvr` does). `/f32` rounds to the
//! nearest binary32, so a value beyond about 3.4e38 becomes an infinity
//! and one below about 1e-45 becomes zero; reading `/f32` is exact.
//!
//! Limits: a 64 KiB chunk is one `poll_read` (one step per KiB, plus the
//! time and memory checks), and the read result is `check_alloc`ed before
//! the file is read, so huge arrays stop on a limit rather than on the
//! operating system. Writing encodes through one fixed buffer; it never
//! copies the array.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Object, Value};

/// Elements per chunk (64 KiB of binary64).
const CHUNK: usize = 8192;

#[derive(Clone, Copy)]
enum Fmt {
    F64,
    F32,
}

impl Fmt {
    fn width(self) -> usize {
        match self {
            Fmt::F64 => 8,
            Fmt::F32 => 4,
        }
    }
}

pub fn install(dict: &mut Dict) {
    super::op(dict, "writereals", writereals);
    super::op(dict, "readreals", readreals);
}

/// Pop an optional format name; returns the format and the next operand.
fn pop_format(it: &mut Interp) -> Result<(Fmt, Object), PsError> {
    let top = it.pop()?;
    let Value::Name(n) = &top.value else {
        return Ok((Fmt::F64, top));
    };
    let fmt = match &**n {
        "f64" => Fmt::F64,
        "f32" => Fmt::F32,
        _ => return Err(PsError::Rangecheck),
    };
    Ok((fmt, it.pop()?))
}

/// One look at the limits for `bytes` of work (a step per KiB).
fn poll(it: &mut Interp, bytes: usize) -> Result<(), PsError> {
    #[cfg(not(target_arch = "wasm32"))]
    if !it.limits().is_none() {
        return it.poll_read(bytes.div_ceil(1024), 0);
    }
    let _ = (it, bytes);
    Ok(())
}

/// array (path) [/f32] writereals -
fn writereals(it: &mut Interp) -> Result<(), PsError> {
    use std::io::Write;
    let (fmt, path) = pop_format(it)?;
    let Value::String(path) = path.value else {
        return Err(PsError::Typecheck);
    };
    let arr = it.pop()?;
    let Value::Array(a) = &arr.value else {
        return Err(PsError::Typecheck);
    };
    let len = a.len();
    // Every element first, so a bad one never truncates the file. `peek`
    // looks in place: no copy of the array that a memory limit could miss.
    for start in (0..len).step_by(CHUNK) {
        let end = (start + CHUNK).min(len);
        for i in start..end {
            let ok = a.peek(i, |o| matches!(o.value, Value::Integer(_) | Value::Real(_)));
            if ok != Some(true) {
                return Err(PsError::Typecheck);
            }
        }
        poll(it, (end - start) * fmt.width())?;
    }
    let name = path.to_vec();
    if name.first() == Some(&b'%') {
        return Err(PsError::UndefinedFilename);
    }
    let name = std::str::from_utf8(&name).map_err(|_| PsError::InvalidFileAccess)?;
    let mut file = it
        .write_policy()
        .open(name, crate::write_roots::WriteMode::Truncate)?;
    let mut buf: Vec<u8> = Vec::with_capacity(CHUNK * fmt.width());
    for start in (0..len).step_by(CHUNK) {
        let end = (start + CHUNK).min(len);
        buf.clear();
        for i in start..end {
            let x = a
                .peek(i, |o| match o.value {
                    Value::Integer(n) => n as f64,
                    Value::Real(r) => r,
                    _ => 0.0,
                })
                .unwrap_or(0.0);
            match fmt {
                Fmt::F64 => buf.extend_from_slice(&x.to_le_bytes()),
                Fmt::F32 => buf.extend_from_slice(&(x as f32).to_le_bytes()),
            }
        }
        poll(it, buf.len())?;
        file.write_all(&buf).map_err(|_| PsError::Io)?;
    }
    Ok(())
}

/// (path) n [/f32] readreals array
fn readreals(it: &mut Interp) -> Result<(), PsError> {
    let (fmt, n) = pop_format(it)?;
    let Value::Integer(n) = n.value else {
        return Err(PsError::Typecheck);
    };
    let Value::String(path) = it.pop()?.value else {
        return Err(PsError::Typecheck);
    };
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    if n > super::data::ALLOC_LIMIT {
        return Err(PsError::Limitcheck);
    }
    let n = n as usize;
    let need = n * fmt.width();
    // The file's bytes and the array both go on the heap: refuse before
    // either is made.
    it.check_alloc(need + n * std::mem::size_of::<Object>())?;
    let bytes = super::file::read_resource(it, &path.text())?;
    if bytes.len() < need {
        return Err(PsError::Rangecheck);
    }
    it.check_alloc(n * std::mem::size_of::<Object>())?;
    let mut items: Vec<Object> = Vec::with_capacity(n);
    for chunk in bytes[..need].chunks(CHUNK * fmt.width()) {
        poll(it, chunk.len())?;
        match fmt {
            Fmt::F64 => items.extend(
                chunk
                    .chunks_exact(8)
                    .map(|b| Object::real(f64::from_le_bytes(b.try_into().unwrap()))),
            ),
            Fmt::F32 => items.extend(
                chunk
                    .chunks_exact(4)
                    .map(|b| Object::real(f64::from(f32::from_le_bytes(b.try_into().unwrap())))),
            ),
        }
    }
    it.push(Object::array(items));
    Ok(())
}
