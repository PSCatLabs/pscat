//! Embedded subset fonts for PDF text (`pscat --pdf-text`, #1193).
//!
//! Each face a document's text uses becomes a Type 0 font with
//! `/Identity-H` encoding over a `CIDFontType0` whose program is a bare,
//! CID-keyed CFF (`/FontFile3 /Subtype /CIDFontType0C`, PDF 1.3), plus a
//! `/ToUnicode` CMap so the text can be copied and searched. A glyph's CID
//! is its glyph id in the source face, so the content stream can name it
//! while the page is still being recorded, before the subset is known.
//!
//! The CFF is rebuilt from the outlines pscat paints
//! (`ttf_parser::Face::outline_glyph`), not copied out of the font file:
//! - one writer serves TrueType (`glyf`) and CFF sources alike, so the
//!   Liberation, TeX Gyre, URW and Noto faces all take the same path;
//! - a variable font gets the instance pscat renders. NotoSansKR and
//!   NotoSansJP default to their Thin master and are pinned to Regular
//!   (`font::prepare_face`); copying their `glyf` would show Thin text in
//!   every viewer;
//! - the subset holds only the glyphs used, with no subsetting library
//!   (none is in the dependency tree).
//!
//! A glyph shared by two characters (NotoSansJP draws U+2012 and U+2013
//! with one glyph) gets a second CID beyond the face's glyph range, with a
//! copy of the charstring, so `/ToUnicode` can give each its own character.
//!
//! Every call into ttf-parser here runs under `guard::catch`, as the show
//! engine's own lookups do: a hostile font can trip an overflow assertion
//! inside the parser in a debug build (the `truetype` fuzz regressions).
//! A glyph whose outline cannot be written exactly (an operand past the
//! 16.16 range, a charstring past 65,535 bytes, a parser panic) is refused,
//! and the recorder keeps its outline in the content stream.
//!
//! TrueType quadratics become cubics exactly (control points at 2/3).
//! Outlines are scaled to the CFF's default 1000-unit em (FontMatrix
//! 0.001, written nowhere, so no Top DICT/FDArray matrix composition can
//! disagree between viewers). A coordinate that is not a whole unit is
//! written as a 16.16 fixed number, and each delta is taken from the
//! previous *rounded* point, so rounding never drifts along a contour.
//! Hints are not carried; pscat's own raster is unhinted too.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;

use ttf_parser::{Face, GlyphId, OutlineBuilder};

use crate::guard::catch;

/// One CID of an embedded font: the character it stands for, its advance
/// in 1/1000 em and the Type 2 charstring of the glyph it draws.
pub(crate) struct CidGlyph {
    pub(crate) ch: char,
    pub(crate) advance: f64,
    pub(crate) charstring: Vec<u8>,
}

/// The glyphs one embedded font holds, by CID. A glyph's first character
/// takes CID = glyph id; another character drawn with the same glyph gets
/// a CID from `number_of_glyphs` up.
pub(crate) struct UsedGlyphs {
    pub(crate) cids: BTreeMap<u16, CidGlyph>,
    pairs: HashMap<(u16, char), u16>,
    /// Glyphs whose outline cannot be embedded: they stay paths.
    refused: HashSet<u16>,
    next_extra: u32,
}

/// CIDs per font: a CFF INDEX counts to 65,535, `.notdef` included.
const MAX_CIDS: usize = 65_534;

impl UsedGlyphs {
    pub(crate) fn new(face: &Face) -> Self {
        UsedGlyphs {
            cids: BTreeMap::new(),
            pairs: HashMap::new(),
            refused: HashSet::new(),
            next_extra: u32::from(catch(|| face.number_of_glyphs()).unwrap_or(u16::MAX)),
        }
    }

    /// The CID that draws `gid` and stands for `ch`, building its
    /// charstring the first time the glyph is seen. `None` keeps the
    /// glyph's outline: it cannot be embedded, or the font is out of CIDs.
    pub(crate) fn cid(&mut self, face: &Face, gid: u16, ch: char, advance: f64) -> Option<u16> {
        if let Some(&cid) = self.pairs.get(&(gid, ch)) {
            return Some(cid);
        }
        if gid == 0 || self.refused.contains(&gid) || self.cids.len() >= MAX_CIDS {
            return None;
        }
        let (cid, charstring) = match self.cids.get(&gid) {
            // Glyph ids are below `number_of_glyphs` and extra CIDs at or
            // above it, so CID `gid` is this glyph's first character.
            Some(first) => {
                let cid = u16::try_from(self.next_extra).ok()?;
                self.next_extra += 1;
                (cid, first.charstring.clone())
            }
            None => match charstring(face, gid) {
                Some(cs) => (gid, cs),
                None => {
                    self.refused.insert(gid);
                    return None;
                }
            },
        };
        self.cids.insert(
            cid,
            CidGlyph {
                ch,
                advance,
                charstring,
            },
        );
        self.pairs.insert((gid, ch), cid);
        Some(cid)
    }
}

/// A glyph's advance in 1/1000 em, rounded to what the PDF writes, so the
/// `/W` entry and the `TJ` adjustments computed against it agree exactly.
pub(crate) fn advance_1000(face: &Face, gid: u16) -> f64 {
    let upem = f64::from(face.units_per_em().max(1));
    let adv = catch(|| face.glyph_hor_advance(GlyphId(gid)))
        .flatten()
        .map(f64::from)
        .unwrap_or(0.0);
    round_to(adv * 1000.0 / upem, 3)
}

fn round_to(v: f64, decimals: i32) -> f64 {
    let m = 10f64.powi(decimals);
    let r = (v * m).round() / m;
    if r == 0.0 { 0.0 } else { r }
}

/// A number with at most `decimals` places, trailing zeros trimmed, never
/// `-0`; the f64 twin of `pdf::fmt_num`.
pub(crate) fn num(v: f64, decimals: usize) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{v:.decimals$}");
    let t = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s.as_str()
    };
    if t == "-0" {
        "0".to_string()
    } else {
        t.to_string()
    }
}

/// Whether a face's licence bits (OS/2 `fsType`) let a document carry a
/// subset of it. The bundled faces are all OFL, GUST/LPPL or URW's
/// AGPL-with-font-exception (`notes/1193-pdf-text.md`); this check is for
/// faces the program or a user font pack brings. Restricted-licence
/// embedding (bit 1 alone), "no subsetting" (bit 8, since this writer only
/// subsets) and "bitmap only" (bit 9) keep the outlines. Preview & Print
/// (bit 2) and Editable (bit 3) allow embedding in a document, which is
/// what a PDF is. As OpenType recommends, set bits are read permissively
/// (bits 2 and 3 both set, as several TeX Gyre faces have it, is
/// Editable). A face with no OS/2 table carries no restriction.
pub(crate) fn embedding_allowed(face: &Face) -> bool {
    let Some(os2) = catch(|| face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2"))) else {
        return false;
    };
    let Some(os2) = os2 else {
        return true;
    };
    let Some(bits) = os2.get(8..10) else {
        return false;
    };
    fs_type_allows(u16::from_be_bytes([bits[0], bits[1]]))
}

fn fs_type_allows(fs_type: u16) -> bool {
    let restricted = fs_type & 0x000E == 0x0002;
    !restricted && fs_type & 0x0300 == 0
}

// ---------------------------------------------------------------------
// Type 2 charstrings
// ---------------------------------------------------------------------

/// Builds one Type 2 charstring from an outline, in 1/65536 units of the
/// 1000-unit em.
struct Charstring {
    out: Vec<u8>,
    /// Source units → 1000-unit em.
    scale: f64,
    /// The last point as written (rounded), in 1/65536 units.
    cur: (i64, i64),
    /// The last point exactly, in 1000-unit em, for the quadratic →
    /// cubic conversion.
    last: (f64, f64),
    /// False once an operand did not fit a Type 2 number.
    exact: bool,
}

const FIXED_ONE: i64 = 65536;

fn fixed(v: f64) -> i64 {
    // A hostile font's coordinates may be anything; a finite clamp keeps
    // the arithmetic below in range (the glyph just draws wrong).
    let v = if v.is_finite() { v } else { 0.0 };
    (v * FIXED_ONE as f64).round().clamp(-1e15, 1e15) as i64
}

/// A charstring operand: a short integer form when the value is whole and
/// small, else a 16.16 fixed number (operator 255). `false` when `d` is
/// beyond what Type 2 can say (±32768 units): the caller refuses the glyph
/// rather than draw it distorted.
fn push_operand(out: &mut Vec<u8>, d: i64) -> bool {
    if d % FIXED_ONE == 0 {
        let v = d / FIXED_ONE;
        if (-107..=107).contains(&v) {
            out.push((v + 139) as u8);
            return true;
        }
        if (108..=1131).contains(&v) {
            let w = v - 108;
            out.extend_from_slice(&[((w >> 8) + 247) as u8, (w & 0xFF) as u8]);
            return true;
        }
        if (-1131..=-108).contains(&v) {
            let w = -v - 108;
            out.extend_from_slice(&[((w >> 8) + 251) as u8, (w & 0xFF) as u8]);
            return true;
        }
        if (-32768..=32767).contains(&v) {
            out.push(28);
            out.extend_from_slice(&(v as i16).to_be_bytes());
            return true;
        }
    }
    let Ok(f) = i32::try_from(d) else {
        return false;
    };
    out.push(255);
    out.extend_from_slice(&f.to_be_bytes());
    true
}

impl Charstring {
    fn point(&mut self, x: f64, y: f64) {
        let p = (fixed(x), fixed(y));
        self.exact &= push_operand(&mut self.out, p.0 - self.cur.0);
        self.exact &= push_operand(&mut self.out, p.1 - self.cur.1);
        self.cur = p;
        self.last = (x, y);
    }
}

impl OutlineBuilder for Charstring {
    fn move_to(&mut self, x: f32, y: f32) {
        let s = self.scale;
        self.point(f64::from(x) * s, f64::from(y) * s);
        self.out.push(21); // rmoveto
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let s = self.scale;
        self.point(f64::from(x) * s, f64::from(y) * s);
        self.out.push(5); // rlineto
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let s = self.scale;
        let (x0, y0) = self.last;
        let (qx, qy) = (f64::from(x1) * s, f64::from(y1) * s);
        let (x3, y3) = (f64::from(x) * s, f64::from(y) * s);
        self.point(x0 + 2.0 / 3.0 * (qx - x0), y0 + 2.0 / 3.0 * (qy - y0));
        self.point(x3 + 2.0 / 3.0 * (qx - x3), y3 + 2.0 / 3.0 * (qy - y3));
        self.point(x3, y3);
        self.out.push(8); // rrcurveto
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let s = self.scale;
        self.point(f64::from(x1) * s, f64::from(y1) * s);
        self.point(f64::from(x2) * s, f64::from(y2) * s);
        self.point(f64::from(x) * s, f64::from(y) * s);
        self.out.push(8); // rrcurveto
    }

    // A Type 2 contour closes itself at the next rmoveto or endchar.
    fn close(&mut self) {}
}

/// The Type 2 limit on one charstring.
const MAX_CHARSTRING: usize = 65_535;

impl Charstring {
    fn new(units_per_em: u16) -> Self {
        Charstring {
            out: Vec::new(),
            scale: 1000.0 / f64::from(units_per_em.max(1)),
            cur: (0, 0),
            last: (0.0, 0.0),
            exact: true,
        }
    }

    /// The finished charstring, or `None` if it cannot be written exactly
    /// within the Type 2 limits.
    fn finish(mut self) -> Option<Vec<u8>> {
        self.out.push(14); // endchar
        (self.exact && self.out.len() <= MAX_CHARSTRING).then_some(self.out)
    }
}

/// The charstring for `gid`, `None` when it cannot be embedded exactly.
/// No width operand: PDF takes advances from the CIDFont's `/W`, which
/// overrides the font program's. An empty glyph (a space) is a bare
/// `endchar`.
fn charstring(face: &Face, gid: u16) -> Option<Vec<u8>> {
    let mut cs = Charstring::new(face.units_per_em());
    // `outline_glyph` is `None` for an empty glyph and for one the parser
    // gives up on halfway; either way nothing usable was drawn.
    let drawn = catch(|| face.outline_glyph(GlyphId(gid), &mut cs).is_some())?;
    if !drawn {
        cs = Charstring::new(face.units_per_em());
    }
    cs.finish()
}

// ---------------------------------------------------------------------
// CFF structure
// ---------------------------------------------------------------------

/// A CFF INDEX: count, offset size, 1-based offsets, data.
fn index(items: &[Vec<u8>]) -> Vec<u8> {
    if items.is_empty() {
        return vec![0, 0];
    }
    let total: usize = items.iter().map(Vec::len).sum();
    let last = total + 1;
    let off_size: usize = match last {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        0x1_0000..=0xFF_FFFF => 3,
        _ => 4,
    };
    let count = u16::try_from(items.len()).unwrap_or(u16::MAX);
    let mut out = Vec::with_capacity(3 + off_size * (items.len() + 1) + total);
    out.extend_from_slice(&count.to_be_bytes());
    out.push(off_size as u8);
    let mut off = 1usize;
    let push_off = |out: &mut Vec<u8>, v: usize| {
        let b = (v as u32).to_be_bytes();
        out.extend_from_slice(&b[4 - off_size..]);
    };
    push_off(&mut out, off);
    for it in items.iter().take(usize::from(count)) {
        off += it.len();
        push_off(&mut out, off);
    }
    for it in items.iter().take(usize::from(count)) {
        out.extend_from_slice(it);
    }
    out
}

/// A DICT integer operand, shortest form.
fn dict_int(out: &mut Vec<u8>, v: i32) {
    match v {
        -107..=107 => out.push((v + 139) as u8),
        108..=1131 => {
            let w = v - 108;
            out.extend_from_slice(&[((w >> 8) + 247) as u8, (w & 0xFF) as u8]);
        }
        -1131..=-108 => {
            let w = -v - 108;
            out.extend_from_slice(&[((w >> 8) + 251) as u8, (w & 0xFF) as u8]);
        }
        -32768..=32767 => {
            out.push(28);
            out.extend_from_slice(&(v as i16).to_be_bytes());
        }
        _ => {
            out.push(29);
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
}

/// A DICT integer in the fixed 5-byte form, for offsets: the Top DICT's
/// length then does not depend on the offsets it holds, so the layout is
/// computed in one pass.
fn dict_int5(out: &mut Vec<u8>, v: usize) {
    out.push(29);
    out.extend_from_slice(&(i32::try_from(v).unwrap_or(i32::MAX)).to_be_bytes());
}

/// The face's bounding box in 1000-unit em, rounded outward.
pub(crate) fn bbox_1000(face: &Face) -> [i32; 4] {
    let k = 1000.0 / f64::from(face.units_per_em().max(1));
    let Some(r) = catch(|| face.global_bounding_box()) else {
        return [0; 4];
    };
    let c = |v: f64| v.clamp(-1e6, 1e6) as i32;
    [
        c((f64::from(r.x_min) * k).floor()),
        c((f64::from(r.y_min) * k).floor()),
        c((f64::from(r.x_max) * k).ceil()),
        c((f64::from(r.y_max) * k).ceil()),
    ]
}

/// A bare CID-keyed CFF holding `notdef` and `glyphs` (by ascending CID,
/// none 0), registry-ordering-supplement Adobe-Identity-0 to match the
/// CIDFont's `/CIDSystemInfo`.
pub(crate) fn cid_cff(
    name: &str,
    bbox: [i32; 4],
    notdef: Vec<u8>,
    glyphs: &[(u16, &[u8])],
) -> Vec<u8> {
    let header = [1u8, 0, 4, 4];
    let name_index = index(&[name.as_bytes().to_vec()]);
    // Custom strings start at SID 391.
    let strings = index(&[b"Adobe".to_vec(), b"Identity".to_vec()]);
    let gsubrs = index(&[]);

    let glyphs = &glyphs[..glyphs.len().min(MAX_CIDS)];
    let mut charstrings = Vec::with_capacity(glyphs.len() + 1);
    charstrings.push(notdef);
    for (_, cs) in glyphs {
        charstrings.push(cs.to_vec());
    }
    let charstrings = index(&charstrings);
    let nglyphs = u16::try_from(glyphs.len() + 1).unwrap_or(u16::MAX);

    // charset format 0: the CID of every glyph after .notdef.
    let mut charset = vec![0u8];
    for (cid, _) in glyphs {
        charset.extend_from_slice(&cid.to_be_bytes());
    }
    // FDSelect format 3: one range, every glyph in FD 0.
    let mut fdselect = vec![3u8, 0, 1, 0, 0, 0];
    fdselect.extend_from_slice(&nglyphs.to_be_bytes());
    let private = vec![139u8, 20, 139, 21]; // defaultWidthX 0, nominalWidthX 0
    let cid_count = i32::from(glyphs.last().map_or(0, |g| g.0)) + 1;

    let top = |offs: [usize; 4]| {
        let mut d = Vec::new();
        dict_int(&mut d, 391);
        dict_int(&mut d, 392);
        dict_int(&mut d, 0);
        d.extend_from_slice(&[12, 30]); // ROS, first in a CIDFont's Top DICT
        dict_int(&mut d, cid_count);
        d.extend_from_slice(&[12, 34]); // CIDCount
        for v in bbox {
            dict_int(&mut d, v);
        }
        d.push(5); // FontBBox
        dict_int5(&mut d, offs[0]);
        d.push(15); // charset
        dict_int5(&mut d, offs[1]);
        d.extend_from_slice(&[12, 37]); // FDSelect
        dict_int5(&mut d, offs[2]);
        d.push(17); // CharStrings
        dict_int5(&mut d, offs[3]);
        d.extend_from_slice(&[12, 36]); // FDArray
        d
    };
    let fd = |size: usize, off: usize| {
        let mut d = Vec::new();
        dict_int5(&mut d, size);
        dict_int5(&mut d, off);
        d.push(18); // Private
        d
    };
    let top_len = index(&[top([0; 4])]).len();
    let charset_at = header.len() + name_index.len() + top_len + strings.len() + gsubrs.len();
    let fdselect_at = charset_at + charset.len();
    let charstrings_at = fdselect_at + fdselect.len();
    let fdarray_at = charstrings_at + charstrings.len();
    let fdarray_len = index(&[fd(0, 0)]).len();
    let private_at = fdarray_at + fdarray_len;

    let mut out = Vec::with_capacity(private_at + private.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&name_index);
    out.extend_from_slice(&index(&[top([
        charset_at,
        fdselect_at,
        charstrings_at,
        fdarray_at,
    ])]));
    out.extend_from_slice(&strings);
    out.extend_from_slice(&gsubrs);
    out.extend_from_slice(&charset);
    out.extend_from_slice(&fdselect);
    out.extend_from_slice(&charstrings);
    out.extend_from_slice(&index(&[fd(private.len(), private_at)]));
    out.extend_from_slice(&private);
    out
}

// ---------------------------------------------------------------------
// PDF objects
// ---------------------------------------------------------------------

/// A PDF name's body from a font's PostScript name: printable ASCII
/// without the PDF delimiters, which is what a PostScript name holds.
fn name_token(ps_name: &str) -> String {
    let s: String = ps_name
        .chars()
        .filter(|c| c.is_ascii_graphic() && !"()<>[]{}/%#".contains(*c))
        .take(60)
        .collect();
    if s.is_empty() { "Font".to_string() } else { s }
}

/// The six-letter subset tag (ISO 32000 9.6.4): different subsets of one
/// face get different `BaseFont` names. FNV-1a over the face's index in
/// the document and its glyph ids, so the tag is the same on every run and
/// every platform (the CLI and the wasm build give the same bytes).
fn subset_tag(font_index: usize, gids: &[u16]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut eat = |b: u8| {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    };
    for b in (font_index as u64).to_be_bytes() {
        eat(b);
    }
    for g in gids {
        for b in g.to_be_bytes() {
            eat(b);
        }
    }
    (0..6)
        .map(|i| char::from(b'A' + ((h >> (i * 8)) % 26) as u8))
        .collect()
}

/// The `/ToUnicode` CMap: each CID to its character, UTF-16BE (a
/// surrogate pair past the BMP).
fn to_unicode(cids: &BTreeMap<u16, CidGlyph>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &CidGlyph)> = cids.iter().collect();
    // At most 100 entries per bfchar block (the CMap format's limit).
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (cid, g) in chunk {
            let mut units = [0u16; 2];
            let dst: String =
                g.ch.encode_utf16(&mut units)
                    .iter()
                    .map(|u| format!("{u:04X}"))
                    .collect();
            let _ = writeln!(s, "<{cid:04X}> <{dst}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// The `/W` array: runs of consecutive CIDs share one bracket.
fn widths(cids: &BTreeMap<u16, CidGlyph>) -> String {
    let mut s = String::from("[");
    let mut prev: Option<u16> = None;
    for (&cid, g) in cids {
        if prev.is_some_and(|p| u32::from(p) + 1 == u32::from(cid)) {
            s.push(' ');
        } else {
            if prev.is_some() {
                s.push(']');
            }
            let _ = write!(s, " {cid} [");
        }
        s.push_str(&num(g.advance, 3));
        prev = Some(cid);
    }
    if prev.is_some() {
        s.push(']');
    }
    s.push_str(" ]");
    s
}

/// A stream object, Flate-compressed like the page contents.
fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let (body, filter) = match crate::pdf::deflate(&[data]) {
        Some(z) => (z, " /Filter /FlateDecode"),
        None => (data.to_vec(), ""),
    };
    let mut out = format!("<< {dict}/Length {}{filter} >>\nstream\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    out.extend_from_slice(b"\nendstream");
    out
}

/// Objects written per embedded font: Type 0 font, CIDFont, descriptor,
/// font program, ToUnicode CMap.
pub(crate) const OBJECTS_PER_FONT: usize = 5;

/// The descriptor's metrics, in 1000-unit em: bbox, ascent, descent, cap
/// height, italic angle, flags.
type Metrics = ([i32; 4], f64, f64, f64, f64, u32);

fn metrics(face: &Face) -> Option<Metrics> {
    catch(|| {
        let k = 1000.0 / f64::from(face.units_per_em().max(1));
        let ascent = f64::from(face.ascender()) * k;
        let descent = f64::from(face.descender()) * k;
        let cap = face.capital_height().map_or(ascent, |c| f64::from(c) * k);
        let italic = f64::from(face.italic_angle());
        // Symbolic (bit 3): the glyphs are named by CID, not a standard
        // Latin set. FixedPitch and Italic as the face says.
        let mut flags = 4;
        if face.is_monospaced() {
            flags |= 1;
        }
        if face.is_italic() || italic != 0.0 {
            flags |= 64;
        }
        (bbox_1000(face), ascent, descent, cap, italic, flags)
    })
}

/// The five objects of document font `font_index`, numbered from `first`.
/// The glyphs come from `used` (their charstrings were built as they were
/// shown); `face` only gives the descriptor's metrics and `.notdef`, and
/// is `None` only if the face vanished, which the registry never does.
pub(crate) fn font_objects(
    face: Option<&Face>,
    ps_name: &str,
    font_index: usize,
    used: &UsedGlyphs,
    first: usize,
) -> [Vec<u8>; OBJECTS_PER_FONT] {
    let cids: Vec<u16> = used.cids.keys().copied().collect();
    let base = format!("{}+{}", subset_tag(font_index, &cids), name_token(ps_name));
    let (cidfont, descriptor, program, cmap) = (first + 1, first + 2, first + 3, first + 4);
    let type0_obj = format!(
        "<< /Type /Font /Subtype /Type0 /BaseFont /{base} /Encoding /Identity-H \
         /DescendantFonts [{cidfont} 0 R] /ToUnicode {cmap} 0 R >>"
    )
    .into_bytes();
    let cid_obj = format!(
        "<< /Type /Font /Subtype /CIDFontType0 /BaseFont /{base} \
         /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
         /FontDescriptor {descriptor} 0 R /DW 0 /W {} >>",
        widths(&used.cids)
    )
    .into_bytes();
    let (bbox, ascent, descent, cap, italic, flags) = face
        .and_then(metrics)
        .unwrap_or(([0; 4], 0.0, 0.0, 0.0, 0.0, 4));
    // `.notdef` is never shown (glyph 0 keeps its outline), so an empty
    // one is as good as the face's when the face cannot give it.
    let notdef = face
        .and_then(|f| charstring(f, 0))
        .unwrap_or_else(|| vec![14]);
    let glyphs: Vec<(u16, &[u8])> = used
        .cids
        .iter()
        .map(|(&cid, g)| (cid, g.charstring.as_slice()))
        .collect();
    let cff = cid_cff(&base, bbox, notdef, &glyphs);
    let descriptor_obj = format!(
        "<< /Type /FontDescriptor /FontName /{base} /Flags {flags} /FontBBox [{} {} {} {}] \
         /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /FontFile3 {program} 0 R >>",
        bbox[0],
        bbox[1],
        bbox[2],
        bbox[3],
        num(italic, 2),
        num(ascent, 0),
        num(descent, 0),
        num(cap, 0),
    )
    .into_bytes();
    let program_obj = stream("/Subtype /CIDFontType0C ", &cff);
    let cmap_obj = stream("", to_unicode(&used.cids).as_bytes());
    [type0_obj, cid_obj, descriptor_obj, program_obj, cmap_obj]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(ch: char, advance: f64) -> CidGlyph {
        CidGlyph {
            ch,
            advance,
            charstring: vec![14],
        }
    }

    #[test]
    fn operands_take_the_short_forms_and_fixed_for_fractions() {
        let enc = |v: i64| {
            let mut o = Vec::new();
            assert!(push_operand(&mut o, v));
            o
        };
        assert_eq!(enc(0), vec![139]);
        assert_eq!(enc(107 * FIXED_ONE), vec![246]);
        assert_eq!(enc(108 * FIXED_ONE), vec![247, 0]);
        assert_eq!(enc(-1131 * FIXED_ONE), vec![254, 255]);
        assert_eq!(enc(2000 * FIXED_ONE), vec![28, 0x07, 0xD0]);
        assert_eq!(enc(FIXED_ONE / 2), vec![255, 0, 0, 0x80, 0]);
        assert_eq!(enc(-32768 * FIXED_ONE), vec![28, 0x80, 0]);
        // Past what a Type 2 number can say: refused, never clamped.
        assert!(!push_operand(&mut Vec::new(), 32768 * FIXED_ONE));
        assert!(!push_operand(&mut Vec::new(), -32769 * FIXED_ONE));
    }

    /// Codex review, #1193: a 16-unit em with a 1024-unit rectangle needs
    /// 64000-unit deltas at 1000/em. Clamped, it drew at half size; now the
    /// glyph is refused and keeps its outline.
    #[test]
    fn a_delta_past_the_type2_range_refuses_the_glyph() {
        let mut cs = Charstring::new(16);
        cs.move_to(0.0, 0.0);
        cs.line_to(1024.0, 0.0);
        cs.line_to(1024.0, 1024.0);
        assert!(cs.finish().is_none());
        let mut cs = Charstring::new(16);
        cs.move_to(0.0, 0.0);
        cs.line_to(500.0, 0.0);
        assert!(cs.finish().is_some());
    }

    #[test]
    fn a_charstring_past_65535_bytes_refuses_the_glyph() {
        let mut cs = Charstring::new(1000);
        cs.move_to(0.0, 0.0);
        for i in 0..22_000 {
            cs.line_to(if i % 2 == 0 { 1.0 } else { 0.0 }, 0.0);
        }
        assert!(cs.finish().is_none());
        let mut cs = Charstring::new(1000);
        cs.move_to(0.0, 0.0);
        for i in 0..20_000 {
            cs.line_to(if i % 2 == 0 { 1.0 } else { 0.0 }, 0.0);
        }
        let out = cs.finish().expect("fits");
        assert!(out.len() <= MAX_CHARSTRING && out.len() > 60_000);
    }

    #[test]
    fn an_index_holds_its_items_with_one_based_offsets() {
        let i = index(&[b"ab".to_vec(), b"c".to_vec()]);
        assert_eq!(i, vec![0, 2, 1, 1, 3, 4, b'a', b'b', b'c']);
        assert_eq!(index(&[]), vec![0, 0]);
    }

    /// The offset size follows the last offset (data length + 1) across
    /// each byte boundary.
    #[test]
    fn index_offset_size_switches_at_the_byte_boundaries() {
        for (len, size) in [
            (0xFE, 1),
            (0xFF, 2),
            (0xFFFE, 2),
            (0xFFFF, 3),
            (0xFF_FFFE, 3),
            (0xFF_FFFF, 4),
        ] {
            let i = index(&[vec![7u8; len]]);
            let size: usize = size;
            assert_eq!(usize::from(i[2]), size, "data length {len:#x}");
            let last = i[3 + size..3 + 2 * size]
                .iter()
                .fold(0usize, |a, &b| a << 8 | usize::from(b));
            assert_eq!(last, len + 1);
            assert_eq!(i.len(), 3 + 2 * size + len);
        }
    }

    #[test]
    fn widths_group_consecutive_cids() {
        let mut g = BTreeMap::new();
        g.insert(3, glyph('a', 500.0));
        g.insert(4, glyph('b', 250.5));
        g.insert(9, glyph('c', 1000.0));
        assert_eq!(widths(&g), "[ 3 [500 250.5] 9 [1000] ]");
    }

    #[test]
    fn to_unicode_writes_surrogate_pairs_beyond_the_bmp() {
        let mut g = BTreeMap::new();
        g.insert(0x41, glyph('\u{1F600}', 0.0));
        g.insert(0x42, glyph('\u{10FFFF}', 0.0));
        let cmap = to_unicode(&g);
        assert!(cmap.contains("<0041> <D83DDE00>"), "{cmap}");
        assert!(cmap.contains("<0042> <DBFFDFFF>"), "{cmap}");
    }

    #[test]
    fn to_unicode_blocks_hold_at_most_100_entries() {
        let mut g = BTreeMap::new();
        for cid in 1..=250u16 {
            g.insert(cid, glyph('x', 0.0));
        }
        let cmap = to_unicode(&g);
        assert_eq!(cmap.matches("100 beginbfchar").count(), 2, "{cmap}");
        assert_eq!(cmap.matches("50 beginbfchar").count(), 1);
        assert_eq!(cmap.matches("endbfchar").count(), 3);
    }

    #[test]
    fn the_subset_tag_is_stable_and_six_capitals() {
        let t = subset_tag(2, &[3, 4, 5]);
        assert_eq!(t, subset_tag(2, &[3, 4, 5]));
        assert_ne!(t, subset_tag(2, &[3, 4]));
        assert_eq!(t.len(), 6);
        assert!(t.chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn fstype_decides_embedding() {
        let ok = fs_type_allows;
        assert!(ok(0x0000));
        assert!(ok(0x0004));
        assert!(ok(0x000C));
        assert!(!ok(0x0002));
        assert!(!ok(0x0100));
        assert!(!ok(0x0208));
    }

    /// A CFF INDEX at `at`: its items and the offset just past it.
    fn read_index(b: &[u8], at: usize) -> (Vec<&[u8]>, usize) {
        let count = usize::from(u16::from_be_bytes([b[at], b[at + 1]]));
        if count == 0 {
            return (Vec::new(), at + 2);
        }
        let os = usize::from(b[at + 2]);
        let off = |i: usize| {
            let p = at + 3 + i * os;
            b[p..p + os]
                .iter()
                .fold(0usize, |a, &x| a << 8 | usize::from(x))
        };
        let data = at + 3 + (count + 1) * os - 1;
        let items = (0..count)
            .map(|i| &b[data + off(i)..data + off(i + 1)])
            .collect();
        (items, data + off(count))
    }

    /// A DICT as (operands, operator) pairs; a two-byte operator is
    /// 1200 + its second byte.
    fn read_dict(d: &[u8]) -> Vec<(Vec<i64>, u16)> {
        let (mut out, mut ops, mut i) = (Vec::new(), Vec::new(), 0);
        while i < d.len() {
            let b0 = d[i];
            match b0 {
                0..=21 => {
                    let op = if b0 == 12 {
                        i += 1;
                        1200 + u16::from(d[i])
                    } else {
                        u16::from(b0)
                    };
                    out.push((std::mem::take(&mut ops), op));
                    i += 1;
                }
                28 => {
                    ops.push(i64::from(i16::from_be_bytes([d[i + 1], d[i + 2]])));
                    i += 3;
                }
                29 => {
                    ops.push(i64::from(i32::from_be_bytes([
                        d[i + 1],
                        d[i + 2],
                        d[i + 3],
                        d[i + 4],
                    ])));
                    i += 5;
                }
                32..=246 => {
                    ops.push(i64::from(b0) - 139);
                    i += 1;
                }
                247..=250 => {
                    ops.push((i64::from(b0) - 247) * 256 + i64::from(d[i + 1]) + 108);
                    i += 2;
                }
                251..=254 => {
                    ops.push(-(i64::from(b0) - 251) * 256 - i64::from(d[i + 1]) - 108);
                    i += 2;
                }
                _ => panic!("unexpected DICT byte {b0}"),
            }
        }
        out
    }

    /// The CFF's structure, read back: ROS first, CIDCount, no FontMatrix
    /// anywhere (the 0.001 default), charset mapping GIDs to CIDs, FDSelect
    /// format 3 putting every glyph in FD 0, one FD whose Private DICT is
    /// where it says.
    #[test]
    fn the_cid_keyed_cff_reads_back() {
        let cs_a: &[u8] = &[139, 139, 21, 14];
        let cs_b: &[u8] = &[14];
        let cff = cid_cff(
            "ABCDEF+Test",
            [-10, -20, 900, 800],
            vec![14],
            &[(5, cs_a), (300, cs_b)],
        );
        assert_eq!(&cff[..4], &[1, 0, 4, 4]);
        let (names, at) = read_index(&cff, 4);
        assert_eq!(names, vec![b"ABCDEF+Test".as_slice()]);
        let (tops, at) = read_index(&cff, at);
        let top = read_dict(tops[0]);
        assert_eq!(top[0], (vec![391, 392, 0], 1230), "ROS first");
        let get = |op: u16| top.iter().find(|e| e.1 == op).map(|e| e.0.clone());
        assert_eq!(get(1234), Some(vec![301]), "CIDCount");
        assert_eq!(get(5), Some(vec![-10, -20, 900, 800]), "FontBBox");
        assert_eq!(get(1207), None, "no Top DICT FontMatrix");
        let (strings, at) = read_index(&cff, at);
        assert_eq!(strings, vec![b"Adobe".as_slice(), b"Identity".as_slice()]);
        let (gsubrs, _) = read_index(&cff, at);
        assert!(gsubrs.is_empty());

        let off = |op: u16| usize::try_from(get(op).expect("offset")[0]).expect("offset");
        assert_eq!(&cff[off(15)..off(15) + 5], &[0, 0, 5, 1, 44], "charset");
        assert_eq!(
            &cff[off(1237)..off(1237) + 8],
            &[3, 0, 1, 0, 0, 0, 0, 3],
            "FDSelect format 3, one range, sentinel 3"
        );
        let (charstrings, _) = read_index(&cff, off(17));
        assert_eq!(charstrings, vec![[14u8].as_slice(), cs_a, cs_b]);
        let (fds, _) = read_index(&cff, off(1236));
        assert_eq!(fds.len(), 1);
        let fd = read_dict(fds[0]);
        assert!(fd.iter().all(|e| e.1 != 1207), "no FD FontMatrix");
        let private = fd.iter().find(|e| e.1 == 18).expect("Private").0.clone();
        let (size, at) = (private[0] as usize, private[1] as usize);
        assert_eq!(&cff[at..at + size], &[139, 20, 139, 21]);
        assert_eq!(at + size, cff.len());
    }
}
