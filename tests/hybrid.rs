//! The hybrid examples in `examples/hybrid/` (pscat composing over an
//! image-model background), and the facts that
//! `docs/guides/pscat-with-image-models.md` states about them. Each poster
//! is checked as a user gets it: rendered by `pscat` to a PNG and to a PDF,
//! the PDF rasterized by hayro (an independent PDF renderer), and the QR
//! code read back by two independent decoders (rqrr, a quirc port; rxing, a
//! ZXing port), from the hybrid and from the "pscat alone" variant
//! (`/PscatOnly true`). The copy each program paints is captured and
//! compared with the brief string for string. The image-model-alone panels
//! are the committed JPEGs that the image model made as whole posters; what
//! the decoders make of their QR codes is pinned here, so the guide's claim
//! is a checked fact, not an opinion.

mod common;
mod qr_support;
use common::CommandDeadline as _;
use qr_support::{Grey, grey_of, raster_pdf, raster_png, read_rxing};
use std::path::{Path, PathBuf};
use std::process::Command;

/// What every QR code on the examples should carry.
const URL: &str = "https://pscatlabs.github.io/pscat/hybrid.html";

struct Example {
    name: &'static str,
    source: &'static str,
    page: &'static str,
    /// The image model's whole-poster attempt, made from the same brief.
    model_alone: &'static str,
    /// The brief: every line of copy the poster must carry, exactly.
    copy: &'static [&'static str],
}

const EXAMPLES: [Example; 3] = [
    Example {
        name: "sky_talk",
        source: "examples/hybrid/sky_talk.ps",
        page: "595x842",
        model_alone: "examples/hybrid/sky_talk_model.jpg",
        copy: &[
            "Why Is the",
            "Sky Blue?",
            "A free public talk on light, air and colour",
            "Dr. Imogen Hartley, atmospheric physicist",
            "Thursday 3 December 2026 · 7:30 pm",
            "Harbourside Library, Lecture Room 2",
            "Scattering relative to red light (700 nm)",
            "400 nm",
            "9.4×",
            "450 nm",
            "5.9×",
            "500 nm",
            "3.8×",
            "550 nm",
            "2.6×",
            "600 nm",
            "1.9×",
            "700 nm",
            "1.0×",
            "Scan: how this poster was made",
        ],
    },
    Example {
        name: "menu_card",
        source: "examples/hybrid/menu_card.ps",
        page: "612x792",
        model_alone: "examples/hybrid/menu_card_model.jpg",
        copy: &[
            "The Lantern Pear",
            "Café · 14 Mill Lane",
            "Breakfast, 8–11",
            "Buttermilk pancakes, maple butter",
            "9.50",
            "Shakshuka with sourdough",
            "11.25",
            "Bircher muesli, poached pear",
            "7.75",
            "Lunch, 12–3",
            "Roast squash and farro salad",
            "12.50",
            "Leek and Gruyère tart",
            "13.00",
            "Soup of the day with bread",
            "8.25",
            "Drinks",
            "Flat white",
            "3.80",
            "Pot of tea",
            "3.20",
            "Pear and ginger soda",
            "4.10",
            "Scan: how this menu was made",
        ],
    },
    Example {
        name: "ferry_timetable",
        source: "examples/hybrid/ferry_timetable.ps",
        page: "595x842",
        model_alone: "examples/hybrid/ferry_model.jpg",
        // The timetable's 40 times are computed by the program; this list
        // is typed out from the brief, so the two are checked against each
        // other.
        copy: &[
            "Harbour Ferry",
            "Winter timetable from Tuesday 1 December 2026",
            "Harbourside Pier",
            "07:15",
            "08:45",
            "10:15",
            "11:45",
            "13:15",
            "14:45",
            "16:15",
            "17:45",
            "Mill Lane Steps",
            "07:22",
            "08:52",
            "10:22",
            "11:52",
            "13:22",
            "14:52",
            "16:22",
            "17:52",
            "Lighthouse Point",
            "07:31",
            "09:01",
            "10:31",
            "12:01",
            "13:31",
            "15:01",
            "16:31",
            "18:01",
            "Gull Island",
            "07:44",
            "09:14",
            "10:44",
            "12:14",
            "13:44",
            "15:14",
            "16:44",
            "18:14",
            "Westbay",
            "07:56",
            "09:26",
            "10:56",
            "12:26",
            "13:56",
            "15:26",
            "16:56",
            "18:26",
            "Single",
            "Return",
            "Adult",
            "3.40",
            "6.20",
            "Child (5–15)",
            "1.70",
            "3.10",
            "Concession",
            "2.30",
            "4.20",
            "Bicycle",
            "1.00",
            "1.80",
            "Children under 5 travel free. Tickets on board, card or cash.",
            "Sailings may be cancelled in high winds; check the board at each pier.",
            "Scan: how this timetable was made",
        ],
    },
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Run pscat from the repository root (the examples read their JPEGs by a
/// relative path) on `prelude` followed by the example's source, with
/// `args` (`{dir}` is the scratch directory). Returns the scratch directory
/// and the raw bytes of standard output.
fn pscat(ex: &Example, tag: &str, prelude: &str, args: &[&str]) -> (PathBuf, Vec<u8>) {
    let dir = common::scratch_dir(&format!("hybrid-{}-{tag}", ex.name));
    let source = std::fs::read_to_string(root().join(ex.source))
        .unwrap_or_else(|e| panic!("read {}: {e}", ex.source));
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_pscat"));
    cmd.current_dir(root()).args(["--page", ex.page]);
    for a in args {
        cmd.arg(a.replace("{dir}", dir.to_str().expect("utf-8 scratch path")));
    }
    cmd.arg("-");
    let out = cmd
        .output_with_input(format!("{prelude}\n{source}").as_bytes())
        .unwrap_or_else(|e| panic!("{} {tag}: run pscat: {e}", ex.name));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success() && !stdout.contains("%%[ Error") && !stderr.contains("%%[ Error"),
        "{} {tag}: pscat {args:?} failed:\n{stdout}{stderr}",
        ex.name
    );
    (dir, out.stdout)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Every symbol rqrr finds, decoded. On a whole poster its finder search
/// can also report a "grid" in the painting (three blob corners), which
/// does not decode; `read_rqrr` insists on exactly one grid, so this reads
/// them all.
fn rqrr_all(g: &Grey) -> Vec<Result<String, String>> {
    let mut image =
        rqrr::PreparedImage::prepare_from_greyscale(g.width, g.height, |x, y| g.at(x, y));
    image
        .detect_grids()
        .iter()
        .map(|grid| {
            let mut payload = Vec::new();
            grid.decode_to(&mut payload)
                .map(|_| String::from_utf8_lossy(&payload).into_owned())
                .map_err(|e| format!("{e:?}"))
        })
        .collect()
}

/// Both decoders must read exactly the URL, and nothing else that decodes.
fn assert_decodes(g: &Grey, ctx: &str) {
    let found = rqrr_all(g);
    let read: Vec<&String> = found.iter().filter_map(|r| r.as_ref().ok()).collect();
    assert_eq!(read, [URL], "{ctx}: rqrr found {found:?}");
    let r = read_rxing(g).unwrap_or_else(|e| panic!("{ctx}: rxing cannot read the QR: {e}"));
    assert_eq!(
        String::from_utf8_lossy(&r.payload),
        URL,
        "{ctx}: rxing read the wrong payload"
    );
}

/// The hybrid and the pscat-alone poster, as a 144 dpi PNG and as a PDF
/// rasterized by hayro at the same scale: both decoders read the URL.
#[test]
fn every_pscat_qr_code_decodes_in_png_and_pdf() {
    for ex in &EXAMPLES {
        for (variant, prelude) in [("hybrid", ""), ("pscat-only", "/PscatOnly true def")] {
            let (dir, _) = pscat(
                ex,
                &format!("{variant}-png"),
                prelude,
                &["--dpi", "144", "--png", "{dir}/out.png"],
            );
            let g = raster_png(&read(&dir.join("out.png")));
            assert_decodes(&g, &format!("{} {variant} PNG", ex.name));

            let (dir, _) = pscat(
                ex,
                &format!("{variant}-pdf"),
                prelude,
                &["--pdf", "{dir}/out.pdf"],
            );
            let g = raster_pdf(&read(&dir.join("out.pdf")), 2.0);
            assert_decodes(&g, &format!("{} {variant} PDF (hayro)", ex.name));
        }
    }
}

/// The text a program paints: `show` and `ashow` (the tracked type) are
/// wrapped to print each string first (the posters' right-aligned and
/// centred helpers call them too).
fn shown_strings(ex: &Example, variant: &str) -> Vec<String> {
    let prelude = format!(
        "{variant}\n/show {{ dup print (\\n) print //show }} bind def\n\
         /ashow {{ dup print (\\n) print //ashow }} bind def"
    );
    let (_, out) = pscat(ex, "copy", &prelude, &["--png", "{dir}/copy.png"]);
    out.split(|&b| b == b'\n').map(latin1).collect()
}

/// The posters' fonts are Latin-1 (ISOLatin1Encoding), and the menu puts an
/// en dash in the unused slot 0x80; read the printed bytes back that way.
fn latin1(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| if b == 0x80 { '\u{2013}' } else { char::from(b) })
        .collect()
}

/// Every line of the brief is painted exactly, for the hybrid and for the
/// pscat-alone variant alike (they share the exact layer).
#[test]
fn the_posters_paint_the_brief_exactly() {
    for ex in &EXAMPLES {
        for variant in ["", "/PscatOnly true def"] {
            let shown = shown_strings(ex, variant);
            for want in ex.copy {
                assert!(
                    shown.iter().any(|s| s == want),
                    "{} {variant:?}: the brief's {want:?} is not painted; painted: {shown:?}",
                    ex.name
                );
            }
        }
    }
}

/// A committed JPEG as the decoders' greyscale.
fn jpeg_grey(path: &str) -> Grey {
    let data = read(&root().join(path));
    let mut dec = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(data));
    dec.decode_headers().expect("JPEG headers");
    let (w, _) = dec.dimensions().expect("JPEG dimensions");
    let rgb = dec.decode().expect("decode JPEG");
    grey_of(
        w,
        rgb.as_chunks::<3>()
            .0
            .iter()
            .map(|p| [p[0], p[1], p[2], 255]),
    )
}

/// The image model's own QR codes, as it drew them in its whole-poster
/// attempts (the committed JPEGs; the PNG originals gave the same result):
/// both decoders find a symbol, and neither can decode it. rqrr locates the
/// finder patterns and fails the error correction; rxing fails its checksum.
/// If a regenerated image ever scans, this test fails and the guide's
/// comparison must be rewritten to match.
#[test]
fn the_image_models_qr_codes_are_found_but_do_not_decode() {
    for ex in &EXAMPLES {
        let g = jpeg_grey(ex.model_alone);
        let found = rqrr_all(&g);
        assert!(
            !found.is_empty(),
            "{}: rqrr finds no symbol at all",
            ex.model_alone
        );
        assert!(
            found.iter().all(Result::is_err),
            "{}: rqrr decoded the image model's QR: {found:?}",
            ex.model_alone
        );
        assert!(
            read_rxing(&g).is_err(),
            "{}: rxing decoded the image model's QR",
            ex.model_alone
        );
    }
}
