//! Shading dictionaries for the `shfill` operator (issue #20) — the engine;
//! `ops/shading.rs` is the operand-stack adapter.
//!
//! `ShadingType` 1 (function-based) and `FunctionType` 0 (sampled) came
//! with #1039 (`FunctionShading`, `Sampled`; a sampled function reads its
//! string's bytes and runs no PostScript, so the paragraph below still
//! holds); `notes/1039-function-based-shading.md`. Otherwise:
//!
//! Scoped to `ShadingType` 2 (axial) and 3 (radial), `ColorSpace`
//! `/DeviceGray`/`/DeviceRGB`/`/DeviceCMYK`, and `FunctionType` 2
//! (exponential interpolation) and 3 (stitching — what makes
//! multi-stop gradients possible). That combination is deliberate, not
//! just the easy 20%: Types 2 and 3 are pure arithmetic over dict
//! contents, so evaluating them never needs to run a PostScript
//! procedure through the machine — unlike `FunctionType` 0 (sampled)
//! or 4 (PostScript calculator), or `Separation`'s tint transform,
//! which all need the `Frame::PostOp` continuation pattern
//! (`ops/color.rs`) because they *do* execute program-supplied code.
//! Staying inside 2/3 keeps this fully synchronous. Documented gaps:
//! `FunctionType` 4 and Indexed/Separation as a shading's `ColorSpace`.
//!
//! A shading's color ramp is pre-sampled into a fixed list of stops
//! (`Shading::stops`) rather than evaluated per-pixel: `Gfx::shfill` hands
//! them straight to a tiny-skia gradient shader, which already does
//! its own linear interpolation between stops. `sample_positions`
//! samples in *gradient-position* space (0..1 along the shading's own
//! Coords/radii), mapping each position into the function's input `t`
//! via the *shading's* own `/Domain` (`t = t0 + pos*(t1-t0)`) — not
//! the function's `/Domain`, which only clips `t` once it gets there;
//! those are different axes and conflating them was a real bug caught
//! by an `advisor` review of this file (docs/archive/NOTES.md's entry has the
//! story). When every leg is exactly linear (`N == 1`, one level of
//! stitching at most — what `gradfn` in `lib/artkit.ps` always emits
//! — *and* nothing downstream of `eval` could turn that linear-in-t
//! output into a non-linear-in-t color, i.e. no `/Range` and a
//! `ColorSpace` that reads the function's output straight through
//! (`DeviceGray`/`DeviceRGB`, not `DeviceCMYK`'s multiplicative
//! `(1-c)(1-k)` conversion) — tiny-skia's own stop-to-stop
//! interpolation is enough to render the whole ramp *exactly* from
//! just the leg boundaries (`interior_bound_positions`) and the
//! points where the shading's swept `t`-range crosses the function's
//! own Domain edge (`clamp_corner_positions` — `eval` clamps there,
//! so the color goes flat beyond it, and both sides of that corner
//! land on the same color) — no dense sampling needed. Anything else
//! falls back to `SAMPLE_COUNT` uniform samples plus those same exact
//! positions folded in. Every one of these gates — the Range/CMYK
//! nonlinearity, nested-stitching blindness, a reversed `/Range`
//! panicking the same way a reversed `/Domain` did, unbounded
//! `/Functions` recursion, and a function's own `/Domain` wrongly
//! defaulting instead of being required — came from a second Codex
//! review of the first round's fix, not the first round itself
//! (docs/archive/NOTES.md's entry has the full story).

use crate::error::PsError;
use crate::object::{Dict, Value};

/// Total samples across the shading's whole [0,1] gradient-position
/// axis, for a function that isn't exactly piecewise-linear.
const SAMPLE_COUNT: usize = 64;

#[derive(Clone)]
pub(crate) enum PsFunction {
    /// `FunctionType` 2: `C0 + x^N * (C1 - C0)`, x clamped to `domain`.
    Exponential {
        domain: (f64, f64),
        c0: Vec<f64>,
        c1: Vec<f64>,
        n: f64,
    },
    /// `FunctionType` 3: `functions[i]` covers the subdomain between
    /// `bounds[i-1]` and `bounds[i]` (domain edges at the ends),
    /// remapped through `encode[i]` before being evaluated.
    Stitching {
        domain: (f64, f64),
        functions: Vec<PsFunction>,
        bounds: Vec<f64>,
        encode: Vec<(f64, f64)>,
    },
    /// `FunctionType` 0 (#1039): a table of samples, interpolated.
    Sampled(Box<Sampled>),
    /// A shading's `/Function` given as an array of one-output functions,
    /// one per colour component (#1039), which gs accepts for every
    /// shading type. Only at the top level, as the PLRM puts it.
    Components(Vec<PsFunction>),
}

/// A `FunctionType` 0 dictionary (#1039, PLRM 3.10.2): `m` inputs, `n`
/// outputs, samples read lazily from the `/DataSource` string's bytes.
#[derive(Clone)]
pub(crate) struct Sampled {
    domain: Vec<(f64, f64)>,
    range: Vec<(f64, f64)>,
    size: Vec<usize>,
    bits: u32,
    encode: Vec<(f64, f64)>,
    decode: Vec<(f64, f64)>,
    /// Shared: every leg that names the same `/DataSource` string in one
    /// shading holds the same bytes (`SAMPLE_DATA`).
    data: std::rc::Rc<[u8]>,
}

thread_local! {
    /// The `/DataSource` bytes already read during the shading being
    /// parsed, by string identity, so a function that references one
    /// sampled function many times (a stitching function's legs) holds one
    /// copy, not one per reference (Codex round 2 on #1322: 128 legs over
    /// one 1 MiB string took 134 MiB past `--max-memory 8`). `Some` only
    /// while `parse_shading_function` runs: a string's identity is only
    /// stable while it is alive.
    static SAMPLE_DATA: std::cell::RefCell<Option<SampleCache>> =
        const { std::cell::RefCell::new(None) };
}

/// `SAMPLE_DATA`'s map: a string's identity to its bytes.
type SampleCache = std::collections::HashMap<(usize, usize, usize), std::rc::Rc<[u8]>>;

/// Clears `SAMPLE_DATA` when the shading parse that set it ends, however
/// it ends.
struct SampleDataScope;

impl SampleDataScope {
    fn enter() -> Self {
        SAMPLE_DATA.with(|c| *c.borrow_mut() = Some(Default::default()));
        SampleDataScope
    }
}

impl Drop for SampleDataScope {
    fn drop(&mut self) {
        SAMPLE_DATA.with(|c| *c.borrow_mut() = None);
    }
}

/// `s`'s bytes, shared with an earlier reference to the same string in
/// the shading being parsed.
fn sample_bytes(s: &crate::object::PsString) -> std::rc::Rc<[u8]> {
    SAMPLE_DATA.with(|c| match c.borrow_mut().as_mut() {
        Some(seen) => seen
            .entry(s.identity())
            .or_insert_with(|| std::rc::Rc::from(&*s.borrow_bytes()))
            .clone(),
        None => std::rc::Rc::from(&*s.borrow_bytes()),
    })
}

impl Sampled {
    /// Evaluates at `x` (one value per input) into `out` (one per output):
    /// each input clamped to its Domain and mapped through Encode onto the
    /// sample grid, the samples at the surrounding nodes interpolated
    /// multilinearly, then mapped through Decode and clamped to Range.
    /// `/Order 3` is interpolated linearly too (a documented gap: gs's
    /// cubic differs, notes/1039). A sample past the end of the data reads
    /// as 0, as in gs. `x.len()` is the input count, checked by every
    /// caller (`nin`), so the 2^m corners here are at most 4.
    fn eval_into(&self, x: &[f64], out: &mut [f64]) {
        let mut u = [0.0; 2];
        for (i, ui) in u.iter_mut().enumerate().take(self.size.len().min(2)) {
            let (d0, d1) = self.domain[i];
            let (e0, e1) = self.encode[i];
            let xi = x.get(i).copied().unwrap_or(0.0).clamp(d0, d1);
            *ui = if d1 > d0 {
                e0 + (xi - d0) * (e1 - e0) / (d1 - d0)
            } else {
                e0
            };
        }
        self.eval_index(&u, out);
    }

    /// [`eval_into`](Self::eval_into) at a position of the sample grid
    /// itself (`u[i]` in `0..Size[i]-1`, clamped there), past Domain and
    /// Encode: what the PDF copy of a Type 1 shading samples at the
    /// table's own nodes (Codex round 1 on #1322).
    fn eval_index(&self, u: &[f64; 2], out: &mut [f64]) {
        let m = self.size.len();
        let n = self.range.len();
        // Lower node and fraction per input; strides in samples.
        let mut lo = [0u64; 2];
        let mut frac = [0f64; 2];
        let mut stride = [0u64; 2];
        let mut s = 1u64;
        for i in 0..m.min(2) {
            let top = (self.size[i] - 1) as f64;
            let e = u[i].clamp(0.0, top);
            let fl = e.floor().min((top - 1.0).max(0.0));
            lo[i] = fl as u64;
            frac[i] = e - fl;
            stride[i] = s;
            s = s.saturating_mul(self.size[i] as u64);
        }
        let max = ((1u64 << self.bits) - 1) as f64;
        for (j, o) in out.iter_mut().enumerate().take(n) {
            let mut acc = 0.0;
            for corner in 0..(1usize << m.min(2)) {
                let mut w = 1.0;
                let mut index = 0u64;
                for i in 0..m.min(2) {
                    let up = corner >> i & 1 == 1;
                    w *= if up { frac[i] } else { 1.0 - frac[i] };
                    let node = lo[i] + u64::from(up);
                    index = index.saturating_add(node.saturating_mul(stride[i]));
                }
                if w == 0.0 {
                    continue;
                }
                acc += w * self.sample(index, j, n) as f64;
            }
            let (d0, d1) = self.decode[j];
            let (r0, r1) = self.range[j];
            *o = (d0 + acc * (d1 - d0) / max).clamp(r0, r1);
        }
    }

    /// Output `j` of sample `index`: `bits` bits, big-endian, from the
    /// data's bit stream; bits past its end read as 0.
    fn sample(&self, index: u64, j: usize, n: usize) -> u64 {
        let bits = u64::from(self.bits);
        let start = index
            .saturating_mul(n as u64)
            .saturating_add(j as u64)
            .saturating_mul(bits);
        if start / 8 >= self.data.len() as u64 {
            // Past the data (a short string reads as 0, as in gs); also
            // keeps the bit arithmetic below off a saturated `start`.
            return 0;
        }
        let byte_at = |i: u64| {
            usize::try_from(i)
                .ok()
                .and_then(|i| self.data.get(i))
                .map_or(0, |&b| u64::from(b))
        };
        if bits % 8 == 0 {
            // Whole bytes (8, 16, 24, 32): start is byte-aligned.
            (0..bits / 8).fold(0, |v, k| v << 8 | byte_at(start / 8 + k))
        } else if bits < 8 {
            // 1, 2 or 4 bits never straddle a byte.
            byte_at(start / 8) >> (8 - bits - start % 8) & ((1 << bits) - 1)
        } else {
            // 12 bits: a bit at a time.
            (0..bits).fold(0, |v, b| {
                let at = start.saturating_add(b);
                v << 1 | (byte_at(at / 8) >> (7 - at % 8) & 1)
            })
        }
    }
}

impl PsFunction {
    pub(crate) fn ncomp(&self) -> usize {
        match self {
            PsFunction::Exponential { c0, .. } => c0.len(),
            PsFunction::Stitching { functions, .. } => {
                functions.first().map(PsFunction::ncomp).unwrap_or(0)
            }
            PsFunction::Sampled(s) => s.range.len(),
            PsFunction::Components(parts) => parts.len(),
        }
    }

    /// The number of inputs: 1 for types 2 and 3, `m` for a sampled one.
    fn nin(&self) -> usize {
        match self {
            PsFunction::Exponential { .. } | PsFunction::Stitching { .. } => 1,
            PsFunction::Sampled(s) => s.size.len(),
            PsFunction::Components(parts) => parts.first().map_or(0, PsFunction::nin),
        }
    }

    /// A one-input function's `/Domain` (#1325: what a mesh shading's
    /// colour ramp is sampled over); an array of functions spans theirs.
    pub(crate) fn domain1(&self) -> (f64, f64) {
        match self {
            PsFunction::Exponential { domain, .. } | PsFunction::Stitching { domain, .. } => {
                (domain.0.min(domain.1), domain.0.max(domain.1))
            }
            PsFunction::Sampled(s) => s.domain[0],
            PsFunction::Components(parts) => parts
                .iter()
                .map(PsFunction::domain1)
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                .unwrap_or((0.0, 1.0)),
        }
    }

    /// A two-input function at `(x, y)` into `out` (#1039), for a Type 1
    /// shading: no allocation, as it runs once a device pixel. Only called
    /// on a function whose `nin` is 2.
    fn eval2(&self, x: f64, y: f64, out: &mut [f64; 4]) {
        match self {
            PsFunction::Sampled(s) => s.eval_into(&[x, y], out),
            PsFunction::Components(parts) => {
                for (o, part) in out.iter_mut().zip(parts) {
                    let mut one = [0.0; 4];
                    part.eval2(x, y, &mut one);
                    *o = one[0];
                }
            }
            PsFunction::Exponential { .. } | PsFunction::Stitching { .. } => {}
        }
    }

    fn eval(&self, x: f64) -> Vec<f64> {
        match self {
            PsFunction::Sampled(s) => {
                let mut out = vec![0.0; s.range.len()];
                s.eval_into(&[x], &mut out);
                out
            }
            PsFunction::Components(parts) => parts
                .iter()
                .map(|p| p.eval(x).first().copied().unwrap_or(0.0))
                .collect(),
            PsFunction::Exponential { domain, c0, c1, n } => {
                let x = x.clamp(domain.0.min(domain.1), domain.0.max(domain.1));
                let xn = crate::fmath::powf(x, *n);
                c0.iter().zip(c1).map(|(a, b)| a + xn * (b - a)).collect()
            }
            PsFunction::Stitching {
                domain,
                functions,
                bounds,
                encode,
            } => {
                // parse_function already rejects a reversed function
                // Domain, but min/max here (matching the Exponential
                // arm above) keeps eval() itself panic-safe rather
                // than leaning on that validation staying correct.
                let x = x.clamp(domain.0.min(domain.1), domain.0.max(domain.1));
                let (idx, lo, hi) = stitch_index(x, *domain, bounds);
                let (e0, e1) = encode[idx];
                let xe = if hi > lo {
                    e0 + (x - lo) * (e1 - e0) / (hi - lo)
                } else {
                    e0
                };
                functions[idx].eval(xe)
            }
        }
    }

    /// True when every leg evaluates to an exactly linear ramp (`N` ==
    /// 1 everywhere it appears) — tiny-skia and SVG both interpolate
    /// linearly between stops, so a function shaped like this needs no
    /// more than a stop at each leg boundary to render *exactly*, not
    /// approximately. `gradfn` (`lib/artkit.ps`) always emits `N == 1`
    /// legs, so every artkit-built gradient takes this path.
    ///
    /// Only *one* level of stitching qualifies — a leg that's itself a
    /// `Stitching` (nested Type 3) always disqualifies the whole
    /// function, even if every one of *its* legs is linear too. A
    /// nested leg's own interior bounds live in a coordinate space
    /// `interior_bound_positions` doesn't reach (only this function's
    /// own top-level `bounds`), so classifying that case "exact" would
    /// silently drop the nested leg's own hard edges instead of just
    /// sampling them less precisely — caught by a Codex review, not
    /// hand-testing (`gradfn` never nests, so nothing here exercised
    /// it). Falling back to the dense uniform grid for a nested case
    /// is imprecise but not wrong, unlike the fast path would be.
    fn is_piecewise_linear(&self) -> bool {
        match self {
            PsFunction::Exponential { n, .. } => (*n - 1.0).abs() < 1e-12,
            PsFunction::Stitching { functions, .. } => functions.iter().all(
                |f| matches!(f, PsFunction::Exponential { n, .. } if (*n - 1.0).abs() < 1e-12),
            ),
            PsFunction::Sampled(_) | PsFunction::Components(_) => false,
        }
    }

    /// This function's own top-level stitching bounds, mapped from
    /// t-space into gradient-position space and kept only when they
    /// fall within `shading_domain` (inclusive of its edges — see
    /// below for why that matters). Each bound contributes *two*
    /// positions straddling it by a small t-space epsilon (`b - eps`,
    /// `b + eps`) rather than one exactly on it: a stitching function
    /// may be genuinely discontinuous at a bound (e.g. a constant-red
    /// leg followed by a constant-blue leg) — `stitch_index`'s `x < b`
    /// test always resolves a sample taken exactly at `b` to the
    /// *right* leg, so a single stop there would smear the entire
    /// *left* leg's segment into a false gradient instead of the hard
    /// edge the dict actually specifies (caught by a Codex review —
    /// every hand-tested example here happens to have continuous legs,
    /// since `gradfn` always builds them that way, so nothing
    /// exercised the discontinuous case before). `eps` is small enough
    /// that a *continuous* bound (`gradfn`'s own output) still reads
    /// as one exact stop for practical purposes.
    ///
    /// Inclusive bounds matter for a bound that coincides with the
    /// function's own Domain edge (e.g. `/Domain [0 1] /Bounds [1]` —
    /// accepted by gs, and a real way to give the *last* leg zero
    /// width) — a second Codex review pass caught this file's own
    /// strict `>`/`<` silently smearing that case into a full-width
    /// gradient too, the same failure shape as the un-straddled bound
    /// this fix's first pass already covered. The `b + eps` half is
    /// redundant there (`eval`'s own domain clamp collapses it right
    /// back to `b`, duplicating a base 0.0/1.0 sample harmlessly), but
    /// `b - eps` isn't: it's what still gets the leg *before* the edge
    /// its true, uncollapsed value.
    fn interior_bound_positions(&self, shading_domain: (f64, f64)) -> Vec<f64> {
        // An array of functions (#1039): every component's own bounds
        // (Codex round 1 on #1322: only a lone stitching dict kept its
        // hard edges).
        if let PsFunction::Components(parts) = self {
            return parts
                .iter()
                .flat_map(|p| p.interior_bound_positions(shading_domain))
                .collect();
        }
        let PsFunction::Stitching { bounds, .. } = self else {
            return Vec::new();
        };
        let (d0, d1) = shading_domain;
        let span = d1 - d0;
        // Exactly zero, not a small-epsilon threshold: /Domain units
        // are arbitrary (confirmed against gs a genuinely tiny but
        // nonzero span, e.g. [0 1e-13], still sweeps both legs across
        // the axis, not one constant color) -- an epsilon here would
        // silently collapse valid, if extreme, shading content.
        if span == 0.0 {
            return Vec::new();
        }
        let (fd0, fd1) = self.own_domain();
        // No absolute floor: an earlier version clamped this to at
        // least 1e-7, which is *larger than the entire domain* for a
        // genuinely tiny one (e.g. /Domain [0 1e-13], confirmed valid
        // against gs) -- both straddle points would clamp to the same
        // domain edge, collapsing the straddle back to a single point.
        // Pure proportionality keeps this a small but always-in-range
        // fraction of whatever the function's own domain span is,
        // however small.
        let eps_t = (fd1 - fd0).abs() * 1e-7;
        let (lo, hi) = (d0.min(d1), d0.max(d1));
        let mut out = Vec::new();
        for &b in bounds {
            if b >= lo && b <= hi {
                out.push((((b - eps_t) - d0) / span).clamp(0.0, 1.0));
                out.push((((b + eps_t) - d0) / span).clamp(0.0, 1.0));
            }
        }
        out
    }

    fn own_domain(&self) -> (f64, f64) {
        match self {
            PsFunction::Exponential { domain, .. } | PsFunction::Stitching { domain, .. } => {
                *domain
            }
            PsFunction::Sampled(s) => s.domain.first().copied().unwrap_or((0.0, 1.0)),
            PsFunction::Components(parts) => parts.first().map_or((0.0, 1.0), Self::own_domain),
        }
    }

    /// Positions where the swept t-range crosses this function's own
    /// Domain edge — `eval` clamps `t` there, so the true color-vs-
    /// position curve has a corner (and goes flat beyond it) whenever
    /// the shading's own `/Domain` sweeps past the function's own. Both
    /// sides of a corner clamp to the *same* color (`eval` at the
    /// function's domain edge), so folding the corner itself in as a
    /// stop is enough to render the flat region exactly with no
    /// further samples — not just an approximation of it.
    fn clamp_corner_positions(&self, shading_domain: (f64, f64)) -> Vec<f64> {
        if let PsFunction::Components(parts) = self {
            return parts
                .iter()
                .flat_map(|p| p.clamp_corner_positions(shading_domain))
                .collect();
        }
        let (d0, d1) = shading_domain;
        let span = d1 - d0;
        // Exactly zero, not a small-epsilon threshold: /Domain units
        // are arbitrary (confirmed against gs a genuinely tiny but
        // nonzero span, e.g. [0 1e-13], still sweeps both legs across
        // the axis, not one constant color) -- an epsilon here would
        // silently collapse valid, if extreme, shading content.
        if span == 0.0 {
            return Vec::new();
        }
        let (fd0, fd1) = self.own_domain();
        let (lo, hi) = (d0.min(d1), d0.max(d1));
        [fd0, fd1]
            .into_iter()
            .filter(|&fd| fd > lo && fd < hi)
            .map(|fd| ((fd - d0) / span).clamp(0.0, 1.0))
            .collect()
    }

    /// Gradient positions (0..1, geometric — 0 at the shading's first
    /// Coords point/circle, 1 at its second) worth sampling when this
    /// function is swept across `shading_domain`. Exact leg boundaries
    /// (plus the function's own domain-clamp corners, if the shading
    /// sweeps past them) for a piecewise-linear function *and* `exact_ok`
    /// — false whenever anything downstream of `eval` (Range clamping,
    /// or a colorspace conversion that isn't itself linear in the
    /// function's output, like DeviceCMYK's `(1-c)(1-k)`) could still
    /// turn a linear-in-t function into a non-linear-in-t color curve
    /// (`build_stops` computes it; caught by a Codex review pointing
    /// out DeviceCMYK specifically — `(1-c)(1-k)` from C0=[0,0,0,0] to
    /// C1=[1,0,0,1] is `(1-t)²`, not `1-t`, so the 2-stop fast path
    /// would have rendered it visibly wrong). A uniform grid plus
    /// those same positions otherwise.
    fn sample_positions(&self, shading_domain: (f64, f64), exact_ok: bool) -> Vec<f64> {
        let mut out = if exact_ok && self.is_piecewise_linear() {
            vec![0.0, 1.0]
        } else {
            linspace(0.0, 1.0, SAMPLE_COUNT)
        };
        out.extend(self.interior_bound_positions(shading_domain));
        out.extend(self.clamp_corner_positions(shading_domain));
        // total_cmp rather than partial_cmp: parse_shading_dict/
        // parse_function already reject non-finite Domain/Bounds
        // values, but a comparator that can't panic keeps that
        // invariant from being load-bearing here too.
        out.sort_by(f64::total_cmp);
        // Tight enough to only merge genuinely-identical positions
        // (e.g. an endpoint that coincides with a clamp corner), not
        // interior_bound_positions' intentionally-close straddle pairs
        // — those need to survive as two distinct stops.
        out.dedup_by(|a, b| (*a - *b).abs() < 1e-13);
        out
    }
}

fn linspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![lo];
    }
    (0..n)
        .map(|i| lo + (hi - lo) * (i as f64 / (n - 1) as f64))
        .collect()
}

/// Which leg of a stitching function covers `x`, plus that leg's own
/// subdomain bounds — `bounds` is assumed already validated (in range,
/// non-decreasing) by `parse_function`.
fn stitch_index(x: f64, domain: (f64, f64), bounds: &[f64]) -> (usize, f64, f64) {
    let mut lo = domain.0;
    for (i, &b) in bounds.iter().enumerate() {
        if x < b {
            return (i, lo, b);
        }
        lo = b;
    }
    (bounds.len(), lo, domain.1)
}

#[derive(Clone, Copy)]
pub(crate) enum CsKind {
    Gray,
    Rgb,
    Cmyk,
}

impl CsKind {
    pub(crate) fn ncomp(self) -> usize {
        match self {
            CsKind::Gray => 1,
            CsKind::Rgb => 3,
            CsKind::Cmyk => 4,
        }
    }

    /// Components clamp to `[0,1]` *before* conversion, not just the
    /// final RGB after — confirmed against gs's own `setcmykcolor`
    /// (`-1 0 0 0.5 setcmykcolor` reads back as `0.5 0.5 0.5`, i.e. C
    /// clamped to 0 first, not the naive `(1-(-1))*(1-0.5) = 1.0`
    /// clamping only the *product*). Matters for CMYK specifically:
    /// the conversion is multiplicative, so an out-of-range component
    /// (reachable from an Exponential function's C0/C1 or an N that
    /// extrapolates past them) changes the *other* channels' results
    /// too, not just its own.
    pub(crate) fn to_rgb(self, c: &[f64]) -> (f64, f64, f64) {
        let clamp = |v: f64| v.clamp(0.0, 1.0);
        match self {
            CsKind::Gray => (clamp(c[0]), clamp(c[0]), clamp(c[0])),
            CsKind::Rgb => (clamp(c[0]), clamp(c[1]), clamp(c[2])),
            CsKind::Cmyk => {
                let (cy, m, y, k) = (clamp(c[0]), clamp(c[1]), clamp(c[2]), clamp(c[3]));
                (
                    (1.0 - cy) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ShadingKind {
    Axial {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    Radial {
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
    },
}

/// A parsed, ready-to-paint shading: geometry plus a pre-sampled color
/// ramp (`(position 0..1, r, g, b)`, all already resolved to RGB).
#[derive(Clone)]
pub(crate) struct Shading {
    pub(crate) kind: ShadingKind,
    pub(crate) stops: Vec<(f32, f32, f32, f32)>,
    /// `/Extend [start end]` (#1040): whether the shading paints past its
    /// first and its last point or circle. A missing key is `[false
    /// false]`, the PLRM default, as in gs.
    pub(crate) extend: [bool; 2],
    /// Optional `/BBox [llx lly urx ury]`, in the same user space as
    /// `Coords` — confirmed against gs it further clips the painted
    /// region (`Gfx::shfill` builds its paint path from this instead
    /// of the whole page when present).
    pub(crate) bbox: Option<(f64, f64, f64, f64)>,
}

/// The most stops the raster's gradient is given (#1188). tiny-skia's
/// gradient finds each pixel's stop by a walk over all of them, so painting
/// costs pixels x stops: a stitching function of 4095 legs (8190 stops)
/// took 5 s in one `shfill` at 300 dpi. Past this many the ramp is sampled
/// at this many evenly spaced positions instead, which bounds a `shfill` at
/// what an ordinary 512-leg ramp costs. Real ramps have a handful of stops
/// (at most 10 in the gallery and examples; a smooth function is sampled at
/// about 64 plus two a leg), so only a hostile or extreme one is resampled.
const MAX_RASTER_STOPS: usize = 1024;

impl Shading {
    /// The stops the raster paints with: [`Shading::stops`], or past
    /// [`MAX_RASTER_STOPS`] the same piecewise-linear ramp sampled at that
    /// many even positions. A leg narrower than the spacing (1/1023 of the
    /// axis, a few device pixels across a page) is blended into its
    /// neighbours there. The SVG and PDF exports keep every stop.
    pub(crate) fn raster_stops(&self) -> std::borrow::Cow<'_, [(f32, f32, f32, f32)]> {
        if self.stops.len() <= MAX_RASTER_STOPS {
            return std::borrow::Cow::Borrowed(&self.stops);
        }
        std::borrow::Cow::Owned(resample_stops(&self.stops, MAX_RASTER_STOPS))
    }
}

/// `stops` (ascending positions) sampled at `n` (2 or more) even positions
/// from 0 to 1, read as tiny-skia reads them: below the first stop its
/// colour, past the last the last's, and at a position several stops share
/// the last of them.
fn resample_stops(stops: &[(f32, f32, f32, f32)], n: usize) -> Vec<(f32, f32, f32, f32)> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    for k in 0..n {
        let t = k as f32 / (n - 1) as f32;
        while i + 1 < stops.len() && stops[i + 1].0 <= t {
            i += 1;
        }
        let a = stops[i];
        let c = match stops.get(i + 1) {
            Some(&b) if t > a.0 && b.0 > a.0 => {
                let f = (t - a.0) / (b.0 - a.0);
                let mix = |x: f32, y: f32| x + (y - x) * f;
                (t, mix(a.1, b.1), mix(a.2, b.2), mix(a.3, b.3))
            }
            _ => (t, a.1, a.2, a.3),
        };
        out.push(c);
    }
    out
}

fn get(d: &Dict, key: &str) -> Result<crate::object::Object, PsError> {
    d.get(key).ok_or(PsError::Rangecheck)
}

/// Non-finite (`inf`/`NaN`, reachable from a real literal like `1e400`
/// — the lexer's `str::parse::<f64>` doesn't reject those) would carry
/// through into the domain-mapping arithmetic (`d0 + pos * span`) and
/// the sort in `sample_positions`, so every numeric read here rejects
/// it up front rather than trusting a `.is_finite()`-free arithmetic
/// chain downstream to stay well-behaved.
fn finite(n: f64) -> Result<f64, PsError> {
    if n.is_finite() {
        Ok(n)
    } else {
        Err(PsError::Rangecheck)
    }
}

fn get_num(d: &Dict, key: &str) -> Result<f64, PsError> {
    match get(d, key)?.value {
        Value::Integer(n) => Ok(n as f64),
        Value::Real(r) => finite(r),
        _ => Err(PsError::Typecheck),
    }
}

fn get_int(d: &Dict, key: &str) -> Result<i64, PsError> {
    match get(d, key)?.value {
        Value::Integer(n) => Ok(n),
        _ => Err(PsError::Typecheck),
    }
}

pub(crate) fn get_farray(d: &Dict, key: &str) -> Result<Vec<f64>, PsError> {
    let Value::Array(a) = get(d, key)?.value else {
        return Err(PsError::Typecheck);
    };
    (0..a.len())
        .map(|i| match a.get(i).map(|o| o.value) {
            Some(Value::Integer(n)) => Ok(n as f64),
            Some(Value::Real(r)) => finite(r),
            _ => Err(PsError::Typecheck),
        })
        .collect()
}

fn get_domain(d: &Dict) -> Result<(f64, f64), PsError> {
    match d.get("Domain") {
        None => Ok((0.0, 1.0)),
        Some(_) => {
            let dm = get_farray(d, "Domain")?;
            match dm.as_slice() {
                [lo, hi] => Ok((*lo, *hi)),
                _ => Err(PsError::Rangecheck),
            }
        }
    }
}

/// Caps `/Functions` nesting (a Type 3 stitching function's legs can
/// themselves be Type 3). Without a cap, a few thousand levels of
/// acyclic nesting — or a self-referential dict built via `put` after
/// construction — overflows the Rust stack instead of raising a
/// catchable error; found by a Codex review, not hand-testing (every
/// example here nests one level deep at most).
const MAX_FUNCTION_DEPTH: usize = 32;

/// Caps the function dicts one shading parses, counting each time a
/// shared one is reached again (#1176). Depth alone does not bound the
/// work: thirty levels of a stitching function whose `/Functions` names the
/// level below twice is 2^30 dicts from thirty in VM, all parsed in one
/// `shfill`, and `build_stops` then evaluates the result at every leg's
/// bounds. Real stitching functions have tens of legs.
const MAX_FUNCTION_NODES: usize = 4096;

fn parse_function(obj: &crate::object::Object) -> Result<PsFunction, PsError> {
    let mut nodes = 0;
    parse_function_at_depth(obj, 0, &mut nodes)
}

fn parse_function_at_depth(
    obj: &crate::object::Object,
    depth: usize,
    nodes: &mut usize,
) -> Result<PsFunction, PsError> {
    *nodes += 1;
    if depth > MAX_FUNCTION_DEPTH || *nodes > MAX_FUNCTION_NODES {
        return Err(PsError::Limitcheck);
    }
    let Value::Dict(dr) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let d = dr.borrow();
    if matches!(
        d.get("FunctionType").map(|o| o.value),
        Some(Value::Integer(0))
    ) {
        return parse_sampled(&d).map(|s| PsFunction::Sampled(Box::new(s)));
    }
    // /Domain is required on a function dict (confirmed against gs:
    // omitting it is a rangecheck) -- unlike a *shading*'s top-level
    // /Domain, which is optional and defaults to [0 1].
    if d.get("Domain").is_none() {
        return Err(PsError::Rangecheck);
    }
    let domain = get_domain(&d)?;
    // A *function's* own Domain must be non-decreasing (confirmed
    // against gs: /Domain [1 0] on a Type 2/3 function dict is a
    // rangecheck) -- unlike a *shading*'s top-level /Domain, which gs
    // accepts reversed as the documented way to flip a gradient's
    // direction, and which this file's own domain-mapping arithmetic
    // (`t = d0 + pos*span`) already handles correctly either way. Also
    // closes a panic: `eval`'s Stitching branch clamps with
    // `x.clamp(domain.0, domain.1)`, which `f64::clamp` panics on if
    // `domain.0 > domain.1` -- reachable with an empty `Bounds` array,
    // which the monotonicity check on `bounds` alone doesn't rule out.
    if domain.0 > domain.1 {
        return Err(PsError::Rangecheck);
    }
    match get_int(&d, "FunctionType")? {
        2 => {
            // C0/C1 default to [0.0]/[1.0] per spec (confirmed against
            // gs: a Type 2 function dict with neither present is
            // accepted); N has *no* default and is required (confirmed
            // the opposite way: gs rejects one with N omitted).
            let c0 = match d.get("C0") {
                Some(_) => get_farray(&d, "C0")?,
                None => vec![0.0],
            };
            let c1 = match d.get("C1") {
                Some(_) => get_farray(&d, "C1")?,
                None => vec![1.0],
            };
            if c0.is_empty() || c0.len() != c1.len() {
                return Err(PsError::Rangecheck);
            }
            // Confirmed against gs: a negative N is a rangecheck. Real
            // reason it matters, not just gs pedantry: eval's `x.powf(n)`
            // at x=0 with n<0 is +inf, which build_stops' finite-component
            // check would already catch downstream -- but only for
            // stops that happen to sample x=0 exactly; N<0 is rejected
            // here instead so that isn't load-bearing for correctness.
            let n = get_num(&d, "N")?;
            if n < 0.0 {
                return Err(PsError::Rangecheck);
            }
            Ok(PsFunction::Exponential { domain, c0, c1, n })
        }
        3 => {
            let Value::Array(fa) = get(&d, "Functions")?.value else {
                return Err(PsError::Typecheck);
            };
            if fa.is_empty() {
                return Err(PsError::Rangecheck);
            }
            let functions: Vec<PsFunction> = (0..fa.len())
                .map(|i| {
                    parse_function_at_depth(&fa.get(i).expect("len checked"), depth + 1, nodes)
                })
                .collect::<Result<_, _>>()?;
            let ncomp = functions[0].ncomp();
            if functions.iter().any(|f| f.ncomp() != ncomp || f.nin() != 1) {
                return Err(PsError::Rangecheck);
            }
            let bounds = get_farray(&d, "Bounds")?;
            if bounds.len() != functions.len() - 1 {
                return Err(PsError::Rangecheck);
            }
            // Bounds must be non-decreasing and inside Domain — a
            // malformed dict here would otherwise reach stitch_index's
            // unguarded functions[idx]/encode[idx] with an index the
            // Functions/Bounds shape check alone can't rule out.
            let mut prev = domain.0;
            for &b in &bounds {
                if b < prev || b > domain.1 {
                    return Err(PsError::Rangecheck);
                }
                prev = b;
            }
            let encode_flat = get_farray(&d, "Encode")?;
            if encode_flat.len() != functions.len() * 2 {
                return Err(PsError::Rangecheck);
            }
            let encode = encode_flat.chunks(2).map(|c| (c[0], c[1])).collect();
            Ok(PsFunction::Stitching {
                domain,
                functions,
                bounds,
                encode,
            })
        }
        _ => Err(PsError::Rangecheck),
    }
}

/// `[a0 b0 a1 b1 ...]` as `count` non-decreasing pairs (rangecheck
/// otherwise, as gs reports a reversed Domain, Range, or a wrong length).
fn get_pairs(d: &Dict, key: &str, count: usize) -> Result<Vec<(f64, f64)>, PsError> {
    let flat = get_farray(d, key)?;
    if flat.len() != count * 2 {
        return Err(PsError::Rangecheck);
    }
    Ok(flat.chunks(2).map(|c| (c[0], c[1])).collect())
}

/// The most inputs or outputs a sampled function may have. No shading
/// reads more than 2 inputs or 4 outputs; this only bounds the parse.
const MAX_SAMPLED_DIMS: usize = 32;

/// The largest sample table a sampled function may describe, in bytes.
const MAX_SAMPLED_TABLE: u64 = 1 << 30;

/// `FunctionType` 0 (#1039), checked as gs checks it (10.07.1): `/Domain`,
/// `/Range`, `/Size`, `/BitsPerSample` and `/DataSource` required; each
/// Domain and Range pair non-decreasing; `/Size` one positive integer per
/// input (gs takes `2.0` too); `/BitsPerSample` 1, 2, 4, 8, 12, 16, 24 or
/// 32; `/Order` 1 or 3; `/Encode` and `/Decode` of the right length. All
/// of those are rangecheck. `/DataSource` must be a string here: gs also
/// reads a file, which pscat does not (rangecheck, a documented gap), and
/// rejects an array as this does. A string shorter than the table is
/// accepted, the missing samples 0, as gs paints them.
fn parse_sampled(d: &Dict) -> Result<Sampled, PsError> {
    let flat = get_farray(d, "Domain")?;
    let m = flat.len() / 2;
    if flat.len() % 2 != 0 || m == 0 || m > MAX_SAMPLED_DIMS {
        return Err(PsError::Rangecheck);
    }
    let domain = get_pairs(d, "Domain", m)?;
    let rflat = get_farray(d, "Range")?;
    let n = rflat.len() / 2;
    if rflat.len() % 2 != 0 || n == 0 || n > MAX_SAMPLED_DIMS {
        return Err(PsError::Rangecheck);
    }
    let range = get_pairs(d, "Range", n)?;
    if domain.iter().chain(&range).any(|&(a, b)| a > b) {
        return Err(PsError::Rangecheck);
    }
    let size: Vec<usize> = get_farray(d, "Size")?
        .into_iter()
        .map(|v| {
            if v >= 1.0 && v.fract() == 0.0 && v <= u32::MAX as f64 {
                Ok(v as usize)
            } else {
                Err(PsError::Rangecheck)
            }
        })
        .collect::<Result<_, _>>()?;
    if size.len() != m {
        return Err(PsError::Rangecheck);
    }
    let bits = get_int(d, "BitsPerSample")?;
    if ![1, 2, 4, 8, 12, 16, 24, 32].contains(&bits) {
        return Err(PsError::Rangecheck);
    }
    // gs allocates the whole table: `/Size [65536 65536]` of RGB bytes is a
    // VMerror there, and a Size whose product overflows a rangecheck. Past
    // MAX_SAMPLED_TABLE bytes this is a limitcheck (pscat has no VMerror).
    let table_bits = size
        .iter()
        .try_fold(1u64, |a, &s| a.checked_mul(s as u64))
        .and_then(|c| c.checked_mul(n as u64))
        .and_then(|c| c.checked_mul(bits as u64))
        .ok_or(PsError::Rangecheck)?;
    if table_bits / 8 > MAX_SAMPLED_TABLE {
        return Err(PsError::Limitcheck);
    }
    if d.get("Order").is_some() && !matches!(get_int(d, "Order")?, 1 | 3) {
        return Err(PsError::Rangecheck);
    }
    let encode = match d.get("Encode") {
        Some(_) => get_pairs(d, "Encode", m)?,
        None => size.iter().map(|&s| (0.0, (s - 1) as f64)).collect(),
    };
    let decode = match d.get("Decode") {
        Some(_) => get_pairs(d, "Decode", n)?,
        None => range.clone(),
    };
    let data = match get(d, "DataSource")?.value {
        Value::String(s) => sample_bytes(&s),
        _ => return Err(PsError::Rangecheck),
    };
    Ok(Sampled {
        domain,
        range,
        size,
        bits: bits as u32,
        encode,
        decode,
        data,
    })
}

fn parse_colorspace(obj: &crate::object::Object) -> Result<CsKind, PsError> {
    let Value::Name(n) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    match &**n {
        "DeviceGray" => Ok(CsKind::Gray),
        "DeviceRGB" => Ok(CsKind::Rgb),
        "DeviceCMYK" => Ok(CsKind::Cmyk),
        _ => Err(PsError::Typecheck),
    }
}

pub(crate) fn build_stops(
    cs: CsKind,
    function: &PsFunction,
    range: Option<&[(f64, f64)]>,
    shading_domain: (f64, f64),
) -> Result<Vec<(f32, f32, f32, f32)>, PsError> {
    let (d0, d1) = shading_domain;
    let span = d1 - d0;
    // DeviceGray/DeviceRGB read the function's output components
    // straight through, so a linear-in-t function stays linear-in-t
    // color; DeviceCMYK's (1-c)(1-k)-shaped conversion doesn't
    // preserve linearity in general. Range clamping can also introduce
    // a knee partway through an otherwise-linear ramp. Either
    // disqualifies the exact 2-stop-per-leg fast path.
    let exact_ok = range.is_none() && matches!(cs, CsKind::Gray | CsKind::Rgb);
    let mut out = Vec::new();
    for pos in function.sample_positions(shading_domain, exact_ok) {
        // pos is a geometric gradient position (0..1); the shading's
        // own /Domain — not the function's — maps that onto the
        // function's input t. (The function's own /Domain, handled
        // inside eval, only clips t once it gets there.)
        let t = if span != 0.0 { d0 + pos * span } else { d0 };
        let mut comps = function.eval(t);
        if comps.len() != cs.ncomp() {
            return Err(PsError::Rangecheck);
        }
        // /Range clips each output component *after* evaluation,
        // independent of /Domain (which clips the input) — optional,
        // applied only at the top-level Function object a shading
        // dict names directly, not recursively per Stitching leg (a
        // documented simplification: real-world Range usage is
        // overwhelmingly on a simple function used directly, not
        // nested inside a stitching function's own /Functions array).
        if let Some(range) = range {
            for (c, &(lo, hi)) in comps.iter_mut().zip(range) {
                *c = c.clamp(lo, hi);
            }
        }
        let (r, g, b) = cs.to_rgb(&comps);
        if !r.is_finite() || !g.is_finite() || !b.is_finite() {
            return Err(PsError::Rangecheck);
        }
        out.push((
            pos.clamp(0.0, 1.0) as f32,
            r.clamp(0.0, 1.0) as f32,
            g.clamp(0.0, 1.0) as f32,
            b.clamp(0.0, 1.0) as f32,
        ));
    }
    Ok(out)
}

/// The top-level Function's own `/Range`, if present: `[min0 max0
/// min1 max1 ...]`, one pair per output component. Optional per spec;
/// clips each evaluated component *after* the fact (`build_stops`),
/// independent of `/Domain` (which clips the *input*).
pub(crate) fn parse_function_range(
    func_obj: &crate::object::Object,
    ncomp: usize,
) -> Result<Option<Vec<(f64, f64)>>, PsError> {
    // An array of one-output functions (#1039): each dictionary's own
    // `/Range` clips its component, validated as for a lone dictionary
    // (Codex round 1 on #1322: they were ignored). A component with none
    // is unclipped.
    if let Value::Array(a) = &func_obj.value {
        let mut out = Vec::with_capacity(a.len());
        let mut any = false;
        for i in 0..a.len() {
            let part = a.get(i).ok_or(PsError::Rangecheck)?;
            match parse_function_range(&part, 1)? {
                Some(r) => {
                    any = true;
                    out.push(r[0]);
                }
                None => out.push((f64::NEG_INFINITY, f64::INFINITY)),
            }
        }
        return Ok(any.then_some(out));
    }
    let Value::Dict(dr) = &func_obj.value else {
        return Ok(None);
    };
    let d = dr.borrow();
    match d.get("Range") {
        None => Ok(None),
        Some(_) => {
            let flat = get_farray(&d, "Range")?;
            if flat.len() != ncomp * 2 {
                return Err(PsError::Rangecheck);
            }
            // Each (min, max) pair must be non-decreasing (confirmed
            // against gs: /Range [1 0] is a rangecheck) -- build_stops
            // feeds these straight into f64::clamp(lo, hi), which
            // panics if lo > hi.
            if flat.chunks(2).any(|c| c[0] > c[1]) {
                return Err(PsError::Rangecheck);
            }
            Ok(Some(flat.chunks(2).map(|c| (c[0], c[1])).collect()))
        }
    }
}

/// `ShadingType`/`ColorSpace`/`Function` are required on a shading
/// dict and report `undefined` when absent (confirmed against gs) —
/// distinct from `Coords` and most function-dict keys, where a
/// missing entry is `rangecheck` via the shared `get` helper. Kept as
/// a separate helper rather than changing `get` itself, since that
/// would flip the (already gs-verified) error for every other caller.
fn get_required(d: &Dict, key: &str) -> Result<crate::object::Object, PsError> {
    d.get(key)
        .ok_or_else(|| PsError::Undefined(key.to_string()))
}

/// A shading's `/Function`: one dictionary, or an array of one-output
/// functions, one per colour component (#1039; gs takes either for every
/// shading type, and an array of the wrong length is a rangecheck). Every
/// one must take `nin` inputs (rangecheck otherwise, as gs reports a
/// one-input function on a Type 1 shading).
pub(crate) fn parse_shading_function(
    func_obj: &crate::object::Object,
    cs: CsKind,
    nin: usize,
) -> Result<PsFunction, PsError> {
    let _scope = SampleDataScope::enter();
    let function = match &func_obj.value {
        Value::Array(a) => {
            if a.len() != cs.ncomp() {
                return Err(PsError::Rangecheck);
            }
            let parts: Vec<PsFunction> = (0..a.len())
                .map(|i| parse_function(&a.get(i).expect("len checked")))
                .collect::<Result<_, _>>()?;
            if parts.iter().any(|f| f.ncomp() != 1) {
                return Err(PsError::Rangecheck);
            }
            PsFunction::Components(parts)
        }
        _ => parse_function(func_obj)?,
    };
    if function.ncomp() != cs.ncomp() || function.nin() != nin {
        return Err(PsError::Rangecheck);
    }
    if let PsFunction::Components(parts) = &function
        && parts.iter().any(|f| f.nin() != nin)
    {
        return Err(PsError::Rangecheck);
    }
    Ok(function)
}

/// What `shfill` paints: a gradient (types 2 and 3), a function-based
/// shading (type 1, #1039) or a mesh (types 4 to 7, #1325).
#[derive(Clone)]
pub(crate) enum ParsedShading {
    Gradient(Shading),
    Function(FunctionShading),
    Mesh(crate::mesh::MeshShading),
}

/// A `ShadingType` 1 shading (#1039): colour is a two-input function of a
/// point `(x, y)` of its own space, which `/Matrix` maps into user space,
/// painted only over the `/Domain` rectangle (and `/BBox`, if given).
#[derive(Clone)]
pub(crate) struct FunctionShading {
    cs: CsKind,
    function: PsFunction,
    range: Option<Vec<(f64, f64)>>,
    /// `[x0 x1 y0 y1]`, each pair non-decreasing.
    pub(crate) domain: [f64; 4],
    /// Shading space to user space, `[a b c d tx ty]`.
    pub(crate) matrix: [f64; 6],
    pub(crate) bbox: Option<(f64, f64, f64, f64)>,
    /// The sampled function behind it, when there is one (every component
    /// with the same Size, Domain and Encode): `pdf_samples` puts the PDF's
    /// nodes on its table's own nodes.
    leaf: Option<Leaf>,
}

/// A sampled function's grid as the PDF copy needs it: per input, its
/// Domain, Encode and Size.
#[derive(Clone, Copy, PartialEq)]
struct Leaf {
    domain: [(f64, f64); 2],
    encode: [(f64, f64); 2],
    size: [usize; 2],
}

/// One axis of the PDF's sampled copy: `nodes` nodes, node `j` at the
/// source table's index `f0 + j / k`, and the PDF `/Encode` pair that maps
/// the shading Domain onto them.
struct PdfAxis {
    f0: f64,
    k: usize,
    nodes: usize,
    encode: (f64, f64),
}

impl FunctionShading {
    /// The colour at `(x, y)` of shading space, clamped to the Domain.
    pub(crate) fn rgb_at(&self, x: f64, y: f64) -> (f32, f32, f32) {
        let [x0, x1, y0, y1] = self.domain;
        let mut out = [0.0; 4];
        self.function
            .eval2(x.clamp(x0, x1), y.clamp(y0, y1), &mut out);
        let comps = &mut out[..self.cs.ncomp()];
        if let Some(range) = &self.range {
            for (c, &(lo, hi)) in comps.iter_mut().zip(range) {
                *c = c.clamp(lo, hi);
            }
        }
        let (r, g, b) = self.cs.to_rgb(comps);
        let fin = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
        (fin(r), fin(g), fin(b))
    }

    /// The PDF copy's grid on axis `a` when a sampled function is behind
    /// the shading: nodes on the table's own nodes (each source interval
    /// split `k` ways), over just the interval the shading Domain reaches
    /// through `/Encode`, so a bilinear table comes back exactly whatever
    /// its Encode (Codex round 1 on #1322: a uniform grid over the Domain
    /// missed the nodes of a nondefault Encode); at most 257 nodes.
    fn pdf_axis(&self, a: usize) -> Option<PdfAxis> {
        let leaf = self.leaf?;
        let (d0, d1) = leaf.domain[a];
        let (e0, e1) = leaf.encode[a];
        let (x0, x1) = (self.domain[2 * a], self.domain[2 * a + 1]);
        let enc = |x: f64| {
            if d1 > d0 {
                e0 + (x - d0) * (e1 - e0) / (d1 - d0)
            } else {
                e0
            }
        };
        let top = (leaf.size[a] - 1) as f64;
        let (u0, u1) = (enc(x0), enc(x1));
        if !(u0.is_finite() && u1.is_finite()) {
            return None;
        }
        let (lo, hi) = (u0.min(u1).clamp(0.0, top), u0.max(u1).clamp(0.0, top));
        let f0 = lo.floor();
        let n = ((hi.ceil() - f0) as usize).max(1);
        let k = 64usize.div_ceil(n).max(1);
        let k = if n * k < 257 { k } else { 1 };
        if n >= 257 {
            return None;
        }
        Some(PdfAxis {
            f0,
            k,
            nodes: n * k + 1,
            encode: ((u0 - f0) * k as f64, (u1 - f0) * k as f64),
        })
    }

    /// The colour at sample-grid position `u` of the sampled function
    /// behind the shading (`pdf_axis`), with the shading's Range and
    /// colour space applied as `rgb_at` does.
    fn rgb_at_index(&self, u: [f64; 2]) -> (f32, f32, f32) {
        let mut out = [0.0; 4];
        match &self.function {
            PsFunction::Sampled(s) => s.eval_index(&u, &mut out),
            PsFunction::Components(parts) => {
                for (o, part) in out.iter_mut().zip(parts) {
                    if let PsFunction::Sampled(s) = part {
                        let mut one = [0.0; 4];
                        s.eval_index(&u, &mut one);
                        *o = one[0];
                    }
                }
            }
            _ => {}
        }
        let comps = &mut out[..self.cs.ncomp()];
        if let Some(range) = &self.range {
            for (c, &(lo, hi)) in comps.iter_mut().zip(range) {
                *c = c.clamp(lo, hi);
            }
        }
        let (r, g, b) = self.cs.to_rgb(comps);
        let fin = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
        (fin(r), fin(g), fin(b))
    }
}

impl FunctionShading {
    /// The sample table of a PDF FunctionType 0 copy of the shading
    /// (#1039): its Size, its `/Encode` (`[0 Size-1]` per input when no
    /// sampled function is behind it) and its RGB colours, 8 bits a
    /// component, the first input varying fastest. On the source table's
    /// nodes when there is one (`pdf_axis`, both axes), else 65 nodes a
    /// side, uniform over the Domain.
    pub(crate) fn pdf_samples(&self) -> ([usize; 2], [(f64, f64); 2], Vec<u8>) {
        let to8 = |(r, g, b): (f32, f32, f32)| {
            [r, g, b].map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
        };
        if let (Some(ax), Some(ay)) = (self.pdf_axis(0), self.pdf_axis(1)) {
            let mut out = Vec::with_capacity(ax.nodes * ay.nodes * 3);
            for j in 0..ay.nodes {
                let v = ay.f0 + j as f64 / ay.k as f64;
                for i in 0..ax.nodes {
                    let u = ax.f0 + i as f64 / ax.k as f64;
                    out.extend(to8(self.rgb_at_index([u, v])));
                }
            }
            return ([ax.nodes, ay.nodes], [ax.encode, ay.encode], out);
        }
        let (gx, gy) = (65, 65);
        let [x0, x1, y0, y1] = self.domain;
        let at = |i: usize, g: usize, a: f64, b: f64| a + (b - a) * i as f64 / (g - 1) as f64;
        let mut out = Vec::with_capacity(gx * gy * 3);
        for j in 0..gy {
            for i in 0..gx {
                out.extend(to8(self.rgb_at(at(i, gx, x0, x1), at(j, gy, y0, y1))));
            }
        }
        let full = |g: usize| (0.0, (g - 1) as f64);
        ([gx, gy], [full(gx), full(gy)], out)
    }
}

/// Type 1 (#1039): `/Domain` (default `[0 1 0 1]`) and `/Matrix` (default
/// identity) as gs 10.07.1 checks them. A Domain of other than four
/// numbers, or with a pair reversed, is a rangecheck; a Matrix of other
/// than six is a rangecheck, a non-number in it a typecheck, and a
/// singular one `undefinedresult`. `/Coords` and `/Extend` mean nothing
/// here and are not read, and `/Background` is ignored by `shfill`.
fn parse_function_shading(d: &Dict, cs: CsKind) -> Result<FunctionShading, PsError> {
    let func_obj = get_required(d, "Function")?;
    let function = parse_shading_function(&func_obj, cs, 2)?;
    let range = parse_function_range(&func_obj, function.ncomp())?;
    let domain = match d.get("Domain") {
        None => [0.0, 1.0, 0.0, 1.0],
        Some(_) => match get_farray(d, "Domain")?.as_slice() {
            &[x0, x1, y0, y1] if x0 <= x1 && y0 <= y1 => [x0, x1, y0, y1],
            _ => return Err(PsError::Rangecheck),
        },
    };
    let matrix = match d.get("Matrix") {
        None => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        Some(_) => {
            let m: [f64; 6] = get_farray(d, "Matrix")?
                .try_into()
                .map_err(|_| PsError::Rangecheck)?;
            if m[0] * m[3] - m[1] * m[2] == 0.0 {
                return Err(PsError::UndefinedResult);
            }
            m
        }
    };
    // gs 10.07.1: a shading Domain not inside the function's own is a
    // rangecheck ("Additional information: [/Domain [0 1 0 1]]").
    let inside = |f: &PsFunction| match f {
        PsFunction::Sampled(s) => {
            let ((a0, a1), (b0, b1)) = (s.domain[0], s.domain[1]);
            domain[0] >= a0 && domain[1] <= a1 && domain[2] >= b0 && domain[3] <= b1
        }
        _ => true,
    };
    let all_inside = match &function {
        PsFunction::Components(parts) => parts.iter().all(inside),
        f => inside(f),
    };
    if !all_inside {
        return Err(PsError::Rangecheck);
    }
    // gs reads the Domain as f32, so a pair that differs only below f32
    // precision is a zero-width one there, and paints nothing (#1333). It
    // is made exactly zero-width here, so the raster, the SVG and the PDF
    // agree with it (the PDF would otherwise write an empty Domain).
    let collapse = |lo: f64, hi: f64| if lo as f32 == hi as f32 { lo } else { hi };
    let domain = [
        domain[0],
        collapse(domain[0], domain[1]),
        domain[2],
        collapse(domain[2], domain[3]),
    ];
    let leaf_of = |f: &PsFunction| match f {
        PsFunction::Sampled(s) => Some(Leaf {
            domain: [s.domain[0], s.domain[1]],
            encode: [s.encode[0], s.encode[1]],
            size: [s.size[0], s.size[1]],
        }),
        _ => None,
    };
    let leaf = match &function {
        PsFunction::Components(parts) => {
            let first = parts.first().and_then(leaf_of);
            parts
                .iter()
                .all(|p| leaf_of(p) == first)
                .then_some(first)
                .flatten()
        }
        f => leaf_of(f),
    };
    Ok(FunctionShading {
        cs,
        function,
        range,
        domain,
        matrix,
        bbox: parse_bbox(d)?,
        leaf,
    })
}

/// Optional `/BBox [llx lly urx ury]`, in the shading's user space.
pub(crate) fn parse_bbox(d: &Dict) -> Result<Option<(f64, f64, f64, f64)>, PsError> {
    match d.get("BBox") {
        None => Ok(None),
        Some(_) => match get_farray(d, "BBox")?.as_slice() {
            [llx, lly, urx, ury] => Ok(Some((*llx, *lly, *urx, *ury))),
            _ => Err(PsError::Rangecheck),
        },
    }
}

/// A shading's `/Background`, as RGB (#1298). `shfill` ignores it; a
/// shading pattern paints it wherever the shading itself does not reach
/// (inside the `/BBox`). As gs 10.07.1 reads it: a value that is not an
/// array is ignored, an array of the wrong length is `rangecheck`, a
/// non-number in it `typecheck`, and a component out of range is clamped.
pub(crate) fn parse_background(d: &Dict) -> Result<Option<(f32, f32, f32)>, PsError> {
    let cs = parse_colorspace(&get_required(d, "ColorSpace")?)?;
    match d.get("Background") {
        Some(o) if matches!(o.value, Value::Array(_)) => {}
        _ => return Ok(None),
    }
    let c = get_farray(d, "Background")?;
    if c.len() != cs.ncomp() {
        return Err(PsError::Rangecheck);
    }
    let (r, g, b) = cs.to_rgb(&c);
    Ok(Some((r as f32, g as f32, b as f32)))
}

pub(crate) fn parse_shading_dict(d: &Dict) -> Result<ParsedShading, PsError> {
    let shading_type = match get_required(d, "ShadingType")?.value {
        Value::Integer(n) => n,
        _ => return Err(PsError::Typecheck),
    };
    let cs = parse_colorspace(&get_required(d, "ColorSpace")?)?;
    if shading_type == 1 {
        return parse_function_shading(d, cs).map(ParsedShading::Function);
    }
    if (4..=7).contains(&shading_type) {
        return crate::mesh::parse(d, shading_type as u8, cs).map(ParsedShading::Mesh);
    }
    let coords = get_farray(d, "Coords")?;
    let func_obj = get_required(d, "Function")?;
    let function = parse_shading_function(&func_obj, cs, 1)?;
    let range = parse_function_range(&func_obj, function.ncomp())?;
    let shading_domain = match d.get("Domain") {
        None => (0.0, 1.0),
        Some(_) => get_domain(d)?,
    };
    // /Extend, if present, must have the PLRM shape. Length and
    // element-type mismatches get different errors (confirmed against
    // gs: a wrong-length array is a rangecheck, not a typecheck).
    let mut extend = [false, false];
    if let Some(ext) = d.get("Extend") {
        let Value::Array(a) = ext.value else {
            return Err(PsError::Typecheck);
        };
        if a.len() != 2 {
            return Err(PsError::Rangecheck);
        }
        for (i, e) in extend.iter_mut().enumerate() {
            match a.get(i).map(|o| o.value) {
                Some(Value::Boolean(b)) => *e = b,
                _ => return Err(PsError::Typecheck),
            }
        }
    }
    let kind = match shading_type {
        2 => match coords.as_slice() {
            [x0, y0, x1, y1] => ShadingKind::Axial {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
            },
            _ => return Err(PsError::Rangecheck),
        },
        3 => match coords.as_slice() {
            [x0, y0, r0, x1, y1, r1] => {
                if *r0 < 0.0 || *r1 < 0.0 {
                    return Err(PsError::Rangecheck);
                }
                ShadingKind::Radial {
                    x0: *x0,
                    y0: *y0,
                    r0: *r0,
                    x1: *x1,
                    y1: *y1,
                    r1: *r1,
                }
            }
            _ => return Err(PsError::Rangecheck),
        },
        _ => return Err(PsError::Rangecheck),
    };
    let bbox = parse_bbox(d)?;
    let stops = build_stops(cs, &function, range.as_deref(), shading_domain)?;
    Ok(ParsedShading::Gradient(Shading {
        kind,
        stops,
        extend,
        bbox,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legs that name one `/DataSource` string share its bytes, and the
    /// cache is gone once the shading is parsed (Codex round 2 on #1322:
    /// each reference copied the table, past the memory limit).
    #[test]
    fn legs_naming_one_data_source_share_its_bytes() {
        let mut it = crate::Interp::new();
        it.run_str(
            "/S << /FunctionType 0 /Domain [0 1] /Range [0 1] /Size [2] \
               /BitsPerSample 8 /DataSource <00ff> >> def \
             << /FunctionType 3 /Domain [0 1] /Bounds [0.5] /Encode [0 1 0 1] \
                /Functions [S S] >>",
        )
        .expect("builds the function");
        let obj = it.pop().expect("the function dict");
        let f = parse_shading_function(&obj, CsKind::Gray, 1).expect("parses");
        let PsFunction::Stitching { functions, .. } = &f else {
            panic!("a stitching function");
        };
        let [PsFunction::Sampled(a), PsFunction::Sampled(b)] = functions.as_slice() else {
            panic!("two sampled legs");
        };
        assert!(
            std::rc::Rc::ptr_eq(&a.data, &b.data),
            "one copy of the bytes"
        );
        assert!(
            SAMPLE_DATA.with(|c| c.borrow().is_none()),
            "the cache is cleared"
        );
    }

    #[test]
    fn exponential_linear_ramp_is_exact_at_endpoints() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0, 0.0, 0.0],
            c1: vec![1.0, 0.5, 0.25],
            n: 1.0,
        };
        assert_eq!(f.eval(0.0), vec![0.0, 0.0, 0.0]);
        assert_eq!(f.eval(1.0), vec![1.0, 0.5, 0.25]);
        assert_eq!(f.eval(0.5), vec![0.5, 0.25, 0.125]);
    }

    #[test]
    fn exponential_clamps_to_domain() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };
        assert_eq!(f.eval(-5.0), vec![0.0]);
        assert_eq!(f.eval(5.0), vec![1.0]);
    }

    #[test]
    fn stitching_picks_the_right_leg_and_remaps() {
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![1.0],
                    c1: vec![0.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.5],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        // First leg covers [0, 0.5) remapped to [0,1]: at x=0.25 -> 0.5.
        assert_eq!(f.eval(0.25), vec![0.5]);
        // Second leg covers [0.5, 1] remapped to [0,1]: at x=0.75 -> 0.5.
        assert_eq!(f.eval(0.75), vec![0.5]);
        // Exact leg boundary lands on the second leg (x < b test).
        assert_eq!(f.eval(0.5), vec![1.0]);
    }

    #[test]
    fn piecewise_linear_stitching_samples_only_exact_breakpoints() {
        // Every leg has N=1, so this whole function is exactly
        // piecewise-linear — sample_positions should need nothing
        // beyond the endpoints and the one interior leg boundary, not
        // a dense uniform grid.
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.3],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        assert!(f.is_piecewise_linear());
        let pts = f.sample_positions((0.0, 1.0), true);
        // Endpoints, plus two straddling the one interior boundary
        // (interior_bound_positions' discontinuity-preserving pair) —
        // not the one-sample-per-boundary a purely continuous
        // function's exactness would only need.
        assert_eq!(pts.len(), 4, "{pts:?}");
        assert!((pts[0] - 0.0).abs() < 1e-9);
        assert!((pts[1] - 0.3).abs() < 1e-6, "{pts:?}");
        assert!((pts[2] - 0.3).abs() < 1e-6, "{pts:?}");
        assert!(pts[1] < pts[2], "straddle pair must stay ordered: {pts:?}");
        assert!((pts[3] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn non_linear_function_samples_a_dense_uniform_grid() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 2.0,
        };
        assert!(!f.is_piecewise_linear());
        assert_eq!(f.sample_positions((0.0, 1.0), true).len(), SAMPLE_COUNT);
    }

    #[test]
    fn interior_bound_maps_through_the_shading_domain_not_the_functions() {
        // The function's own bounds live at t=0.5 (its own [0,1]
        // domain); the *shading*'s domain is [0, 2] here, so that
        // bound should land at gradient position 0.25, not 0.5.
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.5],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        let positions = f.interior_bound_positions((0.0, 2.0));
        assert_eq!(positions.len(), 2, "{positions:?}");
        assert!((positions[0] - 0.25).abs() < 1e-6, "{positions:?}");
        assert!((positions[1] - 0.25).abs() < 1e-6, "{positions:?}");
        assert!(positions[0] < positions[1], "{positions:?}");
    }

    #[test]
    fn discontinuous_stitching_bound_gets_a_hard_edge_not_a_smear() {
        // A constant-red leg followed by a constant-blue leg: eval at
        // the bound itself always resolves to the right leg (blue),
        // per stitch_index's `x < b` test, so a single sample at the
        // bound would have made build_stops smear red into blue across
        // the *entire preceding leg's segment* -- the straddle pair in
        // interior_bound_positions is what keeps that segment solid
        // red right up to the edge.
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![1.0, 0.0, 0.0],
                    c1: vec![1.0, 0.0, 0.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0, 0.0, 1.0],
                    c1: vec![0.0, 0.0, 1.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.5],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        let stops = build_stops(CsKind::Rgb, &f, None, (0.0, 1.0)).expect("valid");
        // Every stop strictly left of the bound must still be pure
        // red; every stop strictly right must be pure blue.
        for &(pos, r, g, b) in &stops {
            if pos < 0.5 - 1e-6 {
                assert_eq!((r, g, b), (1.0, 0.0, 0.0), "left of bound: {stops:?}");
            } else if pos > 0.5 + 1e-6 {
                assert_eq!((r, g, b), (0.0, 0.0, 1.0), "right of bound: {stops:?}");
            }
        }
    }

    #[test]
    fn colorspace_conversion() {
        assert_eq!(CsKind::Gray.to_rgb(&[0.5]), (0.5, 0.5, 0.5));
        assert_eq!(CsKind::Rgb.to_rgb(&[1.0, 0.5, 0.0]), (1.0, 0.5, 0.0));
        let (r, g, b) = CsKind::Cmyk.to_rgb(&[0.0, 0.0, 0.0, 1.0]);
        assert_eq!((r, g, b), (0.0, 0.0, 0.0));
    }

    #[test]
    fn cmyk_never_takes_the_piecewise_linear_fast_path() {
        // (1-c)(1-k) is quadratic in t when c and k both ramp linearly
        // (C0=[0,0,0,0], C1=[1,0,0,1] here), so a 2-stop linear
        // interpolation would render 1-t where the true curve is
        // (1-t)^2 -- confirmed by hand, not against gs (gs's CMYK->RGB
        // conversion isn't the naive product formula this file uses,
        // so it can't validate *this* formula's own self-consistency).
        // build_stops must fall back to the dense grid even though
        // every leg here has N=1 (is_piecewise_linear() is true).
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0, 0.0, 0.0, 0.0],
            c1: vec![1.0, 0.0, 0.0, 1.0],
            n: 1.0,
        };
        assert!(f.is_piecewise_linear());
        let stops = build_stops(CsKind::Cmyk, &f, None, (0.0, 1.0)).expect("valid");
        assert!(
            stops.len() > 10,
            "CMYK must use dense sampling, not the 2-stop fast path: {} stops",
            stops.len()
        );
        // The stop nearest pos=0.5 should show the true quadratic R,
        // not the 1-t a linear interpolation would give.
        let mid = stops
            .iter()
            .min_by(|a, b| (a.0 - 0.5).abs().partial_cmp(&(b.0 - 0.5).abs()).unwrap())
            .unwrap();
        assert!(
            (mid.1 - 0.25).abs() < 0.05,
            "expected R near (1-0.5)^2=0.25 at pos≈0.5, got {mid:?}"
        );
    }

    #[test]
    fn range_disables_the_piecewise_linear_fast_path() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };
        assert!(f.is_piecewise_linear());
        let range = [(0.0, 0.5)];
        let stops = build_stops(CsKind::Gray, &f, Some(&range), (0.0, 1.0)).expect("valid");
        assert!(
            stops.len() > 10,
            "Range must disable the 2-stop fast path: {} stops",
            stops.len()
        );
    }

    #[test]
    fn nested_stitching_disqualifies_the_fast_path() {
        // A one-leg outer Stitching wrapping a child Stitching that
        // itself has a hard red/blue edge in the middle. Even though
        // every N is 1, the *nested* discontinuity lives in a
        // coordinate space interior_bound_positions can't reach, so
        // is_piecewise_linear must be false here -- the 2-endpoint
        // fast path would otherwise silently drop the inner edge.
        let inner = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![1.0, 0.0, 0.0],
                    c1: vec![1.0, 0.0, 0.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0, 0.0, 1.0],
                    c1: vec![0.0, 0.0, 1.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.5],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        let outer = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![inner],
            bounds: vec![],
            encode: vec![(0.0, 1.0)],
        };
        assert!(!outer.is_piecewise_linear());
    }

    #[test]
    fn clamp_corner_lands_where_the_shading_domain_crosses_the_functions_own() {
        // Function's own domain is [0,1]; the shading sweeps t across
        // [-1,3] (twice as wide, off-center) -- the function's domain
        // edges (t=0, t=1) should land at gradient positions 0.25 and
        // 0.5 respectively.
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };
        let corners = f.clamp_corner_positions((-1.0, 3.0));
        assert_eq!(corners.len(), 2, "{corners:?}");
        assert!((corners[0] - 0.25).abs() < 1e-9, "{corners:?}");
        assert!((corners[1] - 0.5).abs() < 1e-9, "{corners:?}");
    }

    #[test]
    fn clamp_corner_makes_the_piecewise_linear_fast_path_exact_past_the_domain_edge() {
        // Same setup, end to end through sample_positions/eval: the
        // segment from the t=1 corner (position 0.5) out to position 1
        // must be *flat* (eval clamps to the same color at both ends),
        // which only holds if the corner itself is an exact stop.
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };
        assert!(f.is_piecewise_linear());
        let positions = f.sample_positions((-1.0, 3.0), true);
        assert!(
            positions.iter().any(|p| (p - 0.5).abs() < 1e-9),
            "{positions:?}"
        );
        // t at position 0.5 is exactly 1.0 (the function's own domain
        // edge); t at position 1.0 is 3.0, clamped to the same 1.0 by
        // eval -- so both ends of that segment must eval identically.
        assert_eq!(f.eval(-1.0 + 0.5 * 4.0), f.eval(-1.0 + 1.0 * 4.0));
    }

    #[test]
    fn non_finite_domain_values_are_rejected_not_left_to_panic() {
        let mut dict = Dict::new();
        dict.put("FunctionType".into(), crate::object::Object::int(2));
        dict.put(
            "Domain".into(),
            crate::object::Object::array(vec![
                crate::object::Object::real(f64::INFINITY),
                crate::object::Object::real(1.0),
            ]),
        );
        dict.put(
            "C0".into(),
            crate::object::Object::array(vec![crate::object::Object::real(0.0)]),
        );
        dict.put(
            "C1".into(),
            crate::object::Object::array(vec![crate::object::Object::real(1.0)]),
        );
        let result = parse_function(&crate::object::Object::lit(Value::Dict(std::rc::Rc::new(
            std::cell::RefCell::new(dict),
        ))));
        assert!(matches!(result, Err(PsError::Rangecheck)));
    }

    /// #1188: a ramp of more than `MAX_RASTER_STOPS` stops reaches the
    /// raster resampled to that many even positions, read as tiny-skia reads
    /// stops; one of at most that many is handed over as it is.
    #[test]
    fn a_ramp_of_very_many_stops_is_resampled_for_the_raster() {
        let ramp = |n: usize| Shading {
            kind: ShadingKind::Axial {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 0.0,
            },
            stops: (0..n)
                .map(|i| {
                    let t = i as f32 / (n - 1) as f32;
                    (t, t, 1.0 - t, 0.5)
                })
                .collect(),
            extend: [true, true],
            bbox: None,
        };
        let small = ramp(MAX_RASTER_STOPS);
        assert!(matches!(
            small.raster_stops(),
            std::borrow::Cow::Borrowed(_)
        ));
        let big = ramp(8190);
        let r = big.raster_stops();
        assert_eq!(r.len(), MAX_RASTER_STOPS);
        for (k, &(t, red, green, blue)) in r.iter().enumerate() {
            assert_eq!(t, k as f32 / (MAX_RASTER_STOPS - 1) as f32);
            // A straight ramp stays the same ramp.
            assert!(
                (red - t).abs() < 1e-5 && (green - (1.0 - t)).abs() < 1e-5,
                "{k}"
            );
            assert_eq!(blue, 0.5);
        }
        // A hard edge at 0.5 (two stops there): the later colour from it on.
        let edge = [
            (0.0, 0.0, 0.0, 0.0),
            (0.5, 0.0, 0.0, 0.0),
            (0.5, 1.0, 1.0, 1.0),
            (1.0, 1.0, 1.0, 1.0),
        ];
        let r = resample_stops(&edge, 5);
        let reds: Vec<f32> = r.iter().map(|s| s.1).collect();
        assert_eq!(reds, [0.0, 0.0, 1.0, 1.0, 1.0]);
        // Before the first stop and past the last: their colours.
        let inner = [(0.25, 0.2, 0.2, 0.2), (0.75, 0.8, 0.8, 0.8)];
        let r = resample_stops(&inner, 5);
        let reds: Vec<f32> = r.iter().map(|s| s.1).collect();
        assert_eq!(reds, [0.2, 0.2, 0.5, 0.8, 0.8]);
    }
}
