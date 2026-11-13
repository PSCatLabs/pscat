//! The `numarray | numstring` operand (PLRM 3rd ed. 3.14.5, "Encoded
//! Number Strings"; issue #1180): a list of numbers given either as an
//! array or as a string packed in the homogeneous number array format.
//!
//! ```text
//! byte 0     149 (the binary token type of a homogeneous number array)
//! byte 1     representation r
//! bytes 2-3  count of numbers, in r's byte order
//! bytes 4..  the numbers
//! ```
//!
//! | r       | each number                          | byte order  |
//! |---------|--------------------------------------|-------------|
//! | 0-31    | 32-bit fixed point, r fraction bits   | high first  |
//! | 32-47   | 16-bit fixed point, r-32 fraction bits| high first  |
//! | 48      | IEEE binary32                        | high first  |
//! | 49      | binary32, native byte order          | count high  |
//! | 128-159 | 32-bit fixed point, r-128 bits        | low first   |
//! | 160-175 | 16-bit fixed point, r-160 bits        | low first   |
//! | 176     | IEEE binary32                        | low first   |
//! | 177     | binary32, native byte order          | count low   |
//!
//! Fixed-point numbers are signed. The checks follow Ghostscript 10.07.1
//! (`tests/xshow.rs` compares them): a string shorter than the header or
//! not starting with 149 is a `typecheck`; any other representation, or a
//! count that is not the number of whole values after the header, is a
//! `rangecheck` (one stray trailing byte is allowed, a whole extra value is
//! not). A non-finite real is an `undefinedresult`. Those header checks come
//! when the operand is read ([`NumOperand::of`]); the values are read, and
//! a non-finite one found, only in [`NumOperand::into_numbers`], because gs
//! checks the count against the text and the current point first.
//!
//! `xshow`, `yshow` and `xyshow` take this operand today. The Level 2
//! `rectfill`, `rectstroke` and `rectclip` accept it too in Ghostscript;
//! pscat's take arrays only.

use crate::binobj::ByteOrder;
use crate::error::PsError;
use crate::object::{Object, PsArray, Value};

/// The binary token type that heads an encoded number string.
const HOMOGENEOUS_ARRAY: u8 = 149;

/// A `numarray | numstring` operand, its structure checked but not its
/// values: Ghostscript checks the count against the text before it checks
/// the current point, and the values (array element types, non-finite
/// reals) after it, so the two steps are separate.
pub(crate) enum NumOperand {
    /// An array (literal, executable or packed): its elements are checked
    /// by [`NumOperand::into_numbers`].
    Array(PsArray),
    /// An encoded number string whose header has been checked.
    Encoded(Encoded),
}

impl NumOperand {
    /// Read `obj` as a number list: `typecheck` for anything but an array
    /// or a string, and the header checks of [`Encoded::parse`] for a
    /// string.
    pub(crate) fn of(obj: &Object) -> Result<NumOperand, PsError> {
        match &obj.value {
            Value::Array(a) => Ok(NumOperand::Array(a.clone())),
            Value::String(s) => Ok(NumOperand::Encoded(Encoded::parse(s.to_vec())?)),
            _ => Err(PsError::Typecheck),
        }
    }

    /// How many numbers the operand holds.
    pub(crate) fn len(&self) -> usize {
        match self {
            NumOperand::Array(a) => a.len(),
            NumOperand::Encoded(e) => e.count,
        }
    }

    /// The numbers, snapshotted: a `typecheck` if any array element (the
    /// ones past those a show needs included, as in gs) is not a number,
    /// an `undefinedresult` for a non-finite encoded real.
    pub(crate) fn into_numbers(self) -> Result<Vec<f64>, PsError> {
        match self {
            NumOperand::Encoded(e) => e.values(),
            NumOperand::Array(a) => (0..a.len())
                .map(|i| match a.get(i).map(|o| o.value) {
                    Some(Value::Integer(n)) => Ok(n as f64),
                    Some(Value::Real(r)) => Ok(r),
                    _ => Err(PsError::Typecheck),
                })
                .collect(),
        }
    }
}

/// How one representation byte lays out its numbers.
#[derive(Clone, Copy)]
enum Repr {
    /// Signed fixed point of `bytes` (2 or 4) bytes, `frac` fraction bits.
    Fixed { bytes: usize, frac: u32 },
    /// IEEE binary32.
    Real,
    /// IEEE binary32 in the host's byte order.
    NativeReal,
}

impl Repr {
    fn width(self) -> usize {
        match self {
            Repr::Fixed { bytes, .. } => bytes,
            Repr::Real | Repr::NativeReal => 4,
        }
    }
}

/// The byte order (of the count, and of the numbers unless native) and
/// layout a representation byte names; `None` for an undefined one.
fn representation(r: u8) -> Option<(ByteOrder, Repr)> {
    let (order, base) = if r < 128 {
        (ByteOrder::High, r)
    } else {
        (ByteOrder::Low, r - 128)
    };
    let repr = match base {
        0..=31 => Repr::Fixed {
            bytes: 4,
            frac: u32::from(base),
        },
        32..=47 => Repr::Fixed {
            bytes: 2,
            frac: u32::from(base - 32),
        },
        48 => Repr::Real,
        49 => Repr::NativeReal,
        _ => return None,
    };
    Some((order, repr))
}

/// An encoded number string with a valid header: its layout and how many
/// numbers it holds, the values still unread.
pub(crate) struct Encoded {
    bytes: Vec<u8>,
    order: ByteOrder,
    repr: Repr,
    count: usize,
}

impl Encoded {
    /// Check the header and the count (see the module comment for the
    /// errors). The values are not looked at.
    fn parse(bytes: Vec<u8>) -> Result<Encoded, PsError> {
        if bytes.len() < 4 || bytes[0] != HOMOGENEOUS_ARRAY {
            return Err(PsError::Typecheck);
        }
        let (order, repr) = representation(bytes[1]).ok_or(PsError::Rangecheck)?;
        let count = usize::from(order.u16(&bytes[2..4]));
        if (bytes.len() - 4) / repr.width() != count {
            return Err(PsError::Rangecheck);
        }
        Ok(Encoded {
            bytes,
            order,
            repr,
            count,
        })
    }

    /// The numbers: `undefinedresult` if one is not finite.
    fn values(&self) -> Result<Vec<f64>, PsError> {
        let (order, repr) = (self.order, self.repr);
        self.bytes[4..]
            .chunks_exact(repr.width())
            .map(|b| {
                let v = match repr {
                    Repr::Fixed { bytes: 2, frac } => {
                        f64::from(order.u16(b) as i16) / f64::from(1u32 << frac)
                    }
                    Repr::Fixed { frac, .. } => {
                        f64::from(order.u32(b) as i32) / (1u64 << frac) as f64
                    }
                    Repr::Real => f64::from(f32::from_bits(order.u32(b))),
                    Repr::NativeReal => f64::from(f32::from_ne_bytes([b[0], b[1], b[2], b[3]])),
                };
                if v.is_finite() {
                    Ok(v)
                } else {
                    Err(PsError::UndefinedResult)
                }
            })
            .collect()
    }
}

/// Decode an encoded number string: the header checks, then the values.
#[cfg(test)]
fn decode(bytes: &[u8]) -> Result<Vec<f64>, PsError> {
    Encoded::parse(bytes.to_vec())?.values()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        let digits: Vec<u8> = s.bytes().filter(u8::is_ascii_hexdigit).collect();
        digits
            .chunks(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect()
    }

    fn nums(s: &str) -> Result<Vec<f64>, PsError> {
        decode(&hex(s))
    }

    #[test]
    fn every_representation_decodes() {
        let one_two_three = Ok(vec![1.0, 2.0, 3.0]);
        // 16-bit fixed point, high and low byte first, 0 and 1 fraction bits.
        assert_eq!(nums("95 20 0003 0001 0002 0003"), one_two_three);
        assert_eq!(nums("95 21 0003 0002 0004 0006"), one_two_three);
        assert_eq!(nums("95 a0 0300 0100 0200 0300"), one_two_three);
        // 32-bit fixed point.
        assert_eq!(nums("95 00 0003 00000001 00000002 00000003"), one_two_three);
        assert_eq!(nums("95 80 0300 01000000 02000000 03000000"), one_two_three);
        assert_eq!(
            nums("95 10 0003 00018000 00020000 00030000"),
            Ok(vec![1.5, 2.0, 3.0])
        );
        // IEEE reals: 1.5 2 3.
        let reals = Ok(vec![1.5, 2.0, 3.0]);
        assert_eq!(nums("95 30 0003 3fc00000 40000000 40400000"), reals);
        assert_eq!(nums("95 b0 0300 0000c03f 00000040 00004040"), reals);
        // Native reals: the count keeps its group's order, the values are
        // the host's (little-endian on every pscat target, as in gs).
        let native: Vec<u8> = [1.5f32, 2.0, 3.0]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect();
        for (r, count) in [(0x31, [0, 3]), (0xb1, [3, 0])] {
            let mut b = vec![149, r, count[0], count[1]];
            b.extend_from_slice(&native);
            assert_eq!(decode(&b), reals, "r = {r}");
        }
    }

    /// The header is checked when the operand is read, the values only
    /// when they are taken, so a show can report its count and current
    /// point errors first, as gs does.
    #[test]
    fn values_are_checked_after_the_header() {
        let inf = Object::lit(Value::String(crate::object::PsString::from_bytes(hex(
            "95 30 0002 3f800000 7f800000",
        ))));
        let op = NumOperand::of(&inf).expect("the header is fine");
        assert_eq!(op.len(), 2);
        assert_eq!(op.into_numbers(), Err(PsError::UndefinedResult));
    }

    #[test]
    fn fixed_point_is_signed() {
        assert_eq!(nums("95 20 0003 ffff fffe 0003"), Ok(vec![-1.0, -2.0, 3.0]));
        assert_eq!(
            nums("95 00 0002 ffffffff 80000000"),
            Ok(vec![-1.0, -2147483648.0])
        );
        // The most fraction bits each width allows: -1 exactly.
        assert_eq!(nums("95 1f 0001 80000000"), Ok(vec![-1.0]));
        assert_eq!(nums("95 2f 0001 8000"), Ok(vec![-1.0]));
    }

    #[test]
    fn bad_strings_error_like_ghostscript() {
        use PsError::{Rangecheck, Typecheck, UndefinedResult};
        assert_eq!(nums(""), Err(Typecheck));
        assert_eq!(nums("95 20 00"), Err(Typecheck));
        assert_eq!(nums("94 20 0001 0001"), Err(Typecheck));
        for r in ["32", "40", "7f", "b2", "c8", "ff"] {
            assert_eq!(nums(&format!("95 {r} 0001 0001")), Err(Rangecheck), "{r}");
        }
        // Too few values for the count, or a whole value too many.
        assert_eq!(nums("95 20 0004 0001 0002 0003"), Err(Rangecheck));
        assert_eq!(nums("95 20 0003 0001 0002 0003 0004"), Err(Rangecheck));
        // A stray byte short of a value is ignored, as gs ignores it.
        assert_eq!(
            nums("95 20 0003 0001 0002 0003 00"),
            Ok(vec![1.0, 2.0, 3.0])
        );
        assert_eq!(nums("95 20 0000"), Ok(vec![]));
        assert_eq!(nums("95 30 0001 7f800000"), Err(UndefinedResult));
        assert_eq!(nums("95 30 0001 7fc00000"), Err(UndefinedResult));
    }
}
