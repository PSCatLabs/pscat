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

/// Text as a tight, opaque strip: black-ish `text` in `Helvetica`
/// (`Helvetica-Bold` if `bold`) at `pt` pixels on white, drawn by pscat's
/// own interpreter and fonts, one point per pixel. Characters outside
/// printable ASCII show as `?` (the strip is set in the standard Latin
/// encoding). `None` for an empty `text`.
pub fn render_text(text: &str, pt: u32, bold: bool) -> Result<Option<Pixmap>, String> {
    if text.is_empty() {
        return Ok(None);
    }
    let mut lit = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' | '(' | ')' => {
                lit.push('\\');
                lit.push(c);
            }
            ' '..='~' => lit.push(c),
            _ => lit.push('?'),
        }
    }
    let font = if bold { "Helvetica-Bold" } else { "Helvetica" };
    let setup = format!("/{font} findfont {pt} scalefont setfont");
    // Measure first, then draw on a page of exactly the text's size.
    let mut probe = crate::Interp::new();
    probe.capture_stdout();
    probe
        .run_source(format!("{setup} ({lit}) stringwidth pop =").as_bytes())
        .map_err(|e| format!("label: {}", probe.error_report(&e)))?;
    let width: f32 = String::from_utf8_lossy(probe.captured_stdout())
        .trim()
        .parse()
        .map_err(|_| "label: could not measure the text".to_string())?;
    // Ascent ~0.72 em, descent ~0.21 em: a 1.3 em strip with the
    // baseline 0.3 em up clips neither.
    let w = (width.ceil() as u32 + 2).clamp(1, MAX_SIDE_PX);
    let h = (pt as f32 * 1.3).ceil() as u32;
    let base = (pt as f32 * 0.3).ceil() as u32;
    let mut interp = crate::Interp::with_page(w, h)
        .ok_or_else(|| "label: unusable text strip size".to_string())?;
    interp
        .run_source(format!("{setup} 0.1 setgray 1 {base} moveto ({lit}) show").as_bytes())
        .map_err(|e| format!("label: {}", interp.error_report(&e)))?;
    Ok(Some(interp.gfx().pixmap.clone()))
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

    #[test]
    fn empty_pages_errors() {
        let err = compose(&[], 1, 1, 0).unwrap_err();
        assert!(err.contains("no frames"), "{err}");
    }
}
