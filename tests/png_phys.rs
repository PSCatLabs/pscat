//! Every PNG pscat writes records its resolution (issue #982): a `pHYs`
//! chunk with unit 1 (metre) and round(dpi / 0.0254) pixels per metre on
//! both axes, and the decoded pixels are the ones the encoder alone gives.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tiny_skia::Pixmap;

mod common;
use common::CommandDeadline as _;

const BIN: &str = env!("CARGO_BIN_EXE_pscat");

/// A 2 x 1 inch page with a coloured square, and a second page.
const CARD: &str = "%!PS\n0.2 0.5 0.8 setrgbcolor 10 10 60 40 rectfill showpage\n\
                    1 0 0 setrgbcolor 0 0 30 30 rectfill showpage\n";

fn pscat(args: &[&str]) -> (bool, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::null())
        .output_deadline()
        .expect("spawn pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn source(dir: &Path) -> PathBuf {
    let path = dir.join("card.ps");
    std::fs::write(&path, CARD).expect("write source");
    path
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The pixels of a PNG, as the straight RGBA tiny-skia decodes.
fn pixels(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let pm = Pixmap::decode_png(png).expect("decode");
    (pm.width(), pm.height(), pm.data().to_vec())
}

/// `png` without its `pHYs` chunk: what the encoder alone wrote.
fn without_phys(png: &[u8]) -> Vec<u8> {
    assert_eq!(&png[37..41], b"pHYs");
    [&png[..33], &png[54..]].concat()
}

#[test]
fn the_default_dpi_records_72_dpi() {
    let dir = common::scratch_dir("phys-default");
    let out = dir.join("a.png");
    let (ok, err) = pscat(&[
        "--page",
        "100x60",
        "--png",
        out.to_str().unwrap(),
        source(&dir).to_str().unwrap(),
    ]);
    assert!(ok, "{err}");
    let first = read(&dir.join("a-001.png"));
    // 72 dpi = 2834.6 px/m, rounded.
    assert_eq!(common::png_phys(&first), (2835, 2835, 1));
    assert_eq!(
        common::png_phys(&read(&dir.join("a-002.png"))),
        (2835, 2835, 1)
    );
    assert_eq!(pixels(&first).0, 100);
}

#[test]
fn dpi_300_records_11811_pixels_per_metre() {
    let dir = common::scratch_dir("phys-300");
    let out = dir.join("a.png");
    let (ok, err) = pscat(&[
        "--page",
        "100x60",
        "--dpi",
        "300",
        "--png",
        out.to_str().unwrap(),
        source(&dir).to_str().unwrap(),
    ]);
    assert!(ok, "{err}");
    for n in 1..=2 {
        let png = read(&dir.join(format!("a-{n:03}.png")));
        assert_eq!(common::png_phys(&png), (11811, 11811, 1), "page {n}");
        // A 100 x 60 point page at 300 dpi.
        assert_eq!((pixels(&png).0, pixels(&png).1), (417, 250));
    }
}

#[test]
fn other_dpis_follow_the_formula() {
    let dir = common::scratch_dir("phys-formula");
    for dpi in [9u32, 96, 144, 600, 1200] {
        let out = dir.join(format!("d{dpi}.png"));
        let (ok, err) = pscat(&[
            "--page",
            "20x20",
            "--dpi",
            &dpi.to_string(),
            "--png",
            out.to_str().unwrap(),
            "-e",
            "0 0 5 5 rectfill",
        ]);
        assert!(ok, "{dpi}: {err}");
        let want = (f64::from(dpi) / 0.0254).round() as u32;
        assert_eq!(common::png_phys(&read(&out)), (want, want, 1), "{dpi} dpi");
    }
}

/// Banded, halftone and sweep renders go through other code paths; each
/// records the resolution too, and the banded file is still the plain one
/// byte for byte.
#[test]
fn bands_halftone_sweep_and_contact_sheet_record_it() {
    let dir = common::scratch_dir("phys-paths");
    let src = dir.join("one.ps");
    std::fs::write(
        &src,
        "%!PS\n0.3 0.6 0.2 setrgbcolor 5 5 40 30 rectfill showpage\n",
    )
    .unwrap();
    let src = src.to_str().unwrap();
    let common_args = ["--page", "60x40", "--dpi", "150"];
    let ppm = (150.0f64 / 0.0254).round() as u32;

    let run = |extra: &[&str], out: &str| -> Vec<u8> {
        let out = dir.join(out);
        let mut args = common_args.to_vec();
        args.extend(extra);
        args.extend(["--png", out.to_str().unwrap(), src]);
        let (ok, err) = pscat(&args);
        assert!(ok, "{extra:?}: {err}");
        read(&out)
    };

    let plain = run(&[], "plain.png");
    assert_eq!(common::png_phys(&plain), (ppm, ppm, 1));
    let banded = run(&["--bands", "3"], "banded.png");
    assert_eq!(
        banded, plain,
        "--bands is still the plain render byte for byte"
    );
    let halftone = run(&["--halftone"], "half.png");
    assert_eq!(common::png_phys(&halftone), (ppm, ppm, 1));

    let sweep_out = dir.join("sweep.png");
    let sheet_out = dir.join("contact.png");
    let (ok, err) = pscat(&[
        "--page",
        "60x40",
        "--dpi",
        "150",
        "--sweep-seed",
        "1:2",
        "--png",
        sweep_out.to_str().unwrap(),
        "--contact-sheet",
        sheet_out.to_str().unwrap(),
        src,
    ]);
    assert!(ok, "{err}");
    for name in ["sweep-001.png", "sweep-002.png", "contact.png"] {
        assert_eq!(
            common::png_phys(&read(&dir.join(name))),
            (ppm, ppm, 1),
            "{name}"
        );
    }
}

/// `pscat sheet` composes cells of any dpi, so it records the nominal 72.
#[test]
fn a_review_sheet_records_72_dpi() {
    let dir = common::scratch_dir("phys-sheet");
    let src = source(&dir);
    let out = dir.join("sheet.png");
    let (ok, err) = pscat(&[
        "sheet",
        "-o",
        out.to_str().unwrap(),
        "--page",
        "60x40",
        "--dpi",
        "72,144",
        src.to_str().unwrap(),
    ]);
    assert!(ok, "{err}");
    assert_eq!(common::png_phys(&read(&out)), (2835, 2835, 1));
}

/// The library's `Page::to_png` is what `--png` writes: the same chunk.
#[test]
fn the_library_page_carries_its_scale() {
    for (scale, want) in [(1.0f32, 2835u32), (300.0 / 72.0, 11811)] {
        let mut interp = pscat::Interp::with_page_scaled(40, 30, scale).expect("page");
        interp.run_str("0 0 10 10 rectfill showpage").expect("run");
        let png = interp.pages()[0].to_png().expect("png");
        assert_eq!(common::png_phys(&png), (want, want, 1));
    }
}

/// The only change to the file is that chunk: remove it and what is left is
/// the encoder's own output, so the decoded pixels cannot have moved.
#[test]
fn removing_the_chunk_leaves_the_encoders_bytes_and_the_same_pixels() {
    let mut interp = pscat::Interp::with_page_scaled(40, 30, 2.0).expect("page");
    interp
        .run_str("0.9 0.2 0.1 setrgbcolor 3 3 20 10 rectfill showpage")
        .expect("run");
    let page = &interp.pages()[0];
    let png = page.to_png().expect("png");
    let bare = without_phys(&png);
    assert_eq!(common::png_chunk_types(&bare), ["IHDR", "IDAT", "IEND"]);
    let (w, h, data) = pixels(&png);
    assert_eq!((pixels(&bare).0, pixels(&bare).1), (w, h));
    assert_eq!(pixels(&bare).2, data);
}
