//! Polymorphic data operators: the array/string/dict element access that
//! real-world PostScript leans on constantly. Interval results are
//! *views* sharing the source's storage, per the PLRM.

use std::cell::RefCell;
use std::rc::Rc;

use crate::error::PsError;
use crate::interp::{ForallSrc, Interp};
use crate::name::PsName;
use crate::object::{Dict, Object, PsArray, Value};

use super::control::pop_proc;
use super::stack::copy_n;

/// Implementation limit for `array`/`string` allocation — keeps a typo'd
/// `1e9 string` from attempting a gigabyte.
pub(super) const ALLOC_LIMIT: i64 = 16_777_216;

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "length", length);
    op(dict, "get", get);
    op(dict, "put", put);
    op(dict, "getinterval", getinterval);
    op(dict, "putinterval", putinterval);
    op(dict, "copy", copy);
    op(dict, "forall", forall);
    op(dict, "array", array);
    op(dict, "string", string);
    op(dict, "aload", aload);
    op(dict, "astore", astore);
    op(dict, "search", search);
    op(dict, "anchorsearch", anchorsearch);
    op(dict, "token", token);
}

fn pop_string(it: &mut Interp) -> Result<crate::object::PsString, PsError> {
    match &it.pop()?.value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(PsError::Typecheck),
    }
}

/// The first index at which `needle` occurs in `hay`; 0 for an empty
/// needle. Linear in `hay.len() + needle.len()` (#1176): the naive
/// `windows().position()` it replaced was O(n*m), so one `search` of a
/// 16 MiB string for an 8 MiB pattern ran for minutes inside one operator,
/// where the execution limits cannot see it.
fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    memchr::memmem::find(hay, needle)
}

/// str seek search -> post match pre true | str false. The three results
/// are views into str, per the PLRM.
fn search(it: &mut Interp) -> Result<(), PsError> {
    let seek = pop_string(it)?;
    let s = pop_string(it)?;
    // Borrowed, not copied: the two may be views of one string, which two
    // shared borrows allow. Both end before the result views are made.
    let found = find_bytes(&s.borrow_bytes(), &seek.borrow_bytes());
    match found {
        Some(i) => {
            let n = seek.len();
            it.push(Object::lit(Value::String(
                s.interval(i + n, s.len() - i - n)?,
            )));
            it.push(Object::lit(Value::String(s.interval(i, n)?)));
            it.push(Object::lit(Value::String(s.interval(0, i)?)));
            it.push(Object::bool(true));
        }
        None => {
            it.push(Object::lit(Value::String(s)));
            it.push(Object::bool(false));
        }
    }
    Ok(())
}

/// str seek anchorsearch -> post match true | str false.
fn anchorsearch(it: &mut Interp) -> Result<(), PsError> {
    let seek = pop_string(it)?;
    let s = pop_string(it)?;
    let n = seek.len();
    let matches = n <= s.len() && s.borrow_bytes()[..n] == *seek.borrow_bytes();
    if matches {
        it.push(Object::lit(Value::String(s.interval(n, s.len() - n)?)));
        it.push(Object::lit(Value::String(s.interval(0, n)?)));
        it.push(Object::bool(true));
    } else {
        it.push(Object::lit(Value::String(s)));
        it.push(Object::bool(false));
    }
    Ok(())
}

/// str token -> post any true | false, or file token -> any true |
/// false (the file's shared position advances past the token).
fn token(it: &mut Interp) -> Result<(), PsError> {
    let src = it.pop()?;
    let s = match &src.value {
        Value::String(s) => s.clone(),
        Value::File(f) => {
            match it.scan_token_from_file(f.clone())? {
                Some(obj) => {
                    it.push(obj);
                    it.push(Object::bool(true));
                }
                None => it.push(Object::bool(false)),
            }
            return Ok(());
        }
        _ => return Err(PsError::Typecheck),
    };
    match it.scan_token_from(s.to_vec())? {
        Some((obj, consumed)) => {
            let consumed = consumed.min(s.len());
            it.push(Object::lit(Value::String(
                s.interval(consumed, s.len() - consumed)?,
            )));
            it.push(obj);
            it.push(Object::bool(true));
        }
        None => it.push(Object::bool(false)),
    }
    Ok(())
}

fn index_of(key: &Object) -> Result<usize, PsError> {
    match key.value {
        Value::Integer(i) if i >= 0 => Ok(i as usize),
        Value::Integer(_) => Err(PsError::Rangecheck),
        _ => Err(PsError::Typecheck),
    }
}

fn length(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let n = match &obj.value {
        Value::Array(a) => a.len(),
        Value::String(s) => s.len(),
        Value::Dict(d) => d.borrow().len(),
        Value::Name(n) => n.len(),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object::int(n as i64));
    Ok(())
}

fn get(it: &mut Interp) -> Result<(), PsError> {
    // An array element or a dictionary entry that is there: nothing can
    // fail, so the operands skip the ledger (#1047).
    if let [.., obj, key] = it.ostack.as_slice() {
        let found = match (&obj.value, &key.value) {
            (Value::Array(a), &Value::Integer(i)) => usize::try_from(i).ok().and_then(|i| a.get(i)),
            (Value::Dict(d), Value::Name(n)) => d.borrow().get_id(n.id()),
            _ => None,
        };
        if let Some(el) = found {
            for _ in 0..2 {
                if let Some(o) = it.pop_settled() {
                    o.drop_inline();
                }
            }
            it.push(el);
            return Ok(());
        }
    }
    let key = it.pop()?;
    let obj = it.pop()?;
    match &obj.value {
        Value::Array(a) => {
            let el = a.get(index_of(&key)?).ok_or(PsError::Rangecheck)?;
            it.push(el);
        }
        Value::String(s) => {
            let b = s.byte(index_of(&key)?).ok_or(PsError::Rangecheck)?;
            it.push(Object::int(i64::from(b)));
        }
        Value::Dict(d) => {
            let v = d
                .borrow()
                .get_obj(&key)?
                .ok_or_else(|| PsError::Undefined(key.text()))?;
            it.push(v);
        }
        _ => return Err(PsError::Typecheck),
    }
    Ok(())
}

fn put(it: &mut Interp) -> Result<(), PsError> {
    // A writable array and an index in it, or a dictionary and a name: the
    // write cannot fail, so the operands skip the ledger (#1047).
    if let [.., obj, key, _] = it.operand_stack() {
        enum Target {
            Array(PsArray, usize),
            Dict(Rc<RefCell<Dict>>, PsName),
        }
        let target = match (&obj.value, &key.value) {
            (Value::Array(a), &Value::Integer(i)) if !obj.is_readonly() => usize::try_from(i)
                .ok()
                .filter(|&i| i < a.len())
                .map(|i| Target::Array(a.clone(), i)),
            (Value::Dict(d), Value::Name(n)) => Some(Target::Dict(d.clone(), n.clone())),
            _ => None,
        };
        if let Some(target) = target
            && let Some(value) = it.pop_settled()
        {
            it.pop_settled();
            it.pop_settled();
            return match target {
                Target::Array(a, i) => {
                    it.journal_array(&a);
                    a.put(i, value)
                }
                Target::Dict(d, n) => {
                    it.journal_dict(&d);
                    d.borrow_mut().put_name(&n, value);
                    Ok(())
                }
            };
        }
    }
    let value = it.pop()?;
    let key = it.pop()?;
    let obj = it.pop()?;
    match &obj.value {
        Value::Array(a) => {
            obj.check_writable()?;
            it.journal_array(a);
            a.put(index_of(&key)?, value)
        }
        // Strings take no journal barrier: restore exempts string
        // contents, per PLRM §3.7.3.2 (pinned against gs — docs/dev/VM.md).
        Value::String(s) => {
            let b = match value.value {
                Value::Integer(i) if (0..=255).contains(&i) => i as u8,
                Value::Integer(_) => return Err(PsError::Rangecheck),
                _ => return Err(PsError::Typecheck),
            };
            s.put_byte(index_of(&key)?, b)
        }
        Value::Dict(d) => {
            it.journal_dict(d);
            d.borrow_mut().put_obj(&key, value)
        }
        _ => Err(PsError::Typecheck),
    }
}

fn getinterval(it: &mut Interp) -> Result<(), PsError> {
    let len = it.pop_int()?;
    let off = it.pop_int()?;
    if len < 0 || off < 0 {
        return Err(PsError::Rangecheck);
    }
    let obj = it.pop()?;
    let value = match &obj.value {
        Value::Array(a) => Value::Array(a.interval(off as usize, len as usize)?),
        Value::String(s) => Value::String(s.interval(off as usize, len as usize)?),
        _ => return Err(PsError::Typecheck),
    };
    it.push(Object {
        value,
        executable: obj.executable,
        readonly: obj.readonly,
        noaccess: obj.noaccess,
        packed: obj.packed,
    });
    Ok(())
}

fn putinterval(it: &mut Interp) -> Result<(), PsError> {
    let src = it.pop()?;
    let off = it.pop_int()?;
    if off < 0 {
        return Err(PsError::Rangecheck);
    }
    let off = off as usize;
    let dst = it.pop()?;
    match (&dst.value, &src.value) {
        (Value::Array(d), Value::Array(s)) => {
            dst.check_writable()?;
            it.journal_array(d);
            // Snapshot first so overlapping views over the same storage
            // copy correctly.
            let items = s.snapshot();
            if off > d.len() || items.len() > d.len() - off {
                return Err(PsError::Rangecheck);
            }
            for (i, el) in items.into_iter().enumerate() {
                d.put(off + i, el)?;
            }
            Ok(())
        }
        (Value::String(d), Value::String(s)) => {
            let bytes = s.to_vec();
            d.write_at(off, &bytes)
        }
        _ => Err(PsError::Typecheck),
    }
}

/// `copy` is polymorphic: `n copy` duplicates the top n stack entries;
/// composite copy fills the second operand from the first and returns
/// the filled prefix (a view, for arrays/strings).
pub(crate) fn copy(it: &mut Interp) -> Result<(), PsError> {
    let top = it.pop()?;
    match &top.value {
        Value::Integer(n) => copy_n(it, *n),
        Value::Array(dst) => {
            let src_obj = it.pop()?;
            let Value::Array(src) = &src_obj.value else {
                return Err(PsError::Typecheck);
            };
            top.check_writable()?;
            it.journal_array(dst);
            let items = src.snapshot();
            if items.len() > dst.len() {
                return Err(PsError::Rangecheck);
            }
            for (i, el) in items.iter().enumerate() {
                dst.put(i, el.clone())?;
            }
            it.push(Object {
                value: Value::Array(dst.interval(0, items.len())?),
                executable: top.executable,
                readonly: false,
                noaccess: false,
                packed: false,
            });
            Ok(())
        }
        Value::String(dst) => {
            let src_obj = it.pop()?;
            let Value::String(src) = &src_obj.value else {
                return Err(PsError::Typecheck);
            };
            let bytes = src.to_vec();
            dst.write_at(0, &bytes)?;
            it.push(Object {
                value: Value::String(dst.interval(0, bytes.len())?),
                executable: top.executable,
                readonly: false,
                noaccess: false,
                packed: false,
            });
            Ok(())
        }
        Value::Dict(dst) => {
            let src_obj = it.pop()?;
            let Value::Dict(src) = &src_obj.value else {
                return Err(PsError::Typecheck);
            };
            it.journal_dict(dst);
            let pairs = src.borrow().pairs();
            {
                let mut d = dst.borrow_mut();
                for (k, v) in pairs {
                    d.put_obj(&k, v)?;
                }
            }
            it.push(top.clone());
            Ok(())
        }
        // `gstate1 gstate2 copy gstate2` (#394): overwrite gstate2.
        Value::GState(dst) => {
            let src_obj = it.pop()?;
            let Value::GState(src) = &src_obj.value else {
                return Err(PsError::Typecheck);
            };
            it.journal_gstate(dst);
            let value = src.borrow().clone();
            *dst.borrow_mut() = value;
            it.push(top.clone());
            Ok(())
        }
        _ => Err(PsError::Typecheck),
    }
}

pub(crate) fn forall(it: &mut Interp) -> Result<(), PsError> {
    let body = pop_proc(it)?;
    let obj = it.pop()?;
    let src = match &obj.value {
        Value::Array(a) => ForallSrc::Array(a.clone(), 0),
        Value::String(s) => ForallSrc::Str(s.clone(), 0),
        Value::Dict(d) => ForallSrc::Pairs(d.borrow().pairs(), 0),
        _ => return Err(PsError::Typecheck),
    };
    it.begin_forall(body, src)
}

fn array(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    if n > ALLOC_LIMIT {
        return Err(PsError::Limitcheck);
    }
    it.check_alloc(n as usize * std::mem::size_of::<Object>())?;
    it.push(Object::array(vec![Object::lit(Value::Null); n as usize]));
    Ok(())
}

fn string(it: &mut Interp) -> Result<(), PsError> {
    let n = it.pop_int()?;
    if n < 0 {
        return Err(PsError::Rangecheck);
    }
    if n > ALLOC_LIMIT {
        return Err(PsError::Limitcheck);
    }
    it.check_alloc(n as usize)?;
    it.push(Object::string(vec![0u8; n as usize]));
    Ok(())
}

fn aload(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    // `aload` multiplies memory in one step (a 16M-element array pushes 16M
    // operands), so it is checked against a memory limit like `array` is.
    it.check_alloc(a.len() * std::mem::size_of::<Object>())?;
    for el in a.snapshot() {
        it.push(el);
    }
    it.push(obj.clone());
    Ok(())
}

fn astore(it: &mut Interp) -> Result<(), PsError> {
    let obj = it.pop()?;
    let Value::Array(a) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    obj.check_writable()?;
    // Enough operands, checked first: a `stackunderflow` leaves the array
    // as it was, as in Ghostscript, and once the check passes nothing can
    // fail, so the elements skip the operand ledger (#1047).
    let n = a.len();
    let Some(base) = it.operand_stack().len().checked_sub(n) else {
        return Err(PsError::StackUnderflow);
    };
    it.journal_array(a);
    for (i, el) in it.ostack.drain(base..).enumerate() {
        a.put(i, el)?;
    }
    it.push(obj.clone());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::find_bytes;

    /// What `search` used before #1176: the reference the linear search
    /// must agree with.
    fn naive(hay: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() {
            Some(0)
        } else if needle.len() > hay.len() {
            None
        } else {
            hay.windows(needle.len()).position(|w| w == needle)
        }
    }

    #[test]
    fn the_linear_search_finds_what_the_naive_one_found() {
        let cases: &[(&[u8], &[u8])] = &[
            (b"", b""),
            (b"", b"a"),
            (b"abc", b""),
            (b"abc", b"abc"),
            (b"abc", b"abcd"),
            (b"aaab", b"aab"),
            (b"abababac", b"ababac"),
            (b"abcdef", b"ef"),
            (b"abcdef", b"a"),
            (b"abcdef", b"xy"),
            (b"\0\0\0\x01", b"\0\x01"),
            (b"\xff\xfe\xff\xfe\xfd", b"\xfe\xfd"),
        ];
        for &(hay, needle) in cases {
            assert_eq!(
                find_bytes(hay, needle),
                naive(hay, needle),
                "{hay:?} {needle:?}"
            );
        }
        // Every needle cut from a small two-letter alphabet, in every
        // haystack up to eight long: overlapping prefixes and periodic
        // patterns are where a fast search goes wrong.
        let words: Vec<Vec<u8>> = (0..=8u32)
            .flat_map(|n| {
                (0..1u32 << n)
                    .map(move |bits| (0..n).map(|i| b'a' + (bits >> i & 1) as u8).collect())
            })
            .collect();
        for hay in &words {
            for needle in words.iter().filter(|w| w.len() <= 4) {
                assert_eq!(
                    find_bytes(hay, needle),
                    naive(hay, needle),
                    "{hay:?} {needle:?}"
                );
            }
        }
    }
}
