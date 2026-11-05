//! The reversed hybrid in `examples/hybrid/alpine_trail.ps` (#1001): pscat
//! draws a layout reference, an image model makes it a photograph, and
//! pscat lays the exact layer back on at the coordinates it defined. The
//! facts `docs/guides/pscat-with-image-models.md` states about it are
//! checked here:
//!
//! - the program still draws the reference the image model was given
//!   (`alpine_trail_ref.png`, committed as sent), so the coordinates the
//!   exact layer uses are the ones the photographs were made from;
//! - how far each photograph moved the layout, measured with one detector
//!   on the committed reference and on the committed JPEG: the ridge line
//!   against the sky, column by column, the edge of the foreground shade
//!   that holds the facts, and the hut's red roof; and how quiet the two
//!   zones left for type stayed. The numbers are pinned;
//! - the QR code of the hybrid and of the pscat-alone variant decodes in
//!   two independent decoders (rqrr, rxing) from the PNG and from the PDF
//!   rasterized by hayro;
//! - every line of the brief is painted exactly;
//! - what the decoders make of the image model's own QR code, from its
//!   whole-poster attempt, is pinned.

mod common;
mod qr_support;
use common::CommandDeadline as _;
use qr_support::{Grey, grey_of, raster_pdf, raster_png, read_rxing};
use std::path::{Path, PathBuf};
use std::process::Command;

const URL: &str = "https://pscatlabs.github.io/pscat/hybrid.html";
const SOURCE: &str = "examples/hybrid/alpine_trail.ps";
const PAGE: &str = "576x864";
/// The layout reference exactly as the image model got it.
const REFERENCE: &str = "examples/hybrid/alpine_trail_ref.png";
const SUMMER: &str = "examples/hybrid/alpine_trail_photo.jpg";
const AUTUMN: &str = "examples/hybrid/alpine_trail_autumn.jpg";
const MODEL_ALONE: &str = "examples/hybrid/alpine_trail_model.jpg";

/// Every line of copy the poster must carry, exactly.
const COPY: &[&str] = &[
    "PIZ CALDERAN",
    "VAL BRENNA · THE SUMMIT TRAIL · SUMMER 2027",
    "Piz Calderan",
    "2,874 m",
    "Piz Mora",
    "2,512 m",
    "Bocchetta di Brenna",
    "2,436 m",
    "Rifugio Lago Scuro",
    "1,862 m",
    "Pont da Brenna",
    "THE TRAIL",
    "Distance, one way",
    "9.4 km",
    "Ascent",
    "1,462 m",
    "Walking time up",
    "6 h 05 min",
    "Hut open",
    "20 June – 20 September",
    "BUS 412",
    "Valbrenna station → Pont da Brenna",
    "07:10   08:25   09:40",
    "Pont da Brenna → Valbrenna station",
    "15:05   16:35   18:05",
    "Scan: how this poster was made",
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Run pscat from the repository root on `prelude` and the example, with
/// `args` (`{dir}` is the scratch directory). Returns the scratch
/// directory and standard output.
fn pscat(tag: &str, prelude: &str, args: &[&str]) -> (PathBuf, Vec<u8>) {
    let dir = common::scratch_dir(&format!("hybrid-reverse-{tag}"));
    let source = std::fs::read_to_string(root().join(SOURCE)).expect("read the example");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
    cmd.current_dir(root()).args(["--page", PAGE]);
    for a in args {
        cmd.arg(a.replace("{dir}", dir.to_str().expect("utf-8 scratch path")));
    }
    cmd.arg("-");
    let out = cmd
        .output_with_input(format!("{prelude}\n{source}").as_bytes())
        .unwrap_or_else(|e| panic!("{tag}: run pscat: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && !stdout.contains("%%[ Error") && !stderr.contains("%%[ Error"),
        "{tag}: pscat {args:?} failed:\n{stdout}{stderr}"
    );
    (dir, out.stdout)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// An RGB raster, row by row from the top.
struct Rgb {
    width: usize,
    height: usize,
    px: Vec<[u8; 3]>,
}

impl Rgb {
    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.px[y * self.width + x]
    }
    /// Rec. 601 luma.
    fn luma(&self, x: usize, y: usize) -> f64 {
        let [r, g, b] = self.at(x, y).map(f64::from);
        0.299 * r + 0.587 * g + 0.114 * b
    }
}

fn rgb_png(bytes: &[u8]) -> Rgb {
    let p = tiny_skia::Pixmap::decode_png(bytes).expect("decode PNG");
    let px = p
        .pixels()
        .iter()
        .map(|c| {
            let c = c.demultiply();
            [c.red(), c.green(), c.blue()]
        })
        .collect();
    Rgb {
        width: p.width() as usize,
        height: p.height() as usize,
        px,
    }
}

fn rgb_jpeg(path: &str) -> Rgb {
    let mut dec = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(read(&root().join(path))));
    dec.decode_headers().expect("JPEG headers");
    let (width, height) = dec.dimensions().expect("JPEG dimensions");
    let raw = dec.decode().expect("decode JPEG");
    let px = raw.as_chunks::<3>().0.to_vec();
    Rgb { width, height, px }
}

fn grey_rgb(img: &Rgb) -> Grey {
    grey_of(img.width, img.px.iter().map(|p| [p[0], p[1], p[2], 255]))
}

/// The image model's own QR code, from its whole-poster attempt (the
/// committed JPEG; the PNG original gave the same result): rqrr finds the
/// symbol and its error correction fails; rxing does not read it. If a
/// regenerated image ever scans, this fails and the guide's comparison
/// must be rewritten to match.
#[test]
fn the_image_models_qr_code_is_found_but_does_not_decode() {
    let g = grey_rgb(&rgb_jpeg(MODEL_ALONE));
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(g.width, g.height, |x, y| g.at(x, y));
    let found: Vec<_> = image
        .detect_grids()
        .iter()
        .map(|grid| grid.decode().map(|(_, s)| s))
        .collect();
    assert!(!found.is_empty(), "rqrr finds no symbol at all");
    assert!(
        found.iter().all(Result::is_err),
        "rqrr decoded the image model's QR: {found:?}"
    );
    assert!(read_rxing(&g).is_err(), "rxing decoded the image model's QR");
}

/// The program still draws the reference the image model was given: the
/// committed PNG is what was sent, and `/Stage /Reference` at 128 dpi
/// renders it again (allowing a little anti-aliasing drift between
/// platforms on the shapes' edges).
#[test]
fn the_program_draws_the_reference_the_model_was_given() {
    let (dir, _) = pscat(
        "reference",
        "/Stage /Reference def",
        &["--dpi", "128", "--png", "{dir}/ref.png"],
    );
    let now = rgb_png(&read(&dir.join("ref.png")));
    let sent = rgb_png(&read(&root().join(REFERENCE)));
    assert_eq!((now.width, now.height), (1024, 1536));
    assert_eq!((sent.width, sent.height), (1024, 1536));
    let differ = now
        .px
        .iter()
        .zip(&sent.px)
        .filter(|(a, b)| (0..3).any(|i| a[i].abs_diff(b[i]) > 24))
        .count();
    assert!(
        differ < now.px.len() / 1000,
        "{differ} px differ from the reference the image model was given"
    );
    for photo in [SUMMER, AUTUMN] {
        let p = rgb_jpeg(photo);
        assert_eq!(
            (p.width, p.height),
            (1024, 1536),
            "{photo}: placed at exactly 128 dpi, no crop"
        );
    }
}

/// The ridge line, per column: scanning down from the top of the sky, the
/// first row whose colour differs from the row 4 px above by more than 60
/// (summed over R, G and B). The sky is a smooth gradient with no
/// clouds, so the first such edge is the rock against it.
fn ridge(img: &Rgb) -> Vec<usize> {
    (0..img.width)
        .map(|x| {
            (300..1100)
                .find(|&y| {
                    let (a, b) = (img.at(x, y - 4), img.at(x, y));
                    (0..3).map(|i| u32::from(a[i].abs_diff(b[i]))).sum::<u32>() > 60
                })
                .unwrap_or_else(|| panic!("no ridge in column {x}"))
        })
        .collect()
}

/// The edge of the foreground shade, per column: the row between 1050
/// and 1450 px where the 16 px above are brightest against the 16 px
/// below (luma summed).
fn shade_edge(img: &Rgb) -> Vec<usize> {
    (0..img.width)
        .map(|x| {
            let col: Vec<f64> = (1050..1450).map(|y| img.luma(x, y)).collect();
            (16..col.len() - 16)
                .max_by(|&i, &j| {
                    let step = |k: usize| {
                        col[k - 16..k].iter().sum::<f64>() - col[k..k + 16].iter().sum::<f64>()
                    };
                    step(i).total_cmp(&step(j))
                })
                .expect("a step")
                + 1050
        })
        .collect()
}

/// The centre of the hut's red roof: the saturated red pixels around the
/// hut, as (x, y).
fn roof(img: &Rgb) -> (f64, f64) {
    let (mut sx, mut sy, mut n) = (0usize, 0usize, 0usize);
    for y in 840..960 {
        for x in 100..300 {
            let [r, g, b] = img.at(x, y).map(u32::from);
            if r > 120 && r * 10 > g * 22 && r * 10 > b * 22 {
                sx += x;
                sy += y;
                n += 1;
            }
        }
    }
    assert!(n > 100, "no red roof found ({n} px)");
    (sx as f64 / n as f64, sy as f64 / n as f64)
}

/// How busy a zone is: the standard deviation of luma about each row's
/// own mean, so a smooth gradient counts as quiet.
fn busyness(img: &Rgb, rows: std::ops::Range<usize>) -> f64 {
    let mut dev = Vec::new();
    for y in rows.step_by(2) {
        let row: Vec<f64> = (0..img.width).step_by(2).map(|x| img.luma(x, y)).collect();
        let m = row.iter().sum::<f64>() / row.len() as f64;
        dev.extend(row.iter().map(|v| v - m));
    }
    (dev.iter().map(|d| d * d).sum::<f64>() / dev.len() as f64).sqrt()
}

/// Sorted absolute differences, column by column.
fn drift(a: &[usize], b: &[usize]) -> Vec<usize> {
    let mut d: Vec<usize> = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).collect();
    d.sort_unstable();
    d
}

struct Fidelity {
    ridge_mean: f64,
    ridge_p95: usize,
    ridge_max: usize,
    shade_median: usize,
    roof: (f64, f64),
    title_busy: f64,
    info_busy: f64,
}

fn fidelity(photo: &str) -> Fidelity {
    let r = rgb_png(&read(&root().join(REFERENCE)));
    let p = rgb_jpeg(photo);
    let d = drift(&ridge(&r), &ridge(&p));
    let s = drift(&shade_edge(&r), &shade_edge(&p));
    let (rx, ry) = roof(&r);
    let (px, py) = roof(&p);
    let f = Fidelity {
        ridge_mean: d.iter().sum::<usize>() as f64 / d.len() as f64,
        ridge_p95: d[d.len() * 95 / 100],
        ridge_max: d[d.len() - 1],
        shade_median: s[s.len() / 2],
        roof: (px - rx, py - ry),
        // the title's rows, and the facts' rows, in the 1536 px frame
        title_busy: busyness(&p, 60..460),
        info_busy: busyness(&p, 1250..1520),
    };
    eprintln!(
        "{photo}: ridge mean {:.2} p95 {} max {}; shade median {}; roof ({:.1}, {:.1}); \
         busyness title {:.1} info {:.1}",
        f.ridge_mean,
        f.ridge_p95,
        f.ridge_max,
        f.shade_median,
        f.roof.0,
        f.roof.1,
        f.title_busy,
        f.info_busy
    );
    f
}

fn near(v: f64, want: f64, tol: f64) -> bool {
    (v - want).abs() <= tol
}

/// The layout fidelity the guide states, for each photograph made from
/// the one reference (1 px is 0.56 pt on the poster).
#[test]
fn the_photographs_keep_the_layout() {
    // (photo, ridge mean, p95, max, shade median, roof dx, dy, title, info)
    let pinned = [
        (SUMMER, 1.21, 4, 7, 4, -3.8, 4.3, 6.6, 9.1),
        (AUTUMN, 1.12, 4, 7, 5, -14.1, 1.3, 5.3, 16.3),
    ];
    for (photo, mean, p95, max, shade, dx, dy, title, info) in pinned {
        let f = fidelity(photo);
        assert!(near(f.ridge_mean, mean, 0.06), "{photo}: ridge mean {:.2}", f.ridge_mean);
        assert_eq!((f.ridge_p95, f.ridge_max), (p95, max), "{photo}: ridge p95, max");
        assert_eq!(f.shade_median, shade, "{photo}: shade edge median");
        assert!(
            near(f.roof.0, dx, 0.6) && near(f.roof.1, dy, 0.6),
            "{photo}: roof moved ({:.1}, {:.1})",
            f.roof.0,
            f.roof.1
        );
        assert!(near(f.title_busy, title, 0.3), "{photo}: title zone {:.1}", f.title_busy);
        assert!(near(f.info_busy, info, 0.3), "{photo}: info zone {:.1}", f.info_busy);
    }
}

/// Both decoders must read exactly the URL.
fn assert_decodes(g: &Grey, ctx: &str) {
    let r = qr_support::read_rqrr(g).unwrap_or_else(|e| panic!("{ctx}: rqrr: {e}"));
    assert_eq!(String::from_utf8_lossy(&r.payload), URL, "{ctx}: rqrr");
    let r = read_rxing(g).unwrap_or_else(|e| panic!("{ctx}: rxing: {e}"));
    assert_eq!(String::from_utf8_lossy(&r.payload), URL, "{ctx}: rxing");
}

#[test]
fn the_qr_code_decodes_in_png_and_pdf() {
    for (variant, prelude) in [("hybrid", ""), ("pscat-only", "/PscatOnly true def")] {
        let (dir, _) = pscat(
            &format!("{variant}-png"),
            prelude,
            &["--dpi", "144", "--png", "{dir}/out.png"],
        );
        assert_decodes(
            &raster_png(&read(&dir.join("out.png"))),
            &format!("{variant} PNG"),
        );
        let (dir, _) = pscat(&format!("{variant}-pdf"), prelude, &["--pdf", "{dir}/out.pdf"]);
        assert_decodes(
            &raster_pdf(&read(&dir.join("out.pdf")), 2.0),
            &format!("{variant} PDF (hayro)"),
        );
    }
}

/// The poster's fonts put the en dash at 0x80 and the arrow at 0x81 of
/// ISOLatin1Encoding; read the printed bytes back that way.
fn latin1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80 => '\u{2013}',
            0x81 => '\u{2192}',
            _ => char::from(b),
        })
        .collect()
}

#[test]
fn the_poster_paints_the_brief_exactly() {
    for variant in ["", "/PscatOnly true def"] {
        let prelude = format!(
            "{variant}\n/show {{ dup print (\\n) print //show }} bind def\n\
             /ashow {{ dup print (\\n) print //ashow }} bind def"
        );
        let (_, out) = pscat("copy", &prelude, &["--png", "{dir}/copy.png"]);
        let shown: Vec<String> = out.split(|&b| b == b'\n').map(latin1).collect();
        for want in COPY {
            assert!(
                shown.iter().any(|s| s == want),
                "{variant:?}: the brief's {want:?} is not painted; painted: {shown:?}"
            );
        }
    }
}
