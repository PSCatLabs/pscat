//! Rasterizers and decoders shared by the QR integration tests (`tests/qr.rs`,
//! `tests/qr_corpus.rs`). Every one is an independent implementation: resvg
//! and hayro share no code with pscat's rasterizer, and the two decoders
//! share none with `lib/qr.ps`, the fixture oracle or each other (rqrr is a
//! port of quirc, rxing of ZXing). All are compiled-in dev-dependencies, so
//! nothing that uses them can skip for want of a tool.
#![allow(dead_code)] // each test crate that mounts this module uses a subset

/// A greyscale raster composited onto white: transparent reads as light.
pub struct Grey {
    pub width: usize,
    pub height: usize,
    pub px: Vec<u8>,
}

impl Grey {
    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.px[y * self.width + x]
    }
}

pub fn grey_of(width: usize, rgba: impl Iterator<Item = [u8; 4]>) -> Grey {
    let px: Vec<u8> = rgba
        .map(|[r, g, b, a]| {
            let a = u32::from(a);
            let luma = (u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114) / 1000;
            ((luma * a + 255 * (255 - a)) / 255) as u8
        })
        .collect();
    Grey {
        width,
        height: px.len() / width,
        px,
    }
}

pub fn grey_tiny(pixmap: &tiny_skia::Pixmap) -> Grey {
    grey_of(
        pixmap.width() as usize,
        pixmap.pixels().iter().map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        }),
    )
}

/// pscat's own PNG, decoded by tiny-skia's PNG reader.
pub fn raster_png(bytes: &[u8]) -> Grey {
    grey_tiny(&tiny_skia::Pixmap::decode_png(bytes).expect("pscat PNG"))
}

/// resvg, an independent SVG renderer, at `scale` device pixels per SVG unit.
pub fn raster_svg(bytes: &[u8], scale: f32) -> Grey {
    let tree =
        resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).expect("parse SVG");
    let size = tree.size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(
        (size.width() * scale).round() as u32,
        (size.height() * scale).round() as u32,
    )
    .expect("SVG pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    grey_tiny(&pixmap)
}

/// hayro, an independent pure-Rust PDF rasterizer, at `scale` device pixels
/// per point. It paints the page white first; its pixels are premultiplied,
/// so un-premultiply before compositing.
pub fn raster_pdf(bytes: &[u8], scale: f32) -> Grey {
    let pdf = hayro::Pdf::new(std::sync::Arc::new(bytes.to_vec())).expect("parse PDF");
    let page = &pdf.pages()[0];
    let pixmap = hayro::render(
        page,
        &hayro::InterpreterSettings::default(),
        &hayro::RenderSettings {
            x_scale: scale,
            y_scale: scale,
            ..Default::default()
        },
    );
    let width = pixmap.width() as usize;
    let bytes = pixmap.take_u8();
    grey_of(
        width,
        bytes.as_chunks::<4>().0.iter().map(|p| {
            let un = |c: u8| match p[3] {
                0 => 0,
                a => ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8,
            };
            [un(p[0]), un(p[1]), un(p[2]), p[3]]
        }),
    )
}

/// The format-information code of an ECC level (ISO/IEC 18004 Table 12):
/// what both decoders report.
pub fn level_code(level: &str) -> u16 {
    match level {
        "L" => 1,
        "M" => 0,
        "Q" => 3,
        "H" => 2,
        other => panic!("level {other}"),
    }
}

/// What a decoder read back. Each decoder reports a different subset, so a
/// field it does not report is `None` and is not asserted.
#[derive(Debug)]
pub struct Read {
    /// The concatenated byte-segment payload, raw bytes (no text decoding).
    pub payload: Vec<u8>,
    pub version: Option<usize>,
    pub mask: Option<u64>,
    pub level: u16,
    /// The corrected data codewords (rxing's raw bytes): the padded data
    /// stream before Reed-Solomon, as the encoder built it.
    pub data_codewords: Option<Vec<u8>>,
}

/// rqrr (a quirc port). The payload is read through its byte writer, not
/// its UTF-8 `String` API, so binary payloads compare exactly.
pub fn read_rqrr(g: &Grey) -> Result<Read, String> {
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(g.width, g.height, |x, y| g.at(x, y));
    let grids = image.detect_grids();
    if grids.len() != 1 {
        return Err(format!("found {} symbols, expected 1", grids.len()));
    }
    let mut payload = Vec::new();
    let meta = grids[0]
        .decode_to(&mut payload)
        .map_err(|e| format!("decode: {e:?}"))?;
    Ok(Read {
        payload,
        version: Some(meta.version.0),
        mask: Some(u64::from(meta.mask)),
        level: meta.ecc_level,
        data_codewords: None,
    })
}

/// rxing (a ZXing port), its QR reader alone, with no pure-barcode hint, so
/// its finder-pattern detector does the locating. The payload comes from the
/// byte-segment metadata, not the charset-guessed text.
pub fn read_rxing(g: &Grey) -> Result<Read, String> {
    use rxing::{RXingResultMetadataType as Key, RXingResultMetadataValue as Value, Reader as _};
    let source = rxing::Luma8LuminanceSource::new(g.px.clone(), g.width as u32, g.height as u32)
        .map_err(|e| format!("luminance source: {e:?}"))?;
    let mut bitmap = rxing::BinaryBitmap::new(rxing::common::HybridBinarizer::new(source));
    let hints = rxing::DecodeHints {
        TryHarder: Some(true),
        ..Default::default()
    };
    let result = rxing::qrcode::QRCodeReader::new()
        .decode_with_hints(&mut bitmap, &hints)
        .map_err(|e| format!("decode: {e:?}"))?;
    let meta = result.getRXingResultMetadata();
    let payload = match meta.get(&Key::BYTE_SEGMENTS) {
        Some(Value::ByteSegments(segments)) => segments.concat(),
        other => return Err(format!("no byte segments: {other:?}")),
    };
    let level = match meta.get(&Key::ERROR_CORRECTION_LEVEL) {
        Some(Value::ErrorCorrectionLevel(code)) => {
            code.parse().map_err(|_| format!("ECC level {code:?}"))?
        }
        other => return Err(format!("no ECC level: {other:?}")),
    };
    Ok(Read {
        payload,
        version: None,
        mask: None,
        level,
        data_codewords: Some(result.getRawBytes().to_vec()),
    })
}

/// The two independent decoders, by name, for per-decoder diagnostics.
pub const DECODERS: [(&str, fn(&Grey) -> Result<Read, String>); 2] =
    [("rqrr", read_rqrr), ("rxing", read_rxing)];
