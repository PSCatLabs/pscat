//! `nulldevice` (#739): marks inside `gsave nulldevice ... grestore`
//! vanish from the PNG canvas, the SVG and the PDF, and everything after
//! the `grestore` renders exactly as if the block had never been there.

use pscat::Interp;

/// Everything the block should leave untouched.
struct Out {
    png: Vec<u8>,
    svg: Vec<String>,
    pdf: Vec<u8>,
    pages: usize,
}

fn render(src: &str) -> Out {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    Out {
        png: it.gfx().pixmap.data().to_vec(),
        svg: it.gfx().svg_pages().expect("svg"),
        pdf: it.gfx().pdf_document().expect("pdf"),
        pages: it.pages().len(),
    }
}

/// Every kind of marking operator, plus page operators.
const BLOCK: &str = "gsave nulldevice
    1 0 0 setrgbcolor 0 0 100 100 rectfill
    newpath 5 5 moveto 95 95 lineto 8 setlinewidth stroke
    /Helvetica findfont 40 scalefont setfont 10 40 moveto (Hi) show
    0 0 1 setrgbcolor 20 20 60 60 rectfill
    gsave 50 50 translate 30 30 scale /DeviceRGB setcolorspace
      2 2 8 [2 0 0 -2 0 2] { <ff0000 00ff00 0000ff ffff00> } image grestore
    gsave 10 10 translate 40 40 scale
      2 1 true [2 0 0 -1 0 1] { <80> } imagemask grestore
    copypage erasepage showpage
    grestore";

const BEFORE: &str = "0.8 0.9 1 setrgbcolor 0 0 100 100 rectfill 0 0 0 setrgbcolor";
const AFTER: &str =
    "0 0.5 0 setrgbcolor 30 30 40 40 rectfill 1 setlinewidth 10 10 80 80 rectstroke";

fn same(a: &Out, b: &Out, what: &str) {
    assert_eq!(a.png, b.png, "{what}: PNG canvas differs");
    assert_eq!(a.svg, b.svg, "{what}: SVG differs");
    assert_eq!(a.pdf, b.pdf, "{what}: PDF differs");
    assert_eq!(a.pages, b.pages, "{what}: page count differs");
}

#[test]
fn marks_inside_nulldevice_leave_every_output_unchanged() {
    let plain = render(&format!("{BEFORE} {AFTER}"));
    let nulled = render(&format!("{BEFORE} {BLOCK} {AFTER}"));
    same(&plain, &nulled, "block between marks");
}

#[test]
fn nulldevice_pages_emit_nothing_even_after_a_real_showpage() {
    // A showpage first, then the block, then nothing else: the null
    // device's showpage/copypage must add no page, and its marks must
    // not turn the empty canvas after the real showpage into a page.
    let plain = render(&format!("{BEFORE} showpage"));
    let nulled = render(&format!("{BEFORE} showpage {BLOCK}"));
    same(&plain, &nulled, "block after showpage");
    assert_eq!(nulled.pages, 1);
}

#[test]
fn nulldevice_discards_marks_when_nothing_restores_it_too() {
    // No grestore: the rest of the job is on the null device.
    let out = render(&format!("{BEFORE} nulldevice {AFTER} showpage"));
    let plain = render(BEFORE);
    assert_eq!(out.png, plain.png);
    assert_eq!(out.svg, plain.svg);
    assert_eq!(out.pages, plain.pages, "no page is emitted");
}

#[test]
fn ctm_is_identity_on_the_null_device_and_restored_by_grestore() {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(
        "2 3 scale gsave nulldevice matrix currentmatrix
         matrix defaultmatrix /d exch def /c exch def
         100 100 translate matrix initgraphics matrix currentmatrix /i exch def
         grestore matrix currentmatrix /r exch def",
    )
    .expect("runs");
    let eval = |it: &mut Interp, s: &str| {
        it.run_str(s).expect("eval");
    };
    eval(
        &mut it,
        "c 0 get 1 eq c 3 get 1 eq and c 4 get 0 eq and c 5 get 0 eq and /cid exch def",
    );
    eval(&mut it, "d 0 get 1 eq d 3 get 1 eq and /did exch def");
    eval(
        &mut it,
        "i 0 get 1 eq i 3 get 1 eq and i 4 get 0 eq and /iid exch def",
    );
    eval(&mut it, "r 0 get 2 eq r 3 get -3 eq and /rok exch def");
    eval(
        &mut it,
        "cid did iid rok and and and { } { 1 0 div } ifelse",
    );
}

#[test]
fn drawing_state_is_not_lost_inside_nulldevice() {
    // Procedures still run, paths are consumed, and the color set inside
    // does not leak out.
    let plain = render("1 0 0 setrgbcolor 20 20 60 60 rectfill");
    let nulled = render(
        "1 0 0 setrgbcolor gsave nulldevice 0 1 0 setrgbcolor
         newpath 0 0 moveto 50 50 lineto fill grestore
         20 20 60 60 rectfill",
    );
    same(&plain, &nulled, "state restored");
}
