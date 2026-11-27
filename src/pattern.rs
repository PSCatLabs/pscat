//! Tiling patterns (#1038): `makepattern`'s PatternType 1, painted by
//! `fill`, `stroke` and the show family in all three outputs.
//!
//! `makepattern` checks the dictionary and keeps the pattern as a
//! [`LazyPattern`]; the `PaintProc` runs once, at the pattern's first use,
//! when a marking operator meets it as the current colour (PLRM 4.9 lets an
//! implementation call it whenever it likes and cache the result; gs does
//! this, #1341). While it runs, [`Gfx`] paints into a *capture*: a
//! tile-sized canvas instead of the page, a fresh SVG recorder, and an
//! empty PDF content stream, all at the CTM `makepattern` had, with the
//! graphics state it had. What the three hold afterwards is the tile in
//! each output:
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
//! distorted by up to a pixel for exactly this; gs adjusts the same way
//! when the device `/BBox` is within half a pixel of the step, and leaves
//! the step fractional otherwise, which pscat does not follow). The cells
//! sit where gs puts them: from the device top-left corner of the
//! `/BBox` rounded to a pixel, not from the rounded origin
//! (`gs_translation`, #1342).
//! When the pattern's axes lie along the device's, the shader then maps
//! the period pixmap one pixel to one pixel, at a whole-pixel offset,
//! sampled nearest: no resampling, so no seams and no blur. The adjusted
//! matrix is the one SVG and PDF get too, so the three outputs place the
//! cells alike across the page instead of drifting apart by the rounding
//! on every cell. A pattern turned off the device axes is resampled
//! (bilinear) from its period.
//!
//! **TilingType 2** (no distortion, #1332) is the exception: its cell keeps
//! its true size and the spacing varies instead. The raster lays the cell
//! out at `floor(i * step)` pixels per axis, as gs does ([`Lattice`]), and
//! SVG and PDF get the unadjusted matrix.
//!
//! **Contexts** (#1300). A pattern is made in a context: the page, or the
//! capture of another pattern's `PaintProc`. Its matrix maps to that
//! context's device space. gs keeps that device matrix and applies it
//! unchanged in whatever device paints with the pattern: inside a
//! `PaintProc` that is the pattern accumulator, a device-aligned bitmap
//! whose origin is the top-left pixel of the cell's `/BBox` in device
//! space. So a page pattern inside a `PaintProc` keeps its device scale
//! (the outer pattern's scale does not reach it) and its lattice starts
//! at that corner; one made inside a `PaintProc` and used on the page is
//! placed by its accumulator matrix read as page device coordinates.
//! Each context records [`Capture::to_gs`], its coordinates to gs's
//! device coordinates for it, and a pattern used in another context is
//! placed through the two ([`foreign_placement`]).
//!
//! **Uncoloured patterns** (PaintType 2) paint their tile's coverage in
//! the colour `setpattern` gives with the pattern: the raster recolours
//! the period, SVG writes the content in `currentColor` under a
//! `color`, PDF writes an uncoloured pattern with no colour operators.
//! Colour operators in the `PaintProc` are ignored, as in gs.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, PremultipliedColorU8, Transform};

use crate::error::PsError;

/// Longest side, in pixels, of a tile's capture canvas or period.
pub(crate) const MAX_TILE_SIDE: u32 = 16384;

/// Most pixels in a tile's capture canvas or period (256 MiB of RGBA).
pub(crate) const MAX_TILE_PIXELS: u64 = 1 << 26;

/// Most captures open at once (a `PaintProc` using a pattern whose own
/// `PaintProc` uses a pattern ...): a pattern that names itself, which the
/// late first run makes possible, recurses until here, as it does in gs
/// until the exec stack gives out (`execstackoverflow`).
pub(crate) const MAX_CAPTURE_DEPTH: usize = 64;

/// Most pixels, canvases and periods together, in the captures open at
/// once, so a self-naming pattern with a large cell stops after a few
/// levels instead of after gigabytes (`limitcheck`).
pub(crate) const MAX_NESTED_PIXELS: u64 = 2 * MAX_TILE_PIXELS;

/// The pixels a tile's capture canvas and period take.
pub(crate) fn tile_pixels(g: &Geometry) -> u64 {
    u64::from(g.content.0) * u64::from(g.content.1) + u64::from(g.period.0) * u64::from(g.period.1)
}

/// Most copies of the capture canvas folded into the period: a cell
/// thousands of steps wide is refused, not drawn thousands of times over.
pub(crate) const MAX_FOLD_COPIES: u64 = 1 << 16;

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
/// pixels, then shifted by under a pixel so the device corner gs puts the
/// `/BBox` at is a whole pixel (#1342); and device space. The capture
/// canvas is R shifted by `origin`.
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
    /// TilingType 2 with a fractional step: where the cells really go.
    pub(crate) lattice: Option<Lattice>,
}

/// The cells of a TilingType 2 pattern whose step is not a whole number of
/// device pixels (#1332). The cell keeps its true size, and the spacing
/// varies instead: cell `i` along a device axis starts at
/// `floor(i * step)` pixels from the anchor, `step` signed, as Ghostscript
/// places them (gs 10.07.1: a 7.3 px step gives gaps of 7, 7, 7, 8, ...).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lattice {
    /// Device step along x and along y, signed, in pixels.
    pub(crate) step: (f64, f64),
    /// Device position of cell 0's top-left pixel (the pixel gs puts the
    /// `/BBox` corner on).
    pub(crate) origin: (i32, i32),
    /// The capture canvas to the device-oriented cell (a signed
    /// permutation of pixels).
    pub(crate) orient: Transform,
    /// The oriented cell's size.
    pub(crate) cell: (u32, u32),
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

/// The device x and y ranges `[lo, hi]` of the box `[x0, y0, x1, y1]`
/// under the linear map `[a, b, c, d]` (tiny-skia's `sx ky kx sy`).
fn device_span([a, b, c, d]: [f64; 4], [x0, y0, x1, y1]: [f64; 4]) -> [(f64, f64); 2] {
    let corners = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)];
    let span = |f: &dyn Fn(f64, f64) -> f64| {
        corners
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(x, y)| {
                let v = f(x, y);
                (lo.min(v), hi.max(v))
            })
    };
    [span(&|x, y| a * x + c * y), span(&|x, y| b * x + d * y)]
}

/// gs's `round`: half up, as at its pattern accumulator's origin
/// (`capture_to_gs`).
fn gs_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// gs's whole-pixel cell size along a device axis, from the device range
/// of the `/BBox` (`floor(bbw + 0.5)`, at least 1).
fn gs_size((u0, u1): (f64, f64)) -> f64 {
    gs_round(u1 - u0).max(1.0)
}

/// The factor gs scales a pattern axis by to make a device step of
/// `step` the whole `size`: exactly for a step of 2 px or less, else to
/// `fixed_1` (1/256 px) short of it.
fn gs_scale(size: f64, step: f64) -> f64 {
    if step <= 2.0 {
        size / step
    } else {
        (size - 1.0 / 256.0) / step
    }
}

/// The device translation, along one device axis, of an axis-aligned (or
/// quarter-turned) pattern whose cell is placed as gs 10.07.1 places it
/// (#1342; `gsptype1.c`, `gs_pattern1_make_pattern`). gs first moves the
/// pattern so the device top-left corner of its `/BBox` (under the
/// unadjusted matrix) lands on the nearest pixel. For TilingType 1 and 3
/// whose device `/BBox` is within half a pixel of the step on both axes,
/// it then scales the cell to a whole-pixel step, moves it by the whole
/// pixels that keep the `/BBox` centre where it was (`floor(o * (size -
/// step) + 0.5)`, `o` the centre in steps; not for a step of 2 px or
/// less), and puts the scaled `/BBox` corner on a pixel again. The cells
/// sit at that corner plus whole steps.
///
/// `t` is the translation, `span` the device range of the `/BBox` under
/// the unadjusted matrix (relative to `t`), `step` the device step length,
/// `scaled` the factor gs scaled this device axis's extent by (`None` when
/// it does not adjust), and `lo` the low edge of pscat's own cell relative
/// to its translation. The result puts pscat's cell edge on gs's corner.
fn gs_translation(t: f64, span: (f64, f64), step: f64, scaled: Option<f64>, lo: f64) -> f64 {
    let (u0, u1) = span;
    // adjust_bbox_to_pixel_origin under the unadjusted matrix.
    let t1 = t + (gs_round(t + u0) - (t + u0));
    let Some(scale) = scaled else {
        return gs_round(t1 + u0) - lo;
    };
    let t2 = if step <= 2.0 {
        t1
    } else {
        let o = (u0 + u1) / 2.0 / step;
        t1 - gs_round(o * (gs_size(span) - step))
    };
    // fix_bbox_after_matrix_adjustment: the scaled corner on a pixel.
    gs_round(t2 + u0 * scale) - lo
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
    let lx = ax * crate::fmath::hypot(sx_, ky);
    let ly = ay * crate::fmath::hypot(kx, sy_);
    if !(lx.is_finite() && ly.is_finite()) || lx <= 0.0 || ly <= 0.0 {
        return Err(PsError::Rangecheck);
    }
    let pw = checked_side(lx.round())?;
    let ph = checked_side(ly.round())?;
    checked_area(pw, ph)?;
    let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
    let tol = 1e-6;
    let along = ky.abs() <= tol * sx_.abs() && kx.abs() <= tol * sy_.abs();
    let across = sx_.abs() <= tol * ky.abs() && sy_.abs() <= tol * kx.abs();
    // TilingType 2 allows no distortion (PLRM 4.9.2): an axis-aligned
    // cell keeps its true size and the spacing is spread over whole
    // pixels instead (#1332). Types 1 and 3 scale the cell to the step.
    let whole = |l: f64, p: u32| (l - f64::from(p)).abs() <= 1e-3;
    let undistorted = spec.tiling_type == 2
        && (along || across)
        && lx >= 1.0
        && ly >= 1.0
        && !(whole(lx, pw) && whole(ly, ph));
    let (rsx, rsy) = if undistorted {
        (lx / ax, ly / ay)
    } else {
        (f64::from(pw) / ax, f64::from(ph) / ay)
    };
    let [bx0, by0, bx1, by1] = spec.bbox;
    let (rx0, rx1, ry0, ry1) = (bx0 * rsx, bx1 * rsx, by0 * rsy, by1 * rsy);
    if ![rx0, rx1, ry0, ry1].iter().all(|v| v.is_finite()) {
        return Err(PsError::Rangecheck);
    }
    let lin = if along {
        Some([sign(sx_), 0.0, 0.0, sign(sy_)])
    } else if across {
        Some([0.0, sign(ky), sign(kx), 0.0])
    } else {
        None
    };
    // `shift`: R is shifted by it (in R units) before the capture canvas's
    // pixel grid is laid over it, so the canvas pixels are device pixels.
    let (shader, exact, shift) = match lin {
        Some([a, b, c, d]) => {
            // Where gs puts the cell (#1342), split into a whole-pixel
            // shader translation and a sub-pixel shift of R.
            let [ux, uy] = device_span([sx_, ky, kx, sy_], spec.bbox);
            let [px, py] = device_span([a, b, c, d], [rx0, ry0, rx1, ry1]);
            // Device step lengths along x and y.
            let (dsx, dsy) = if along { (lx, ly) } else { (ly, lx) };
            let near = |step: f64, (u0, u1): (f64, f64)| (step - (u1 - u0)).abs() < 0.5;
            let gs_adjusts = spec.tiling_type != 2 && near(dsx, ux) && near(dsy, uy);
            // gs scales the pattern's x axis by the device x factor and its
            // y axis by the device y one, so a quarter-turned pattern's
            // device x extent gets the device y factor.
            let (fx, fy) = (gs_scale(gs_size(ux), dsx), gs_scale(gs_size(uy), dsy));
            let (fx, fy) = if along { (fx, fy) } else { (fy, fx) };
            let cx = gs_translation(tx, ux, dsx, gs_adjusts.then_some(fx), px.0);
            let cy = gs_translation(ty, uy, dsy, gs_adjusts.then_some(fy), py.0);
            let (stx, sty) = (cx.round(), cy.round());
            // R shift g with lin * g = (cx - stx, cy - sty); lin is its own
            // inverse's transpose (a signed permutation).
            let (ex, ey) = (cx - stx, cy - sty);
            let g = (a * ex + b * ey, c * ex + d * ey);
            ([a, b, c, d, stx, sty], true, g)
        }
        None => (
            [sx_ / rsx, ky / rsx, kx / rsy, sy_ / rsy, tx, ty],
            false,
            (0.0, 0.0),
        ),
    };
    let shader_t = Transform::from_row(
        shader[0] as f32,
        shader[1] as f32,
        shader[2] as f32,
        shader[3] as f32,
        shader[4] as f32,
        shader[5] as f32,
    );
    // Pattern space to device: R's scale and shift, then the shader.
    let matrix = Transform::from_row(
        (shader[0] * rsx) as f32,
        (shader[1] * rsx) as f32,
        (shader[2] * rsy) as f32,
        (shader[3] * rsy) as f32,
        (shader[4] + shader[0] * shift.0 + shader[2] * shift.1) as f32,
        (shader[5] + shader[1] * shift.0 + shader[3] * shift.1) as f32,
    );
    // The /BBox in shifted R. A corner gs puts on a pixel is a whole number
    // here up to rounding noise, which must not add an empty pixel row.
    let snap = |v: f64| {
        if (v - v.round()).abs() < 1e-6 {
            v.round()
        } else {
            v
        }
    };
    let (rx0, rx1) = (snap(rx0 + shift.0), snap(rx1 + shift.0));
    let (ry0, ry1) = (snap(ry0 + shift.1), snap(ry1 + shift.1));
    let (ix0, iy0) = (rx0.floor(), ry0.floor());
    let cw = checked_side(rx1.ceil() - ix0)?;
    let ch = checked_side(ry1.ceil() - iy0)?;
    checked_area(cw, ch)?;
    if ix0.abs() > 1e9 || iy0.abs() > 1e9 {
        return Err(PsError::Limitcheck);
    }
    let capture = Transform::from_row(
        rsx as f32,
        0.0,
        0.0,
        rsy as f32,
        (shift.0 - ix0) as f32,
        (shift.1 - iy0) as f32,
    );
    let lattice = undistorted.then(|| {
        // Pixel (u, v) of the capture canvas is R pixel (ix0 + u, iy0 + v),
        // which the shader's signed permutation takes to the device.
        let (a, b, c, d) = (shader[0], shader[1], shader[2], shader[3]);
        let corners = [
            (0.0, 0.0),
            (cw as f64, 0.0),
            (0.0, ch as f64),
            (cw as f64, ch as f64),
        ]
        .map(|(u, v)| (ix0 + u, iy0 + v));
        let lo = |f: &dyn Fn(f64, f64) -> f64| {
            corners
                .iter()
                .map(|&(x, y)| f(x, y))
                .fold(f64::INFINITY, f64::min)
        };
        let (minx, miny) = (lo(&|x, y| a * x + c * y), lo(&|x, y| b * x + d * y));
        let step = if along {
            (sign(sx_) * lx, sign(sy_) * ly)
        } else {
            (sign(kx) * ly, sign(ky) * lx)
        };
        Lattice {
            step,
            origin: ((shader[4] + minx) as i32, (shader[5] + miny) as i32),
            orient: Transform::from_row(
                a as f32,
                b as f32,
                c as f32,
                d as f32,
                (a * ix0 + c * iy0 - minx) as f32,
                (b * ix0 + d * iy0 - miny) as f32,
            ),
            cell: if along { (cw, ch) } else { (ch, cw) },
        }
    });
    Ok(Geometry {
        capture,
        shader: shader_t,
        matrix,
        exact,
        origin: (ix0 as i32, iy0 as i32),
        content: (cw, ch),
        period: (pw, ph),
        lattice,
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

/// Largest canvas, in pixels, a fractional TilingType 2 lattice is laid
/// out for; a bigger one paints from the period instead (cells at their
/// true size, the step rounded to whole pixels).
pub(crate) const MAX_LATTICE_PIXELS: u64 = 1 << 24;

/// The capture canvas oriented to the device (a signed permutation of
/// its pixels), the cell [`lay_out`] places.
pub(crate) fn orient_cell(content: &Pixmap, lat: &Lattice) -> Option<Pixmap> {
    let mut cell = Pixmap::new(lat.cell.0, lat.cell.1)?;
    let paint = PixmapPaint {
        quality: FilterQuality::Nearest,
        ..PixmapPaint::default()
    };
    cell.draw_pixmap(0, 0, content.as_ref(), &paint, lat.orient, None);
    Some(cell)
}

/// Rows of a cell turned between two looks at the limits.
const TURN_ROWS_PER_CHECK: u32 = 64;

/// [`orient_cell`] for `lat.orient` a signed permutation with a whole
/// translation (what [`lattice_in_gs`] gives), copied pixel by pixel so
/// the limits are looked at every [`TURN_ROWS_PER_CHECK`] rows (#1373,
/// Codex round 1, finding B): the same pixels as `orient_cell`.
fn orient_cell_polled(
    content: &Pixmap,
    lat: &Lattice,
    probe: &crate::limits::LimitProbe,
) -> Result<Option<Pixmap>, PsError> {
    let Some(mut cell) = Pixmap::new(lat.cell.0, lat.cell.1) else {
        return Ok(None);
    };
    let Some(inv) = lat.orient.invert() else {
        return Ok(None);
    };
    let (sw, sh) = (i64::from(content.width()), i64::from(content.height()));
    let src = content.pixels();
    let (w, h) = (lat.cell.0, lat.cell.1);
    let dst = cell.pixels_mut();
    for v in 0..h {
        if v % TURN_ROWS_PER_CHECK == 0 {
            probe.check(0)?;
        }
        for u in 0..w {
            // The source pixel whose centre the pixel centre maps from.
            let (x, y) = (f64::from(u) + 0.5, f64::from(v) + 0.5);
            let sx = (f64::from(inv.sx) * x + f64::from(inv.kx) * y + f64::from(inv.tx)).floor();
            let sy = (f64::from(inv.ky) * x + f64::from(inv.sy) * y + f64::from(inv.ty)).floor();
            let (sx, sy) = (sx as i64, sy as i64);
            if (0..sw).contains(&sx) && (0..sh).contains(&sy) {
                dst[(v * w + u) as usize] = src[(sy * sw + sx) as usize];
            }
        }
    }
    Ok(Some(cell))
}

/// The device positions `origin + floor(i * step)` of the cells along one
/// axis that reach `[0, len)`, each `size` pixels long. Computed from the
/// pattern's origin for every `i`, never by adding steps up, so a band or
/// a larger page places the same cell in the same pixel.
fn lattice_positions(origin: i32, step: f64, size: u32, len: u32) -> Vec<i32> {
    let (o, size, len) = (f64::from(origin), f64::from(size), f64::from(len));
    // origin + i*step lies in (-size - 1, len + 1) for the cells that reach.
    let (a, b) = ((-size - 1.0 - o) / step, (len + 1.0 - o) / step);
    let (lo, hi) = (a.min(b).floor() as i64 - 1, a.max(b).ceil() as i64 + 1);
    let mut at: Vec<i32> = (lo..=hi)
        .filter_map(|i| {
            // The nudge keeps a product that is a whole number in f64 from
            // falling a pixel short.
            let pos = origin as f64 + (i as f64 * step + 1e-6).floor();
            (pos + size > 0.0 && pos < len).then_some(pos as i32)
        })
        .collect();
    // Overlapping cells: the later (right, lower) copy is on top, as in gs.
    at.sort_unstable();
    at
}

/// How many cells [`lattice_positions`] gives along one axis, and how many
/// `i` it looks at to find them, counted without allocating.
fn lattice_count(origin: i32, step: f64, size: u32, len: u32) -> (u64, u64) {
    let (o, sz, l) = (f64::from(origin), f64::from(size), f64::from(len));
    let (a, b) = ((-sz - 1.0 - o) / step, (l + 1.0 - o) / step);
    let (lo, hi) = (a.min(b).floor() as i64 - 1, a.max(b).ceil() as i64 + 1);
    let n = (lo..=hi)
        .filter(|&i| {
            let pos = o + (i as f64 * step + 1e-6).floor();
            pos + sz > 0.0 && pos < l
        })
        .count();
    (n as u64, (hi - lo + 1).max(0) as u64)
}

/// What laying `cell` out over a `w` x `h` canvas takes: the peak bytes
/// held (the canvas, a row of cells, the cell positions, the cell turned
/// to gs's device when `turned`, and for an uncoloured pattern the
/// recoloured cell) and the work: pixels written (the turn and the
/// recolouring included) and positions looked at; and whether any cell
/// reaches the canvas. With none, the layout is the empty canvas: nothing
/// is turned, recoloured or copied (#1373, Codex round 1, finding B).
fn lay_out_cost(
    cell: (u32, u32),
    lat: &Lattice,
    w: u32,
    h: u32,
    recolored: bool,
    turned: bool,
) -> (usize, u64, bool) {
    let (cw, ch) = (u64::from(cell.0), u64::from(cell.1));
    let (nx, ix) = lattice_count(lat.origin.0, lat.step.0, cell.0, w);
    let (ny, iy) = lattice_count(lat.origin.1, lat.step.1, cell.1, h);
    let (w, h) = (u64::from(w), u64::from(h));
    if nx == 0 || ny == 0 {
        let bytes = usize::try_from(4 * w * h).unwrap_or(usize::MAX);
        return (bytes, ix + iy, false);
    }
    let extra = u64::from(recolored) + u64::from(turned);
    let bytes = 4 * (w * h + w * ch + extra * cw * ch + nx + ny);
    let work = extra * cw * ch + nx * cw * ch + ny * w * ch + ix + iy;
    (usize::try_from(bytes).unwrap_or(usize::MAX), work, true)
}

/// The lattice `lat` (in its pattern's context's device coordinates) in
/// gs's device coordinates for that context, which `to_gs` maps to, its
/// `orient` the map from `lat`'s oriented cell to the cell there (#1373). gs lays
/// the cells out at `floor(i * step)` in the device that paints: for a
/// context whose coordinates are flipped or turned against gs's (a
/// capture of a pattern whose axes are not the device's), the steps are
/// floored there, not in pscat's coordinates. `None` when `to_gs` is not a
/// signed permutation with a whole-pixel translation.
fn lattice_in_gs(lat: &Lattice, to_gs: Transform) -> Option<Lattice> {
    let t = pixel_exact(to_gs)?;
    let (sx, ky, kx, sy) = (
        f64::from(t.sx),
        f64::from(t.ky),
        f64::from(t.kx),
        f64::from(t.sy),
    );
    // The device step vectors (step.0, 0) and (0, step.1) under t.
    let along = ky == 0.0;
    let step = if along {
        (sx * lat.step.0, sy * lat.step.1)
    } else {
        (kx * lat.step.1, ky * lat.step.0)
    };
    // Cell 0's pixel rectangle, mapped.
    let (x0, y0) = (f64::from(lat.origin.0), f64::from(lat.origin.1));
    let (x1, y1) = (x0 + f64::from(lat.cell.0), y0 + f64::from(lat.cell.1));
    let map = |x: f64, y: f64| {
        (
            sx * x + kx * y + f64::from(t.tx),
            ky * x + sy * y + f64::from(t.ty),
        )
    };
    let corners = [map(x0, y0), map(x1, y0), map(x0, y1), map(x1, y1)];
    let gx = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let gy = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    if gx.abs() > 1e9 || gy.abs() > 1e9 {
        return None;
    }
    let cell = if along {
        lat.cell
    } else {
        (lat.cell.1, lat.cell.0)
    };
    // Oriented cell pixel (u, v) is context pixel (x0 + u, y0 + v), so the
    // turn is t's linear part with the translation map(x0, y0) - (gx, gy),
    // a whole number between 0 and the cell's size. Worked out in f64:
    // composed in f32, a far origin (1e8 px) cancels it away (#1373, Codex
    // round 1, finding A).
    let (tx, ty) = map(x0, y0);
    let turn = Transform::from_row(t.sx, t.ky, t.kx, t.sy, (tx - gx) as f32, (ty - gy) as f32);
    Some(Lattice {
        step,
        origin: (gx as i32, gy as i32),
        orient: turn,
        cell,
    })
}

/// The box, in gs's device coordinates, of a `w` x `h` canvas whose
/// coordinates `here_to_gs` (a signed permutation, whole translation) maps
/// there: its top-left pixel and size.
fn canvas_in_gs(here_to_gs: Transform, w: u32, h: u32) -> ((i32, i32), (u32, u32)) {
    let mut pts = [
        tiny_skia::Point::from_xy(0.0, 0.0),
        tiny_skia::Point::from_xy(w as f32, h as f32),
    ];
    here_to_gs.map_points(&mut pts);
    let (x, y) = (pts[0].x.min(pts[1].x), pts[0].y.min(pts[1].y));
    let size = if here_to_gs.ky == 0.0 { (w, h) } else { (h, w) };
    ((x.round() as i32, y.round() as i32), size)
}

/// `cell` repeated on the lattice over a `w` x `h` canvas (same device
/// coordinates as the shader's): one row of cells is laid out first, then
/// that row at every row position, so the work follows the canvas's area.
/// The limits are looked at between copies.
pub(crate) fn lay_out(
    cell: &Pixmap,
    lat: &Lattice,
    w: u32,
    h: u32,
    probe: &crate::limits::LimitProbe,
) -> Result<Option<Pixmap>, PsError> {
    let xs = lattice_positions(lat.origin.0, lat.step.0, cell.width(), w);
    let ys = lattice_positions(lat.origin.1, lat.step.1, cell.height(), h);
    let paint = PixmapPaint::default();
    let Some(mut row) = Pixmap::new(w, cell.height()) else {
        return Ok(None);
    };
    for &x in &xs {
        probe.check(0)?;
        row.draw_pixmap(x, 0, cell.as_ref(), &paint, Transform::identity(), None);
    }
    let Some(mut out) = Pixmap::new(w, h) else {
        return Ok(None);
    };
    for &y in &ys {
        probe.check(0)?;
        out.draw_pixmap(0, y, row.as_ref(), &paint, Transform::identity(), None);
    }
    Ok(Some(out))
}

/// The SVG side of a tile: the capture recorder's defs and body, in
/// capture-canvas coordinates.
pub(crate) struct SvgTile {
    /// The defs this capture wrote itself (its clips, gradients and the
    /// `<pattern>`s of the tiles it paints with).
    pub(crate) defs: String,
    /// The captured defs of other tiles this capture paints with, by that
    /// tile's id (#1343): kept apart from `defs` so a recorder that takes
    /// in both this tile and one of those writes the shared chunk once.
    pub(crate) imports: Vec<(usize, String)>,
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
    /// A fractional TilingType 2 tile's cell, oriented to the device.
    cell: Option<Rc<Pixmap>>,
    /// The cells laid out over the last canvas painted on, with its size
    /// and (for an uncoloured pattern) colour.
    laid: RefCell<Option<LaidOut>>,
    /// Present when SVG was being recorded at `makepattern`.
    pub(crate) svg: Option<SvgTile>,
    /// The PDF recorder's pattern index, when PDF was being recorded.
    pub(crate) pdf: Option<usize>,
    /// Its context's coordinates to gs's device coordinates for that
    /// context (identity on the page), for use in another context.
    pub(crate) to_gs: Transform,
}

/// What a layout was made for: the canvas's size, the colour (zero for a
/// coloured pattern), and the painting context's map to gs's device
/// coordinates (rounded: it is a signed permutation with a whole
/// translation).
type LaidKey = (u32, u32, [u8; 3], [i32; 6]);

/// A lattice laid out over a canvas, what it was made for, and its shader
/// transform (the layout's pixels to the canvas's).
type LaidOut = (LaidKey, Rc<Pixmap>, Transform);

/// Where a pattern made in another context paints (#1300): its matrix in
/// the painting context's coordinates, the raster shader, and the PDF
/// `/Matrix` into the painting stream's default space.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    /// Pattern space to the painting context's coordinates.
    pub(crate) matrix: Transform,
    /// The period pixmap's shader transform.
    pub(crate) shader: Transform,
    pub(crate) quality: FilterQuality,
    /// Pattern space to the default space of the stream that paints.
    pub(crate) pdf_matrix: Transform,
}

/// `t` with its translation rounded, when it maps pixels to pixels (a
/// signed permutation with a near-whole translation), so it samples
/// nearest; `None` otherwise.
fn pixel_exact(t: Transform) -> Option<Transform> {
    let unit = |v: f32| v.abs() < 1e-4 || (v.abs() - 1.0).abs() < 1e-4;
    let whole = |v: f32| (v - v.round()).abs() < 1e-3;
    let along = t.ky.abs() < 1e-4 && t.kx.abs() < 1e-4 && t.sx.abs() > 0.5 && t.sy.abs() > 0.5;
    let across = t.sx.abs() < 1e-4 && t.sy.abs() < 1e-4 && t.ky.abs() > 0.5 && t.kx.abs() > 0.5;
    let ok = [t.sx, t.ky, t.kx, t.sy].iter().all(|&v| unit(v))
        && (along || across)
        && whole(t.tx)
        && whole(t.ty);
    ok.then(|| {
        Transform::from_row(
            t.sx.round(),
            t.ky.round(),
            t.kx.round(),
            t.sy.round(),
            t.tx.round(),
            t.ty.round(),
        )
    })
}

/// The placement of `tile` painted in a context whose coordinates map to
/// gs's device coordinates by `here_to_gs`, with `to_default` its
/// coordinates to the painting stream's default space (PDF). `None` when
/// `here_to_gs` is singular.
pub(crate) fn foreign_placement(
    tile: &PatternTile,
    here_to_gs: Transform,
    to_default: Transform,
) -> Option<Placement> {
    // The tile's context to the painting one, through gs's device space.
    let hop = context_hop(tile.to_gs, here_to_gs)?;
    let matrix = hop.pre_concat(tile.geom.matrix);
    let shader = hop.pre_concat(tile.geom.shader);
    let (shader, quality) = match pixel_exact(shader) {
        Some(t) => (t, FilterQuality::Nearest),
        None => (shader, FilterQuality::Bilinear),
    };
    Some(Placement {
        matrix,
        shader,
        quality,
        pdf_matrix: to_default.pre_concat(matrix),
    })
}

/// gs's device coordinates for a capture of `geom` made in a context
/// whose coordinates map to gs's by `parent_to_gs`: the capture canvas to
/// the parent's coordinates (through the tile's shader), to gs's, less
/// the accumulator's origin: the top-left corner of the `/BBox` in gs's
/// device space under the pattern's own matrix (before the whole-pixel
/// adjustment), rounded to the nearest pixel (measured against gs 10.07.1
/// with fractional corners and translations on both sides of a half
/// pixel).
pub(crate) fn capture_to_gs(
    spec: &TileSpec,
    geom: &Geometry,
    parent_to_gs: Transform,
) -> Transform {
    let to_gs = parent_to_gs.pre_concat(if geom.exact { geom.matrix } else { spec.matrix });
    let [x0, y0, x1, y1] = spec.bbox.map(|v| v as f32);
    let mut pts = [
        tiny_skia::Point::from_xy(x0, y0),
        tiny_skia::Point::from_xy(x1, y0),
        tiny_skia::Point::from_xy(x0, y1),
        tiny_skia::Point::from_xy(x1, y1),
    ];
    to_gs.map_points(&mut pts);
    let min = |f: fn(&tiny_skia::Point) -> f32| pts.iter().map(f).fold(f32::INFINITY, f32::min);
    let (dx, dy) = ((min(|p| p.x) + 0.5).floor(), (min(|p| p.y) + 0.5).floor());
    let (dx, dy) = if dx.is_finite() && dy.is_finite() {
        (dx, dy)
    } else {
        (0.0, 0.0)
    };
    Transform::from_translate(-dx, -dy)
        .pre_concat(parent_to_gs)
        .pre_concat(geom.shader)
        .pre_concat(Transform::from_translate(
            geom.origin.0 as f32,
            geom.origin.1 as f32,
        ))
}

impl PatternTile {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: usize,
        context: usize,
        spec: TileSpec,
        geom: Geometry,
        period: Pixmap,
        cell: Option<Pixmap>,
        svg: Option<SvgTile>,
        pdf: Option<usize>,
        to_gs: Transform,
    ) -> Self {
        PatternTile {
            id,
            uncolored: spec.uncolored,
            context,
            spec,
            geom,
            period: Rc::new(period),
            recolored: RefCell::new(None),
            cell: cell.map(Rc::new),
            laid: RefCell::new(None),
            svg,
            pdf,
            to_gs,
        }
    }

    /// The raster shader's source: the period (recoloured to `rgb` for an
    /// uncoloured pattern), its transform, and the sampling. A fractional
    /// TilingType 2 tile paints from its cells laid out over the `canvas`
    /// instead (#1332), in this context or another (#1373), when
    /// [`lay`](Self::lay) has made that layout for the same `here_to_gs`,
    /// and from the period when it has not (a layout a limit or the size
    /// refused, or a context not pixel-aligned with gs's device). `place`,
    /// for a pattern of another context, gives the period's transform and
    /// sampling.
    pub(crate) fn raster(
        &self,
        rgb: (f32, f32, f32),
        place: Option<&Placement>,
        canvas: (u32, u32),
        here_to_gs: Transform,
    ) -> (Rc<Pixmap>, Transform, FilterQuality) {
        if let Some((pm, t)) = self.laid_over(rgb, canvas, here_to_gs) {
            return (pm, t, FilterQuality::Nearest);
        }
        let (shader, quality) = match place {
            Some(p) => (p.shader, p.quality),
            None if self.geom.exact => (self.geom.shader, FilterQuality::Nearest),
            None => (self.geom.shader, FilterQuality::Bilinear),
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
        (pm, shader, quality)
    }
}

impl PatternTile {
    /// The key of a layout over `canvas` in a context whose coordinates
    /// map to gs's by `here` (already [`pixel_exact`]).
    fn laid_key(&self, rgb: (f32, f32, f32), canvas: (u32, u32), here: Transform) -> LaidKey {
        let c = if self.uncolored {
            [to_u8(rgb.0), to_u8(rgb.1), to_u8(rgb.2)]
        } else {
            [0; 3]
        };
        let m = [here.sx, here.ky, here.kx, here.sy, here.tx, here.ty].map(|v| v as i32);
        (canvas.0, canvas.1, c, m)
    }

    /// The layout [`lay`](Self::lay) made for this canvas (and colour) in
    /// a context mapping to gs's device by `here_to_gs`, and its shader.
    fn laid_over(
        &self,
        rgb: (f32, f32, f32),
        canvas: (u32, u32),
        here_to_gs: Transform,
    ) -> Option<(Rc<Pixmap>, Transform)> {
        let here = pixel_exact(here_to_gs)?;
        let key = self.laid_key(rgb, canvas, here);
        match &*self.laid.borrow() {
            Some((k, pm, t)) if *k == key => Some((pm.clone(), *t)),
            _ => None,
        }
    }

    /// Lay a fractional TilingType 2 tile's cells out over `canvas` for the
    /// raster to paint from (#1332), unless that is already done, the tile
    /// has no lattice, or the layout is too big (past
    /// [`MAX_LATTICE_PIXELS`], or the heap headroom the limits leave: then
    /// the tile paints from its period). Its work, in pixels, is added to
    /// `work` for the operator to charge, and a limit (steps, time or
    /// memory while laying out) is an error. A matching layout is reused
    /// before anything is checked; a layout for another canvas, colour or
    /// context is dropped first, so only one is ever held.
    ///
    /// The cells are laid out in gs's device coordinates, where gs floors
    /// the steps (#1373): the canvas's coordinates map there by
    /// `here_to_gs` (identity on the page), the tile's context's by its
    /// [`to_gs`](Self::to_gs), the two being the same space in gs (a
    /// pattern keeps its device matrix in whatever device paints with it).
    /// Either map not a signed permutation with a whole translation (a
    /// context turned off the device axes or scaled): the period instead.
    pub(crate) fn lay(
        &self,
        rgb: (f32, f32, f32),
        canvas: (u32, u32),
        here_to_gs: Transform,
        probe: &crate::limits::LimitProbe,
        work: &mut u64,
    ) -> Result<(), PsError> {
        let (Some(cell), Some(lat)) = (self.cell.as_ref(), self.geom.lattice.as_ref()) else {
            return Ok(());
        };
        if self.laid_over(rgb, canvas, here_to_gs).is_some() {
            return Ok(());
        }
        *self.laid.borrow_mut() = None;
        let (Some(here), Some(gs_lat)) = (pixel_exact(here_to_gs), lattice_in_gs(lat, self.to_gs))
        else {
            return Ok(());
        };
        if u64::from(canvas.0) * u64::from(canvas.1) > MAX_LATTICE_PIXELS {
            return Ok(());
        }
        let Some(from_gs) = here.invert() else {
            return Ok(());
        };
        let key = self.laid_key(rgb, canvas, here);
        // The canvas in gs's device coordinates, and the lattice relative
        // to its top-left pixel.
        let ((gx, gy), (w, h)) = canvas_in_gs(here, canvas.0, canvas.1);
        let ox = i64::from(gs_lat.origin.0) - i64::from(gx);
        let oy = i64::from(gs_lat.origin.1) - i64::from(gy);
        let (Ok(ox), Ok(oy)) = (i32::try_from(ox), i32::try_from(oy)) else {
            return Ok(());
        };
        let lat = Lattice {
            origin: (ox, oy),
            ..gs_lat
        };
        let turned = !gs_lat.orient.is_identity();
        let (bytes, cost, any) = lay_out_cost(lat.cell, &lat, w, h, self.uncolored, turned);
        if probe.check_alloc(bytes).is_err() {
            return Ok(());
        }
        // The whole layout's work, the turn included, before any of it.
        probe.check(cost / 1024)?;
        let turned_cell;
        let cell: &Pixmap = if !any {
            cell
        } else if turned {
            match orient_cell_polled(cell, &gs_lat, probe)? {
                Some(c) => {
                    turned_cell = c;
                    &turned_cell
                }
                None => return Ok(()),
            }
        } else {
            cell
        };
        let laid = if !any {
            Pixmap::new(w, h)
        } else if self.uncolored {
            let tinted = recolor(cell, [key.2[0], key.2[1], key.2[2]]);
            lay_out(&tinted, &lat, w, h, probe)?
        } else {
            lay_out(cell, &lat, w, h, probe)?
        };
        if let Some(pm) = laid {
            let shader = from_gs.pre_concat(Transform::from_translate(gx as f32, gy as f32));
            *self.laid.borrow_mut() = Some((key, Rc::new(pm), shader));
            *work = work.saturating_add(cost);
        }
        Ok(())
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
    /// The capture canvas to gs's device coordinates for it, its pattern
    /// accumulator's (#1300; [`capture_to_gs`]).
    pub(crate) to_gs: Transform,
    /// Where `makepattern` ran: the tile belongs to that context, not to
    /// the one it is first used in (#1341).
    pub(crate) origin: Origin,
}

/// The context a pattern was made in (#1341), recorded at `makepattern`
/// because the `PaintProc` runs later, possibly in another context (the
/// page's pattern first used inside a `PaintProc`, or the other way
/// round).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Origin {
    /// The capture it was made in, 0 for the page.
    pub(crate) context: usize,
    /// That context's coordinates to gs's device coordinates for it.
    pub(crate) to_gs: Transform,
    /// That context's coordinates to the default space of the PDF stream
    /// painting there.
    pub(crate) to_default: Transform,
}

/// A pattern `makepattern` made, whose `PaintProc` has run or has not yet
/// (#1341). One per pattern dictionary, shared by every copy of the
/// graphics state that holds it as its colour, so the `PaintProc` runs
/// once however many times, and in however many `gsave` levels, it is
/// used. A `PaintProc` that fails leaves it unrun, so the next use runs
/// it again (gs does the same).
pub(crate) struct LazyPattern {
    /// Reserved at `makepattern`, so the SVG names of a tile do not depend
    /// on when it is first used.
    pub(crate) id: usize,
    pub(crate) spec: TileSpec,
    pub(crate) proc_: crate::object::Object,
    pub(crate) origin: Origin,
    /// The graphics state `makepattern` ran in, which the `PaintProc`
    /// starts from (gs: its line width and colour, not the state at first
    /// use). Dropped once the tile is made.
    state: RefCell<Option<Box<crate::gfx::GraphicsState>>>,
    tile: RefCell<Option<Rc<PatternTile>>>,
}

impl LazyPattern {
    pub(crate) fn new(
        id: usize,
        spec: TileSpec,
        proc_: crate::object::Object,
        origin: Origin,
        state: Box<crate::gfx::GraphicsState>,
    ) -> Self {
        LazyPattern {
            id,
            spec,
            proc_,
            origin,
            state: RefCell::new(Some(state)),
            tile: RefCell::new(None),
        }
    }

    /// The tile, once the `PaintProc` has run.
    pub(crate) fn tile(&self) -> Option<Rc<PatternTile>> {
        self.tile.borrow().clone()
    }

    pub(crate) fn set_tile(&self, tile: Rc<PatternTile>) {
        *self.tile.borrow_mut() = Some(tile);
        *self.state.borrow_mut() = None;
    }

    /// A copy of the state to run the `PaintProc` from.
    pub(crate) fn run_state(&self) -> Option<Box<crate::gfx::GraphicsState>> {
        self.state.borrow().clone()
    }
}

impl Drop for LazyPattern {
    /// Patterns made one after another, each with the last as its current
    /// colour, hold each other through their saved states: free the chain
    /// in a loop, not by recursion.
    fn drop(&mut self) {
        let mut work: Vec<Box<crate::gfx::GraphicsState>> =
            self.state.get_mut().take().into_iter().collect();
        while let Some(mut st) = work.pop() {
            if let Some(p) = st.take_pattern()
                && let PatternRef::Tile(lazy) = p.pattern
                && let Ok(mut inner) = Rc::try_unwrap(lazy)
            {
                work.extend(inner.state.get_mut().take());
            }
        }
    }
}

/// What a fill or stroke paints with, as the SVG and PDF recorders take
/// it.
#[derive(Clone)]
pub(crate) enum Ink {
    Rgb((f32, f32, f32)),
    /// A tiling pattern, the colour an uncoloured one paints in, and,
    /// for a pattern made in another context, where it paints (#1300).
    Pattern(Rc<PatternTile>, (f32, f32, f32), Option<Placement>),
    /// A shading pattern (#1298) and, for one made in another context,
    /// where it paints.
    Shading(Rc<ShadingPattern>, Option<ShadingPlace>),
}

impl Ink {
    /// A pattern of either kind: what an imagemask paints through a
    /// stencil rather than in a colour (#1299).
    pub(crate) fn is_pattern(&self) -> bool {
        !matches!(self, Ink::Rgb(_))
    }
}

/// A pattern `makepattern` made, as the registry and the graphics state
/// hold it: a tiling pattern (#1038) or a shading pattern (#1298).
#[derive(Clone)]
pub(crate) enum PatternRef {
    Tile(Rc<LazyPattern>),
    Shading(Rc<ShadingPattern>),
}

impl PatternRef {
    /// PaintType 2. A shading pattern is always coloured.
    pub(crate) fn uncolored(&self) -> bool {
        match self {
            PatternRef::Tile(t) => t.spec.uncolored,
            PatternRef::Shading(_) => false,
        }
    }
}

/// A shading pattern (#1298): `makepattern`'s PatternType 2. Its shading
/// is painted in pattern space, which `matrix` (the dictionary's matrix
/// times the CTM at `makepattern`) maps to the device, wherever a fill,
/// stroke, show or imagemask paints with it; `/Background` is painted
/// where the shading does not reach, inside its `/BBox`. Nothing is run
/// and nothing is captured, so it holds the shading itself:
///
/// - **PNG**: [`crate::gfx::shading_layer`] paints the shading into a layer
///   over the pixels each mark covers, which the mark uses as its shader.
/// - **SVG**: the gradient itself, or a `<pattern>` holding what `shfill`
///   would write ([`SvgShading`]).
/// - **PDF**: a `/PatternType 2` pattern on the shading.
pub(crate) struct ShadingPattern {
    /// The context it was made in (`PatternTile::context`).
    pub(crate) context: usize,
    pub(crate) shading: crate::shading::ParsedShading,
    pub(crate) background: Option<(f32, f32, f32)>,
    /// Pattern space to the context's device space.
    pub(crate) matrix: Transform,
    /// The context's coordinates to gs's device coordinates (#1300).
    pub(crate) to_gs: Transform,
    /// Present when SVG was being recorded at `makepattern`.
    pub(crate) svg: Option<SvgShading>,
    /// The SVG images made for uses in other contexts (#1368), by
    /// [`SvgUseKey`]. `None` is a use the shading does not reach. Held here
    /// so the mark that needs one makes it under its limits
    /// ([`Gfx::pattern_source`](crate::gfx::Gfx::pattern_source)) and the
    /// recorder only reads it; the bytes are the process's heap, which the
    /// memory limit counts.
    pub(crate) svg_uses: RefCell<HashMap<SvgUseKey, Option<Rc<SvgImage>>>>,
    /// The PDF recorder's pattern index, when PDF was being recorded and
    /// the shading paints anything.
    pub(crate) pdf: Option<usize>,
    /// The PDF shading's own space to the context's device space: `matrix`
    /// with the small-geometry rescale (`vector_shading`) folded in.
    pub(crate) pdf_ctm: Transform,
    /// The `/Matrix` the PDF pattern was written with: `pdf_ctm` into the
    /// default space of the stream `makepattern` ran in.
    pub(crate) pdf_matrix: Transform,
}

/// A shading pattern's SVG form, in its context's device coordinates.
pub(crate) struct SvgShading {
    /// A gradient (types 2 and 3): geometry, shading space to device, the
    /// stops and `/Extend`; `None` for a Type 1 shading and for one that
    /// paints nothing.
    pub(crate) gradient: Option<SvgGradient>,
    /// A Type 1 or mesh shading's layer, over the canvas it was made on.
    pub(crate) image: Option<SvgImage>,
    /// The canvas `image` was cut to: a mark on another canvas, or in
    /// another context, needs an image of its own (`svg_uses`).
    pub(crate) canvas: (u32, u32),
    /// Where a Type 1 or mesh shading paints: its box in device space
    /// `[x0, y0, x1, y1]`, cut to the `/BBox`, before any canvas cuts it.
    /// `None` for a gradient (which SVG draws itself) and a shading that
    /// paints nothing.
    pub(crate) shape: Option<[f64; 4]>,
    /// The `/BBox` corners in device space.
    pub(crate) bbox: Option<[(f32, f32); 4]>,
    pub(crate) background: Option<(f32, f32, f32)>,
}

/// A Type 1 or mesh shading's layer as SVG embeds it: the PNG, its size and
/// its top-left pixel, in the device space of the canvas it was made for.
pub(crate) type SvgImage = (Vec<u8>, u32, u32, i32, i32);

/// Which image a use in another context needs (#1368): the bits of the map
/// from the shading's context to the painting one, and the painting
/// canvas's size.
pub(crate) type SvgUseKey = ([u32; 6], u32, u32);

impl ShadingPattern {
    /// The key of the SVG image a mark on `canvas`, with the context map
    /// `hop` (`None` for the context the pattern was made in), needs, when
    /// the image made at `makepattern` is not it: the use is in another
    /// context or on a canvas of another size, and the shading has an image
    /// to make. Computed here for both the mark that makes the image and
    /// the recorder that writes it, so they cannot disagree.
    pub(crate) fn svg_use_key(
        &self,
        hop: Option<Transform>,
        canvas: (u32, u32),
    ) -> Option<SvgUseKey> {
        let svg = self.svg.as_ref()?;
        svg.shape?;
        let hop = hop.unwrap_or_default();
        if hop.is_identity() && canvas == svg.canvas {
            return None;
        }
        let bits = [hop.sx, hop.ky, hop.kx, hop.sy, hop.tx, hop.ty].map(f32::to_bits);
        Some((bits, canvas.0, canvas.1))
    }
}

/// An axial or radial shading as SVG writes it: geometry, its own space to
/// device, the stops and `/Extend`.
pub(crate) type SvgGradient = (
    crate::svg::ShKind,
    Transform,
    Vec<(f32, f32, f32, f32)>,
    [bool; 2],
);

/// Where a shading pattern made in another context paints (#1300's rule).
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShadingPlace {
    /// Its context's coordinates to the painting context's.
    pub(crate) hop: Transform,
    /// The PDF `/Matrix`: the shading's space to the default space of the
    /// stream that paints.
    pub(crate) pdf_matrix: Transform,
}

/// The map from the coordinates of a context whose own map to gs's device
/// coordinates is `there_to_gs` to those of one whose map is `here_to_gs`
/// (#1300). `None` when `here_to_gs` is singular.
pub(crate) fn context_hop(there_to_gs: Transform, here_to_gs: Transform) -> Option<Transform> {
    Some(here_to_gs.invert()?.pre_concat(there_to_gs))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 37 x 150 canvas with every pixel distinct, and a lattice of its
    /// cell turned by `orient` (the eight signed permutations).
    fn turn_case(orient: [f32; 4]) -> (Pixmap, Lattice) {
        let (w, h) = (37u32, 150u32);
        let mut pm = Pixmap::new(w, h).unwrap();
        for (i, p) in pm.pixels_mut().iter_mut().enumerate() {
            let v = (i % 251) as u8;
            *p = PremultipliedColorU8::from_rgba(v / 2, v / 3, v / 4, v.max(1)).unwrap();
        }
        let [a, b, c, d] = orient;
        let along = b == 0.0;
        let cell = if along { (w, h) } else { (h, w) };
        // The translation that puts the turned canvas at (0, 0).
        let tx = -(a.min(0.0) * w as f32 + c.min(0.0) * h as f32);
        let ty = -(b.min(0.0) * w as f32 + d.min(0.0) * h as f32);
        let lat = Lattice {
            step: (1.0, 1.0),
            origin: (0, 0),
            orient: Transform::from_row(a, b, c, d, tx, ty),
            cell,
        };
        (pm, lat)
    }

    const TURNS: [[f32; 4]; 8] = [
        [1.0, 0.0, 0.0, 1.0],
        [-1.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, -1.0],
        [-1.0, 0.0, 0.0, -1.0],
        [0.0, 1.0, 1.0, 0.0],
        [0.0, -1.0, 1.0, 0.0],
        [0.0, 1.0, -1.0, 0.0],
        [0.0, -1.0, -1.0, 0.0],
    ];

    /// The polled turn gives the pixels the nearest-sampled draw gives, and
    /// looks at the limits as it goes (#1373, Codex round 1 on PR #1480,
    /// finding B).
    #[test]
    fn the_polled_turn_matches_the_draw_and_looks_at_the_limits() {
        for o in TURNS {
            let (pm, lat) = turn_case(o);
            let drawn = orient_cell(&pm, &lat).unwrap();
            crate::limits::CHECKS.with(|c| c.set(0));
            let polled = orient_cell_polled(&pm, &lat, &crate::limits::LimitProbe::NONE)
                .unwrap()
                .unwrap();
            assert!(polled.data() == drawn.data(), "{o:?}");
            let rows = u64::from(lat.cell.1).div_ceil(u64::from(TURN_ROWS_PER_CHECK));
            assert_eq!(crate::limits::CHECKS.with(|c| c.get()), rows, "{o:?}");
        }
        let (pm, lat) = turn_case(TURNS[5]);
        let past = crate::limits::LimitProbe {
            deadline_ns: Some(0),
            ..crate::limits::LimitProbe::NONE
        };
        assert_eq!(
            orient_cell_polled(&pm, &lat, &past).err(),
            Some(PsError::TimeExceeded)
        );
    }

    /// No cell reaching the canvas: no turn, no recolouring, no copies, so
    /// none of their work or bytes; one reaching: the turn and the
    /// recolouring are charged with the copies (finding B).
    #[test]
    fn a_layout_charges_its_turn_only_when_a_cell_reaches() {
        let lat = |origin| Lattice {
            step: (4000.3, 4000.3),
            origin,
            orient: Transform::identity(),
            cell: (2000, 2000),
        };
        let (bytes, work, any) =
            lay_out_cost((2000, 2000), &lat((1000, -2900)), 100, 100, true, true);
        assert!(!any);
        assert_eq!(bytes, 4 * 100 * 100);
        assert!(work < 16, "{work}");
        let (_, plain, any) = lay_out_cost((2000, 2000), &lat((0, 0)), 100, 100, false, false);
        assert!(any);
        let (_, both, _) = lay_out_cost((2000, 2000), &lat((0, 0)), 100, 100, true, true);
        assert_eq!(both - plain, 2 * 2000 * 2000);
    }

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
        // gs puts the /BBox top (500.6 - 7.3 = 493.3) on row 493, so the
        // pattern origin sits at row 500, not at round(500.6) = 501 (gs
        // 10.07.1, this matrix on a 520 pt page: cell tops 486 493 500).
        assert_eq!(
            (g.shader.sx, g.shader.sy, g.shader.tx, g.shader.ty),
            (1.0, -1.0, 10.0, 500.0)
        );
        // The adjusted matrix takes a step to exactly 7 px.
        let mut p = [tiny_skia::Point::from_xy(7.3, 0.0)];
        g.matrix.map_points(&mut p);
        assert!((p[0].x - 17.0).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn a_rotated_pattern_is_resampled() {
        // 30 degrees.
        let m = Transform::from_row(0.866_025_4, 0.5, -0.5, 0.866_025_4, 0.0, 0.0);
        let g = geometry(&spec(m, [0.0, 0.0, 10.0, 10.0], 10.0, 10.0)).unwrap();
        assert!(!g.exact);
        assert_eq!(g.period, (10, 10));
    }

    #[test]
    fn a_capture_maps_to_gs_from_its_rounded_bbox_corner() {
        // Outer pattern at scale 2, translated (4, 7), on a 205 px page
        // (y flipped); /BBox corner x = -13.3 -> -22.6 in device, y top
        // 27.2 -> 198 - 54.4 = 143.6. gs anchors at (-23, 144).
        let m = Transform::from_row(2.0, 0.0, 0.0, -2.0, 4.0, 198.0);
        let sp = spec(m, [-13.3, -21.0, 30.0, 27.2], 400.0, 400.0);
        let g = geometry(&sp).unwrap();
        let to_gs = capture_to_gs(&sp, &g, Transform::identity());
        // Pattern space through the capture CTM, then to gs: the pattern
        // origin lands at its device point less the anchor, after the move
        // that rounds the corner (-0.4, +0.4, #1342's rule; the nested
        // sweeps in its note hold that rule to gs's raster).
        let mut p = [tiny_skia::Point::from_xy(0.0, 0.0)];
        g.capture.map_points(&mut p);
        to_gs.map_points(&mut p);
        assert!(
            (p[0].x - 26.6).abs() < 1e-3 && (p[0].y - 54.4).abs() < 1e-3,
            "{p:?}"
        );
    }

    #[test]
    fn only_pixel_to_pixel_maps_sample_nearest() {
        let flip = Transform::from_row(1.0, 0.0, 0.0, -1.0, 3.0002, 7.0);
        assert_eq!(
            pixel_exact(flip),
            Some(Transform::from_row(1.0, 0.0, 0.0, -1.0, 3.0, 7.0))
        );
        let swap = Transform::from_row(0.0, 1.0, -1.0, 0.0, 2.0, 5.0);
        assert!(pixel_exact(swap).is_some());
        assert!(pixel_exact(Transform::from_row(1.0, 0.0, 0.0, 1.0, 0.5, 0.0)).is_none());
        assert!(pixel_exact(Transform::from_scale(1.5, 1.0)).is_none());
        assert!(pixel_exact(Transform::from_row(0.866, 0.5, -0.5, 0.866, 0.0, 0.0)).is_none());
    }

    fn spec2(matrix: Transform, w: f64, step: f64) -> TileSpec {
        TileSpec {
            tiling_type: 2,
            ..spec(matrix, [0.0, 0.0, w, w], step, step)
        }
    }

    #[test]
    fn type_2_keeps_the_cell_and_lays_out_the_steps() {
        // 72 dpi page, y flipped, a 7.3 pt step and a 3.65 pt cell.
        let m = Transform::from_row(1.0, 0.0, 0.0, -1.0, 0.0, 100.0);
        let g = geometry(&spec2(m, 3.65, 7.3)).unwrap();
        // The capture is at the true scale, the matrix unadjusted.
        assert_eq!((g.capture.sx, g.capture.sy), (1.0, 1.0));
        assert_eq!((g.matrix.sx, g.matrix.sy), (1.0, -1.0));
        assert_eq!(g.content, (4, 4));
        let lat = g.lattice.expect("a lattice");
        assert_eq!(lat.step, (7.3, -7.3));
        // The y flip puts the cell's top row 4 px above the anchor.
        assert_eq!(lat.origin, (0, 96));
        // Type 1 on the same step scales the cell to 7 px, no lattice.
        let one = geometry(&spec(m, [0.0, 0.0, 3.65, 3.65], 7.3, 7.3)).unwrap();
        assert!(one.lattice.is_none());
        assert!((one.capture.sx - 7.0 / 7.3).abs() < 1e-6);
        // A whole-pixel step needs no lattice.
        assert!(geometry(&spec2(m, 3.0, 8.0)).unwrap().lattice.is_none());
    }

    #[test]
    fn lattice_positions_are_floors_from_the_origin() {
        // gs 10.07.1 at 7.3 px: starts 0 7 14 21 29 36 43 51 58 65 73.
        let xs = lattice_positions(0, 7.3, 4, 80);
        assert_eq!(&xs[..11], &[0, 7, 14, 21, 29, 36, 43, 51, 58, 65, 73]);
        // A negative step floors toward minus infinity: 0 -8 -15 -22 -30
        // (the first five from the origin back).
        assert_eq!(
            lattice_positions(100, -7.3, 4, 101)
                .iter()
                .rev()
                .take(5)
                .copied()
                .collect::<Vec<_>>(),
            [100, 92, 85, 78, 70]
        );
        // A cell that starts left of the canvas and still reaches it.
        assert_eq!(lattice_positions(-2, 7.3, 4, 8), vec![-2, 5]);
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
