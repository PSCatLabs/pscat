//! Unicode-mode text past one face (issue #1425): a fallback chain across the
//! compiled-in faces, colour emoji from the bundled Twemoji face, and the
//! report of a character nothing covers. `docs/dev/FONTS.md`, "Unicode text,
//! fallback faces and colour emoji", has the design.
//!
//! Only Unicode-mode outline fonts come here (a `CatalogEncoding::Unicode`
//! face, or any outline font dict with `/PSCatUnicode true`). Byte-mode
//! fonts keep the Encoding path untouched, so Ghostscript parity there is
//! unaffected.

use std::sync::OnceLock;

use ttf_parser::colr::{ClipBox, CompositeMode, Paint, Painter};
use ttf_parser::{Face, GlyphId, RgbaColor, Transform};

use super::{
    FontState, GlyphTransform, PathSink, ShowMode, advance_of, builtin_index, decode_one,
    embedded_fid, face_for, glyph_for_char, glyph_transform, loaded_fid, missing_glyph,
    paint_resolved_glyph_limited, resolve_glyph,
};
use crate::encodings;
use crate::error::PsError;
use crate::gfx::{DevPoint, Gfx, PsPath};

/// The bundled colour emoji face: Twemoji 14.0.1 as Mozilla's COLR v0 build
/// (`fonts/emoji/PROVENANCE.md`). Compiled in, so emoji work the same in
/// the binary, the crate and the browser build.
static EMOJI_DATA: &[u8] = include_bytes!("../../fonts/emoji/TwemojiMozilla.ttf");

fn emoji_face() -> Option<&'static Face<'static>> {
    static FACE: OnceLock<Option<Face<'static>>> = OnceLock::new();
    FACE.get_or_init(|| Face::parse(EMOJI_DATA, 0).ok())
        .as_ref()
}

/// Codepoints with `Emoji_Presentation=Yes` (Unicode 14.0 `emoji-data.txt`,
/// the version the bundled art follows), merged into ranges. These draw as
/// emoji even without a U+FE0F after them; every other emoji-capable
/// character (U+2764 HEAVY BLACK HEART, digits, ...) is text unless U+FE0F
/// asks for the emoji.
const EMOJI_PRESENTATION: &[(u32, u32)] = &[
    (0x231A, 0x231B),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x25FD, 0x25FE),
    (0x2614, 0x2615),
    (0x2648, 0x2653),
    (0x267F, 0x267F),
    (0x2693, 0x2693),
    (0x26A1, 0x26A1),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D4, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26FA, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x270A, 0x270B),
    (0x2728, 0x2728),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1E6, 0x1F1FF),
    (0x1F201, 0x1F201),
    (0x1F21A, 0x1F21A),
    (0x1F22F, 0x1F22F),
    (0x1F232, 0x1F236),
    (0x1F238, 0x1F23A),
    (0x1F250, 0x1F251),
    (0x1F300, 0x1F320),
    (0x1F32D, 0x1F335),
    (0x1F337, 0x1F37C),
    (0x1F37E, 0x1F393),
    (0x1F3A0, 0x1F3CA),
    (0x1F3CF, 0x1F3D3),
    (0x1F3E0, 0x1F3F0),
    (0x1F3F4, 0x1F3F4),
    (0x1F3F8, 0x1F43E),
    (0x1F440, 0x1F440),
    (0x1F442, 0x1F4FC),
    (0x1F4FF, 0x1F53D),
    (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F57A, 0x1F57A),
    (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A4),
    (0x1F5FB, 0x1F64F),
    (0x1F680, 0x1F6C5),
    (0x1F6CC, 0x1F6CC),
    (0x1F6D0, 0x1F6D2),
    (0x1F6D5, 0x1F6D7),
    (0x1F6DD, 0x1F6DF),
    (0x1F6EB, 0x1F6EC),
    (0x1F6F4, 0x1F6FC),
    (0x1F7E0, 0x1F7EB),
    (0x1F7F0, 0x1F7F0),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1F9FF),
    (0x1FA70, 0x1FA74),
    (0x1FA78, 0x1FA7C),
    (0x1FA80, 0x1FA86),
    (0x1FA90, 0x1FAAC),
    (0x1FAB0, 0x1FABA),
    (0x1FAC0, 0x1FAC5),
    (0x1FAD0, 0x1FAD9),
    (0x1FAE0, 0x1FAE7),
    (0x1FAF0, 0x1FAF6),
];

fn in_ranges(ranges: &[(u32, u32)], cp: u32) -> bool {
    ranges
        .binary_search_by(|&(lo, hi)| {
            if hi < cp {
                std::cmp::Ordering::Less
            } else if lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// `Default_Ignorable_Code_Point` (Unicode 14.0 `DerivedCoreProperties.txt`):
/// joiners, variation selectors, bidi controls, tags. They draw nothing and
/// take no width when no face has a glyph for them, and are never reported.
const IGNORABLE: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFF8),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];

/// C0 and C1 controls (a `\n` or `\t` left in a shown string): blank, with
/// the advance a missing glyph takes, as in byte mode. Never reported.
fn is_control(cp: u32) -> bool {
    cp < 0x20 || (0x7F..=0x9F).contains(&cp)
}

const VS15: u32 = 0xFE0E;
const VS16: u32 = 0xFE0F;
const KEYCAP: u32 = 0x20E3;

/// Whether `cp` asks for the emoji face before the text faces: U+FE0F after
/// it, an `Emoji_Presentation` character, or a skin-tone modifier or keycap
/// mark after it. U+FE0E (text presentation) always keeps the text faces
/// first.
fn emoji_first(cp: u32, next: Option<u32>) -> bool {
    match next {
        Some(VS15) => false,
        Some(VS16 | KEYCAP | 0x1F3FB..=0x1F3FF) => true,
        _ => in_ranges(EMOJI_PRESENTATION, cp),
    }
}

/// A compiled-in text face the chain tries after the font's own face.
enum Fallback {
    /// A built-in face, by its standard name.
    Builtin(&'static str),
    /// An embedded symbolic face (Symbol, ZapfDingbats), whose `cmap` holds
    /// only its byte codes: a character is found through the face's
    /// encoding, by glyph name.
    Symbolic {
        stem: &'static str,
        names: fn() -> [&'static str; 256],
        to_char: fn(&str) -> Option<char>,
    },
}

/// Liberation Sans, Serif and Mono (Latin, Greek, Cyrillic, Hebrew and
/// common punctuation and symbols), then URW Symbol (math, arrows) and
/// ZapfDingbats. The catalog's Unicode-mode faces (Korean, Japanese, Thai)
/// are not in the chain: they are font packs, not compiled in, and loading
/// one mid-show is a follow-up. A character only they have is reported
/// with a hint naming the face.
const FALLBACKS: [Fallback; 5] = [
    Fallback::Builtin("Helvetica"),
    Fallback::Builtin("Times-Roman"),
    Fallback::Builtin("Courier"),
    Fallback::Symbolic {
        stem: "StandardSymbolsPS",
        names: encodings::symbol_encoding,
        to_char: encodings::symbol_name_to_char,
    },
    Fallback::Symbolic {
        stem: "D050000L",
        names: encodings::dingbats_encoding,
        to_char: encodings::dingbat_name_to_char,
    },
];

/// The FID of an embedded symbolic face, registering it on first use. The
/// registration allocates a parsed `Face` (its bytes are compiled in), held
/// against the memory cap first.
fn symbolic_fid(stem: &str, probe: &crate::limits::LimitProbe) -> Result<Option<i64>, PsError> {
    if let Some(fid) = loaded_fid(stem, stem) {
        return Ok(Some(fid));
    }
    probe.check_alloc(std::mem::size_of::<Face>())?;
    Ok(embedded_fid(stem))
}

/// What one Unicode-mode scalar came to.
pub(super) struct Shown {
    /// The user-space advance.
    pub adv: (f64, f64),
    /// Bytes after this scalar that an emoji ligature took in (a ZWJ
    /// sequence, a flag, a keycap): those scalars draw nothing.
    pub absorbed: usize,
    /// No face had the character: `.notdef` was drawn and it is reported.
    pub missing: bool,
    /// A default-ignorable character no face drew: it takes no
    /// `ashow`/`widthshow` extra either, like the scalars a ligature took in.
    pub ignorable: bool,
}

impl Shown {
    fn drawn(adv: (f64, f64)) -> Self {
        Shown {
            adv,
            absorbed: 0,
            missing: false,
            ignorable: false,
        }
    }
}

/// One glyph of a Unicode-mode outline font: the font's own face, then the
/// fallback chain, then the emoji face (first instead, when the text asks
/// for emoji presentation), then `.notdef`. `text` and `after` are the shown
/// string and the byte offset just past this scalar, for the presentation
/// selector and emoji sequences that follow it.
#[allow(clippy::too_many_arguments)]
pub(super) fn show_scalar(
    gfx: &mut Gfx,
    fs: &FontState,
    code: u32,
    text: &[u8],
    after: usize,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<Shown, PsError> {
    let face = face_for(fs.fid)?;
    let Some(ch) = char::from_u32(code) else {
        return missing(gfx, face, fs, mode, pen, probe, work);
    };
    let next = decode_one(text, after, true).map(|(c, _)| c);
    let blank = is_control(code) || in_ranges(IGNORABLE, code);
    let want_emoji = !blank && emoji_first(code, next);
    if want_emoji && let Some(shown) = emoji(gfx, fs, ch, text, after, mode, pen, probe, work)? {
        return Ok(shown);
    }
    if let Some(gid) = glyph_for_char(face, ch) {
        let adv = paint_resolved_glyph_limited(
            gfx,
            face,
            fs.matrix,
            Some(gid),
            mode,
            pen,
            Some((fs.fid, ch)),
            GlyphId(0),
            probe,
            work,
        )?;
        // A face's own joiner or selector glyph is zero width too.
        return Ok(Shown {
            ignorable: in_ranges(IGNORABLE, code),
            ..Shown::drawn(adv)
        });
    }
    if in_ranges(IGNORABLE, code) {
        return Ok(Shown {
            ignorable: true,
            ..Shown::drawn((0.0, 0.0))
        });
    }
    if is_control(code) {
        let adv = paint_resolved_glyph_limited(
            gfx,
            face,
            fs.matrix,
            None,
            mode,
            pen,
            None,
            missing_glyph(face, fs.fid),
            probe,
            work,
        )?;
        return Ok(Shown::drawn(adv));
    }
    for fallback in &FALLBACKS {
        let found =
            match fallback {
                Fallback::Builtin(name) => builtin_index(name)
                    .filter(|&fid| fid != fs.fid)
                    .and_then(|fid| {
                        let face = face_for(fid).ok()?;
                        Some((fid, face, glyph_for_char(face, ch)?))
                    }),
                Fallback::Symbolic {
                    stem,
                    names,
                    to_char,
                } => match symbolic_fid(stem, probe)? {
                    Some(fid) if fid != fs.fid => {
                        // A scan of the 256 encoding names, only for a
                        // character the earlier faces lacked.
                        *work = work.saturating_add(256);
                        face_for(fid).ok().and_then(|face| {
                            let name = names().into_iter().find(|n| to_char(n) == Some(ch))?;
                            Some((fid, face, resolve_glyph(face, name)?))
                        })
                    }
                    _ => None,
                },
            };
        if let Some((fid, fb_face, gid)) = found {
            let adv = paint_resolved_glyph_limited(
                gfx,
                fb_face,
                fs.matrix,
                Some(gid),
                mode,
                pen,
                Some((fid, ch)),
                GlyphId(0),
                probe,
                work,
            )?;
            return Ok(Shown::drawn(adv));
        }
    }
    if !want_emoji
        && !blank
        && next != Some(VS15)
        && let Some(shown) = emoji(gfx, fs, ch, text, after, mode, pen, probe, work)?
    {
        return Ok(shown);
    }
    missing(gfx, face, fs, mode, pen, probe, work)
}

/// Nothing has the character: the font's own `.notdef` (a box, on the
/// faces that draw one), with its own advance, and a report.
fn missing(
    gfx: &mut Gfx,
    face: &Face,
    fs: &FontState,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<Shown, PsError> {
    let adv = paint_resolved_glyph_limited(
        gfx,
        face,
        fs.matrix,
        Some(GlyphId(0)),
        mode,
        pen,
        None,
        GlyphId(0),
        probe,
        work,
    )?;
    Ok(Shown {
        missing: true,
        ..Shown::drawn(adv)
    })
}

/// The emoji face's glyph for `ch`, joined with the sequence after it when
/// the face has a ligature for that (`None`: the face lacks `ch`).
#[allow(clippy::too_many_arguments)]
fn emoji(
    gfx: &mut Gfx,
    fs: &FontState,
    ch: char,
    text: &[u8],
    after: usize,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<Option<Shown>, PsError> {
    let Some(face) = emoji_face() else {
        return Ok(None);
    };
    let Some(first) = glyph_for_char(face, ch) else {
        return Ok(None);
    };
    let (gid, absorbed) = ligature(face, first, text, after, work);
    let adv = paint_emoji(gfx, face, fs.matrix, gid, mode, pen, probe, work)?;
    Ok(Some(Shown {
        absorbed,
        ..Shown::drawn(adv)
    }))
}

/// The most scalars after the first one an emoji ligature is matched
/// against. The bundled face's longest sequence has 9 (a family with skin
/// tones); the cap bounds the look-ahead whatever the text holds.
const MAX_COMPONENTS: usize = 16;

/// The longest GSUB ligature (type 4) of the emoji face that starts with
/// `first` and matches the scalars from `after` on, and how many bytes
/// those scalars take; `(first, 0)` when none matches. The look-ahead is
/// decoded once into a fixed buffer, and every component compared is added
/// to `work` (a base like U+1F468 starts over 300 ligatures).
fn ligature(
    face: &Face,
    first: GlyphId,
    text: &[u8],
    after: usize,
    work: &mut u64,
) -> (GlyphId, usize) {
    let Some(gsub) = face.tables().gsub else {
        return (first, 0);
    };
    let mut comps = [GlyphId(0); MAX_COMPONENTS];
    let mut ends = [0usize; MAX_COMPONENTS];
    let (mut n, mut pos) = (0, after);
    while n < MAX_COMPONENTS {
        let Some((cp, len)) = decode_one(text, pos, true) else {
            break;
        };
        let Some(g) = char::from_u32(cp).and_then(|c| glyph_for_char(face, c)) else {
            break;
        };
        comps[n] = g;
        pos += len;
        ends[n] = pos;
        n += 1;
    }
    if n == 0 {
        return (first, 0);
    }
    let mut best = (first, 0usize);
    for li in 0..gsub.lookups.len() {
        let Some(lookup) = gsub.lookups.get(li) else {
            continue;
        };
        for sub in lookup
            .subtables
            .into_iter::<ttf_parser::gsub::SubstitutionSubtable>()
        {
            let ttf_parser::gsub::SubstitutionSubtable::Ligature(lig) = sub else {
                continue;
            };
            let Some(set) = lig
                .coverage
                .get(first)
                .and_then(|i| lig.ligature_sets.get(i))
            else {
                continue;
            };
            for l in set {
                let k = usize::from(l.components.len());
                *work = work.saturating_add(k.max(1) as u64);
                if k > best.1
                    && k <= n
                    && l.components
                        .into_iter()
                        .zip(&comps[..k])
                        .all(|(a, b)| a == *b)
                {
                    best = (l.glyph, k);
                }
            }
        }
    }
    match best.1 {
        0 => (first, 0),
        k => (best.0, ends[k - 1] - after),
    }
}

/// Records a COLR v0 glyph's layers: each outline and the colour it is
/// filled with. The bundled face is v0 only (solid colours); the other
/// v1 operations are not reached and do nothing.
struct Layers {
    current: GlyphId,
    layers: Vec<(GlyphId, RgbaColor)>,
}

impl<'a> Painter<'a> for Layers {
    fn outline_glyph(&mut self, glyph_id: GlyphId) {
        self.current = glyph_id;
    }
    fn paint(&mut self, paint: Paint<'a>) {
        if let Paint::Solid(c) = paint {
            self.layers.push((self.current, c));
        }
    }
    fn push_clip(&mut self) {}
    fn push_clip_box(&mut self, _: ClipBox) {}
    fn pop_clip(&mut self) {}
    fn push_layer(&mut self, _: CompositeMode) {}
    fn pop_layer(&mut self) {}
    fn push_transform(&mut self, _: Transform) {}
    fn pop_transform(&mut self) {}
}

/// Paint (or append, or measure) one emoji-face glyph: its colour layers,
/// each filled with its own colour (times the current alpha), through the
/// same FontMatrix, CTM and limits as a text glyph. `charpath` appends
/// every layer's outline. Layers are always outlines, never PDF or SVG text.
#[allow(clippy::too_many_arguments)]
fn paint_emoji(
    gfx: &mut Gfx,
    face: &'static Face<'static>,
    fm: [f64; 6],
    gid: GlyphId,
    mode: &ShowMode,
    pen: DevPoint,
    probe: &crate::limits::LimitProbe,
    work: &mut u64,
) -> Result<(f64, f64), PsError> {
    if matches!(mode, ShowMode::Width) || !face.is_color_glyph(gid) {
        return paint_resolved_glyph_limited(
            gfx,
            face,
            fm,
            Some(gid),
            mode,
            pen,
            None,
            GlyphId(0),
            probe,
            work,
        );
    }
    let (r, g, b) = gfx.state().rgb;
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let foreground = RgbaColor::new(to_u8(r), to_u8(g), to_u8(b), 255);
    let mut rec = Layers {
        current: GlyphId(0),
        layers: Vec::new(),
    };
    face.paint_color_glyph(gid, 0, foreground, &mut rec);
    let m: GlyphTransform = glyph_transform(gfx, face, fm, pen);
    for (layer, c) in rec.layers {
        let mut sink = PathSink {
            path: PsPath::default(),
            m,
            cur: (0.0, 0.0),
        };
        if face.outline_glyph(layer, &mut sink).is_none() || sink.path.is_empty() {
            continue;
        }
        match mode {
            ShowMode::Paint => {
                let rgb = (
                    f32::from(c.red) / 255.0,
                    f32::from(c.green) / 255.0,
                    f32::from(c.blue) / 255.0,
                );
                gfx.fill_path_rgb(&sink.path, rgb, f32::from(c.alpha) / 255.0, probe, work)?;
            }
            ShowMode::Charpath => gfx.append_path(&sink.path),
            ShowMode::Width => {}
        }
    }
    let k = 1000.0 / f64::from(face.units_per_em());
    let w = f64::from(advance_of(face, gid).unwrap_or(0));
    Ok((fm[0] * k * w, fm[1] * k * w))
}

/// The note for a character no face had, `pscat: warning:` like
/// `findfont`'s stand-in notes. Korean, Japanese and Thai name the
/// Unicode-mode face that has them.
pub(crate) fn missing_glyph_note(cp: u32, font: &str) -> String {
    let shown = match char::from_u32(cp) {
        Some(c) if !is_control(c as u32) && !in_ranges(IGNORABLE, cp) => format!(" ({c})"),
        _ => String::new(),
    };
    let what = if cp == u32::from(char::REPLACEMENT_CHARACTER) {
        " (invalid UTF-8, or a replacement character)".to_string()
    } else {
        shown
    };
    let mut note = format!(
        "pscat: warning: no glyph for U+{cp:04X}{what} in /{font} or its fallback faces; drew .notdef"
    );
    if let Some(stem) = script_face(cp) {
        let pack = crate::fontpacks::pack_for_stem(&format!("{stem}-Regular"));
        note.push_str(&format!(
            ". Show it with a Unicode-mode face such as /{stem}{}",
            script_face_hint(stem, pack)
        ));
    }
    note
}

#[cfg(not(target_arch = "wasm32"))]
fn script_face_hint(_stem: &str, pack: Option<&str>) -> String {
    match pack {
        Some(p) => format!(" (font pack {p}: `pscat fonts install --pack {p}`)"),
        None => String::new(),
    }
}

#[cfg(target_arch = "wasm32")]
fn script_face_hint(stem: &str, _pack: Option<&str>) -> String {
    format!(" (load it with addFont('{stem}-Regular', bytes))")
}

/// The catalog's Unicode-mode face for a Korean, Japanese/CJK or Thai
/// character.
fn script_face(cp: u32) -> Option<&'static str> {
    match cp {
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => Some("NotoSansKR"),
        0x3000..=0x30FF
        | 0x31F0..=0x31FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF => Some("NotoSansJP"),
        0x0E00..=0x0E7F => Some("NotoSansThai"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_tables_are_sorted_and_disjoint() {
        for table in [EMOJI_PRESENTATION, IGNORABLE] {
            for w in table.windows(2) {
                assert!(w[0].1 < w[1].0, "{w:x?}");
            }
            assert!(table.iter().all(|&(lo, hi)| lo <= hi));
        }
    }

    #[test]
    fn presentation() {
        assert!(emoji_first(0x1F600, None));
        assert!(!emoji_first(0x2764, None));
        assert!(emoji_first(0x2764, Some(VS16)));
        assert!(!emoji_first(0x1F600, Some(VS15)));
        assert!(emoji_first(0x23, Some(VS16)));
        assert!(!emoji_first(0x41, None));
    }

    #[test]
    fn bundled_emoji_face_parses_with_colour_layers() {
        let face = emoji_face().expect("bundled emoji face parses");
        let gid = glyph_for_char(face, '\u{1F600}').expect("grinning face");
        assert!(face.is_color_glyph(gid));
    }

    #[test]
    fn flag_and_zwj_sequences_join() {
        let face = emoji_face().expect("face");
        let mut work = 0;
        // U+1F1FA U+1F1F8: one flag glyph, the second scalar taken in.
        let text = "\u{1F1FA}\u{1F1F8}".as_bytes();
        let first = glyph_for_char(face, '\u{1F1FA}').expect("RI U");
        let (gid, absorbed) = ligature(face, first, text, 4, &mut work);
        assert_ne!(gid, first);
        assert_eq!(absorbed, 4);
        assert!(work > 0);
        // A lone regional indicator stays itself.
        let (gid, absorbed) = ligature(face, first, "\u{1F1FA}".as_bytes(), 4, &mut work);
        assert_eq!((gid, absorbed), (first, 0));
    }
}
