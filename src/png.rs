//! PNG output with a resolution record.
//!
//! tiny-skia's encoder writes only `IHDR`, `IDAT` and `IEND`, so a render
//! made with `--dpi 300` did not say so and print dialogs assumed 72 or 96
//! dpi (issue #982). Every PNG pscat writes goes through here: the bytes are
//! tiny-skia's with one `pHYs` chunk (unit 1, the metre) spliced in before
//! the first `IDAT`. `IDAT` is not touched, so the decoded pixels are exactly
//! what the encoder alone would have given.

// The one place tiny-skia's own encoder is called (clippy.toml bans it elsewhere).
#![allow(clippy::disallowed_methods)]

use std::path::Path;

/// Metres per inch, exactly.
const METRES_PER_INCH: f64 = 0.0254;

/// Pixels per metre for `dpi`: `round(dpi / 0.0254)`.
pub fn pixels_per_metre(dpi: f64) -> u32 {
    (dpi / METRES_PER_INCH).round() as u32
}

/// The `pHYs` chunk (length, type, data, CRC) for `scale` device pixels per
/// point (`dpi / 72`), the same figure on both axes, unit 1 (metre).
fn phys_chunk(scale: f32) -> [u8; 21] {
    let ppm = pixels_per_metre(f64::from(scale) * 72.0).to_be_bytes();
    let mut chunk = [0u8; 21];
    chunk[..4].copy_from_slice(&9u32.to_be_bytes());
    chunk[4..8].copy_from_slice(b"pHYs");
    chunk[8..12].copy_from_slice(&ppm);
    chunk[12..16].copy_from_slice(&ppm);
    chunk[16] = 1;
    let mut crc = flate2::Crc::new();
    crc.update(&chunk[4..17]);
    chunk[17..].copy_from_slice(&crc.sum().to_be_bytes());
    chunk
}

/// `pixmap` as a PNG file recording a resolution of `scale` device pixels
/// per point (`dpi / 72`, so 1.0 is 72 dpi).
pub fn encode(pixmap: &tiny_skia::Pixmap, scale: f32) -> std::io::Result<Vec<u8>> {
    let mut png = pixmap.encode_png().map_err(std::io::Error::other)?;
    // The 8-byte signature, then chunks: length, type, data, CRC.
    let mut at = 8;
    loop {
        let bad = || std::io::Error::other("PNG encoder output has no IDAT chunk");
        let header = png.get(at..at + 8).ok_or_else(bad)?;
        if &header[4..] == b"IDAT" {
            break;
        }
        let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as usize;
        at += 12 + len;
    }
    png.splice(at..at, phys_chunk(scale));
    Ok(png)
}

/// Write `pixmap` to `path` as [`encode`] gives it.
pub fn save(pixmap: &tiny_skia::Pixmap, scale: f32, path: impl AsRef<Path>) -> std::io::Result<()> {
    std::fs::write(path, encode(pixmap, scale)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_per_metre_rounds_dpi_over_0_0254() {
        assert_eq!(pixels_per_metre(72.0), 2835);
        assert_eq!(pixels_per_metre(96.0), 3780);
        assert_eq!(pixels_per_metre(144.0), 5669);
        assert_eq!(pixels_per_metre(300.0), 11811);
    }

    #[test]
    fn the_scale_a_render_carries_gives_the_same_figure_as_its_dpi() {
        // Interp::with_page_scaled takes dpi / 72 as an f32; the chunk is
        // derived from that scale, and must agree with the plain formula
        // for every dpi the CLI accepts.
        for dpi in 9u32..=1200 {
            let scale = dpi as f32 / 72.0;
            let want = pixels_per_metre(f64::from(dpi));
            let got = u32::from_be_bytes(phys_chunk(scale)[8..12].try_into().unwrap());
            assert_eq!(got, want, "{dpi} dpi");
        }
    }

    #[test]
    fn the_chunk_sits_before_idat_and_the_pixels_are_unchanged() {
        let mut pm = tiny_skia::Pixmap::new(5, 3).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(10, 200, 30, 255));
        let plain = pm.encode_png().unwrap();
        let png = encode(&pm, 300.0 / 72.0).unwrap();
        assert_eq!(png.len(), plain.len() + 21);
        // IHDR is 25 bytes after the signature; pHYs follows it directly.
        assert_eq!(&png[..33], &plain[..33]);
        assert_eq!(&png[33..37], &9u32.to_be_bytes());
        assert_eq!(&png[37..41], b"pHYs");
        assert_eq!(&png[41..45], &11811u32.to_be_bytes());
        assert_eq!(&png[45..49], &11811u32.to_be_bytes());
        assert_eq!(png[49], 1);
        assert_eq!(&png[54..], &plain[33..]);
        let back = tiny_skia::Pixmap::decode_png(&png).expect("decodes, CRC included");
        assert_eq!(back.data(), pm.data());
    }
}
