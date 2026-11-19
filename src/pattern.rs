//! Tiling patterns (#1038): `makepattern`'s PatternType 1, painted by
//! `fill`, `stroke` and the show family in all three outputs.
//!
//! `makepattern` runs the pattern's `PaintProc` once, at once (PLRM 4.9
//! lets an implementation call it whenever it likes and cache the
//! result; gs calls it at first use). While it runs, [`Gfx`] paints into
//! a *capture*: a tile-sized canvas instead of the page, a fresh SVG
//! recorder, and an empty PDF content stream, all at the same CTM. What
//! the three hold afterwards is the tile in each output:
//!
//! - **PNG**: the capture canvas is folded into one period of the
//!   lattice (copies of it at every whole step, so a cell larger than
//!   the step wraps round), and fills use it as a repeating
//!   `tiny_skia::Pattern` shader.
//! - **SVG**: a `<pattern>` def, its content the captured elements.
//! - **PDF**: a `/PatternType 1` pattern stream holding the captured
//!   content and its own resources.
//!
//! **Whole-pixel steps.** The period is a whole number of device pixels
//! in each direction: the pattern matrix is adjusted so the step lengths
//! round to whole pixels (TilingType 1 and 3 allow a cell to be
//! distorted by up to a pixel for exactly this; gs adjusts the same way).
//! When the pattern's axes lie along the device's, the shader then maps
//! the period pixmap one pixel to one pixel, at a whole-pixel offset,
//! sampled nearest: no resampling, so no seams and no blur. The adjusted
//! matrix is the one SVG and PDF get too, so the three outputs place the
//! cells alike across the page instead of drifting apart by the rounding
//! on every cell. A pattern turned off the device axes is resampled
//! (bilinear) from its period.
//!
//! **Contexts.** A pattern is made in a context: the page, or the
//! capture of another pattern's `PaintProc`. Its matrix maps to that
//! context's device space, so it paints only there: a page pattern used
//! inside a `PaintProc`, or one made in a `PaintProc` and used after it,
//! paints nothing (a documented gap; nesting patterns is rare).
//!
//! **Uncoloured patterns** (PaintType 2) paint their tile's coverage in
//! the colour `setpattern` gives with the pattern: the raster recolours
//! the period, SVG writes the content in `currentColor` under a
//! `color`, PDF writes an uncoloured pattern with no colour operators.
//! Colour operators in the `PaintProc` are ignored, as in gs.

use std::cell::RefCell;
use std::rc::Rc;

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};

use crate::error::PsError;

/// Longest side, in pixels, of a tile's capture canvas or period.
pub(crate) const MAX_TILE_SIDE: u32 = 16384;

/// Most pixels in a tile's capture canvas or period (256 MiB of RGBA).
pub(crate) const MAX_TILE_PIXELS: u64 = 1 << 26;

/// Most copies of the capture canvas folded into the period: a cell
/// thousands of steps wide is refused, not drawn thousands of times over.
const MAX_FOLD_COPIES: u64 = 1 << 16;

/// What `makepattern` read from a PatternType 1 dictionary.
#[derive(Clone, Debug)]
pub(crate) struct TileSpec {
    /// PaintType 2.
    pub(crate) uncolored: bool,
    pub(crate) tiling_type: u8,
    /// Pattern space, `x0 < x1`, `y0 < y1`.
    pub(crate) bbox: [f64; 4],
    pub(crate) xstep: f64,
    pub(crate) ystep: f64,
    /// Pattern space to (the context's) device space: the dictionary's
    /// matrix times the CTM at `makepattern`.
    pub(crate) matrix: Transform,
}

/// Where a tile's pieces sit, worked out once from its [`TileSpec`].
///
/// Three spaces: pattern space; R, pattern space scaled by
/// `diag(pw/|XStep|, ph/|YStep|)` so a step is a whole `pw` x `ph`
/// pixels; and device space. The capture canvas is R shifted by
/// `origin`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    /// Pattern space to the capture canvas: the CTM `PaintProc` runs at.
    pub(crate) capture: Transform,
    /// R to device space: the period pixmap's shader transform.
    pub(crate) shader: Transform,
    /// The adjusted pattern matrix, pattern space to device space
    /// (`shader` after the R scale). SVG and PDF write this one.
    pub(crate) matrix: Transform,
    /// `shader` maps pixels to pixels (pattern axes along the device's):
    /// sample nearest.
    pub(crate) exact: bool,
    /// The capture canvas's top-left corner in R.
    pub(crate) origin: (i32, i32),
    /// The capture canvas's size.
    pub(crate) content: (u32, u32),
    /// One period of the lattice, `(pw, ph)`.
    pub(crate) period: (u32, u32),
}

fn checked_side(len: f64) -> Result<u32, PsError> {
    if !len.is_finite() {
        return Err(PsError::Rangecheck);
    }
    let n = len.max(1.0);
    if n > f64::from(MAX_TILE_SIDE) {
        return Err(PsError::Limitcheck);
    }
    Ok(n as u32)
}

fn checked_area(w: u32, h: u32) -> Result<(), PsError> {
    if u64::from(w) * u64::from(h) > MAX_TILE_PIXELS {
        return Err(PsError::Limitcheck);
    }
    Ok(())
}

/// The tile's geometry, or the error `makepattern` gives: `rangecheck`
/// for a pattern matrix that collapses a step, `limitcheck` for a tile
/// past [`MAX_TILE_SIDE`] or [`MAX_TILE_PIXELS`].
pub(crate) fn geometry(spec: &TileSpec) -> Result<Geometry, PsError> {
    let m = spec.matrix;
    let (sx_, ky, kx, sy_, tx, ty) = (
        f64::from(m.sx),
        f64::from(m.ky),
        f64::from(m.kx),
        f64::from(m.sy),
        f64::from(m.tx),
        f64::from(m.ty),
    );
    let (ax, ay) = (spec.xstep.abs(), spec.ystep.abs());
    // Device length of one step along each pattern axis.
    let lx = ax * sx_.hypot(ky);
    let ly = ay * kx.hypot(sy_);
    if !(lx.is_finite() && ly.is_finite()) || lx <= 0.0 || ly <= 0.0 {
        return Err(PsError::Rangecheck);
    }
    let pw = checked_side(lx.round())?;
    let ph = checked_side(ly.round())?;
    checked_area(pw, ph)?;
    let (rsx, rsy) = (f64::from(pw) / ax, f64::from(ph) / ay);
    let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    let tol = 1e-6;
    let along = ky.abs() <= tol * sx_.abs() && kx.abs() <= tol * sy_.abs();
    let across = sx_.abs() <= tol * ky.abs() && sy_.abs() <= tol * kx.abs();
    let (shader, exact) = if along {
        (
            [sign(sx_), 0.0, 0.0, sign(sy_), tx.round(), ty.round()],
            true,
        )
    } else if across {
        ([0.0, sign(ky), sign(kx), 0.0, tx.round(), ty.round()], true)
    } else {
        ([sx_ / rsx, ky / rsx, kx / rsy, sy_ / rsy, tx, ty], false)
    };
    let shader_t = Transform::from_row(
        shader[0] as f32,
        shader[1] as f32,
        shader[2] as f32,
        shader[3] as f32,
        shader[4] as f32,
        shader[5] as f32,
    );
    // Pattern space to device: R's scale, then the shader.
    let matrix = Transform::from_row(
        (shader[0] * rsx) as f32,
        (shader[1] * rsx) as f32,
        (shader[2] * rsy) as f32,
        (shader[3] * rsy) as f32,
        shader[4] as f32,
        shader[5] as f32,
    );
    let [bx0, by0, bx1, by1] = spec.bbox;
    let (rx0, rx1, ry0, ry1) = (bx0 * rsx, bx1 * rsx, by0 * rsy, by1 * rsy);
    if ![rx0, rx1, ry0, ry1].iter().all(|v| v.is_finite()) {
        return Err(PsError::Rangecheck);
    }
    let (ix0, iy0) = (rx0.floor(), ry0.floor());
    let cw = checked_side(rx1.ceil() - ix0)?;
    let ch = checked_side(ry1.ceil() - iy0)?;
    checked_area(cw, ch)?;
    if ix0.abs() > 1e9 || iy0.abs() > 1e9 {
        return Err(PsError::Limitcheck);
    }
    let capture = Transform::from_row(rsx as f32, 0.0, 0.0, rsy as f32, -ix0 as f32, -iy0 as f32);
    Ok(Geometry {
        capture,
        shader: shader_t,
        matrix,
        exact,
        origin: (ix0 as i32, iy0 as i32),
        content: (cw, ch),
        period: (pw, ph),
    })
}

/// The lattice offsets `o + k*p` (any integer `k`) at which a run of
/// `len` pixels starting there overlaps `[0, p)`.
fn fold_offsets(o: i32, len: u32, p: u32) -> Vec<i32> {
    let (o, len, p) = (i64::from(o), i64::from(len), i64::from(p));
    // o + k*p + len > 0  and  o + k*p < p
    let lo = (-len - o).div_euclid(p) + 1;
    let hi = (p - o - 1).div_euclid(p);
    (lo..=hi).map(|k| (o + k * p) as i32).collect()
}

/// How many copies [`fold`] would draw; refused past
/// [`MAX_FOLD_COPIES`] before anything is allocated.
pub(crate) fn check_fold(g: &Geometry) -> Result<(), PsError> {
    let n = |len: u32, p: u32| u64::from(len).div_ceil(u64::from(p)) + 1;
    let copies = n(g.content.0, g.period.0) * n(g.content.1, g.period.1);
    if copies > MAX_FOLD_COPIES {
        return Err(PsError::Limitcheck);
    }
    Ok(())
}

/// One period of the lattice: the capture canvas drawn at every whole
/// step that reaches the period, so a cell bigger than its step wraps.
/// Later copies go over earlier ones (the PLRM leaves overlapping cells'
/// order to the implementation).
pub(crate) fn fold(content: &Pixmap, g: &Geometry) -> Option<Pixmap> {
    let (pw, ph) = g.period;
    let mut out = Pixmap::new(pw, ph)?;
    let xs = fold_offsets(g.origin.0, content.width(), pw);
    let ys = fold_offsets(g.origin.1, content.height(), ph);
    let paint = PixmapPaint::default();
    for &y in &ys {
        for &x in &xs {
            out.draw_pixmap(x, y, content.as_ref(), &paint, Transform::identity(), None);
        }
    }
    Some(out)
}

/// The SVG side of a tile: the capture recorder's defs and body, in
/// capture-canvas coordinates.
pub(crate) struct SvgTile {
    pub(crate) defs: String,
    pub(crate) body: String,
}

/// A pattern `makepattern` made, as the graphics state holds it.
pub(crate) struct PatternTile {
    /// Unique in the run; names the tile's SVG defs.
    pub(crate) id: usize,
    pub(crate) uncolored: bool,
    /// The capture this pattern was made in, 0 for the page.
    pub(crate) context: usize,
    pub(crate) spec: TileSpec,
    pub(crate) geom: Geometry,
    /// One period of the lattice, premultiplied RGBA.
    period: Rc<Pixmap>,
    /// The last colour an uncoloured period was painted in.
    recolored: RefCell<Option<([u8; 3], Rc<Pixmap>)>>,
    /// Present when SVG was being recorded at `makepattern`.
    pub(crate) svg: Option<SvgTile>,
    /// The PDF recorder's pattern index, when PDF was being recorded.
    pub(crate) pdf: Option<usize>,
}

impl PatternTile {
    pub(crate) fn new(
        id: usize,
        context: usize,
        spec: TileSpec,
        geom: Geometry,
        period: Pixmap,
        svg: Option<SvgTile>,
        pdf: Option<usize>,
    ) -> Self {
        PatternTile {
            id,
            uncolored: spec.uncolored,
            context,
            spec,
            geom,
            period: Rc::new(period),
            recolored: RefCell::new(None),
            svg,
            pdf,
        }
    }

    /// The raster shader's source: the period (recoloured to `rgb` for an
    /// uncoloured pattern), its transform, and the sampling.
    pub(crate) fn raster(&self, rgb: (f32, f32, f32)) -> (Rc<Pixmap>, Transform, FilterQuality) {
        let quality = if self.geom.exact {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bilinear
        };
        let pm = if self.uncolored {
            let key = [to_u8(rgb.0), to_u8(rgb.1), to_u8(rgb.2)];
            let mut cache = self.recolored.borrow_mut();
            match &*cache {
                Some((k, pm)) if *k == key => pm.clone(),
                _ => {
                    let pm = Rc::new(recolor(&self.period, key));
                    *cache = Some((key, pm.clone()));
                    pm
                }
            }
        } else {
            self.period.clone()
        };
        (pm, self.geom.shader, quality)
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `src`'s coverage (its alpha) in one colour.
fn recolor(src: &Pixmap, [r, g, b]: [u8; 3]) -> Pixmap {
    let mut out = src.clone();
    let mul = |c: u8, a: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
    for p in out.pixels_mut() {
        let a = p.alpha();
        if let Some(c) = PremultipliedColorU8::from_rgba(mul(r, a), mul(g, a), mul(b, a), a) {
            *p = c;
        }
    }
    out
}

/// A pattern's `PaintProc` running (#1038): what [`crate::gfx::Gfx`]
/// swapped out of the page (or the enclosing capture) to paint the tile,
/// put back when it ends or is unwound.
pub(crate) struct Capture {
    /// This capture's context id (what its patterns' `context` holds).
    pub(crate) id: usize,
    /// The context `makepattern` ran in.
    pub(crate) context: usize,
    pub(crate) spec: TileSpec,
    pub(crate) geom: Geometry,
    pub(crate) page: Pixmap,
    pub(crate) svg: Option<Box<crate::svg::SvgRecorder>>,
    pub(crate) pdf: Option<crate::pdf::PdfCapture>,
    pub(crate) pending_erase: bool,
    pub(crate) painted_since_page: bool,
    pub(crate) dirty: bool,
    pub(crate) band: Option<(u32, u32)>,
    pub(crate) suppress_paint: u32,
    pub(crate) coverage: Option<tiny_skia::Mask>,
    pub(crate) snapshot: (usize, Box<crate::gfx::GraphicsState>),
}

/// What a fill or stroke paints with, as the SVG and PDF recorders take
/// it.
#[derive(Clone)]
pub(crate) enum Ink {
    Rgb((f32, f32, f32)),
    /// A tiling pattern, and the colour an uncoloured one paints in.
    Pattern(Rc<PatternTile>, (f32, f32, f32)),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(matrix: Transform, bbox: [f64; 4], xstep: f64, ystep: f64) -> TileSpec {
        TileSpec {
            uncolored: false,
            tiling_type: 1,
            bbox,
            xstep,
            ystep,
            matrix,
        }
    }

    #[test]
    fn fold_offsets_cover_the_period_and_nothing_else() {
        // A cell exactly one period, at the origin: one copy.
        assert_eq!(fold_offsets(0, 10, 10), vec![0]);
        // Shifted: the copy before reaches in too.
        assert_eq!(fold_offsets(3, 10, 10), vec![-7, 3]);
        // A cell two and a half periods wide.
        assert_eq!(fold_offsets(-5, 25, 10), vec![-15, -5, 5]);
        // Far from the origin, same answer as its residue.
        assert_eq!(fold_offsets(1003, 10, 10), vec![-7, 3]);
    }

    #[test]
    fn whole_pixel_steps_snap_an_axis_aligned_matrix() {
        // 72 dpi page, y flipped: a 7.3 pt step becomes 7 px.
        let m = Transform::from_row(1.0, 0.0, 0.0, -1.0, 10.25, 500.6);
        let g = geometry(&spec(m, [0.0, 0.0, 7.3, 7.3], 7.3, 7.3)).unwrap();
        assert_eq!(g.period, (7, 7));
        assert!(g.exact);
        assert_eq!(
            (g.shader.sx, g.shader.sy, g.shader.tx, g.shader.ty),
            (1.0, -1.0, 10.0, 501.0)
        );
        // The adjusted matrix takes a step to exactly 7 px.
        let mut p = [tiny_skia::Point::from_xy(7.3, 0.0)];
        g.matrix.map_points(&mut p);
        assert!((p[0].x - 17.0).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn a_rotated_pattern_is_resampled() {
        let m = Transform::from_rotate(30.0);
        let g = geometry(&spec(m, [0.0, 0.0, 10.0, 10.0], 10.0, 10.0)).unwrap();
        assert!(!g.exact);
        assert_eq!(g.period, (10, 10));
    }

    #[test]
    fn collapsed_and_huge_tiles_are_refused() {
        let flat = Transform::from_row(1.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(matches!(
            geometry(&spec(flat, [0.0, 0.0, 1.0, 1.0], 1.0, 1.0)),
            Err(PsError::Rangecheck)
        ));
        let big = Transform::from_scale(100.0, 100.0);
        assert!(matches!(
            geometry(&spec(big, [0.0, 0.0, 1000.0, 1.0], 1000.0, 1.0)),
            Err(PsError::Limitcheck)
        ));
    }
}
