//! Binary object sequences (PLRM 3.14.4 and 3.14.5; issue #733).
//!
//! A sequence is a compact binary form of a PostScript object tree: a
//! header, an array of 8-byte object slots, and the character data of the
//! strings and names. `printobject` and `writeobject` produce one,
//! the scanner reads one back (a token whose first byte is 128 to 131).
//!
//! ```text
//! short header (4 bytes)       type  count  total(2)
//! long header  (8 bytes)       type  0      count(2)  total(4)
//! object slot  (8 bytes)       attr/type  tag  length(2)  value(4)
//! ```
//!
//! `type` is the token type: 128 and 129 for IEEE numbers with the high
//! or the low byte first, 130 and 131 for the same with native reals. A
//! pscat host is always little-endian IEEE, so 130 and 131 read and write
//! exactly as 128 and 129 do (Ghostscript does the same). `total` counts
//! the whole sequence, header included. The short header is used when
//! the sequence is at most 65535 bytes and has at most 255 top-level
//! objects.
//!
//! A slot's first byte is the object type with bit 7 set for an
//! executable object. The types: 0 null, 1 integer, 2 real, 3 name,
//! 4 boolean, 5 string, 6 immediately evaluated name, 9 array, 10 mark.
//! The second byte is the tag (only meaningful on a top-level object).
//! The `length` field is the character count of a string or name or the
//! element count of an array; for a real it is 0 for an IEEE binary32 or
//! the number of fraction bits of a fixed-point integer. The `value` field
//! is the integer, the real, 0 or 1 for a boolean, or for a string, name
//! or array the offset of its data from the start of the first top-level
//! object (the byte after the header).
//!
//! Layout. The writer lays arrays out breadth first: the top-level slots,
//! then the elements of each array in the order the arrays were met.
//! Strings and names go after every slot, in the order their slots were
//! written. Nothing is shared: a composite reached twice is written twice.
//! Ghostscript lays its output out the same way, which is what the byte
//! comparison in `tests/binobj.rs` checks.
//!
//! What can be written. Integers (32-bit signed), reals (IEEE binary32,
//! so the writer rounds; infinities, NaN and magnitudes beyond binary32
//! are `limitcheck`), booleans, null, mark, names, strings and arrays.
//! Strings, names and arrays are at most 65535 long (the length field),
//! the empty name is refused (its length 0 is the system name table's
//! marker), nesting is at most [`MAX_NESTING`] deep and the whole sequence
//! at most [`MAX_BYTES`]. Those are all `limitcheck`; anything else (a
//! dictionary, a file, an operator) is `typecheck`.
//!
//! What can be read. The same, plus fixed-point reals and immediately
//! evaluated names. Names with length 0 (system name table indices) and
//! 0xFFFF (user name table) are a `syntaxerror`: pscat has neither table.
//! Every offset and length is checked against the sequence. A sequence
//! decodes to at most one object per 8 bytes of its body, whichever
//! slots they are, so an offset that points back (a cycle) or a layout that
//! shares one array between many parents cannot multiply the input: it
//! runs out of that budget (`syntaxerror`) long before memory does.
//! Nesting beyond [`MAX_NESTING`] is `limitcheck`.

use std::collections::VecDeque;

use crate::error::PsError;
use crate::object::{Object, PsArray, Value};

/// Deepest nesting of arrays in a sequence, both written and read.
pub const MAX_NESTING: usize = 500;

/// Longest sequence the writer produces (header, slots and characters).
/// Also what stops a cyclic array: it would otherwise grow without end.
pub const MAX_BYTES: usize = 64 << 20;

/// Bytes of a short header; a long one is twice that.
const SHORT_HEADER: usize = 4;
const LONG_HEADER: usize = 8;

/// Which byte of a multi-byte field comes first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ByteOrder {
    High,
    Low,
}

impl ByteOrder {
    /// The byte order of a binary object sequence token (128 to 131).
    pub fn of_token(b: u8) -> Option<ByteOrder> {
        match b {
            128 | 130 => Some(ByteOrder::High),
            129 | 131 => Some(ByteOrder::Low),
            _ => None,
        }
    }

    /// The byte order of a `setobjectformat` value (1 to 4).
    pub fn of_format(fmt: u8) -> ByteOrder {
        if fmt % 2 == 1 {
            ByteOrder::High
        } else {
            ByteOrder::Low
        }
    }

    fn put_u16(self, out: &mut Vec<u8>, v: u16) {
        out.extend_from_slice(&match self {
            ByteOrder::High => v.to_be_bytes(),
            ByteOrder::Low => v.to_le_bytes(),
        });
    }

    fn put_u32(self, out: &mut Vec<u8>, v: u32) {
        out.extend_from_slice(&match self {
            ByteOrder::High => v.to_be_bytes(),
            ByteOrder::Low => v.to_le_bytes(),
        });
    }

    fn u16(self, b: &[u8]) -> u16 {
        let a = [b[0], b[1]];
        match self {
            ByteOrder::High => u16::from_be_bytes(a),
            ByteOrder::Low => u16::from_le_bytes(a),
        }
    }

    fn u32(self, b: &[u8]) -> u32 {
        let a = [b[0], b[1], b[2], b[3]];
        match self {
            ByteOrder::High => u32::from_be_bytes(a),
            ByteOrder::Low => u32::from_le_bytes(a),
        }
    }
}

fn syntax(msg: &str) -> PsError {
    PsError::Syntax(format!("binary object sequence: {msg}"))
}

/// What the first bytes of a sequence say about its extent: `(header
/// length, total length including the header, top-level object count)`.
///
/// `head` must hold the first four bytes, and the first eight when the
/// second byte is 0 (the caller reads that many before asking).
pub fn parse_header(head: &[u8]) -> Result<(usize, usize, usize), PsError> {
    let order = ByteOrder::of_token(head[0]).ok_or_else(|| syntax("not a sequence token"))?;
    let (hdr, total, count) = if head[1] != 0 {
        (
            SHORT_HEADER,
            order.u16(&head[2..4]) as usize,
            head[1] as usize,
        )
    } else {
        if head.len() < LONG_HEADER {
            return Err(syntax("truncated header"));
        }
        (
            LONG_HEADER,
            order.u32(&head[4..8]) as usize,
            order.u16(&head[2..4]) as usize,
        )
    };
    if total < hdr || count.saturating_mul(8) > total - hdr {
        return Err(syntax("bad header"));
    }
    Ok((hdr, total, count))
}

/// Whether the header at the start of `head` is the long form, so the
/// scanner knows how many bytes to read before calling [`parse_header`].
pub fn header_is_long(head: &[u8]) -> bool {
    head.len() >= 2 && head[1] == 0
}

// ---------------------------------------------------------------- reading

/// Bound on what one sequence can decode to, and the recursion state.
struct Reader<'a> {
    order: ByteOrder,
    /// Everything after the header.
    body: &'a [u8],
    /// Slots still allowed to be decoded: one per 8 bytes of body.
    budget: usize,
    /// Characters of strings and names still allowed to be copied. Several
    /// slots may point at the same bytes, and each copy is new storage, so
    /// without this a 73 KB sequence could ask for 64 MiB of strings. A
    /// writer lays every string's characters out once, so the total can
    /// never legitimately exceed the body.
    chars_left: usize,
}

/// Decode a whole sequence (header included) into its top-level objects.
/// `immediate` resolves an immediately evaluated name.
pub fn decode(
    bytes: &[u8],
    immediate: &mut dyn FnMut(&str) -> Result<Object, PsError>,
) -> Result<Vec<Object>, PsError> {
    if bytes.len() < SHORT_HEADER {
        return Err(syntax("truncated header"));
    }
    let order = ByteOrder::of_token(bytes[0]).ok_or_else(|| syntax("not a sequence token"))?;
    let (hdr, total, count) = parse_header(bytes)?;
    if bytes.len() < total {
        return Err(syntax("sequence too short"));
    }
    let body = &bytes[hdr..total];
    let mut r = Reader {
        order,
        body,
        budget: body.len() / 8,
        chars_left: body.len(),
    };
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        out.push(r.object(i * 8, 0, immediate)?);
    }
    Ok(out)
}

impl Reader<'_> {
    /// Characters at `off` (relative to the body), `len` of them.
    fn chars(&mut self, off: usize, len: usize) -> Result<&[u8], PsError> {
        self.chars_left = self
            .chars_left
            .checked_sub(len)
            .ok_or_else(|| syntax("strings overlap or repeat"))?;
        off.checked_add(len)
            .and_then(|end| self.body.get(off..end))
            .ok_or_else(|| syntax("offset out of range"))
    }

    fn name_text(&mut self, len: usize, off: usize) -> Result<String, PsError> {
        if len == 0 || len == 0xFFFF {
            return Err(syntax(
                "system and user name table indices are not supported",
            ));
        }
        Ok(String::from_utf8_lossy(self.chars(off, len)?).into_owned())
    }

    fn object(
        &mut self,
        at: usize,
        depth: usize,
        immediate: &mut dyn FnMut(&str) -> Result<Object, PsError>,
    ) -> Result<Object, PsError> {
        if depth > MAX_NESTING {
            return Err(PsError::Limitcheck);
        }
        if self.budget == 0 {
            return Err(syntax("objects overlap or repeat"));
        }
        self.budget -= 1;
        let slot = at
            .checked_add(8)
            .and_then(|end| self.body.get(at..end))
            .ok_or_else(|| syntax("object slot out of range"))?;
        let exec = slot[0] & 0x80 != 0;
        let len = self.order.u16(&slot[2..4]) as usize;
        let val = self.order.u32(&slot[4..8]);
        let value = match slot[0] & 0x7f {
            0 => Value::Null,
            1 => Value::Integer(i64::from(val as i32)),
            2 => Value::Real(real_of(len, val)?),
            3 => Value::Name(crate::name::intern(&self.name_text(len, val as usize)?)),
            4 => Value::Boolean(val != 0),
            5 => Value::String(crate::object::PsString::from_bytes(
                self.chars(val as usize, len)?.to_vec(),
            )),
            6 => {
                let text = self.name_text(len, val as usize)?;
                return immediate(&text);
            }
            9 => {
                let start = val as usize;
                len.checked_mul(8)
                    .and_then(|n| start.checked_add(n))
                    .filter(|&e| e <= self.body.len())
                    .ok_or_else(|| syntax("array out of range"))?;
                let mut items = Vec::with_capacity(len.min(self.budget));
                for i in 0..len {
                    items.push(self.object(start + i * 8, depth + 1, immediate)?);
                }
                Value::Array(PsArray::from_vec(items))
            }
            10 => Value::Mark,
            _ => return Err(syntax("invalid object type")),
        };
        Ok(Object {
            value,
            executable: exec,
        })
    }
}

/// A real slot: IEEE binary32 when `len` is 0, else the 32-bit integer
/// `val` divided by 2 to the `len`.
fn real_of(len: usize, val: u32) -> Result<f64, PsError> {
    if len == 0 {
        let x = f64::from(f32::from_bits(val));
        if x.is_finite() {
            Ok(x)
        } else {
            Err(PsError::UndefinedResult)
        }
    } else if len <= 32 {
        Ok(f64::from(val as i32) / (1u64 << len) as f64)
    } else {
        Err(syntax("invalid real number"))
    }
}

// ---------------------------------------------------------------- writing

struct Writer {
    order: ByteOrder,
    /// Object slots, from the first top-level object.
    body: Vec<u8>,
    /// Characters of every string and name.
    chars: Vec<u8>,
    /// Slots whose value field holds an offset into `chars`, to be moved
    /// past the slots once their number is known.
    fix: Vec<usize>,
    /// Arrays still to be laid out: slot, array, nesting depth.
    queue: VecDeque<(usize, PsArray, usize)>,
}

/// Encode one object as a sequence in the byte order of `fmt`
/// (a `setobjectformat` value, 1 to 4), with `tag` in its tag byte.
/// `charge` hears about the bytes produced so far after each array, so
/// the caller can apply its step, time and memory limits.
pub fn encode(
    obj: &Object,
    tag: u8,
    fmt: u8,
    charge: &mut dyn FnMut(usize) -> Result<(), PsError>,
) -> Result<Vec<u8>, PsError> {
    let order = ByteOrder::of_format(fmt);
    let mut w = Writer {
        order,
        body: Vec::new(),
        chars: Vec::new(),
        fix: Vec::new(),
        queue: VecDeque::new(),
    };
    w.slot(obj, 0)?;
    w.body[1] = tag;
    let mut charged = 0;
    while let Some((slot, arr, depth)) = w.queue.pop_front() {
        let off = w.body.len() as u32;
        w.set_value(slot, off);
        for i in 0..arr.len() {
            let el = arr.get(i).ok_or(PsError::Rangecheck)?;
            w.slot(&el, depth + 1)?;
            // After every element, not every array: one array can name the
            // same 65535-byte string 65535 times, which is 4 GiB of copies
            // if the check waits for the array's end.
            let used = w.body.len() + w.chars.len();
            if used > MAX_BYTES {
                return Err(PsError::Limitcheck);
            }
            if used - charged >= 1024 {
                charge(used - charged)?;
                charged = used;
            }
        }
    }
    let slots_len = w.body.len();
    for slot in std::mem::take(&mut w.fix) {
        let off = w.value_at(slot) + slots_len as u32;
        w.set_value(slot, off);
    }
    let payload = slots_len + w.chars.len();
    let short = SHORT_HEADER + payload <= 0xFFFF;
    let hdr = if short { SHORT_HEADER } else { LONG_HEADER };
    let mut out = Vec::with_capacity(hdr + payload);
    out.push(127 + fmt);
    if short {
        out.push(1);
        order.put_u16(&mut out, (hdr + payload) as u16);
    } else {
        out.push(0);
        order.put_u16(&mut out, 1);
        order.put_u32(&mut out, (hdr + payload) as u32);
    }
    out.extend_from_slice(&w.body);
    out.extend_from_slice(&w.chars);
    Ok(out)
}

impl Writer {
    fn value_at(&self, slot: usize) -> u32 {
        let b = &self.body[slot * 8 + 4..slot * 8 + 8];
        self.order.u32(b)
    }

    fn set_value(&mut self, slot: usize, v: u32) {
        let bytes = match self.order {
            ByteOrder::High => v.to_be_bytes(),
            ByteOrder::Low => v.to_le_bytes(),
        };
        self.body[slot * 8 + 4..slot * 8 + 8].copy_from_slice(&bytes);
    }

    /// Append the slot for `o`; queue it if it is an array.
    fn slot(&mut self, o: &Object, depth: usize) -> Result<(), PsError> {
        let index = self.body.len() / 8;
        let exec = if o.executable { 0x80 } else { 0 };
        let (ty, len, val): (u8, u16, u32) = match &o.value {
            Value::Null => (0, 0, 0),
            Value::Integer(i) => (
                1,
                0,
                i32::try_from(*i).map_err(|_| PsError::Limitcheck)? as u32,
            ),
            Value::Real(r) => {
                let f = *r as f32;
                if !r.is_finite() || !f.is_finite() {
                    return Err(PsError::Limitcheck);
                }
                (2, 0, f.to_bits())
            }
            Value::Boolean(b) => (4, 0, u32::from(*b)),
            Value::Mark => (10, 0, 0),
            Value::Name(n) => {
                let text = n.as_bytes();
                if text.is_empty() || text.len() >= 0xFFFF {
                    return Err(PsError::Limitcheck);
                }
                self.fix.push(index);
                let off = self.chars.len() as u32;
                self.chars.extend_from_slice(text);
                (3, text.len() as u16, off)
            }
            Value::String(s) => {
                if s.len() > 0xFFFF {
                    return Err(PsError::Limitcheck);
                }
                self.fix.push(index);
                let off = self.chars.len() as u32;
                self.chars.extend_from_slice(&s.borrow_bytes());
                (5, s.len() as u16, off)
            }
            Value::Array(a) => {
                if a.len() > 0xFFFF || depth >= MAX_NESTING {
                    return Err(PsError::Limitcheck);
                }
                self.queue.push_back((index, a.clone(), depth));
                (9, a.len() as u16, 0)
            }
            _ => return Err(PsError::Typecheck),
        };
        self.body.push(ty | exec);
        self.body.push(0);
        self.order.put_u16(&mut self.body, len);
        self.order.put_u32(&mut self.body, val);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(o: &Object, fmt: u8) -> Vec<u8> {
        encode(o, 0, fmt, &mut |_| Ok(())).unwrap()
    }

    fn dec(b: &[u8]) -> Vec<Object> {
        decode(b, &mut |_| Err(PsError::Undefined("imm".into()))).unwrap()
    }

    #[test]
    fn integer_bytes_match_ghostscript() {
        let b = enc(&Object::int(42), 1);
        assert_eq!(b, [0x80, 1, 0, 12, 1, 0, 0, 0, 0, 0, 0, 42]);
        let b = enc(&Object::int(42), 2);
        assert_eq!(b, [0x81, 1, 12, 0, 1, 0, 0, 0, 42, 0, 0, 0]);
    }

    #[test]
    fn roundtrip_nested() {
        let o = Object::array(vec![
            Object::int(-7),
            Object::real(2.5),
            Object::name("abc"),
            Object::string(b"hi".to_vec()),
            Object::array(vec![Object::int(3), Object::bool(true)]),
            Object::procedure(vec![Object::exec(Value::Name(crate::name::intern("add")))]),
            Object::lit(Value::Null),
            Object::lit(Value::Mark),
        ]);
        for fmt in 1..=4 {
            let back = dec(&enc(&o, fmt));
            assert_eq!(back.len(), 1);
            assert_eq!(back[0].repr(), o.repr(), "format {fmt}");
        }
    }

    #[test]
    fn long_header_roundtrip() {
        let big = Object::string(vec![7; 65_000]);
        let o = Object::array(vec![big.clone(), big]);
        for fmt in [1, 2] {
            let b = enc(&o, fmt);
            assert_eq!(b[1], 0, "long form");
            assert_eq!(dec(&b)[0].repr(), o.repr());
        }
    }

    #[test]
    fn fixed_point_reals() {
        // 1.5 with 8 fraction bits.
        let b = [0x80, 1, 0, 12, 2, 0, 0, 8, 0, 0, 1, 0x80];
        assert_eq!(dec(&b)[0].repr(), "1.5");
    }

    #[test]
    fn bad_sequences_error_without_panicking() {
        // Truncated, offsets out of range, a cycle, a repeated array.
        let cases: Vec<Vec<u8>> = vec![
            vec![0x80, 1, 0, 12, 1, 0, 0, 0, 0, 0, 0],
            vec![0x80, 1, 0, 12, 5, 0, 0, 2, 0, 0, 0, 8],
            vec![0x80, 1, 0, 12, 9, 0, 0, 1, 0, 0, 0, 0],
            vec![0x80, 1, 0, 12, 9, 0, 0, 1, 0, 0, 0, 1],
            vec![0x80, 1, 0, 12, 9, 0, 0, 0xff, 0, 0, 0, 8],
            vec![0x80, 1, 0, 12, 8, 0, 0, 0, 0, 0, 0, 0],
            vec![0x80, 1, 0, 12, 3, 0, 0, 0, 0, 0, 0, 8],
            vec![0x80, 0, 0, 1, 0xff, 0xff, 0xff, 0xff],
            vec![0x80, 255, 0, 4],
            vec![0x80],
        ];
        for c in cases {
            assert!(decode(&c, &mut |_| Ok(Object::int(0))).is_err(), "{c:02x?}");
        }
    }

    #[test]
    fn cycles_and_sharing_cannot_multiply_the_input() {
        // An array whose single element is itself (offset 0 is its slot).
        let b = [0x80, 1, 0, 12, 9, 0, 0, 1, 0, 0, 0, 0];
        assert!(decode(&b, &mut |_| Ok(Object::int(0))).is_err());
        // Two elements that both point at one array of two integers
        // (offset 24). The body holds 5 slots, and decoding would take 7.
        let mut b = vec![0x80, 1, 0, 44];
        for slot in [
            [9, 0, 0, 2, 0, 0, 0, 8],
            [9, 0, 0, 2, 0, 0, 0, 24],
            [9, 0, 0, 2, 0, 0, 0, 24],
            [1, 0, 0, 0, 0, 0, 0, 5],
            [1, 0, 0, 0, 0, 0, 0, 6],
        ] {
            b.extend_from_slice(&slot);
        }
        let err = decode(&b, &mut |_| Ok(Object::int(0))).unwrap_err();
        assert!(err.to_string().contains("overlap"), "{err}");
    }

    /// Random bytes behind a valid header never panic or hang: every
    /// outcome is an object or an error.
    #[test]
    fn random_bodies_never_panic() {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for round in 0..4000 {
            let n = (next() % 120) as usize;
            let mut body: Vec<u8> = (0..n).map(|_| next() as u8).collect();
            // Make most slots plausible: a known type, small lengths.
            for slot in body.chunks_mut(8) {
                if round % 2 == 0 && slot.len() == 8 {
                    slot[0] = [0, 1, 2, 3, 4, 5, 6, 9, 10, 0x83, 0x89][(next() % 11) as usize];
                    slot[2] = 0;
                    slot[3] %= 6;
                    slot[4] = 0;
                    slot[5] = 0;
                    slot[6] = 0;
                    slot[7] %= 70;
                }
            }
            let total = 4 + body.len();
            let order = [0x80u8, 0x81][(next() % 2) as usize];
            let mut bytes = vec![order, 1];
            if order == 0x80 {
                bytes.extend_from_slice(&(total as u16).to_be_bytes());
            } else {
                bytes.extend_from_slice(&(total as u16).to_le_bytes());
            }
            bytes.extend_from_slice(&body);
            let _ = decode(&bytes, &mut |_| Ok(Object::int(0)));
        }
    }

    #[test]
    fn writer_limits() {
        let w = |o: Object| encode(&o, 0, 1, &mut |_| Ok(())).unwrap_err();
        assert_eq!(w(Object::int(1 << 31)), PsError::Limitcheck);
        assert_eq!(w(Object::real(1e39)), PsError::Limitcheck);
        assert_eq!(w(Object::real(f64::NAN)), PsError::Limitcheck);
        assert_eq!(w(Object::string(vec![0; 65_536])), PsError::Limitcheck);
        assert_eq!(w(Object::name("")), PsError::Limitcheck);
        assert_eq!(
            w(Object::array(vec![Object::int(0); 65_536])),
            PsError::Limitcheck
        );
        let mut deep = Object::array(vec![]);
        for _ in 0..MAX_NESTING + 2 {
            deep = Object::array(vec![deep]);
        }
        assert_eq!(w(deep), PsError::Limitcheck);
        assert_eq!(
            w(Object::lit(Value::Dict(std::rc::Rc::new(
                std::cell::RefCell::new(crate::object::Dict::new())
            )))),
            PsError::Typecheck
        );
    }
}
