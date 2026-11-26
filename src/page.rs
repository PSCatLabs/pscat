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

    /// The pixels as premultiplied RGBA, borrowed: the page's own storage,
    /// row by row from the top-left, `width * height * 4` bytes, each colour
    /// channel already multiplied by the pixel's alpha. No copy is made,
    /// which is what a host that reads many pages (video frames) wants; for
    /// an opaque page the bytes equal [`rgba8`](Self::rgba8)'s.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(4, 4).expect("valid page size");
    /// interp.run_str("0 0 1 setrgbcolor 0 0 4 4 rectfill")?;
    /// let page = interp.into_last_page();
    /// assert_eq!(&page.premultiplied_rgba8()[..4], &[0, 0, 255, 255]);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn premultiplied_rgba8(&self) -> &[u8] {
        self.pixmap.data()
    }

    /// The premultiplied pixels ([`premultiplied_rgba8`](Self::premultiplied_rgba8)),
    /// writable, for a host that composites or filters a page in place.
    /// Every pixel must stay premultiplied (no colour channel above its
    /// alpha); the page's encoders assume it.
    pub fn premultiplied_rgba8_mut(&mut self) -> &mut [u8] {
        self.pixmap.data_mut()
    }

    /// A page from premultiplied RGBA pixels, `width * height * 4` bytes
    /// laid out as [`premultiplied_rgba8`](Self::premultiplied_rgba8)
    /// returns them, at 72 dpi. `None` if the size is 0 or over 8000 pixels
    /// on a side, or `data` is not exactly that long.
    ///
    /// ```
    /// let page = pscat::Page::from_premultiplied_rgba8(1, 2, vec![255, 0, 0, 255, 0, 0, 0, 0])
    ///     .expect("valid size");
    /// assert_eq!(page.rgba8(), [255, 0, 0, 255, 0, 0, 0, 0]);
    /// assert!(pscat::Page::from_premultiplied_rgba8(1, 2, vec![0; 4]).is_none());
    /// ```
    pub fn from_premultiplied_rgba8(width: u32, height: u32, data: Vec<u8>) -> Option<Page> {
        if width > MAX_SIDE_PX || height > MAX_SIDE_PX {
            return None;
        }
        let size = tiny_skia::IntSize::from_wh(width, height)?;
        tiny_skia::Pixmap::from_vec(data, size).map(|pixmap| Page::new(pixmap, 1.0))
    }

    /// A fully transparent page of `width`x`height` pixels at 72 dpi: an
    /// empty layer for [`Interp::swap_canvas`](crate::Interp::swap_canvas).
    /// `None` for a side of 0 or over 8000 pixels.
    ///
    /// ```
    /// let page = pscat::Page::transparent(3, 2).expect("valid size");
    /// assert!(page.premultiplied_rgba8().iter().all(|b| *b == 0));
    /// ```
    pub fn transparent(width: u32, height: u32) -> Option<Page> {
        if width > MAX_SIDE_PX || height > MAX_SIDE_PX {
            return None;
        }
        tiny_skia::Pixmap::new(width, height).map(|pixmap| Page::new(pixmap, 1.0))
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

    /// The size of the canvas, the raster the program is painting on, in
    /// device pixels: the page size times the scale.
    pub fn canvas_size(&self) -> (u32, u32) {
        let pixmap = self.gfx().page_pixmap();
        (pixmap.width(), pixmap.height())
    }

    /// The canvas's pixels as it stands, borrowed, in the layout of
    /// [`Page::premultiplied_rgba8`]. This is the page in progress, whether
    /// or not the program has run `showpage`; a host that renders frame
    /// after frame on one interpreter reads each frame here without copying
    /// a [`Page`] out.
    ///
    /// ```
    /// let mut interp = pscat::Interp::with_page(2, 2).expect("valid page size");
    /// interp.run_str("0 1 0 setrgbcolor 0 0 2 2 rectfill")?;
    /// assert_eq!(interp.canvas_size(), (2, 2));
    /// assert_eq!(&interp.canvas_premultiplied_rgba8()[..4], &[0, 255, 0, 255]);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn canvas_premultiplied_rgba8(&self) -> &[u8] {
        self.gfx().page_pixmap().data()
    }

    /// Exchange the canvas for `page`: the program paints on `page` from now
    /// on, and the canvas as it stood comes back. The graphics state (the
    /// transformation, the clip, the colour) is untouched, so what is drawn
    /// next lands where it would have. A host uses this to paint part of a
    /// picture on a separate layer (swap in a [`Page::transparent`], run
    /// the drawing, swap back, then composite the layer itself), to keep a
    /// picture across runs, or to start each frame from a prepared
    /// background.
    ///
    /// `page` must be the canvas's size in pixels
    /// ([`canvas_size`](Self::canvas_size)); one that is not is handed back
    /// as the error. The returned page carries the interpreter's resolution.
    /// Not for use while SVG or PDF output is being recorded: those record
    /// drawing operations, not this raster. Inside a pattern's `PaintProc`
    /// there is no page to exchange and `page` is handed back. While a
    /// transparency group is open (`begingroup`) the canvas is the page
    /// below the group's layer: the exchange swaps that page, and the
    /// group still composites onto it at `endgroup`.
    ///
    /// ```
    /// use pscat::{Interp, Page};
    /// let mut interp = Interp::with_page(2, 2).expect("valid page size");
    /// interp.run_str("1 0 0 setrgbcolor 0 0 2 2 rectfill")?;
    /// let layer = Page::transparent(2, 2).expect("valid size");
    /// let background = interp.swap_canvas(layer).expect("same size");
    /// interp.run_str("0 0 1 setrgbcolor 0 0 1 2 rectfill")?; // left half, on the layer
    /// let layer = interp.swap_canvas(background).expect("same size");
    /// assert_eq!(layer.rgba8()[..8], [0, 0, 255, 255, 0, 0, 0, 0]);
    /// assert_eq!(&interp.canvas_premultiplied_rgba8()[..4], &[255, 0, 0, 255]);
    /// assert!(interp.swap_canvas(Page::transparent(3, 3).expect("valid size")).is_err());
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    pub fn swap_canvas(&mut self, page: Page) -> Result<Page, Page> {
        if (page.width(), page.height()) != self.canvas_size() {
            return Err(page);
        }
        let (scale, given) = (self.gfx().scale(), page.scale);
        match self.gfx_mut().swap_pixmap(page.pixmap) {
            Ok(old) => Ok(Page::new(old, scale)),
            Err(back) => Err(Page::new(back, given)),
        }
    }

    /// Paint `page`, a raster the host holds, through the current
    /// transformation, clip, `setalpha` and `setblendmode`, as `image` paints
    /// samples a program supplied. The raster's pixel grid fills the user-space
    /// unit square with its first row along the top (`y = 1`), exactly the
    /// mapping of `w h 8 [w 0 0 -h 0 h] image`, so the host places it with the
    /// operators it already has and scales it up to size:
    ///
    /// ```
    /// use pscat::{Interp, Page, host::Sampling};
    /// let mut interp = Interp::with_page(20, 10).expect("valid page size");
    /// // A layer: red on the left half, opaque; the rest transparent.
    /// let mut layer = Page::transparent(2, 1).expect("valid size");
    /// layer.premultiplied_rgba8_mut()[..4].copy_from_slice(&[255, 0, 0, 255]);
    /// // Paint it 20 x 10 units from the origin, at half strength.
    /// interp.run_str("gsave 0.5 setalpha 20 10 scale")?;
    /// interp.draw_page(&layer, Sampling::Nearest)?;
    /// interp.run_str("grestore")?;
    /// let canvas = interp.canvas_premultiplied_rgba8();
    /// let left = &canvas[(5 * 20 + 3) * 4..][..4]; // half-strength red on white
    /// assert!(left[0] == 255 && (126..=129).contains(&left[1]) && left[3] == 255);
    /// assert_eq!(&canvas[(5 * 20 + 15) * 4..][..4], &[255, 255, 255, 255]);
    /// # Ok::<(), pscat::PsError>(())
    /// ```
    ///
    /// The raster is composited where the program paints: on the layer while a
    /// transparency group is open (the group composites it at `endgroup`, and a
    /// host reading the canvas sees the page below until then), inside a
    /// pattern's `PaintProc` on the tile. The clip, `setalpha` (as opacity)
    /// and `setblendmode` apply as for a fill, and the edge of the raster is
    /// anti-aliased. A `page` of straight-alpha art must be made premultiplied
    /// ([`Page::from_premultiplied_rgba8`]); a transparent pixel paints nothing.
    ///
    /// It paints and records nothing, and is not an error, when the program
    /// paints nowhere (`nulldevice`), the transformation is singular or not
    /// finite, or the raster lies wholly off the canvas. A raster that is not
    /// on the canvas at all is not embedded in SVG or PDF output either. In an
    /// uncoloured pattern's `PaintProc` it paints as `image` does: the tile
    /// keeps the raster's coverage and the pattern's colour replaces its own;
    /// SVG records that as a mask of the raster's alpha and PDF as an image
    /// mask (a bit a pixel, set where the alpha is at least half), which is
    /// all a PDF tile without colour may hold.
    ///
    /// **SVG and PDF output** embed the raster as an image at its placement,
    /// with the clip and the alpha and blend mode, as `image` is recorded
    /// (PNG in SVG, an RGB image with a soft mask in PDF; a raster with no
    /// transparent pixel carries no mask). Recording copies the pixels, so a
    /// raster over [`MAX_EMBEDDED_RASTER_PIXELS`](crate::host::MAX_EMBEDDED_RASTER_PIXELS) is `limitcheck` while either
    /// recorder is on, with nothing painted or recorded.
    ///
    /// **Limits.** Called from a host operator, the work is charged to the
    /// running program: a step per 1024 device pixels the footprint reaches
    /// (and per pixel recorded), and the time and memory caps and the
    /// interrupt flag are looked at, all before anything is painted, so a
    /// refused call leaves the canvas as it was. They are looked at again
    /// between the heavy steps (the recorders' pixel copies, strips of the
    /// fill), and the recorders are written only once their copies exist, so a
    /// limit that fires part way leaves the canvas and the output as they were.
    /// Between runs only the memory cap (for the recorders' copies) and the
    /// interrupt flag apply. Under `--bands` only the band's rows are filled.
    pub fn draw_page(
        &mut self,
        page: &Page,
        sampling: crate::host::Sampling,
    ) -> Result<(), crate::PsError> {
        use crate::host::{MAX_EMBEDDED_RASTER_PIXELS, Sampling};
        let (w, h) = (page.width(), page.height());
        let Some(plan) = self.gfx().plan_raster(w, h) else {
            return Ok(());
        };
        let (svg, pdf) = (self.gfx().svg_wanted(), self.gfx().pdf_wanted());
        let px = u64::from(w) * u64::from(h);
        let recorders = u64::from(svg) + u64::from(pdf);
        if recorders > 0 && px > MAX_EMBEDDED_RASTER_PIXELS as u64 {
            return Err(crate::PsError::Limitcheck);
        }
        // What the recorders hold at once: SVG, a premultiplied copy's
        // demultiplied encode and the PNG (12 bytes a pixel); PDF, RGB, alpha
        // and the demultiplied pixel (8).
        let bytes = px * (if svg { 12 } else { 0 } + if pdf { 8 } else { 0 });
        let work = plan.area.saturating_add(px.saturating_mul(recorders));
        self.host_poll(work / 1024 + 1)?;
        self.check_alloc(usize::try_from(bytes).unwrap_or(usize::MAX))?;
        let smooth = matches!(sampling, Sampling::Bilinear);
        // The work is charged; what remains is looked at between the heavy
        // steps (the recorders' copies, the strips of the fill) for the
        // interrupt flag and, while a program runs, the time and memory caps.
        // Between runs the limits belong to the run that ended: only the flag.
        let flag = self.interrupt.clone();
        let probe = if self.running() {
            self.limit_probe()
        } else {
            crate::limits::LimitProbe::NONE
        };
        let mut look = || {
            if flag
                .as_ref()
                .is_some_and(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            {
                return Err(crate::PsError::Interrupted);
            }
            probe.check(0)
        };
        let watching = flag.is_some() || probe != crate::limits::LimitProbe::NONE;
        let poll: Option<&mut dyn FnMut() -> Result<(), crate::PsError>> =
            if watching { Some(&mut look) } else { None };
        self.gfx_mut()
            .place_raster(page.pixmap(), &plan, smooth, poll)
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
            None if index == shown.len() && index < self.page_count() => gfx.page_pixmap(),
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
        // A group a slice left open has its layer as the raster: the page is
        // the canvas below it, copied (the layer is dropped with `self`).
        if canvas_is_last && self.gfx().page_is_displaced() {
            return Page::new(self.gfx().page_pixmap().clone(), scale);
        }
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
