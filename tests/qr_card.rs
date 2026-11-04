//! The gallery business card, `gallery/pscat_card.ps`: two 252 x 144 pt
//! sides from one PostScript source, with a QR code for the repository on the
//! back. The checks run on what a user actually gets: the card is exported
//! through `pscat --pdf`, `--svg` and `--png --dpi 300`, the SVG and PDF are
//! rasterized by independent renderers (resvg, hayro), and both independent
//! decoders must read the exact URL from the back of every export and find no
//! symbol on the front. Around that: the physical size of every page, the
//! 9 pt safe margin (nothing printed near the trim, so no bleed is needed),
//! every module and the quiet zone pixel by pixel, the minimum type size,
//! base-35 fonts only and no raster images. The committed gallery still, a
//! proof of both sides, must decode too.

mod common;
mod qr_support;
use common::CommandDeadline as _;
use pscat::{Interp, Value};
use qr_support::{DECODERS, Grey, raster_pdf_page, raster_png, raster_svg};
use std::path::{Path, PathBuf};
use std::process::Command;

const CARD: &str = "gallery/pscat_card.ps";
const STILL: &str = "gallery/renders/pscat_card.png";
const URL: &str = "https://github.com/PSCatLabs/pscat";

/// The card's QR placement, as `gallery/pscat_card.ps` draws it: the
/// lower-left corner of the quiet zone and the module side, in points.
/// Every value is a whole number of 300 dpi pixels (0.24 pt each).
const QR_X: f64 = 171.12;
const QR_Y: f64 = 37.44;
const MODULE: f64 = 1.68;
const QUIET: usize = 4;

/// The print resolution and the card's own.
const DPI: f64 = 300.0;
const CARD_W: f64 = 252.0;
const CARD_H: f64 = 144.0;
/// The safe margin: 1/8 in inside the trim on every side.
const SAFE: f64 = 9.0;

/// The navy the symbol is drawn in, as the greyscale the rasters are read in
/// (`qr_support::grey_of`'s luma of 27 42 82).
const NAVY_GREY: u8 = 42;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// The symbol the card should carry, encoded in-process from the library:
/// (version, modules row-major from the top-left, 1 = dark).
fn expected_symbol() -> (u64, usize, Vec<u8>) {
    let mut it = Interp::with_page(8, 8).expect("small test page");
    let src = format!(
        "(lib/qr.ps) run ({URL}) << /Level /H >> qrencode /qsym exch def \
         /qs qsym /Size get def /qo qs qs mul string def \
         0 1 qs 1 sub {{ /qy exch def qsym /Modules get qy get \
           0 1 qs 1 sub {{ /qx exch def dup qx get {{ qo qy qs mul qx add 1 put }} if }} for pop \
         }} for \
         qsym /Version get qs qo"
    );
    it.run_str(&src)
        .unwrap_or_else(|e| panic!("encode: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    assert_eq!(stack.len(), 3, "encode leaves version, size, modules");
    let modules = match &stack[2].value {
        Value::String(s) => s.borrow_bytes().to_vec(),
        _ => panic!("modules: {}", stack[2].repr()),
    };
    let size: usize = stack[1].repr().parse().expect("size");
    let version: u64 = stack[0].repr().parse().expect("version");
    (version, size, modules)
}

/// Run pscat on the card with `args` (output flags) in a fresh scratch
/// directory; returns that directory. `stdin` replaces the file with a
/// program read from standard input.
fn pscat(tag: &str, args: &[&str], stdin: Option<&str>) -> (PathBuf, String) {
    let dir = common::scratch_dir(&format!("qr-card-{tag}"));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
    cmd.current_dir(root());
    for a in args {
        cmd.arg(a.replace("{dir}", dir.to_str().expect("utf-8 scratch path")));
    }
    let out = match stdin {
        Some(program) => {
            cmd.arg("-");
            cmd.output_with_input(program.as_bytes())
        }
        None => {
            cmd.arg(CARD);
            cmd.output_deadline()
        }
    }
    .unwrap_or_else(|e| panic!("{tag}: run pscat: {e}"));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "{tag}: pscat {args:?} failed:\n{text}"
    );
    (dir, text)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Points to device pixels at `DPI`, which must land on a whole pixel.
fn px(pt: f64) -> usize {
    let p = pt * DPI / 72.0;
    assert!(
        (p - p.round()).abs() < 1e-6 && p >= 0.0,
        "{pt} pt is not a whole 300 dpi pixel"
    );
    p.round() as usize
}

/// Every pixel of the drawing (quiet zone included) whose lower-left corner
/// is at (`x0`, `y0`) points on a page `page_h` points tall must be exactly
/// light or dark as its module says: at a whole-pixel pitch no pixel
/// straddles a module edge, so this pins the quiet-zone width, every module
/// edge and the absence of seams between touching modules.
fn check_symbol_pixels(
    g: &Grey,
    page_h: f64,
    x0: f64,
    y0: f64,
    size: usize,
    modules: &[u8],
    ctx: &str,
) {
    let step = px(MODULE);
    let cells = size + 2 * QUIET;
    let left = px(x0);
    let top = px(page_h - y0) - cells * step;
    for row in 0..cells {
        for col in 0..cells {
            let (r, c) = (row as i64 - QUIET as i64, col as i64 - QUIET as i64);
            let inside = (0..size as i64).contains(&r) && (0..size as i64).contains(&c);
            let dark = inside && modules[r as usize * size + c as usize] == 1;
            let (want, area) = if dark {
                (NAVY_GREY, "dark module")
            } else if inside {
                (255, "light module")
            } else {
                (255, "quiet zone")
            };
            for y in top + row * step..top + (row + 1) * step {
                for x in left + col * step..left + (col + 1) * step {
                    let got = g.at(x, y);
                    assert!(
                        got.abs_diff(want) <= 4,
                        "{ctx}: pixel ({x}, {y}) in the {area} at column {col}, row {row} \
                         (quiet zone included) is grey {got}, want {want}"
                    );
                }
            }
        }
    }
}

/// Nothing is printed within the safe margin: every pixel of the 9 pt band
/// inside each trim edge is paper white.
fn check_safe_margin(g: &Grey, ctx: &str) {
    let m = (SAFE * DPI / 72.0).floor() as usize; // 37 whole pixels of the 37.5
    let (w, h) = (g.width, g.height);
    for y in 0..h {
        for x in 0..w {
            if x < m || x >= w - m || y < m || y >= h - m {
                let got = g.at(x, y);
                assert!(
                    got >= 250,
                    "{ctx}: pixel ({x}, {y}), inside the 9 pt safe margin, is grey {got}"
                );
            }
        }
    }
}

/// Both decoders read exactly the URL, at version 4 and level H where they
/// report it.
fn check_decodes(g: &Grey, version: u64, ctx: &str) {
    for (name, decode) in DECODERS {
        let r = decode(g).unwrap_or_else(|e| panic!("{ctx}: {name} cannot read the QR: {e}"));
        assert_eq!(
            r.payload,
            URL.as_bytes(),
            "{ctx}: {name} read {:?}",
            String::from_utf8_lossy(&r.payload)
        );
        assert_eq!(r.level, qr_support::level_code("H"), "{ctx}: {name} level");
        if let Some(v) = r.version {
            assert_eq!(v as u64, version, "{ctx}: {name} version");
        }
    }
}

/// Neither decoder finds a symbol (the front has none).
fn check_no_symbol(g: &Grey, ctx: &str) {
    for (name, decode) in DECODERS {
        assert!(
            decode(g).is_err(),
            "{ctx}: {name} found a QR symbol on the front"
        );
    }
}

#[test]
fn every_export_has_two_card_sized_sides_and_the_back_scans() {
    let (version, size, modules) = expected_symbol();
    // The 34-byte URL is exactly version 4's capacity at level H, so the
    // symbol is 33 modules: 41 with the quiet zone, 68.88 pt, which is the
    // design's floor of 0.8 in and more.
    assert_eq!((version, size), (4, 33), "symbol version and size");
    let side = (size + 2 * QUIET) as f64 * MODULE;
    assert!(side >= 57.6, "symbol side {side} pt is under 0.8 in");
    assert!(
        QR_X >= SAFE
            && QR_Y >= SAFE
            && QR_X + side <= CARD_W - SAFE
            && QR_Y + side <= CARD_H - SAFE,
        "the symbol, quiet zone included, must sit inside the safe margin"
    );

    let (pdf_dir, _) = pscat(
        "pdf",
        &[
            "--page",
            "252x144",
            "--lint-strict",
            "--pdf",
            "{dir}/card.pdf",
        ],
        None,
    );
    let pdf = read(&pdf_dir.join("card.pdf"));
    let pdf_text = common::pdf_text(&pdf);
    assert_eq!(
        pdf_text.matches("/MediaBox [0 0 252 144]").count(),
        2,
        "PDF: two pages, each 252 x 144 pt"
    );
    assert!(
        pdf_text.contains("/Count 2"),
        "PDF page tree counts two pages"
    );
    assert!(
        !pdf_text.contains("/Subtype /Image"),
        "PDF carries a raster image"
    );

    let (svg_dir, _) = pscat(
        "svg",
        &["--page", "252x144", "--svg", "{dir}/card.svg"],
        None,
    );
    let (png_dir, _) = pscat(
        "png",
        &[
            "--page",
            "252x144",
            "--dpi",
            "300",
            "--png",
            "{dir}/card.png",
        ],
        None,
    );
    let scale = (DPI / 72.0) as f32;
    for (side_index, side_name) in [(0, "front"), (1, "back")] {
        let n = side_index + 1;
        let svg = read(&svg_dir.join(format!("card-{n:03}.svg")));
        let svg_text = String::from_utf8_lossy(&svg);
        assert!(
            svg_text.contains("width=\"252pt\" height=\"144pt\" viewBox=\"0 0 252 144\""),
            "SVG {side_name}: not 252 x 144 user units (points)"
        );
        assert!(
            !svg_text.contains("<image"),
            "SVG {side_name}: carries a raster image"
        );
        let png = read(&png_dir.join(format!("card-{n:03}.png")));
        let rasters = [
            ("PNG 300 dpi", raster_png(&png)),
            ("SVG via resvg", raster_svg(&svg, scale)),
            ("PDF via hayro", raster_pdf_page(&pdf, scale, side_index)),
        ];
        for (export, g) in &rasters {
            let ctx = format!("{export}, {side_name}");
            assert_eq!(
                (g.width, g.height),
                (1050, 600),
                "{ctx}: 3.5 x 2 in at 300 dpi"
            );
            check_safe_margin(g, &ctx);
            if side_index == 0 {
                check_no_symbol(g, &ctx);
            } else {
                check_symbol_pixels(g, CARD_H, QR_X, QR_Y, size, &modules, &ctx);
                check_decodes(g, version, &ctx);
            }
        }
    }
}

/// The committed gallery still is the proof sheet the source draws with
/// `/CardProof` defined (command in the source's header): 300 x 360 pt at
/// 300 dpi, both sides on it, the back's symbol still on whole pixels and
/// read by both decoders.
#[test]
fn the_gallery_still_is_a_scannable_proof_of_both_sides() {
    let (version, size, modules) = expected_symbol();
    let g = raster_png(&read(&root().join(STILL)));
    assert_eq!(
        (g.width, g.height),
        (1250, 1500),
        "{STILL}: 300 x 360 pt at 300 dpi"
    );
    // The back sits at (24, 24) on the proof, the front above it.
    check_symbol_pixels(&g, 360.0, 24.0 + QR_X, 24.0 + QR_Y, size, &modules, STILL);
    check_decodes(&g, version, STILL);
    let grids = {
        let mut image =
            rqrr::PreparedImage::prepare_from_greyscale(g.width, g.height, |x, y| g.at(x, y));
        image.detect_grids().len()
    };
    assert_eq!(grids, 1, "{STILL}: exactly one symbol on the proof");
}

/// The card sets every letter at 9 pt or more, in LaserWriter 35 faces only
/// (compiled into pscat: no font pack needed), and paints no raster image.
/// Checked by running the card with `findfont`, the show operators and the
/// image operators wrapped to report each use.
#[test]
fn the_card_uses_base_35_fonts_at_9_pt_or_more_and_no_images() {
    const BASE_35: [&str; 35] = [
        "AvantGarde-Book",
        "AvantGarde-BookOblique",
        "AvantGarde-Demi",
        "AvantGarde-DemiOblique",
        "Bookman-Demi",
        "Bookman-DemiItalic",
        "Bookman-Light",
        "Bookman-LightItalic",
        "Courier",
        "Courier-Bold",
        "Courier-BoldOblique",
        "Courier-Oblique",
        "Helvetica",
        "Helvetica-Bold",
        "Helvetica-BoldOblique",
        "Helvetica-Narrow",
        "Helvetica-Narrow-Bold",
        "Helvetica-Narrow-BoldOblique",
        "Helvetica-Narrow-Oblique",
        "Helvetica-Oblique",
        "NewCenturySchlbk-Bold",
        "NewCenturySchlbk-BoldItalic",
        "NewCenturySchlbk-Italic",
        "NewCenturySchlbk-Roman",
        "Palatino-Bold",
        "Palatino-BoldItalic",
        "Palatino-Italic",
        "Palatino-Roman",
        "Symbol",
        "Times-Bold",
        "Times-BoldItalic",
        "Times-Italic",
        "Times-Roman",
        "ZapfChancery-MediumItalic",
        "ZapfDingbats",
    ];
    // The text size a show draws at: the font's em, through the font matrix
    // and the CTM, in device units (points at the default 72 dpi).
    let prelude = "\
        /card-em { currentfont /FontMatrix get dup 2 get exch 3 get dtransform \
          dup mul exch dup mul add sqrt 1000 mul } def \
        /card-report { (CARD-SHOW ) print card-em == } def \
        /findfont { (CARD-FONT ) print dup == systemdict /findfont get exec } def \
        /show { card-report systemdict /show get exec } def \
        /ashow { card-report systemdict /ashow get exec } def \
        /widthshow { card-report systemdict /widthshow get exec } def \
        /awidthshow { card-report systemdict /awidthshow get exec } def \
        /kshow { card-report systemdict /kshow get exec } def \
        /xshow { card-report systemdict /xshow get exec } def \
        /yshow { card-report systemdict /yshow get exec } def \
        /xyshow { card-report systemdict /xyshow get exec } def \
        /glyphshow { card-report systemdict /glyphshow get exec } def \
        /image { (CARD-RASTER) = systemdict /image get exec } def \
        /colorimage { (CARD-RASTER) = systemdict /colorimage get exec } def \
        /imagemask { (CARD-RASTER) = systemdict /imagemask get exec } def\n";
    let source = String::from_utf8(read(&root().join(CARD))).expect("utf-8 source");
    let (_, out) = pscat(
        "fonts",
        &["--page", "252x144", "--headless"],
        Some(&format!("{prelude}{source}")),
    );
    let mut fonts = 0;
    let mut shows = 0;
    for line in out.lines() {
        if let Some(name) = line.strip_prefix("CARD-FONT /") {
            assert!(
                BASE_35.contains(&name),
                "the card uses {name}, not a LaserWriter 35 face"
            );
            fonts += 1;
        } else if let Some(em) = line.strip_prefix("CARD-SHOW ") {
            let em: f64 = em
                .trim()
                .parse()
                .unwrap_or_else(|_| panic!("show size {em:?}"));
            assert!(em >= 9.0 - 1e-6, "text set at {em} pt, under 9 pt");
            shows += 1;
        }
        assert!(
            !line.starts_with("CARD-RASTER"),
            "the card paints a raster image"
        );
    }
    assert!(
        fonts >= 4 && shows >= 10,
        "too few fonts ({fonts}) or shows ({shows}) seen:\n{out}"
    );
}
