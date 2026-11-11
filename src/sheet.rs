//! Review sheets: rendered pages laid out side by side, with labels.
//!
//! This is the layout and drawing behind `pscat sheet` and the plugin's
//! `review_sheet` tool, for a program that renders the cells itself (in a
//! sandboxed worker, say) and wants pscat's sheet around them. A sheet is a
//! list of rows; each row has an optional label above it and its cells left
//! to right; each cell is an image with an optional caption under it. Every
//! image is drawn at its own pixel size, with a grey hairline around it, and
//! centred in a column as wide as the widest cell in it, so a 48 px icon
//! sits at true size beside a 512 px render.
//!
//! The work splits into three steps, so a caller can refuse an oversized
//! sheet before rendering anything:
//!
//! 1. [`SheetLayout::new`] places the rows from their sizes alone (pure
//!    arithmetic: no pixel is allocated), and fails past [`MAX_SIDE_PX`] on
//!    a side.
//! 2. The caller renders each cell to a [`Page`] (an [`Interp`]'s
//!    [`into_last_page`](Interp::into_last_page) is the page `pscat sheet`
//!    takes from a cell's program), and sets the labels and captions with
//!    [`text_strip`]. [`zoom`] enlarges a small cell pixel by pixel.
//! 3. A [`Sheet`] is the white canvas for the layout: put each piece in its
//!    slot, then take the sheet as a [`Page`] and encode it.
//!
//! ```
//! use pscat::sheet::{CellSize, RowSize, Sheet, SheetLayout, text_strip};
//! use pscat::Interp;
//!
//! // One row, labelled, with one 60x40 pt program rendered at 72 dpi.
//! let label = text_strip("red square", 16, true)?.expect("non-empty text");
//! let mut interp = Interp::with_page(60, 40).expect("valid page size");
//! interp.run_str("1 0 0 setrgbcolor 10 10 20 20 rectfill").expect("runs");
//! let cell = interp.into_last_page();
//!
//! let row = RowSize::new(
//!     (label.width(), label.height()),
//!     vec![CellSize::new((cell.width(), cell.height()), (0, 0))],
//! );
//! let layout = SheetLayout::new(&[row])?;
//! let mut sheet = Sheet::new(layout)?;
//! sheet.put_header(0, &label)?;
//! sheet.put_image(0, 0, &cell)?;
//! let png = sheet.into_page().to_png().expect("PNG encoding");
//! assert_eq!(&png[1..4], b"PNG");
//! # Ok::<(), pscat::sheet::SheetError>(())
//! ```

use std::fmt;

#[cfg(doc)]
use crate::Interp;
use crate::Page;
use crate::contact_sheet as cs;

/// The most pixels on one side of a sheet, or of one zoomed cell.
pub const MAX_SIDE_PX: u32 = cs::MAX_SIDE_PX;

/// The longest text [`text_strip`] sets, in characters.
pub const MAX_TEXT_CHARS: usize = 1000;

/// The tallest type [`text_strip`] sets, in pixels.
pub const MAX_TEXT_PX: u32 = 256;

/// What went wrong, for a caller that maps errors to its own codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SheetErrorKind {
    /// The sheet has no cells at all.
    Empty,
    /// The sheet, or a zoomed cell, would be over [`MAX_SIDE_PX`] on a side,
    /// or too large to allocate.
    TooLarge,
    /// A row or cell index that the layout does not have.
    NoSuchSlot,
    /// An image whose size is not the one the layout planned for its slot,
    /// or a text strip larger than its slot.
    WrongSize,
    /// [`text_strip`] was given more than [`MAX_TEXT_CHARS`] characters, or
    /// a type size of 0 or over [`MAX_TEXT_PX`], or could not set the text.
    Text,
}

/// Why a sheet could not be laid out or drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetError {
    kind: SheetErrorKind,
    message: String,
}

impl SheetError {
    fn new(kind: SheetErrorKind, message: impl Into<String>) -> Self {
        SheetError {
            kind,
            message: message.into(),
        }
    }

    /// What kind of failure this is.
    pub fn kind(&self) -> SheetErrorKind {
        self.kind
    }
}

impl fmt::Display for SheetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for SheetError {}

/// One cell's footprint, for [`SheetLayout::new`]: the image as it will be
/// shown (after any [`zoom`]) and its caption strip, `(0, 0)` for none.
/// Sizes are in pixels, width first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellSize {
    image: (u32, u32),
    caption: (u32, u32),
}

impl CellSize {
    /// A cell of an `image`-sized image with a `caption`-sized caption under
    /// it (`(0, 0)` for no caption).
    pub fn new(image: (u32, u32), caption: (u32, u32)) -> Self {
        CellSize { image, caption }
    }

    /// The image's size.
    pub fn image(&self) -> (u32, u32) {
        self.image
    }

    /// The caption's size, `(0, 0)` for none.
    pub fn caption(&self) -> (u32, u32) {
        self.caption
    }
}

/// One row, for [`SheetLayout::new`]: the size of the label strip above it
/// (`(0, 0)` for none) and its cells, left to right.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowSize {
    header: (u32, u32),
    cells: Vec<CellSize>,
}

impl RowSize {
    /// A row with a `header`-sized label strip (`(0, 0)` for none) over
    /// `cells`.
    pub fn new(header: (u32, u32), cells: Vec<CellSize>) -> Self {
        RowSize { header, cells }
    }

    /// The label strip's size, `(0, 0)` for none.
    pub fn header(&self) -> (u32, u32) {
        self.header
    }

    /// The cells, left to right.
    pub fn cells(&self) -> &[CellSize] {
        &self.cells
    }
}

/// Where everything on a sheet goes. Positions are the top-left corner of
/// each piece, in pixels from the sheet's top-left; an image's position is
/// inside its hairline.
#[derive(Debug)]
pub struct SheetLayout {
    inner: cs::Layout,
    rows: Vec<RowSize>,
}

impl SheetLayout {
    /// Place `rows`. Sizes only: nothing is drawn or allocated, so a caller
    /// can size a sheet against its own budget before any cell is rendered.
    /// A text strip's size can be an upper bound here (the strip is drawn
    /// at the top-left of its slot); an image's must be exact.
    ///
    /// Fails with [`SheetErrorKind::Empty`] when no row has a cell, and
    /// [`SheetErrorKind::TooLarge`] when the sheet would be over
    /// [`MAX_SIDE_PX`] on a side.
    pub fn new(rows: &[RowSize]) -> Result<SheetLayout, SheetError> {
        if rows.iter().all(|r| r.cells.is_empty()) {
            return Err(SheetError::new(
                SheetErrorKind::Empty,
                "contact sheet: no cells to lay out",
            ));
        }
        let sizes: Vec<cs::RowSize> = rows
            .iter()
            .map(|r| cs::RowSize {
                header: r.header,
                cells: r
                    .cells
                    .iter()
                    .map(|c| cs::CellSize {
                        image: c.image,
                        caption: c.caption,
                    })
                    .collect(),
            })
            .collect();
        let inner = cs::Layout::new(&sizes).map_err(|m| {
            // The message ends with advice for `pscat sheet`'s flags, which
            // a library caller does not have: keep the size.
            let size = m.split(" -- ").next().unwrap_or_default();
            SheetError::new(SheetErrorKind::TooLarge, size)
        })?;
        Ok(SheetLayout {
            inner,
            rows: rows.to_vec(),
        })
    }

    /// The sheet's size in pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.inner.width, self.inner.height)
    }

    /// The rows the layout was made from.
    pub fn rows(&self) -> &[RowSize] {
        &self.rows
    }

    /// Top-left of row `row`'s label strip, `None` past the last row.
    pub fn header_xy(&self, row: usize) -> Option<(u32, u32)> {
        self.inner.header_xy.get(row).copied()
    }

    /// Top-left of the image of row `row`, cell `cell` (inside its
    /// hairline), `None` for a slot the layout does not have.
    pub fn image_xy(&self, row: usize, cell: usize) -> Option<(u32, u32)> {
        self.inner.image_xy.get(row)?.get(cell).copied()
    }

    /// Top-left of the caption of row `row`, cell `cell`, `None` for a slot
    /// the layout does not have.
    pub fn caption_xy(&self, row: usize, cell: usize) -> Option<(u32, u32)> {
        self.inner.caption_xy.get(row)?.get(cell).copied()
    }

    fn cell(&self, row: usize, cell: usize) -> Result<&CellSize, SheetError> {
        self.rows
            .get(row)
            .and_then(|r| r.cells.get(cell))
            .ok_or_else(|| {
                SheetError::new(
                    SheetErrorKind::NoSuchSlot,
                    format!("contact sheet: no cell {cell} in row {row}"),
                )
            })
    }
}

/// A sheet being drawn: white, the size of its [`SheetLayout`].
#[derive(Debug)]
pub struct Sheet {
    layout: SheetLayout,
    canvas: tiny_skia::Pixmap,
}

impl Sheet {
    /// A blank sheet for `layout`. This allocates the sheet's pixels:
    /// check [`SheetLayout::size`] against your budget first.
    pub fn new(layout: SheetLayout) -> Result<Sheet, SheetError> {
        let canvas = layout
            .inner
            .new_sheet()
            .map_err(|m| SheetError::new(SheetErrorKind::TooLarge, m))?;
        Ok(Sheet { layout, canvas })
    }

    /// The layout this sheet draws.
    pub fn layout(&self) -> &SheetLayout {
        &self.layout
    }

    /// Draw row `row`'s label strip. It must fit the size the layout
    /// planned for it.
    pub fn put_header(&mut self, row: usize, strip: &Page) -> Result<(), SheetError> {
        let planned = self.layout.rows.get(row).map(RowSize::header);
        let at = self.layout.header_xy(row);
        let (Some(planned), Some(at)) = (planned, at) else {
            return Err(SheetError::new(
                SheetErrorKind::NoSuchSlot,
                format!("contact sheet: no row {row}"),
            ));
        };
        fits("label", row, None, strip, planned)?;
        cs::blit_at(&mut self.canvas, at.0, at.1, strip.pixmap());
        Ok(())
    }

    /// Draw the image of row `row`, cell `cell`, with its hairline around
    /// it. It must be exactly the size the layout planned for it.
    pub fn put_image(&mut self, row: usize, cell: usize, image: &Page) -> Result<(), SheetError> {
        let planned = self.layout.cell(row, cell)?.image;
        let got = (image.width(), image.height());
        if got != planned {
            return Err(SheetError::new(
                SheetErrorKind::WrongSize,
                format!(
                    "contact sheet: the image for row {row}, cell {cell} is {}x{}px, not the \
                     {}x{}px planned",
                    got.0, got.1, planned.0, planned.1
                ),
            ));
        }
        let Some((x, y)) = self.layout.image_xy(row, cell) else {
            return Err(SheetError::new(
                SheetErrorKind::NoSuchSlot,
                format!("contact sheet: no cell {cell} in row {row}"),
            ));
        };
        cs::frame(&mut self.canvas, x, y, got.0, got.1);
        cs::blit_at(&mut self.canvas, x, y, image.pixmap());
        Ok(())
    }

    /// Draw the caption of row `row`, cell `cell`. It must fit the size the
    /// layout planned for it.
    pub fn put_caption(&mut self, row: usize, cell: usize, strip: &Page) -> Result<(), SheetError> {
        let planned = self.layout.cell(row, cell)?.caption;
        fits("caption", row, Some(cell), strip, planned)?;
        let Some((x, y)) = self.layout.caption_xy(row, cell) else {
            return Err(SheetError::new(
                SheetErrorKind::NoSuchSlot,
                format!("contact sheet: no cell {cell} in row {row}"),
            ));
        };
        cs::blit_at(&mut self.canvas, x, y, strip.pixmap());
        Ok(())
    }

    /// The finished sheet. It records 72 dpi, whatever the dpi of its cells,
    /// as `pscat sheet` writes it.
    pub fn into_page(self) -> Page {
        Page::new(self.canvas, 1.0)
    }
}

/// A text strip must fit inside the slot planned for it.
fn fits(
    what: &str,
    row: usize,
    cell: Option<usize>,
    strip: &Page,
    planned: (u32, u32),
) -> Result<(), SheetError> {
    if strip.width() <= planned.0 && strip.height() <= planned.1 {
        return Ok(());
    }
    let place = match cell {
        Some(c) => format!("row {row}, cell {c}"),
        None => format!("row {row}"),
    };
    Err(SheetError::new(
        SheetErrorKind::WrongSize,
        format!(
            "contact sheet: the {what} for {place} is {}x{}px, larger than the {}x{}px planned",
            strip.width(),
            strip.height(),
            planned.0,
            planned.1
        ),
    ))
}

/// `text` as a tight strip of dark grey type `px` pixels tall on white,
/// bold or not, set by pscat's own interpreter and fonts as `pscat sheet`
/// sets its labels. The face is picked per character by script: Latin
/// (accents included) in Helvetica; Korean, Japanese, Chinese and Thai in
/// the `cjk-thai` font pack's Noto Sans faces. A character no installed face
/// has is drawn as `?`; when the pack is missing, one warning per face goes
/// to stderr. `None` for an empty `text`. The strip records 72 dpi.
///
/// The text is passed to PostScript as an escaped string literal, never as
/// code. Setting it runs the interpreter without limits, so the length is
/// capped: at most [`MAX_TEXT_CHARS`] characters, at 1 to [`MAX_TEXT_PX`]
/// pixels; the strip is at most [`MAX_SIDE_PX`] wide.
pub fn text_strip(text: &str, px: u32, bold: bool) -> Result<Option<Page>, SheetError> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(SheetError::new(
            SheetErrorKind::Text,
            format!("contact sheet: a text strip is at most {MAX_TEXT_CHARS} characters"),
        ));
    }
    if px == 0 || px > MAX_TEXT_PX {
        return Err(SheetError::new(
            SheetErrorKind::Text,
            format!("contact sheet: text is 1 to {MAX_TEXT_PX} pixels tall, not {px}"),
        ));
    }
    cs::render_text(text, px, bold)
        .map(|strip| strip.map(|p| Page::new(p, 1.0)))
        .map_err(|m| SheetError::new(SheetErrorKind::Text, m))
}

/// `page` enlarged `k` times with nearest-neighbour sampling: every pixel
/// becomes a `k`x`k` block, so a small icon can be read pixel by pixel.
/// The result records `k` times the page's resolution, so it prints at the
/// same physical size. `None` for `k` of 0, or past [`MAX_SIDE_PX`] on a
/// side.
///
/// ```
/// let page = pscat::Page::blank(4, 3).expect("valid size");
/// let big = pscat::sheet::zoom(&page, 5).expect("fits");
/// assert_eq!((big.width(), big.height()), (20, 15));
/// assert!(pscat::sheet::zoom(&page, 0).is_none());
/// ```
pub fn zoom(page: &Page, k: u32) -> Option<Page> {
    if k == 0 {
        return None;
    }
    let zoomed = cs::zoom_nearest(page.pixmap(), k)?;
    Some(Page::new(zoomed, page.scale() * k as f32))
}
