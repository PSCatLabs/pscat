//! `--pdf-text` (#1193): `show` text as PDF text objects in embedded subset
//! fonts, so a reader can select, search and extract it. Off by default, and
//! then the PDF is the outline PDF it always was. On, the text must still
//! look like the outlines: gs (and poppler, when installed) rasterize the
//! text PDF and compare it with pscat's canvas and with the outline PDF.
//! `pdftotext` (poppler) reads the text back. The tests that need gs,
//! pdftotext or pdftoppm skip without them, with a `SKIPPED` line.

use std::process::Command;

use pscat::{Interp, PdfText};
use tiny_skia::Pixmap;

mod common;
use common::CommandDeadline as _;
use common::pdf_text;

const SIZE: u32 = 500;
const BLOCK: u32 = 10;

fn render(mode: Option<PdfText>, src: &[u8]) -> (Vec<u8>, Pixmap) {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    if let Some(m) = mode {
        it.set_pdf_text(m);
    }
    it.enable_pdf();
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    (
        it.pdf_document().expect("recording on"),
        it.gfx().pixmap.clone(),
    )
}

fn text_pdf(src: &str) -> Vec<u8> {
    render(Some(PdfText::Text), src.as_bytes()).0
}

/// Every page content stream, inflated.
fn content(pdf: &[u8]) -> String {
    pdf_text(pdf)
}

/// Text objects in the document.
fn text_objects(pdf: &[u8]) -> usize {
    content(pdf).matches(" TJ ET Q").count()
}

/// A page of show-family text: Latin in three standard faces, Hangul
/// (Unicode-mode NotoSansKR), a TeX Gyre face, rotation, a clip, colour,
/// alpha, a blend mode over other text, and every per-glyph spacing
/// operator.
const SAMPLE: &str = "
/Helvetica findfont 24 scalefont setfont
30 450 moveto (Hello, PDF text!) show
/Times-Italic findfont 18 scalefont setfont
30 415 moveto 3 0 (Spaced out) ashow
30 390 moveto 6 0 8#040 (word spacing here) widthshow
30 365 moveto { pop pop 2 0 rmoveto } (kerned by kshow) kshow
/NotoSansKR-Regular findfont 30 scalefont setfont
30 320 moveto (\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{d55c}\u{ad6d}\u{c5b4}) show
/Palatino-Roman findfont 20 scalefont setfont
gsave 300 200 translate 30 rotate 0 0 moveto (Rotated Palatino) show grestore
/Courier findfont 16 scalefont setfont
30 270 moveto (xyshow) [12 0 12 3 12 -3 14 0 12 0 12 0] xyshow
30 245 moveto (xshow) [11 11 13 11 11] xshow
1 0 0 setrgbcolor 30 220 moveto (Red text) show
gsave 30 180 moveto 0 0 1 setrgbcolor
  newpath 30 170 moveto 120 0 rlineto 0 30 rlineto -120 0 rlineto closepath clip
  newpath 30 180 moveto /Helvetica-Bold findfont 28 scalefont setfont (Clipped text) show
grestore
gsave 0.5 setalpha 0 0.5 0 setrgbcolor /Helvetica findfont 40 scalefont setfont
  30 120 moveto (Half) show grestore
gsave /Multiply setblendmode 1 0.6 0 setrgbcolor /Helvetica findfont 40 scalefont setfont
  60 110 moveto (Blend) show grestore
";

#[test]
fn the_default_pdf_has_no_fonts_and_equals_explicit_paths() {
    for file in ["examples/postcard.ps", "examples/specimen.ps"] {
        let src = std::fs::read(file).expect("example");
        let (default, _) = render(None, &src);
        let (paths, _) = render(Some(PdfText::Paths), &src);
        assert_eq!(default, paths, "{file}");
        assert!(!content(&default).contains("/Font"), "{file}");
        assert!(!content(&default).contains(" BT "), "{file}");
    }
    let (default, _) = render(None, SAMPLE.as_bytes());
    assert!(!content(&default).contains(" Tf "));
}

#[test]
fn text_mode_writes_text_objects_in_embedded_subset_fonts() {
    let pdf = text_pdf(SAMPLE);
    let doc = content(&pdf);
    // One Type 0 font per face: Helvetica, Times-Italic, NotoSansKR,
    // Palatino, Courier, Helvetica-Bold.
    assert_eq!(doc.matches("/Subtype /Type0 ").count(), 6, "{doc}");
    assert_eq!(doc.matches("/Subtype /CIDFontType0C ").count(), 6);
    assert_eq!(doc.matches("/Encoding /Identity-H ").count(), 6);
    assert_eq!(doc.matches("/ToUnicode ").count(), 6);
    assert!(doc.contains("+LiberationSans /Encoding"), "{doc}");
    assert!(doc.contains("+NotoSansKR-Regular /Encoding"), "{doc}");
    // The page lists them; Tf is size 1 with the size in Tm, which maps
    // the em to y-down device space (the page's flip `cm` is in force).
    assert!(doc.contains("/Font << /F0 "), "{doc}");
    assert!(doc.contains(" BT /F0 1 Tf 24 0 0 -24 30 50 Tm ["), "{doc}");
    assert!(
        doc.contains(" 17.32051 -10 -10 -17.32051 300 300 Tm "),
        "{doc}"
    );
    // Hangul is in the ToUnicode map (U+C548 is the first syllable).
    assert!(doc.contains("> <C548>"), "{doc}");
    // ashow's extra 3 pt at 18 pt is 166.67/1000 em, as a TJ adjustment.
    assert!(doc.contains("-166.67"), "{doc}");
    // The clipped run sits inside its clip group, the alpha run selects
    // its ExtGState.
    assert!(doc.contains(" W n \nq 0 0 1 rg BT "), "{doc}");
    assert!(doc.contains("q /GS0 gs 0 0.5 0 rg BT "), "{doc}");
}

/// The CID a `/ToUnicode` map gives `ch` (as its UTF-16BE hex).
fn cid_of(doc: &str, utf16: &str) -> String {
    let at = doc
        .find(&format!("> <{utf16}>"))
        .unwrap_or_else(|| panic!("no ToUnicode entry for {utf16}: {doc}"));
    doc[at - 4..at].to_string()
}

/// Codex review, #1193: NotoSansJP draws U+2012 and U+2013 with one glyph.
/// Each character gets its own code (the glyph id, then one past the
/// face's glyph range, with the same outline), so extraction says which.
#[test]
fn a_glyph_shared_by_two_characters_extracts_as_each() {
    let pdf = text_pdf(
        "/NotoSansJP-Regular findfont 20 scalefont setfont 10 50 moveto \
         (\u{2012}\u{2013}\u{2012}) show",
    );
    let doc = content(&pdf);
    let (figure, en) = (cid_of(&doc, "2012"), cid_of(&doc, "2013"));
    assert_ne!(figure, en, "{doc}");
    assert!(
        doc.contains(&format!("[<{figure}{en}{figure}>] TJ")),
        "{doc}"
    );
    // Both codes are in /W, with the same advance.
    let w = doc
        .split("/W [")
        .nth(1)
        .expect("/W")
        .split(" >>")
        .next()
        .expect("/W");
    let n = |cid: &str| u16::from_str_radix(cid, 16).expect("hex").to_string();
    assert!(w.contains(&format!(" {} [", n(&figure))), "{w}");
    assert!(w.contains(&format!(" {} [", n(&en))), "{w}");
}

/// A glyph under a blend mode is its own text object inside its
/// `ExtGState`, as under alpha (the raster blends each glyph on its own).
#[test]
fn blended_glyphs_are_one_text_object_each_in_their_extgstate() {
    let pdf = text_pdf(
        "/Helvetica findfont 30 scalefont setfont 0 0 1 setrgbcolor
         10 50 moveto (Ink) show
         /Multiply setblendmode 1 0 0 setrgbcolor 10 50 moveto (Ink) show",
    );
    let doc = content(&pdf);
    assert!(doc.contains("/BM /Multiply"), "{doc}");
    assert_eq!(doc.matches("q /GS0 gs 1 0 0 rg BT ").count(), 3, "{doc}");
    assert_eq!(doc.matches("q 0 0 1 rg BT ").count(), 1, "{doc}");
}

#[test]
fn ineligible_glyphs_keep_their_outlines() {
    // charpath, a Type 3 font, and stringwidth: no text objects, no fonts.
    let pdf = text_pdf(
        "/Helvetica findfont 30 scalefont setfont
         newpath 20 100 moveto (Path) true charpath fill
         (width) stringwidth pop pop
         /T3 8 dict begin /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def
           /FontBBox [0 0 1000 1000] def /Encoding 256 array def
           0 1 255 { Encoding exch /.notdef put } for Encoding 65 /A put
           /BuildChar { pop 1000 0 0 0 1000 1000 setcachedevice 0 0 1000 1000 rectfill } def
         currentdict end definefont pop
         /T3 findfont 20 scalefont setfont 20 40 moveto (AAA) show",
    );
    let doc = content(&pdf);
    assert_eq!(text_objects(&pdf), 0, "{doc}");
    assert!(!doc.contains("/Font"), "{doc}");
    assert!(doc.contains(" f\nQ"), "the outlines are fills: {doc}");
}

#[test]
fn pages_list_only_the_fonts_they_use() {
    let pdf = text_pdf(
        "/Helvetica findfont 20 scalefont setfont 20 20 moveto (one) show showpage
         /Courier findfont 20 scalefont setfont 20 20 moveto (two) show showpage",
    );
    let doc = content(&pdf);
    assert!(doc.contains("/Resources << /Font << /F0 "), "{doc}");
    assert!(doc.contains("/Resources << /Font << /F1 "), "{doc}");
    assert!(!doc.contains("/F0 << /F1"), "{doc}");
}

#[test]
fn mid_run_reads_show_the_text_so_far_and_the_run_continues() {
    let mut it = Interp::with_page(200, 100).expect("page");
    it.set_pdf_text(PdfText::Text);
    it.enable_pdf();
    it.run_str("/Helvetica findfont 20 scalefont setfont 10 50 moveto (ab) show")
        .expect("runs");
    let early = content(&it.pdf_document().expect("pdf"));
    assert_eq!(early.matches(" TJ ET Q").count(), 1, "{early}");
    it.run_str("(cd) show").expect("runs");
    let late = content(&it.pdf_document().expect("pdf"));
    // The same line: still one text object, now four glyphs.
    assert_eq!(late.matches(" TJ ET Q").count(), 1, "{late}");
    assert!(late.contains("Tm [<0044004500460047>] TJ"), "{late}");
}

#[test]
fn the_cli_flag_needs_pdf_and_writes_fonts() {
    let dir = common::scratch_dir("pscat-pdf-text-cli");
    let out = dir.join("t.pdf");
    let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args([
            "--pdf-text",
            "-e",
            "/Helvetica findfont 12 scalefont setfont 9 9 moveto (x) show",
        ])
        .output_deadline()
        .expect("run pscat");
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("--pdf-text needs --pdf"));
    let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--page", "100x50", "--pdf"])
        .arg(&out)
        .args([
            "--pdf-text",
            "-e",
            "/Helvetica findfont 12 scalefont setfont 9 9 moveto (x) show",
        ])
        .output_deadline()
        .expect("run pscat");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let pdf = std::fs::read(&out).expect("pdf written");
    assert!(content(&pdf).contains("/Subtype /CIDFontType0C "));
}

// ---------------------------------------------------------------------
// External readers
// ---------------------------------------------------------------------

/// Whether a poppler tool runs; the skip is announced like gs's.
fn tool_available(tool: &str) -> bool {
    let ok = Command::new(tool)
        .arg("-v")
        .output_deadline()
        .is_ok_and(|o| o.status.success());
    if !ok {
        let thread = std::thread::current();
        eprintln!(
            "SKIPPED ({tool} not installed): {}",
            thread.name().unwrap_or("(unnamed test)")
        );
    }
    ok
}

#[test]
fn pdftotext_extracts_latin_and_hangul() {
    if !tool_available("pdftotext") {
        return;
    }
    let dir = common::scratch_dir("pscat-pdf-text-extract");
    let path = dir.join("sample.pdf");
    let extract = |pdf: &[u8]| {
        std::fs::write(&path, pdf).expect("write pdf");
        let out = Command::new("pdftotext")
            .args(["-layout"])
            .arg(&path)
            .arg("-")
            .output_deadline()
            .expect("run pdftotext");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    // The re-encoding idiom (a copy of Helvetica with ISOLatin1Encoding,
    // #1022's FACE_KEY path), Japanese and Thai.
    let more = extract(&text_pdf(
        "/Helvetica findfont dup length dict begin
           { 1 index /FID ne { def } { pop pop } ifelse } forall
           /Encoding ISOLatin1Encoding def currentdict end
         /Helvetica-Latin1 exch definefont 20 scalefont setfont
         20 150 moveto (caf\\351 na\\357ve) show
         /NotoSansJP-Regular findfont 20 scalefont setfont
         20 100 moveto (\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}) show
         /NotoSansThai-Regular findfont 20 scalefont setfont
         20 50 moveto (\u{e20}\u{e32}\u{e29}\u{e32}\u{e44}\u{e17}\u{e22}) show
         /NotoSansJP-Regular findfont 20 scalefont setfont
         20 20 moveto (A\u{2012}B\u{2013}C) show",
    ));
    for want in [
        "caf\u{e9} na\u{ef}ve",
        "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}",
        "\u{e20}\u{e32}\u{e29}\u{e32}\u{e44}\u{e17}\u{e22}",
        "A\u{2012}B\u{2013}C",
    ] {
        assert!(more.contains(want), "{want:?} not in {more}");
    }
    let text = extract(&text_pdf(SAMPLE));
    for want in [
        "Hello, PDF text!",
        "word spacing here",
        "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{d55c}\u{ad6d}\u{c5b4}",
        "Red text",
        "Clipped text",
        "Half",
    ] {
        assert!(text.contains(want), "{want:?} not in {text}");
    }
    // ashow/kshow spacing reads as letters, not lost glyphs.
    let squeezed: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(squeezed.contains("Spacedout"), "{text}");
    assert!(squeezed.contains("kernedbykshow"), "{text}");
    // The outline PDF has no text at all.
    assert!(
        extract(&render(None, SAMPLE.as_bytes()).0)
            .trim()
            .is_empty()
    );
}

fn mean_block_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    assert_eq!((a.width(), a.height()), (b.width(), b.height()));
    let down = |pm: &Pixmap| {
        let (w, h) = (pm.width(), pm.height());
        let bw = w / BLOCK;
        let mut cells = vec![[0f64; 3]; (bw * (h / BLOCK)) as usize];
        let data = pm.data();
        for y in 0..h / BLOCK * BLOCK {
            for x in 0..bw * BLOCK {
                let i = ((y * w + x) * 4) as usize;
                let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
                for ch in 0..3 {
                    cell[ch] += f64::from(data[i + ch]);
                }
            }
        }
        cells
    };
    let n = f64::from(BLOCK * BLOCK);
    let (da, db) = (down(a), down(b));
    let total: f64 = da
        .iter()
        .zip(&db)
        .map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs() / n).sum::<f64>())
        .sum();
    total / (da.len() * 3) as f64
}

/// Rasterize `pdf` at 72 dpi with gs or pdftoppm.
fn rasterize(tool: &str, pdf: &[u8], name: &str) -> Pixmap {
    let dir = common::scratch_dir("pscat-pdf-text-raster");
    let pdf_path = dir.join(format!("{name}.pdf"));
    std::fs::write(&pdf_path, pdf).expect("write pdf");
    let png = match tool {
        "gs" => {
            let png = dir.join(format!("{name}.png"));
            let out = Command::new("gs")
                .args([
                    "-q",
                    "-dNOPAUSE",
                    "-dBATCH",
                    "-sDEVICE=png16m",
                    "-dGraphicsAlphaBits=4",
                    "-dTextAlphaBits=4",
                    "-dPDFSTOPONERROR",
                    &format!("-g{SIZE}x{SIZE}"),
                    "-r72",
                ])
                .arg(format!("-o{}", png.display()))
                .arg(&pdf_path)
                .output_deadline()
                .expect("run gs");
            // gs repairs a broken font quietly and exits 0; with -q it says
            // nothing at all about a sound one.
            assert!(
                out.status.success() && out.stdout.is_empty() && out.stderr.is_empty(),
                "gs: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            png
        }
        _ => {
            let stem = dir.join(name);
            let out = Command::new("pdftoppm")
                .args(["-r", "72", "-png", "-singlefile"])
                .arg(&pdf_path)
                .arg(&stem)
                .output_deadline()
                .expect("run pdftoppm");
            assert!(
                out.status.success() && out.stderr.is_empty(),
                "pdftoppm: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            stem.with_extension("png")
        }
    };
    let data = std::fs::read(&png).expect("read png");
    Pixmap::decode_png(&data).expect("decode png")
}

/// The text PDF, rasterized by `tool`, against pscat's canvas and against
/// the same tool's raster of the outline PDF. 3.0 is the mean block
/// difference `tests/pdf.rs` allows its PDF round trips.
fn assert_renders_like_outlines(tool: &str) {
    let (outline_pdf, canvas) = render(None, SAMPLE.as_bytes());
    let (text, canvas2) = render(Some(PdfText::Text), SAMPLE.as_bytes());
    assert_eq!(canvas.data(), canvas2.data(), "the raster never changes");
    let theirs_text = rasterize(tool, &text, &format!("{tool}-text"));
    let theirs_outline = rasterize(tool, &outline_pdf, &format!("{tool}-outline"));
    let vs_canvas = mean_block_diff(&canvas, &theirs_text);
    let vs_outline = mean_block_diff(&theirs_outline, &theirs_text);
    eprintln!("{tool}: text PDF vs canvas {vs_canvas:.3}, vs outline PDF {vs_outline:.3}");
    assert!(vs_canvas < 3.0, "{tool}: text PDF vs canvas {vs_canvas:.2}");
    assert!(
        vs_outline < 3.0,
        "{tool}: text PDF vs outline PDF {vs_outline:.2}"
    );
}

#[test]
fn gs_renders_the_text_pdf_like_the_outlines() {
    if !common::gs_available() {
        return;
    }
    assert_renders_like_outlines("gs");
}

#[test]
fn pdftoppm_renders_the_text_pdf_like_the_outlines() {
    if !tool_available("pdftoppm") {
        return;
    }
    assert_renders_like_outlines("pdftoppm");
}
