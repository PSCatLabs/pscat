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
//! TrueType quadratics become cubics exactly (control points at 2/3).
//! Outlines are scaled to the CFF's default 1000-unit em (FontMatrix
//! 0.001, written nowhere, so no Top DICT/FDArray matrix composition can
//! disagree between viewers). A coordinate that is not a whole unit is
//! written as a 16.16 fixed number, and each delta is taken from the
//! previous *rounded* point, so rounding never drifts along a contour.
//! Hints are not carried; pscat's own raster is unhinted too.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// What one embedded font needs from the recorder: the glyphs it used,
/// each with the character it stands for (first one seen) and its advance
/// in 1/1000 em.
#[derive(Default)]
pub(crate) struct UsedGlyphs {
    pub(crate) glyphs: BTreeMap<u16, (char, f64)>,
}

/// A glyph's advance in 1/1000 em, rounded to what the PDF writes, so the
/// `/W` entry and the `TJ` adjustments computed against it agree exactly.
pub(crate) fn advance_1000(face: &Face, gid: u16) -> f64 {
    let upem = f64::from(face.units_per_em().max(1));
    let adv = face
        .glyph_hor_advance(GlyphId(gid))
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
    let Some(os2) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2")) else {
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
}

const FIXED_ONE: i64 = 65536;

fn fixed(v: f64) -> i64 {
    // A hostile font's coordinates may be anything; a finite clamp keeps
    // the arithmetic below in range (the glyph just draws wrong).
    let v = if v.is_finite() { v } else { 0.0 };
    (v * FIXED_ONE as f64).round().clamp(-1e15, 1e15) as i64
}

/// A charstring operand: a short integer form when the value is whole and
/// small, else a 16.16 fixed number (operator 255).
fn push_operand(out: &mut Vec<u8>, d: i64) {
    if d % FIXED_ONE == 0 {
        let v = d / FIXED_ONE;
        if (-107..=107).contains(&v) {
            out.push((v + 139) as u8);
            return;
        }
        if (108..=1131).contains(&v) {
            let w = v - 108;
            out.extend_from_slice(&[((w >> 8) + 247) as u8, (w & 0xFF) as u8]);
            return;
        }
        if (-1131..=-108).contains(&v) {
            let w = -v - 108;
            out.extend_from_slice(&[((w >> 8) + 251) as u8, (w & 0xFF) as u8]);
            return;
        }
        if (-32768..=32767).contains(&v) {
            out.push(28);
            out.extend_from_slice(&(v as i16).to_be_bytes());
            return;
        }
    }
    let f = d.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    out.push(255);
    out.extend_from_slice(&f.to_be_bytes());
}

impl Charstring {
    fn point(&mut self, x: f64, y: f64) {
        let p = (fixed(x), fixed(y));
        push_operand(&mut self.out, p.0 - self.cur.0);
        push_operand(&mut self.out, p.1 - self.cur.1);
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

/// The charstring for `gid`. No width operand: PDF takes advances from the
/// CIDFont's `/W`, which overrides the font program's.
fn charstring(face: &Face, gid: u16) -> Vec<u8> {
    let mut cs = Charstring {
        out: Vec::new(),
        scale: 1000.0 / f64::from(face.units_per_em().max(1)),
        cur: (0, 0),
        last: (0.0, 0.0),
    };
    // An outline the parser rejects halfway (a hostile font) is dropped
    // whole rather than left half-drawn.
    if face.outline_glyph(GlyphId(gid), &mut cs).is_none() {
        cs.out.clear();
    }
    cs.out.push(14); // endchar
    cs.out
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
    let r = face.global_bounding_box();
    let c = |v: f64| v.clamp(-1e6, 1e6) as i32;
    [
        c((f64::from(r.x_min) * k).floor()),
        c((f64::from(r.y_min) * k).floor()),
        c((f64::from(r.x_max) * k).ceil()),
        c((f64::from(r.y_max) * k).ceil()),
    ]
}

/// A bare CID-keyed CFF holding `.notdef` and `gids` (ascending, no 0),
/// registry-ordering-supplement Adobe-Identity-0 to match the CIDFont's
/// `/CIDSystemInfo`, CID = source glyph id.
pub(crate) fn cid_cff(face: &Face, name: &str, gids: &[u16]) -> Vec<u8> {
    let header = [1u8, 0, 4, 4];
    let name_index = index(&[name.as_bytes().to_vec()]);
    // Custom strings start at SID 391.
    let strings = index(&[b"Adobe".to_vec(), b"Identity".to_vec()]);
    let gsubrs = index(&[]);

    let mut charstrings = Vec::with_capacity(gids.len() + 1);
    charstrings.push(charstring(face, 0));
    for &g in gids {
        charstrings.push(charstring(face, g));
    }
    let charstrings = index(&charstrings);
    let nglyphs = u16::try_from(gids.len() + 1).unwrap_or(u16::MAX);

    // charset format 0: the CID of every glyph after .notdef.
    let mut charset = vec![0u8];
    for &g in gids {
        charset.extend_from_slice(&g.to_be_bytes());
    }
    // FDSelect format 3: one range, every glyph in FD 0.
    let mut fdselect = vec![3u8, 0, 1, 0, 0, 0];
    fdselect.extend_from_slice(&nglyphs.to_be_bytes());
    let private = vec![139u8, 20, 139, 21]; // defaultWidthX 0, nominalWidthX 0
    let cid_count = i32::from(gids.last().copied().unwrap_or(0)) + 1;
    let bbox = bbox_1000(face);

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

/// The `/ToUnicode` CMap: each CID to its character, UTF-16BE.
fn to_unicode(glyphs: &BTreeMap<u16, (char, f64)>) -> String {
    let mut s = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let entries: Vec<(&u16, &(char, f64))> = glyphs.iter().collect();
    // At most 100 entries per bfchar block (the CMap format's limit).
    for chunk in entries.chunks(100) {
        let _ = writeln!(s, "{} beginbfchar", chunk.len());
        for (gid, (ch, _)) in chunk {
            let mut units = [0u16; 2];
            let dst: String = ch
                .encode_utf16(&mut units)
                .iter()
                .map(|u| format!("{u:04X}"))
                .collect();
            let _ = writeln!(s, "<{gid:04X}> <{dst}>");
        }
        s.push_str("endbfchar\n");
    }
    s.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    s
}

/// The `/W` array: runs of consecutive CIDs share one bracket.
fn widths(glyphs: &BTreeMap<u16, (char, f64)>) -> String {
    let mut s = String::from("[");
    let mut prev: Option<u16> = None;
    for (&gid, &(_, w)) in glyphs {
        if prev.is_some_and(|p| u32::from(p) + 1 == u32::from(gid)) {
            s.push(' ');
        } else {
            if prev.is_some() {
                s.push(']');
            }
            let _ = write!(s, " {gid} [");
        }
        s.push_str(&num(w, 3));
        prev = Some(gid);
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

/// The five objects of document font `font_index`, numbered from `first`.
/// `face` is `None` only if the face vanished, which the registry never
/// does; the font then has empty glyphs rather than a broken file.
pub(crate) fn font_objects(
    face: Option<&Face>,
    ps_name: &str,
    font_index: usize,
    used: &UsedGlyphs,
    first: usize,
) -> [Vec<u8>; OBJECTS_PER_FONT] {
    let gids: Vec<u16> = used.glyphs.keys().copied().filter(|&g| g != 0).collect();
    let base = format!("{}+{}", subset_tag(font_index, &gids), name_token(ps_name));
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
        widths(&used.glyphs)
    )
    .into_bytes();
    let (bbox, ascent, descent, cap, italic, flags, cff) = match face {
        Some(face) => {
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
            (
                bbox_1000(face),
                ascent,
                descent,
                cap,
                italic,
                flags,
                cid_cff(face, &base, &gids),
            )
        }
        None => ([0; 4], 0.0, 0.0, 0.0, 0.0, 4, Vec::new()),
    };
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
    let cmap_obj = stream("", to_unicode(&used.glyphs).as_bytes());
    [type0_obj, cid_obj, descriptor_obj, program_obj, cmap_obj]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operands_take_the_short_forms_and_fixed_for_fractions() {
        let enc = |v: i64| {
            let mut o = Vec::new();
            push_operand(&mut o, v);
            o
        };
        assert_eq!(enc(0), vec![139]);
        assert_eq!(enc(107 * FIXED_ONE), vec![246]);
        assert_eq!(enc(108 * FIXED_ONE), vec![247, 0]);
        assert_eq!(enc(-1131 * FIXED_ONE), vec![254, 255]);
        assert_eq!(enc(2000 * FIXED_ONE), vec![28, 0x07, 0xD0]);
        assert_eq!(enc(FIXED_ONE / 2), vec![255, 0, 0, 0x80, 0]);
    }

    #[test]
    fn an_index_holds_its_items_with_one_based_offsets() {
        let i = index(&[b"ab".to_vec(), b"c".to_vec()]);
        assert_eq!(i, vec![0, 2, 1, 1, 3, 4, b'a', b'b', b'c']);
        assert_eq!(index(&[]), vec![0, 0]);
    }

    #[test]
    fn widths_group_consecutive_cids() {
        let mut g = BTreeMap::new();
        g.insert(3, ('a', 500.0));
        g.insert(4, ('b', 250.5));
        g.insert(9, ('c', 1000.0));
        assert_eq!(widths(&g), "[ 3 [500 250.5] 9 [1000] ]");
    }

    #[test]
    fn to_unicode_writes_surrogate_pairs_beyond_the_bmp() {
        let mut g = BTreeMap::new();
        g.insert(0x41, ('\u{1F600}', 0.0));
        assert!(to_unicode(&g).contains("<0041> <D83DDE00>"));
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
}
