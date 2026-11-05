//! The reversed hybrid in `examples/hybrid/alpine_trail.ps` (#1001): pscat
//! draws a layout reference, an image model makes it a photograph, and
//! pscat lays the exact layer back on at the coordinates it defined. The
//! facts `docs/guides/pscat-with-image-models.md` states about it are
//! checked here:
//!
//! - the program still draws, pixel for pixel, the reference the image
//!   model was given (so the coordinates the exact layer uses are the ones
//!   the picture was made from);
//! - how far the photograph moved the layout: the ridge line against the
//!   sky, column by column, and the hut's red roof, measured with one
//!   detector on the reference and on the committed JPEG, and pinned;
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
const PHOTO: &str = "examples/hybrid/alpine_trail_photo.jpg";
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
}

fn rgb_png(bytes: &[u8]) -> Rgb {
    let p = tiny_skia::Pixmap::decode_png(bytes).expect("pscat PNG");
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
/// committed JPEG; the PNG original gave the same result): neither
/// decoder reads it. If a regenerated image ever scans, this fails and the
/// guide's comparison must be rewritten to match.
#[test]
fn the_image_models_qr_code_does_not_decode() {
    let g = grey_rgb(&rgb_jpeg(MODEL_ALONE));
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(g.width, g.height, |x, y| g.at(x, y));
    let found: Vec<_> = image
        .detect_grids()
        .iter()
        .map(|grid| grid.decode().map(|(_, s)| s))
        .collect();
    eprintln!("rqrr on the image model's poster: {found:?}");
    assert!(
        found.iter().all(Result::is_err),
        "rqrr decoded the image model's QR: {found:?}"
    );
    assert!(read_rxing(&g).is_err(), "rxing decoded the image model's QR");
}

/// The reference as the image model got it: `/Stage /Reference` at 128 dpi.
fn reference() -> Rgb {
    let (dir, _) = pscat(
        "reference",
        "/Stage /Reference def",
        &["--dpi", "128", "--png", "{dir}/ref.png"],
    );
    rgb_png(&read(&dir.join("ref.png")))
}

/// The ridge line, per column: scanning down from the top of the sky, the
/// first row whose colour differs from the row 4 px above by more than 60
/// (summed over R, G and B). The sky is a smooth gradient with no
/// clouds, so the first such edge is the rock against it.
fn ridge(img: &Rgb) -> Vec<Option<usize>> {
    (0..img.width)
        .map(|x| {
            (300..1100).find(|&y| {
                let (a, b) = (img.at(x, y - 4), img.at(x, y));
                (0..3).map(|i| a[i].abs_diff(b[i]) as u32).sum::<u32>() > 60
            })
        })
        .collect()
}

/// The centre of the hut's red roof: the strongly red pixels in the
/// region around the hut, as (x, y, count).
fn roof(img: &Rgb) -> (f64, f64, usize) {
    let (mut sx, mut sy, mut n) = (0usize, 0usize, 0usize);
    for y in 820..1000 {
        for x in 100..450 {
            let [r, g, b] = img.at(x, y).map(u32::from);
            if r > 110 && r * 10 > g * 18 && r * 10 > b * 18 {
                sx += x;
                sy += y;
                n += 1;
            }
        }
    }
    assert!(n > 100, "no red roof found ({n} px)");
    (sx as f64 / n as f64, sy as f64 / n as f64, n)
}

/// The program still draws the reference the photograph was made from:
/// the size the model was given, and the same block-in.
#[test]
fn the_reference_is_the_image_models_input_size() {
    let r = reference();
    assert_eq!((r.width, r.height), (1024, 1536));
    let photo = rgb_jpeg(PHOTO);
    assert_eq!(
        (photo.width, photo.height),
        (1024, 1536),
        "the photograph is placed at exactly 128 dpi, no crop"
    );
}

/// The layout fidelity the guide states: along the 1024 columns of the
/// ridge, the photograph's edge is a mean of 1.2 px from the reference's
/// (1 px is 0.56 pt on the poster), 95% of columns within 4 px, none
/// more than 7 px; the hut's red roof moved 6.1 px left and 3.5 px down.
#[test]
fn the_photograph_keeps_the_layout() {
    let r = reference();
    let p = rgb_jpeg(PHOTO);
    let (a, b) = (ridge(&r), ridge(&p));
    let mut d: Vec<usize> = a
        .iter()
        .zip(&b)
        .map(|(a, b)| {
            let (a, b) = (a.expect("reference ridge"), b.expect("photo ridge"));
            a.abs_diff(b)
        })
        .collect();
    d.sort_unstable();
    let mean = d.iter().sum::<usize>() as f64 / d.len() as f64;
    let p95 = d[d.len() * 95 / 100];
    let max = d[d.len() - 1];
    eprintln!("ridge drift: mean {mean:.2} px, p95 {p95} px, max {max} px");
    assert!(
        (1.15..1.25).contains(&mean),
        "mean ridge drift {mean:.2} px, the guide says 1.2"
    );
    assert_eq!((p95, max), (4, 7), "p95 and max ridge drift");

    let (rx, ry, _) = roof(&r);
    let (px, py, _) = roof(&p);
    let (dx, dy) = (px - rx, py - ry);
    eprintln!("roof moved {dx:.1} px across, {dy:.1} px down");
    assert!(
        (-6.6..-5.6).contains(&dx) && (3.0..4.0).contains(&dy),
        "the hut's roof moved ({dx:.1}, {dy:.1}) px, the guide says (-6.1, 3.5)"
    );
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
