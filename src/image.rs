//! The sampled-image engine behind `image`, `imagemask`, and
//! `colorimage` (Stage 7 task 4).
//!
//! Like the show family, an image is an execution-stack frame: data
//! arrives incrementally (a file read, a string, or — the Level 1
//! classic — a procedure called repeatedly for more scanlines, which
//! the machine runs as an ordinary frame above this one). Once the
//! buffer is complete, rasterization is one pass: every device pixel
//! inside the image's unit-square footprint is inverse-mapped through
//! CTM⁻¹ and the ImageMatrix to a source sample (nearest neighbor —
//! found files at found sizes don't miss the interpolation).

use crate::error::PsError;
use crate::file::FileHandle;
use crate::gfx::{Gfx, IndexedTable};
use crate::object::Object;

/// How decoded samples become colors. `Direct` is the ncomp-driven
/// gray/RGB/CMYK interpretation; the other two come from the current
/// color space when the dict form starts.
pub(crate) enum ImageColor {
    Direct,
    /// Samples are indices into an Indexed lookup table.
    Indexed(std::rc::Rc<IndexedTable>),
    /// Samples are Separation tints, rendered as `1 - tint` gray —
    /// a documented approximation (running the tint transform per
    /// pixel through the machine is not feasible; most separations
    /// are dark inks, for which this is close).
    SeparationApprox,
}

pub(crate) enum ImageSource {
    /// A procedure returning strings; empty string = end of data.
    Proc(Object),
    File(FileHandle),
    /// A string source (or already-complete data).
    Bytes(Vec<u8>),
}

/// What the machine should do after a step.
pub(crate) enum ImageStep {
    /// Run the data-source procedure; feed me its result next step.
    Exec(Object),
    /// Rasterized (or abandoned at EOD); pop the frame.
    Done,
}

/// Upper bounds on the stencil composite's expansion (#739): at most 64x
/// per axis and 16 Mpixel in all, so a tiny image under a huge mask cannot
/// blow up memory; past that the stencil is sampled at the image's cells.
const MAX_STENCIL_SCALE: f64 = 64.0;
const MAX_COMPOSITE_PIXELS: usize = 16 << 20;

/// The integer expansion actually used: `want`, lowered until the raster
/// fits `MAX_COMPOSITE_PIXELS`. The cap limits only the expansion: at 1 the
/// image is always emitted, as a plain image of that size would be.
fn expansion(width: usize, height: usize, want: usize) -> usize {
    let cells = |k: usize| {
        width
            .saturating_mul(k)
            .saturating_mul(height.saturating_mul(k))
    };
    let mut k = want.max(1);
    while k > 1 && cells(k) > MAX_COMPOSITE_PIXELS {
        k -= 1;
    }
    k
}

/// A stenciled image flattened to RGBA at `k`x the image's resolution.
struct Composite {
    w: usize,
    h: usize,
    k: usize,
    rgba: Vec<u8>,
}

impl Composite {
    /// Device transform for the composite's pixel coordinates, given the
    /// image-sample-to-device transform `t`.
    fn transform(&self, t: tiny_skia::Transform) -> [f32; 6] {
        let k = self.k as f32;
        [t.sx / k, t.ky / k, t.kx / k, t.sy / k, t.tx, t.ty]
    }
}

pub(crate) struct ImageCtx {
    pub width: usize,
    pub height: usize,
    pub bits: u32,
    pub ncomp: u32,
    /// Per-component [min, max] sample mapping, `2*ncomp` entries.
    pub decode: Vec<f32>,
    /// User-space unit square → image space (the ImageMatrix).
    pub matrix: [f64; 6],
    /// Some(paint_ones): imagemask stenciling the current color.
    pub mask: Option<bool>,
    pub color: ImageColor,
    pub source: ImageSource,
    pub data: Vec<u8>,
    /// `ImageType 3` (#739): the 1-bit stencil that gates this image's
    /// pixels, with its own source, resolution and ImageMatrix. Fed after
    /// this image's own data (InterleaveType 3: separate sources).
    pub stencil: Option<Box<ImageCtx>>,
    waiting: bool,
    eod: bool,
}

impl ImageCtx {
    // One argument per image parameter; a builder would be ceremony.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        width: usize,
        height: usize,
        bits: u32,
        ncomp: u32,
        decode: Vec<f32>,
        matrix: [f64; 6],
        mask: Option<bool>,
        color: ImageColor,
        source: ImageSource,
    ) -> Self {
        ImageCtx {
            width,
            height,
            bits,
            ncomp,
            decode,
            matrix,
            mask,
            color,
            source,
            data: Vec::new(),
            stencil: None,
            waiting: false,
            eod: false,
        }
    }

    pub(crate) fn with_stencil(mut self, stencil: ImageCtx) -> Self {
        self.stencil = Some(Box::new(stencil));
        self
    }

    fn row_bytes(&self) -> usize {
        (self.width * self.ncomp as usize * self.bits as usize).div_ceil(8)
    }

    fn needed(&self) -> usize {
        self.row_bytes() * self.height
    }

    pub(crate) fn waiting(&self) -> bool {
        self.waiting || self.stencil.as_ref().is_some_and(|s| s.waiting())
    }

    /// The data-source procedure's result, handed over by the machine
    /// (to whichever of the image and its stencil asked for it).
    pub(crate) fn supply(&mut self, obj: Object) -> Result<(), PsError> {
        if !self.waiting
            && let Some(st) = &mut self.stencil
        {
            return st.supply(obj);
        }
        self.waiting = false;
        match &obj.value {
            crate::object::Value::String(s) => {
                if s.is_empty() {
                    self.eod = true;
                } else {
                    self.data.extend(s.to_vec());
                }
                Ok(())
            }
            _ => Err(PsError::Typecheck),
        }
    }

    pub(crate) fn step(&mut self, gfx: &mut Gfx) -> Result<ImageStep, PsError> {
        // gs reads the mask's source first, then the image's (probed with
        // two data procedures, #739), so a shared file or one procedure
        // counter sees the same order.
        if let Some(st) = &mut self.stencil {
            if let Some(exec) = st.fill()? {
                return Ok(exec);
            }
            // Finish the mask's filter chain before the image starts
            // reading, or the image would take the mask's EOD marker for
            // data (`currentfile /ASCIIHexDecode filter` ... `80>` then
            // the image bytes). Not when both planes read the very same
            // file object: they then share its position.
            let shared = matches!(
                (&st.source, &self.source),
                (ImageSource::File(a), ImageSource::File(b)) if std::rc::Rc::ptr_eq(a, b)
            );
            if !shared {
                st.drain_filters()?;
            }
        }
        if let Some(exec) = self.fill()? {
            return Ok(exec);
        }
        self.drain_filters()?;
        if let Some(st) = &mut self.stencil {
            st.drain_filters()?;
        }
        // Premature EOD renders what arrived; missing samples read as 0.
        self.rasterize(gfx);
        Ok(ImageStep::Done)
    }

    /// Read this plane's own source until it has all its samples (or
    /// hits EOD). `Some(Exec)` means a data procedure must run first.
    fn fill(&mut self) -> Result<Option<ImageStep>, PsError> {
        while self.data.len() < self.needed() && !self.eod {
            match &self.source {
                ImageSource::Bytes(_) => {
                    let ImageSource::Bytes(b) =
                        std::mem::replace(&mut self.source, ImageSource::Bytes(Vec::new()))
                    else {
                        unreachable!("matched above");
                    };
                    self.data.extend(b);
                    self.eod = true;
                }
                ImageSource::File(f) => {
                    let want = self.needed() - self.data.len();
                    let mut f = f.borrow_mut();
                    for _ in 0..want {
                        match f.read_byte()? {
                            Some(b) => self.data.push(b),
                            None => {
                                self.eod = true;
                                break;
                            }
                        }
                    }
                }
                ImageSource::Proc(p) => {
                    self.waiting = true;
                    return Ok(Some(ImageStep::Exec(p.clone())));
                }
            }
        }
        Ok(None)
    }

    /// A filter source is drained to its own EOD marker — every
    /// layer of the chain — so the underlying file (usually the
    /// program) resumes right after the encoded data. gs-pinned
    /// inline-image behavior.
    fn drain_filters(&mut self) -> Result<(), PsError> {
        if let ImageSource::File(f) = &self.source {
            let mut layer = f.clone();
            while layer.borrow().is_filter() {
                while layer.borrow_mut().read_byte()?.is_some() {}
                let Some(next) = layer.borrow().filter_source() else {
                    break;
                };
                layer = next;
            }
        }
        Ok(())
    }

    /// The sample cell of this plane that user-space point (ux, uy) falls
    /// in (nearest neighbour: the cell containing the point), or None
    /// outside the source rectangle. The one place a point becomes a
    /// sample position, so a smoother filter (#730 /Interpolate) has a
    /// single seam to replace.
    fn cell_at(&self, ux: f64, uy: f64) -> Option<(usize, usize)> {
        let m = self.matrix;
        let ix = (m[0] * ux + m[2] * uy + m[4]).floor();
        let iy = (m[1] * ux + m[3] * uy + m[5]).floor();
        if ix < 0.0 || iy < 0.0 {
            return None;
        }
        let (ix, iy) = (ix as usize, iy as usize);
        (ix < self.width && iy < self.height).then_some((ix, iy))
    }

    /// Whether this stencil plane (an imagemask-style 1-bit plane) lets
    /// the point through: its cell exists and carries the paint polarity.
    fn stencil_passes(&self, ux: f64, uy: f64) -> bool {
        let paint_ones = self.mask.unwrap_or(true);
        self.cell_at(ux, uy)
            .is_some_and(|(ix, iy)| (self.sample(ix, iy, 0) != 0) == paint_ones)
    }

    /// One sample component, MSB-first within each byte, rows padded to
    /// byte boundaries.
    fn sample(&self, ix: usize, iy: usize, comp: usize) -> u32 {
        let bitpos =
            iy * self.row_bytes() * 8 + (ix * self.ncomp as usize + comp) * self.bits as usize;
        let mut v: u32 = 0;
        for i in 0..self.bits as usize {
            let bit = bitpos + i;
            let byte = self.data.get(bit / 8).copied().unwrap_or(0);
            v = (v << 1) | u32::from((byte >> (7 - bit % 8)) & 1);
        }
        v
    }

    fn decoded(&self, ix: usize, iy: usize, comp: usize) -> f32 {
        let maxval = ((1u32 << self.bits) - 1) as f32;
        let v = self.sample(ix, iy, comp) as f32 / maxval;
        let d0 = self.decode.get(comp * 2).copied().unwrap_or(0.0);
        let d1 = self.decode.get(comp * 2 + 1).copied().unwrap_or(1.0);
        (d0 + v * (d1 - d0)).clamp(0.0, 1.0)
    }

    fn rgb_at(&self, ix: usize, iy: usize) -> (f32, f32, f32) {
        match &self.color {
            ImageColor::Indexed(t) => {
                // Decode maps the sample straight to an index (default
                // [0, 2^bits-1]) — deliberately not the unit-clamped
                // `decoded()` path.
                let maxval = ((1u32 << self.bits) - 1) as f32;
                let v = self.sample(ix, iy, 0) as f32 / maxval;
                let d0 = self.decode.first().copied().unwrap_or(0.0);
                let d1 = self.decode.get(1).copied().unwrap_or(maxval);
                let idx = (d0 + v * (d1 - d0)).round().max(0.0) as u32;
                return t.rgb_at(idx.min(t.hival) as usize);
            }
            ImageColor::SeparationApprox => {
                let g = 1.0 - self.decoded(ix, iy, 0);
                return (g, g, g);
            }
            ImageColor::Direct => {}
        }
        match self.ncomp {
            1 => {
                let g = self.decoded(ix, iy, 0);
                (g, g, g)
            }
            3 => (
                self.decoded(ix, iy, 0),
                self.decoded(ix, iy, 1),
                self.decoded(ix, iy, 2),
            ),
            _ => {
                let c = self.decoded(ix, iy, 0);
                let m = self.decoded(ix, iy, 1);
                let y = self.decoded(ix, iy, 2);
                let k = self.decoded(ix, iy, 3);
                (
                    (1.0 - c) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
        }
    }

    /// The masked image as one straight-alpha RGBA raster (#739), the
    /// form the SVG and PDF backends take (a PNG alpha channel, a PDF
    /// /SMask). The image's samples are expanded by an integer factor
    /// large enough for the stencil's own resolution, and every cell is
    /// gated by the stencil exactly as the rasterizer gates a pixel, so
    /// mask and image may differ in size and ImageMatrix.
    fn composite(&self, st: &ImageCtx) -> Option<Composite> {
        let inv = invert6(self.matrix)?;
        // Samples per user unit along each axis, image vs stencil.
        let axes = |m: &[f64; 6]| (m[0].hypot(m[1]), m[2].hypot(m[3]));
        let ((dx, dy), (sx, sy)) = (axes(&self.matrix), axes(&st.matrix));
        let ratio = |s: f64, d: f64| if d > 0.0 { (s / d - 1e-6).ceil() } else { 1.0 };
        let mut want = ratio(sx, dx)
            .max(ratio(sy, dy))
            .clamp(1.0, MAX_STENCIL_SCALE);
        // Where the stencil's cell grid does not sit on the image's (a
        // rotation, or an offset that is not a whole number of mask
        // cells), mask edges fall inside image cells: expand enough to
        // keep them, to quarter-cell accuracy.
        let (m, i) = (&st.matrix, &inv);
        let rel = [
            m[0] * i[0] + m[2] * i[1],
            m[1] * i[0] + m[3] * i[1],
            m[0] * i[2] + m[2] * i[3],
            m[1] * i[2] + m[3] * i[3],
            m[0] * i[4] + m[2] * i[5] + m[4],
            m[1] * i[4] + m[3] * i[5] + m[5],
        ];
        let off_grid = |v: f64| (v - v.round()).abs() > 1e-6;
        if rel[1].abs() > 1e-9 || rel[2].abs() > 1e-9 || off_grid(rel[4]) || off_grid(rel[5]) {
            want = want.max(4.0);
        }
        let k = expansion(self.width, self.height, want as usize);
        let (w, h) = (self.width * k, self.height * k);
        if w == 0 || h == 0 {
            return None;
        }
        let mut rgba = Vec::with_capacity(w * h * 4);
        for cy in 0..h {
            for cx in 0..w {
                let (r, g, b) = self.rgb_at(cx / k, cy / k);
                // Centre of this raster cell in image space, then user space.
                let (px, py) = ((cx as f64 + 0.5) / k as f64, (cy as f64 + 0.5) / k as f64);
                let ux = inv[0] * px + inv[2] * py + inv[4];
                let uy = inv[1] * px + inv[3] * py + inv[5];
                let a = if st.stencil_passes(ux, uy) { 255 } else { 0 };
                rgba.extend([
                    (r * 255.0).round() as u8,
                    (g * 255.0).round() as u8,
                    (b * 255.0).round() as u8,
                    a,
                ]);
            }
        }
        Some(Composite { w, h, k, rgba })
    }

    fn record_svg_stenciled(&self, gfx: &mut Gfx, st: &ImageCtx, t: tiny_skia::Transform) {
        let Some(c) = self.composite(st) else { return };
        let Some(mut pm) = tiny_skia::Pixmap::new(c.w as u32, c.h as u32) else {
            return;
        };
        for (dst, px) in pm.pixels_mut().iter_mut().zip(c.rgba.chunks_exact(4)) {
            let a = u32::from(px[3]);
            let pre = |v: u8| ((u32::from(v) * a + 127) / 255) as u8;
            if let Some(p) = tiny_skia::PremultipliedColorU8::from_rgba(
                pre(px[0]),
                pre(px[1]),
                pre(px[2]),
                px[3],
            ) {
                *dst = p;
            }
        }
        if let Ok(png) = pm.encode_png() {
            gfx.svg_image(&png, c.w, c.h, c.transform(t));
        }
    }

    /// Mirror this image into the SVG recorder: the source samples as
    /// an embedded PNG plus the exact device transform the rasterizer
    /// applies (CTM ∘ ImageMatrix⁻¹, mapping sample coordinates to
    /// device space).
    fn record_svg(&self, gfx: &mut Gfx, ctm: tiny_skia::Transform) {
        let m = self.matrix;
        let img = tiny_skia::Transform::from_row(
            m[0] as f32,
            m[1] as f32,
            m[2] as f32,
            m[3] as f32,
            m[4] as f32,
            m[5] as f32,
        );
        let Some(inv) = img.invert() else { return };
        let t = inv.post_concat(ctm);
        if let Some(st) = &self.stencil {
            self.record_svg_stenciled(gfx, st, t);
            return;
        }
        let Some(mut pm) =
            tiny_skia::Pixmap::new(self.width.max(1) as u32, self.height.max(1) as u32)
        else {
            return;
        };
        let paint_rgb = gfx.rgb();
        for iy in 0..self.height {
            for ix in 0..self.width {
                let (r, g, b, a) = match self.mask {
                    Some(paint_ones) => {
                        if (self.sample(ix, iy, 0) != 0) != paint_ones {
                            (0.0, 0.0, 0.0, 0.0)
                        } else {
                            (
                                paint_rgb.0 as f32,
                                paint_rgb.1 as f32,
                                paint_rgb.2 as f32,
                                1.0,
                            )
                        }
                    }
                    None => {
                        let (r, g, b) = self.rgb_at(ix, iy);
                        (r, g, b, 1.0)
                    }
                };
                let px = tiny_skia::PremultipliedColorU8::from_rgba(
                    (r * a * 255.0).round() as u8,
                    (g * a * 255.0).round() as u8,
                    (b * a * 255.0).round() as u8,
                    (a * 255.0).round() as u8,
                );
                if let Some(px) = px {
                    pm.pixels_mut()[iy * self.width + ix] = px;
                }
            }
        }
        if let Ok(png) = pm.encode_png() {
            gfx.svg_image(
                &png,
                self.width,
                self.height,
                [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty],
            );
        }
    }

    /// Mirror into the PDF recorder: masks pass their packed 1-bit
    /// rows straight through (that's already our wire format); color
    /// images decode to interleaved RGB.
    fn record_pdf(&self, gfx: &mut Gfx, ctm: tiny_skia::Transform) {
        let m = self.matrix;
        let img = tiny_skia::Transform::from_row(
            m[0] as f32,
            m[1] as f32,
            m[2] as f32,
            m[3] as f32,
            m[4] as f32,
            m[5] as f32,
        );
        let Some(inv) = img.invert() else { return };
        let t = inv.post_concat(ctm);
        if let Some(st) = &self.stencil {
            if let Some(c) = self.composite(st) {
                let mut rgb = Vec::with_capacity(c.w * c.h * 3);
                let mut alpha = Vec::with_capacity(c.w * c.h);
                for px in c.rgba.chunks_exact(4) {
                    rgb.extend_from_slice(&px[..3]);
                    alpha.push(px[3]);
                }
                gfx.pdf_image(
                    c.w,
                    c.h,
                    crate::pdf::ImageData::RgbA { rgb, alpha },
                    c.transform(t),
                );
            }
            return;
        }
        let data = match self.mask {
            Some(paint_ones) => crate::pdf::ImageData::Mask {
                bits: self.data.clone(),
                paint_ones,
            },
            None => {
                let mut rgb = Vec::with_capacity(self.width * self.height * 3);
                for iy in 0..self.height {
                    for ix in 0..self.width {
                        let (r, g, b) = self.rgb_at(ix, iy);
                        rgb.push((r * 255.0).round() as u8);
                        rgb.push((g * 255.0).round() as u8);
                        rgb.push((b * 255.0).round() as u8);
                    }
                }
                crate::pdf::ImageData::Rgb(rgb)
            }
        };
        gfx.pdf_image(
            self.width,
            self.height,
            data,
            [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty],
        );
    }

    /// True when the ImageMatrix maps the user-space unit square's
    /// corners exactly onto the source rectangle's corners: the usual
    /// `[w 0 0 ±h …]` idiom (or a transposed/mirrored one). An
    /// invertible affine map sending four corners into the rectangle's
    /// four corners is onto them, so this is "footprint = unit square".
    fn unit_footprint(&self) -> bool {
        let m = self.matrix;
        let (w, h) = (self.width as f64, self.height as f64);
        [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .into_iter()
            .all(|(ux, uy)| {
                let ix = m[0] * ux + m[2] * uy + m[4];
                let iy = m[1] * ux + m[3] * uy + m[5];
                (ix == 0.0 || ix == w) && (iy == 0.0 || iy == h)
            })
    }

    fn rasterize(&self, gfx: &mut Gfx) {
        if self.width == 0 || self.height == 0 || self.bits == 0 || gfx.is_null_device() {
            return; // nothing to paint, or `nulldevice` discards it (#739)
        }
        let ctm = gfx.ctm();
        let Some(inv) = ctm.invert() else {
            return; // singular CTM: the image has no area
        };
        gfx.prepare_paint();
        if gfx.svg_wanted() {
            self.record_svg(gfx, ctm);
        }
        if gfx.pdf_wanted() {
            self.record_pdf(gfx, ctm);
        }
        // The image's footprint in user space is ImageMatrix⁻¹ of the
        // source rectangle [0,w]×[0,h] (PLRM 4.10.3): the unit square
        // only for the usual `w h scale` + `[w 0 0 ±h …]` idiom. That
        // idiom keeps its historical unit-square clip below, so its
        // edge pixels stay exactly as before (#534); any other
        // invertible matrix clips by the source rectangle alone.
        let m = self.matrix;
        let unit = self.unit_footprint();
        let corners: [(f32, f32); 4] = if unit {
            [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
        } else {
            let Some(mi) = invert6(m) else {
                return; // singular: refused at the operator, never here
            };
            let (w, h) = (self.width as f64, self.height as f64);
            [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(ix, iy)| {
                (
                    (mi[0] * ix + mi[2] * iy + mi[4]) as f32,
                    (mi[1] * ix + mi[3] * iy + mi[5]) as f32,
                )
            })
        };
        // Device bounding box of that footprint.
        let (pw, ph) = (gfx.pixmap.width() as i64, gfx.pixmap.height() as i64);
        let mut x0 = f32::MAX;
        let mut y0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y1 = f32::MIN;
        for (ux, uy) in corners {
            let dx = ctm.sx * ux + ctm.kx * uy + ctm.tx;
            let dy = ctm.ky * ux + ctm.sy * uy + ctm.ty;
            x0 = x0.min(dx);
            y0 = y0.min(dy);
            x1 = x1.max(dx);
            y1 = y1.max(dy);
        }
        let px0 = (x0.floor() as i64).max(0);
        let py0 = (y0.floor() as i64).max(0);
        let px1 = (x1.ceil() as i64).min(pw);
        let py1 = (y1.ceil() as i64).min(ph);

        let paint_rgb = gfx.rgb();
        let mask_data = gfx.clip_mask_data();
        let width = gfx.pixmap.width() as usize;
        let data = gfx.pixmap.data_mut();
        let mut painted = false;
        for py in py0..py1 {
            for px in px0..px1 {
                // Device pixel center → user space → image space.
                let (cx, cy) = (px as f32 + 0.5, py as f32 + 0.5);
                let ux = inv.sx * cx + inv.kx * cy + inv.tx;
                let uy = inv.ky * cx + inv.sy * cy + inv.ty;
                if unit && (!(0.0..1.0).contains(&ux) || !(0.0..1.0).contains(&uy)) {
                    continue;
                }
                let (ux, uy) = (f64::from(ux), f64::from(uy));
                let Some((ix, iy)) = self.cell_at(ux, uy) else {
                    continue;
                };
                if let Some(st) = &self.stencil
                    && !st.stencil_passes(ux, uy)
                {
                    continue;
                }
                let rgb = match self.mask {
                    Some(paint_ones) => {
                        let one = self.sample(ix, iy, 0) != 0;
                        if one != paint_ones {
                            continue;
                        }
                        (paint_rgb.0 as f32, paint_rgb.1 as f32, paint_rgb.2 as f32)
                    }
                    None => self.rgb_at(ix, iy),
                };
                let clip = mask_data
                    .as_ref()
                    .map_or(255, |m| m.data()[py as usize * width + px as usize]);
                if clip == 0 {
                    continue;
                }
                let a = f32::from(clip) / 255.0;
                let idx = (py as usize * width + px as usize) * 4;
                for (ch, v) in [rgb.0, rgb.1, rgb.2].into_iter().enumerate() {
                    let src = v * 255.0;
                    let dst = f32::from(data[idx + ch]);
                    data[idx + ch] = (src * a + dst * (1.0 - a)).round() as u8;
                }
                data[idx + 3] = 255;
                painted = true;
            }
        }
        if painted {
            gfx.dirty = true;
        }
    }
}

/// The inverse of a PostScript matrix `[a b c d tx ty]`, or None when
/// it is singular (determinant exactly zero, as gs tests it).
pub(crate) fn invert6(m: [f64; 6]) -> Option<[f64; 6]> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(m[4] * a + m[5] * c), -(m[4] * b + m[5] * d)])
}

#[cfg(test)]
mod tests {
    use super::expansion;

    #[test]
    fn expansion_is_capped_but_never_below_one() {
        assert_eq!(expansion(2, 2, 8), 8);
        assert_eq!(expansion(2000, 2000, 64), 2, "16 Mpixel cap");
        assert_eq!(expansion(5000, 4000, 8), 1, "over the cap still emits");
        assert_eq!(expansion(usize::MAX, usize::MAX, 8), 1, "no overflow");
        assert_eq!(expansion(0, 0, 8), 8);
    }
}
