//! Analytic area-coverage rasteriser for fills (issue #692).
//!
//! tiny-skia 0.12's anti-aliased path fill (`scan::path_aa`) supersamples
//! 4x4: each device pixel is sampled on four sub-scanlines. A filled shape
//! thinner than a quarter pixel can miss every sub-scanline and vanish,
//! and anything between a quarter and half a pixel hits exactly one, so
//! a ramp of thin rules collapses into one or two grays. Strokes don't
//! suffer: tiny-skia draws a sub-pixel stroke as a hairline whose alpha
//! is scaled by the width.
//!
//! This module computes each pixel's coverage as the exact area of the
//! pixel the path covers, the way font rasterisers do (font-rs, stb_truetype
//! v2, FreeType's "smooth" rasteriser). Every edge adds its signed area,
//! winding included, into an accumulation buffer; a running sum along
//! each row then gives each pixel's winding-weighted coverage, and the
//! fill rule maps that to opacity:
//!
//! * nonzero: `min(|v|, 1)`;
//! * even-odd: `v` folded modulo 2 (0 → 0, 1 → 1, 2 → 0, ...).
//!
//! That is the same quantity FreeType's rasteriser applies its fill rule
//! to, so it is exact wherever only one region's edges cross a pixel and
//! the usual approximation where edges of different windings share one.
//!
//! The result is written into a page-sized [`tiny_skia::Mask`] over the
//! path's pixel bounds (times the clip mask, if any). The caller
//! composites it with `fill_rect`, so colour, alpha and blend modes keep
//! going through tiny-skia's pipeline exactly as before.

use tiny_skia::{Mask, Path, PathSegment};

/// Flattening tolerance for curves, in device pixels.
const TOLERANCE: f32 = 0.05;
/// Upper bound on chords per curve, so a huge curve can't stall a fill.
const MAX_CHORDS: usize = 1024;
/// Accumulation floats per band: bounds the scratch buffer for a
/// page-sized fill to about a megabyte, whatever the page size.
const BAND_FLOATS: usize = 1 << 18;

/// Fill rule, mirroring `tiny_skia::FillRule`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Rule {
    NonZero,
    EvenOdd,
}

impl From<tiny_skia::FillRule> for Rule {
    fn from(r: tiny_skia::FillRule) -> Self {
        match r {
            tiny_skia::FillRule::Winding => Rule::NonZero,
            tiny_skia::FillRule::EvenOdd => Rule::EvenOdd,
        }
    }
}

/// Pixel rectangle `[x0, x1) x [y0, y1)` inside the mask.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct PixRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// One edge, endpoints in device pixels (y down).
#[derive(Clone, Copy)]
struct Line {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

/// Rasterise `path` (device coordinates) into `mask`, writing coverage
/// for every pixel of the returned rectangle and touching nothing outside
/// it. Coverage is multiplied by `clip` (same size as `mask`) when given.
/// Returns `None` when the path covers no pixel of the mask.
pub(crate) fn rasterize(
    path: &Path,
    rule: Rule,
    mask: &mut Mask,
    clip: Option<&Mask>,
) -> Option<PixRect> {
    let (mw, mh) = (mask.width(), mask.height());
    let b = path.bounds();
    let x0 = b.left().floor().max(0.0);
    let y0 = b.top().floor().max(0.0);
    let x1 = b.right().ceil().min(mw as f32);
    let y1 = b.bottom().ceil().min(mh as f32);
    if !(x0 < x1 && y0 < y1) {
        return None;
    }
    let rect = PixRect {
        x0: x0 as u32,
        y0: y0 as u32,
        x1: x1 as u32,
        y1: y1 as u32,
    };
    let w = (rect.x1 - rect.x0) as usize;
    let h = (rect.y1 - rect.y0) as usize;

    // Edges relative to the rectangle's origin, x clipped to [0, w]: an
    // edge left of the rectangle still adds its winding to every pixel
    // right of it, so it collapses onto x = 0; one right of it affects
    // nothing visible and collapses onto x = w.
    let mut lines = Vec::new();
    flatten(path, x0, y0, |l| push_clipped(&mut lines, l, w as f32));
    if lines.is_empty() {
        return None;
    }

    let stride = w + 2;
    let band = (BAND_FLOATS / stride).clamp(1, h);
    let mut acc = vec![0.0f32; stride * band];
    let mstride = mw as usize;
    let mut by = 0;
    while by < h {
        let bh = band.min(h - by);
        acc[..stride * bh].fill(0.0);
        for l in &lines {
            accumulate(&mut acc, stride, by, bh, l);
        }
        for row in 0..bh {
            let py = rect.y0 as usize + by + row;
            let start = py * mstride + rect.x0 as usize;
            let out = &mut mask.data_mut()[start..start + w];
            let src = &acc[row * stride..row * stride + w];
            let mut sum = 0.0f32;
            for (o, a) in out.iter_mut().zip(src) {
                sum += *a;
                *o = to_alpha(sum, rule);
            }
            if let Some(c) = clip {
                let cl = &c.data()[start..start + w];
                for (o, c) in out.iter_mut().zip(cl) {
                    *o = mul255(*o, *c);
                }
            }
        }
        by += bh;
    }
    Some(rect)
}

/// Winding-weighted coverage to an 8-bit alpha under `rule`.
fn to_alpha(v: f32, rule: Rule) -> u8 {
    let c = match rule {
        Rule::NonZero => v.abs().min(1.0),
        Rule::EvenOdd => {
            let m = v.abs() % 2.0;
            if m > 1.0 { 2.0 - m } else { m }
        }
    };
    (c * 255.0 + 0.5) as u8
}

/// `a * b / 255`, rounded.
fn mul255(a: u8, b: u8) -> u8 {
    let p = a as u32 * b as u32 + 128;
    ((p + (p >> 8)) >> 8) as u8
}

/// Walk `path` as straight edges relative to `(ox, oy)`, closing every
/// subpath (a fill closes open subpaths implicitly).
fn flatten(path: &Path, ox: f32, oy: f32, mut emit: impl FnMut(Line)) {
    let mut start = (0.0f32, 0.0f32);
    let mut cur = start;
    let mut edge = |from: (f32, f32), to: (f32, f32)| {
        emit(Line {
            x0: from.0 - ox,
            y0: from.1 - oy,
            x1: to.0 - ox,
            y1: to.1 - oy,
        })
    };
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                edge(cur, start);
                start = (p.x, p.y);
                cur = start;
            }
            PathSegment::LineTo(p) => {
                edge(cur, (p.x, p.y));
                cur = (p.x, p.y);
            }
            PathSegment::QuadTo(c, p) => {
                // Degree-elevate to a cubic: one flattening routine.
                let c1 = (
                    cur.0 + 2.0 / 3.0 * (c.x - cur.0),
                    cur.1 + 2.0 / 3.0 * (c.y - cur.1),
                );
                let c2 = (p.x + 2.0 / 3.0 * (c.x - p.x), p.y + 2.0 / 3.0 * (c.y - p.y));
                cubic(cur, c1, c2, (p.x, p.y), &mut |q| {
                    edge(cur, q);
                    cur = q;
                });
            }
            PathSegment::CubicTo(c1, c2, p) => {
                cubic(cur, (c1.x, c1.y), (c2.x, c2.y), (p.x, p.y), &mut |q| {
                    edge(cur, q);
                    cur = q;
                });
            }
            PathSegment::Close => {
                edge(cur, start);
                cur = start;
            }
        }
    }
    edge(cur, start);
}

/// Emit the chord endpoints of the cubic `p0 c1 c2 p3` (excluding `p0`),
/// enough chords that each stays within [`TOLERANCE`] of the curve.
fn cubic(
    p0: (f32, f32),
    c1: (f32, f32),
    c2: (f32, f32),
    p3: (f32, f32),
    point: &mut dyn FnMut((f32, f32)),
) {
    let dd = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        let x = a.0 - 2.0 * b.0 + c.0;
        let y = a.1 - 2.0 * b.1 + c.1;
        (x * x + y * y).sqrt()
    };
    let d = dd(p0, c1, c2).max(dd(c1, c2, p3));
    // A chord over parameter step 1/n deviates by at most
    // 3/4 * d / n^2 from the cubic (d = largest second difference).
    let n = ((0.75 * d / TOLERANCE).sqrt().ceil() as usize).clamp(1, MAX_CHORDS);
    for i in 1..n {
        let t = i as f32 / n as f32;
        let mt = 1.0 - t;
        let (a, b, c, e) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
        point((
            a * p0.0 + b * c1.0 + c * c2.0 + e * p3.0,
            a * p0.1 + b * c1.1 + c * c2.1 + e * p3.1,
        ));
    }
    point(p3);
}

/// Split `l` where it crosses x = 0 and x = `w`, clamp the pieces to
/// `[0, w]`, and keep the non-horizontal ones.
fn push_clipped(out: &mut Vec<Line>, l: Line, w: f32) {
    if l.y0 == l.y1 || !(l.x0.is_finite() && l.x1.is_finite()) {
        return;
    }
    let mut ts = [0.0f32, 1.0, 1.0, 1.0];
    let mut n = 1;
    for edge in [0.0, w] {
        if (l.x0 - edge) * (l.x1 - edge) < 0.0 {
            ts[n] = (edge - l.x0) / (l.x1 - l.x0);
            n += 1;
        }
    }
    ts[n] = 1.0;
    ts[1..n].sort_by(|a, b| a.total_cmp(b));
    let at = |t: f32| {
        (
            (l.x0 + (l.x1 - l.x0) * t).clamp(0.0, w),
            l.y0 + (l.y1 - l.y0) * t,
        )
    };
    for i in 0..n {
        let (ax, ay) = if i == 0 {
            (l.x0.clamp(0.0, w), l.y0)
        } else {
            at(ts[i])
        };
        let (bx, by) = if i + 1 == n {
            (l.x1.clamp(0.0, w), l.y1)
        } else {
            at(ts[i + 1])
        };
        if ay != by {
            out.push(Line {
                x0: ax,
                y0: ay,
                x1: bx,
                y1: by,
            });
        }
    }
}

/// Add the signed area of edge `l` to the rows `[by, by + bh)` of the
/// accumulation buffer (`acc` row 0 is row `by`). x is already in
/// `[0, stride - 2]`. After a running sum along a row, each cell holds
/// the winding-weighted area of its pixel the path covers. The per-row
/// arithmetic is font-rs's (Raph Levien, Apache-2.0/MIT).
fn accumulate(acc: &mut [f32], stride: usize, by: usize, bh: usize, l: &Line) {
    let (dir, a, b) = if l.y0 < l.y1 {
        (1.0f32, (l.x0, l.y0), (l.x1, l.y1))
    } else {
        (-1.0f32, (l.x1, l.y1), (l.x0, l.y0))
    };
    let top = by as f32;
    let bot = (by + bh) as f32;
    if b.1 <= top || a.1 >= bot {
        return;
    }
    let dxdy = (b.0 - a.0) / (b.1 - a.1);
    let ys = a.1.max(top);
    let ye = b.1.min(bot);
    let mut x = a.0 + (ys - a.1) * dxdy;
    let xmax = (stride - 2) as f32;
    let first = ys.floor() as usize;
    let last = (ye.ceil() as usize).min(by + bh);
    for y in first..last {
        let row = (y - by) * stride;
        let fy = y as f32;
        let dy = (fy + 1.0).min(ye) - fy.max(ys);
        if dy <= 0.0 {
            continue;
        }
        let xnext = (x + dxdy * dy).clamp(0.0, xmax);
        let d = dy * dir;
        let (x0, x1) = if x < xnext { (x, xnext) } else { (xnext, x) };
        let x0floor = x0.floor();
        let x0i = x0floor as usize;
        let x1ceil = x1.ceil();
        let x1i = x1ceil as usize;
        if x1i <= x0i + 1 {
            let xmf = 0.5 * (x + xnext) - x0floor;
            acc[row + x0i] += d - d * xmf;
            acc[row + x0i + 1] += d * xmf;
        } else {
            let s = 1.0 / (x1 - x0);
            let x0f = x0 - x0floor;
            let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
            let x1f = x1 - x1ceil + 1.0;
            let am = 0.5 * s * x1f * x1f;
            acc[row + x0i] += d * a0;
            if x1i == x0i + 2 {
                acc[row + x0i + 1] += d * (1.0 - a0 - am);
            } else {
                let a1 = s * (1.5 - x0f);
                acc[row + x0i + 1] += d * (a1 - a0);
                for xi in x0i + 2..x1i - 1 {
                    acc[row + xi] += d * s;
                }
                let a2 = a1 + (x1i - x0i - 3) as f32 * s;
                acc[row + x1i - 1] += d * (1.0 - a2 - am);
            }
            acc[row + x1i] += d * am;
        }
        x = xnext;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{PathBuilder, Rect};

    fn render(path: &Path, rule: Rule, w: u32, h: u32) -> Mask {
        let mut m = Mask::new(w, h).unwrap();
        rasterize(path, rule, &mut m, None);
        m
    }

    fn mean(m: &Mask) -> f64 {
        m.data().iter().map(|&v| v as f64).sum::<f64>() / m.data().len() as f64 / 255.0
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Path {
        PathBuilder::from_rect(Rect::from_xywh(x, y, w, h).unwrap())
    }

    #[test]
    fn thin_rule_coverage_tracks_its_area() {
        // The issue's ramp: rules 0.04 .. 0.56 px tall, each on its own
        // canvas, so coverage is area / canvas area.
        for &h in &[0.04f32, 0.1, 0.15, 0.25, 0.38, 0.56] {
            let m = render(&rect(0.0, 3.3, 16.0, h), Rule::NonZero, 16, 8);
            let expected = (h * 16.0 / 128.0) as f64;
            assert!(
                (mean(&m) - expected).abs() < 0.004,
                "h={h}: mean {} vs {expected}",
                mean(&m)
            );
        }
    }

    #[test]
    fn diagonal_sliver_keeps_its_area() {
        // A 0.1 px wide parallelogram at a slant: area 0.1 * 10.
        let mut pb = PathBuilder::new();
        pb.move_to(1.0, 1.0);
        pb.line_to(1.1, 1.0);
        pb.line_to(9.1, 11.0);
        pb.line_to(9.0, 11.0);
        pb.close();
        let m = render(&pb.finish().unwrap(), Rule::NonZero, 12, 12);
        let total: f64 = m.data().iter().map(|&v| v as f64 / 255.0).sum();
        assert!((total - 1.0).abs() < 0.05, "total {total}");
    }

    #[test]
    fn fill_rules_on_overlaps() {
        // Same-wound overlap stays at full coverage under nonzero.
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(0.0, 0.0, 6.0, 6.0).unwrap());
        pb.push_rect(Rect::from_xywh(2.0, 2.0, 6.0, 6.0).unwrap());
        let same = pb.finish().unwrap();
        let m = render(&same, Rule::NonZero, 8, 8);
        assert_eq!(m.data()[3 * 8 + 3], 255);
        // ... and drops out under even-odd.
        let m = render(&same, Rule::EvenOdd, 8, 8);
        assert_eq!(m.data()[3 * 8 + 3], 0);
        assert_eq!(m.data()[8 + 1], 255);

        // Opposite-wound overlap cancels under nonzero.
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(0.0, 0.0, 6.0, 6.0).unwrap());
        pb.move_to(2.0, 2.0);
        pb.line_to(2.0, 8.0);
        pb.line_to(8.0, 8.0);
        pb.line_to(8.0, 2.0);
        pb.close();
        let m = render(&pb.finish().unwrap(), Rule::NonZero, 8, 8);
        assert_eq!(m.data()[3 * 8 + 3], 0);
        assert_eq!(m.data()[8 + 1], 255);
        assert_eq!(m.data()[7 * 8 + 7], 255);
    }

    #[test]
    fn half_pixel_edges_and_off_canvas_parts() {
        // Spans x -5 .. 2.5: the part off the left edge still fills.
        let m = render(&rect(-5.0, 0.0, 7.5, 2.0), Rule::NonZero, 4, 2);
        assert_eq!(&m.data()[..4], &[255, 255, 128, 0]);
    }

    #[test]
    fn circle_area_and_banding() {
        // A disc big enough to need several bands on a wide canvas.
        let r = 300.0;
        let pb = PathBuilder::from_circle(400.0, 400.0, r).unwrap();
        let m = render(&pb, Rule::NonZero, 2000, 800);
        let total: f64 = m.data().iter().map(|&v| v as f64 / 255.0).sum();
        let expected = std::f64::consts::PI * (r as f64) * (r as f64);
        assert!(
            (total / expected - 1.0).abs() < 0.002,
            "{total} vs {expected}"
        );
    }

    #[test]
    fn clip_scales_coverage() {
        let mut clip = Mask::new(4, 1).unwrap();
        clip.data_mut().copy_from_slice(&[255, 128, 0, 255]);
        let mut m = Mask::new(4, 1).unwrap();
        let r = rasterize(
            &rect(0.0, 0.0, 4.0, 1.0),
            Rule::NonZero,
            &mut m,
            Some(&clip),
        );
        assert_eq!(
            r,
            Some(PixRect {
                x0: 0,
                y0: 0,
                x1: 4,
                y1: 1
            })
        );
        assert_eq!(m.data(), &[255, 128, 0, 255]);
    }
}
