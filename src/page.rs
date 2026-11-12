//! Rendered pages: what [`Interp::pages`](crate::Interp::pages) hands back.

use std::path::Path;

/// One rendered page: a raster of device pixels, RGBA, 8 bits per channel.
///
/// The size is the interpreter's page size times its scale
/// ([`Interp::with_page_scaled`](crate::Interp::with_page_scaled)), so a
/// 612x792-point page at 144 dpi is 1224x1584 pixels. The background is
/// opaque white unless the program painted over it.
///
/// ```
/// let mut interp = pscat::Interp::with_page(100, 50).expect("valid page size");
/// interp.run_str("1 0 0 setrgbcolor 0 0 100 50 rectfill showpage")?;
/// let page = &interp.pages()[0];
/// assert_eq!((page.width(), page.height()), (100, 50));
/// assert_eq!(&page.rgba8()[..4], &[255, 0, 0, 255]);
/// # Ok::<(), pscat::PsError>(())
/// ```
#[derive(Clone)]
pub struct Page {
    pixmap: tiny_skia::Pixmap,
    /// Device pixels per point (dpi / 72), recorded in the PNG's `pHYs`.
    scale: f32,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}

impl Page {
    pub(crate) fn new(pixmap: tiny_skia::Pixmap, scale: f32) -> Self {
        Page { pixmap, scale }
    }

    /// Width in device pixels.
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    /// Height in device pixels.
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// The pixels as straight (not premultiplied) RGBA, row by row from the
    /// top-left, `width * height * 4` bytes.
    pub fn rgba8(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixmap.data().len());
        for p in self.pixmap.pixels() {
            let c = p.demultiply();
            out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        out
    }

    /// The page encoded as a PNG file, byte for byte what `pscat --png`
    /// writes for the same page (without `--halftone`). The file records
    /// the page's resolution in a `pHYs` chunk, so a 300 dpi page prints at
    /// its true size.
    pub fn to_png(&self) -> std::io::Result<Vec<u8>> {
        crate::png::encode(&self.pixmap, self.scale)
    }

    /// Write the page to `path` as a PNG (see [`to_png`](Self::to_png)).
    pub fn save_png(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        std::fs::write(path, self.to_png()?)
    }

    /// A white page of `width`x`height` pixels at 72 dpi, for a slot whose
    /// program left nothing to show. `None` for a side of 0 or over 8000
    /// pixels, the same ceiling an interpreter's page has.
    ///
    /// ```
    /// let page = pscat::Page::blank(40, 20).expect("valid size");
    /// assert_eq!((page.width(), page.height()), (40, 20));
    /// assert_eq!(&page.rgba8()[..4], &[255, 255, 255, 255]);
    /// assert!(pscat::Page::blank(0, 20).is_none());
    /// ```
    pub fn blank(width: u32, height: u32) -> Option<Page> {
        if width > MAX_SIDE_PX || height > MAX_SIDE_PX {
            return None;
        }
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        pixmap.fill(tiny_skia::Color::WHITE);
        Some(Page::new(pixmap, 1.0))
    }

    /// The page screened to pure black and white like a mono laser
    /// printer's: a 45 degree clustered-dot screen whose dots cover as much
    /// of each cell as the page is dark there. This is what `pscat
    /// --halftone --png` writes, byte for byte: `page.halftone().to_png()`.
    /// The result keeps the page's size and resolution.
    ///
    /// The screen is laid out in device pixels, so the dots are the same
    /// size at any dpi. A page with a transparent background (a chalkboard
    /// without a board) is screened as if it were printed on white paper.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(64, 64).expect("valid page size");
    /// interp.run_str("0.5 setgray 0 0 64 64 rectfill")?;
    /// let screened = interp.pages()[0].halftone();
    /// // Only black and white pixels remain.
    /// assert!(screened.rgba8().chunks(4).all(|p| p[0] == 0 || p[0] == 255));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn halftone(&self) -> Page {
        Page::new(crate::halftone::screen(&self.on_white()), self.scale)
    }

    /// The pixels as if printed on white paper: the page itself when it is
    /// opaque (no copy), else a copy composited onto white. The halftone
    /// screen and the sheet's byte copy both read pixels as opaque colour,
    /// and a premultiplied transparent pixel is (0, 0, 0, 0): black.
    pub(crate) fn on_white(&self) -> std::borrow::Cow<'_, tiny_skia::Pixmap> {
        if self.pixmap.pixels().iter().all(|p| p.alpha() == 255) {
            return std::borrow::Cow::Borrowed(&self.pixmap);
        }
        // Premultiplied source-over white is `c + (255 - a)` per channel,
        // which stays within 255.
        let mut paper = self.pixmap.clone();
        for px in paper.data_mut().as_chunks_mut::<4>().0 {
            let under = 255 - px[3];
            for c in &mut px[..3] {
                *c = c.saturating_add(under);
            }
            px[3] = 255;
        }
        std::borrow::Cow::Owned(paper)
    }

    pub(crate) fn pixmap(&self) -> &tiny_skia::Pixmap {
        &self.pixmap
    }

    pub(crate) fn scale(&self) -> f32 {
        self.scale
    }
}

/// The most pixels on a side of a page (`Interp::with_page_scaled`'s and
/// `--page`'s ceiling).
const MAX_SIDE_PX: u32 = 8000;

// Page access and output setup on `Interp` beyond `pages()` and
// `enable_svg`/`enable_pdf` (issue #1098). These live here, beside `Page`,
// rather than in `interp.rs`: they are about the rendered output, and need
// nothing of the machine but its graphics device.
impl crate::Interp {
    /// Make the page background transparent instead of opaque white: PNG
    /// pages get a real alpha channel, and SVG pages leave out their white
    /// backdrop. For artwork that is placed on something else (a garment, a
    /// slide). Call it before the program paints anything: it clears the
    /// page. `pscat --transparent` does this.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// interp.set_transparent_background();
    /// interp.run_str("0 0 5 5 rectfill")?;
    /// let page = interp.into_last_page();
    /// assert_eq!(page.rgba8()[3], 0); // the top-left corner is clear
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn set_transparent_background(&mut self) {
        self.gfx_mut().set_transparent_background();
    }

    /// Give the PDF being recorded the title and author in `source`'s DSC
    /// header comments (`%%Title:`, and `%%For:` or else `%%Author:`, read
    /// up to `%%EndComments`), as `pscat --pdf` does for the program it
    /// runs. Call it after [`enable_pdf`](crate::Interp::enable_pdf):
    /// without a PDF being recorded it does nothing.
    ///
    /// ```
    /// let src = "%!PS\n%%Title: Quarterly report\n%%EndComments\nshowpage\n";
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// interp.enable_pdf();
    /// interp.set_pdf_info_from_dsc(src.as_bytes());
    /// interp.run_str(src)?;
    /// let pdf = interp.pdf_document().expect("PDF enabled");
    /// assert!(String::from_utf8_lossy(&pdf).contains("/Title (Quarterly report)"));
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn set_pdf_info_from_dsc(&mut self, source: &[u8]) {
        let (title, author) = crate::pdf::scan_document_info(source);
        self.gfx_mut().set_pdf_info(title, author);
    }

    /// How many pages [`pages`](crate::Interp::pages) would return, without
    /// copying any of them.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// interp.run_str("showpage showpage 0 0 5 5 rectfill")?;
    /// assert_eq!(interp.page_count(), 3);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn page_count(&self) -> usize {
        let gfx = self.gfx();
        let shown = gfx.pages().len();
        if shown == 0 || gfx.has_trailing_art() {
            shown + 1
        } else {
            shown
        }
    }

    /// Page `index` of [`pages`](crate::Interp::pages), copying only that
    /// one, or `None` past [`page_count`](crate::Interp::page_count). To
    /// encode or screen a long document page by page with one extra raster
    /// in memory at a time:
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// interp.run_str("showpage 0 0 5 5 rectfill showpage")?;
    /// for i in 0..interp.page_count() {
    ///     let page = interp.page(i).expect("in range");
    ///     let png = page.halftone().to_png().expect("PNG encoding");
    ///     assert_eq!(&png[1..4], b"PNG");
    /// }
    /// assert!(interp.page(2).is_none());
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn page(&self, index: usize) -> Option<Page> {
        let gfx = self.gfx();
        let shown = gfx.pages();
        let raster = match shown.get(index) {
            Some(p) => p,
            None if index == shown.len() && index < self.page_count() => &gfx.pixmap,
            None => return None,
        };
        Some(Page::new(raster.clone(), gfx.scale()))
    }

    /// The last of [`pages`](crate::Interp::pages), taken out of the
    /// interpreter, which is dropped. The canvas is moved out rather than
    /// copied when it is the last page; when a shown page is, the canvas is
    /// freed before that page is copied. So at no point are there more page
    /// rasters in memory than the interpreter already held. A program that
    /// shows several pages gives its final one; one that failed gives the
    /// page as far as it got.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(10, 10).expect("valid page size");
    /// let _ = interp.run_str("1 0 0 setrgbcolor 0 0 10 10 rectfill nosuchop");
    /// let page = interp.into_last_page();
    /// assert_eq!(&page.rgba8()[..4], &[255, 0, 0, 255]);
    /// ```
    pub fn into_last_page(mut self) -> Page {
        let canvas_is_last = self.gfx().pages().is_empty() || self.gfx().has_trailing_art();
        let scale = self.gfx().scale();
        // A 1x1 raster is always allocatable; it stands in for the canvas
        // until the interpreter is dropped.
        let stand_in = tiny_skia::Pixmap::new(1, 1).expect("a 1x1 pixmap");
        let canvas = std::mem::replace(&mut self.gfx_mut().pixmap, stand_in);
        if !canvas_is_last {
            drop(canvas);
            // `canvas_is_last` is false only when a page was shown, so
            // this always finds one.
            if let Some(last) = self.gfx().pages().last() {
                return Page::new(last.clone(), scale);
            }
            return Page::new(self.gfx().pixmap.clone(), scale);
        }
        Page::new(canvas, scale)
    }
}
