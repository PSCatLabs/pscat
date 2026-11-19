//! A 16-bit shadow of the page for fills that composite (#731 part 2).
//!
//! The page is tiny-skia's 8-bit premultiplied RGBA [`Pixmap`], and every
//! reader (PNG, the window, WebAssembly, sheets, bands) keeps reading it.
//! An 8-bit page cannot hold a stack of faint layers: once
//! `(target - current) * alpha` falls under half a level a layer changes
//! nothing, so 1000 fills of white at 0.002 over black stalled at 128 where
//! the arithmetic gives 221. [`Deep`] keeps a 16-bit copy of each pixel a
//! fill composites, composites in f32 with the exact `setalpha`, and writes
//! both the 16-bit value and the 8-bit pixel each time, so the page stays
//! current and nothing downstream changes.
//!
//! Only `fill` and glyphs composite here; strokes, images and `shfill` still
//! write the 8-bit page through tiny-skia. Each 16-bit pixel checks itself
//! against the 8-bit one before use: if another writer moved the 8-bit
//! pixel out of the 16-bit value's rounding bucket, the 16-bit value is
//! taken from the 8-bit pixel again. So nothing has to tell the shadow
//! what the other writers did, and the shadow can be wrong by at most the
//! half level the 8-bit page itself rounds to.
//!
//! A single blend over a pixel the shadow had not seen gives tiny-skia's f32
//! pipeline's bytes exactly when the alpha is 1 (same inputs, same float
//! operations, round to nearest even); a test pins that, so edges and
//! glyphs of opaque art render as before. Below alpha 1 the result can
//! move by a level, because the alpha is exact where tiny-skia's paint
//! carries it as a byte.
//!
//! Memory: 8 bytes a pixel, allocated a row at a time when a fill first
//! composites into it. Under `--max-memory` the shadow is used only when
//! a whole page of it would take at most a quarter of the cap, decided
//! from the page size and the cap alone so a render never depends on how
//! much heap happens to be free ([`Deep::allowed`]).

use tiny_skia::{Pixmap, PremultipliedColorU8};

/// Tolerance of the bucket check, in 16-bit units: a pixel's 16-bit value
/// `d` and its 8-bit value `p` agree when `|d - 257 p| <= BUCKET`. Half a
/// level is 128.5 units; a tie rounds either way, so 129.
const BUCKET: i32 = 129;

/// tiny-skia's byte-to-float factor (`load_8888`), kept identical so a
/// pixel loaded from the 8-bit page composites exactly as tiny-skia would.
const FROM_U8: f32 = 1.0 / 255.0;
const FROM_U16: f32 = 1.0 / 65535.0;

/// How a composited fill blends with the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Blend {
    Normal,
    Multiply,
}

/// The source of a composite: a flat colour (each channel the byte tiny-skia
/// would get, as a float, unpremultiplied), its exact alpha, and the blend.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Source {
    rgb: [f32; 3],
    alpha: f32,
    blend: Blend,
}

impl Source {
    pub(crate) fn new(rgb8: [u8; 3], alpha: f32, blend: Blend) -> Self {
        Source {
            rgb: rgb8.map(|c| f32::from(c) * FROM_U8),
            alpha: alpha.clamp(0.0, 1.0),
            blend,
        }
    }
}

/// The 16-bit shadow: one premultiplied RGBA `u16` quadruple per pixel, a
/// row allocated when first composited into (an empty row is not yet
/// allocated).
#[derive(Default)]
pub(crate) struct Deep {
    width: usize,
    rows: Vec<Vec<u16>>,
}

impl Deep {
    /// Whether a `width` x `height` page may have a shadow under a memory
    /// cap of `max_memory` bytes (`None`: no cap): a whole page of it at
    /// most a quarter of the cap.
    pub(crate) fn allowed(width: u32, height: u32, max_memory: Option<usize>) -> bool {
        match max_memory {
            None => true,
            Some(max) => {
                (width as usize)
                    .saturating_mul(height as usize)
                    .saturating_mul(8)
                    <= max / 4
            }
        }
    }

    /// Forget everything: the page was erased or replaced.
    pub(crate) fn clear(&mut self) {
        self.width = 0;
        self.rows = Vec::new();
    }

    /// Fit the shadow to `pixmap`, clearing it if the size changed.
    fn fit(&mut self, pixmap: &Pixmap) {
        let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
        if self.width != w || self.rows.len() != h {
            self.width = w;
            self.rows = Vec::new();
            self.rows.resize_with(h, Vec::new);
        }
    }

    /// Record that an opaque fill set pixels `x0..x1` of row `y` to `color`
    /// in the 8-bit page (tiny-skia's fill, which this does not repeat).
    /// Only a row already allocated is touched; a row that is not reads the
    /// 8-bit page when it is first composited into.
    pub(crate) fn note_opaque(
        &mut self,
        pixmap: &Pixmap,
        y: usize,
        x0: usize,
        x1: usize,
        color: PremultipliedColorU8,
    ) {
        self.fit(pixmap);
        let Some(row) = self.rows.get_mut(y) else {
            return;
        };
        if row.is_empty() {
            return;
        }
        let q =
            [color.red(), color.green(), color.blue(), color.alpha()].map(|c| u16::from(c) * 257);
        let x1 = x1.min(self.width);
        for px in row[x0.min(x1) * 4..x1 * 4].chunks_exact_mut(4) {
            px.copy_from_slice(&q);
        }
    }

    /// Composite `src` into pixels `x0..x1` of row `y`, each with coverage
    /// `cov[x - x0]` out of 255 (`None`: full coverage), into both the
    /// shadow and `pixmap`.
    pub(crate) fn composite(
        &mut self,
        pixmap: &mut Pixmap,
        y: usize,
        x0: usize,
        x1: usize,
        cov: Option<&[u8]>,
        src: &Source,
    ) {
        self.fit(pixmap);
        let w = self.width;
        if y >= self.rows.len() || x0 >= x1.min(w) {
            return;
        }
        let x1 = x1.min(w);
        let row = &mut self.rows[y];
        let fresh = row.is_empty();
        if fresh {
            row.resize(w * 4, 0);
        }
        let deep = &mut row[x0 * 4..x1 * 4];
        let page = &mut pixmap.pixels_mut()[y * w + x0..y * w + x1];
        // The source, premultiplied in f32 as tiny-skia's `premultiply`.
        let sa = src.alpha;
        let s = [src.rgb[0] * sa, src.rgb[1] * sa, src.rgb[2] * sa, sa];
        for (i, (d16, p8)) in deep.chunks_exact_mut(4).zip(page.iter_mut()).enumerate() {
            let k = match cov {
                Some(c) => match c[i] {
                    0 => continue,
                    255 => None,
                    m => Some(f32::from(m) * FROM_U8),
                },
                None => None,
            };
            let p = [p8.red(), p8.green(), p8.blue(), p8.alpha()];
            let mut d = [0.0_f32; 4];
            let stale =
                fresh || (0..4).any(|c| (i32::from(d16[c]) - i32::from(p[c]) * 257).abs() > BUCKET);
            for c in 0..4 {
                // A value that is exactly the 8-bit one loads as tiny-skia
                // loads that byte, so a first blend matches its bytes.
                let q = u16::from(p[c]) * 257;
                d[c] = if stale || d16[c] == q {
                    f32::from(p[c]) * FROM_U8
                } else {
                    f32::from(d16[c]) * FROM_U16
                };
            }
            let da = d[3];
            let mut out = [0.0_f32; 4];
            for c in 0..4 {
                let r = match src.blend {
                    Blend::Normal => d[c] * (1.0 - sa) + s[c],
                    Blend::Multiply => s[c] * (1.0 - da) + d[c] * (1.0 - sa) + s[c] * d[c],
                };
                out[c] = match k {
                    Some(k) => (r - d[c]) * k + d[c],
                    None => r,
                };
            }
            let mut b = [0_u8; 4];
            for c in 0..4 {
                let v = out[c].clamp(0.0, 1.0);
                b[c] = (v * 255.0).round_ties_even() as u8;
                d16[c] = (v * 65535.0).round_ties_even() as u16;
            }
            // Premultiplied: no channel above alpha (each is rounded on
            // its own, so a colour can pass the alpha by one at the top).
            let a = b[3];
            *p8 = PremultipliedColorU8::from_rgba(b[0].min(a), b[1].min(a), b[2].min(a), a)
                .unwrap_or(PremultipliedColorU8::TRANSPARENT);
            let a16 = d16[3];
            for c in 0..3 {
                d16[c] = d16[c].min(a16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::{BlendMode, Mask, Paint, Rect, Transform};

    /// tiny-skia's f32 pipeline filling a 1x1 pixel of `backdrop` with
    /// `rgb`/`alpha8` at coverage `m`, as `fill_coverage` did before.
    fn skia(backdrop: [u8; 4], rgb: [u8; 3], alpha8: u8, blend: BlendMode, m: u8) -> [u8; 4] {
        let mut pm = Pixmap::new(1, 1).unwrap();
        pm.pixels_mut()[0] =
            PremultipliedColorU8::from_rgba(backdrop[0], backdrop[1], backdrop[2], backdrop[3])
                .unwrap();
        let mut paint = Paint::default();
        paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], alpha8);
        paint.blend_mode = blend;
        paint.anti_alias = false;
        paint.force_hq_pipeline = true;
        let mut mask = Mask::new(1, 1).unwrap();
        mask.data_mut()[0] = m;
        let rect = Rect::from_ltrb(0.0, 0.0, 1.0, 1.0).unwrap();
        let mask = if m == 255 { None } else { Some(&mask) };
        pm.fill_rect(rect, &paint, Transform::identity(), mask);
        let p = pm.pixels()[0];
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    fn deep(backdrop: [u8; 4], rgb: [u8; 3], alpha: f32, blend: Blend, m: u8) -> [u8; 4] {
        let mut pm = Pixmap::new(1, 1).unwrap();
        pm.pixels_mut()[0] =
            PremultipliedColorU8::from_rgba(backdrop[0], backdrop[1], backdrop[2], backdrop[3])
                .unwrap();
        let mut deep = Deep::default();
        let src = Source::new(rgb, alpha, blend);
        deep.composite(&mut pm, 0, 0, 1, Some(&[m]), &src);
        let p = pm.pixels()[0];
        [p.red(), p.green(), p.blue(), p.alpha()]
    }

    /// A small deterministic generator, so the sweep is the same each run.
    fn lcg(state: &mut u32) -> u8 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*state >> 24) as u8
    }

    /// At alpha 1 a first blend gives tiny-skia's bytes exactly, for both
    /// blend modes, any coverage, over opaque and translucent backdrops: the
    /// shadow changes only what stacking changes.
    #[test]
    fn a_first_opaque_blend_matches_tiny_skia() {
        let mut seed = 731;
        for _ in 0..20_000 {
            let a = lcg(&mut seed);
            let a = if a < 128 { 255 } else { a };
            let backdrop = [0, 0, 0].map(|_: u8| lcg(&mut seed).min(a));
            let backdrop = [backdrop[0], backdrop[1], backdrop[2], a];
            let rgb = [lcg(&mut seed), lcg(&mut seed), lcg(&mut seed)];
            let m = lcg(&mut seed);
            for (sk, dp) in [
                (BlendMode::SourceOver, Blend::Normal),
                (BlendMode::Multiply, Blend::Multiply),
            ] {
                assert_eq!(
                    deep(backdrop, rgb, 1.0, dp, m),
                    skia(backdrop, rgb, 255, sk, m),
                    "{dp:?} {rgb:?} over {backdrop:?} at coverage {m}"
                );
            }
        }
    }

    /// Below alpha 1 a first blend is within a level of tiny-skia's (whose
    /// alpha is a byte).
    #[test]
    fn a_first_translucent_blend_is_within_a_level() {
        let mut seed = 1731;
        for _ in 0..20_000 {
            let backdrop = [lcg(&mut seed), lcg(&mut seed), lcg(&mut seed), 255];
            let rgb = [lcg(&mut seed), lcg(&mut seed), lcg(&mut seed)];
            let alpha8 = lcg(&mut seed);
            let m = lcg(&mut seed);
            for (sk, dp) in [
                (BlendMode::SourceOver, Blend::Normal),
                (BlendMode::Multiply, Blend::Multiply),
            ] {
                let got = deep(backdrop, rgb, f32::from(alpha8) / 255.0, dp, m);
                let want = skia(backdrop, rgb, alpha8, sk, m);
                for c in 0..4 {
                    assert!(got[c].abs_diff(want[c]) <= 1, "{dp:?} {got:?} vs {want:?}");
                }
            }
        }
    }

    /// A pixel another writer changed is read from the 8-bit page again.
    #[test]
    fn a_pixel_changed_behind_the_shadow_is_reloaded() {
        let mut pm = Pixmap::new(1, 1).unwrap();
        pm.fill(tiny_skia::Color::BLACK);
        let mut deep = Deep::default();
        let white = Source::new([255, 255, 255], 0.002, Blend::Normal);
        for _ in 0..1000 {
            deep.composite(&mut pm, 0, 0, 1, None, &white);
        }
        assert!(pm.pixels()[0].red() > 200);
        // As a stroke would: the 8-bit pixel set to black behind its back.
        pm.fill(tiny_skia::Color::BLACK);
        deep.composite(&mut pm, 0, 0, 1, None, &white);
        assert_eq!(
            pm.pixels()[0].red(),
            1,
            "one layer over black, not over the old grey"
        );
    }

    #[test]
    fn the_memory_rule_depends_only_on_page_and_cap() {
        assert!(Deep::allowed(10_000, 10_000, None));
        assert!(Deep::allowed(1000, 1000, Some(32_000_000)));
        assert!(!Deep::allowed(1000, 1000, Some(31_999_999)));
    }
}
