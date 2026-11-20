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
//! distorted by up to a pixel for exactly this; gs adjusts the same way).
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
    /// Device position of cell 0's top-left pixel (the anchor, rounded
    /// to a pixel, plus the cell's offset from it).
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

/// `cell` repeated on the lattice over a `w` x `h` canvas (same device
/// coordinates as the shader's): one row of cells is laid out first, then
/// that row at every row position, so the work follows the canvas's area.
pub(crate) fn lay_out(cell: &Pixmap, lat: &Lattice, w: u32, h: u32) -> Option<Pixmap> {
    if u64::from(w) * u64::from(h) > MAX_LATTICE_PIXELS {
        return None;
    }
    let xs = lattice_positions(lat.origin.0, lat.step.0, cell.width(), w);
    let ys = lattice_positions(lat.origin.1, lat.step.1, cell.height(), h);
    let paint = PixmapPaint::default();
    let mut row = Pixmap::new(w, cell.height())?;
    for &x in &xs {
        row.draw_pixmap(x, 0, cell.as_ref(), &paint, Transform::identity(), None);
    }
    let mut out = Pixmap::new(w, h)?;
    for &y in &ys {
        out.draw_pixmap(0, y, row.as_ref(), &paint, Transform::identity(), None);
    }
    Some(out)
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

/// A lattice laid out over a canvas of `.0` x `.1` pixels, in the colour
/// `.2` (zero for a coloured pattern).
type LaidOut = ((u32, u32, [u8; 3]), Rc<Pixmap>);

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
    let hop = here_to_gs.invert()?.pre_concat(tile.to_gs);
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
    let to_gs = parent_to_gs.pre_concat(spec.matrix);
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
    /// TilingType 2 tile paints from its cells laid out over the
    /// `canvas` instead (#1332); `None` there paints from the period (the
    /// layout would not fit `--max-memory`). `place`, for a pattern of
    /// another context, gives the transform and sampling instead.
    pub(crate) fn raster(
        &self,
        rgb: (f32, f32, f32),
        place: Option<&Placement>,
        canvas: Option<(u32, u32)>,
    ) -> (Rc<Pixmap>, Transform, FilterQuality) {
        if place.is_none()
            && let Some(canvas) = canvas
            && let Some(pm) = self.lattice_over(rgb, canvas)
        {
            return (pm, Transform::identity(), FilterQuality::Nearest);
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
    /// The cells of a fractional TilingType 2 tile on a `canvas`, or
    /// `None` for any other tile and for a canvas too big to lay out.
    fn lattice_over(&self, rgb: (f32, f32, f32), canvas: (u32, u32)) -> Option<Rc<Pixmap>> {
        let (cell, lat) = (self.cell.as_ref()?, self.geom.lattice.as_ref()?);
        let key = if self.uncolored {
            [to_u8(rgb.0), to_u8(rgb.1), to_u8(rgb.2)]
        } else {
            [0; 3]
        };
        let key = (canvas.0, canvas.1, key);
        if let Some((k, pm)) = &*self.laid.borrow()
            && *k == key
        {
            return Some(pm.clone());
        }
        let pm = if self.uncolored {
            lay_out(
                &recolor(cell, [key.2[0], key.2[1], key.2[2]]),
                lat,
                canvas.0,
                canvas.1,
            )?
        } else {
            lay_out(cell, lat, canvas.0, canvas.1)?
        };
        let pm = Rc::new(pm);
        *self.laid.borrow_mut() = Some((key, pm.clone()));
        Some(pm)
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
                && let Ok(mut inner) = Rc::try_unwrap(p.lazy)
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
        // origin lands at its device point less the anchor.
        let mut p = [tiny_skia::Point::from_xy(0.0, 0.0)];
        g.capture.map_points(&mut p);
        to_gs.map_points(&mut p);
        assert!(
            (p[0].x - 27.0).abs() < 1e-3 && (p[0].y - 54.0).abs() < 1e-3,
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
