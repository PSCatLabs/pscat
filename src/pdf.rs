//! PDF export — Stage 9 task 2.
//!
//! **Design note** (the [opus+review] decision the roadmap asked
//! for): three candidates were weighed. *Re-execution* (run the
//! program once per output target) loses — programs have side
//! effects, `rand` streams, and file reads that must not happen
//! twice. A *retained display list* (record neutral paint ops, then
//! serialize to SVG/PDF/…) is the textbook answer but adds a third
//! representation of every op; with only two streaming consumers it
//! buys nothing yet. Chosen: the **paint-pipeline mirror** the SVG
//! exporter proved out — a recorder hooked at the same six seams in
//! `Gfx`, streaming device-space ops straight into PDF content
//! syntax. If a third export target ever appears, promote the shared
//! hook calls into a display list and make SVG/PDF serializers of it.
//!
//! Mapping notes:
//! - Every page's content stream starts with `1 0 0 -1 0 H cm`, so
//!   the interpreter's y-down device coordinates are used verbatim.
//! - Each element is wrapped in its own `q … Q`, so the fill/stroke
//!   state it sets (colour, width, dash, `ExtGState`) can't leak into
//!   the next one. Consecutive elements that carry the *same* clip
//!   chain (same `Rc`, i.e. no `clip` between them) additionally share
//!   one outer `q <clip> … Q` group, so a heavily clipped page writes
//!   the clip path once per run rather than once per element (issue
//!   #219: one touched stroke was 36 KB → 39.6 MB before). Unclipped
//!   elements get no outer group, exactly as before. The group is
//!   closed lazily: on a different chain, on `erase`, and in the page
//!   snapshots taken by `end_page`/`finish` (the live stream keeps its
//!   group open, which `copypage` needs).
//! - Each page's content stream is Flate-compressed (`/Filter
//!   /FlateDecode`), like the image XObjects: the operators are very
//!   repetitive text, and a stroke-heavy piece (`gallery/alpine_lake`:
//!   ~166k fills, some 1 MB each) shrinks ~3.5x (258 MB → 74 MB, issue
//!   #244). Anything that greps the raw PDF for operators must inflate
//!   first (`tests/common/mod.rs::pdf_text`).
//! - Text arrives as glyph outlines (same as SVG); no fonts are
//!   embedded. With [`PdfText::Text`] (`pscat --pdf-text`, #1193) a glyph
//!   of an embeddable TrueType/OpenType face is written as text instead:
//!   one `BT … TJ ET` per run of glyphs on a baseline, in a Type 0 font
//!   with an embedded CFF subset and a `/ToUnicode` map
//!   (`src/pdf_font.rs`). A run is held open while glyphs join it and is
//!   written before the next mark of any kind (every writer goes through
//!   `enter_clip`), so paint order holds.
//! - Images become Flate-compressed RGB XObjects; imagemasks become
//!   1-bit `/ImageMask` stencils painted in the current fill color or
//!   pattern (#1299),
//!   which is precisely PostScript's own semantics.
//! - `shfill` becomes the `sh` operator on a `/Shading` resource
//!   (issue #1028; before it, a flat fill in the ramp's average colour).
//!   The shading keeps its Coords in user space and the CTM goes in a
//!   `cm` (the page's flip `cm` is already in force); its colour ramp is
//!   the same pre-sampled RGB stop list the raster and SVG paint, written
//!   as a FunctionType 2 (one leg) or 3 (stitching N-1 legs) function, so
//!   all three outputs interpolate the same stops. Function and shading
//!   objects are document-global and deduped by content.
//!
//! Verified end-to-end by rasterizing the output with gs and
//! block-comparing against our own canvas (tests/pdf.rs).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{self, Write as _};
use std::rc::Rc;

use thiserror::Error;

use tiny_skia::FillRule;

use crate::gfx::{ClipNode, Composite};
use crate::limits::LimitProbe;
use crate::pattern::Ink;
use crate::pdf_font::{OBJECTS_PER_FONT, UsedGlyphs};
use crate::svg::Chain;
use crate::textrun::{GlyphText, Placement, TextRun};

/// How PDF output writes text (#1193).
///
/// Set with `Gfx::set_pdf_text` (`interp.set_pdf_text(..)`) or
/// `pscat --pdf-text`. Only PDF output reads it: PNG and SVG are unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PdfText {
    /// Every glyph is its outline, a filled path: no fonts in the file. The
    /// default, and the only form before #1193.
    #[default]
    Paths,
    /// Glyphs of TrueType and OpenType faces whose licence bits allow
    /// embedding (the bundled Liberation, TeX Gyre, URW and Noto faces, the
    /// catalog, font packs) become PDF text in an embedded subset of the
    /// face, with a `/ToUnicode` map: selectable, searchable and
    /// extractable, drawn from the same outlines. Type 1 and Type 3 fonts,
    /// `charpath`, and glyphs with no Unicode name keep their outlines.
    Text,
}

/// The open text run: glyphs on one baseline in one face, size, frame,
/// paint and clip, written as one text object.
struct PdfRun {
    text: TextRun,
    /// The run's em → device matrix (linear part), which is its `Tm`.
    em: [f64; 4],
    /// The first glyph's device origin.
    origin: (f64, f64),
    /// Document font index (`/F<n>`).
    font: usize,
    /// The `ExtGState` selection, empty for the default.
    gs: String,
    /// Each glyph's advance in 1/1000 em, as `/W` gives it.
    widths: Vec<f64>,
    /// Each glyph's character code (its CID in the embedded font).
    cids: Vec<u16>,
}

pub struct PdfRecorder {
    width: u32,
    height: u32,
    /// Current page's content stream (PDF operators, text form).
    content: String,
    /// Finished pages' content streams.
    pages: Vec<String>,
    /// Parallel to `pages`: each finished page's size, since
    /// `setpagedevice` can change it between pages (#909).
    page_sizes: Vec<(u32, u32)>,
    /// Image XObjects (raw stream bytes, already entity-complete),
    /// document-global; pages reference them by index.
    images: Vec<PdfImage>,
    /// Which images each finished page references.
    page_images: Vec<Vec<usize>>,
    current_images: Vec<usize>,
    /// `ExtGState` dict bodies (issue #47's alpha/blend export), e.g.
    /// `/ca 0.32 /CA 0.32 /BM /Multiply`, deduped by content and
    /// document-global; pages reference them by index, exactly like
    /// `images` above. Stays empty for any program that never touches
    /// `setalpha`/`setblendmode`, which is what keeps those documents
    /// byte-identical to what this exporter produced before.
    gstates: Vec<String>,
    /// Which gstates each finished page references.
    page_gstates: Vec<Vec<usize>>,
    current_gstates: Vec<usize>,
    /// Document `/Info` fields (issue #8) — from `%%Title:`/`%%For:`
    /// DSC header comments via `scan_document_info`, when present.
    title: Option<String>,
    author: Option<String>,
    /// The clip chain of the currently open shared `q <clip> …` group in
    /// `content`, if one is open. Compared by `Rc` identity; held here,
    /// so the address can't be recycled while the group is open.
    open_clip: Option<Rc<ClipNode>>,
    /// Function dict bodies for shadings (#1028), deduped by content so a
    /// ramp reused by many `shfill`s is written once.
    functions: Vec<String>,
    /// `functions` by body, so interning is one hash lookup: a generative
    /// piece may `shfill` thousands of times.
    function_ids: HashMap<String, usize>,
    /// The function index each stop list (as bits) already produced, so a
    /// repeated ramp skips building and merging its function text.
    stops_ids: HashMap<Vec<[u32; 4]>, usize>,
    /// Shading dicts: the body (minus `/Function`) and the index of its
    /// function in `functions`, deduped; pages reference them by index,
    /// like `images`. Empty for a program that never calls `shfill`, so
    /// such documents are byte-identical to what came before.
    shadings: Vec<(String, usize)>,
    shading_ids: HashMap<(String, usize), usize>,
    /// Which shadings each finished page references.
    page_shadings: Vec<Vec<usize>>,
    current_shadings: Vec<usize>,
    /// `current_shadings` as a set, for the per-`shfill` membership test.
    current_shading_set: HashSet<usize>,
    /// `/DOCINFO` pdfmark entries (#1032), in first-set order, a later
    /// mark replacing a key in place. `Title`/`Author` here win over the
    /// DSC ones above; `Producer` never gets here (gs keeps its own too).
    docinfo: Vec<(String, PdfValue)>,
    /// Named objects from `/OBJ` (and forward `{name}` references, which
    /// make an empty dict as gs does), numbered after the shadings.
    named: Vec<NamedObj>,
    named_ids: HashMap<String, usize>,
    /// `/EMBED` attachments: name-tree key (raw bytes) and the file
    /// specification, written into the Catalog's `/EmbeddedFiles`.
    embeds: Vec<(Vec<u8>, PdfValue)>,
    /// Bytes pdfmark has stored so far (stream data, strings, names),
    /// against [`MAX_MARK_BYTES`].
    mark_bytes: usize,
    /// The text run being collected (`--pdf-text`), written before the
    /// next mark. Always `None` without text output.
    run: Option<PdfRun>,
    /// Embedded fonts (#1193): FID and the glyphs used, document-global,
    /// numbered after the named objects. Empty without text output, so
    /// such documents keep every object number they had.
    fonts: Vec<(i64, UsedGlyphs)>,
    font_ids: HashMap<i64, usize>,
    /// Which fonts each finished page references.
    page_fonts: Vec<Vec<usize>>,
    current_fonts: Vec<usize>,
    /// Tiling pattern streams (#1038), document-global, numbered after the
    /// fonts; pages (and patterns) reference them by index, like `images`.
    /// Empty for a program that paints with no pattern, so such documents
    /// keep every object number they had.
    patterns: Vec<PdfPattern>,
    /// Copies of a pattern stream under another `/Matrix` (#1300), for a
    /// pattern painted in a context other than the one it was made in,
    /// by original index and the matrix's bits.
    pattern_variants: HashMap<(usize, [u32; 6]), usize>,
    /// Which patterns each finished page references.
    page_patterns: Vec<Vec<usize>>,
    current_patterns: Vec<usize>,
    /// Whether each finished page paints an uncoloured pattern, which
    /// needs the `[/Pattern /DeviceRGB]` colour space resource.
    page_pattern_cs: Vec<bool>,
    current_pattern_cs: bool,
    /// Recording an uncoloured pattern's content stream (#1038): no
    /// colour operators, and no colour images or shadings, which an
    /// uncoloured pattern may not hold.
    uncolored: bool,
}

/// A `/PatternType 1` pattern stream (#1038): the dictionary's own keys,
/// its content, and the resources the content uses.
#[derive(Clone)]
struct PdfPattern {
    dict: String,
    content: String,
    images: Vec<usize>,
    gstates: Vec<usize>,
    shadings: Vec<usize>,
    fonts: Vec<usize>,
    patterns: Vec<usize>,
    pattern_cs: bool,
}

/// The page state a pattern capture swapped out of the recorder
/// (`PdfRecorder::begin_capture`, #1038).
pub(crate) struct PdfCapture {
    content: String,
    open_clip: Option<Rc<ClipNode>>,
    images: Vec<usize>,
    gstates: Vec<usize>,
    shadings: Vec<usize>,
    shading_set: HashSet<usize>,
    fonts: Vec<usize>,
    patterns: Vec<usize>,
    pattern_cs: bool,
    uncolored: bool,
}

/// A PDF object built by `pdfmark` (#1032): the interpreter converts the
/// PostScript operands into this tree, and `finish` writes it once the
/// object numbers are known. `Ref` is an index into the named objects.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PdfValue {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(String),
    /// A byte string, written as an escaped literal.
    Str(Vec<u8>),
    /// A text string (a `/DOCINFO` value): UTF-8 becomes UTF-16BE, see
    /// [`write_text_bytes`].
    Text(Vec<u8>),
    Array(Vec<PdfValue>),
    Dict(Vec<(String, PdfValue)>),
    /// A `{name}` reference as the operator read it; the recorder turns
    /// it into `Ref` when the value is stored.
    Named(String),
    Ref(usize),
}

/// What `/OBJ /type` asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjKind {
    Dict,
    Array,
    Stream,
}

struct NamedObj {
    kind: ObjKind,
    /// Defined by `/OBJ` (false: only referenced so far).
    defined: bool,
    /// Dict entries, or a stream's dict entries.
    dict: Vec<(String, PdfValue)>,
    items: Vec<PdfValue>,
    data: Vec<u8>,
}

/// The most stream data `/PUT` may add to one document (64 MiB): a
/// decoding filter read to its end can expand without bound.
pub(crate) const MAX_MARK_BYTES: usize = 64 << 20;

/// Why a pdfmark edit of the recorder was refused; the operator maps it
/// to a PostScript error.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MarkError {
    /// Wrong operands for this mark (PostScript `rangecheck`).
    Range,
    /// The object is not of the kind this mark needs (`typecheck`).
    Type,
    /// Past [`MAX_MARK_BYTES`] (`limitcheck`).
    Limit,
}

/// Put `key` in `entries`, replacing an existing entry in place.
fn set_entry(entries: &mut Vec<(String, PdfValue)>, key: String, value: PdfValue) {
    match entries.iter_mut().find(|(k, _)| *k == key) {
        Some(slot) => slot.1 = value,
        None => entries.push((key, value)),
    }
}

impl PdfRecorder {
    pub fn new(width: u32, height: u32) -> Self {
        PdfRecorder {
            width,
            height,
            content: String::new(),
            pages: Vec::new(),
            page_sizes: Vec::new(),
            images: Vec::new(),
            page_images: Vec::new(),
            current_images: Vec::new(),
            gstates: Vec::new(),
            page_gstates: Vec::new(),
            current_gstates: Vec::new(),
            title: None,
            author: None,
            open_clip: None,
            functions: Vec::new(),
            function_ids: HashMap::new(),
            stops_ids: HashMap::new(),
            shadings: Vec::new(),
            shading_ids: HashMap::new(),
            page_shadings: Vec::new(),
            current_shadings: Vec::new(),
            current_shading_set: HashSet::new(),
            docinfo: Vec::new(),
            named: Vec::new(),
            named_ids: HashMap::new(),
            embeds: Vec::new(),
            mark_bytes: 0,
            run: None,
            fonts: Vec::new(),
            font_ids: HashMap::new(),
            page_fonts: Vec::new(),
            current_fonts: Vec::new(),
            patterns: Vec::new(),
            pattern_variants: HashMap::new(),
            page_patterns: Vec::new(),
            current_patterns: Vec::new(),
            page_pattern_cs: Vec::new(),
            current_pattern_cs: false,
            uncolored: false,
        }
    }

    /// Start recording a pattern's content stream (#1038): the page's
    /// stream and resource lists are set aside, and marks go to an empty
    /// stream until [`end_capture`](Self::end_capture).
    pub(crate) fn begin_capture(&mut self, uncolored: bool) -> PdfCapture {
        self.flush_text();
        PdfCapture {
            content: std::mem::take(&mut self.content),
            open_clip: self.open_clip.take(),
            images: std::mem::take(&mut self.current_images),
            gstates: std::mem::take(&mut self.current_gstates),
            shadings: std::mem::take(&mut self.current_shadings),
            shading_set: std::mem::take(&mut self.current_shading_set),
            fonts: std::mem::take(&mut self.current_fonts),
            patterns: std::mem::take(&mut self.current_patterns),
            pattern_cs: std::mem::take(&mut self.current_pattern_cs),
            uncolored: std::mem::replace(&mut self.uncolored, uncolored),
        }
    }

    /// Put the page state back; what was captured comes back as a
    /// pattern (its content and resources).
    fn restore_capture(&mut self, saved: PdfCapture) -> PdfPattern {
        self.flush_text();
        self.close_clip();
        self.current_shading_set = saved.shading_set;
        self.open_clip = saved.open_clip;
        self.uncolored = saved.uncolored;
        PdfPattern {
            dict: String::new(),
            content: std::mem::replace(&mut self.content, saved.content),
            images: std::mem::replace(&mut self.current_images, saved.images),
            gstates: std::mem::replace(&mut self.current_gstates, saved.gstates),
            shadings: std::mem::replace(&mut self.current_shadings, saved.shadings),
            fonts: std::mem::replace(&mut self.current_fonts, saved.fonts),
            patterns: std::mem::replace(&mut self.current_patterns, saved.patterns),
            pattern_cs: std::mem::replace(&mut self.current_pattern_cs, saved.pattern_cs),
        }
    }

    /// The pattern's content is complete: store it as a pattern stream and
    /// return its index. `matrix` maps pattern space to the default space
    /// of the stream that will paint with it; `to_pattern` maps the
    /// captured content (in the capture canvas's coordinates) to pattern
    /// space, written as the stream's first `cm`.
    pub(crate) fn end_capture(
        &mut self,
        saved: PdfCapture,
        spec: &crate::pattern::TileSpec,
        matrix: tiny_skia::Transform,
        to_pattern: tiny_skia::Transform,
    ) -> usize {
        let mut pat = self.restore_capture(saved);
        let num = |v: f64| p(v as f32);
        let [x0, y0, x1, y1] = spec.bbox;
        pat.dict = format!(
            "/Type /Pattern /PatternType 1 /PaintType {} /TilingType {} /BBox [{} {} {} {}] \
             /XStep {} /YStep {} /Matrix [{}]",
            if spec.uncolored { 2 } else { 1 },
            spec.tiling_type,
            num(x0),
            num(y0),
            num(x1),
            num(y1),
            num(spec.xstep),
            num(spec.ystep),
            matrix_text(matrix),
        );
        pat.content = format!("{} cm\n{}", matrix_text(to_pattern), pat.content);
        self.patterns.push(pat);
        self.patterns.len() - 1
    }

    /// Pattern `idx` under `matrix` instead of its own `/Matrix` (#1300):
    /// the same content and resources in a new pattern stream, made once
    /// per matrix. A pattern's `/Matrix` maps to the default space of the
    /// stream that paints with it, so one used in another context needs
    /// its own.
    fn pattern_variant(&mut self, idx: usize, matrix: tiny_skia::Transform) -> usize {
        let bits = [
            matrix.sx, matrix.ky, matrix.kx, matrix.sy, matrix.tx, matrix.ty,
        ]
        .map(f32::to_bits);
        if let Some(&v) = self.pattern_variants.get(&(idx, bits)) {
            return v;
        }
        let mut pat = self.patterns[idx].clone();
        let keep = pat.dict.rfind(" /Matrix [").unwrap_or(pat.dict.len());
        pat.dict.truncate(keep);
        let _ = write!(pat.dict, " /Matrix [{}]", matrix_text(matrix));
        self.patterns.push(pat);
        let v = self.patterns.len() - 1;
        self.pattern_variants.insert((idx, bits), v);
        v
    }

    /// The pattern's `PaintProc` was unwound: put the page state back and
    /// drop what was captured.
    pub(crate) fn abort_capture(&mut self, saved: PdfCapture) {
        let _ = self.restore_capture(saved);
    }

    /// The colour-setting operators for `ink`, fill (`rg`, `cs`/`scn`) or
    /// stroke (`RG`, `CS`/`SCN`), with a trailing space; empty in an
    /// uncoloured pattern's content, which takes no colour.
    fn ink_ops(&mut self, ink: &Ink, stroking: bool) -> String {
        if self.uncolored {
            return String::new();
        }
        match ink {
            Ink::Rgb(rgb) => format!(
                "{} {} {} {} ",
                f(rgb.0),
                f(rgb.1),
                f(rgb.2),
                if stroking { "RG" } else { "rg" }
            ),
            Ink::Pattern(tile, rgb, place) => {
                let Some(idx) = tile.pdf else {
                    return String::new();
                };
                let idx = match place {
                    Some(p) => self.pattern_variant(idx, p.pdf_matrix),
                    None => idx,
                };
                if !self.current_patterns.contains(&idx) {
                    self.current_patterns.push(idx);
                }
                let (cs, scn) = if stroking {
                    ("CS", "SCN")
                } else {
                    ("cs", "scn")
                };
                if tile.uncolored {
                    self.current_pattern_cs = true;
                    format!(
                        "/PCS {cs} {} {} {} /P{idx} {scn} ",
                        f(rgb.0),
                        f(rgb.1),
                        f(rgb.2)
                    )
                } else {
                    format!("/Pattern {cs} /P{idx} {scn} ")
                }
            }
        }
    }

    /// `/DOCINFO` (#1032): set document-info entries. `Producer` is
    /// dropped, as gs's pdfwrite does: the producer is always pscat.
    pub(crate) fn docinfo(&mut self, mut entries: Vec<(String, PdfValue)>) {
        self.resolve_entries(&mut entries);
        for (key, value) in entries {
            if key != "Producer" {
                set_entry(&mut self.docinfo, key, value);
            }
        }
    }

    /// The index of named object `name`, creating an empty, undefined
    /// dict for a forward reference (gs does the same).
    pub(crate) fn named_ref(&mut self, name: &str) -> usize {
        if let Some(&i) = self.named_ids.get(name) {
            return i;
        }
        let i = self.named.len();
        self.named.push(NamedObj {
            kind: ObjKind::Dict,
            defined: false,
            dict: Vec::new(),
            items: Vec::new(),
            data: Vec::new(),
        });
        self.named_ids.insert(name.to_string(), i);
        i
    }

    /// Turn every `Named` reference in `v` into a `Ref`.
    fn resolve(&mut self, v: &mut PdfValue) {
        match v {
            PdfValue::Named(name) => {
                let i = self.named_ref(&std::mem::take(name));
                *v = PdfValue::Ref(i);
            }
            PdfValue::Array(items) => items.iter_mut().for_each(|x| self.resolve(x)),
            PdfValue::Dict(entries) => self.resolve_entries(entries),
            _ => {}
        }
    }

    fn resolve_entries(&mut self, entries: &mut [(String, PdfValue)]) {
        for (_, v) in entries.iter_mut() {
            self.resolve(v);
        }
    }

    /// `/OBJ`: define named object `name` as `kind`. Defining a name twice
    /// is a `rangecheck`; an object only referenced so far takes the kind
    /// unless it was already given dict entries and `kind` is not a dict.
    pub(crate) fn define(&mut self, name: &str, kind: ObjKind) -> Result<(), MarkError> {
        let i = self.named_ref(name);
        let obj = &mut self.named[i];
        if obj.defined || (kind != ObjKind::Dict && !obj.dict.is_empty()) {
            return Err(MarkError::Range);
        }
        obj.defined = true;
        obj.kind = kind;
        Ok(())
    }

    /// `/PUT` of dict entries into a dict or a stream's dict. For a stream
    /// `Length` is ignored: it is always computed.
    pub(crate) fn put_dict(
        &mut self,
        obj: usize,
        mut entries: Vec<(String, PdfValue)>,
    ) -> Result<(), MarkError> {
        if self.named[obj].kind == ObjKind::Array {
            return Err(MarkError::Type);
        }
        self.resolve_entries(&mut entries);
        let o = &mut self.named[obj];
        for (key, value) in entries {
            if !(o.kind == ObjKind::Stream && key == "Length") {
                set_entry(&mut o.dict, key, value);
            }
        }
        Ok(())
    }

    /// `/PUT` of data onto a stream.
    pub(crate) fn put_data(&mut self, obj: usize, data: &[u8]) -> Result<(), MarkError> {
        if self.named[obj].kind != ObjKind::Stream {
            return Err(MarkError::Type);
        }
        let total = self.mark_bytes.saturating_add(data.len());
        if total > MAX_MARK_BYTES {
            return Err(MarkError::Limit);
        }
        self.mark_bytes = total;
        self.named[obj].data.extend_from_slice(data);
        Ok(())
    }

    /// Bytes pdfmark may still store before [`MAX_MARK_BYTES`].
    pub(crate) fn mark_room(&self) -> usize {
        MAX_MARK_BYTES - self.mark_bytes
    }

    /// Count `n` bytes a mark stored as values (strings, names) against
    /// [`MAX_MARK_BYTES`]; the operator checked they fit.
    pub(crate) fn charge(&mut self, n: usize) {
        self.mark_bytes = self.mark_bytes.saturating_add(n).min(MAX_MARK_BYTES);
    }

    /// The kind of named object `name`; a dict if it does not exist yet,
    /// as a reference would make it.
    pub(crate) fn kind_of(&self, name: &str) -> ObjKind {
        self.named_ids
            .get(name)
            .map_or(ObjKind::Dict, |&i| self.named[i].kind)
    }

    /// `/PUT` into an array: element `index`, padding with nulls.
    pub(crate) fn put_index(
        &mut self,
        obj: usize,
        index: usize,
        mut value: PdfValue,
    ) -> Result<(), MarkError> {
        if self.named[obj].kind != ObjKind::Array {
            return Err(MarkError::Type);
        }
        self.resolve(&mut value);
        let o = &mut self.named[obj];
        if index >= o.items.len() {
            o.items.resize(index + 1, PdfValue::Null);
        }
        o.items[index] = value;
        Ok(())
    }

    /// `/APPEND` onto an array.
    pub(crate) fn append(&mut self, obj: usize, mut value: PdfValue) -> Result<(), MarkError> {
        if self.named[obj].kind != ObjKind::Array {
            return Err(MarkError::Type);
        }
        self.resolve(&mut value);
        self.named[obj].items.push(value);
        Ok(())
    }

    /// `/EMBED`: attach the file specification `fs` under `name` in the
    /// document's `/EmbeddedFiles` name tree; a later one of the same name
    /// replaces it.
    pub(crate) fn embed(&mut self, name: Vec<u8>, mut fs: PdfValue) {
        self.resolve(&mut fs);
        match self.embeds.iter_mut().find(|(k, _)| *k == name) {
            Some(slot) => slot.1 = fs,
            None => self.embeds.push((name, fs)),
        }
    }

    /// Set document metadata for the `/Info` dictionary. `None` leaves
    /// that field out entirely (no empty `/Title ()` clutter).
    pub fn set_info(&mut self, title: Option<String>, author: Option<String>) {
        self.title = title;
        self.author = author;
    }

    /// Register this element's alpha/blend as an `ExtGState` and
    /// return the `/GSn gs` operator that selects it — empty for
    /// opaque source-over, so nothing is emitted (and no `/ExtGState`
    /// resource dict appears) unless a program actually asked for it.
    ///
    /// PDF splits what `setalpha` merges: `ca` is the fill (non-stroking)
    /// constant alpha and `CA` the stroking one. pscat's contract is a
    /// single alpha covering both, so both keys get the same value.
    fn gs_ref(&mut self, comp: Composite) -> String {
        if comp.is_default() {
            return String::new();
        }
        let body = format!(
            "/ca {} /CA {} /BM /{}",
            f(comp.alpha),
            f(comp.alpha),
            comp.blend.as_ps_name(),
        );
        let idx = match self.gstates.iter().position(|g| *g == body) {
            Some(i) => i,
            None => {
                self.gstates.push(body);
                self.gstates.len() - 1
            }
        };
        if !self.current_gstates.contains(&idx) {
            self.current_gstates.push(idx);
        }
        format!("/GS{idx} gs ")
    }

    fn clip_prelude(&self, chain: &Chain) -> String {
        let mut out = String::new();
        let mut node: Option<Rc<ClipNode>> = chain.clone();
        while let Some(n) = node {
            let _ = write!(
                out,
                "{} {} n ",
                path_ops(&n.path, true),
                match n.rule {
                    FillRule::EvenOdd => "W*",
                    _ => "W",
                }
            );
            node = n.parent.clone();
        }
        out
    }

    /// Make `content`'s open clip group match `chain` before an element
    /// is written. A clipped chain identical (by `Rc`) to the open one
    /// reuses it; otherwise the open group (if any) is closed and, for a
    /// clipped chain, a new `q <clip>` group opens. Unclipped elements
    /// are written with no outer group, as before.
    fn enter_clip(&mut self, chain: &Chain) {
        // Every writer comes through here first, so an open text run is
        // written before the mark that follows it. (`flush_text` calls back
        // in here with the run already taken, which returns at once.)
        self.flush_text();
        let same = match (&self.open_clip, chain) {
            (Some(open), Some(want)) => Rc::ptr_eq(open, want),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.close_clip();
        if chain.is_some() {
            let clip = self.clip_prelude(chain);
            let _ = writeln!(self.content, "q {clip}");
            self.open_clip = chain.clone();
        }
    }

    fn close_clip(&mut self) {
        if self.open_clip.take().is_some() {
            self.content.push_str("Q\n");
        }
    }

    /// The current page's content with any open text run written and any
    /// open clip group closed, for a finished-page snapshot. The live
    /// `content` is left untouched (`copypage` keeps painting into it), and
    /// the run stays open, so a mid-run read (`pdf_document`) shows the
    /// text so far and later glyphs on the line still join it.
    fn snapshot(&self) -> String {
        let tail = self.snapshot_tail();
        let mut s = String::with_capacity(self.content.len() + tail.len());
        s.push_str(&self.content);
        s.push_str(&tail);
        s
    }

    /// One glyph as text (#1193): continue the open run, or write it and
    /// open a new one. `false` when the glyph cannot be embedded (its
    /// outline does not fit a Type 2 charstring, or the parser panics on
    /// it): the caller then records its outline instead. A glyph with no
    /// placement (a collapsed font matrix) paints nothing in the raster
    /// either, so it is dropped and counts as taken.
    pub(crate) fn glyph(
        &mut self,
        g: &GlyphText,
        rgb: (f32, f32, f32),
        comp: Composite,
        chain: &Chain,
    ) -> bool {
        let Some(p) = Placement::of(g) else {
            return true;
        };
        let Some(face) = crate::font::pdf_face(g.fid) else {
            return false;
        };
        // A font joins the document with its first embeddable glyph, so a
        // face whose every glyph is refused writes no font objects.
        let (font, cid) = match self.font_ids.get(&g.fid) {
            Some(&i) => match self.fonts[i].1.cid(face, g.gid, g.ch, g.advance) {
                Some(cid) => (i, cid),
                None => return false,
            },
            None => {
                let mut used = UsedGlyphs::new(face);
                let Some(cid) = used.cid(face, g.gid, g.ch, g.advance) else {
                    return false;
                };
                let i = self.fonts.len();
                self.fonts.push((g.fid, used));
                self.font_ids.insert(g.fid, i);
                (i, cid)
            }
        };
        if let Some(run) = &mut self.run
            && run.text.accepts(g, &p, rgb, comp, chain)
        {
            run.text.push(g, &p);
            run.widths.push(g.advance);
            run.cids.push(cid);
            return true;
        }
        self.flush_text();
        let gs = self.gs_ref(comp);
        // Registered when the run opens, so a page snapshotted with the
        // run still open lists the font in its resources.
        if !self.current_fonts.contains(&font) {
            self.current_fonts.push(font);
        }
        self.run = Some(PdfRun {
            text: TextRun::new(g, p, rgb, comp, chain.clone()),
            em: [g.em[0], g.em[1], g.em[2], g.em[3]],
            origin: (g.em[4], g.em[5]),
            font,
            gs,
            widths: vec![g.advance],
            cids: vec![cid],
        });
        true
    }

    /// Write the open text run into the content.
    fn flush_text(&mut self) {
        let Some(run) = self.run.take() else {
            return;
        };
        self.enter_clip(&run.text.chain);
        Self::write_run(&mut self.content, &run);
    }

    /// One text object: `Tf` at size 1 and the em → device matrix as `Tm`
    /// (the page's flip `cm` is in force), so text space is one em. Each
    /// glyph after the first is placed at pscat's own x by a `TJ`
    /// adjustment against the advance `/W` gives it, so ashow, widthshow,
    /// kshow and the xshow family keep their spacing. The adjustment is
    /// taken from where the previous one actually left the pen, so
    /// rounding never accumulates along a line.
    fn write_run(out: &mut String, run: &PdfRun) {
        let [a, b, c, d] = run.em;
        let size = run.text.size;
        let x0 = run.text.glyphs.first().map_or(0.0, |g| g.x);
        let mut tj = String::new();
        let mut hex_open = false;
        let mut pen = 0.0f64;
        let glyphs = run.text.glyphs.iter().zip(&run.widths).zip(&run.cids);
        for (i, ((g, &w), &cid)) in glyphs.enumerate() {
            if i > 0 {
                let target = (g.x - x0) / size;
                // Hundredths of a thousandth of an em: what is printed.
                let n = ((pen - target) * 100_000.0).round() / 100.0;
                if n != 0.0 && n.is_finite() {
                    if hex_open {
                        tj.push('>');
                        hex_open = false;
                    }
                    let _ = write!(tj, " {} ", crate::pdf_font::num(n, 2));
                    pen -= n / 1000.0;
                }
            }
            if !hex_open {
                tj.push('<');
                hex_open = true;
            }
            let _ = write!(tj, "{cid:04X}");
            pen += w / 1000.0;
        }
        if hex_open {
            tj.push('>');
        }
        let fm = |v: f64| crate::pdf_font::num(v, 5);
        let _ = writeln!(
            out,
            "q {}{} {} {} rg BT /F{} 1 Tf {} {} {} {} {} {} Tm [{}] TJ ET Q",
            run.gs,
            f(run.text.rgb.0),
            f(run.text.rgb.1),
            f(run.text.rgb.2),
            run.font,
            fm(a),
            fm(b),
            fm(c),
            fm(d),
            crate::pdf_font::num(run.origin.0, 3),
            crate::pdf_font::num(run.origin.1, 3),
            tj.trim(),
        );
    }

    pub(crate) fn fill(
        &mut self,
        path: &crate::gfx::PsPath,
        rule: FillRule,
        ink: &Ink,
        comp: Composite,
        chain: &Chain,
    ) {
        self.enter_clip(chain);
        let gs = self.gs_ref(comp);
        let colour = self.ink_ops(ink, false);
        let _ = writeln!(
            self.content,
            "q {gs}{colour}{} {}
Q",
            path_ops(path, true),
            match rule {
                FillRule::EvenOdd => "f*",
                _ => "f",
            }
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stroke(
        &mut self,
        path: &crate::gfx::PsPath,
        pen: Option<(tiny_skia::Transform, usize)>,
        ink: &Ink,
        width: f32,
        cap: tiny_skia::LineCap,
        join: tiny_skia::LineJoin,
        miter_limit: f32,
        dash: Option<(&[f32], f32)>,
        comp: Composite,
        chain: &Chain,
    ) {
        self.enter_clip(chain);
        let gs = self.gs_ref(comp);
        let colour = self.ink_ops(ink, true);
        let cap = match cap {
            tiny_skia::LineCap::Round => 1,
            tiny_skia::LineCap::Square => 2,
            _ => 0,
        };
        let join = match join {
            tiny_skia::LineJoin::Round => 1,
            tiny_skia::LineJoin::Bevel => 2,
            _ => 0,
        };
        // A pen-space width and dashes (#1196) get the same extra decimals
        // as the path, and a nonzero one never reads as 0.
        let num = |v: f32| match pen {
            Some((_, extra)) => {
                let s = fmt_num(v, 3 + extra);
                if s == "0" && v != 0.0 { p(v) } else { s }
            }
            None => f(v),
        };
        let dash = match dash {
            Some((pattern, phase)) => {
                let list: Vec<String> = pattern.iter().map(|d| num(*d)).collect();
                format!("[{}] {} d ", list.join(" "), num(phase))
            }
            None => String::new(),
        };
        // `path` is in pen space when the CTM is anisotropic (#1196,
        // `Gfx::stroke_space`): PDF strokes in user space, so a `cm` of
        // the pen-to-device map gives the elliptical pen. The `cm` is at
        // full f32 precision, like the shading `cm` (its entries can be
        // near 1, where three decimals would move a far point), and the
        // path at the extra decimals the caller asks for, since the `cm`
        // stretches its rounding too.
        let (pen_cm, path_text) = match pen {
            Some((l, extra)) => (
                format!("{} {} {} {} 0 0 cm ", p(l.sx), p(l.ky), p(l.kx), p(l.sy)),
                path.to_pdf_ops_with(false, PATH_DECIMALS + extra),
            ),
            None => (String::new(), path_ops(path, false)),
        };
        let _ = writeln!(
            self.content,
            "q {gs}{pen_cm}{colour}{} w {cap} J {join} j {} M {dash}{} S
Q",
            num(width),
            f(miter_limit),
            path_text,
        );
    }

    /// `shfill` (#1028): paint `kind` with the colour ramp `stops`
    /// (`(position, r, g, b)`, positions non-decreasing over 0..1) under
    /// `ctm` (user space to device space, `[a b c d e f]`). `region` is the
    /// device-space `/BBox` parallelogram when the shading has one; it
    /// becomes a clip before the `cm`, the same region the raster fills.
    /// Without it `sh` paints the whole clip, as the raster's page rect
    /// does.
    ///
    /// `extend` is the shading's own `/Extend` (#1040; before, always
    /// `[true true]`), which the raster honours as a PDF reader does.
    /// `/Background` is ignored by `shfill` (PLRM), so it is not written
    /// either.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn shfill(
        &mut self,
        kind: &crate::svg::ShKind,
        ctm: [f32; 6],
        stops: &[(f32, f32, f32, f32)],
        extend: [bool; 2],
        region: Option<&crate::gfx::PsPath>,
        comp: Composite,
        chain: &Chain,
    ) {
        // A non-finite number has no PDF spelling; writing `inf` would make
        // the whole file invalid. Every caller keeps these finite, so this
        // is a last guard, not a path content takes.
        let geometry = match *kind {
            crate::svg::ShKind::Axial { x0, y0, x1, y1 } => vec![x0, y0, x1, y1],
            crate::svg::ShKind::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
            } => vec![x0, y0, r0, x1, y1, r1],
        };
        if !geometry.iter().chain(ctm.iter()).all(|v| v.is_finite()) {
            return;
        }
        let stops_key: Vec<[u32; 4]> = stops
            .iter()
            .map(|s| [s.0.to_bits(), s.1.to_bits(), s.2.to_bits(), s.3.to_bits()])
            .collect();
        let fidx = match self.stops_ids.get(&stops_key) {
            Some(&i) => i,
            None => {
                let func = stops_function(stops);
                let i = intern(&mut self.functions, &mut self.function_ids, func);
                self.stops_ids.insert(stops_key, i);
                i
            }
        };
        let ext = format!("/Extend [{} {}]", extend[0], extend[1]);
        let body = match *kind {
            crate::svg::ShKind::Axial { x0, y0, x1, y1 } => format!(
                "/ShadingType 2 /ColorSpace /DeviceRGB /Coords [{} {} {} {}] {ext}",
                p(x0),
                p(y0),
                p(x1),
                p(y1)
            ),
            crate::svg::ShKind::Radial {
                x0,
                y0,
                r0,
                x1,
                y1,
                r1,
            } => format!(
                "/ShadingType 3 /ColorSpace /DeviceRGB /Coords [{} {} {} {} {} {}] {ext}",
                p(x0),
                p(y0),
                p(r0),
                p(x1),
                p(y1),
                p(r1)
            ),
        };
        let key = (body, fidx);
        let sidx = match self.shading_ids.get(&key) {
            Some(&i) => i,
            None => {
                let i = self.shadings.len();
                self.shadings.push(key.clone());
                self.shading_ids.insert(key, i);
                i
            }
        };
        if self.current_shading_set.insert(sidx) {
            self.current_shadings.push(sidx);
        }
        self.enter_clip(chain);
        let gs = self.gs_ref(comp);
        let clip = match region {
            Some(path) => format!("{} W n ", path_ops(path, true)),
            None => String::new(),
        };
        let _ = writeln!(
            self.content,
            "q {gs}{clip}{} {} {} {} {} {} cm /Sh{sidx} sh
Q",
            p(ctm[0]),
            p(ctm[1]),
            p(ctm[2]),
            p(ctm[3]),
            p(ctm[4]),
            p(ctm[5]),
        );
    }

    /// An image: `rgb_or_mask` is either interleaved RGB samples or,
    /// for masks, packed 1-bit rows; `t` maps sample coords to device
    /// space (the rasterizer's own transform). A mask paints `ink`, the
    /// current colour or pattern (#1299: `/Pattern cs /Pn scn` before the
    /// `Do`, which PDF allows for a stencil mask).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn image(
        &mut self,
        w: usize,
        h: usize,
        data: ImageData,
        t: [f32; 6],
        ink: &Ink,
        interpolate: bool,
        comp: Composite,
        chain: &Chain,
    ) {
        // `/Interpolate true` (#730): a viewer hint on the colour image
        // only. Never on an imagemask, nor on an ImageType 3 soft mask
        // (the raster keeps the stencil edge hard).
        let hint = if interpolate {
            " /Interpolate true"
        } else {
            ""
        };
        // An uncoloured pattern's content may hold no colour image (#1038).
        if self.uncolored && !matches!(data, ImageData::Mask { .. }) {
            return;
        }
        // A soft mask is its own image entry (never in a page's XObject
        // resources), referenced from the main image by index.
        let (dict_extra, raw, smask): (String, Vec<u8>, Option<usize>) = match data {
            ImageData::Rgb(px) => (
                format!("/ColorSpace /DeviceRGB /BitsPerComponent 8{hint}"),
                px,
                None,
            ),
            ImageData::Mask { bits, paint_ones } => (
                format!(
                    "/ImageMask true /BitsPerComponent 1 /Decode [{} {}]",
                    if paint_ones { 1 } else { 0 },
                    if paint_ones { 0 } else { 1 }
                ),
                bits,
                None,
            ),
            ImageData::RgbA { rgb, alpha } => {
                let mask = Self::image_object(
                    w,
                    h,
                    "/ColorSpace /DeviceGray /BitsPerComponent 8",
                    alpha,
                    None,
                );
                let at = self.images.len();
                self.images.push(mask);
                (
                    format!("/ColorSpace /DeviceRGB /BitsPerComponent 8{hint}"),
                    rgb,
                    Some(at),
                )
            }
        };
        let idx = self.images.len();
        self.images
            .push(Self::image_object(w, h, &dict_extra, raw, smask));
        self.current_images.push(idx);

        // cm maps the unit square to device space: sample→device `t`
        // composed with unit→sample [W 0 0 -H 0 H] (PDF images put
        // row 0 along the top edge of the unit square).
        let unit = tiny_skia::Transform::from_row(w as f32, 0.0, 0.0, -(h as f32), 0.0, h as f32);
        let m = unit.post_concat(tiny_skia::Transform::from_row(
            t[0], t[1], t[2], t[3], t[4], t[5],
        ));
        self.enter_clip(chain);
        // `ca` and `BM` apply to images and stencils alike (#766); an
        // SMask's own alpha multiplies with `ca`. Empty for the default.
        let gs = self.gs_ref(comp);
        let colour = self.ink_ops(ink, false);
        let _ = writeln!(
            self.content,
            "q {gs}{colour}{} {} {} {} {} {} cm /Im{idx} Do
Q",
            f(m.sx),
            f(m.ky),
            f(m.kx),
            f(m.sy),
            f(m.tx),
            f(m.ty),
        );
    }

    /// Build one Flate-compressed image XObject around `raw`.
    fn image_object(
        w: usize,
        h: usize,
        dict_extra: &str,
        raw: Vec<u8>,
        smask: Option<usize>,
    ) -> PdfImage {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write as _;

        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        let _ = enc.write_all(&raw);
        let compressed = enc.finish().unwrap_or(raw);
        let head =
            format!("<< /Type /XObject /Subtype /Image /Width {w} /Height {h} {dict_extra} ")
                .into_bytes();
        let mut tail = format!(
            "/Filter /FlateDecode /Length {} >>\nstream\n",
            compressed.len()
        )
        .into_bytes();
        tail.extend_from_slice(&compressed);
        tail.extend_from_slice(b"\nendstream");
        PdfImage { head, smask, tail }
    }

    pub(crate) fn erase(&mut self) {
        self.run = None;
        self.current_fonts.clear();
        self.content.clear();
        self.open_clip = None;
        self.current_images.clear();
        self.current_gstates.clear();
        self.current_shadings.clear();
        self.current_shading_set.clear();
        self.current_patterns.clear();
        self.current_pattern_cs = false;
    }

    /// The page size from now on (`setpagedevice`, #909); finished pages
    /// keep theirs.
    pub(crate) fn set_size(&mut self, width: u32, height: u32) {
        self.flush_text();
        self.width = width;
        self.height = height;
    }

    pub(crate) fn end_page(&mut self) {
        self.flush_text();
        self.pages.push(self.snapshot());
        self.page_sizes.push((self.width, self.height));
        self.page_images.push(self.current_images.clone());
        self.page_gstates.push(self.current_gstates.clone());
        self.page_shadings.push(self.current_shadings.clone());
        self.page_fonts.push(self.current_fonts.clone());
        self.page_patterns.push(self.current_patterns.clone());
        self.page_pattern_cs.push(self.current_pattern_cs);
    }

    /// Serialize the whole document.
    pub fn finish(&self, trailing_art: bool) -> Vec<u8> {
        // No budget, so nothing refuses and a `Vec` sink cannot fail.
        self.finish_limited(trailing_art, LimitProbe::NONE)
            .expect("an unlimited write to a Vec cannot fail")
    }

    /// The document as a `Vec`, refused with [`PdfWriteError::Memory`]
    /// before the `Vec` or any working buffer would pass the memory cap
    /// `probe` carries (#1107).
    pub(crate) fn finish_limited(
        &self,
        trailing_art: bool,
        probe: LimitProbe,
    ) -> Result<Vec<u8>, PdfWriteError> {
        let mut sink = BudgetedVec {
            v: Vec::new(),
            probe,
        };
        self.write_document(trailing_art, probe, &mut sink)?;
        Ok(sink.v)
    }

    /// Write the document to `sink` as it is built (#1107): no object list
    /// and no copy of the output is kept, so what is held beside the
    /// recorder is one compressed page or image stream at a time. The bytes
    /// are the ones `finish` returns.
    pub(crate) fn write_document(
        &self,
        trailing_art: bool,
        probe: LimitProbe,
        sink: &mut dyn io::Write,
    ) -> Result<(), PdfWriteError> {
        let mut out = Out::new(sink, probe);
        let _ = self.write_objects(trailing_art, &mut out);
        out.finish()
    }

    /// The tail of `snapshot`: what an open group, a pending text run and
    /// the closing `Q` add after `content`, so the finished document can
    /// deflate `content` in place instead of cloning it.
    fn snapshot_tail(&self) -> String {
        let mut s = String::new();
        let mut open = self.open_clip.is_some();
        if let Some(run) = &self.run {
            let same = match (&self.open_clip, &run.text.chain) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                if open {
                    s.push_str("Q\n");
                }
                open = run.text.chain.is_some();
                if open {
                    let _ = writeln!(s, "q {}", self.clip_prelude(&run.text.chain));
                }
            }
            // Inside the run's clip group, before the group's `Q`.
            Self::write_run(&mut s, run);
        }
        if open {
            s.push_str("Q\n");
        }
        s
    }

    /// Every object, in file order, then the xref table and trailer. An
    /// error is kept in `out` (and the rest of the writes skipped), `Err`
    /// only says to stop early.
    fn write_objects(&self, trailing_art: bool, out: &mut Out) -> Result<(), ()> {
        // The pages, finished ones then (if it holds art) the open one,
        // borrowed: only the open page's small tail is built.
        let mut pages: Vec<PageSrc> = Vec::with_capacity(self.pages.len() + 1);
        for (i, content) in self.pages.iter().enumerate() {
            pages.push(PageSrc {
                content,
                tail: String::new(),
                size: self.page_sizes[i],
                images: &self.page_images[i],
                gstates: &self.page_gstates[i],
                shadings: &self.page_shadings[i],
                fonts: &self.page_fonts[i],
                patterns: &self.page_patterns[i],
                pattern_cs: self.page_pattern_cs[i],
            });
        }
        if pages.is_empty() || trailing_art {
            pages.push(PageSrc {
                content: &self.content,
                tail: self.snapshot_tail(),
                size: (self.width, self.height),
                images: &self.current_images,
                gstates: &self.current_gstates,
                shadings: &self.current_shadings,
                fonts: &self.current_fonts,
                patterns: &self.current_patterns,
                pattern_cs: self.current_pattern_cs,
            });
        }
        let npages = pages.len();

        // Object layout: 1 catalog, 2 pages tree, then per page
        // [page, contents], then all images, in that order.
        let kids: Vec<String> = (0..npages).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
        let image_base = 3 + npages * 2;
        // Shading functions, then shadings, follow the images (#1028), so a
        // document without shadings keeps every object number it had.
        let function_base = image_base + self.images.len();
        let shading_base = function_base + self.functions.len();
        // pdfmark's named objects (#1032) follow the shadings, for the same
        // reason.
        let named_base = shading_base + self.shadings.len();
        let refs = |i: usize| named_base + i;
        // Embedded fonts (#1193) follow the named objects, so a document
        // without text output keeps every object number it had.
        let font_base = named_base + self.named.len();
        // Pattern streams (#1038) follow the fonts, for the same reason.
        let bases = Bases {
            image: image_base,
            shading: shading_base,
            font: font_base,
            pattern: font_base + self.fonts.len() * OBJECTS_PER_FONT,
        };
        out.bytes(b"%PDF-1.4\n");
        out.begin_obj();
        if self.embeds.is_empty() {
            out.bytes(b"<< /Type /Catalog /Pages 2 0 R >>");
        } else {
            // One root node holding every key, sorted by bytes as a name
            // tree must be.
            let mut embeds: Vec<&(Vec<u8>, PdfValue)> = self.embeds.iter().collect();
            embeds.sort_by(|a, b| a.0.cmp(&b.0));
            out.bytes(b"<< /Type /Catalog /Pages 2 0 R /Names << /EmbeddedFiles << /Names [");
            for (key, fs) in embeds {
                out.bytes(b" ");
                let _ = write_literal_string(&mut *out, key);
                out.bytes(b" ");
                fs.write_pdf(&mut *out, &refs);
            }
            out.bytes(b" ] >> >> >>");
        }
        out.end_obj()?;
        out.begin_obj();
        let _ = write!(
            out,
            "<< /Type /Pages /Kids [{}] /Count {npages} >>",
            kids.join(" ")
        );
        out.end_obj()?;
        for (i, page) in pages.iter().enumerate() {
            let (width, height) = page.size;
            let parts = resource_parts(
                self,
                &bases,
                page.images,
                page.gstates,
                page.shadings,
                page.fonts,
                page.patterns,
                page.pattern_cs,
            );
            let resources = if parts.is_empty() {
                String::new()
            } else {
                format!(" /Resources <<{parts} >>")
            };
            out.begin_obj();
            let _ = write!(
                out,
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}]{resources} \
                 /Contents {} 0 R >>",
                width,
                height,
                3 + i * 2 + 1,
            );
            out.end_obj()?;
            // The page content is Flate-compressed like the image
            // XObjects: it is highly repetitive text (a stroke-heavy
            // gallery piece is hundreds of MB raw, ~3.5x smaller
            // deflated; issue #244). The `cm` prefix is part of the
            // compressed stream. It is deflated straight from the
            // recorder's own text (#1107), and only the packed stream is
            // held while it is written.
            let prefix = format!("1 0 0 -1 0 {height} cm\n");
            let z = match deflate_budgeted(
                &[
                    prefix.as_bytes(),
                    page.content.as_bytes(),
                    page.tail.as_bytes(),
                ],
                out.probe,
            ) {
                Ok(z) => z,
                Err(e) => return out.fail(e),
            };
            out.begin_obj();
            let _ = write!(
                out,
                "<< /Length {} /Filter /FlateDecode >>\nstream\n",
                z.len()
            );
            out.bytes(&z);
            out.bytes(b"\nendstream");
            out.end_obj()?;
        }
        for img in &self.images {
            out.begin_obj();
            out.bytes(&img.head);
            if let Some(at) = img.smask {
                let _ = write!(out, "/SMask {} 0 R ", image_base + at);
            }
            out.bytes(&img.tail);
            out.end_obj()?;
        }
        for func in &self.functions {
            out.begin_obj();
            out.bytes(func.as_bytes());
            out.end_obj()?;
        }
        for (body, fidx) in &self.shadings {
            out.begin_obj();
            let _ = write!(out, "<< {body} /Function {} 0 R >>", function_base + fidx);
            out.end_obj()?;
        }
        for obj in &self.named {
            out.begin_obj();
            obj.write_pdf(out, &refs);
            out.end_obj()?;
        }
        for (i, (fid, used)) in self.fonts.iter().enumerate() {
            let face = crate::font::embed_face(*fid);
            // `face` gives the descriptor's metrics and the name; the
            // glyphs were built as they were shown.
            let first = font_base + i * OBJECTS_PER_FONT;
            debug_assert_eq!(first, out.objects() + 1);
            let objs = crate::pdf_font::font_objects(
                face.as_ref().map(|(f, _)| *f),
                face.as_ref().map_or("Font", |(_, n)| n.as_str()),
                i,
                used,
                first,
            );
            for obj in &objs {
                out.begin_obj();
                out.bytes(obj);
                out.end_obj()?;
            }
        }
        for (i, pat) in self.patterns.iter().enumerate() {
            let parts = resource_parts(
                self,
                &bases,
                &pat.images,
                &pat.gstates,
                &pat.shadings,
                &pat.fonts,
                &pat.patterns,
                pat.pattern_cs,
            );
            let z = match deflate_budgeted(&[pat.content.as_bytes()], out.probe) {
                Ok(z) => z,
                Err(e) => return out.fail(e),
            };
            debug_assert_eq!(out.objects() + 1, bases.pattern + i);
            out.begin_obj();
            let _ = write!(
                out,
                "<< {} /Resources <<{parts} >> /Length {} /Filter /FlateDecode >>\nstream\n",
                pat.dict,
                z.len()
            );
            out.bytes(&z);
            out.bytes(b"\nendstream");
            out.end_obj()?;
        }

        // /Info is appended last, after every page/image object, so it
        // never shifts the kids/image_base positional math above —
        // those were computed against the object count *before* this
        // point. Only emitted when there's something to say; every
        // pscat PDF still gets /Producer.
        //
        // A `/DOCINFO` pdfmark entry wins over the DSC comment for the same
        // key, whichever comes first in the file (gs's pdfwrite: a
        // `/Title` mark replaces `%%Title:`, and `%%For:` stays the
        // author when no mark sets one). Title and Author keep their place
        // after Producer; the other marked keys follow in first-set order.
        let info_obj_num = out.objects() + 1;
        out.begin_obj();
        out.bytes(b"<< /Producer (pscat)");
        let marked = |key: &str| self.docinfo.iter().find(|(k, _)| k == key).map(|e| &e.1);
        for (key, dsc) in [("Title", &self.title), ("Author", &self.author)] {
            if let Some(v) = marked(key) {
                let _ = write!(out, " /{key} ");
                v.write_pdf(&mut *out, &refs);
            } else if let Some(s) = dsc {
                // Straight into the file: a non-ASCII title is four bytes of
                // file for each of its bytes, and a DSC line has no length cap.
                let _ = write!(out, " /{key} ");
                let _ = write_text_bytes(&mut *out, s.as_bytes());
            }
        }
        for (key, value) in &self.docinfo {
            if key != "Title" && key != "Author" {
                out.bytes(b" ");
                let _ = write_name(&mut *out, key);
                out.bytes(b" ");
                value.write_pdf(&mut *out, &refs);
            }
        }
        out.bytes(b" >>");
        out.end_obj()?;

        let xref_at = out.pos();
        let count = out.objects();
        let _ = write!(out, "xref\n0 {}\n", count + 1);
        out.bytes(b"0000000000 65535 f \n");
        for i in 0..count {
            let off = out.offset(i);
            let _ = writeln!(out, "{off:010} 00000 n ");
        }
        let _ = write!(
            out,
            "trailer\n<< /Size {} /Root 1 0 R /Info {info_obj_num} 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            count + 1
        );
        out.check()
    }
}

/// What the PDF writer may take past the run's `--max-memory`, in bytes
/// (#1107): a run that a limit stopped is at its cap, and its page is still
/// written, so the writer's own working memory (the largest packed page or
/// image stream, a 64 KiB piece) gets this much more. A document that fits
/// the limit while running is written inside it plus this.
pub const WRITE_MARGIN: usize = 16 << 20;

/// One page as `write_objects` has it: borrowed from the recorder, so a
/// big content stream is not cloned (#1107).
struct PageSrc<'a> {
    content: &'a str,
    /// What the open page's pending run and groups add after `content`
    /// (`snapshot_tail`); empty for a finished page.
    tail: String,
    size: (u32, u32),
    images: &'a [usize],
    gstates: &'a [usize],
    shadings: &'a [usize],
    fonts: &'a [usize],
    patterns: &'a [usize],
    pattern_cs: bool,
}

/// Object numbers `write_objects` laid out, for resource dicts.
struct Bases {
    image: usize,
    shading: usize,
    font: usize,
    pattern: usize,
}

/// A `/Resources` entry list (with a leading space each), empty when the
/// content uses nothing. Pages and pattern streams both use it.
#[allow(clippy::too_many_arguments)]
fn resource_parts(
    rec: &PdfRecorder,
    b: &Bases,
    images: &[usize],
    gstates: &[usize],
    shadings: &[usize],
    fonts: &[usize],
    patterns: &[usize],
    pattern_cs: bool,
) -> String {
    let xobjects: Vec<String> = images
        .iter()
        .map(|&idx| format!("/Im{idx} {} 0 R", b.image + idx))
        .collect();
    // ExtGStates are written inline in the page's own
    // /Resources rather than as indirect objects: they're a
    // handful of numbers each, and keeping them out of the
    // numbered-object list means the kids/image_base
    // positional math above needs no adjustment at all.
    let extgstates: Vec<String> = gstates
        .iter()
        .map(|&idx| format!("/GS{idx} << {} >>", rec.gstates[idx]))
        .collect();
    let mut parts = String::new();
    if !xobjects.is_empty() {
        let _ = write!(parts, " /XObject << {} >>", xobjects.join(" "));
    }
    if !extgstates.is_empty() {
        let _ = write!(parts, " /ExtGState << {} >>", extgstates.join(" "));
    }
    if !shadings.is_empty() {
        let list: Vec<String> = shadings
            .iter()
            .map(|&idx| format!("/Sh{idx} {} 0 R", b.shading + idx))
            .collect();
        let _ = write!(parts, " /Shading << {} >>", list.join(" "));
    }
    if !fonts.is_empty() {
        let list: Vec<String> = fonts
            .iter()
            .map(|&idx| format!("/F{idx} {} 0 R", b.font + idx * OBJECTS_PER_FONT))
            .collect();
        let _ = write!(parts, " /Font << {} >>", list.join(" "));
    }
    if !patterns.is_empty() {
        let list: Vec<String> = patterns
            .iter()
            .map(|&idx| format!("/P{idx} {} 0 R", b.pattern + idx))
            .collect();
        let _ = write!(parts, " /Pattern << {} >>", list.join(" "));
    }
    if pattern_cs {
        parts.push_str(" /ColorSpace << /PCS [/Pattern /DeviceRGB] >>");
    }
    parts
}

/// Why a PDF could not be written ([`Interp::write_pdf`](crate::Interp::write_pdf),
/// [`Interp::pdf_document_limited`](crate::Interp::pdf_document_limited)).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PdfWriteError {
    /// Writing the document would take the heap past
    /// [`Limits::max_memory`](crate::Limits::max_memory). It was refused
    /// before the allocation that would pass it; nothing more was written,
    /// and what had been written is incomplete.
    #[error("limitexceeded: memory (the PDF needs more than --max-memory allows)")]
    Memory,
    /// The sink refused a write.
    #[error("{0}")]
    Io(io::Error),
}

/// The `io::Error` the writers raise for the memory cap, told from a
/// sink's own failure by its payload.
#[derive(Debug)]
struct MemoryLimit;

impl std::fmt::Display for MemoryLimit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the memory limit")
    }
}

impl std::error::Error for MemoryLimit {}

fn memory_refused() -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, MemoryLimit)
}

/// A failure of a sink or of the memory cap, as the caller sees it.
fn classify(e: io::Error) -> PdfWriteError {
    if e.get_ref().is_some_and(|r| r.is::<MemoryLimit>()) {
        PdfWriteError::Memory
    } else {
        PdfWriteError::Io(e)
    }
}

/// A growing `Vec<u8>` that asks the memory cap before it takes more
/// room, growing by half again (not doubling) so the last step stays small.
struct BudgetedVec {
    v: Vec<u8>,
    probe: LimitProbe,
}

impl io::Write for BudgetedVec {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let need = self.v.len() + data.len();
        let cap = self.v.capacity();
        if need > cap {
            let target = need.max(cap + cap / 2);
            self.probe
                .check_alloc(target - cap)
                .map_err(|_| memory_refused())?;
            self.v.reserve_exact(target - self.v.len());
        }
        self.v.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Zlib-compress the concatenation of `parts`, refusing before the packed
/// bytes pass the memory cap (#1107).
fn deflate_budgeted(parts: &[&[u8]], probe: LimitProbe) -> io::Result<Vec<u8>> {
    use flate2::{Compression, write::ZlibEncoder};
    let mut enc = ZlibEncoder::new(
        BudgetedVec {
            v: Vec::new(),
            probe,
        },
        Compression::default(),
    );
    for part in parts {
        enc.write_all(part)?;
    }
    Ok(enc.finish()?.v)
}

/// Where `write_objects` puts the file: pieces of up to `OUT_CHUNK` bytes
/// go on to the sink, and the first failure is kept and every later write
/// skipped. It also counts, so object offsets need no copy of the output.
struct Out<'a> {
    sink: &'a mut dyn io::Write,
    buf: Vec<u8>,
    /// Bytes already handed to `sink`.
    written: usize,
    err: Option<io::Error>,
    probe: LimitProbe,
    /// Where each object so far starts.
    offsets: Vec<usize>,
}

/// Bytes `Out` collects before it hands them on (and looks at the memory
/// cap).
const OUT_CHUNK: usize = 64 << 10;

impl<'a> Out<'a> {
    fn new(sink: &'a mut dyn io::Write, probe: LimitProbe) -> Self {
        Out {
            sink,
            buf: Vec::with_capacity(OUT_CHUNK + 4096),
            written: 0,
            err: None,
            probe,
            offsets: Vec::new(),
        }
    }

    /// Offset of the next byte.
    fn pos(&self) -> usize {
        self.written + self.buf.len()
    }

    fn objects(&self) -> usize {
        self.offsets.len()
    }

    fn offset(&self, i: usize) -> usize {
        self.offsets[i]
    }

    fn begin_obj(&mut self) {
        let at = self.pos();
        self.offsets.push(at);
        let n = self.offsets.len();
        let _ = writeln!(self, "{n} 0 obj");
    }

    /// End the object; `Err` when a write has failed, to stop early.
    fn end_obj(&mut self) -> Result<(), ()> {
        self.bytes(b"\nendobj\n");
        self.check()
    }

    fn check(&self) -> Result<(), ()> {
        if self.err.is_some() { Err(()) } else { Ok(()) }
    }

    /// Record `e` as the failure and say to stop.
    fn fail(&mut self, e: io::Error) -> Result<(), ()> {
        if self.err.is_none() {
            self.err = Some(e);
        }
        Err(())
    }

    fn bytes(&mut self, data: &[u8]) {
        if self.err.is_some() {
            return;
        }
        if data.len() >= OUT_CHUNK {
            self.flush_buf();
            self.pass(data);
        } else {
            self.buf.extend_from_slice(data);
            if self.buf.len() >= OUT_CHUNK {
                self.flush_buf();
            }
        }
    }

    fn flush_buf(&mut self) {
        let buf = std::mem::take(&mut self.buf);
        self.pass(&buf);
        self.buf = buf;
        self.buf.clear();
    }

    /// Hand `data` to the sink, after a look at the memory cap: whatever
    /// working buffers are alive now (a packed stream, say) are counted.
    fn pass(&mut self, data: &[u8]) {
        if self.err.is_some() || data.is_empty() {
            return;
        }
        if self.probe.check_alloc(0).is_err() {
            self.err = Some(memory_refused());
            return;
        }
        match self.sink.write_all(data) {
            Ok(()) => self.written += data.len(),
            Err(e) => self.err = Some(e),
        }
    }

    fn finish(mut self) -> Result<(), PdfWriteError> {
        self.flush_buf();
        match self.err {
            Some(e) => Err(classify(e)),
            None => Ok(()),
        }
    }
}

impl std::fmt::Write for Out<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.bytes(s.as_bytes());
        Ok(())
    }
}

/// One image XObject. `smask` names another entry of `images` whose
/// object number is only known in `finish`, so the stream is kept in two
/// halves around the `/SMask` reference.
struct PdfImage {
    head: Vec<u8>,
    smask: Option<usize>,
    tail: Vec<u8>,
}

pub(crate) enum ImageData {
    /// Interleaved 8-bit RGB, W*H*3 bytes.
    Rgb(Vec<u8>),
    /// Packed 1-bit rows (byte-aligned), stencil polarity included.
    Mask { bits: Vec<u8>, paint_ones: bool },
    /// 8-bit RGB plus a same-size 8-bit alpha plane, written as an
    /// `/SMask` (`ImageType 3`, #739).
    RgbA { rgb: Vec<u8>, alpha: Vec<u8> },
}

/// `a b c d e f` at full precision, for a `cm` or a `/Matrix`.
fn matrix_text(t: tiny_skia::Transform) -> String {
    format!(
        "{} {} {} {} {} {}",
        p(t.sx),
        p(t.ky),
        p(t.kx),
        p(t.sy),
        p(t.tx),
        p(t.ty)
    )
}

/// Zlib-compress the concatenation of `parts` (PDF `FlateDecode`).
pub(crate) fn deflate(parts: &[&[u8]]) -> Option<Vec<u8>> {
    deflate_budgeted(parts, LimitProbe::NONE).ok()
}

/// Index of `body` in `list` (looked up through `ids`), appending it if new.
fn intern(list: &mut Vec<String>, ids: &mut HashMap<String, usize>, body: String) -> usize {
    if let Some(&i) = ids.get(&body) {
        return i;
    }
    let i = list.len();
    list.push(body.clone());
    ids.insert(body, i);
    i
}

/// The PDF function for a shading's RGB stop list (#1028): linear
/// interpolation between consecutive stops, which is what tiny-skia and
/// SVG do with the same list. One leg is a FunctionType 2 (`N 1`); more
/// are a FunctionType 3 stitching inline type 2 legs at the interior stop
/// positions. A zero-width leg (two stops at one position: a hard edge)
/// is dropped, which leaves the jump between its neighbours and keeps
/// `/Bounds` strictly increasing, as PDF requires. Positions are compared
/// as written, so two that print the same count as one.
fn stops_function(stops: &[(f32, f32, f32, f32)]) -> String {
    let rgb = |s: &(f32, f32, f32, f32)| format!("{} {} {}", f(s.1), f(s.2), f(s.3));
    let leg = |a: &(f32, f32, f32, f32), b: &(f32, f32, f32, f32)| {
        format!(
            "<< /FunctionType 2 /Domain [0 1] /C0 [{}] /C1 [{}] /N 1 >>",
            rgb(a),
            rgb(b)
        )
    };
    let stops = merge_collinear_stops(stops);
    type Stop = (f32, f32, f32, f32);
    // (end position, from, to) for every leg of positive width; each
    // starts where the previous one ended (the first at 0).
    let mut spans: Vec<(f32, Stop, Stop)> = Vec::new();
    for w in stops.windows(2) {
        if w[1].0 > w[0].0 {
            spans.push((w[1].0.clamp(0.0, 1.0), w[0], w[1]));
        }
    }
    // A hard edge at either end of the axis (two stops at 0, or at 1, in
    // different colours) has no leg of its own, but the raster pads with
    // the outermost stop's colour, so the PDF must end on it too: give it
    // a sliver leg, `EDGE` wide, or recolour a leg already that narrow.
    const EDGE: f32 = 1e-6;
    let same = |a: &Stop, b: &Stop| a.1 == b.1 && a.2 == b.2 && a.3 == b.3;
    if let (Some(&last), Some(&(_, _, to))) = (stops.last(), spans.last())
        && !same(&last, &to)
    {
        let n = spans.len();
        let start = if n >= 2 { spans[n - 2].0 } else { 0.0 };
        if start < 1.0 - EDGE {
            spans[n - 1].0 = 1.0 - EDGE;
            spans.push((1.0, to, last));
        } else {
            spans[n - 1].2 = last;
        }
    }
    if let (Some(&first), Some(&(end, from, _))) = (stops.first(), spans.first())
        && !same(&first, &from)
    {
        if end > EDGE {
            spans.insert(0, (EDGE, first, from));
        } else {
            spans[0].1 = first;
        }
    }
    // (end position as written, leg); a leg whose end prints the same as
    // its start is dropped, so `/Bounds` stays strictly increasing.
    let mut legs: Vec<(String, String)> = Vec::new();
    let mut start = "0".to_string();
    for (end, from, to) in &spans {
        let end = p(*end);
        if end == start {
            continue;
        }
        legs.push((end.clone(), leg(from, to)));
        start = end;
    }
    match legs.len() {
        0 => {
            // No positive-width leg (one stop, or all at one position): a
            // constant, the last stop's colour (what a pad gradient shows).
            let last = stops.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
            leg(&last, &last)
        }
        1 => legs.pop().map(|l| l.1).unwrap_or_default(),
        n => {
            let bounds: Vec<&str> = legs[..n - 1].iter().map(|l| l.0.as_str()).collect();
            let fns: Vec<&str> = legs.iter().map(|l| l.1.as_str()).collect();
            let encode = vec!["0 1"; n].join(" ");
            format!(
                "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{encode}] >>",
                fns.join(" "),
                bounds.join(" ")
            )
        }
    }
}

/// Drop the stops that lie on the straight line between their neighbours
/// (to within `TOL` per channel, far below the 3-decimal colour the PDF
/// writes and the 1/255 a raster resolves), so a constant or linear run of
/// a densely sampled ramp is one leg, not dozens. Greedy: from each kept
/// stop, extend the segment while every stop it skips stays on the line.
/// A hard edge (two stops at one position) is never merged across.
fn merge_collinear_stops(stops: &[(f32, f32, f32, f32)]) -> Vec<(f32, f32, f32, f32)> {
    const TOL: f32 = 1e-4;
    let on_line = |a: &(f32, f32, f32, f32), b: &(f32, f32, f32, f32), m: &(f32, f32, f32, f32)| {
        if !(a.0 < m.0 && m.0 < b.0) {
            return false;
        }
        let t = (m.0 - a.0) / (b.0 - a.0);
        let near = |ca: f32, cb: f32, cm: f32| (ca + t * (cb - ca) - cm).abs() <= TOL;
        near(a.1, b.1, m.1) && near(a.2, b.2, m.2) && near(a.3, b.3, m.3)
    };
    let mut out = Vec::with_capacity(stops.len());
    let mut i = 0;
    while i < stops.len() {
        out.push(stops[i]);
        let mut j = i + 1;
        while j + 1 < stops.len()
            && (i + 1..=j).all(|m| on_line(&stops[i], &stops[j + 1], &stops[m]))
        {
            j += 1;
        }
        i = j;
    }
    out
}

/// PDF path-construction operators for a device-space path. `area` is
/// true for a fill or a clip, false for a stroke (see `to_pdf_ops`).
fn path_ops(path: &crate::gfx::PsPath, area: bool) -> String {
    path.to_pdf_ops(area)
}

/// A PDF number with at most `decimals` fractional digits, trailing zeros
/// and a bare `.` trimmed, and never `-0` (a tiny negative rounds to `0`).
pub(crate) fn fmt_num(v: f32, decimals: usize) -> String {
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

/// Colours, alpha, line widths, dash arrays, miter limits and the image
/// `cm`: three decimals. A scale factor or a hairline width keeps its
/// relative precision this way, which two decimals would not.
fn f(v: f32) -> String {
    fmt_num(v, 3)
}

/// Path coordinates (`m l c` in fills, strokes and clips): two decimals
/// (#320). Device units are points, so the error is at most 0.005 pt, and
/// the same coordinate always rounds to the same text, so shapes that
/// shared an edge still share it. Roughly -20% on a speck-heavy page.
#[cfg(test)]
pub(crate) fn path_coord(v: f32) -> String {
    fmt_num(v, PATH_DECIMALS)
}

/// The decimals `path_coord` writes.
pub(crate) const PATH_DECIMALS: usize = 2;

/// Shading geometry and its `cm` (#1028): the shortest text that reads
/// back as the same `f32` (Rust's `Display` never uses an exponent, which
/// PDF numbers cannot have), so a rotated or finely scaled CTM keeps all
/// its precision; never `-0`.
fn p(v: f32) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    format!("{v}")
}

/// Encode a PDF text-string object. Printable-ASCII input becomes an
/// escaped literal string `(...)`; anything else (non-ASCII text —
/// plausible for a title given this project's Korean/Japanese font
/// work, or stray control bytes) becomes a UTF-16BE hex string with
/// the standard `FEFF` BOM prefix, the PDF spec's documented mechanism
/// for non-PDFDocEncoding text strings. Either form round-trips
/// through gs (tests/pdf.rs), so there's no silent mojibake either way.
#[cfg(test)]
fn pdf_string(s: &str) -> String {
    let mut out = String::new();
    let _ = write_text_bytes(&mut out, s.as_bytes());
    out
}

/// A byte string as a PDF literal string: `(`, `)` and `\` escaped, and
/// every byte outside printable ASCII written as a 3-digit octal escape,
/// so the object text stays ASCII and reads back as exactly these bytes.
/// Written straight into `out`, so a big string is never built twice
/// (#1107: a NUL byte is four bytes of file).
fn write_literal_string(out: &mut dyn std::fmt::Write, bytes: &[u8]) -> std::fmt::Result {
    // Pieces of a few hundred bytes, so the writer is called once for many.
    let mut piece = String::with_capacity(512);
    piece.push('(');
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                piece.push('\\');
                piece.push(b as char);
            }
            0x20..=0x7e => piece.push(b as char),
            _ => {
                piece.push('\\');
                piece.push(char::from(b'0' + (b >> 6)));
                piece.push(char::from(b'0' + ((b >> 3) & 7)));
                piece.push(char::from(b'0' + (b & 7)));
            }
        }
        if piece.len() >= 500 {
            out.write_str(&piece)?;
            piece.clear();
        }
    }
    piece.push(')');
    out.write_str(&piece)
}

/// A text string (a `/DOCINFO` value, #1032). Bytes that already start
/// with the UTF-16BE byte order mark are the program's own UTF-16 and
/// pass through as hex; printable ASCII stays a literal; other valid
/// UTF-8 becomes UTF-16BE, so a reader shows the characters the source
/// spelled (gs writes the UTF-8 bytes as they are, which a reader decodes
/// as PDFDocEncoding: mojibake); anything else is written byte for byte.
fn write_text_bytes(out: &mut dyn std::fmt::Write, bytes: &[u8]) -> std::fmt::Result {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    // Pieces of a few hundred bytes, so the writer is called once for many.
    let mut piece = String::with_capacity(512);
    let flush = |piece: &mut String, out: &mut dyn std::fmt::Write| -> std::fmt::Result {
        if piece.len() >= 500 {
            out.write_str(piece)?;
            piece.clear();
        }
        Ok(())
    };
    let hex = |piece: &mut String, b: u8| {
        piece.push(char::from(HEX[usize::from(b >> 4)]));
        piece.push(char::from(HEX[usize::from(b & 15)]));
    };
    if bytes.starts_with(&[0xfe, 0xff]) {
        piece.push('<');
        for &b in bytes {
            hex(&mut piece, b);
            flush(&mut piece, out)?;
        }
        piece.push('>');
        return out.write_str(&piece);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) if s.chars().all(|c| c.is_ascii() && !c.is_ascii_control()) => {
            piece.push('(');
            for c in s.chars() {
                if c == '(' || c == ')' || c == '\\' {
                    piece.push('\\');
                }
                piece.push(c);
                flush(&mut piece, out)?;
            }
            piece.push(')');
            out.write_str(&piece)
        }
        Ok(s) => {
            piece.push_str("<FEFF");
            for unit in s.encode_utf16() {
                let [hi, lo] = unit.to_be_bytes();
                hex(&mut piece, hi);
                hex(&mut piece, lo);
                flush(&mut piece, out)?;
            }
            piece.push('>');
            out.write_str(&piece)
        }
        Err(_) => write_literal_string(out, bytes),
    }
}

/// A PDF name object: `/` and the name's bytes, each byte outside the
/// regular characters (printable ASCII less the delimiters and `#`)
/// written as `#XX` (PDF 1.2+). So a PostScript `/text#2Fplain` keeps its
/// `#` and comes out `/text#232Fplain`, as gs writes it; spell the name
/// `(text/plain) cvn` to get `/text#2Fplain` in the file.
fn write_name(out: &mut dyn std::fmt::Write, name: &str) -> std::fmt::Result {
    out.write_char('/')?;
    for &b in name.as_bytes() {
        match b {
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#' => {
                write!(out, "#{b:02X}")?
            }
            0x21..=0x7e => out.write_char(b as char)?,
            _ => write!(out, "#{b:02X}")?,
        }
    }
    Ok(())
}

/// A real as the shortest decimal that reads back as the same `f64`
/// (`Display` never uses an exponent, which PDF numbers cannot have);
/// never `-0`. The operator refuses non-finite values before they get here.
fn pdf_real(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".to_string();
    }
    let s = format!("{v}");
    if s.contains('.') { s } else { format!("{s}.0") }
}

fn write_entries(
    out: &mut dyn std::fmt::Write,
    entries: &[(String, PdfValue)],
    refs: &dyn Fn(usize) -> usize,
) {
    for (key, value) in entries {
        let _ = out.write_char(' ');
        let _ = write_name(out, key);
        let _ = out.write_char(' ');
        value.write_pdf(out, refs);
    }
}

impl PdfValue {
    /// Append this value's PDF syntax to `out`, which is the file itself
    /// (#1107): a value is not built as text and copied again. `refs` maps
    /// a named object to its object number.
    fn write_pdf(&self, out: &mut dyn std::fmt::Write, refs: &dyn Fn(usize) -> usize) {
        match self {
            PdfValue::Null => {
                let _ = out.write_str("null");
            }
            PdfValue::Bool(b) => {
                let _ = out.write_str(if *b { "true" } else { "false" });
            }
            PdfValue::Int(i) => {
                let _ = write!(out, "{i}");
            }
            PdfValue::Real(r) => {
                let _ = out.write_str(&pdf_real(*r));
            }
            PdfValue::Name(n) => {
                let _ = write_name(out, n);
            }
            PdfValue::Str(s) => {
                let _ = write_literal_string(out, s);
            }
            PdfValue::Text(s) => {
                let _ = write_text_bytes(out, s);
            }
            PdfValue::Array(items) => {
                let _ = out.write_char('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        let _ = out.write_char(' ');
                    }
                    v.write_pdf(out, refs);
                }
                let _ = out.write_char(']');
            }
            PdfValue::Dict(entries) => write_dict(out, entries, refs),
            PdfValue::Ref(i) => {
                let _ = write!(out, "{} 0 R", refs(*i));
            }
            // Every stored value was resolved; never reached.
            PdfValue::Named(_) => {
                let _ = out.write_str("null");
            }
        }
    }
}

fn write_dict(
    out: &mut dyn std::fmt::Write,
    entries: &[(String, PdfValue)],
    refs: &dyn Fn(usize) -> usize,
) {
    let _ = out.write_str("<<");
    write_entries(out, entries, refs);
    let _ = out.write_str(" >>");
}

impl NamedObj {
    /// Write the object's body. A stream's data is Flate-compressed unless
    /// the program gave a `/Filter` (then the data is taken as already
    /// encoded), and `/Length` is always the real one.
    fn write_pdf(&self, out: &mut Out, refs: &dyn Fn(usize) -> usize) {
        match self.kind {
            ObjKind::Dict => write_dict(out, &self.dict, refs),
            ObjKind::Array => {
                out.bytes(b"[");
                for (i, v) in self.items.iter().enumerate() {
                    if i > 0 {
                        out.bytes(b" ");
                    }
                    v.write_pdf(out, refs);
                }
                out.bytes(b"]");
            }
            ObjKind::Stream => {
                // `/DecodeParms` without `/Filter` means nothing to a
                // reader, but pscat's own Flate filter must not pick it up.
                let has_filter = self
                    .dict
                    .iter()
                    .any(|(k, _)| k == "Filter" || k == "DecodeParms");
                let packed = if has_filter {
                    None
                } else {
                    match deflate_budgeted(&[&self.data], out.probe) {
                        Ok(z) => Some(z),
                        Err(e) => {
                            let _ = out.fail(e);
                            return;
                        }
                    }
                };
                out.bytes(b"<<");
                write_entries(out, &self.dict, refs);
                let body: &[u8] = match &packed {
                    Some(z) => {
                        out.bytes(b" /Filter /FlateDecode");
                        z
                    }
                    None => &self.data,
                };
                let _ = write!(out, " /Length {} >>\nstream\n", body.len());
                out.bytes(body);
                out.bytes(b"\nendstream");
            }
        }
    }
}

/// Scan a PostScript source's DSC header comments for document
/// metadata: `%%Title:` and, for the author, `%%For:` (the DSC-correct
/// keyword — takes precedence if both appear) or `%%Author:` (a common
/// non-standard alias, used only when `%%For:` is absent). Stops at
/// `%%EndComments` or the first non-comment, non-blank line, whichever
/// comes first — `%%EndComments` is DSC's authoritative header
/// boundary, and stopping there (rather than only at the first
/// non-`%` line) keeps a comment-only prologue, or an embedded
/// document's own `%%Title:`/`%%For:`, from overwriting the outer
/// document's metadata.
///
/// Verified against gs, this project's semantics oracle: `gs
/// -sDEVICE=pdfwrite` on a file with `%%Title:`/`%%For:` header
/// comments embeds them as the output PDF's document metadata
/// (`dc:title`/`dc:creator` XMP entries) — so honoring the same two
/// comments here matches an established convention, not a new one.
pub fn scan_document_info(source: &[u8]) -> (Option<String>, Option<String>) {
    let mut title = None;
    let mut author = None;
    for raw_line in source.split(|&b| b == b'\n') {
        let line = String::from_utf8_lossy(raw_line);
        let line = line.trim_end_matches('\r').trim_start();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("%%Title:") {
            title = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("%%For:") {
            author = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("%%Author:") {
            author.get_or_insert_with(|| rest.trim().to_string());
        } else if line.starts_with("%%EndComments") || !line.starts_with('%') {
            break;
        }
    }
    (title, author)
}

#[cfg(test)]
mod tests {
    #[test]
    fn number_formatting_trims_and_never_prints_negative_zero() {
        assert_eq!(f(-0.0), "0");
        assert_eq!(f(-0.0004), "0");
        assert_eq!(f(0.125), "0.125");
        assert_eq!(f(0.1239), "0.124");
        assert_eq!(path_coord(-0.004), "0");
        assert_eq!(path_coord(-0.006), "-0.01");
        assert_eq!(path_coord(100.0), "100");
        assert_eq!(path_coord(12.5), "12.5");
        assert_eq!(path_coord(10.126), "10.13");
        assert_eq!(fmt_num(100.0, 0), "100");
        assert_eq!(fmt_num(-0.4, 0), "0");
    }

    use super::*;

    #[test]
    fn scans_title_and_for() {
        let src = b"%!PS-Adobe-3.0\n%%Title: Moonlit Poem\n%%For: Jerome\n%%EndComments\n/x 1 def\n%%Title: not this one\n";
        let (title, author) = scan_document_info(src);
        assert_eq!(title.as_deref(), Some("Moonlit Poem"));
        assert_eq!(author.as_deref(), Some("Jerome"));
    }

    #[test]
    fn falls_back_to_author_when_for_absent() {
        let src = b"%!PS-Adobe-3.0\n%%Author: Jerome\n%%EndComments\n";
        let (_, author) = scan_document_info(src);
        assert_eq!(author.as_deref(), Some("Jerome"));
    }

    #[test]
    fn for_wins_over_author_regardless_of_order() {
        let src = b"%%Author: Wrong\n%%For: Right\n";
        let (_, author) = scan_document_info(src);
        assert_eq!(author.as_deref(), Some("Right"));

        let src2 = b"%%For: Right\n%%Author: Wrong\n";
        let (_, author2) = scan_document_info(src2);
        assert_eq!(author2.as_deref(), Some("Right"));
    }

    #[test]
    fn stops_at_first_non_comment_line() {
        let src = b"%%Title: Real Title\n/x 1 def\n%%Title: Ignored\n";
        let (title, _) = scan_document_info(src);
        assert_eq!(title.as_deref(), Some("Real Title"));
    }

    #[test]
    fn stops_at_end_comments_even_amid_a_comment_only_prologue() {
        // A %%Title: after %%EndComments but still before any executable
        // line (e.g. an embedded document's own header) must not override
        // the outer document's metadata.
        let src = b"%%Title: Outer\n%%EndComments\n%%Title: Inner, ignored\n/x 1 def\n";
        let (title, _) = scan_document_info(src);
        assert_eq!(title.as_deref(), Some("Outer"));
    }

    #[test]
    fn no_header_comments_yields_none() {
        let (title, author) = scan_document_info(b"/x 1 def\n");
        assert_eq!(title, None);
        assert_eq!(author, None);
    }

    #[test]
    fn pdf_string_escapes_ascii_specials() {
        assert_eq!(pdf_string("plain"), "(plain)");
        assert_eq!(pdf_string("A (Test) Title"), "(A \\(Test\\) Title)");
        assert_eq!(pdf_string("back\\slash"), "(back\\\\slash)");
    }

    #[test]
    fn pdf_string_uses_utf16be_hex_for_non_ascii() {
        // U+B2EC (달, "moon") -> UTF-16BE 0xB2EC, BOM-prefixed hex string.
        let s = pdf_string("달");
        assert_eq!(s, "<FEFFB2EC>");
    }
}
