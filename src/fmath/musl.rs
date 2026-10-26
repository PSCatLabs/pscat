//! `exp`, `log` and `pow` as in musl 1.2.5 (`src/math/exp.c`, `log.c`,
//! `pow.c`), which are ARM's optimized-routines versions
//! (MIT licence, Copyright (c) 2018, Arm Limited; the full text is under
//! "Math: musl libm port" in THIRD_PARTY_NOTICES.md). Ported line for line, the
//! generic variant: no fused multiply-add, the table-based argument
//! reduction. The tables are in `tables.rs`.
//!
//! Why this exists (issue #738): the `libm` crate's `exp`, `log` and `pow` are
//! FreeBSD msun's, an older design than musl 1.2's, so they differ from the
//! rounding that musl, Apple's libm and glibc give. pscat's Linux musl builds
//! (and the reference renders made with them) use musl's, so using these keeps
//! Linux output unchanged while making macOS and the browser build match it.
//!
//! The file depends on nothing in this crate, so a test harness can include
//! it by path. Special cases (zero, infinity, NaN, overflow, underflow) return
//! the values musl does; no floating-point exception flags are raised.

// A line-for-line port: keep the C's operand order and spelling so it can be
// compared with musl's source (`lo = one - hi + y + lo`).
#![allow(clippy::assign_op_pattern)]

use super::tables::{
    EXP_INVLN2N, EXP_NEGLN2HIN, EXP_NEGLN2LON, EXP_POLY, EXP_SHIFT, EXP_TAB, LOG_LN2HI, LOG_LN2LO,
    LOG_POLY, LOG_POLY1, LOG_TAB, LOG_TAB2, POW_LN2HI, POW_LN2LO, POW_LOG_POLY, POW_LOG_TAB,
};

const EXP_TABLE_BITS: u32 = 7;
const EXP_N: u64 = 1 << EXP_TABLE_BITS;
const LOG_TABLE_BITS: u32 = 7;
const LOG_N: u64 = 1 << LOG_TABLE_BITS;
const POW_LOG_TABLE_BITS: u32 = 7;
const POW_LOG_N: u64 = 1 << POW_LOG_TABLE_BITS;
const SIGN_BIAS: u32 = 0x800 << EXP_TABLE_BITS;

#[inline]
fn f(bits: u64) -> f64 {
    f64::from_bits(bits)
}

#[inline]
fn top12(x: f64) -> u32 {
    (x.to_bits() >> 52) as u32
}

/// `__math_uflow(sign)`: a signed zero.
#[inline]
fn uflow(sign: u32) -> f64 {
    if sign != 0 { -0.0 } else { 0.0 }
}

/// `__math_oflow(sign)`: a signed infinity.
#[inline]
fn oflow(sign: u32) -> f64 {
    if sign != 0 {
        f64::NEG_INFINITY
    } else {
        f64::INFINITY
    }
}

/// `2^e` for a normal exponent `e`, exact.
#[inline]
fn pow2(e: i32) -> f64 {
    f(((e + 1023) as u64) << 52)
}

/// `e^x`.
pub fn exp(x: f64) -> f64 {
    let mut abstop = top12(x) & 0x7ff;
    let t54 = top12(f(0x3c90_0000_0000_0000)); // top12(0x1p-54)
    if abstop.wrapping_sub(t54) >= top12(512.0) - t54 {
        if abstop.wrapping_sub(t54) >= 0x8000_0000 {
            // Avoid spurious underflow for tiny x. 0 is common input.
            return 1.0 + x;
        }
        if abstop >= top12(1024.0) {
            if x.to_bits() == f64::NEG_INFINITY.to_bits() {
                return 0.0;
            }
            if abstop >= top12(f64::INFINITY) {
                return 1.0 + x;
            }
            return if x.to_bits() >> 63 != 0 {
                uflow(0)
            } else {
                oflow(0)
            };
        }
        // Large x is special cased below.
        abstop = 0;
    }

    // exp(x) = 2^(k/N) * exp(r), with exp(r) in [2^(-1/2N),2^(1/2N)].
    // x = ln2/N*k + r, with int k and r in [-ln2/2N, ln2/2N].
    let z = f(EXP_INVLN2N) * x;
    // z - kd is in [-1, 1] in non-nearest rounding modes.
    let mut kd = z + f(EXP_SHIFT);
    let ki = kd.to_bits();
    kd -= f(EXP_SHIFT);
    let r = x + kd * f(EXP_NEGLN2HIN) + kd * f(EXP_NEGLN2LON);
    // 2^(k/N) ~= scale * (1 + tail).
    let idx = (2 * (ki % EXP_N)) as usize;
    let top = ki << (52 - EXP_TABLE_BITS);
    let tail = f(EXP_TAB[idx]);
    // This is only a valid scale when -1023*N < k < 1024*N.
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    // exp(x) = 2^(k/N) * exp(r) ~= scale + scale * (tail + exp(r) - 1).
    let r2 = r * r;
    let (c2, c3, c4, c5) = (
        f(EXP_POLY[0]),
        f(EXP_POLY[1]),
        f(EXP_POLY[2]),
        f(EXP_POLY[3]),
    );
    let tmp = tail + r + r2 * (c2 + r * c3) + r2 * r2 * (c4 + r * c5);
    if abstop == 0 {
        return specialcase(tmp, sbits, ki);
    }
    let scale = f(sbits);
    scale + scale * tmp
}

/// Natural logarithm.
pub fn log(x: f64) -> f64 {
    const OFF: u64 = 0x3fe6_0000_0000_0000;
    let mut ix = x.to_bits();
    let top = (ix >> 48) as u32;
    let lo_bound = (1.0f64 - f(0x3fb0_0000_0000_0000)).to_bits(); // 1 - 0x1p-4
    let hi_bound = (1.0f64 + f(0x3fb0_9000_0000_0000)).to_bits(); // 1 + 0x1.09p-4
    if ix.wrapping_sub(lo_bound) < hi_bound - lo_bound {
        // Handle close to 1.0 inputs separately.
        if ix == 1.0f64.to_bits() {
            return 0.0;
        }
        let b = |i: usize| f(LOG_POLY1[i]);
        let r = x - 1.0;
        let r2 = r * r;
        let r3 = r * r2;
        let mut y = r3
            * (b(1)
                + r * b(2)
                + r2 * b(3)
                + r3 * (b(4)
                    + r * b(5)
                    + r2 * b(6)
                    + r3 * (b(7) + r * b(8) + r2 * b(9) + r3 * b(10))));
        // Worst-case error is around 0.507 ULP.
        let mut w = r * f(0x41a0_0000_0000_0000); // 0x1p27
        let rhi = r + w - w;
        let rlo = r - rhi;
        w = rhi * rhi * b(0); // B[0] == -0.5.
        let hi = r + w;
        let mut lo = r - hi + w;
        lo += b(0) * rlo * (rhi + r);
        y += lo;
        y += hi;
        return y;
    }
    if top.wrapping_sub(0x0010) >= 0x7ff0 - 0x0010 {
        // x < 0x1p-1022 or inf or nan.
        if ix.wrapping_mul(2) == 0 {
            return f64::NEG_INFINITY; // __math_divzero(1)
        }
        if ix == f64::INFINITY.to_bits() {
            return x; // log(inf) == inf.
        }
        if top & 0x8000 != 0 || top & 0x7ff0 == 0x7ff0 {
            return f64::NAN; // __math_invalid(x)
        }
        // x is subnormal, normalize it.
        ix = (x * f(0x4330_0000_0000_0000)).to_bits(); // x * 0x1p52
        ix = ix.wrapping_sub(52u64 << 52);
    }

    // x = 2^k z; where z is in range [OFF,2*OFF) and exact.
    // The range is split into N subintervals.
    // The ith subinterval contains z and c is near its center.
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - LOG_TABLE_BITS)) % LOG_N) as usize;
    let k = (tmp as i64) >> 52; // arithmetic shift
    let iz = ix.wrapping_sub(tmp & (0xfffu64 << 52));
    let invc = f(LOG_TAB[i][0]);
    let logc = f(LOG_TAB[i][1]);
    let z = f(iz);

    // log(x) = log1p(z/c-1) + log(c) + k*Ln2.
    // r ~= z/c - 1, |r| < 1/(2*N).
    // rounding error: 0x1p-55/N + 0x1p-66.
    let r = (z - f(LOG_TAB2[i][0]) - f(LOG_TAB2[i][1])) * invc;
    let kd = k as f64;

    // hi + lo = r + log(c) + k*Ln2.
    let w = kd * f(LOG_LN2HI) + logc;
    let hi = w + r;
    let lo = w - hi + r + kd * f(LOG_LN2LO);

    // log(x) = lo + (log1p(r) - r) + hi.
    let r2 = r * r;
    let a = |i: usize| f(LOG_POLY[i]);
    lo + r2 * a(0) + r * r2 * (a(1) + r * a(2) + r2 * (a(3) + r * a(4))) + hi
}

/// Returns `y` and `tail` such that `y + tail = log(x)`, `y` rounded and
/// `tail` carrying about 15 more bits. `ix` is the bit representation of `x`,
/// normalized in the subnormal range using the sign bit for the exponent.
fn log_inline(ix: u64) -> (f64, f64) {
    const OFF: u64 = 0x3fe6_9555_0000_0000;
    // x = 2^k z; where z is in range [OFF,2*OFF) and exact.
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> (52 - POW_LOG_TABLE_BITS)) % POW_LOG_N) as usize;
    let k = (tmp as i64) >> 52; // arithmetic shift
    let iz = ix.wrapping_sub(tmp & (0xfffu64 << 52));
    let z = f(iz);
    let kd = k as f64;

    // log(x) = k*Ln2 + log(c) + log1p(z/c-1).
    let invc = f(POW_LOG_TAB[i][0]);
    let logc = f(POW_LOG_TAB[i][1]);
    let logctail = f(POW_LOG_TAB[i][2]);

    // Split z such that rhi, rlo and rhi*rhi are exact and |rlo| <= |r|.
    let zhi = f(iz.wrapping_add(1u64 << 31) & (u64::MAX << 32));
    let zlo = z - zhi;
    let rhi = zhi * invc - 1.0;
    let rlo = zlo * invc;
    let r = rhi + rlo;

    // k*Ln2 + log(c) + r.
    let t1 = kd * f(POW_LN2HI) + logc;
    let t2 = t1 + r;
    let lo1 = kd * f(POW_LN2LO) + logctail;
    let lo2 = t1 - t2 + r;

    let a = |i: usize| f(POW_LOG_POLY[i]);
    let ar = a(0) * r; // A[0] = -0.5.
    let ar2 = r * ar;
    let ar3 = r * ar2;
    // k*Ln2 + log(c) + r + A[0]*r*r.
    let arhi = a(0) * rhi;
    let arhi2 = rhi * arhi;
    let hi = t2 + arhi2;
    let lo3 = rlo * (ar + arhi);
    let lo4 = t2 - hi + arhi2;
    // p = log1p(r) - r - A[0]*r*r.
    let p = ar3 * (a(1) + r * a(2) + ar2 * (a(3) + r * a(4) + ar2 * (a(5) + r * a(6))));
    let lo = lo1 + lo2 + lo3 + lo4 + p;
    let y = hi + lo;
    (y, hi - y + lo)
}

/// `sign * exp(x + xtail)` where `|xtail| < 2^-8/N` and `|xtail| <= |x|`.
/// `sign_bias` is `SIGN_BIAS` or 0 and sets the sign to -1 or 1.
fn exp_inline(x: f64, xtail: f64, sign_bias: u32) -> f64 {
    let mut abstop = top12(x) & 0x7ff;
    let t54 = top12(f(0x3c90_0000_0000_0000)); // top12(0x1p-54)
    if abstop.wrapping_sub(t54) >= top12(512.0) - t54 {
        if abstop.wrapping_sub(t54) >= 0x8000_0000 {
            // Avoid spurious underflow for tiny x. 0 is common input.
            let one = 1.0 + x;
            return if sign_bias != 0 { -one } else { one };
        }
        if abstop >= top12(1024.0) {
            // inf and nan are already handled.
            return if x.to_bits() >> 63 != 0 {
                uflow(sign_bias)
            } else {
                oflow(sign_bias)
            };
        }
        // Large x is special cased below.
        abstop = 0;
    }

    // exp(x) = 2^(k/N) * exp(r), with exp(r) in [2^(-1/2N),2^(1/2N)].
    // x = ln2/N*k + r, with int k and r in [-ln2/2N, ln2/2N].
    let z = f(EXP_INVLN2N) * x;
    let mut kd = z + f(EXP_SHIFT);
    let ki = kd.to_bits();
    kd -= f(EXP_SHIFT);
    let mut r = x + kd * f(EXP_NEGLN2HIN) + kd * f(EXP_NEGLN2LON);
    // The code assumes 2^-200 < |xtail| < 2^-8/N.
    r += xtail;
    // 2^(k/N) ~= scale * (1 + tail).
    let idx = (2 * (ki % EXP_N)) as usize;
    let top = ki.wrapping_add(u64::from(sign_bias)) << (52 - EXP_TABLE_BITS);
    let tail = f(EXP_TAB[idx]);
    // This is only a valid scale when -1023*N < k < 1024*N.
    let sbits = EXP_TAB[idx + 1].wrapping_add(top);
    let r2 = r * r;
    let (c2, c3, c4, c5) = (
        f(EXP_POLY[0]),
        f(EXP_POLY[1]),
        f(EXP_POLY[2]),
        f(EXP_POLY[3]),
    );
    let tmp = tail + r + r2 * (c2 + r * c3) + r2 * r2 * (c4 + r * c5);
    if abstop == 0 {
        return specialcase(tmp, sbits, ki);
    }
    let scale = f(sbits);
    scale + scale * tmp
}

/// `scale + scale * tmp` where the result may overflow or be subnormal: the
/// exponent adjustments of `specialcase` in `exp.c` and `pow.c`. `pow.c`'s
/// version also handles a negative `scale` (an odd power of a negative
/// base); for a positive `scale` it is `exp.c`'s, step for step.
fn specialcase(tmp: f64, mut sbits: u64, ki: u64) -> f64 {
    if ki & 0x8000_0000 == 0 {
        // k > 0, the exponent of scale might have overflowed by <= 460.
        sbits = sbits.wrapping_sub(1009u64 << 52);
        let scale = f(sbits);
        return pow2(1009) * (scale + scale * tmp);
    }
    // k < 0, need special care in the subnormal range.
    sbits = sbits.wrapping_add(1022u64 << 52);
    // Note: sbits is signed scale.
    let scale = f(sbits);
    let mut y = scale + scale * tmp;
    if y.abs() < 1.0 {
        let mut one = 1.0;
        if y < 0.0 {
            one = -1.0;
        }
        let mut lo = scale - y + scale * tmp;
        let hi = one + y;
        lo = one - hi + y + lo;
        y = (hi + lo) - one;
        // Fix the sign of 0.
        if y == 0.0 {
            y = f(sbits & 0x8000_0000_0000_0000);
        }
    }
    pow2(-1022) * y
}

/// 0 if not an integer, 1 if odd, 2 if even. `iy` is the bit representation
/// of a non-zero finite value.
fn checkint(iy: u64) -> u32 {
    let e = ((iy >> 52) & 0x7ff) as i32;
    if e < 0x3ff {
        return 0;
    }
    if e > 0x3ff + 52 {
        return 2;
    }
    if iy & ((1u64 << (0x3ff + 52 - e)) - 1) != 0 {
        return 0;
    }
    if iy & (1u64 << (0x3ff + 52 - e)) != 0 {
        return 1;
    }
    2
}

/// True if `i` is the bit representation of 0, infinity or NaN.
#[inline]
fn zeroinfnan(i: u64) -> bool {
    i.wrapping_mul(2).wrapping_sub(1) >= 2 * f64::INFINITY.to_bits() - 1
}

/// `x` to the power `y`.
pub fn pow(x: f64, y: f64) -> f64 {
    let mut sign_bias = 0u32;
    let mut ix = x.to_bits();
    let iy = y.to_bits();
    let mut topx = top12(x);
    let topy = top12(y);
    if topx.wrapping_sub(0x001) >= 0x7ff - 0x001
        || (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be
    {
        // Note: if |y| > 1075 * ln2 * 2^53 ~= 0x1.749p62 then pow(x,y) = inf/0
        // and if |y| < 2^-54 / 1075 ~= 0x1.e7b6p-65 then pow(x,y) = +-1.
        // Special cases: (x < 0x1p-126 or inf or nan) or
        // (|y| < 0x1p-65 or |y| >= 0x1p63 or nan).
        if zeroinfnan(iy) {
            if iy.wrapping_mul(2) == 0 {
                return 1.0;
            }
            if ix == 1.0f64.to_bits() {
                return 1.0;
            }
            if ix.wrapping_mul(2) > 2 * f64::INFINITY.to_bits()
                || iy.wrapping_mul(2) > 2 * f64::INFINITY.to_bits()
            {
                return x + y;
            }
            if ix.wrapping_mul(2) == 2 * 1.0f64.to_bits() {
                return 1.0;
            }
            if (ix.wrapping_mul(2) < 2 * 1.0f64.to_bits()) == (iy >> 63 == 0) {
                return 0.0; // |x|<1 && y==inf or |x|>1 && y==-inf.
            }
            return y * y;
        }
        if zeroinfnan(ix) {
            let mut x2 = x * x;
            if ix >> 63 != 0 && checkint(iy) == 1 {
                x2 = -x2;
            }
            return if iy >> 63 != 0 { 1.0 / x2 } else { x2 };
        }
        // Here x and y are non-zero finite.
        if ix >> 63 != 0 {
            // Finite x < 0.
            let yint = checkint(iy);
            if yint == 0 {
                return f64::NAN; // __math_invalid(x)
            }
            if yint == 1 {
                sign_bias = SIGN_BIAS;
            }
            ix &= 0x7fff_ffff_ffff_ffff;
            topx &= 0x7ff;
        }
        if (topy & 0x7ff).wrapping_sub(0x3be) >= 0x43e - 0x3be {
            // Note: sign_bias == 0 here because y is not odd.
            if ix == 1.0f64.to_bits() {
                return 1.0;
            }
            if (topy & 0x7ff) < 0x3be {
                // |y| < 2^-65, x^y ~= 1 + y*log(x).
                return if ix > 1.0f64.to_bits() {
                    1.0 + y
                } else {
                    1.0 - y
                };
            }
            return if (ix > 1.0f64.to_bits()) == (topy < 0x800) {
                oflow(0)
            } else {
                uflow(0)
            };
        }
        if topx == 0 {
            // Normalize subnormal x so exponent becomes negative.
            ix = (x * f(0x4330_0000_0000_0000)).to_bits(); // x * 0x1p52
            ix &= 0x7fff_ffff_ffff_ffff;
            ix = ix.wrapping_sub(52u64 << 52);
        }
    }

    let (hi, lo) = log_inline(ix);
    let yhi = f(iy & (u64::MAX << 27));
    let ylo = y - yhi;
    let lhi = f(hi.to_bits() & (u64::MAX << 27));
    let llo = hi - lhi + lo;
    let ehi = yhi * lhi;
    let elo = ylo * lhi + y * llo; // |elo| < |ehi| * 2^-25.
    exp_inline(ehi, elo, sign_bias)
}
