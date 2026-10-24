//! Composite same-sized rendered pages into one grid PNG (issue #21):
//! a seed/parameter sweep's frames laid out side by side so an agent
//! can compare results in one image instead of opening N files.
//!
//! Every sweep frame renders at the same `--page`/`--dpi`, so cells
//! are always uniform size — this is a plain row-major byte copy, no
//! scaling or blending. Pages are opaque (see `halftone.rs`'s same
//! observation), so a straight RGBA copy is exact.

use tiny_skia::Pixmap;

/// Matches `--page`'s own per-side ceiling (`main.rs`) — a sweep
/// shouldn't be able to blow past the allocation guard a single
/// render already respects just by asking for more frames.
pub const MAX_SIDE_PX: u32 = 8000;

/// Gap in device pixels painted white between cells.
pub const GAP: u32 = 4;

/// Allocate a blank (white) `cols`×`rows` grid sized for `cell_w`×
/// `cell_h` cells with `gap` device pixels between them. Errors
/// instead of allocating past [`MAX_SIDE_PX`] on a side — a caller
/// (`main.rs`'s sweep loop) calls this *before* rendering any frames,
/// so an oversized request fails fast instead of after N renders.
pub fn new_sheet(
    cols: u32,
    rows: u32,
    cell_w: u32,
    cell_h: u32,
    gap: u32,
) -> Result<Pixmap, String> {
    // u64 + checked arithmetic (not u32 `*`/`+`, which `main.rs`'s own
    // caller happens to stay under but this is a public function): a
    // caller passing large enough values could overflow u32 before
    // the MAX_SIDE_PX check ever runs -- a debug-build panic, or
    // release-mode wraparound silently accepting an oversized request
    // as a small one. u64 alone pushes the same risk out rather than
    // closing it (two near-u32::MAX terms summed can still overflow
    // u64), so this checks for real, not just further away (round-5
    // cross-model review, PR #66).
    let too_large = || "contact sheet dimensions overflow".to_string();
    let dim = |cells: u32, cell: u32| -> Result<u64, String> {
        (cells as u64)
            .checked_mul(cell as u64)
            .and_then(|a| a.checked_add((gap as u64).checked_mul(cells.saturating_sub(1) as u64)?))
            .ok_or_else(too_large)
    };
    let (sheet_w, sheet_h) = (dim(cols, cell_w)?, dim(rows, cell_h)?);
    if sheet_w > MAX_SIDE_PX as u64 || sheet_h > MAX_SIDE_PX as u64 {
        return Err(format!(
            "contact sheet would be {sheet_w}x{sheet_h}px, over the {MAX_SIDE_PX}px-per-side \
             limit -- use fewer sweep values, a smaller --page/--dpi, or drop --contact-sheet \
             for individual --png frames"
        ));
    }
    // Safe: just checked both are <= MAX_SIDE_PX, which fits u32.
    let (sheet_w, sheet_h) = (sheet_w as u32, sheet_h as u32);
    let mut sheet = Pixmap::new(sheet_w, sheet_h)
        .ok_or_else(|| format!("cannot allocate a {sheet_w}x{sheet_h}px contact sheet"))?;
    sheet.fill(tiny_skia::Color::WHITE);
    Ok(sheet)
}

/// Copy `frame` into cell `index` (row-major: 0 top-left, 1 to its
/// right, ...) of a `cols`-wide grid on `sheet`, with `gap` device
/// pixels between cells. `frame` must fit within one cell (true for
/// every sweep frame -- all render at the same `--page`/`--dpi`).
pub fn blit_cell(sheet: &mut Pixmap, cols: u32, gap: u32, index: usize, frame: &Pixmap) {
    let (w, h) = (frame.width(), frame.height());
    let (col, row) = (index as u32 % cols, index as u32 / cols);
    let (ox, oy) = (col * (w + gap), row * (h + gap));
    let sw = sheet.width();
    let sdata = sheet.data_mut();
    let pdata = frame.data();
    for y in 0..h {
        let src = &pdata[(y * w * 4) as usize..((y * w + w) * 4) as usize];
        let dst = (((oy + y) * sw + ox) * 4) as usize;
        sdata[dst..dst + src.len()].copy_from_slice(src);
    }
}

/// Lay `pages` into a `cols`×`rows` grid in one call — the batch form
/// of [`new_sheet`]/[`blit_cell`], for a caller that already has every
/// frame in memory (this module's own tests; `main.rs`'s sweep loop
/// uses the two primitives directly to stream frames instead). Extra
/// cells beyond `pages.len()` stay white.
pub fn compose(pages: &[Pixmap], cols: u32, rows: u32, gap: u32) -> Result<Pixmap, String> {
    let Some(first) = pages.first() else {
        return Err("contact sheet: no frames to compose".to_string());
    };
    // main.rs's sweep loop validates this itself before ever calling
    // new_sheet/blit_cell directly, but compose is a public API of
    // its own -- a caller that skips that check would otherwise send
    // an out-of-grid index into blit_cell's slice arithmetic and
    // panic (round-4 cross-model review, PR #66).
    if (cols as u64) * (rows as u64) < pages.len() as u64 {
        return Err(format!(
            "contact sheet: {cols}x{rows} grid has only {} cells for {} frames",
            cols as u64 * rows as u64,
            pages.len()
        ));
    }
    let mut sheet = new_sheet(cols, rows, first.width(), first.height(), gap)?;
    for (i, page) in pages.iter().enumerate() {
        blit_cell(&mut sheet, cols, gap, i, page);
    }
    Ok(sheet)
}

// ---------------------------------------------------------------
// Labelled sheets with mixed cell sizes (issue #693, `pscat sheet`).
//
// The grid above is for a sweep's same-sized frames. A review sheet --
// logo candidates each at 512 px and at 48 px, with names -- needs rows
// of cells of different sizes, and text. `Layout` is the pure geometry
// (it knows sizes, never pixels, so a caller can reject an oversized
// sheet before rendering a single cell); the `blit_at`/`frame` helpers
// and `render_text` do the drawing, with the labels set by pscat's own
// PostScript text rendering rather than a second imaging stack.
// ---------------------------------------------------------------

/// White margin around the whole sheet, in pixels.
pub const MARGIN: u32 = 12;
/// Horizontal gap between columns.
pub const COL_GAP: u32 = 14;
/// Vertical gap between rows.
pub const ROW_GAP: u32 = 18;
/// Gap between a cell's border and the text above or below it.
pub const TEXT_GAP: u32 = 5;
/// Width of the hairline drawn just outside each image, so a pale image
/// is still visible against the white sheet. It is added to the cell's
/// footprint, never drawn over the image.
pub const BORDER: u32 = 1;

/// One cell's footprint: the image, and its caption (`(0, 0)` for none).
#[derive(Clone, Copy, Debug)]
pub struct CellSize {
    pub image: (u32, u32),
    pub caption: (u32, u32),
}

/// One row: an optional header line above it and its cells, left to right.
#[derive(Clone, Debug)]
pub struct RowSize {
    pub header: (u32, u32),
    pub cells: Vec<CellSize>,
}

/// Where everything goes, from [`Layout::new`]. Columns are as wide as
/// their widest cell or caption, and each cell is centred in its slot,
/// so a 48 px render sits at true size beside a 512 px one.
#[derive(Debug)]
pub struct Layout {
    pub width: u32,
    pub height: u32,
    /// Top-left of each row's header text.
    pub header_xy: Vec<(u32, u32)>,
    /// Top-left of each image (inside its border), by row then column.
    pub image_xy: Vec<Vec<(u32, u32)>>,
    /// Top-left of each caption, by row then column.
    pub caption_xy: Vec<Vec<(u32, u32)>>,
}

impl Layout {
    /// Place `rows`. Errors, rather than allocating, past
    /// [`MAX_SIDE_PX`] on a side, or for an empty sheet.
    pub fn new(rows: &[RowSize]) -> Result<Layout, String> {
        if rows.is_empty() || rows.iter().all(|r| r.cells.is_empty()) {
            return Err("contact sheet: no cells to lay out".to_string());
        }
        let too_large = || "contact sheet dimensions overflow".to_string();
        let ncols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        let b2 = 2 * BORDER as u64;
        let mut col_w = vec![0u64; ncols];
        for row in rows {
            for (c, cell) in row.cells.iter().enumerate() {
                let w = (cell.image.0 as u64 + b2).max(cell.caption.0 as u64);
                col_w[c] = col_w[c].max(w);
            }
        }
        let header_w = rows.iter().map(|r| r.header.0 as u64).max().unwrap_or(0);
        let cols_total: u64 = col_w.iter().sum::<u64>() + COL_GAP as u64 * (ncols as u64 - 1);
        let width = (cols_total.max(header_w))
            .checked_add(2 * MARGIN as u64)
            .ok_or_else(too_large)?;

        // Row bands, top to bottom.
        let mut y = MARGIN as u64;
        let mut bands = Vec::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            if i > 0 {
                y = y.checked_add(ROW_GAP as u64).ok_or_else(too_large)?;
            }
            let header_h = if row.header.1 > 0 {
                row.header.1 as u64 + TEXT_GAP as u64
            } else {
                0
            };
            let image_h = row
                .cells
                .iter()
                .map(|c| c.image.1 as u64 + b2)
                .max()
                .unwrap_or(0);
            let caption_h = row
                .cells
                .iter()
                .map(|c| c.caption.1 as u64)
                .max()
                .unwrap_or(0);
            let caption_h = if caption_h > 0 {
                caption_h + TEXT_GAP as u64
            } else {
                0
            };
            bands.push((y, header_h, image_h));
            y = y
                .checked_add(header_h + image_h + caption_h)
                .ok_or_else(too_large)?;
        }
        let height = y.checked_add(MARGIN as u64).ok_or_else(too_large)?;
        if width > MAX_SIDE_PX as u64 || height > MAX_SIDE_PX as u64 {
            return Err(format!(
                "contact sheet would be {width}x{height}px, over the {MAX_SIDE_PX}px-per-side \
                 limit -- use fewer cells, a lower --dpi, or a smaller --page"
            ));
        }

        let mut col_x = Vec::with_capacity(ncols);
        let mut x = MARGIN as u64;
        for w in &col_w {
            col_x.push(x);
            x += w + COL_GAP as u64;
        }
        let mut layout = Layout {
            width: width as u32,
            height: height as u32,
            header_xy: Vec::new(),
            image_xy: Vec::new(),
            caption_xy: Vec::new(),
        };
        for (row, &(top, header_h, image_h)) in rows.iter().zip(&bands) {
            layout.header_xy.push((MARGIN, top as u32));
            let band_top = top + header_h;
            let mut images = Vec::new();
            let mut captions = Vec::new();
            for (c, cell) in row.cells.iter().enumerate() {
                let slot = col_w[c];
                let outer_w = cell.image.0 as u64 + b2;
                let outer_h = cell.image.1 as u64 + b2;
                let ix = col_x[c] + (slot - outer_w) / 2 + BORDER as u64;
                let iy = band_top + (image_h - outer_h) / 2 + BORDER as u64;
                images.push((ix as u32, iy as u32));
                let cx = col_x[c] + slot.saturating_sub(cell.caption.0 as u64) / 2;
                let cy = band_top + image_h + TEXT_GAP as u64;
                captions.push((cx as u32, cy as u32));
            }
            layout.image_xy.push(images);
            layout.caption_xy.push(captions);
        }
        Ok(layout)
    }

    /// A blank white sheet of this layout's size.
    pub fn new_sheet(&self) -> Result<Pixmap, String> {
        let mut sheet = Pixmap::new(self.width, self.height).ok_or_else(|| {
            format!(
                "cannot allocate a {}x{}px contact sheet",
                self.width, self.height
            )
        })?;
        sheet.fill(tiny_skia::Color::WHITE);
        Ok(sheet)
    }
}

/// Copy `src` onto `sheet` with its top-left at `(x, y)`, clipped to the
/// sheet. A straight byte copy, as [`blit_cell`] is: pages and the text
/// strips from [`render_text`] are opaque.
pub fn blit_at(sheet: &mut Pixmap, x: u32, y: u32, src: &Pixmap) {
    let (sw, sh) = (sheet.width(), sheet.height());
    if x >= sw || y >= sh {
        return;
    }
    let w = src.width().min(sw - x) as usize;
    let rows = src.height().min(sh - y);
    let (src_stride, dst_stride) = (src.width() as usize * 4, sw as usize * 4);
    let sdata = sheet.data_mut();
    let pdata = src.data();
    for row in 0..rows as usize {
        let s = row * src_stride;
        let d = (y as usize + row) * dst_stride + x as usize * 4;
        sdata[d..d + w * 4].copy_from_slice(&pdata[s..s + w * 4]);
    }
}

/// Draw the hairline [`BORDER`] around the `w`x`h` image whose top-left
/// is `(x, y)`: the ring of pixels just outside it. Call before
/// [`blit_at`]ing the image, so the border never covers a pixel of it.
pub fn frame(sheet: &mut Pixmap, x: u32, y: u32, w: u32, h: u32) {
    let (x0, y0) = (x.saturating_sub(BORDER), y.saturating_sub(BORDER));
    let (outer_w, outer_h) = (w + (x - x0) + BORDER, h + (y - y0) + BORDER);
    let mut ring = Pixmap::new(outer_w, outer_h).expect("a border is never empty");
    ring.fill(tiny_skia::Color::from_rgba8(190, 190, 190, 255));
    blit_at(sheet, x0, y0, &ring);
}

/// Scale `src` up by the whole factor `k` with nearest-neighbour
/// sampling: every source pixel becomes a `k`x`k` block, so a 48 px
/// icon can be inspected pixel by pixel. `None` if the result would
/// be past [`MAX_SIDE_PX`] on a side.
pub fn zoom_nearest(src: &Pixmap, k: u32) -> Option<Pixmap> {
    let (w, h) = (src.width().checked_mul(k)?, src.height().checked_mul(k)?);
    if w > MAX_SIDE_PX || h > MAX_SIDE_PX {
        return None;
    }
    let mut out = Pixmap::new(w, h)?;
    let (sw, k) = (src.width() as usize, k as usize);
    let sdata = src.data();
    let odata = out.data_mut();
    for oy in 0..h as usize {
        let srow = (oy / k) * sw;
        for ox in 0..w as usize {
            let s = (srow + ox / k) * 4;
            let d = (oy * w as usize + ox) * 4;
            odata[d..d + 4].copy_from_slice(&sdata[s..s + 4]);
        }
    }
    Some(out)
}

/// The face a run of a label is set in. Nothing in pscat picked a face
/// by script before (the Stage 22/23 faces are chosen by name in the
/// program), so this is the one place that maps a codepoint to a face:
/// Latin in the built-in Helvetica (accents too, through
/// `ISOLatin1Encoding`), and Hangul, kana/kanji and Thai in the
/// Unicode-mode Noto Sans catalog faces from the `cjk-thai` font pack.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Face {
    Latin,
    Hangul,
    Japanese,
    Thai,
}

impl Face {
    /// The catalog face's name; `None` for the built-in Latin face.
    fn catalog_name(self) -> Option<&'static str> {
        match self {
            Face::Latin => None,
            Face::Hangul => Some("NotoSansKR-Regular"),
            Face::Japanese => Some("NotoSansJP-Regular"),
            Face::Thai => Some("NotoSansThai-Regular"),
        }
    }
}

/// How one character takes part in splitting a label into runs.
enum Class {
    /// Space and ASCII punctuation: joins the run before it, so a `(` or
    /// a space does not flip the face.
    Neutral,
    /// A character with a face of its own.
    Strong(Face),
    /// No face covers it: drawn as `?`, and neutral.
    Missing,
}

fn classify(c: char) -> Class {
    match c as u32 {
        0x20..=0x2f | 0x3a..=0x40 | 0x5b..=0x60 | 0x7b..=0x7e => Class::Neutral,
        0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0xa1..=0xff => Class::Strong(Face::Latin),
        // Jamo, compatibility jamo, Hangul syllables.
        0x1100..=0x11ff | 0x3130..=0x318f | 0xa960..=0xa97f | 0xac00..=0xd7ff => {
            Class::Strong(Face::Hangul)
        }
        // CJK punctuation, kana, ideographs (Han is set in the Japanese
        // face: pscat ships no Chinese-specific one), fullwidth forms.
        0x3000..=0x30ff
        | 0x31f0..=0x31ff
        | 0x3400..=0x4dbf
        | 0x4e00..=0x9fff
        | 0xf900..=0xfaff
        | 0xff00..=0xff9f => Class::Strong(Face::Japanese),
        0x0e00..=0x0e7f => Class::Strong(Face::Thai),
        _ => Class::Missing,
    }
}

/// Base letter then combining mark, as the Latin-1 letter they compose
/// to: `(mark, "<base><composed>" pairs)`.
const LATIN1_COMPOSE: [(char, &str); 7] = [
    (
        '\u{300}',
        "A\u{c0}E\u{c8}I\u{cc}O\u{d2}U\u{d9}a\u{e0}e\u{e8}i\u{ec}o\u{f2}u\u{f9}",
    ),
    (
        '\u{301}',
        "A\u{c1}E\u{c9}I\u{cd}O\u{d3}U\u{da}Y\u{dd}a\u{e1}e\u{e9}i\u{ed}o\u{f3}u\u{fa}y\u{fd}",
    ),
    (
        '\u{302}',
        "A\u{c2}E\u{ca}I\u{ce}O\u{d4}U\u{db}a\u{e2}e\u{ea}i\u{ee}o\u{f4}u\u{fb}",
    ),
    ('\u{303}', "A\u{c3}N\u{d1}O\u{d5}a\u{e3}n\u{f1}o\u{f5}"),
    (
        '\u{308}',
        "A\u{c4}E\u{cb}I\u{cf}O\u{d6}U\u{dc}a\u{e4}e\u{eb}i\u{ef}o\u{f6}u\u{fc}y\u{ff}",
    ),
    ('\u{30a}', "A\u{c5}a\u{e5}"),
    ('\u{327}', "C\u{c7}c\u{e7}"),
];

/// `text` with the decomposed forms macOS file names use composed: a
/// Latin letter plus a combining mark becomes its Latin-1 letter, and
/// Hangul jamo become syllables, so the default label of `café.ps` or
/// `한글.ps` (stored decomposed) shows. Only what the faces here can
/// draw is composed; any other sequence is left for [`classify`].
fn precompose(text: &str) -> String {
    let mut out: Vec<char> = Vec::with_capacity(text.len());
    for c in text.chars() {
        let composed = out.last().and_then(|&last| match (last as u32, c as u32) {
            // Leading consonant + vowel, then + trailing consonant.
            (l @ 0x1100..=0x1112, v @ 0x1161..=0x1175) => {
                char::from_u32(0xac00 + ((l - 0x1100) * 21 + (v - 0x1161)) * 28)
            }
            (s @ 0xac00..=0xd7a3, t @ 0x11a8..=0x11c2) if (s - 0xac00) % 28 == 0 => {
                char::from_u32(s + (t - 0x11a7))
            }
            _ => LATIN1_COMPOSE
                .iter()
                .find(|(mark, _)| *mark == c)
                .and_then(|(_, pairs)| {
                    let pairs: Vec<char> = pairs.chars().collect();
                    pairs.chunks(2).find(|p| p[0] == last).map(|p| p[1])
                }),
        });
        match composed {
            Some(x) => *out.last_mut().unwrap() = x,
            None => out.push(c),
        }
    }
    out.into_iter().collect()
}

/// `text` split into runs of one face each. Neutral characters join the
/// run before them (or, at the start, the first run). Controls and
/// characters no face covers become `?`.
fn segment(text: &str) -> Vec<(Face, String)> {
    let mut runs: Vec<(Face, String)> = Vec::new();
    // Neutrals seen before any strong character.
    let mut lead = String::new();
    for c in text.chars() {
        let (face, c) = match classify(c) {
            Class::Strong(f) => (Some(f), c),
            Class::Neutral => (None, c),
            Class::Missing => (None, '?'),
        };
        match (face, runs.last_mut()) {
            (Some(f), Some((last, run))) if *last == f => run.push(c),
            (Some(f), _) => {
                let mut run = std::mem::take(&mut lead);
                run.push(c);
                runs.push((f, run));
            }
            (None, Some((_, run))) => run.push(c),
            (None, None) => lead.push(c),
        }
    }
    if !lead.is_empty() {
        runs.push((Face::Latin, lead));
    }
    runs
}

/// A PostScript string literal body for `run`. A Latin run is one byte
/// per character (`ISOLatin1Encoding` for the accented ones, as octal
/// escapes); the Unicode-mode faces take the UTF-8 text as it is.
fn literal(face: Face, run: &str) -> String {
    let mut lit = String::with_capacity(run.len());
    for c in run.chars() {
        match c {
            '\\' | '(' | ')' => {
                lit.push('\\');
                lit.push(c);
            }
            ' '..='~' => lit.push(c),
            _ if face == Face::Latin => lit.push_str(&format!("\\{:03o}", c as u32)),
            _ => lit.push(c),
        }
    }
    lit
}

/// Redefines `Helvetica`/`Helvetica-Bold` under `ISOLatin1Encoding` as
/// `/PscatSheetLatin1[Bold]`, for a label with accented letters.
/// The copy keeps `FID`: pscat finds the outlines through it, and
/// `definefont` gives a dict without one the substitute (regular) face.
const LATIN1_PRELUDE: &str = "/Helvetica /PscatSheetLatin1 /Helvetica-Bold /PscatSheetLatin1Bold \
     2 { exch findfont dup length dict begin \
         { def } forall \
         /Encoding ISOLatin1Encoding def currentdict end definefont pop } repeat";

/// Text as a tight, opaque strip: black-ish `text` at `pt` pixels on
/// white, drawn by pscat's own interpreter and fonts, one point per
/// pixel. The face is picked per character by script (see [`Face`]):
/// `Helvetica` (`Helvetica-Bold` if `bold`) for Latin, accents included;
/// Noto Sans KR, JP or Thai for Hangul, kana/kanji or Thai. Those three
/// live in the `cjk-thai` font pack: when it is not installed the
/// characters are drawn as `?` and one warning per face names the pack,
/// on stderr. Any other character is drawn as `?`. `None` for an empty
/// `text`.
pub fn render_text(text: &str, pt: u32, bold: bool) -> Result<Option<Pixmap>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let mut runs = segment(&precompose(text));
    // A face from a pack that is not installed: its characters are `?`
    // (the ASCII spaces and punctuation that joined the run stay).
    for face in [Face::Hangul, Face::Japanese, Face::Thai] {
        if runs.iter().any(|(f, _)| *f == face) && !face_available(face) {
            for (f, run) in &mut runs {
                if *f == face {
                    *run = unavailable(run);
                    *f = Face::Latin;
                }
            }
        }
    }
    // A character its face has no glyph for (a Simplified-only Han
    // character in the Japanese face) would draw as nothing: `?` instead.
    for (face, run) in &mut runs {
        if let Some(name) = face.catalog_name() {
            *run = run
                .chars()
                .map(|c| {
                    if c.is_ascii() || crate::font::has_glyph(name, c) {
                        c
                    } else {
                        '?'
                    }
                })
                .collect();
        }
    }
    // (setup, literal) per run.
    let latin = if bold { "Helvetica-Bold" } else { "Helvetica" };
    let mut prelude = "";
    let parts: Vec<(String, String)> = runs
        .iter()
        .map(|(face, run)| {
            let font = match face.catalog_name() {
                Some(name) => name,
                None if run.is_ascii() => latin,
                None => {
                    prelude = LATIN1_PRELUDE;
                    if bold {
                        "PscatSheetLatin1Bold"
                    } else {
                        "PscatSheetLatin1"
                    }
                }
            };
            (
                format!("/{font} findfont {pt} scalefont setfont"),
                literal(*face, run),
            )
        })
        .collect();
    // Measure first, then draw on a page of exactly the text's size.
    let mut probe = crate::Interp::new();
    probe.capture_stdout();
    let measure: String = parts
        .iter()
        .map(|(setup, lit)| format!("{setup} ({lit}) stringwidth pop =\n"))
        .collect();
    probe
        .run_source(format!("{prelude}\n{measure}").as_bytes())
        .map_err(|e| format!("label: {}", probe.error_report(&e)))?;
    let width: f32 = String::from_utf8_lossy(probe.captured_stdout())
        .lines()
        .map(|l| l.trim().parse::<f32>())
        .sum::<Result<f32, _>>()
        .map_err(|_| "label: could not measure the text".to_string())?;
    // Latin: ascent ~0.72 em, descent ~0.21 em, so a 1.3 em strip with
    // the baseline 0.3 em up clips neither. The Noto faces reach ~0.84 em
    // up and ~0.26 em down (Thai below-vowels), so a strip with one gets
    // 0.1 em more of room.
    let (tall, low) = if runs.iter().any(|(f, _)| *f != Face::Latin) {
        (1.4, 0.36)
    } else {
        (1.3, 0.3)
    };
    let w = (width.ceil() as u32 + 2).clamp(1, MAX_SIDE_PX);
    let h = (pt as f32 * tall).ceil() as u32;
    let base = (pt as f32 * low).ceil() as u32;
    let mut interp = crate::Interp::with_page(w, h)
        .ok_or_else(|| "label: unusable text strip size".to_string())?;
    let mut draw = format!("{prelude}\n0.1 setgray 1 {base} moveto\n");
    for (setup, lit) in &parts {
        draw.push_str(&format!("{setup} ({lit}) show\n"));
    }
    interp
        .run_source(draw.as_bytes())
        .map_err(|e| format!("label: {}", interp.error_report(&e)))?;
    Ok(Some(interp.gfx().pixmap.clone()))
}

/// `run` for a face that is not installed: every non-ASCII character is
/// `?`; the ASCII spaces and punctuation that joined the run stay.
fn unavailable(run: &str) -> String {
    run.chars()
        .map(|c| if c.is_ascii() { c } else { '?' })
        .collect()
}

/// Whether `face`'s font loads; when it does not, warns once per face
/// per process with pscat's own hint naming the font pack.
fn face_available(face: Face) -> bool {
    use std::sync::{Mutex, OnceLock};
    static WARNED: OnceLock<Mutex<Vec<Face>>> = OnceLock::new();
    let Some(name) = face.catalog_name() else {
        return true;
    };
    let mut probe = crate::Interp::new();
    let Err(e) = probe.run_source(format!("/{name} findfont pop").as_bytes()) else {
        return true;
    };
    let mut warned = WARNED.get_or_init(Default::default).lock().unwrap();
    if !warned.contains(&face) {
        warned.push(face);
        let report = probe.error_report(&e);
        // The hint is the report's last line, already prefixed `pscat: `.
        let hint = report.lines().last().unwrap_or(&report);
        eprintln!("pscat sheet: {}", hint.trim_start_matches("pscat: "));
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(
            rgba[0], rgba[1], rgba[2], rgba[3],
        ));
        p
    }

    #[test]
    fn two_by_one_places_cells_left_to_right() {
        let red = solid(4, 4, [255, 0, 0, 255]);
        let blue = solid(4, 4, [0, 0, 255, 255]);
        let sheet = compose(&[red, blue], 2, 1, 0).unwrap();
        assert_eq!(sheet.width(), 8);
        assert_eq!(sheet.height(), 4);
        let px = |x: u32, y: u32| {
            let i = ((y * sheet.width() + x) * 4) as usize;
            &sheet.data()[i..i + 4]
        };
        assert_eq!(px(0, 0), &[255, 0, 0, 255]);
        assert_eq!(px(4, 0), &[0, 0, 255, 255]);
    }

    #[test]
    fn gap_leaves_a_white_seam() {
        let a = solid(2, 2, [0, 0, 0, 255]);
        let b = solid(2, 2, [0, 0, 0, 255]);
        let sheet = compose(&[a, b], 2, 1, 2).unwrap();
        assert_eq!(sheet.width(), 6); // 2 + 2(gap) + 2
        let i = (2 * 4) as usize; // first gap column, row 0
        assert_eq!(&sheet.data()[i..i + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn oversized_grid_errors_instead_of_allocating() {
        let page = solid(1000, 1000, [0, 0, 0, 255]);
        let pages: Vec<_> = std::iter::repeat_n(page, 9).collect();
        let err = compose(&pages, 9, 9, 0).unwrap_err();
        assert!(err.contains("over the"), "{err}");
    }

    /// Regression (round-4 cross-model review, PR #66): a grid with
    /// fewer cells than pages used to send an out-of-grid index into
    /// `blit_cell`'s slice arithmetic and panic, rather than erroring.
    #[test]
    fn grid_too_small_for_pages_errors_instead_of_panicking() {
        let a = solid(2, 2, [0, 0, 0, 255]);
        let b = solid(2, 2, [0, 0, 0, 255]);
        let err = compose(&[a, b], 1, 1, 0).unwrap_err();
        assert!(err.contains("only 1 cells for 2 frames"), "{err}");
    }

    /// Regression (round-5 cross-model review, PR #66): `new_sheet`
    /// is a public function, not just `main.rs`'s own already-bounded
    /// caller -- large enough `cols`/`cell_w`/`gap` must error, not
    /// overflow `u32` (a debug-build panic) or wrap around in release.
    #[test]
    fn extreme_dimensions_error_instead_of_overflowing() {
        assert!(new_sheet(u32::MAX, 2, u32::MAX, 2, u32::MAX).is_err());
        assert!(new_sheet(u32::MAX / 2, 1, 3, 1, 0).is_err());
    }

    fn cell(w: u32, h: u32, cap: (u32, u32)) -> CellSize {
        CellSize {
            image: (w, h),
            caption: cap,
        }
    }

    #[test]
    fn layout_centres_cells_in_columns_as_wide_as_the_widest() {
        let rows = [
            RowSize {
                header: (0, 0),
                cells: vec![cell(100, 100, (0, 0)), cell(20, 20, (0, 0))],
            },
            RowSize {
                header: (0, 0),
                cells: vec![cell(60, 40, (0, 0))],
            },
        ];
        let l = Layout::new(&rows).unwrap();
        // 12 + 102 + 14 + 22 + 12, and 12 + 102 + 18 + 42 + 12.
        assert_eq!((l.width, l.height), (162, 186));
        assert_eq!(l.image_xy[0][0], (13, 13));
        // 20 px cell: column starts 12+102+14 = 128, slot 22, so no offset.
        assert_eq!(l.image_xy[0][1], (129, 13 + 40));
        // Row two's 60 px cell, in the 102 px first column: (102-62)/2 = 20.
        assert_eq!(l.image_xy[1][0].0, 12 + 20 + 1);
    }

    #[test]
    fn layout_widens_a_column_for_its_caption_and_the_sheet_for_a_header() {
        let rows = [RowSize {
            header: (300, 15),
            cells: vec![cell(10, 10, (50, 11))],
        }];
        let l = Layout::new(&rows).unwrap();
        assert_eq!(l.width, 12 + 300 + 12);
        // header 15 + 5 gap, image band 12, caption 11 + 5 gap.
        assert_eq!(l.height, 12 + 20 + 12 + 16 + 12);
        // Image centred in the 50 px caption column: (50-12)/2 + 1.
        assert_eq!(l.image_xy[0][0].0, 12 + 19 + 1);
        assert_eq!(l.caption_xy[0][0], (12, 12 + 20 + 12 + 5));
    }

    #[test]
    fn layout_errors_on_empty_and_oversized_sheets() {
        assert!(Layout::new(&[]).is_err());
        let wide = RowSize {
            header: (0, 0),
            cells: vec![cell(5000, 10, (0, 0)), cell(5000, 10, (0, 0))],
        };
        let err = Layout::new(&[wide]).unwrap_err();
        assert!(err.contains("over the"), "{err}");
        let huge = RowSize {
            header: (0, 0),
            cells: vec![cell(u32::MAX, u32::MAX, (u32::MAX, u32::MAX)); 2],
        };
        assert!(Layout::new(&[huge.clone(), huge]).is_err());
    }

    #[test]
    fn blit_at_clips_to_the_sheet() {
        let mut sheet = solid(4, 4, [255, 255, 255, 255]);
        let red = solid(3, 3, [255, 0, 0, 255]);
        blit_at(&mut sheet, 3, 3, &red);
        blit_at(&mut sheet, 9, 9, &red); // wholly outside: ignored
        let px = |x: usize, y: usize| sheet.data()[(y * 4 + x) * 4];
        assert_eq!((px(3, 3), px(2, 3), px(3, 2)), (255, 255, 255));
        let g = |x: usize, y: usize| sheet.data()[(y * 4 + x) * 4 + 1];
        assert_eq!((g(3, 3), g(2, 2)), (0, 255));
    }

    #[test]
    fn zoom_makes_exact_blocks_and_refuses_oversize() {
        let mut p = solid(2, 1, [255, 255, 255, 255]);
        p.data_mut()[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let z = zoom_nearest(&p, 3).unwrap();
        assert_eq!((z.width(), z.height()), (6, 3));
        let r = |x: usize, y: usize| z.data()[(y * 6 + x) * 4];
        assert_eq!((r(0, 0), r(2, 2), r(3, 0), r(5, 2)), (0, 0, 255, 255));
        let big = solid(MAX_SIDE_PX / 2 + 1, 1, [0, 0, 0, 255]);
        assert!(zoom_nearest(&big, 2).is_none());
    }

    #[test]
    fn render_text_makes_a_tight_inked_strip() {
        let strip = render_text("A: one-line cat", 15, true).unwrap().unwrap();
        assert!((60..200).contains(&strip.width()), "{}", strip.width());
        assert_eq!(strip.height(), 20);
        let dark = strip.data().chunks(4).filter(|p| p[0] < 100).count();
        assert!(dark > 40, "{dark} dark pixels");
        assert!(render_text("", 11, false).unwrap().is_none());
        // Metacharacters and non-ASCII cannot break the literal.
        assert!(render_text("a) (b \\ \u{e9}", 11, false).unwrap().is_some());
        assert!(render_text("%! (", 11, false).unwrap().is_some());
    }

    fn dark(p: &Pixmap) -> usize {
        p.data().chunks(4).filter(|p| p[0] < 128).count()
    }

    /// The pre-#707 pipeline for printable ASCII, kept as the reference
    /// a Latin-only label must still match to the byte.
    fn ascii_reference(text: &str, pt: u32, bold: bool) -> Pixmap {
        let mut lit = String::new();
        for c in text.chars() {
            if matches!(c, '\\' | '(' | ')') {
                lit.push('\\');
            }
            lit.push(c);
        }
        let font = if bold { "Helvetica-Bold" } else { "Helvetica" };
        let setup = format!("/{font} findfont {pt} scalefont setfont");
        let mut probe = crate::Interp::new();
        probe.capture_stdout();
        probe
            .run_source(format!("{setup} ({lit}) stringwidth pop =").as_bytes())
            .unwrap();
        let width: f32 = String::from_utf8_lossy(probe.captured_stdout())
            .trim()
            .parse()
            .unwrap();
        let w = width.ceil() as u32 + 2;
        let h = (pt as f32 * 1.3).ceil() as u32;
        let base = (pt as f32 * 0.3).ceil() as u32;
        let mut interp = crate::Interp::with_page(w, h).unwrap();
        interp
            .run_source(format!("{setup} 0.1 setgray 1 {base} moveto ({lit}) show").as_bytes())
            .unwrap();
        interp.gfx().pixmap.clone()
    }

    #[test]
    fn latin_only_labels_are_unchanged() {
        for (text, pt, bold) in [
            ("A: one-line cat", 16, true),
            ("128x128 px (shown at 4x)", 12, false),
            ("a) (b \\ %! ~", 16, true),
            ("logo_v2-final.final", 12, false),
        ] {
            let strip = render_text(text, pt, bold).unwrap().unwrap();
            let want = ascii_reference(text, pt, bold);
            assert_eq!(
                (strip.width(), strip.height()),
                (want.width(), want.height()),
                "{text}"
            );
            assert_eq!(strip.data(), want.data(), "{text}");
        }
    }

    fn runs(text: &str) -> Vec<(Face, &str)> {
        segment(text)
            .into_iter()
            .map(|(f, r)| (f, Box::leak(r.into_boxed_str()) as &str))
            .collect()
    }

    #[test]
    fn segment_splits_by_script_and_neutrals_join_the_run_before() {
        assert_eq!(runs("logo (a)"), [(Face::Latin, "logo (a)")]);
        assert_eq!(runs("caf\u{e9}"), [(Face::Latin, "caf\u{e9}")]);
        assert_eq!(
            runs("A: \u{d55c}\u{ae00} B"),
            [
                (Face::Latin, "A: "),
                (Face::Hangul, "\u{d55c}\u{ae00} "),
                (Face::Latin, "B"),
            ]
        );
        assert_eq!(
            runs("\u{65e5}\u{672c}\u{8a9e} \u{3072}\u{3089}"),
            [(Face::Japanese, "\u{65e5}\u{672c}\u{8a9e} \u{3072}\u{3089}")]
        );
        assert_eq!(
            runs("\u{e44}\u{e17}\u{e22}"),
            [(Face::Thai, "\u{e44}\u{e17}\u{e22}")]
        );
        // Leading neutrals go with the first run; no strong character is Latin.
        assert_eq!(runs("(\u{d55c}"), [(Face::Hangul, "(\u{d55c}")]);
        assert_eq!(runs("%! ("), [(Face::Latin, "%! (")]);
        // Controls and uncovered scripts are `?`.
        assert_eq!(runs("a\tb\u{1f600}\u{0416}"), [(Face::Latin, "a?b??")]);
    }

    #[test]
    fn a_missing_pack_keeps_ascii_punctuation_and_questions_the_rest() {
        assert_eq!(unavailable("(\u{d55c}\u{ae00}) 1"), "(??) 1");
    }

    #[test]
    fn precompose_joins_decomposed_latin_and_hangul() {
        assert_eq!(precompose("cafe\u{301} Nin\u{303}o"), "caf\u{e9} Ni\u{f1}o");
        assert_eq!(precompose("u\u{308}ber A\u{30a}"), "\u{fc}ber \u{c5}");
        assert_eq!(precompose("plain ASCII"), "plain ASCII");
        // L+V+T jamo, as macOS stores `\u{d55c}\u{ae00}`; LV without a T.
        assert_eq!(
            precompose("\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}"),
            "\u{d55c}\u{ae00}"
        );
        assert_eq!(precompose("\u{1100}\u{1161}"), "\u{ac00}");
        // A mark with no Latin-1 letter stays, and is drawn as `?`.
        assert_eq!(precompose("z\u{301}"), "z\u{301}");
    }

    #[test]
    fn a_decomposed_label_draws_like_the_composed_one() {
        for (nfd, nfc) in [
            ("cafe\u{301}", "caf\u{e9}"),
            (
                "\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}",
                "\u{d55c}\u{ae00}",
            ),
        ] {
            let a = render_text(nfd, 16, true).unwrap().unwrap();
            let b = render_text(nfc, 16, true).unwrap().unwrap();
            assert_eq!(a.data(), b.data(), "{nfd:?}");
        }
    }

    #[test]
    fn literal_escapes_metacharacters_and_latin1() {
        assert_eq!(literal(Face::Latin, "a(b)\\c"), "a\\(b\\)\\\\c");
        assert_eq!(literal(Face::Latin, "caf\u{e9}"), "caf\\351");
        assert_eq!(literal(Face::Hangul, "(\u{d55c})"), "\\(\u{d55c}\\)");
    }

    #[test]
    fn accented_latin_labels_show_the_accent() {
        let plain = render_text("cafe", 16, true).unwrap().unwrap();
        let accent = render_text("caf\u{e9}", 16, true).unwrap().unwrap();
        let query = render_text("caf?", 16, true).unwrap().unwrap();
        assert_ne!(accent.data(), plain.data());
        assert_ne!(accent.data(), query.data());
        // The acute adds ink above the x-height and keeps the width.
        assert_eq!(accent.width(), plain.width());
        assert!(dark(&accent) > dark(&plain));
        // Every Latin-1 letter draws (none is the `?` fallback).
        for c in "\u{c0}\u{c7}\u{d1}\u{dc}\u{df}\u{e5}\u{f8}\u{ff}".chars() {
            let strip = render_text(&c.to_string(), 16, false).unwrap().unwrap();
            let q = render_text("?", 16, false).unwrap().unwrap();
            assert_ne!(strip.data(), q.data(), "{c}");
        }
    }

    #[test]
    fn korean_japanese_chinese_and_thai_labels_draw() {
        let q = render_text("???", 16, true).unwrap().unwrap();
        for text in [
            "\u{d55c}\u{ae00}",
            "\u{65e5}\u{672c}\u{8a9e}",
            "\u{3072}\u{3089}\u{304c}\u{306a}",
            "\u{4e2d}\u{6587}",
            "\u{e44}\u{e17}\u{e22}",
            // Stacked Thai marks and a below-baseline vowel stay in the strip.
            "\u{e1f}\u{e37}\u{e49}\u{e19} \u{e1b}\u{e38}",
        ] {
            let strip = render_text(text, 16, true).unwrap().unwrap();
            assert_eq!(strip.height(), 23, "{text}");
            assert!(dark(&strip) > 40, "{text}: {} dark px", dark(&strip));
            assert_ne!(strip.data(), q.data(), "{text}");
            // The strip's top and bottom rows stay clear of ink.
            let row = strip.width() as usize * 4;
            let top = &strip.data()[..row];
            let bottom = &strip.data()[strip.data().len() - row..];
            assert!(
                top.chunks(4).all(|p| p[0] > 200),
                "{text}: ink clipped at top"
            );
            assert!(
                bottom.chunks(4).all(|p| p[0] > 200),
                "{text}: ink clipped at bottom"
            );
        }
    }

    #[test]
    fn scripts_mix_in_one_label() {
        let mixed = render_text(
            "A \u{d55c}\u{ae00} \u{65e5}\u{672c} caf\u{e9} \u{e44}\u{e17}\u{e22}",
            16,
            true,
        )
        .unwrap()
        .unwrap();
        let a = render_text("A", 16, true).unwrap().unwrap();
        assert!(mixed.width() > a.width() * 8);
        assert!(dark(&mixed) > 150);
    }

    /// A Han character the Japanese face lacks (Simplified-only) must
    /// not vanish: it draws as `?`.
    #[test]
    fn a_character_its_face_lacks_draws_as_a_question_mark() {
        let strip = render_text("\u{7b80}", 16, false).unwrap().unwrap();
        assert!(dark(&strip) > 10, "blank strip");
    }

    #[test]
    fn empty_pages_errors() {
        let err = compose(&[], 1, 1, 0).unwrap_err();
        assert!(err.contains("no frames"), "{err}");
    }
}
