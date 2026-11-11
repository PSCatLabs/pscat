//! `sin`, `cos` and `atan` over a batch, for `.kernelmath /fast` (#1100).
//!
//! The exact path calls `fmath` once a lane: libm's (musl's, FreeBSD
//! msun's) `sin`, `cos` and `atan2`, with their branches. These are the same
//! algorithms written lane by lane without branches: every path a lane could
//! take is computed and the lane's own one selected, so the loops vectorise.
//! The arithmetic of each path is libm's, in libm's order, so a lane's
//! result is meant to be the exact path's bits; `tests` checks that over
//! millions of inputs. Lanes outside the ranges written here (NaN,
//! infinities, zeros where libm special-cases them, |x| past libm's medium
//! reduction) are left to the exact scalar code ([`Lane`]s back as a mask).
//!
//! The constants are libm's (`k_sin.rs`, `k_cos.rs`, `rem_pio2.rs`,
//! `atan.rs`, `atan2.rs`).

// libm's constants are spelled as libm spells them, to be checked against
// it; each still rounds to the same f64.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use super::exec::{LANES, Lane};

const S1: f64 = -1.66666666666666324348e-01;
const S2: f64 = 8.33333333332248946124e-03;
const S3: f64 = -1.98412698298579493134e-04;
const S4: f64 = 2.75573137070700676789e-06;
const S5: f64 = -2.50507602534068634195e-08;
const S6: f64 = 1.58969099521155010221e-10;

const C1: f64 = 4.16666666666666019037e-02;
const C2: f64 = -1.38888888888741095749e-03;
const C3: f64 = 2.48015872894767294178e-05;
const C4: f64 = -2.75573143513906633035e-07;
const C5: f64 = 2.08757232129817482790e-09;
const C6: f64 = -1.13596475577881948265e-11;

const TO_INT: f64 = 1.5 / f64::EPSILON;
const INV_PIO2: f64 = 6.36619772367581382433e-01;
const PIO2_1: f64 = 1.57079632673412561417e+00;
const PIO2_1T: f64 = 6.07710050650619224932e-11;
const PIO2_2: f64 = 6.07710050630396597660e-11;
const PIO2_2T: f64 = 2.02226624879595063154e-21;
const PIO2_3: f64 = 2.02226624871116645580e-21;
const PIO2_3T: f64 = 8.47842766036889956997e-32;

/// `k_sin(x, y, 1)`.
#[inline(always)]
fn k_sin1(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    x - ((z * (0.5 * y - v * r) - y) - v * S1)
}

/// `k_sin(x, 0, 0)`.
#[inline(always)]
fn k_sin0(x: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    x + v * (S1 + z * r)
}

/// `k_cos(x, y)`.
#[inline(always)]
fn k_cos(x: f64, y: f64) -> f64 {
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

// Everything below is written for the loops to vectorise: 64-bit integers
// only (an f64's lane width), `&` rather than `&&`, and each branch of
// libm's a value computed on every lane and chosen with `sel`.

/// The high word of `x` without its sign.
#[inline(always)]
fn hi(x: f64) -> u64 {
    (x.to_bits() >> 32) & 0x7fff_ffff
}

/// The biased exponent of `x`.
#[inline(always)]
fn exp_bits(x: f64) -> i64 {
    ((x.to_bits() >> 52) & 0x7ff) as i64
}

#[inline(always)]
fn sel(c: bool, a: f64, b: f64) -> f64 {
    if c { a } else { b }
}

/// Whether libm's medium reduction of `x` (`ix = hi(x)`) goes past its
/// first round (cancellation near a multiple of pi/2).
#[inline(always)]
fn needs_round2(x: f64, ix: u64) -> bool {
    let f_n = (x * INV_PIO2 + TO_INT) - TO_INT;
    let y01 = (x - f_n * PIO2_1) - f_n * PIO2_1T;
    (ix >> 20) as i64 - exp_bits(y01) > 16
}

/// libm's `sin(x)` (`COS`: `cos(x)`) for |x| below its medium-case bound
/// (`ix = hi(x)` below 0x413921fb). `R23`: some lane of the batch needs
/// the reduction's later rounds; `DIRECT`: some lane is at most pi/4 (with
/// neither, those paths are not computed, and no lane would choose them).
#[inline(always)]
fn sin_cos<const COS: bool, const R23: bool, const DIRECT: bool>(x: f64, ix: u64) -> f64 {
    // rem_pio2's medium case: its three rounds, and the one it stops at.
    let tmp = x * INV_PIO2 + TO_INT;
    let f_n = tmp - TO_INT;
    let r1 = x - f_n * PIO2_1;
    let w1 = f_n * PIO2_1T;
    let y01 = r1 - w1;
    let w2a = f_n * PIO2_2;
    let r2 = r1 - w2a;
    let w2 = f_n * PIO2_2T - ((r1 - r2) - w2a);
    let y02 = r2 - w2;
    let w3a = f_n * PIO2_3;
    let r3 = r2 - w3a;
    let w3 = f_n * PIO2_3T - ((r2 - r3) - w3a);
    let y03 = r3 - w3;
    let ex = (ix >> 20) as i64;
    let round2 = R23 && ex - exp_bits(y01) > 16;
    let round3 = round2 & (ex - exp_bits(y02) > 49);
    let r = sel(round3, r3, sel(round2, r2, r1));
    let w = sel(round3, w3, sel(round2, w2, w1));
    let my0 = sel(round3, y03, sel(round2, y02, y01));
    let my1 = (r - my0) - w;
    // Its small multiples of pi/2 (up to 9pi/4): one round with a fixed
    // n, except right at pi/2, pi, 3pi/2 and 2pi.
    let k = sel(
        ix <= 0x4002_d97c,
        1.0,
        sel(ix <= 0x400f_6a7a, 2.0, sel(ix <= 0x4015_fdbc, 3.0, 4.0)),
    );
    let kf = sel(x.is_sign_negative(), -k, k);
    let z = x - kf * PIO2_1;
    let sy0 = z - kf * PIO2_1T;
    let sy1 = (z - sy0) - kf * PIO2_1T;
    let small = (ix <= 0x401c_463b)
        & !((ix <= 0x400f_6a7a) & ((ix & 0xfffff) == 0x921fb))
        & (ix != 0x4012_d97c)
        & (ix != 0x4019_21fb);
    let y0 = sel(small, sy0, my0);
    let y1 = sel(small, sy1, my1);
    let n = if small { kf as i64 } else { f_n as i64 };
    let s = k_sin1(y0, y1);
    let c = k_cos(y0, y1);
    // sin: s, c, -s, -c by n & 3; cos one quadrant on.
    let q = (n + i64::from(COS)) & 3;
    let v = sel(q & 1 == 0, s, c);
    let reduced = sel(q & 2 == 0, v, -v);
    if !DIRECT {
        return reduced;
    }
    // |x| up to pi/4: no reduction (and below 2^-26 or so, x or 1).
    let direct = if COS {
        sel(ix < 0x3e46_a09e, 1.0, k_cos(x, 0.0))
    } else {
        sel(ix < 0x3e50_0000, x, k_sin0(x))
    };
    sel(ix <= 0x3fe9_21fb, direct, reduced)
}

#[inline(always)]
fn sin_cos_lanes<const COS: bool>(xd: &Lane, t: &mut Lane) -> u64 {
    let mut cold = [0u64; LANES];
    let (mut x, mut ix) = ([0.0; LANES], [0u64; LANES]);
    let (mut r23, mut direct) = ([0u64; LANES], [0u64; LANES]);
    for l in 0..LANES {
        let r = xd[l].to_radians();
        let ix0 = hi(r);
        let ok = ix0 < 0x4139_21fb;
        cold[l] = u64::from(!ok);
        // A cold lane computes something harmless and is redone.
        x[l] = sel(ok, r, 0.0);
        ix[l] = if ok { ix0 } else { 0 };
        r23[l] = u64::from(needs_round2(x[l], ix[l]));
        direct[l] = u64::from(ix[l] <= 0x3fe9_21fb);
    }
    let any = |f: &[u64; LANES]| f.iter().fold(0, |a, b| a | b) != 0;
    macro_rules! pass {
        ($r23:literal, $direct:literal) => {
            for l in 0..LANES {
                t[l] = sin_cos::<COS, $r23, $direct>(x[l], ix[l]);
            }
        };
    }
    match (any(&r23), any(&direct)) {
        (false, false) => pass!(false, false),
        (false, true) => pass!(false, true),
        (true, false) => pass!(true, false),
        (true, true) => pass!(true, true),
    }
    mask(&cold)
}

/// `sin` (`cos`) of degrees over the batch, as `lane_un`: `fmath::sin` of
/// `to_radians`. Returns the lanes left for the exact code: NaN, the
/// infinities and |x| past about 9.4e7 degrees.
// Out of line: inside `Machine::block` the lane arrays are split into
// scalars, and these loops vectorise only in a function of their own
// (#1101).
#[inline(never)]
pub(crate) fn sin_cos_deg(x: &Lane, t: &mut Lane, cos: bool) -> u64 {
    if cos {
        sin_cos_lanes::<true>(x, t)
    } else {
        sin_cos_lanes::<false>(x, t)
    }
}

const ATANHI: [f64; 4] = [
    4.63647609000806093515e-01,
    7.85398163397448278999e-01,
    9.82793723247329054082e-01,
    1.57079632679489655800e+00,
];
const ATANLO: [f64; 4] = [
    2.26987774529616870924e-17,
    3.06161699786838301793e-17,
    1.39033110312309984516e-17,
    6.12323399573676603587e-17,
];
const AT: [f64; 11] = [
    3.33333333333329318027e-01,
    -1.99999999998764832476e-01,
    1.42857142725034663711e-01,
    -1.11111104054623557880e-01,
    9.09088713343650656196e-02,
    -7.69187620504482999495e-02,
    6.66107313738753120669e-02,
    -5.83357013379057348645e-02,
    4.97687799461593236017e-02,
    -3.65315727442169155270e-02,
    1.62858201153657823623e-02,
];
const PI: f64 = 3.1415926535897931160E+00;
const PI_LO: f64 = 1.2246467991473531772E-16;

/// libm's `atan(x)` for a non-negative, finite `x` below 2^66 (all
/// [`atan2`] asks of it).
#[inline(always)]
fn atan_pos(x: f64) -> f64 {
    let ix = hi(x);
    // libm's intervals: below 7/16 no reduction; then 11/16, 19/16,
    // 39/16 and above, each reduced by a quotient.
    let c0 = ix < 0x3fdc_0000;
    let c1 = ix < 0x3fe6_0000;
    let c2 = ix < 0x3ff3_0000;
    let c3 = ix < 0x4003_8000;
    let num = sel(c1, 2. * x - 1., sel(c2, x - 1., sel(c3, x - 1.5, -1.)));
    let den = sel(c1, 2. + x, sel(c2, x + 1., sel(c3, 1. + 1.5 * x, x)));
    let xr = sel(c0, x, num / den);
    let z = xr * xr;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    let ahi = sel(
        c1,
        ATANHI[0],
        sel(c2, ATANHI[1], sel(c3, ATANHI[2], ATANHI[3])),
    );
    let alo = sel(
        c1,
        ATANLO[0],
        sel(c2, ATANLO[1], sel(c3, ATANLO[2], ATANLO[3])),
    );
    let poly = xr - xr * (s1 + s2);
    let table = ahi - ((xr * (s1 + s2) - alo) - xr);
    // Below 2^-27 libm returns x itself.
    sel(c0, sel(ix < 0x3e40_0000, xr, poly), table)
}

/// libm's `atan2(y, x)` for both finite and non-zero, `x` not 1.0 (the
/// cases it special-cases go to the exact code).
#[inline(always)]
fn atan2(y: f64, x: f64) -> f64 {
    let (hx, hy) = (x.to_bits() >> 32, y.to_bits() >> 32);
    // 2 * sign(x) + sign(y)
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    let (ix, iy) = (hx & 0x7fff_ffff, hy & 0x7fff_ffff);
    // |y/x| past 2^64, and (x < 0) below 2^-64. (No u32 wraps here: both
    // are finite.)
    let big = ix + (64 << 20) < iy;
    let tiny = (m & 2 != 0) & (iy + (64 << 20) < ix);
    let q = (y / x).abs();
    // A quotient past 2^66 only on a `big` lane, whose result is fixed.
    let z = sel(tiny, 0.0, atan_pos(sel(big, 0.0, q)));
    let neg_y = m & 1 != 0;
    let pos = sel(neg_y, -z, z);
    let negx = sel(neg_y, (z - PI_LO) - PI, PI - (z - PI_LO));
    let r = sel(m & 2 != 0, negx, pos);
    sel(big, sel(neg_y, -PI / 2.0, PI / 2.0), r)
}

#[inline(always)]
fn atan_lanes(num: &Lane, den: &Lane, t: &mut Lane) -> u64 {
    let mut cold = [0u64; LANES];
    for l in 0..LANES {
        let (y, x) = (num[l], den[l]);
        let ok =
            (hi(x) < 0x7ff0_0000) & (hi(y) < 0x7ff0_0000) & (x != 0.0) & (y != 0.0) & (x != 1.0);
        cold[l] = u64::from(!ok);
        let a = atan2(sel(ok, y, 1.0), sel(ok, x, 2.0));
        let angle = a.to_degrees();
        t[l] = sel(angle < 0.0, angle + 360.0, angle);
    }
    mask(&cold)
}

/// PostScript's `num den atan` over the batch, as `lane_bin`: degrees in
/// [0, 360). Returns the lanes left for the exact code: a NaN, an
/// infinity, a zero (`0 0 atan` is its error), and a `den` of 1.0.
#[inline(never)]
pub(crate) fn atan_deg(num: &Lane, den: &Lane, t: &mut Lane) -> u64 {
    atan_lanes(num, den, t)
}

/// Lane flags (0 or 1) as a mask.
#[inline(always)]
fn mask(f: &[u64; LANES]) -> u64 {
    f.iter().enumerate().fold(0, |m, (l, &b)| m | b << l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmath;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        /// A double spread over many magnitudes, either sign.
        fn wide(&mut self, lo: i32, hi: i32) -> f64 {
            let e = lo + (self.next() % (hi - lo + 1) as u64) as i32;
            let m = 1.0 + (self.next() >> 11) as f64 / (1u64 << 53) as f64;
            let s = if self.next() & 1 == 0 { 1.0 } else { -1.0 };
            s * m * pow2(e)
        }
    }

    /// 2^e, for a normal e.
    fn pow2(e: i32) -> f64 {
        f64::from_bits(((e + 1023) as u64) << 52)
    }

    /// Compares every lane the vector code did (not left cold); returns
    /// how many.
    fn check_trig(xs: &[f64]) -> usize {
        let mut n = 0;
        for ch in xs.chunks(LANES) {
            let mut x = [0.0; LANES];
            x[..ch.len()].copy_from_slice(ch);
            for cos in [false, true] {
                let mut t = [0.0; LANES];
                let cold = sin_cos_deg(&x, &mut t, cos);
                for l in 0..ch.len() {
                    if cold >> l & 1 != 0 {
                        continue;
                    }
                    n += 1;
                    let r = x[l].to_radians();
                    let want = if cos { fmath::cos(r) } else { fmath::sin(r) };
                    assert_eq!(
                        t[l].to_bits(),
                        want.to_bits(),
                        "{} of {} degrees ({:e} rad)",
                        if cos { "cos" } else { "sin" },
                        x[l],
                        r
                    );
                }
            }
        }
        n
    }

    fn check_atan(pairs: &[(f64, f64)]) -> usize {
        let mut n = 0;
        for ch in pairs.chunks(LANES) {
            let (mut y, mut x) = ([0.0; LANES], [0.0; LANES]);
            for (l, &(a, b)) in ch.iter().enumerate() {
                y[l] = a;
                x[l] = b;
            }
            let mut t = [0.0; LANES];
            let cold = atan_deg(&y, &x, &mut t);
            for l in 0..ch.len() {
                if cold >> l & 1 != 0 {
                    continue;
                }
                n += 1;
                let mut want = fmath::atan2(y[l], x[l]).to_degrees();
                if want < 0.0 {
                    want += 360.0;
                }
                assert_eq!(t[l].to_bits(), want.to_bits(), "{} {} atan", y[l], x[l]);
            }
        }
        n
    }

    /// Each libm range boundary, and the multiples of 45 and 90 degrees.
    fn trig_edges() -> Vec<f64> {
        let mut v = Vec::new();
        for ix in [
            0x3e46_a09e_u32,
            0x3e50_0000,
            0x3fe9_21fb,
            0x4002_d97c,
            0x400f_6a7a,
            0x4012_d97c,
            0x4015_fdbc,
            0x4019_21fb,
            0x401c_463b,
            0x4139_21fb,
        ] {
            for lo in [0u32, 1, 0x5444_2d18, 0xffff_ffff] {
                for d in [-1i64, 0, 1] {
                    let b = ((u64::from(ix) << 32) | u64::from(lo)) as i64 + d;
                    let r = f64::from_bits(b as u64);
                    // As degrees whose radians land there, and as is.
                    v.push(r.to_degrees());
                    v.push(r);
                    v.push(-r);
                }
            }
        }
        for k in -2000..2000 {
            v.push(f64::from(k) * 45.0);
            v.push(f64::from(k) * 30.0);
            v.push(f64::from(k) * 90.0 + 1e-9);
        }
        v.extend([0.0, -0.0, 1e-300, 5e-324, f64::NAN, f64::INFINITY, 1e30]);
        v
    }

    #[test]
    fn sin_and_cos_are_the_exact_paths_bits() {
        let edges = trig_edges();
        // (Each is compared twice, sin and cos.)
        assert!(check_trig(&edges) * 10 >= edges.len() * 2 * 8);
        let mut r = Rng(0x1100);
        let n = if cfg!(debug_assertions) {
            200_000
        } else {
            4_000_000
        };
        let xs: Vec<f64> = (0..n).map(|_| r.wide(-40, 26)).collect();
        // (Past about 9.4e7 degrees a lane is libm's large case, cold.)
        assert!(check_trig(&xs) >= 2 * n * 95 / 100);
        // Degrees as programs make them.
        let xs: Vec<f64> = (0..n)
            .map(|_| (r.next() % 2_000_000) as f64 / 1000.0 - 1000.0)
            .collect();
        assert_eq!(check_trig(&xs), 2 * n);
    }

    #[test]
    fn atan_is_the_exact_paths_bits() {
        let mut r = Rng(0x2200);
        let n = if cfg!(debug_assertions) {
            200_000
        } else {
            4_000_000
        };
        let mut pairs: Vec<(f64, f64)> =
            (0..n).map(|_| (r.wide(-80, 80), r.wide(-80, 80))).collect();
        for &a in &[
            0.0, -0.0, 1.0, -1.0, 0.4375, 0.6875, 1.1875, 2.4375, 1e-30, 1e30,
        ] {
            for &b in &[0.0, -0.0, 1.0, -1.0, 2.0, -3.0, 1e-30, 1e30, f64::INFINITY] {
                pairs.push((a, b));
                pairs.push((b, a));
            }
        }
        // Around atan's interval bounds, and |y/x| near 2^64 and 2^-64.
        for ix in [
            0x3e40_0000_u64,
            0x3fdc_0000,
            0x3fe6_0000,
            0x3ff3_0000,
            0x4003_8000,
        ] {
            for d in [-1i64, 0, 1] {
                let q = f64::from_bits(((ix << 32) as i64 + d) as u64);
                pairs.push((q, 1.5));
                pairs.push((q * 3.0, 3.0));
                pairs.push((-q, -7.0));
            }
        }
        for e in [60, 63, 64, 65, 66, 67] {
            let p = pow2(e);
            pairs.extend([(p, 1.5), (1.5, p), (-1.5, -p), (p * 1.7, -0.9)]);
        }
        // Nearly all are the vector code's (the special ones are few).
        assert!(check_atan(&pairs) >= n);
    }

    /// ns a lane, vector against scalar: `cargo test --release --lib
    /// vmath_speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vmath_speed() {
        let mut r = Rng(9);
        let xs: Vec<Lane> = (0..4096)
            .map(|_| std::array::from_fn(|_| (r.next() % 720_000) as f64 / 1000.0 - 360.0))
            .collect();
        let reps = 200;
        let lanes = (xs.len() * LANES * reps) as f64;
        let mut sink = 0.0;
        let t = std::time::Instant::now();
        for _ in 0..reps {
            for x in &xs {
                let mut o = [0.0; LANES];
                sin_cos_deg(x, &mut o, false);
                sink += o[3];
            }
        }
        let v = t.elapsed().as_nanos() as f64 / lanes;
        let t = std::time::Instant::now();
        for _ in 0..reps {
            for x in &xs {
                for &a in x {
                    sink += fmath::sin(a.to_radians());
                }
            }
        }
        let sc = t.elapsed().as_nanos() as f64 / lanes;
        let t = std::time::Instant::now();
        for _ in 0..reps {
            for (k, x) in xs.iter().enumerate() {
                let mut o = [0.0; LANES];
                atan_deg(x, &xs[(k + 1) % xs.len()], &mut o);
                sink += o[3];
            }
        }
        let va = t.elapsed().as_nanos() as f64 / lanes;
        let t = std::time::Instant::now();
        for _ in 0..reps {
            for (k, x) in xs.iter().enumerate() {
                for (a, b) in x.iter().zip(&xs[(k + 1) % xs.len()]) {
                    sink += fmath::atan2(*a, *b).to_degrees();
                }
            }
        }
        let sa = t.elapsed().as_nanos() as f64 / lanes;
        println!(
            "sin: vector {v:.2} ns, scalar {sc:.2} ns; atan: vector {va:.2} ns, scalar {sa:.2} ns ({sink})"
        );
    }
}
