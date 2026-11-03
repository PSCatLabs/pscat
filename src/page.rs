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
}
