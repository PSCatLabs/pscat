//! Native coherent-noise operators (issue #734): gradient (Perlin) and
//! value noise in 2D and 3D, fBm and ridged sums, and 2D gradient noise
//! with analytic derivatives. pscat extensions, not PLRM operators.
//!
//! ```text
//! seed noiseseed -              build the permutation table from an int
//! x y gnoise2 n                 2D gradient noise, about [-0.71, 0.71]
//! x y z gnoise3 n               3D gradient noise, about [-1, 1]
//! x y gnoise2d n dx dy          gnoise2 plus its analytic gradient
//! x y vnoise2 v                 2D value noise, in [-0.996, 0.996]
//! x y z vnoise3 v               3D value noise
//! x y oct gain lac fbm2 v       fBm of gnoise2 (fbm3, vfbm2, vfbm3 alike)
//! x y oct gain lac ridged2 v    ridged sum of gnoise2 (ridged3 alike)
//! x y oct gain lac offset wgain angle ridgedmf2 v
//!                               Musgrave's ridged multifractal (issue #845)
//! x y oct gain lac angle erodedfbm2 v
//!                               derivative-damped ("eroded") fBm (issue #845)
//! x y perm .permnoise2 n        artkit's noise2 off a caller's table
//! ```
//!
//! **Determinism.** Every result is a fixed sequence of IEEE-754 f64
//! adds, subtracts and multiplies (plus `floor` and table lookups), in
//! the order spelled out below and in `lib/noiseref.ps`, the PostScript
//! reference that reproduces each operator bit for bit
//! (`tests/noise.rs` holds the two to that). No libm transcendental, no
//! fused multiply-add: the same bits on every platform and in the WASM
//! build. Two #845 additions stay inside that rule: `erodedfbm2` divides
//! once per octave (IEEE division is correctly rounded, so as exact as a
//! multiply), and the `angle` of `ridgedmf2`/`erodedfbm2` goes through
//! `fmath::cos`/`fmath::sin` of `angle.to_radians()`, exactly what the
//! PostScript `cos`/`sin` operators compute.
//!
//! **Seed.** `noiseseed` fills a private 256-entry permutation by a
//! Fisher-Yates shuffle of 0..255 (`i` from 255 down to 1, swap `P[i]`
//! with `P[j]`) driven by its own LCG, `s = (s * 1103515245 + 12345) & 2^31-1`
//! starting from `s = seed & 2^31-1`, with `j = (s >> 16) mod (i + 1)`
//! after each step. It never reads or advances `rand`'s state. A fresh
//! interpreter behaves as if `0 noiseseed` had run.
//!
//! **Sweeps (issue #753).** `--sweep-seed N` also builds the table from
//! `N`, as `N noiseseed` would, before the program runs, so a seed sweep
//! varies noise art too. A program that calls `noiseseed` keeps its own
//! table, as one that calls `srand` does not (the override replaces that
//! seed): noise seeds are chosen by the art, rand seeds by the sweep.
//! Sampling the table counts as the override firing, so the "had no
//! effect" warning stays quiet for a noise-only piece.
//!
//! **Lattice.** `xi = floor(x)` as an integer (a value out of integer
//! range is a `rangecheck`, as `floor cvi` gives), `xf = x - xi`, and
//! the table index is `xi & 255`, so the noise repeats every 256 units.
//! Hashes: `h2(i, j) = P[(P[i] + j) & 255]` and
//! `h3(i, j, k) = P[(P[(P[i] + j) & 255] + k) & 255]`.
//! The fade is the quintic `f(t) = ((t³·10) − (t³·t)·15) + (t³·t²)·6`
//! with `t² = t·t`, `t³ = t²·t`; `lerp(a, b, t) = (b − a)·t + a`.
//!
//! - gnoise2 is exactly artkit's `noise2` formula: gradient
//!   `G2[(h & 7)]` from `(1,0) (-1,0) (0,1) (0,-1)` and the four
//!   diagonals `(±0.70710678, ±0.70710678)`, dot `gx·x + gy·y`, then
//!   `lerp(lerp(n00, n10, u), lerp(n01, n11, u), v)`.
//! - gnoise3 is Perlin's improved noise: the 16 gradients of his
//!   reference `grad(h & 15)`, dot `(gx·x + gy·y) + gz·z`, lerps in x,
//!   then y, then z.
//! - Value noise's lattice value is `(h − 127.5)·0.0078125`.
//! - fBm: `sum = 0, amp = 1, freq = 1`; octave `k` (from 0) samples at
//!   `(x·freq + k·17.3203125, y·freq + k·11.4140625, z·freq +
//!   k·7.6015625)` (the shift keeps octaves from all vanishing on the
//!   same lattice points), `sum = sum + amp·n`, then `amp = amp·gain`,
//!   `freq = freq·lac`. Not normalized. Ridged uses `n = 1 − |g|`,
//!   `n = n·n` per octave. `oct` is an integer in 0..=32.
//! - gnoise2d's derivatives: see `gnoise2_derivs` below.
//!
//! **Octave walk of `ridgedmf2` and `erodedfbm2` (issue #845).** `oct` is
//! an integer or a real in 0..=32. `full = floor(oct)` octaves count
//! whole; a remainder `r = oct − full > 0` adds one more octave whose term
//! is multiplied by `r`, so detail fades in as `oct` grows (Musgrave's
//! fBm remainder). With `c = cos(angle)`, `s = sin(angle)` (degrees),
//! `lc = lac·c`, `ls = lac·s`: `px = x, py = y`, `sum = 0, amp = 1`, and
//! octave `k` (from 0) samples gnoise2 at `(px + k·17.3203125, py +
//! k·11.4140625)`, then `amp = amp·gain`, `px' = (lc·px) − (ls·py)`,
//! `py' = (ls·px) + (lc·py)`. The rotation keeps the octaves' lattices
//! from lining up (angle 0 gives exactly `lac·px`).
//! - ridgedmf2 (Musgrave, *Texturing & Modeling*): `w = 1` to start;
//!   per octave `sig = offset − |n|`, `sig = sig·sig`, `sig = sig·w`,
//!   term `t = amp·sig` (`t = t·r` for the fractional octave), `sum =
//!   sum + t`, then `w = sig·wgain` clamped to [0, 1] (`if w > 1 then 1`,
//!   then `if w < 0 then 0`). Musgrave's `H` is `gain = lac^−H`; his early
//!   exit at a tiny weight is left out, so the sum is exact.
//! - erodedfbm2 (Quílez's derivative-damped fBm): `dx = dy = 0`; per
//!   octave `(n, gx, gy) = gnoise2d`, `dx = dx + gx`, `dy = dy + gy`,
//!   `t = amp·(n / (1 + ((dx·dx) + (dy·dy))))` (`t = t·r` for the
//!   fractional octave), `sum = sum + t`. The gradients are summed as
//!   sampled, unrotated and unscaled, as in the common formulation.
//!
//! Neither is normalized.
//!
//! `.permnoise2` is artkit's `noise2` kept bit-identical, integer
//! operands and all: it does its arithmetic with PostScript's own
//! int/real promotion rules, so an all-integer lattice point still
//! returns the same `0` (an integer) or `0.0` the procedure did.

use crate::error::PsError;
use crate::interp::Interp;
use crate::object::{Dict, Num, Object, PsArray, Value};

pub fn install(dict: &mut Dict) {
    use super::op;
    op(dict, "noiseseed", noiseseed);
    op(dict, "gnoise2", gnoise2);
    op(dict, "gnoise3", gnoise3);
    op(dict, "gnoise2d", gnoise2d);
    op(dict, "vnoise2", vnoise2);
    op(dict, "vnoise3", vnoise3);
    op(dict, "fbm2", fbm2);
    op(dict, "fbm3", fbm3);
    op(dict, "vfbm2", vfbm2);
    op(dict, "vfbm3", vfbm3);
    op(dict, "ridged2", ridged2);
    op(dict, "ridged3", ridged3);
    op(dict, "ridgedmf2", ridgedmf2);
    op(dict, "erodedfbm2", erodedfbm2);
    op(dict, ".permnoise2", permnoise2);
}

const MASK31: i64 = 0x7fff_ffff;

/// Most octaves an fBm or ridged sum accepts; past ~20 the octaves are
/// below f64's resolution anyway, and the cap bounds a hosted call.
const MAX_OCTAVES: i64 = 32;

/// Per-octave domain shift (exact binary fractions, so the PostScript
/// reference writes the same constants).
const SHIFT: [f64; 3] = [17.3203125, 11.4140625, 7.6015625];

/// The permutation `seed noiseseed` builds (see the module docs).
pub(crate) fn perm_from_seed(seed: i64) -> [u8; 256] {
    let mut p = [0u8; 256];
    for (i, v) in p.iter_mut().enumerate() {
        *v = i as u8;
    }
    let mut s = seed & MASK31;
    for i in (1..256usize).rev() {
        s = (s.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & MASK31;
        let j = ((s >> 16) % (i as i64 + 1)) as usize;
        p.swap(i, j);
    }
    p
}

fn noiseseed(it: &mut Interp) -> Result<(), PsError> {
    let seed = it.pop_int()?;
    it.noise_perm = perm_from_seed(seed);
    // The program's own seed wins over a sweep's (issue #753).
    it.noise_from_override = false;
    Ok(())
}

/// Called by every operator that samples `noise_perm`: sampling the table
/// a seed override built is the override taking effect (issue #753).
fn sampled(it: &mut Interp) {
    if it.noise_from_override {
        it.seed_override_fired = true;
    }
}

// --- PostScript-typed arithmetic, for artkit's exact noise2 ----------

/// `add`/`sub`/`mul` exactly as `ops/arith.rs` does them: integer when
/// both operands are integers and the result fits, real otherwise.
fn n_op(a: Num, b: Num, int_op: fn(i64, i64) -> Option<i64>, real_op: fn(f64, f64) -> f64) -> Num {
    match (a, b) {
        (Num::Int(x), Num::Int(y)) => {
            int_op(x, y).map_or_else(|| Num::Real(real_op(x as f64, y as f64)), Num::Int)
        }
        _ => Num::Real(real_op(a.to_f64(), b.to_f64())),
    }
}
fn nadd(a: Num, b: Num) -> Num {
    n_op(a, b, i64::checked_add, |x, y| x + y)
}
fn nsub(a: Num, b: Num) -> Num {
    n_op(a, b, i64::checked_sub, |x, y| x - y)
}
fn nmul(a: Num, b: Num) -> Num {
    n_op(a, b, i64::checked_mul, |x, y| x * y)
}

/// `floor cvi`.
fn floor_int(x: Num) -> Result<i64, PsError> {
    match x {
        Num::Int(i) => Ok(i),
        Num::Real(r) => {
            let t = r.floor();
            if !t.is_finite() || t < i64::MIN as f64 || t > i64::MAX as f64 {
                return Err(PsError::Rangecheck);
            }
            Ok(t as i64)
        }
    }
}

/// A permutation table: pscat's own, or a PostScript array (artkit's
/// `Perm`), read one entry at a time with `get`'s errors.
trait Table {
    fn at(&self, i: i64) -> Result<i64, PsError>;
}

impl Table for [u8; 256] {
    #[inline]
    fn at(&self, i: i64) -> Result<i64, PsError> {
        Ok(i64::from(self[(i & 255) as usize]))
    }
}

impl Table for PsArray {
    fn at(&self, i: i64) -> Result<i64, PsError> {
        let idx = usize::try_from(i).map_err(|_| PsError::Rangecheck)?;
        match self.peek(idx, |o| o.value.clone()) {
            Some(Value::Integer(v)) => Ok(v),
            Some(_) => Err(PsError::Typecheck),
            None => Err(PsError::Rangecheck),
        }
    }
}

/// `P[(P[i] + j) & 255]` (artkit's `gethash`). A non-integer entry
/// would make `add ... 255 and` a typecheck.
fn hash2<T: Table + ?Sized>(p: &T, i: i64, j: i64) -> Result<i64, PsError> {
    let s = p.at(i)?.checked_add(j).ok_or(PsError::Typecheck)?;
    p.at(s & 255)
}

/// artkit's `Grad2` writes the diagonal as `0.70710678`; the same
/// truncated value, not `FRAC_1_SQRT_2`, keeps `noise2` bit-identical.
#[allow(clippy::approx_constant)]
const D: f64 = 0.707_106_78;
const G2: [Num; 16] = [
    Num::Int(1),
    Num::Int(0),
    Num::Int(-1),
    Num::Int(0),
    Num::Int(0),
    Num::Int(1),
    Num::Int(0),
    Num::Int(-1),
    Num::Real(D),
    Num::Real(D),
    Num::Real(-D),
    Num::Real(D),
    Num::Real(D),
    Num::Real(-D),
    Num::Real(-D),
    Num::Real(-D),
];

fn grad2(h: i64, x: Num, y: Num) -> Num {
    let i = ((h & 7) * 2) as usize;
    nadd(nmul(G2[i], x), nmul(G2[i + 1], y))
}

fn nfade(t: Num) -> Num {
    let t2 = nmul(t, t);
    let t3 = nmul(t2, t);
    let a = nmul(t3, Num::Int(10));
    let b = nmul(nmul(t3, t), Num::Int(15));
    let c = nmul(nmul(t3, t2), Num::Int(6));
    nadd(nsub(a, b), c)
}

fn nlerp(a: Num, b: Num, t: Num) -> Num {
    nadd(nmul(nsub(b, a), t), a)
}

/// The 2D lattice cell of `(x, y)`: table indices and fractional parts.
struct Cell2 {
    xi: i64,
    yi: i64,
    xi1: i64,
    yi1: i64,
    xf: Num,
    yf: Num,
}

fn cell2(x: Num, y: Num) -> Result<Cell2, PsError> {
    let x0 = floor_int(x)?;
    let y0 = floor_int(y)?;
    let xi = x0 & 255;
    let yi = y0 & 255;
    Ok(Cell2 {
        xi,
        yi,
        xi1: (xi + 1) & 255,
        yi1: (yi + 1) & 255,
        xf: nsub(x, Num::Int(x0)),
        yf: nsub(y, Num::Int(y0)),
    })
}

/// The four corner hashes of a 2D cell: 00, 10, 01, 11.
fn corners2<T: Table + ?Sized>(p: &T, c: &Cell2) -> Result<[i64; 4], PsError> {
    Ok([
        hash2(p, c.xi, c.yi)?,
        hash2(p, c.xi1, c.yi)?,
        hash2(p, c.xi, c.yi1)?,
        hash2(p, c.xi1, c.yi1)?,
    ])
}

/// artkit's `noise2`, operation for operation.
fn gradient2<T: Table + ?Sized>(p: &T, x: Num, y: Num) -> Result<Num, PsError> {
    let c = cell2(x, y)?;
    let h = corners2(p, &c)?;
    let one = Num::Int(1);
    let u = nfade(c.xf);
    let v = nfade(c.yf);
    let n00 = grad2(h[0], c.xf, c.yf);
    let n10 = grad2(h[1], nsub(c.xf, one), c.yf);
    let n01 = grad2(h[2], c.xf, nsub(c.yf, one));
    let n11 = grad2(h[3], nsub(c.xf, one), nsub(c.yf, one));
    Ok(nlerp(nlerp(n00, n10, u), nlerp(n01, n11, u), v))
}

// --- plain f64 paths (every public operator takes its operands as reals)

#[inline]
fn fade(t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    ((t3 * 10.0) - ((t3 * t) * 15.0)) + ((t3 * t2) * 6.0)
}

#[inline]
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    ((b - a) * t) + a
}

fn g2(p: &[u8; 256], x: f64, y: f64) -> Result<f64, PsError> {
    Ok(gradient2(p, Num::Real(x), Num::Real(y))?.to_f64())
}

/// 2D gradient noise and its analytic gradient. With `a, b, c, d` the
/// corner dots (00, 10, 01, 11), `g?` their gradients, `u = f(xf)`,
/// `v = f(yf)` and `f'(t) = ((t·t)·30)·((t − 1)·(t − 1))`:
///
/// ```text
/// k1 = b − a    k2 = c − a    k3 = ((a − b) − c) + d
/// dx = (((gax + u·(gbx − gax)) + v·(gcx − gax)) + (u·v)·(((gax − gbx) − gcx) + gdx)) + f'(xf)·(k1 + k3·v)
/// dy = (((gay + u·(gby − gay)) + v·(gcy − gay)) + (u·v)·(((gay − gby) − gcy) + gdy)) + f'(yf)·(k2 + k3·u)
/// ```
/// (each `·` binds tighter than `+`/`−`; parentheses give the order).
/// `n` is gnoise2's own value, bit for bit.
fn gnoise2_derivs(p: &[u8; 256], x: f64, y: f64) -> Result<(f64, f64, f64), PsError> {
    let n = g2(p, x, y)?;
    let c = cell2(Num::Real(x), Num::Real(y))?;
    let h = corners2(p, &c)?;
    let (xf, yf) = (c.xf.to_f64(), c.yf.to_f64());
    let g = |h: i64| {
        let i = ((h & 7) * 2) as usize;
        (G2[i].to_f64(), G2[i + 1].to_f64())
    };
    let (ga, gb, gc, gd) = (g(h[0]), g(h[1]), g(h[2]), g(h[3]));
    let a = (ga.0 * xf) + (ga.1 * yf);
    let b = (gb.0 * (xf - 1.0)) + (gb.1 * yf);
    let cc = (gc.0 * xf) + (gc.1 * (yf - 1.0));
    let d = (gd.0 * (xf - 1.0)) + (gd.1 * (yf - 1.0));
    let u = fade(xf);
    let v = fade(yf);
    let dfade = |t: f64| ((t * t) * 30.0) * ((t - 1.0) * (t - 1.0));
    let k1 = b - a;
    let k2 = cc - a;
    let k3 = ((a - b) - cc) + d;
    let dx = (((ga.0 + (u * (gb.0 - ga.0))) + (v * (gc.0 - ga.0)))
        + ((u * v) * (((ga.0 - gb.0) - gc.0) + gd.0)))
        + (dfade(xf) * (k1 + (k3 * v)));
    let dy = (((ga.1 + (u * (gb.1 - ga.1))) + (v * (gc.1 - ga.1)))
        + ((u * v) * (((ga.1 - gb.1) - gc.1) + gd.1)))
        + (dfade(yf) * (k2 + (k3 * u)));
    Ok((n, dx, dy))
}

/// `gnoise2d` without the `Num` detour, for the octave loop of
/// `erodedfbm2`. `n` is `lerp(lerp(a, b, u), lerp(c, d, u), v)` of the
/// same corner dots, which is gradient2's grouping for real operands (an
/// integer gradient component `g` times a real `x` is `g as f64 * x`
/// there too), so all three values equal `gnoise2_derivs`' bit for bit;
/// `tests/noise.rs` holds erodedfbm2 to `nrgnoise2d`'s values.
#[inline]
fn g2d(p: &[u8; 256], x: f64, y: f64) -> Result<(f64, f64, f64), PsError> {
    let (xi, xf) = lattice(x)?;
    let (yi, yf) = lattice(y)?;
    let (xi1, yi1) = ((xi + 1) & 255, (yi + 1) & 255);
    let g = |i: i64, j: i64| -> Result<(f64, f64), PsError> {
        let k = ((hash2(p, i, j)? & 7) * 2) as usize;
        Ok((G2[k].to_f64(), G2[k + 1].to_f64()))
    };
    let (ga, gb, gc, gd) = (g(xi, yi)?, g(xi1, yi)?, g(xi, yi1)?, g(xi1, yi1)?);
    let (x1, y1) = (xf - 1.0, yf - 1.0);
    let a = (ga.0 * xf) + (ga.1 * yf);
    let b = (gb.0 * x1) + (gb.1 * yf);
    let c = (gc.0 * xf) + (gc.1 * y1);
    let d = (gd.0 * x1) + (gd.1 * y1);
    let u = fade(xf);
    let v = fade(yf);
    let n = lerp(lerp(a, b, u), lerp(c, d, u), v);
    let dfade = |t: f64| ((t * t) * 30.0) * ((t - 1.0) * (t - 1.0));
    let k1 = b - a;
    let k2 = c - a;
    let k3 = ((a - b) - c) + d;
    let dx = (((ga.0 + (u * (gb.0 - ga.0))) + (v * (gc.0 - ga.0)))
        + ((u * v) * (((ga.0 - gb.0) - gc.0) + gd.0)))
        + (dfade(xf) * (k1 + (k3 * v)));
    let dy = (((ga.1 + (u * (gb.1 - ga.1))) + (v * (gc.1 - ga.1)))
        + ((u * v) * (((ga.1 - gb.1) - gc.1) + gd.1)))
        + (dfade(yf) * (k2 + (k3 * u)));
    Ok((n, dx, dy))
}

fn lattice(x: f64) -> Result<(i64, f64), PsError> {
    let x0 = floor_int(Num::Real(x))?;
    Ok((x0 & 255, x - x0 as f64))
}

#[inline]
fn value(h: i64) -> f64 {
    (h as f64 - 127.5) * 0.0078125
}

fn v2(p: &[u8; 256], x: f64, y: f64) -> Result<f64, PsError> {
    let c = cell2(Num::Real(x), Num::Real(y))?;
    let h = corners2(p, &c)?;
    let (u, v) = (fade(c.xf.to_f64()), fade(c.yf.to_f64()));
    Ok(lerp(
        lerp(value(h[0]), value(h[1]), u),
        lerp(value(h[2]), value(h[3]), u),
        v,
    ))
}

/// Perlin's improved-noise gradients, `grad(h & 15)` as vectors.
const G3: [[f64; 3]; 16] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
    [1.0, 1.0, 0.0],
    [0.0, -1.0, 1.0],
    [-1.0, 1.0, 0.0],
    [0.0, -1.0, -1.0],
];

/// Shared 3D cell walk: `corner(h, dx, dy, dz)` gives the corner's
/// contribution for hash `h` at offset (0 or 1 per axis).
fn noise3(
    p: &[u8; 256],
    x: f64,
    y: f64,
    z: f64,
    corner: impl Fn(i64, f64, f64, f64) -> f64,
) -> Result<f64, PsError> {
    let (xi, xf) = lattice(x)?;
    let (yi, yf) = lattice(y)?;
    let (zi, zf) = lattice(z)?;
    let (xi1, yi1, zi1) = ((xi + 1) & 255, (yi + 1) & 255, (zi + 1) & 255);
    let h = |i, j, k| -> Result<i64, PsError> {
        let s = hash2(p, i, j)?;
        p.at((s + k) & 255)
    };
    let (x1, y1, z1) = (xf - 1.0, yf - 1.0, zf - 1.0);
    let n000 = corner(h(xi, yi, zi)?, xf, yf, zf);
    let n100 = corner(h(xi1, yi, zi)?, x1, yf, zf);
    let n010 = corner(h(xi, yi1, zi)?, xf, y1, zf);
    let n110 = corner(h(xi1, yi1, zi)?, x1, y1, zf);
    let n001 = corner(h(xi, yi, zi1)?, xf, yf, z1);
    let n101 = corner(h(xi1, yi, zi1)?, x1, yf, z1);
    let n011 = corner(h(xi, yi1, zi1)?, xf, y1, z1);
    let n111 = corner(h(xi1, yi1, zi1)?, x1, y1, z1);
    let (u, v, w) = (fade(xf), fade(yf), fade(zf));
    let a = lerp(lerp(n000, n100, u), lerp(n010, n110, u), v);
    let b = lerp(lerp(n001, n101, u), lerp(n011, n111, u), v);
    Ok(lerp(a, b, w))
}

fn g3(p: &[u8; 256], x: f64, y: f64, z: f64) -> Result<f64, PsError> {
    noise3(p, x, y, z, |h, x, y, z| {
        let g = G3[(h & 15) as usize];
        ((g[0] * x) + (g[1] * y)) + (g[2] * z)
    })
}

fn v3(p: &[u8; 256], x: f64, y: f64, z: f64) -> Result<f64, PsError> {
    noise3(p, x, y, z, |h, _, _, _| value(h))
}

// --- operators -------------------------------------------------------

fn pop_xy(it: &mut Interp) -> Result<(f64, f64), PsError> {
    let y = it.pop_f64()?;
    let x = it.pop_f64()?;
    Ok((x, y))
}

fn pop_xyz(it: &mut Interp) -> Result<(f64, f64, f64), PsError> {
    let z = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    Ok((x, y, z))
}

fn gnoise2(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let (x, y) = pop_xy(it)?;
    let n = g2(&it.noise_perm, x, y)?;
    it.push(Object::real(n));
    Ok(())
}

fn gnoise3(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let (x, y, z) = pop_xyz(it)?;
    let n = g3(&it.noise_perm, x, y, z)?;
    it.push(Object::real(n));
    Ok(())
}

fn gnoise2d(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let (x, y) = pop_xy(it)?;
    let (n, dx, dy) = gnoise2_derivs(&it.noise_perm, x, y)?;
    it.push(Object::real(n));
    it.push(Object::real(dx));
    it.push(Object::real(dy));
    Ok(())
}

fn vnoise2(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let (x, y) = pop_xy(it)?;
    let n = v2(&it.noise_perm, x, y)?;
    it.push(Object::real(n));
    Ok(())
}

fn vnoise3(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let (x, y, z) = pop_xyz(it)?;
    let n = v3(&it.noise_perm, x, y, z)?;
    it.push(Object::real(n));
    Ok(())
}

/// How an octave's sample enters the sum.
#[derive(Clone, Copy)]
enum Shape {
    Plain,
    Ridged,
}

/// `oct gain lac` off the stack, then the sum over octaves of
/// `sample(coords)` (coords already scaled and shifted).
fn octaves<const N: usize>(
    it: &mut Interp,
    shape: Shape,
    sample: fn(&[u8; 256], [f64; N]) -> Result<f64, PsError>,
) -> Result<(), PsError> {
    sampled(it);
    let lac = it.pop_f64()?;
    let gain = it.pop_f64()?;
    let oct = it.pop_int()?;
    let mut pt = [0.0; N];
    for k in (0..N).rev() {
        pt[k] = it.pop_f64()?;
    }
    if !(0..=MAX_OCTAVES).contains(&oct) {
        return Err(PsError::Rangecheck);
    }
    let (mut sum, mut amp, mut freq) = (0.0, 1.0, 1.0);
    for k in 0..oct {
        let mut q = [0.0; N];
        for (i, c) in q.iter_mut().enumerate() {
            *c = (pt[i] * freq) + (k as f64 * SHIFT[i]);
        }
        let mut n = sample(&it.noise_perm, q)?;
        if let Shape::Ridged = shape {
            n = 1.0 - n.abs();
            n *= n;
        }
        sum += amp * n;
        amp *= gain;
        freq *= lac;
    }
    it.push(Object::real(sum));
    Ok(())
}

fn fbm2(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Plain, |p, [x, y]| g2(p, x, y))
}
fn fbm3(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Plain, |p, [x, y, z]| g3(p, x, y, z))
}
fn vfbm2(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Plain, |p, [x, y]| v2(p, x, y))
}
fn vfbm3(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Plain, |p, [x, y, z]| v3(p, x, y, z))
}
fn ridged2(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Ridged, |p, [x, y]| g2(p, x, y))
}
fn ridged3(it: &mut Interp) -> Result<(), PsError> {
    octaves(it, Shape::Ridged, |p, [x, y, z]| g3(p, x, y, z))
}

/// The octave walk shared by `ridgedmf2` and `erodedfbm2` (module docs):
/// `oct` (int or real, 0..=32), `gain`, `lac` and `angle` give the count,
/// the fractional weight of the last octave, the amplitudes and the
/// sample points.
struct Walk {
    full: i64,
    rem: f64,
    gain: f64,
    lc: f64,
    ls: f64,
}

impl Walk {
    fn new(oct: f64, gain: f64, lac: f64, angle: f64) -> Result<Walk, PsError> {
        // `!(..)` so a NaN count is a rangecheck too.
        if !(0.0..=MAX_OCTAVES as f64).contains(&oct) {
            return Err(PsError::Rangecheck);
        }
        let full = oct.floor();
        let rad = angle.to_radians();
        Ok(Walk {
            full: full as i64,
            rem: oct - full,
            gain,
            lc: lac * crate::fmath::cos(rad),
            ls: lac * crate::fmath::sin(rad),
        })
    }

    /// Sum `term(sample x, sample y)` over the octaves (called in octave
    /// order), each times `amp`, the fractional last one times `rem` too.
    fn sum(
        &self,
        x: f64,
        y: f64,
        mut term: impl FnMut(f64, f64) -> Result<f64, PsError>,
    ) -> Result<f64, PsError> {
        let count = self.full + i64::from(self.rem > 0.0);
        let (mut px, mut py) = (x, y);
        let (mut sum, mut amp) = (0.0, 1.0);
        for k in 0..count {
            let kf = k as f64;
            let mut t = amp * term(px + (kf * SHIFT[0]), py + (kf * SHIFT[1]))?;
            if k == self.full {
                t *= self.rem;
            }
            sum += t;
            amp *= self.gain;
            let nx = (self.lc * px) - (self.ls * py);
            py = (self.ls * px) + (self.lc * py);
            px = nx;
        }
        Ok(sum)
    }
}

/// `x y oct gain lac offset wgain angle ridgedmf2 v`.
fn ridgedmf2(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let angle = it.pop_f64()?;
    let wgain = it.pop_f64()?;
    let offset = it.pop_f64()?;
    let lac = it.pop_f64()?;
    let gain = it.pop_f64()?;
    let oct = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    let walk = Walk::new(oct, gain, lac, angle)?;
    let p = &it.noise_perm;
    let mut w = 1.0;
    let v = walk.sum(x, y, |sx, sy| {
        let mut sig = offset - g2(p, sx, sy)?.abs();
        sig *= sig;
        sig *= w;
        // f64::clamp is the reference's `1.0 gt`/`0.0 lt` pair: the
        // ranges are disjoint, -0.0 stays -0.0, and a NaN passes through.
        w = (sig * wgain).clamp(0.0, 1.0);
        Ok(sig)
    })?;
    it.push(Object::real(v));
    Ok(())
}

/// `x y oct gain lac angle erodedfbm2 v`.
fn erodedfbm2(it: &mut Interp) -> Result<(), PsError> {
    sampled(it);
    let angle = it.pop_f64()?;
    let lac = it.pop_f64()?;
    let gain = it.pop_f64()?;
    let oct = it.pop_f64()?;
    let (x, y) = pop_xy(it)?;
    let walk = Walk::new(oct, gain, lac, angle)?;
    let p = &it.noise_perm;
    let (mut dx, mut dy) = (0.0, 0.0);
    let v = walk.sum(x, y, |sx, sy| {
        let (n, gx, gy) = g2d(p, sx, sy)?;
        dx += gx;
        dy += gy;
        Ok(n / (1.0 + ((dx * dx) + (dy * dy))))
    })?;
    it.push(Object::real(v));
    Ok(())
}

/// `x y perm .permnoise2 n`: artkit's `noise2` against `perm` (its
/// `Perm`), with PostScript's own number typing (module docs).
fn permnoise2(it: &mut Interp) -> Result<(), PsError> {
    let perm = match it.pop()?.value {
        Value::Array(a) => a,
        _ => return Err(PsError::Typecheck),
    };
    let y = it.pop_num()?;
    let x = it.pop_num()?;
    let n = gradient2(&perm, x, y)?;
    it.push(match n {
        Num::Int(i) => Object::int(i),
        Num::Real(r) => Object::real(r),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_tables_are_permutations() {
        for seed in [0, 1, -1, 42, i64::MAX] {
            let mut p = perm_from_seed(seed).to_vec();
            p.sort_unstable();
            assert_eq!(p, (0..=255).collect::<Vec<u8>>(), "seed {seed}");
        }
        assert_ne!(perm_from_seed(1), perm_from_seed(2));
    }

    #[test]
    fn fade_and_derivative_agree() {
        for i in 1..100 {
            let t = i as f64 / 100.0;
            let h = 1e-6;
            let fd = (fade(t + h) - fade(t - h)) / (2.0 * h);
            let an = ((t * t) * 30.0) * ((t - 1.0) * (t - 1.0));
            assert!((fd - an).abs() < 1e-6, "t {t}: {fd} vs {an}");
        }
    }
}
