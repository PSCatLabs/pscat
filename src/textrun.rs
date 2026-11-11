//! Text-run capture (#1119): the glyphs of a `show` as characters in a
//! named standard face, rather than as outlines, for an exporter that
//! can write real text.
//!
//! The SVG recorder is the one user today (`--svg-text`, which writes
//! `<text>` instead of glyph paths). The types here know nothing about
//! SVG on purpose: the PDF side of #1026 (text objects with an embedded
//! subset font) needs the same three facts per glyph (which face, which
//! glyph id and character, and where in device space) and the same
//! grouping into runs, so it can build on this module instead of
//! re-deriving them from the show engine (`notes/1119-svg-text.md`).
//!
//! Only the twelve built-in faces qualify (Helvetica, Times and Courier
//! in four styles each, drawn with the Liberation faces). A browser can
//! match them metrically: Arial, Times New Roman and Courier New (and
//! Liberation itself) have the same advance widths, so a substituted face
//! sits on the same per-glyph origins without gaps or collisions. The
//! other standard-35 families (Palatino, Bookman, ...) have no common
//! metric-compatible system face, so they keep their outlines.

use std::rc::Rc;

use crate::gfx::{ClipNode, Composite};

/// One glyph a `show` painted, as text: the character it stands for, the
/// face's glyph id (for a future embedded-font writer), the font's FID,
/// and the em-square → device transform at the glyph's origin.
///
/// `em` maps glyph space in ems (1.0 = one em, y up, origin on the
/// baseline at the pen) to device space: `[a b c d tx ty]`, PostScript
/// row-vector layout, `(tx, ty)` the device pen position.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphText {
    pub(crate) ch: char,
    pub(crate) gid: u16,
    pub(crate) fid: i64,
    pub(crate) em: [f64; 6],
}

/// How a standard face is named to a viewer that substitutes fonts.
pub(crate) struct StandardFace {
    /// A CSS font-family list: the PostScript family first (what the
    /// program asked for), then metric-compatible faces, then a generic.
    pub(crate) css_family: &'static str,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
}

const SANS: &str = "Helvetica,Arial,'Liberation Sans',sans-serif";
const SERIF: &str = "Times,'Times New Roman','Liberation Serif',serif";
// Courier New first: the system "Courier" on macOS is a lighter, narrower
// face than the Liberation Mono outlines; all three have 600-unit advances.
const MONO: &str = "'Courier New',Courier,'Liberation Mono',monospace";

/// The face `fid` names, when it is one of the built-in standard faces
/// text output covers; `None` keeps its glyphs as outlines.
pub(crate) fn standard_face(fid: i64) -> Option<StandardFace> {
    let name = crate::font::builtin_ps_name(fid)?;
    let (family, style) = match name.split_once('-') {
        Some((f, s)) => (f, s),
        None => (name, ""),
    };
    let css_family = match family {
        "Helvetica" => SANS,
        "Times" => SERIF,
        "Courier" => MONO,
        _ => return None,
    };
    Some(StandardFace {
        css_family,
        bold: style.contains("Bold"),
        italic: style.contains("Italic") || style.contains("Oblique"),
    })
}

/// Whether `ch` can stand for its glyph as text: printable, allowed in
/// XML 1.0, and drawn by a browser where the outline would be. Format
/// characters (soft hyphen, zero-width and bidi controls) render as
/// nothing and combining marks attach to the previous character, so
/// those keep their outlines.
pub(crate) fn text_safe(ch: char) -> bool {
    let c = u32::from(ch);
    !(ch.is_control()
        || c == 0xAD
        || (0x300..=0x36F).contains(&c)
        || (0x200B..=0x200F).contains(&c)
        || (0x2028..=0x202E).contains(&c)
        || (0x2060..=0x206F).contains(&c)
        || c == 0xFEFF
        || c == 0xFFFE
        || c == 0xFFFF)
}

/// Where one glyph sits in its run's text frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    /// The em size in device units: what a viewer's font-size is.
    pub(crate) size: f64,
    /// Text space (x right, y down, ems scaled to `size`) → device, with
    /// the scale divided out: `[a b c d]`, `|det| = 1`. The identity for
    /// upright, unskewed text, which is the common case.
    pub(crate) frame: [f64; 4],
    /// The glyph origin in the frame's coordinates (device coordinates
    /// when the frame is the identity).
    pub(crate) x: f64,
    pub(crate) y: f64,
}

const FRAME_EPS: f64 = 1e-9;

impl Placement {
    /// `None` for a degenerate transform (a zero-size or collapsed font
    /// matrix), which keeps its outline.
    pub(crate) fn of(g: &GlyphText) -> Option<Placement> {
        let [a, b, c, d, tx, ty] = g.em;
        // Text space has y down; glyph space has y up.
        let (ta, tb, tc, td) = (a, b, -c, -d);
        let det = ta * td - tb * tc;
        let size = det.abs().sqrt();
        if !(size.is_finite() && size > 1e-6) {
            return None;
        }
        let frame = [ta / size, tb / size, tc / size, td / size];
        let (x, y) = if is_identity(frame) {
            (tx, ty)
        } else {
            // frame⁻¹ applied to the pen: local coordinates.
            let fdet = frame[0] * frame[3] - frame[1] * frame[2];
            (
                (frame[3] * tx - frame[2] * ty) / fdet,
                (-frame[1] * tx + frame[0] * ty) / fdet,
            )
        };
        Some(Placement { size, frame, x, y })
    }
}

fn is_identity(f: [f64; 4]) -> bool {
    (f[0] - 1.0).abs() < FRAME_EPS
        && f[1].abs() < FRAME_EPS
        && f[2].abs() < FRAME_EPS
        && (f[3] - 1.0).abs() < FRAME_EPS
}

/// One glyph of a run: its character, glyph id and pen x in the frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunGlyph {
    pub(crate) ch: char,
    /// SVG names the character; an embedded-font writer (the PDF half of
    /// #1026) needs the glyph itself, which a re-encoded font can pick
    /// apart from its character.
    #[allow(dead_code)]
    pub(crate) gid: u16,
    pub(crate) x: f64,
}

/// Consecutive glyphs that one text element can carry: same face, size,
/// frame, paint and clip, on one baseline. Each glyph keeps its own x,
/// so pscat's metrics place every glyph whatever face a viewer
/// substitutes.
pub(crate) struct TextRun {
    pub(crate) fid: i64,
    pub(crate) size: f64,
    pub(crate) frame: [f64; 4],
    /// The baseline, in the frame's coordinates.
    pub(crate) y: f64,
    pub(crate) rgb: (f32, f32, f32),
    pub(crate) comp: Composite,
    pub(crate) chain: Option<Rc<ClipNode>>,
    pub(crate) glyphs: Vec<RunGlyph>,
}

/// Same baseline: within a thousandth of a device unit, the precision the
/// SVG recorder prints coordinates at.
const BASELINE_EPS: f64 = 5e-4;

impl TextRun {
    pub(crate) fn new(
        g: &GlyphText,
        p: Placement,
        rgb: (f32, f32, f32),
        comp: Composite,
        chain: Option<Rc<ClipNode>>,
    ) -> Self {
        TextRun {
            fid: g.fid,
            size: p.size,
            frame: p.frame,
            y: p.y,
            rgb,
            comp,
            chain,
            glyphs: vec![RunGlyph {
                ch: g.ch,
                gid: g.gid,
                x: p.x,
            }],
        }
    }

    /// Whether `g`, placed at `p` and painted with this paint and clip,
    /// continues this run. Anything that changes how the run renders
    /// (face, size, frame, colour, alpha or blend, clip, baseline) starts
    /// a new one.
    ///
    /// A glyph painted with alpha or a blend mode never joins a run. The
    /// raster composites each glyph on its own, so where two glyphs overlap
    /// the second blends over the first; a viewer composites one element (or
    /// its blend group) once, which would lose that overlap. One element per
    /// glyph keeps it, as one `<path>` per glyph did.
    pub(crate) fn accepts(
        &self,
        g: &GlyphText,
        p: &Placement,
        rgb: (f32, f32, f32),
        comp: Composite,
        chain: &Option<Rc<ClipNode>>,
    ) -> bool {
        let same_chain = match (&self.chain, chain) {
            (None, None) => true,
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            _ => false,
        };
        comp.is_default()
            && self.fid == g.fid
            && (self.size - p.size).abs() <= self.size * FRAME_EPS
            && self
                .frame
                .iter()
                .zip(p.frame.iter())
                .all(|(a, b)| (a - b).abs() < FRAME_EPS)
            && (self.y - p.y).abs() < BASELINE_EPS
            && self.rgb == rgb
            && self.comp == comp
            && same_chain
    }

    /// Upright, unskewed text: the run needs no transform.
    pub(crate) fn upright(&self) -> bool {
        is_identity(self.frame)
    }

    pub(crate) fn push(&mut self, g: &GlyphText, p: &Placement) {
        self.glyphs.push(RunGlyph {
            ch: g.ch,
            gid: g.gid,
            x: p.x,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(em: [f64; 6]) -> GlyphText {
        GlyphText {
            ch: 'a',
            gid: 1,
            fid: 0,
            em,
        }
    }

    #[test]
    fn upright_text_places_in_device_coordinates() {
        // 12 pt at 72 dpi on a y-down device: glyph y up becomes device y down.
        let p = Placement::of(&glyph([12.0, 0.0, 0.0, -12.0, 30.0, 50.0])).expect("placement");
        assert!(is_identity(p.frame));
        assert!((p.size - 12.0).abs() < 1e-12);
        assert_eq!((p.x, p.y), (30.0, 50.0));
    }

    #[test]
    fn rotated_text_gets_a_frame_and_local_coordinates() {
        // 90° counter-clockwise on the page: glyph x runs up the device (−y).
        let p = Placement::of(&glyph([0.0, -10.0, -10.0, 0.0, 40.0, 60.0])).expect("placement");
        assert!(!is_identity(p.frame));
        assert!((p.size - 10.0).abs() < 1e-12);
        // The frame maps (x, y) local back to the device pen.
        let [a, b, c, d] = p.frame;
        let (dx, dy) = (a * p.x + c * p.y, b * p.x + d * p.y);
        assert!((dx - 40.0).abs() < 1e-9 && (dy - 60.0).abs() < 1e-9);
    }

    #[test]
    fn a_collapsed_font_matrix_has_no_placement() {
        assert!(Placement::of(&glyph([0.0, 0.0, 0.0, 0.0, 1.0, 1.0])).is_none());
    }

    #[test]
    fn only_the_builtin_standard_faces_qualify() {
        let helv = standard_face(0).expect("Helvetica is FID 0");
        assert!(!helv.bold && !helv.italic);
        let courier_bo = standard_face(11).expect("Courier-BoldOblique is FID 11");
        assert!(courier_bo.bold && courier_bo.italic);
        assert!(courier_bo.css_family.contains("monospace"));
        assert!(standard_face(12).is_none());
        assert!(standard_face(-1).is_none());
    }

    #[test]
    fn invisible_and_combining_characters_keep_outlines() {
        assert!(text_safe('A') && text_safe(' ') && text_safe('&') && text_safe('\u{e9}'));
        for c in [
            '\u{9}', '\u{ad}', '\u{301}', '\u{200b}', '\u{feff}', '\u{ffff}',
        ] {
            assert!(!text_safe(c), "{c:?}");
        }
    }
}
