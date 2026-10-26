//! `writereals` and `readreals`: save and load a numeric array as packed
//! little-endian binary (issues #735, #773).
//!
//! ```text
//! array (path) [/f32] writereals          -       % truncates the file
//! array file   [/f32] writereals          -       % to an open output file
//! (path) n          [/f32] readreals      array   % n reals from the start
//! (path) offset n   [/f32] readreals      array   % ... from element `offset`
//! (path) /all       [/f32] readreals      array   % every value in the file
//! (path) offset /all [/f32] readreals     array   % every value from `offset`
//! file   n|/all     [/f32] readreals      array   % from the file's position
//! ```
//!
//! Formats are `/f64` (the default, 8 bytes each) and `/f32` (4 bytes); any
//! other name is `rangecheck`. The byte order is little-endian on every
//! platform.
//!
//! **Count and offset** (issue #773). The count is an integer or `/all`
//! (a name, as `/f32` is: the PLRM's options are names, and `-1` as "all"
//! would turn an `n 1 sub` that went negative into a silent read of the whole
//! file, so a negative count stays `rangecheck`). The offset is in values, not
//! bytes, and comes first, as `getinterval`'s index does, so `(p) 100 50`
//! reads values 100 to 149 and `(p) 100 /all` reads from 100 to the end. An
//! offset past the last value is `rangecheck`; one equal to the value count
//! with `/all` is an empty array. A trailing partial value is ignored. An
//! `/all` result over the 16M-element array cap is `limitcheck` (never cut
//! short). The offset belongs to the path form only: an open file already has
//! a position (`setfileposition`, in bytes), and a second kind of offset next
//! to it would be a trap, so `file offset n readreals` is `typecheck`.
//!
//! **File objects.** `writereals` writes to any open output file (`(%stdout)
//! (w) file`, an `(a)` append file, a `(w)` disk file) at its current
//! position, after what is already there or buffered, and counts against
//! `--max-write-bytes` like `writestring` (the whole write against the cap
//! first, so a refusal writes none of it). It does not close the file. A
//! closed or input file is `ioerror`. `readreals` reads from an input file's
//! current position and leaves it just after the values it took, so reads
//! compose with `setfileposition` and with `currentfile` (inline binary
//! data). On a file backed by bytes (`(r) file`), a short file is
//! `rangecheck` and nothing is consumed; `/all` takes whole values and
//! leaves a trailing partial value unread. A decode filter cannot tell how
//! much is left, so a short read there has already consumed what it read.
//! A closed or output file is `ioerror`.
//!
//! Writing to a path follows the write policy of issue #740
//! (`--allow-write`; the path goes through
//! [`crate::write_roots::WriteRoots::open`], which is
//! `Interp::writable_path` plus the open itself) and truncates an existing
//! file. Reading a path follows the read policy of `(path) (r) file` (the
//! same search order, the same limit-aware read, so read roots apply).
//!
//! Errors, in the order they are checked:
//! - `stackunderflow` / `typecheck`: a missing or wrong operand type, or an
//!   array element that is not an integer or real. Every element is checked
//!   before the file is opened or written, so a bad array never truncates the
//!   target.
//! - `invalidfileaccess` / `undefinedfilename`: as for `(path) (w) file`
//!   (writing) or `(path) (r) file` (reading).
//! - `rangecheck`: a negative count or offset, an unknown format name, an
//!   offset past the last value, or fewer values than asked for. The PLRM
//!   keeps `ioerror` for an I/O failure that is not end-of-file, and a short
//!   file is end-of-file, so it is not `ioerror`; asking for more values
//!   than exist is a range error, like `getinterval` past the end.
//! - `limitcheck`: a count (or an `/all` result) over the 16M-element `array`
//!   cap; or, for writing, the whole array would pass the write-bytes cap
//!   (`--max-write-bytes`, `Interp::set_max_write_bytes`), which is checked
//!   before the file is opened or written, so the target is left as it was.
//! - `ioerror`: the OS refused a write, or the file object is closed or the
//!   wrong direction.
//!
//! A limit that stops a long write leaves the file as far as it got (a path
//! was truncated when opened): short, so a later `readreals` of the full
//! count is `rangecheck` rather than a silently wrong array.
//!
//! Bytes past the values asked for (a longer file, or a trailing partial
//! value) are ignored. The values are preserved bit for bit: NaN, infinities,
//! `-0.0` and subnormals round-trip through `/f64`. Integers are written as
//! reals (an integer above 2^53 rounds as `cvr` does). `/f32` rounds to the
//! nearest binary32, so a value beyond about 3.4e38 becomes an infinity
//! and one below about 1e-45 becomes zero; reading `/f32` is exact.
//!
//! Limits: a 64 KiB chunk is one `poll_read` (one step per KiB, plus the
//! time and memory checks), and the result is `check_alloc`ed before it is
//! made, so huge arrays stop on a limit rather than on the operating system.
//! Writing encodes through one fixed buffer; it never copies the array. A
//! path read takes the whole file into memory first (as `(r) file` does,
//! under the same limits); a file-object read takes only the values asked
//! for from the bytes the file already holds.

use crate::error::PsError;
use crate::file::{FileHandle, Sink};
use crate::interp::Interp;
use crate::object::{Dict, Object, PsArray, Value};

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
/// With `allow_all`, the name `/all` is the next operand itself (the count
/// of `readreals`), not a format.
fn pop_format(it: &mut Interp, allow_all: bool) -> Result<(Fmt, Object), PsError> {
    let top = it.pop()?;
    let Value::Name(n) = &top.value else {
        return Ok((Fmt::F64, top));
    };
    let fmt = match &**n {
        "f64" => Fmt::F64,
        "f32" => Fmt::F32,
        "all" if allow_all => return Ok((Fmt::F64, top)),
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

/// Encode elements `start..end` of `a` into `buf` (cleared first).
fn encode(a: &PsArray, start: usize, end: usize, fmt: Fmt, buf: &mut Vec<u8>) {
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
}

/// array (path) [/f32] writereals -
/// array file [/f32] writereals -
fn writereals(it: &mut Interp) -> Result<(), PsError> {
    use std::io::Write;
    let (fmt, target) = pop_format(it, false)?;
    if !matches!(target.value, Value::String(_) | Value::File(_)) {
        return Err(PsError::Typecheck);
    }
    let arr = it.pop()?;
    let Value::Array(a) = &arr.value else {
        return Err(PsError::Typecheck);
    };
    let len = a.len();
    // Every element first, so a bad one never truncates the file or sends a
    // byte. `peek` looks in place: no copy of the array that a memory limit
    // could miss.
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
    let mut buf: Vec<u8> = Vec::with_capacity(CHUNK.min(len) * fmt.width());
    let path = match target.value {
        Value::File(f) => return write_to_file(it, &f, a, fmt, &mut buf),
        Value::String(path) => path,
        _ => return Err(PsError::Typecheck),
    };
    let name = path.to_vec();
    if name.first() == Some(&b'%') {
        return Err(PsError::UndefinedFilename);
    }
    let name = std::str::from_utf8(&name).map_err(|_| PsError::InvalidFileAccess)?;
    // The whole write against the byte cap before the file is opened, so a
    // refusal leaves the target as it was (issue #762).
    it.check_write_bytes((len * fmt.width()) as u64)?;
    // What an open handle still holds goes to disk before this open
    // truncates the file (as when writes were unbuffered).
    let _ = it.flush_files();
    let mut file = it
        .write_policy()
        .open(name, crate::write_roots::WriteMode::Truncate)?;
    for start in (0..len).step_by(CHUNK) {
        let end = (start + CHUNK).min(len);
        encode(a, start, end, fmt, &mut buf);
        poll(it, buf.len())?;
        it.charge_write_bytes(buf.len() as u64)?;
        file.write_all(&buf).map_err(|_| PsError::Io)?;
    }
    Ok(())
}

/// The file-object form of `writereals` (issue #773): the bytes go through
/// the same path as `writestring`, so `%stdout`, append files, buffering,
/// `fileposition` and the write cap all behave as they do there.
fn write_to_file(
    it: &mut Interp,
    f: &FileHandle,
    a: &PsArray,
    fmt: Fmt,
    buf: &mut Vec<u8>,
) -> Result<(), PsError> {
    let len = a.len();
    // A closed or input file is an `ioerror` even for an empty array.
    super::file::write_bytes(it, f, &[])?;
    // A disk file's whole write against the cap first: a refusal writes none
    // of it (as `writehexstring` does).
    if matches!(f.borrow_mut().sink_mut(), Some(Sink::Disk(_))) {
        it.check_write_bytes((len * fmt.width()) as u64)?;
    }
    for start in (0..len).step_by(CHUNK) {
        let end = (start + CHUNK).min(len);
        encode(a, start, end, fmt, buf);
        poll(it, buf.len())?;
        super::file::write_bytes(it, f, buf)?;
    }
    Ok(())
}

/// Append `bytes` (whole values) to `items` as reals.
fn decode(fmt: Fmt, bytes: &[u8], items: &mut Vec<Object>) {
    match fmt {
        Fmt::F64 => items.extend(
            bytes
                .chunks_exact(8)
                .map(|b| Object::real(f64::from_le_bytes(b.try_into().unwrap()))),
        ),
        Fmt::F32 => items.extend(
            bytes
                .chunks_exact(4)
                .map(|b| Object::real(f64::from(f32::from_le_bytes(b.try_into().unwrap())))),
        ),
    }
}

/// A count operand checked against the array cap.
fn checked_count(n: i64) -> Result<usize, PsError> {
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    if n > super::data::ALLOC_LIMIT {
        return Err(PsError::Limitcheck);
    }
    Ok(n as usize)
}

/// (path) [offset] n|/all [/f32] readreals array
/// file n|/all [/f32] readreals array
fn readreals(it: &mut Interp) -> Result<(), PsError> {
    let (fmt, cnt) = pop_format(it, true)?;
    let count = match &cnt.value {
        Value::Integer(n) => Some(*n),
        Value::Name(n) if &**n == "all" => None,
        Value::Name(_) => return Err(PsError::Rangecheck),
        _ => return Err(PsError::Typecheck),
    };
    let mut src = it.pop()?;
    let mut offset = None;
    if let Value::Integer(o) = src.value {
        offset = Some(o);
        src = it.pop()?;
    }
    match src.value {
        Value::String(path) => {
            let want = count.map(checked_count).transpose()?;
            let off = offset.unwrap_or(0);
            if off < 0 {
                return Err(PsError::Rangecheck);
            }
            read_path(it, fmt, &path.text(), off, want)
        }
        Value::File(f) if offset.is_none() => {
            let want = count.map(checked_count).transpose()?;
            read_file(it, fmt, &f, want)
        }
        _ => Err(PsError::Typecheck),
    }
}

/// `(path) offset n|/all readreals`; `want` is `None` for `/all`.
fn read_path(
    it: &mut Interp,
    fmt: Fmt,
    path: &str,
    offset: i64,
    want: Option<usize>,
) -> Result<(), PsError> {
    let w = fmt.width();
    let obj = std::mem::size_of::<Object>();
    if let Some(n) = want {
        // The file's bytes and the array both go on the heap: refuse before
        // either is made.
        it.check_alloc(n * (w + obj))?;
    }
    let bytes = super::file::read_resource(it, path)?;
    let avail = bytes.len() / w;
    let off = usize::try_from(offset)
        .ok()
        .filter(|o| *o <= avail)
        .ok_or(PsError::Rangecheck)?;
    let n = match want {
        Some(n) if n > avail - off => return Err(PsError::Rangecheck),
        Some(n) => n,
        None => {
            let n = avail - off;
            checked_count(n as i64)?;
            n
        }
    };
    it.check_alloc(n * obj)?;
    let mut items: Vec<Object> = Vec::with_capacity(n);
    for chunk in bytes[off * w..(off + n) * w].chunks(CHUNK * w) {
        poll(it, chunk.len())?;
        decode(fmt, chunk, &mut items);
    }
    it.push(Object::array(items));
    Ok(())
}

/// `file n|/all readreals`: from the file's position, which moves past the
/// values taken.
fn read_file(
    it: &mut Interp,
    fmt: Fmt,
    f: &FileHandle,
    want: Option<usize>,
) -> Result<(), PsError> {
    let w = fmt.width();
    let obj = std::mem::size_of::<Object>();
    // A closed file or an output file is an `ioerror`, whatever was asked.
    f.borrow_mut().read_into(&mut Vec::new(), 0)?;
    // `None` for a filter, which cannot say how much is left.
    let left = f.borrow().bytes_available().map(|b| b / w);
    let want = match (want, left) {
        (Some(n), Some(left)) if n > left => return Err(PsError::Rangecheck),
        (None, Some(left)) => Some(checked_count(left as i64)?),
        (want, _) => want,
    };
    if let Some(n) = want {
        it.check_alloc(n * obj)?;
    }
    let mut items: Vec<Object> = Vec::with_capacity(want.unwrap_or(0));
    let mut buf: Vec<u8> = Vec::new();
    loop {
        let todo = match want {
            Some(n) => n - items.len(),
            None => CHUNK,
        };
        if todo == 0 {
            break;
        }
        let k = todo.min(CHUNK);
        if want.is_none() {
            // Unknown length: grow by chunks, keeping to the cap and the
            // memory limit (the copy a doubling Vec makes counts too).
            it.check_alloc((items.len() + k) * obj + items.len() * obj)?;
        }
        buf.clear();
        let got = f.borrow_mut().read_into(&mut buf, k * w)?;
        poll(it, got)?;
        decode(fmt, &buf[..got / w * w], &mut items);
        checked_count(items.len() as i64)?;
        if got < k * w {
            // End of data: fewer values than asked for is a range error, but
            // `/all` is happy with what there was.
            if want.is_some() {
                return Err(PsError::Rangecheck);
            }
            break;
        }
    }
    it.push(Object::array(items));
    Ok(())
}
