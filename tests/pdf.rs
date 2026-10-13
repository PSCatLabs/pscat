//! PDF export — Stage 9 task 2. The strong test rasterizes our PDF
//! *with gs* and block-compares against our own canvas: the document
//! must mean what the raster meant. Skipped without gs, like golden.

use std::process::Command;

mod common;

use common::pdf_text;
use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 500;
const BLOCK: u32 = 10;

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn pdf_and_canvas(src: &[u8]) -> (Vec<u8>, Pixmap) {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    (
        it.gfx().pdf_document().expect("recording on"),
        it.gfx().pixmap.clone(),
    )
}

fn gs_rasterize(pdf: &[u8], name: &str) -> Pixmap {
    let dir = std::env::temp_dir();
    let pdf_path = dir.join(format!("pscat_pdftest_{name}.pdf"));
    let png_path = dir.join(format!("pscat_pdftest_{name}.png"));
    std::fs::write(&pdf_path, pdf).expect("write pdf");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            "-dTextAlphaBits=4",
            &format!("-g{SIZE}x{SIZE}"),
            "-r72",
        ])
        .arg(format!("-o{}", png_path.display()))
        .arg(&pdf_path)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected our PDF");
    let data = std::fs::read(&png_path).expect("read gs png");
    let _ = std::fs::remove_file(&pdf_path);
    let _ = std::fs::remove_file(&png_path);
    Pixmap::decode_png(&data).expect("decode png")
}

fn mean_block_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    let down = |pm: &Pixmap| {
        let (w, h) = (pm.width(), pm.height());
        let (bw, bh) = (w / BLOCK, h / BLOCK);
        let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
        let data = pm.data();
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
                for ch in 0..3 {
                    cell[ch] += f64::from(data[i + ch]);
                }
            }
        }
        let n = f64::from(BLOCK * BLOCK);
        for c in &mut cells {
            for ch in c.iter_mut() {
                *ch /= n;
            }
        }
        cells
    };
    let (da, db) = (down(a), down(b));
    let total: f64 = da
        .iter()
        .zip(&db)
        .map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs()).sum::<f64>())
        .sum();
    total / (da.len() * 3) as f64
}

#[test]
fn postcard_pdf_means_what_the_raster_meant() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = std::fs::read("examples/postcard.ps").expect("example");
    let (pdf, canvas) = pdf_and_canvas(&src);
    let theirs = gs_rasterize(&pdf, "postcard");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "gs render of our PDF diverges: mean {mean:.2}");
}

#[test]
fn specimen_text_survives_as_outlines() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = std::fs::read("examples/specimen.ps").expect("example");
    let (pdf, canvas) = pdf_and_canvas(&src);
    let theirs = gs_rasterize(&pdf, "specimen");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "outline text diverges: mean {mean:.2}");
}

#[test]
fn document_structure_is_sane() {
    let (pdf, _) = pdf_and_canvas(
        b"newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill showpage
          newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath fill showpage",
    );
    let text = pdf_text(&pdf);
    assert!(text.starts_with("%PDF-1.4"));
    assert!(text.contains("/Count 2"), "two pages");
    assert!(text.contains("startxref"));
    assert!(text.ends_with("%%EOF\n"));
}

/// Page content streams are Flate-compressed (issue #244: alpine_lake
/// was 258 MB of plain operators, ~74 MB deflated), and inflate to the
/// `cm` flip followed by the page's operators.
#[test]
fn page_content_is_flate_compressed() {
    let (pdf, _) = pdf_and_canvas(b"1 0 0 setrgbcolor 10 10 50 50 rectfill");
    let raw = String::from_utf8_lossy(&pdf);
    assert!(
        raw.contains("/Length ") && raw.contains("/Filter /FlateDecode >>\nstream\n"),
        "content stream dict declares FlateDecode"
    );
    assert!(
        !raw.contains(" rg "),
        "operators are not stored as plain text"
    );
    let text = pdf_text(&pdf);
    assert!(
        text.contains(&format!("stream\n1 0 0 -1 0 {SIZE} cm\n")),
        "inflated stream starts with the flip: {text}"
    );
    assert!(text.contains("1 0 0 rg"));
}

#[test]
fn images_become_xobjects() {
    let (pdf, _) = pdf_and_canvas(
        b"10 10 translate 80 80 scale
          2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image",
    );
    let text = pdf_text(&pdf);
    assert!(text.contains("/Subtype /Image"), "XObject present");
    assert!(
        text.contains("/Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode"),
        "the image dict itself is Flate-compressed"
    );
    assert!(text.contains("/Im0 Do"));
}

/// Regression guard: `stroke`'s PDF recording used to live inside
/// `if self.svg.is_some()`, so a `--pdf`-only export (the common case
/// -- no `--svg`) silently dropped every stroked line. `fill` and
/// `fill_path_direct` never had this bug; `pdf_and_canvas` here
/// deliberately enables *only* PDF recording, matching `--pdf` alone,
/// to catch a regression back to the svg-gated version.
#[test]
fn strokes_appear_in_pdf_only_export() {
    let (pdf, _) = pdf_and_canvas(
        b"0.7 0.7 0.75 setrgbcolor 2 setlinewidth
          newpath 10 50 moveto 90 50 lineto stroke",
    );
    let text = pdf_text(&pdf);
    assert!(text.contains(" S\n") || text.contains(" S\r"), "{text}");
    assert!(
        text.contains("0.7 0.7 0.75 RG"),
        "stroke color recorded: {text}"
    );
}

/// Issue #20: real PDF shading dictionaries need pattern-colorspace
/// machinery this exporter doesn't have, so `shfill` approximates as a
/// flat fill in the ramp's average color -- deliberately, so a
/// `--pdf`-only export gets *something* rather than the shading
/// silently vanishing (the same failure class `strokes_appear_in_pdf_
/// only_export` above guards, which is exactly why this needs its own
/// regression test rather than trusting the raster/SVG coverage).
#[test]
fn shfill_appears_as_an_average_color_fill_in_pdf_only_export() {
    let (pdf, _) = pdf_and_canvas(
        b"<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0]
             /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill",
    );
    let text = pdf_text(&pdf);
    assert!(text.contains(" f\n") || text.contains(" f\r"), "{text}");
    // Average of a black-to-white ramp lands at mid-gray.
    assert!(text.contains("0.5 0.5 0.5 rg"), "{text}");
}

#[test]
fn imagemasks_are_stencils() {
    let (pdf, _) = pdf_and_canvas(
        b"1 0 0 setrgbcolor 10 40 translate 80 20 scale
          2 1 true [2 0 0 -1 0 1] {<80>} imagemask",
    );
    let text = pdf_text(&pdf);
    assert!(text.contains("/ImageMask true"), "{text}");
    assert!(text.contains("1 0 0 rg"), "painted in current color");
}

/// Issue #8: `%%Title:`/`%%For:` DSC header comments become the PDF's
/// `/Info` metadata, same convention `main.rs` wires via
/// `scan_document_info`. Exercised end to end here (not just the
/// pdf.rs unit tests) so a regression in the Gfx/PdfRecorder wiring
/// itself, not just the scanner, would fail a test.
#[test]
fn dsc_header_comments_become_pdf_info() {
    let src = b"%%Title: Moonlit Poem\n%%For: Jerome\n%%EndComments\n\
                newpath 10 10 moveto 90 10 lineto stroke showpage";
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_pdf();
    let (title, author) = pscat::pdf::scan_document_info(src);
    it.gfx_mut().set_pdf_info(title, author);
    it.run_source(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let pdf = it.gfx().pdf_document().expect("recording on");
    let text = pdf_text(&pdf);
    assert!(text.contains("/Producer (pscat)"));
    assert!(text.contains("/Title (Moonlit Poem)"));
    assert!(text.contains("/Author (Jerome)"));
    assert!(text.contains("/Info "), "trailer references /Info");

    if gs_available() {
        // A broken object/xref layout (e.g. /Info shifting the page or
        // image object numbers) would make gs reject the file outright
        // -- this is a validity smoke test, not a pixel comparison.
        let _ = gs_rasterize(&pdf, "dsc_info");
    } else {
        eprintln!("skipping gs validity check: gs not installed");
    }
}

#[test]
fn no_dsc_comments_still_gets_producer_but_no_title() {
    let (pdf, _) = pdf_and_canvas(b"newpath 10 10 moveto 90 10 lineto stroke showpage");
    let text = pdf_text(&pdf);
    assert!(text.contains("/Producer (pscat)"));
    assert!(!text.contains("/Title"));
    assert!(!text.contains("/Author"));
}

/// Issue #47's alpha/blend export, through the same
/// rasterize-with-gs-and-compare gate every other PDF feature here
/// goes through. This test is also the *answer* to the open question
/// issue #46's spike left for #47 (`docs/WATERCOLOR.md`, "Public
/// contract"): whether PDF-path verification can substitute for the
/// `ghostscript_accepts_*` pattern for alpha-bearing content, given
/// gs has no PostScript-callable alpha operator at all. It can, and
/// it's strictly stronger — `ghostscript_accepts_*` only asserts gs
/// doesn't error, while this asserts gs's own transparency
/// implementation lands on the same pixels tiny-skia did.
#[test]
fn alpha_survives_the_round_trip_through_gs() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    // Three overlapping translucent washes plus a translucent stroke,
    // so both /ca (fill) and /CA (stroke) are exercised.
    let src = b"0.55 setalpha
        0.95 0.85 0.25 setrgbcolor newpath 200 300 120 0 360 arc closepath fill
        0.85 0.20 0.25 setrgbcolor newpath 300 300 120 0 360 arc closepath fill
        0.15 0.35 0.75 setrgbcolor newpath 250 220 120 0 360 arc closepath fill
        0 0 0 setrgbcolor 12 setlinewidth
        newpath 60 60 moveto 440 60 lineto stroke";
    let (pdf, canvas) = pdf_and_canvas(src);
    let text = pdf_text(&pdf);
    assert!(text.contains("/ExtGState"), "alpha needs an ExtGState");
    assert!(text.contains("/ca 0.55"), "fill alpha");
    assert!(text.contains("/CA 0.55"), "stroke alpha");
    let theirs = gs_rasterize(&pdf, "alpha");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "alpha diverges through gs: mean {mean:.2}");
}

#[test]
fn multiply_blending_survives_the_round_trip_through_gs() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = b"/Multiply setblendmode 0.7 setalpha
        0.95 0.85 0.25 setrgbcolor newpath 200 300 120 0 360 arc closepath fill
        0.15 0.35 0.75 setrgbcolor newpath 300 300 120 0 360 arc closepath fill";
    let (pdf, canvas) = pdf_and_canvas(src);
    let text = pdf_text(&pdf);
    assert!(
        text.contains("/BM /Multiply"),
        "blend mode in the ExtGState"
    );
    let theirs = gs_rasterize(&pdf, "multiply");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "Multiply diverges through gs: mean {mean:.2}");
}

/// The other half of the contract: a program that never asks for
/// either one must produce a PDF with no transparency machinery in it
/// at all, so existing output is unchanged rather than merely
/// equivalent.
#[test]
fn ordinary_programs_get_no_extgstate() {
    let (pdf, _) = pdf_and_canvas(
        b"0 0 1 setrgbcolor newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill
          1 0 0 setrgbcolor 4 setlinewidth newpath 10 95 moveto 90 95 lineto stroke",
    );
    let text = pdf_text(&pdf);
    assert!(!text.contains("/ExtGState"), "no alpha, no ExtGState");
    assert!(
        !text.contains(" gs\n") && !text.contains(" gs "),
        "no gs operator"
    );
}

// --- consecutive elements with one clip chain share a q/clip/Q group
// (issue #219, slice S0 of the canvas-surface design) ---

/// Every page content stream's `q`/`Q` operators must pair up: a
/// group left open (or closed twice) is the way this optimization would
/// break a document without any single element looking wrong.
fn assert_q_balanced(pdf: &[u8]) {
    let text = pdf_text(pdf);
    let mut streams = 0;
    for chunk in text.split("stream\n").skip(1) {
        let body = chunk.split("endstream").next().unwrap_or("");
        if !body.starts_with("1 0 0 -1") {
            continue; // an image XObject, not page content
        }
        streams += 1;
        let (mut depth, mut max) = (0i32, 0i32);
        for tok in body.split_whitespace() {
            match tok {
                "q" => {
                    depth += 1;
                    max = max.max(depth);
                }
                "Q" => depth -= 1,
                _ => {}
            }
            assert!(depth >= 0, "Q without a matching q");
        }
        assert_eq!(depth, 0, "page content leaves a q group open");
        assert!(max <= 2, "clip group nests deeper than group + element");
    }
    assert!(streams > 0, "no page content found");
}

/// The shape the real consumers produce: one `clip`, then many
/// `gsave ... grestore` marks. The clip path must be written once, not
/// once per mark, and the file must still pair its q/Q.
#[test]
fn a_run_of_elements_under_one_clip_writes_the_clip_once() {
    let (pdf, _) = pdf_and_canvas(
        b"newpath 250 250 200 0 360 arc closepath clip
          0 1 59 { /i exch def gsave
            i 60 div 0.4 0.7 setrgbcolor
            newpath i 6 mul 60 add 100 moveto 20 0 rlineto 0 20 rlineto closepath fill
            0.1 setgray 3 setlinewidth [4 2] 0 setdash
            newpath i 6 mul 60 add 300 moveto 30 30 rlineto stroke
          grestore } for",
    );
    let text = pdf_text(&pdf);
    assert_eq!(text.matches(" W n ").count(), 1, "clip path repeated");
    assert_eq!(text.matches(" f\n").count(), 60, "all fills kept");
    assert_eq!(text.matches(" S\n").count(), 60, "all strokes kept");
    assert_q_balanced(&pdf);
}

/// A program that never clips gets no outer group at all.
#[test]
fn unclipped_pages_get_no_outer_group() {
    let (pdf, _) = pdf_and_canvas(
        b"1 0 0 setrgbcolor 10 10 50 50 rectfill 0 0 1 setrgbcolor 70 10 50 50 rectfill",
    );
    let text = pdf_text(&pdf);
    assert!(!text.contains(" W n"), "no clip, no W");
    assert!(!text.contains("\nq \n"), "no group");
    assert_q_balanced(&pdf);
}

/// Same raster as the canvas through gs, across everything that has to
/// open, reuse or close a group: two different clips, a nested clip, an
/// unclipped element in between, and per-element state (alpha, dash,
/// line width, colour) that must not leak from one element in a group
/// to the next.
#[test]
fn shared_clip_groups_round_trip_through_gs() {
    if !gs_available() {
        eprintln!("skipping pdf test: gs not installed");
        return;
    }
    let src = b"
        gsave newpath 150 250 120 0 360 arc closepath clip
          0.5 setalpha 0.9 0.2 0.2 setrgbcolor 20 100 250 150 rectfill
          1 setalpha 0.1 0.5 0.9 setrgbcolor 100 220 250 80 rectfill
          12 setlinewidth [10 6] 0 setdash 0 setgray newpath 20 150 moveto 300 150 lineto stroke
          [] 0 setdash 0.2 0.7 0.3 setrgbcolor 3 setlinewidth newpath 20 200 moveto 300 200 lineto stroke
          gsave newpath 150 250 60 0 360 arc closepath clip
            0.95 0.8 0.1 setrgbcolor 0 0 500 500 rectfill
          grestore
          0.3 0.3 0.8 setrgbcolor 100 100 100 30 rectfill
        grestore
        0.8 0.4 0.1 setrgbcolor 400 40 60 60 rectfill
        gsave 300 300 150 150 rectclip
          0.2 0.2 0.2 setrgbcolor newpath 250 250 moveto 500 500 lineto 12 setlinewidth stroke
        grestore
        gsave newpath 150 250 120 0 360 arc closepath clip
          0.6 0.1 0.6 setrgbcolor 130 230 40 40 rectfill
        grestore";
    let (pdf, canvas) = pdf_and_canvas(src);
    assert_q_balanced(&pdf);
    let theirs = gs_rasterize(&pdf, "clipgroups");
    let mean = mean_block_diff(&canvas, &theirs);
    assert!(mean < 6.0, "clip groups diverge through gs: mean {mean:.2}");
}

/// `copypage` keeps painting into the same content after snapshotting
/// it, `showpage` snapshots then starts clean, and `erasepage` throws
/// the stream away: none of them may leave a group open or closed twice,
/// and the page after a snapshot must not inherit its group.
#[test]
fn clip_groups_survive_copypage_showpage_and_erasepage() {
    let (pdf, _) = pdf_and_canvas(
        b"newpath 250 250 200 0 360 arc closepath clip
          1 0 0 setrgbcolor 100 100 100 100 rectfill
          copypage
          0 1 0 setrgbcolor 200 200 100 100 rectfill
          showpage
          newpath 100 100 300 300 rectclip
          0 0 1 setrgbcolor 150 150 100 100 rectfill
          erasepage
          newpath 250 250 100 0 360 arc closepath clip
          0.5 setgray 200 200 50 50 rectfill
          copypage",
    );
    assert_q_balanced(&pdf);
    let text = pdf_text(&pdf);
    // copypage, showpage, the final copypage, and the art painted after
    // it (finish() emits that as a trailing page).
    assert!(text.contains("/Count 4"), "unexpected page count");
}
