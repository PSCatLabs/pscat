//! Transparency groups, `begingroup`/`endgroup` (#1406): the raster against
//! the same marks painted one by one, the PDF form XObject and the SVG
//! `<g>`, and gs rasterising pscat's PDF against pscat's own canvas.
//! `notes/1406-transparency-groups.md` is the design.

use std::process::Command;

mod common;

use common::pdf_text;
use pscat::Interp;
use tiny_skia::Pixmap;

const SIZE: u32 = 100;

fn canvas(src: &str) -> Pixmap {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.gfx().pixmap.clone()
}

/// RGBA at user-space point (x, y) of a 100 x 100 page at 72 dpi.
fn px(pm: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = pm.pixel(x, SIZE - 1 - y).expect("on the page");
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn close(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

fn max_diff(a: &Pixmap, b: &Pixmap) -> u8 {
    a.data()
        .iter()
        .zip(b.data())
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

const GROUND: &str = "0.9 0.8 0.2 setrgbcolor 0 0 100 100 rectfill ";

#[test]
fn one_mark_in_a_group_is_the_mark_at_the_groups_alpha() {
    for (open, per_mark) in [
        ("<< /Alpha 0.4 >>", "0.4 setalpha"),
        ("<< /Blend /Multiply >>", "/Multiply setblendmode"),
        (
            "<< /Alpha 0.6 /Blend /Multiply >>",
            "0.6 setalpha /Multiply setblendmode",
        ),
    ] {
        let grouped = canvas(&format!(
            "{GROUND} {open} begingroup 0.1 0.3 0.9 setrgbcolor 20 20 50 50 rectfill endgroup"
        ));
        let single = canvas(&format!(
            "{GROUND} {per_mark} 0.1 0.3 0.9 setrgbcolor 20 20 50 50 rectfill"
        ));
        let d = max_diff(&grouped, &single);
        assert!(d <= 1, "{open}: grouped and per-mark differ by {d}");
    }
}

#[test]
fn overlapping_marks_in_a_faded_group_are_not_darker_where_they_overlap() {
    let src = format!(
        "{GROUND} << /Alpha 0.5 >> begingroup
           0 0 1 setrgbcolor 10 10 50 50 rectfill
           0 0 1 setrgbcolor 30 30 50 50 rectfill
         endgroup"
    );
    let pm = canvas(&src);
    let once = px(&pm, 15, 15);
    let overlap = px(&pm, 45, 45);
    assert!(close(once, overlap, 0), "{once:?} vs {overlap:?}");
    // Per-mark alpha darkens the overlap: the group is what differs.
    let per = canvas(&format!(
        "{GROUND} 0.5 setalpha 0 0 1 setrgbcolor 10 10 50 50 rectfill 30 30 50 50 rectfill"
    ));
    assert!(!close(px(&per, 15, 15), px(&per, 45, 45), 4));
    assert!(close(px(&per, 15, 15), once, 1));
}

#[test]
fn the_group_alpha_multiplies_currentalpha_and_nests() {
    let want = canvas(&format!(
        "{GROUND} 0.25 setalpha 1 0 0 setrgbcolor 20 20 60 60 rectfill"
    ));
    for open in [
        "0.5 setalpha << /Alpha 0.5 >> begingroup",
        "<< /Alpha 0.5 >> begingroup << /Alpha 0.5 >> begingroup",
        "0.5 setalpha << >> begingroup 0.5 setalpha",
    ] {
        let got = canvas(&format!(
            "{GROUND} {open} 1 0 0 setrgbcolor 20 20 60 60 rectfill endgroup endgroup"
        ));
        let d = max_diff(&got, &want);
        assert!(d <= 2, "{open}: differs from 0.25 by {d}");
    }
}

/// Run `src` and return the operand stack, each object as `==` writes it.
fn stack(src: &str) -> Vec<String> {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

#[test]
fn alpha_and_blend_start_fresh_inside_and_come_back_after() {
    let got = stack(
        "0.25 setalpha /Multiply setblendmode 7 setlinewidth
         << /Alpha 0.5 >> begingroup
           currentalpha currentblendmode 2 setlinewidth
         endgroup currentalpha currentblendmode currentlinewidth",
    );
    assert_eq!(got, ["1.0", "/Normal", "0.25", "/Multiply", "7.0"]);
}

#[test]
fn grestore_and_grestoreall_stop_at_the_group() {
    // Stray grestores inside the group give back the group's own start
    // (alpha 1, gray 0.25) and never pop the level below it, so endgroup
    // finds the gsave level with alpha 0.75, and one more grestore pops it.
    let got = stack(
        "0.25 setgray gsave 0.75 setalpha
         << >> begingroup 0.9 setgray grestore grestore grestoreall
           currentgray currentalpha
         endgroup currentgray currentalpha grestore currentalpha",
    );
    assert_eq!(got, ["0.25", "1.0", "0.25", "0.75", "1.0"]);
}

#[test]
fn a_clip_inside_the_group_ends_with_it_and_the_outer_clip_holds() {
    let pm = canvas(&format!(
        "{GROUND} 0 0 50 100 rectclip
         << /Alpha 0.5 >> begingroup
           0 0 100 50 rectclip 0 0 1 setrgbcolor 0 0 100 100 rectfill
         endgroup
         1 0 0 setrgbcolor 0 60 100 40 rectfill"
    ));
    // Inside both clips: faded blue over the ground.
    assert_ne!(px(&pm, 25, 25), px(&pm, 75, 25));
    // Outside the outer clip: ground, untouched by the group.
    assert!(close(px(&pm, 75, 25), [230, 204, 51, 255], 1));
    // The inner clip ended with the group: red reaches y 60..100 but not x
    // past 50 (the outer clip is still in force).
    assert!(close(px(&pm, 25, 80), [255, 0, 0, 255], 0));
    assert!(close(px(&pm, 75, 80), [230, 204, 51, 255], 1));
}

#[test]
fn an_isolated_group_keeps_multiply_inside_from_reaching_the_page() {
    // Multiply inside an isolated group blends with the group's transparent
    // backdrop, so the mark lands as Normal (PDF /I true, SVG isolation).
    let grouped = canvas(&format!(
        "{GROUND} << >> begingroup /Multiply setblendmode 0 0 1 setrgbcolor 20 20 50 50 rectfill endgroup"
    ));
    let normal = canvas(&format!("{GROUND} 0 0 1 setrgbcolor 20 20 50 50 rectfill"));
    assert!(max_diff(&grouped, &normal) <= 1);
}

#[test]
fn a_group_works_on_a_transparent_page() {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().set_transparent_background();
    it.run_str("<< /Alpha 0.5 >> begingroup 1 0 0 setrgbcolor 10 10 50 50 rectfill endgroup")
        .expect("runs");
    let c = px(&it.gfx().pixmap, 20, 20);
    assert!(close(c, [128, 0, 0, 128], 1), "{c:?}");
    assert_eq!(px(&it.gfx().pixmap, 80, 80), [0, 0, 0, 0]);
}

#[test]
fn an_unclosed_group_ends_at_showpage_and_at_the_end_of_the_program() {
    let faded = canvas(&format!(
        "{GROUND} 0.5 setalpha 0 0 1 setrgbcolor 20 20 50 50 rectfill"
    ));
    // At the end of the program.
    let open = canvas(&format!(
        "{GROUND} << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 20 20 50 50 rectfill"
    ));
    assert!(max_diff(&open, &faded) <= 1);
    // At showpage: the page shown holds the group; a later endgroup has
    // nothing open and does nothing.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(&format!(
        "{GROUND} << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 20 20 50 50 rectfill
         showpage endgroup endgroup"
    ))
    .expect("runs");
    let pages = it.gfx().pages();
    assert_eq!(pages.len(), 1);
    assert!(max_diff(&pages[0], &faded) <= 1);
    // An error ends it too, and keeps its marks.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    let _ = it.run_str(&format!(
        "{GROUND} << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 20 20 50 50 rectfill nosuchop"
    ));
    assert!(max_diff(&it.gfx().pixmap, &faded) <= 1);
}

#[test]
fn a_pending_erase_clears_the_page_not_the_layer() {
    // After showpage the page erases at the first mark; a group opened
    // there must erase the page below it and leave no blank page behind.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(&format!(
        "{GROUND} showpage << >> begingroup endgroup
         << /Alpha 0.5 >> begingroup 1 0 0 setrgbcolor 0 0 10 10 rectfill endgroup"
    ))
    .expect("runs");
    let pm = &it.gfx().pixmap;
    assert_eq!(px(pm, 50, 50), [255, 255, 255, 255], "the page was erased");
    assert!(close(px(pm, 5, 5), [255, 128, 128, 255], 1));
    // An empty group after showpage does not make a page of its own.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(&format!("{GROUND} showpage << >> begingroup endgroup"))
        .expect("runs");
    assert!(!it.gfx().has_trailing_art());
}

#[test]
fn restore_past_a_group_ends_it() {
    let faded = canvas(&format!(
        "{GROUND} 0.5 setalpha 0 0 1 setrgbcolor 20 20 50 50 rectfill"
    ));
    let pm = canvas(&format!(
        "{GROUND} save << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 20 20 50 50 rectfill
         restore endgroup"
    ));
    assert!(max_diff(&pm, &faded) <= 1);
}

#[test]
fn a_group_inside_a_pattern_paintproc_paints_the_tile() {
    // The tile: two overlapping squares in a group at 0.5, so the overlap
    // is no darker. An endgroup in the PaintProc with no group of its own
    // does nothing to the page's group around the fill.
    let pm = canvas(
        "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20]
            /XStep 20 /YStep 20
            /PaintProc { pop << /Alpha 0.5 >> begingroup
                0 0 1 setrgbcolor 0 0 12 12 rectfill 6 6 12 12 rectfill
              endgroup endgroup } >> matrix makepattern
         << >> begingroup setpattern 0 0 100 100 rectfill endgroup",
    );
    let once = px(&pm, 2, 2);
    let overlap = px(&pm, 9, 9);
    assert!(close(once, overlap, 1), "{once:?} vs {overlap:?}");
    assert!(close(once, [128, 128, 255, 255], 2), "{once:?}");
}

#[test]
fn bad_operands_are_refused() {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert_eq!(
        it.run_str("1 begingroup").map_err(|e| e.name()),
        Err("typecheck")
    );
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert_eq!(
        it.run_str("<< /Blend /Screen >> begingroup")
            .map_err(|e| e.name()),
        Err("rangecheck")
    );
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert_eq!(
        it.run_str("<< /Alpha (x) >> begingroup")
            .map_err(|e| e.name()),
        Err("typecheck")
    );
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert_eq!(
        it.run_str("<< >> begingroup << /PageSize [50 50] >> setpagedevice")
            .map_err(|e| e.name()),
        Err("invalidaccess")
    );
    // Depth is capped.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert_eq!(
        it.run_str("100 { << >> begingroup } repeat")
            .map_err(|e| e.name()),
        Err("limitcheck")
    );
    // Unknown keys are ignored; endgroup with nothing open does nothing.
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    assert!(
        it.run_str("endgroup << /Isolated true >> begingroup endgroup endgroup")
            .is_ok()
    );
}

#[test]
fn the_group_is_held_against_the_step_limit() {
    // A 2000 x 2000 page is 4 M pixels, about 4000 steps to make a layer
    // and as many to composite it.
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(pscat::Limits::none().with_max_steps(3000))
        .expect("limits");
    assert_eq!(
        it.run_str("<< >> begingroup endgroup")
            .map_err(|e| e.name()),
        Err("limitexceeded")
    );
}

fn svg_of(src: &str) -> String {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(src).expect("runs");
    it.gfx().svg_pages().expect("svg on").concat()
}

#[test]
fn svg_wraps_the_group_in_one_g() {
    let svg = svg_of(
        "<< /Alpha 0.5 /Blend /Multiply >> begingroup
           0 0 1 setrgbcolor 10 10 50 50 rectfill 30 30 50 50 rectfill
         endgroup",
    );
    assert!(
        svg.contains("<g opacity=\"0.5\" style=\"mix-blend-mode:multiply\"><path"),
        "{svg}"
    );
    assert_eq!(svg.matches("fill-opacity").count(), 0, "{svg}");
    assert_eq!(svg.matches("<path").count(), 2, "{svg}");
    let svg = svg_of("<< >> begingroup 0 0 1 setrgbcolor 10 10 50 50 rectfill endgroup");
    assert!(
        svg.contains("<g style=\"isolation:isolate\"><path"),
        "{svg}"
    );
    // An empty group writes nothing; nested groups nest.
    let svg = svg_of(
        "<< >> begingroup endgroup
         << /Alpha 0.5 >> begingroup << /Alpha 0.5 >> begingroup
           0 0 10 10 rectfill endgroup endgroup",
    );
    assert!(!svg.contains("isolation"), "{svg}");
    assert!(
        svg.contains("<g opacity=\"0.5\"><g opacity=\"0.5\"><path"),
        "{svg}"
    );
}

fn pdf_and_canvas(src: &str, size: u32) -> (Vec<u8>, Pixmap) {
    let mut it = Interp::with_page(size, size).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    (
        it.gfx().pdf_document().expect("recording on"),
        it.gfx().pixmap.clone(),
    )
}

#[test]
fn pdf_writes_an_isolated_group_form() {
    let (pdf, _) = pdf_and_canvas(
        "0.2 0.2 0.2 setrgbcolor 0 0 10 10 rectfill
         << /Alpha 0.5 /Blend /Multiply >> begingroup
           0 0 1 setrgbcolor 10 10 50 50 rectfill 30 30 50 50 rectfill
         endgroup
         << >> begingroup endgroup",
        SIZE,
    );
    let text = pdf_text(&pdf);
    assert!(
        text.contains(
            "/Subtype /Form /BBox [0 0 100 100] /Group << /Type /Group /S /Transparency /I true"
        ),
        "{text}"
    );
    assert!(text.contains("/ca 0.5 /CA 0.5 /BM /Multiply"), "{text}");
    assert!(text.contains("/XObject << /X0 "), "{text}");
    assert!(text.contains(" Do Q"), "{text}");
    // One form only: the empty group wrote nothing.
    assert_eq!(text.matches("/Subtype /Form").count(), 1, "{text}");
    // The marks inside carry no alpha of their own.
    assert_eq!(text.matches("/ca ").count(), 1, "{text}");
}

fn gs_rasterize(pdf: &[u8], name: &str, size: u32) -> Pixmap {
    let dir = common::scratch_dir("pscat_groups");
    let pdf_path = dir.join(format!("{name}.pdf"));
    let png_path = dir.join(format!("{name}.png"));
    std::fs::write(&pdf_path, pdf).expect("write pdf");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            &format!("-g{size}x{size}"),
            "-r72",
        ])
        .arg(format!("-o{}", png_path.display()))
        .arg(&pdf_path)
        .status_deadline()
        .expect("run gs");
    assert!(status.success(), "gs rejected our PDF");
    let data = std::fs::read(&png_path).expect("read gs png");
    Pixmap::decode_png(&data).expect("decode png")
}

use common::CommandDeadline as _;

#[test]
fn gs_renders_the_pdf_groups_as_pscat_rasters_them() {
    if !common::gs_available() {
        return;
    }
    // Rectangles only, on pixel edges, so gs's and pscat's edge rules
    // agree and the comparison is per pixel.
    let src = "0.9 0.8 0.2 setrgbcolor 0 0 200 100 rectfill
        << /Alpha 0.5 >> begingroup
          0 0 1 setrgbcolor 20 20 100 100 rectfill
          1 0 0 setrgbcolor 60 60 100 100 rectfill
        endgroup
        0.6 setalpha << /Blend /Multiply >> begingroup
          0.2 0.6 1 setrgbcolor 140 10 50 180 rectfill
          << /Alpha 0.5 >> begingroup 0 0.5 0 setrgbcolor 150 20 30 160 rectfill endgroup
        endgroup
        1 setalpha gsave 10 150 40 40 rectclip
        << /Alpha 0.7 >> begingroup /Multiply setblendmode
          0 0 0.5 setrgbcolor 0 0 200 200 rectfill
        endgroup grestore
        % A pattern painted inside a group, and a group inside a pattern's
        % PaintProc: the pattern's /Matrix must place it in the form's space.
        << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20]
           /XStep 20 /YStep 20
           /PaintProc { pop << /Alpha 0.5 >> begingroup
             0 0 1 setrgbcolor 0 0 12 12 rectfill 6 6 12 12 rectfill
           endgroup } >> matrix makepattern
        << /Blend /Multiply >> begingroup setpattern 60 140 60 40 rectfill endgroup";
    let (pdf, ours) = pdf_and_canvas(src, 200);
    let theirs = gs_rasterize(&pdf, "groups", 200);
    let mut worst = 0;
    for (a, b) in ours.pixels().iter().zip(theirs.pixels()) {
        for (x, y) in [
            (a.red(), b.red()),
            (a.green(), b.green()),
            (a.blue(), b.blue()),
        ] {
            worst = worst.max(x.abs_diff(y));
        }
    }
    assert!(worst <= 2, "gs and pscat differ by up to {worst}");
}

/// The SVG means what the raster meant: resvg renders the `<g opacity>` /
/// `mix-blend-mode` / `isolation` groups as pscat's layers composite.
#[test]
fn resvg_renders_the_svg_groups_as_pscat_rasters_them() {
    let src = "0.9 0.8 0.2 setrgbcolor 0 0 100 50 rectfill
        << /Alpha 0.5 >> begingroup
          0 0 1 setrgbcolor 10 10 50 50 rectfill
          1 0 0 setrgbcolor 30 30 50 50 rectfill
        endgroup
        << /Blend /Multiply >> begingroup
          0.2 0.6 1 setrgbcolor 70 5 25 90 rectfill
        endgroup
        << >> begingroup /Multiply setblendmode
          0 0.5 0 setrgbcolor 5 80 20 15 rectfill
        endgroup";
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(src).expect("runs");
    let ours = it.gfx().pixmap.clone();
    let svg = it.gfx().svg_pages().expect("svg on").concat();
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &resvg::usvg::Options::default())
        .expect("parse SVG");
    let mut theirs = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).expect("pixmap");
    // The page is written in points (`width="100pt"`): draw it at one
    // device pixel per user unit, as pscat's canvas is.
    let size = tree.size();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            SIZE as f32 / size.width(),
            SIZE as f32 / size.height(),
        ),
        &mut theirs.as_mut(),
    );
    let mut worst = (0, 0);
    for (i, (a, b)) in ours.data().iter().zip(theirs.data()).enumerate() {
        if a.abs_diff(*b) > worst.0 {
            worst = (a.abs_diff(*b), i / 4);
        }
    }
    let (d, at) = worst;
    let (x, y) = (at as u32 % SIZE, at as u32 / SIZE);
    assert!(
        d <= 2,
        "resvg and pscat differ by {d} at device ({x}, {y}): {:?} vs {:?}\n{svg}",
        ours.pixel(x, y),
        theirs.pixel(x, y)
    );
}

/// An error or `quit` inside a `PaintProc`, with a group open there and one
/// on the page: the capture is unwound, both groups end, and the page (not
/// a layer) is what remains.
#[test]
fn an_error_inside_a_paintproc_with_groups_open_leaves_the_page() {
    for stop in ["nosuchop", "quit"] {
        let mut it = Interp::with_page(SIZE, SIZE).expect("page");
        let _ = it.run_str(&format!(
            "{GROUND} << /Alpha 0.5 >> begingroup
             << /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20]
                /XStep 20 /YStep 20
                /PaintProc {{ pop << >> begingroup 0 0 12 12 rectfill {stop} }}
             >> matrix makepattern setpattern 0 0 50 50 rectfill"
        ));
        let pm = &it.gfx().pixmap;
        assert_eq!((pm.width(), pm.height()), (SIZE, SIZE), "{stop}");
        assert!(close(px(pm, 90, 90), [230, 204, 51, 255], 1), "{stop}");
        assert!(close(px(pm, 5, 5), [230, 204, 51, 255], 1), "{stop}");
    }
}

/// Codex round 1, finding 4: a group composites through the 16-bit shadow,
/// as a translucent fill does, so a stack of faint groups reaches the
/// colour the arithmetic gives: 1 - 0.999^1000 = 0.632, level 161, where an
/// 8-bit composite added a quarter of a level and rounded it away (black).
#[test]
fn a_stack_of_faint_groups_reaches_the_arithmetic_colour() {
    let pm = canvas(
        "0 setgray 0 0 100 100 rectfill
         1000 { << /Alpha 0.001 >> begingroup 1 setgray 0 0 100 100 rectfill endgroup } repeat",
    );
    let c = px(&pm, 50, 50);
    assert!(close(c, [161, 161, 161, 255], 1), "{c:?}");
    // The same through per-mark alpha, for comparison: the fill path.
    let per = canvas(
        "0 setgray 0 0 100 100 rectfill 0.001 setalpha 1000 { 1 setgray 0 0 100 100 rectfill } repeat",
    );
    assert!(close(px(&per, 50, 50), c, 1), "{:?}", px(&per, 50, 50));
    // Faint marks inside the group keep their precision into the page too.
    let pm = canvas(
        "0 setgray 0 0 100 100 rectfill
         << /Alpha 0.5 >> begingroup 0.002 setalpha 1000 { 1 setgray 0 0 100 100 rectfill } repeat endgroup",
    );
    // 0.5 x (1 - 0.998^1000) = 0.432, level 110.
    assert!(
        close(px(&pm, 50, 50), [110, 110, 110, 255], 1),
        "{:?}",
        px(&pm, 50, 50)
    );
}

/// Codex round 1, finding 3: a group that ends with no `endgroup` (the end
/// of the program, a page boundary) has its composite charged too. On a
/// 2000 x 2000 page a layer is about 3900 steps to make and as many to
/// composite: under a 5000-step cap the group's own `endgroup` is refused,
/// and so must the same group ended by the end of the program or showpage.
#[test]
fn a_group_ended_implicitly_is_charged() {
    for src in [
        "<< >> begingroup 0 0 10 10 rectfill endgroup",
        "<< >> begingroup 0 0 10 10 rectfill",
        "<< >> begingroup 0 0 10 10 rectfill showpage",
        "save << >> begingroup 0 0 10 10 rectfill restore",
    ] {
        let mut it = Interp::with_page(2000, 2000).expect("page");
        it.set_limits(pscat::Limits::none().with_max_steps(5000))
            .expect("limits");
        assert_eq!(
            it.run_str(src).map_err(|e| e.name()),
            Err("limitexceeded"),
            "{src}"
        );
    }
    // One layer alone fits the cap.
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(pscat::Limits::none().with_max_steps(5000))
        .expect("limits");
    assert!(it.run_str("0 0 10 10 rectfill").is_ok());
}

/// Codex round 1, finding 2: a limit found when a slice starts aborts the
/// program with a group open. The canvas must be the page again (not the
/// group's transparent layer), and the SVG must hold the group closed.
/// The time limit has passed by then, so the group's composite stops at
/// once: the page keeps what it had.
#[test]
fn a_limit_between_slices_ends_the_open_group() {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.gfx_mut().enable_svg();
    it.set_limits(pscat::Limits::none().with_max_time(std::time::Duration::from_millis(300)))
        .expect("limits");
    it.begin_source(
        format!(
            "{GROUND} << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 0 0 50 50 rectfill
             {{ }} loop"
        )
        .as_bytes(),
    );
    // Run until the program is inside the loop, within the time limit.
    for _ in 0..4 {
        assert_eq!(it.step_n(64), Ok(true));
    }
    std::thread::sleep(std::time::Duration::from_millis(400));
    assert_eq!(it.step_n(64).map_err(|e| e.name()), Err("limitexceeded"));
    let pm = &it.gfx().pixmap;
    assert_eq!((pm.width(), pm.height()), (SIZE, SIZE));
    assert!(
        close(px(pm, 75, 75), [230, 204, 51, 255], 1),
        "{:?}",
        px(pm, 75, 75)
    );
    assert_eq!(px(pm, 25, 25)[3], 255, "the page, not the layer");
    let svg = it.gfx().svg_pages().expect("svg on").concat();
    assert!(svg.contains("<g opacity=\"0.5\">"), "{svg}");
}
