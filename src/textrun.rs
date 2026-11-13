//! Text-run capture (#1119): the glyphs of a `show` as characters in a
//! named standard face, rather than as outlines, for an exporter that
//! can write real text.
//!
//! Two recorders use it: SVG (`--svg-text`, #1119, which writes `<text>`
//! instead of glyph paths) and PDF (`--pdf-text`, #1193, text objects in an
//! embedded subset font). The types here know nothing about either format:
//! both need the same three facts per glyph (which face, which glyph id
//! and character, and where in device space) and the same grouping into
//! runs (`notes/1119-svg-text.md`, `notes/1193-pdf-text.md`). Which glyphs
//! qualify differs, because SVG names a face for the viewer to substitute
//! and PDF carries the face itself.
//!
//! For SVG only the twelve built-in faces qualify (Helvetica, Times and
//! Courier in four styles each, drawn with the Liberation faces). A browser
//! can match them metrically: Arial, Times New Roman and Courier New (and
//! Liberation itself) have the same advance widths, so a substituted face
//! sits on the same per-glyph origins without gaps or collisions. The
//! other standard-35 families (Palatino, Bookman, ...) have no common
//! metric-compatible system face, so they keep their outlines.

use std::rc::Rc;

use crate::gfx::{ClipNode, Composite};

/// One glyph a `show` painted, as text: the character it stands for, the
/// face's glyph id, the font's FID, and the em-square → device transform
/// at the glyph's origin.
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
    /// The glyph's advance in 1/1000 em, as an embedded font's `/W` gives
    /// it (`pdf_font::advance_1000`).
    pub(crate) advance: f64,
    /// Whether the PDF recorder may write it as text: its face may be
    /// embedded and the glyph is a real one (`pdf_eligible`).
    pub(crate) embeddable: bool,
}

/// Whether the SVG recorder takes `g` as text: a standard face and a
/// character a viewer draws on its own at the glyph's x.
pub(crate) fn svg_eligible(g: &GlyphText) -> bool {
    text_safe(g.ch) && standard_face(g.fid).is_some()
}

/// Whether the PDF recorder takes `g` as text. The PDF carries the glyph
/// itself, so any script and any mark qualifies: the character only feeds
/// the `/ToUnicode` map that extraction reads. A control character maps to
/// nothing a reader wants, and glyph 0 is `.notdef`, which is CID 0 in the
/// embedded font, so both keep their outlines.
pub(crate) fn pdf_eligible(g: &GlyphText) -> bool {
    g.embeddable && g.gid != 0 && !g.ch.is_control()
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

/// Whether `ch` can stand for its glyph as text. A viewer must draw it
/// on its own, left to right, at the glyph's x: an allowlist of spacing,
/// left-to-right blocks the standard faces cover (Latin, Greek, Cyrillic,
/// punctuation, symbols). Everything else keeps its outline: controls and
/// format characters (soft hyphen, zero-width and bidi controls) draw
/// nothing; combining marks attach to the previous character in any
/// block; right-to-left letters are reordered by the viewer's bidi pass.
/// An allowlist, because std has no Unicode category or bidi tables and a
/// denylist missed marks outside U+0300-036F (Codex review, #1119).
pub(crate) fn text_safe(ch: char) -> bool {
    const SPACING_LTR: &[(u32, u32)] = &[
        (0x20, 0x7E),     // Basic Latin
        (0xA0, 0xAC),     // Latin-1 punctuation (U+00AD soft hyphen left out)
        (0xAE, 0x2FF),    // Latin-1 letters, Latin Extended-A/B, IPA, spacing modifiers
        (0x370, 0x3FF),   // Greek and Coptic
        (0x400, 0x482),   // Cyrillic (U+0483-0489 combining left out)
        (0x48A, 0x52F),   // Cyrillic and its supplement
        (0x1E00, 0x1FFF), // Latin Extended Additional, Greek Extended
        (0x2010, 0x2027), // General Punctuation up to the line/paragraph separators
        (0x2030, 0x205E), // ... after the bidi embedding controls
        (0x2070, 0x209F), // superscripts and subscripts
        (0x20A0, 0x20CF), // currency
        (0x2100, 0x218F), // letterlike, number forms
        (0x2190, 0x23FF), // arrows, mathematical and technical symbols
        (0x2500, 0x25FF), // box drawing, block elements, geometric shapes
        (0xFB00, 0xFB06), // Latin ligatures
    ];
    let c = u32::from(ch);
    SPACING_LTR.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
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
    /// SVG names the character; PDF names the glyph (its CID), which a
    /// re-encoded font can pick apart from its character.
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
            advance: 500.0,
            embeddable: true,
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
    fn invisible_combining_and_rtl_characters_keep_outlines() {
        for c in [
            'A', ' ', '&', '\u{e9}', '\u{3a9}', '\u{416}', '\u{2019}', '\u{20ac}', '\u{fb01}',
        ] {
            assert!(text_safe(c), "{c:?}");
        }
        for c in [
            '\u{9}', '\u{7f}', '\u{ad}', '\u{301}', '\u{483}', '\u{5b0}', '\u{5d0}', '\u{627}',
            '\u{200b}', '\u{202e}', '\u{20d7}', '\u{feff}', '\u{ffff}', '\u{e000}',
        ] {
            assert!(!text_safe(c), "{c:?}");
        }
    }
}
