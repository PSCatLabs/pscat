//! Behavioral contracts of lib/warp.ps: the point warps, path transforms,
//! and running unmodified programs through decorated painting operators.
//! Guard errors live alongside their guards in %%SelfTest blocks.

use pscat::Interp;

const FUNCTIONAL: &str = include_str!("../lib/functional.ps");
const ASPECT: &str = include_str!("../lib/aspect.ps");
const WARP: &str = include_str!("../lib/warp.ps");

fn interpreter(width: u32, height: u32) -> Interp {
    let mut it = Interp::with_page(width, height).expect("test page");
    it.capture_stdout();
    for lib in [FUNCTIONAL, ASPECT, WARP] {
        it.run_str(lib).expect("library");
    }
    it
}

fn run(src: &str) -> Interp {
    let mut it = interpreter(100, 100);
    it.run_str(src)
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    assert_eq!(it.dict_stack_len(), 2, "dictionary leaked: {src}");
    it
}

/// Numbers left on the operand stack, rounded to 3 decimals.
fn numbers(src: &str) -> Vec<f64> {
    run(src)
        .operand_stack()
        .iter()
        .map(|o| {
            let v: f64 = o
                .repr()
                .parse()
                .unwrap_or_else(|_| panic!("{src}: {}", o.repr()));
            (v * 1000.0).round() / 1000.0
        })
        .collect()
}

#[test]
fn point_warps_move_points_where_their_formulas_say() {
    assert_eq!(numbers("3 4 10 20 wp-shift exec"), [13.0, 24.0]);
    assert_eq!(numbers("3 4 2 0.5 wp-scale exec"), [6.0, 2.0]);
    // a quarter wavelength along x is the crest of the sine
    assert_eq!(numbers("25 0 8 100 wp-ripple exec"), [25.0, 8.0]);
    // lens: d = 10 inside r = 50, factor 1 + 0.4 (1 - 0.04)
    assert_eq!(
        numbers("110 100 100 100 50 0.4 wp-lens exec"),
        [113.84, 100.0]
    );
    // outside the lens and the swirl, points do not move
    assert_eq!(
        numbers("200 100 100 100 50 0.4 wp-lens exec"),
        [200.0, 100.0]
    );
    assert_eq!(
        numbers("200 100 100 100 50 90 wp-swirl exec"),
        [200.0, 100.0]
    );
    // polar: x = 0 is twelve o'clock at radius r0 + y
    assert_eq!(numbers("0 10 0 0 100 0.5 wp-polar exec"), [0.0, 110.0]);
    // perspective sends the rectangle's corners to the quadrilateral's
    assert_eq!(
        numbers(
            "/p 0 0 100 0 90 80 10 80 100 100 wp-perspective def \
             0 0 p 100 0 p 100 100 p 0 100 p"
        ),
        [0.0, 0.0, 100.0, 0.0, 90.0, 80.0, 10.0, 80.0]
    );
    // and, unlike any CTM, does not keep the centre at the centre
    assert_eq!(
        numbers("50 50 0 0 100 0 90 80 10 80 100 100 wp-perspective exec"),
        [50.0, 44.444]
    );
}

#[test]
fn paths_round_trip_as_records_and_subdivide_every_edge() {
    let it = run("newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath \
                  wp-path dup 4 wp-subdivide length exch length");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    // 4 records; subdivided: moveto, 3 + 3 + 4 steps (the closing
    // diagonal is about 14.1 long), and the closepath record kept
    assert_eq!(stack, ["12", "4"]);
    let it = run("newpath 0 0 moveto 10 0 lineto wp-path dup wp-emit wp-path eq");
    assert_eq!(
        it.operand_stack()[0].repr(),
        "false",
        "fresh records each read"
    );
}

#[test]
fn unroll_keeps_a_shape_that_straddles_twelve_o_clock_whole() {
    // A short segment just left of twelve o'clock to just right of it: its
    // angles are about 357 and 3 degrees, so a naive map tears it apart.
    let xs = numbers(
        "[[/m -0.5 10] [/l 0.5 10]] 0 0 1 wp-unroll exec \
         dup 0 get 1 get exch 1 get 1 get",
    );
    assert!((xs[1] - xs[0]).abs() < 10.0, "torn across the seam: {xs:?}");
}

#[test]
fn bound_programs_draw_through_decorators_installed_before_they_load() {
    let it = run("/n << /Calls 0 >> def \
         { /tri { newpath 0 0 moveto 10 0 lineto 0 10 lineto closepath fill } bind def \
           tri tri } \
         << /fill { n aop-count } >> wp-run-decorated \
         n /Calls get systemdict /fill get type");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["2", "operatortype"], "systemdict untouched");
}

#[test]
fn the_scope_suppresses_showpage_and_rebalances_after_errors() {
    run("{ showpage copypage erasepage } << >> wp-run-decorated");
    // a program that leaves dictionaries open, then fails
    let it = run(
        "{ { 5 dict begin 5 dict begin 1 0 div } << >> wp-run-decorated } stopped \
         countdictstack $error /errorname get",
    );
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["true", "2", "/undefinedresult"]);
    // a program that only leaves dictionaries open
    run("{ 5 dict begin } << >> wp-run-decorated");
}

/// Mean RGB of each `block`-pixel square, for comparisons that ignore
/// antialiasing differences along edges.
fn block_means(it: &Interp, block: usize) -> Vec<f64> {
    let pixmap = &it.gfx().pixmap;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let data = pixmap.data();
    let mut out = Vec::new();
    for by in (0..h).step_by(block) {
        for bx in (0..w).step_by(block) {
            let mut sum = 0u64;
            for y in by..(by + block).min(h) {
                for x in bx..(bx + block).min(w) {
                    let i = (y * w + x) * 4;
                    sum += u64::from(data[i]) + u64::from(data[i + 1]) + u64::from(data[i + 2]);
                }
            }
            out.push(sum as f64 / (block * block * 3) as f64);
        }
    }
    out
}

#[test]
fn an_identity_warp_redraws_an_unmodified_gallery_piece_faithfully() {
    let mut plain = Interp::with_page(640, 640).expect("page");
    plain
        .run_str("(gallery/woven_labyrinth.ps) run")
        .expect("plain render");
    let mut warped = interpreter(640, 640);
    warped
        .run_str("(gallery/woven_labyrinth.ps) {} 3 wp-run")
        .unwrap_or_else(|e| panic!("{}", warped.error_report(&e)));
    assert_eq!(warped.dict_stack_len(), 2);
    let (a, b) = (block_means(&plain, 10), block_means(&warped, 10));
    let worst = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    assert!(worst < 4.0, "identity warp changed the picture: {worst}");

    // and a real warp does change it
    let mut lensed = interpreter(640, 640);
    lensed
        .run_str("(gallery/woven_labyrinth.ps) 320 330 235 0.45 wp-lens 2 wp-run")
        .expect("lens render");
    let c = block_means(&lensed, 10);
    let moved = a
        .iter()
        .zip(&c)
        .filter(|(x, y)| (*x - *y).abs() > 20.0)
        .count();
    assert!(moved > 200, "the lens moved only {moved} blocks");
}

/// Worst per-block difference of mean RGB between two renders.
fn worst_block_diff(a: &Interp, b: &Interp, block: usize) -> f64 {
    block_means(a, block)
        .iter()
        .zip(&block_means(b, block))
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

/// Render `src` on a 400 x 400 page with the libraries loaded, user space
/// scaled 4x so a 100-unit drawing fills it.
fn render(src: &str) -> Interp {
    let mut it = interpreter(400, 400);
    it.run_str(&format!("4 4 scale {src}"))
        .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    assert_eq!(it.dict_stack_len(), 2, "dictionary leaked: {src}");
    it
}

/// Grey level (0 black, 255 white) at user point x y of a `render`.
fn grey_at(it: &Interp, x: f64, y: f64) -> u8 {
    let pixmap = &it.gfx().pixmap;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let (px, py) = ((x * 4.0) as usize, h - 1 - (y * 4.0) as usize);
    let i = (py * w + px) * 4;
    let d = pixmap.data();
    ((u32::from(d[i]) + u32::from(d[i + 1]) + u32::from(d[i + 2])) / 3) as u8
}

// A triangle whose last edge runs along y = 20 from x = 20 to x = 80. The
// ripple lifts that edge into a wave (y = 25 at x = 50), so the straight
// chord between its warped ends (y = 20 at x = 50) is the visible bug.
const TRIANGLE: &str = "newpath 80 20 moveto 50 80 lineto 20 20 lineto";
const RIPPLE: &str = "10 120 wp-ripple";

#[test]
fn a_closepath_record_follows_the_samples_of_its_closing_edge() {
    let it = run("newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath \
                  wp-path 4 wp-subdivide dup dup length 1 sub get 0 get \
                  exch dup length 2 sub get aload pop");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["/z", "/l", "0.0", "0.0"]);

    // so a closed shape bends along its closing edge exactly as if that
    // edge had been drawn with lineto
    let closed = render(&format!("{TRIANGLE} closepath {RIPPLE} 2 wp-warppath fill"));
    let explicit = render(&format!(
        "{TRIANGLE} 80 20 lineto closepath {RIPPLE} 2 wp-warppath fill"
    ));
    let worst = worst_block_diff(&closed, &explicit, 8);
    assert!(
        worst < 4.0,
        "closing edge drawn from the wrong point: {worst}"
    );
    assert!(
        grey_at(&closed, 50.0, 22.0) > 200,
        "a straight chord was filled"
    );
    assert!(grey_at(&closed, 50.0, 28.0) < 60, "the wave was not filled");
}

#[test]
fn fill_and_clip_bend_the_implicit_closing_edge_of_an_open_path() {
    let explicit = render(&format!(
        "{{ {TRIANGLE} 80 20 lineto closepath fill }} {RIPPLE} 2 wp-run"
    ));
    for op in ["fill", "eofill"] {
        let open = render(&format!("{{ {TRIANGLE} {op} }} {RIPPLE} 2 wp-run"));
        let worst = worst_block_diff(&open, &explicit, 8);
        assert!(worst < 4.0, "{op}: implicit closing edge not bent: {worst}");
        assert!(grey_at(&open, 50.0, 22.0) > 200, "{op}: straight chord");
    }
    // clipping to the open triangle, then painting the whole page
    let page = "newpath -50 -50 moveto 150 -50 lineto 150 150 lineto -50 150 lineto fill";
    for op in ["clip", "eoclip"] {
        let open = render(&format!("{{ {TRIANGLE} {op} {page} }} {RIPPLE} 2 wp-run"));
        let worst = worst_block_diff(&open, &explicit, 8);
        assert!(worst < 4.0, "{op}: implicit closing edge not bent: {worst}");
    }
}

#[test]
fn clip_keeps_the_unwarped_path_as_the_real_clip_does() {
    // the path survives clip unwarped, so the fill after it is warped once
    let square = "newpath 20 20 moveto 40 0 rlineto 0 40 rlineto -40 0 rlineto closepath";
    let native = render(&format!("30 0 translate {square} clip fill"));
    for op in ["clip", "eoclip"] {
        let warped = render(&format!("{{ {square} {op} fill }} 30 0 wp-shift 2 wp-run"));
        let worst = worst_block_diff(&warped, &native, 8);
        assert!(worst < 4.0, "{op} left a warped path behind: {worst}");
    }
    // and keeps its curves: one moveto and four curvetos for a circle
    let it = run("{ newpath 50 50 20 0 360 arc clip \
                  0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 100 add } {} \
                  pathforall } {} 2 wp-run");
    assert_eq!(it.operand_stack()[0].repr(), "401");
}

#[test]
fn show_keeps_the_current_path_as_the_real_show_does() {
    // a line, a label, then the line stroked: the label must not erase it
    let program = "/Helvetica findfont 20 scalefont setfont \
                   newpath 10 10 moveto 90 10 lineto 30 50 moveto (A) show \
                   4 setlinewidth stroke";
    let native = render(program);
    let warped = render(&format!("{{ {program} }} {{}} 2 wp-run"));
    let worst = worst_block_diff(&warped, &native, 8);
    assert!(worst < 4.0, "identity warp changed line-then-show: {worst}");
    assert!(grey_at(&warped, 50.0, 10.0) < 60, "the line was lost");
    // the current point advances as it would
    assert_eq!(
        numbers(
            "/Helvetica findfont 20 scalefont setfont \
                 { newpath 30 50 moveto (A) show currentpoint } {} 2 wp-run \
                 newpath 30 50 moveto (A) show currentpoint"
        ),
        numbers(
            "/Helvetica findfont 20 scalefont setfont \
                 newpath 30 50 moveto (A) show currentpoint \
                 newpath 30 50 moveto (A) show currentpoint"
        ),
    );
}

#[test]
fn a_decorated_program_can_take_operands_and_leave_results() {
    assert_eq!(numbers("{ 42 } << >> wp-run-decorated"), [42.0]);
    assert_eq!(numbers("5 { 2 mul } << >> wp-run-decorated"), [10.0]);
    assert!(numbers("1 2 { clear } << >> wp-run-decorated").is_empty());
    // an error is still re-raised after the scope is closed
    let it = run(
        "{ { 7 5 dict begin 1 0 div } << >> wp-run-decorated } stopped \
                  countdictstack $error /errorname get",
    );
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack.last().map(String::as_str), Some("/undefinedresult"));
    assert_eq!(&stack[stack.len() - 3..stack.len() - 1], ["true", "2"]);
}

#[test]
fn a_decorator_that_fails_to_install_still_closes_the_scope() {
    // Codex round 2 on #962: installing happened before `stopped`, so an
    // error there left the scope dictionary (and the wrappers in it) open.
    // (the operands the failed install was working on stay behind, as after
    // any PostScript error; what matters is the dictionary depth)
    let it = run("{ {} << /fill { pop 1 } >> wp-run-decorated } stopped countdictstack");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(&stack[stack.len() - 2..], ["true", "2"], "{stack:?}");
    // and fill is the real one again afterwards
    assert_eq!(
        numbers("/fill load type /operatortype eq { 1 } { 0 } ifelse"),
        [1.0]
    );
}

#[test]
fn decorating_exec_or_stopped_cannot_stop_the_program_or_the_cleanup() {
    // Codex round 2 on #962: the runner loaded exec and stopped at run time,
    // after the decorators were installed, so it picked up the wrappers.
    assert_eq!(
        numbers("{ 42 } << /exec { } >> wp-run-decorated countdictstack"),
        [42.0, 2.0]
    );
    assert_eq!(
        numbers("{ 42 } << /stopped { } >> wp-run-decorated countdictstack"),
        [42.0, 2.0]
    );
    // an error inside still re-raises, with the scope closed
    let it =
        run("{ { 1 0 div } << /exec { } /stopped { } >> wp-run-decorated } stopped countdictstack");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["true", "2"], "{stack:?}");
}

/// The show-family programs the decorators must draw through the warp
/// (#1172): a line, the text, then the line stroked, so a decorator that
/// lost the caller's path would show as a missing line.
const SHOW_FAMILY: [(&str, &str); 9] = [
    ("ashow", "2 1 (AVA) ashow"),
    ("widthshow", "6 0 32 (A V) widthshow"),
    ("awidthshow", "6 2 32 1 0 (A V) awidthshow"),
    ("kshow", "{ pop pop -3 0 rmoveto } (AVA) kshow"),
    ("glyphshow", "/eacute glyphshow /Aring glyphshow"),
    // #1191: the glyphs go where the numbers put them
    ("xshow", "(ABC) [20 22 24] xshow"),
    ("yshow", "(ABC) [8 -6 10] yshow"),
    ("xyshow", "(ABC) [20 4 22 -3 24 6] xyshow"),
    // a number string: IEEE binary32, big-endian, 6 numbers: 20 4 22 -3 24 6
    (
        "xyshow (encoded)",
        "(ABC) (\\225\\060\\000\\006\\101\\240\\000\\000\\100\\200\\000\\000\\101\\260\\000\\000\\300\\100\\000\\000\\101\\300\\000\\000\\100\\300\\000\\000) xyshow",
    ),
];

fn show_program(text: &str) -> String {
    format!(
        "/Helvetica findfont 24 scalefont setfont \
         newpath 10 10 moveto 90 10 lineto 15 50 moveto {text} \
         4 setlinewidth stroke"
    )
}

#[test]
fn the_rest_of_the_show_family_draws_through_the_warp() {
    for (op, text) in SHOW_FAMILY {
        let program = show_program(text);
        let native = render(&program);
        // an identity warp redraws it (block means: the warp flattens and
        // subdivides the outlines, so the edges are not byte-identical)
        let warped = render(&format!("{{ {program} }} {{}} 2 wp-run"));
        let worst = worst_block_diff(&warped, &native, 8);
        assert!(worst < 4.0, "{op}: identity warp changed the text: {worst}");
        assert!(grey_at(&warped, 50.0, 10.0) < 60, "{op}: the line was lost");
        // a shift moves the text with everything else, so it went through
        // the warp rather than being drawn flat
        let shifted = render(&format!("{{ {program} }} 20 5 wp-shift 2 wp-run"));
        let moved = render(&format!("20 5 translate {program}"));
        let worst = worst_block_diff(&shifted, &moved, 8);
        assert!(
            worst < 4.0,
            "{op}: the text did not follow the warp: {worst}"
        );
    }
}

/// What a program left on the operand stack, as text.
fn stack_text(src: &str) -> Vec<String> {
    run(src).operand_stack().iter().map(|o| o.repr()).collect()
}

/// `body` run plain and then through an identity warp leave the same stack.
fn assert_same_as_native(body: &str) {
    let native = stack_text(body);
    let warped = stack_text(&format!("{{ {body} }} {{}} 2 wp-run"));
    assert_eq!(warped, native, "{body}");
}

#[test]
fn the_show_family_leaves_the_current_point_and_stacks_as_the_real_operators_do() {
    // (run checks that no dictionary is left open after the scope)
    for (_, text) in SHOW_FAMILY {
        assert_same_as_native(&format!(
            "/Helvetica findfont 24 scalefont setfont 7 newpath 15 50 moveto {text} \
             currentpoint"
        ));
    }
    // kshow's procedure sees the codes, the current point after each glyph,
    // the caller's operands below them and the caller's dictionaries
    assert_same_as_native(
        "/Helvetica findfont 24 scalefont setfont /d0 countdictstack def \
         99 newpath 15 50 moveto \
         { 2 array astore countdictstack d0 sub currentpoint 3 0 rmoveto } (ABC) kshow \
         currentpoint",
    );
    // a font set by the procedure is the font of the glyphs after it, and
    // stays set; here it switches from byte codes to UTF-8 mid-string
    assert_same_as_native(
        "/Helvetica findfont 24 scalefont setfont newpath 15 50 moveto \
         { 2 array astore /NotoSansKR-Regular findfont 20 scalefont setfont } \
         (A\u{c548}\u{c548}) kshow currentpoint currentfont /FontName get",
    );
    // an exit from the procedure is refused, as the real kshow refuses it
    assert_same_as_native(
        "/Helvetica findfont 24 scalefont setfont newpath 15 50 moveto \
         { { pop pop exit } (ABC) kshow } stopped $error /errorname get currentpoint",
    );
    // glyphshow leaves the caller's font, Encoding and all
    assert_same_as_native(
        "/Helvetica findfont 24 scalefont setfont newpath 15 50 moveto /eacute glyphshow \
         currentfont /Encoding get 233 get currentfont /FontName get",
    );
}

#[test]
fn the_show_family_cuts_text_into_glyphs_as_the_interpreter_does() {
    // kshow reports every glyph's code, so it shows the cut: one byte per
    // glyph in a byte font, one UTF-8 sequence in a Unicode-mode font, and
    // an ill-formed sequence as U+FFFD glyphs of its longest valid prefix
    let texts = [
        "\u{c548} \u{b155}A",
        // a surrogate, a truncated three-byte sequence, two lone lead bytes
        "\\355\\240\\200A\\341\\200B\\303\\303",
        // a four-byte scalar, one past U+10FFFF, an overlong slash
        "\\360\\237\\230\\200\\364\\220\\200\\200\\300\\257",
    ];
    for font in ["NotoSansKR-Regular", "Helvetica"] {
        for text in texts {
            let setup = format!("/{font} findfont 20 scalefont setfont newpath 10 10 moveto");
            assert_same_as_native(&format!(
                "{setup} [ {{ 2 array astore currentpoint 3 array astore }} ({text}) kshow \
                 currentpoint ]"
            ));
            assert_same_as_native(&format!("{setup} 2 1 ({text}) ashow currentpoint"));
            assert_same_as_native(&format!("{setup} 5 0 32 ({text}) widthshow currentpoint"));
        }
    }
    // a Type 3 font's glyph procedure runs once per glyph, as without the
    // warp: the Unicode-mode check reads /UnicodeBuildChar instead of
    // showing a probe string
    assert_same_as_native(
        "/n 0 def /T3 << /FontType 3 /FontMatrix [0.001 0 0 0.001 0 0] \
         /FontBBox [0 0 1000 1000] /Encoding 256 array \
         /BuildChar { /n n 1 add def pop pop 600 0 setcharwidth } >> definefont \
         20 scalefont setfont newpath 10 10 moveto 1 0 (\\303\\251) ashow n currentpoint",
    );
}

const MARQUEE: &str = include_str!("../lib/fonts/marquee.ps");

/// `render`, with the bundled Type 3 face Marquee loaded and seeded.
fn render_marquee(src: &str) -> Interp {
    let mut it = interpreter(400, 400);
    it.run_str(MARQUEE).expect("Marquee");
    it.run_str(&format!(
        "4 4 scale 2 srand /Marquee findfont 24 scalefont setfont {src}"
    ))
    .unwrap_or_else(|e| panic!("{src}: {}", it.error_report(&e)));
    assert_eq!(it.dict_stack_len(), 2, "dictionary leaked: {src}");
    it
}

/// Pixels that are not white.
fn ink(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0].min(p[1]).min(p[2]) < 200)
        .count()
}

#[test]
fn type3_text_bends_under_every_show_operator() {
    // charpath traces no Type 3 glyph, so these go to the real operator,
    // whose BuildChar paints with the decorated fill and stroke
    let texts = [
        ("show", "(AB) show"),
        ("ashow", "2 1 (AB) ashow"),
        ("widthshow", "6 0 32 (A B) widthshow"),
        ("awidthshow", "6 2 32 1 0 (A B) awidthshow"),
        ("kshow", "{ pop pop -3 0 rmoveto } (AB) kshow"),
        ("glyphshow", "/A glyphshow /B glyphshow"),
        ("xshow", "(AB) [22 24] xshow"),
        ("yshow", "(AB) [4 -6] yshow"),
        ("xyshow", "(AB) [22 4 24 -6] xyshow"),
    ];
    for (op, text) in texts {
        let program = format!("newpath 10 40 moveto {text}");
        let native = render_marquee(&program);
        assert!(
            ink(&native) > 2000,
            "{op}: Marquee drew {} pixels",
            ink(&native)
        );
        let warped = render_marquee(&format!("{{ {program} }} {{}} 2 wp-run"));
        assert!(ink(&warped) > 2000, "{op}: the warped text vanished");
        let worst = worst_block_diff(&warped, &native, 8);
        assert!(
            worst < 4.0,
            "{op}: identity warp changed Type 3 text: {worst}"
        );
        let shifted = render_marquee(&format!("{{ {program} }} 20 5 wp-shift 2 wp-run"));
        let moved = render_marquee(&format!("20 5 translate {program}"));
        let worst = worst_block_diff(&shifted, &moved, 8);
        assert!(
            worst < 4.0,
            "{op}: Type 3 text did not follow the warp: {worst}"
        );
    }
}

#[test]
fn a_failing_show_operator_leaves_what_the_real_one_leaves() {
    // bad operands and a missing current point go to the real operator,
    // so the error and the operands left behind are its own
    for body in [
        "newpath 10 10 moveto { 1 (x) (AB) ashow } stopped",
        "newpath { 1 1 (AB) ashow } stopped",
        "newpath { 1 (AB) ashow } stopped",
        "newpath 10 10 moveto { 1 1 300 (AB) widthshow } stopped",
        "newpath 10 10 moveto { 1 1 32.0 (AB) widthshow } stopped",
        "newpath { 1 1 (x) 1 1 (AB) awidthshow } stopped",
        "newpath 10 10 moveto { 5 (AB) kshow } stopped",
        "newpath 10 10 moveto { { pop pop newpath } (AB) kshow } stopped",
        "newpath 10 10 moveto { 65 glyphshow } stopped",
        "newpath 10 10 moveto { 7 show } stopped",
        // xshow, yshow, xyshow (#1191): the real operator checks, in its order
        "newpath 10 10 moveto { (AB) [1] xshow } stopped",
        "newpath 10 10 moveto { (AB) [1 2 3] xyshow } stopped",
        "newpath { (AB) [1 2] xshow } stopped",
        "newpath { (AB) [1] xshow } stopped",
        "newpath 10 10 moveto { (AB) [1 (x)] xshow } stopped",
        "newpath { (AB) [1 (x)] yshow } stopped",
        "newpath 10 10 moveto { (AB) [1 2 (x) 4] xyshow } stopped",
        "newpath 10 10 moveto { 7 [1 2] xshow } stopped",
        "newpath 10 10 moveto { (AB) 5 yshow } stopped",
        "newpath 10 10 moveto { [1 2] (AB) xyshow } stopped",
        "newpath 10 10 moveto { [1 2] xshow } stopped",
        "newpath 10 10 moveto { (AB) (nope) xshow } stopped",
        "newpath 10 10 moveto { (AB) (\\225\\060\\000\\003\\100\\000\\000\\000) xshow } stopped",
        "newpath { (AB) (\\225\\060\\000\\002\\100\\000\\000\\000\\100\\000\\000\\000) xshow } stopped",
    ] {
        assert_same_as_native(&format!(
            "/Helvetica findfont 24 scalefont setfont 9 {body} $error /errorname get"
        ));
    }
    // a warp that fails mid-glyph leaves no gsave and no glyph path behind
    let it = run("/Helvetica findfont 24 scalefont setfont 5 setlinewidth \
                  { newpath 0 0 moveto 50 50 lineto 10 10 moveto \
                    { 1 1 (AB) ashow } stopped pop } { pop pop 1 0 div } 2 wp-run \
                  currentlinewidth [ { } { } { } { } pathforall ] length");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["5.0", "6"], "{stack:?}");
}

/// The path painting operators, each with the operands it is called with.
const PAINTERS: [(&str, &str); 8] = [
    ("fill", "fill"),
    ("eofill", "eofill"),
    ("stroke", "stroke"),
    ("clip", "clip"),
    ("eoclip", "eoclip"),
    ("rectfill", "1 2 30 40 rectfill"),
    ("rectstroke", "1 2 30 40 rectstroke"),
    ("rectclip", "1 2 30 40 rectclip"),
];

/// Run each path painting operator under a warp that fails and check what
/// the program finds when it catches the error: `run_with` is the end of
/// the `wp-run` / `wp-run-path` call that follows the program. Returns the
/// error name and offending command. The operator is called with the
/// operands it is called with; the CTM is moved first, so a CTM left at the
/// warp's base would show; `/y` is the program's own, which must still
/// resolve to it, so a warp's scratch dictionary left open would show.
fn painter_after_failure(op: &str, call: &str, run_with: &str) -> (String, String) {
    let operands = call.strip_suffix(op).unwrap().trim();
    let program = format!(
        "/y 7 def {{ /snap {{ [ {{2 array astore}} {{2 array astore}} {{6 array astore}} {{[/z]}} \
                      pathforall ] }} bind def \
           1 setlinewidth 10 20 translate gsave 9 setlinewidth \
           newpath 0 0 moveto 10 0 20 30 40 50 curveto 40 0 lineto closepath \
           /args [ 77 66 {operands} ] def \
           /pre snap def /ctm0 matrix currentmatrix def /d0 countdictstack def \
           args aload pop {{ {op} }} stopped \
           /caught exch def \
           count array astore /after exch def \
           $error /errorname get $error /command get countdictstack d0 sub \
           snap pre matrix currentmatrix ctm0 \
           grestore currentlinewidth after args caught y \
         }} {run_with}"
    );
    let it = run(&program);
    let s: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    let [
        name,
        command,
        dicts,
        path,
        pre,
        ctm,
        ctm0,
        width,
        after,
        args,
        caught,
        y,
    ] = &s[..]
    else {
        panic!("{op}: {s:?}");
    };
    assert_eq!(caught, "true", "{op}: no error reached the program");
    assert_eq!(after, args, "{op}: operand stack");
    assert_eq!(dicts, "0", "{op}: dictionary stack");
    assert_eq!(y, "7", "{op}: the program's y was shadowed");
    assert_eq!(path, pre, "{op}: current path");
    assert!(
        path.contains("10.0 0.0 20.0 30.0 40.0 50.0"),
        "{op}: curve kept"
    );
    assert_eq!(ctm, ctm0, "{op}: CTM");
    assert_eq!(width, "1.0", "{op}: a gsave was left behind");
    (name.clone(), command.clone())
}

#[test]
fn a_failing_warp_leaves_the_stack_and_graphics_state_as_the_call_had_them() {
    // Under a warp that raises undefinedresult (#1184), each decorated
    // painting operator, caught with stopped, leaves what the real one
    // leaves: the operands it was called with (the real rectfill keeps
    // them on a typecheck: gs prints 77 1 2 (a) 4 for `77 1 2 (a) 4
    // rectfill` under stopped), the path and CTM it was called with and
    // the gsave depth.
    for (op, call) in PAINTERS {
        let (name, command) = painter_after_failure(op, call, "{ pop pop 1 0 div } 2 wp-run");
        assert_eq!(name, "/undefinedresult", "{op}");
        assert_eq!(command, "div", "{op}");
    }
}

#[test]
fn a_path_transform_that_fails_with_its_own_mark_open_is_unwound_by_depth() {
    // #1199: the guard used to clear back to the nearest mark, which was the
    // transform's own: `{ mark 1 0 div }` left the decorator's records, a
    // mark and the rect operands behind, and the operands were not put back
    // (gs: `77 1 2 3 4 rectfill` leaves them on an error, run above).
    for (op, call) in PAINTERS {
        let (name, command) =
            painter_after_failure(op, call, "{ mark 1 0 div } 2 wp-run-path");
        assert_eq!(name, "/undefinedresult", "{op}");
        assert_eq!(command, "div", "{op}");
        // a mark and more of the transform's own debris
        let (name, _) = painter_after_failure(
            op,
            call,
            "{ [ exch { 1 } forall mark 5 6 7 1 0 div } 2 wp-run-path",
        );
        assert_eq!(name, "/undefinedresult", "{op}");
    }
}

#[test]
fn a_closure_warp_that_fails_does_not_leave_its_scratch_dictionary_open() {
    // #1199: wp-unroll with a point exactly at its centre fails in `atan`
    // (0 0 atan is undefinedresult in gs too) with the warp's dictionary
    // open: countdictstack stayed one higher and the program's y resolved to
    // the warp's. The path starts at 10 20, the centre. The operators whose
    // path has no point exactly there use a closure warp that always fails,
    // with a y of its own.
    for (op, call) in PAINTERS {
        let (name, _) = painter_after_failure(
            op,
            call,
            "<< /y 99 >> { pop pop y 0 div } wp--closure 2 wp-run",
        );
        assert_eq!(name, "/undefinedresult", "{op}");
        if matches!(op, "fill" | "eofill" | "clip" | "eoclip") {
            let (name, _) = painter_after_failure(op, call, "10 20 1 wp-unroll 2 wp-run-path");
            assert_eq!(name, "/undefinedresult", "{op}: wp-unroll at its centre");
        }
    }
}

#[test]
fn a_transform_that_leaves_other_than_one_array_is_refused_and_unwound() {
    // #1199: `{ 99 exch }` leaves two results and used to be accepted, and a
    // transform that leaves nothing made the guard eat the caller's operands.
    for (op, call) in PAINTERS {
        for transform in ["{ 99 exch }", "{ pop }", "{ pop 5 }"] {
            let (name, command) =
                painter_after_failure(op, call, &format!("{transform} 2 wp-run-path"));
            assert_eq!(name, "/undefined", "{op} {transform}");
            assert_eq!(command, "wp-warp-must-return-one-result", "{op} {transform}");
        }
    }
}

#[test]
fn cshow_needs_no_decorator_because_what_its_procedure_shows_goes_through_the_warp() {
    // #1191: cshow paints nothing. The procedure gets code wx wy, and the
    // real widths are right under any warp (a warp does not change a
    // font's metrics), while the glyphs the procedure shows with `show`
    // are drawn by the decorated operator.
    let text = "{ pop pop 1 string dup 0 4 -1 roll put show } (AVA) cshow";
    let program = show_program(text);
    let native = render(&program);
    assert!(ink(&native) > 2000, "cshow drew {} pixels", ink(&native));
    let warped = render(&format!("{{ {program} }} {{}} 2 wp-run"));
    let worst = worst_block_diff(&warped, &native, 8);
    assert!(worst < 4.0, "identity warp changed cshow text: {worst}");
    let shifted = render(&format!("{{ {program} }} 20 5 wp-shift 2 wp-run"));
    let moved = render(&format!("20 5 translate {program}"));
    let worst = worst_block_diff(&shifted, &moved, 8);
    assert!(worst < 4.0, "cshow's text did not follow the warp: {worst}");
    // the widths it reports, the current point it leaves and the stack
    assert_same_as_native(
        "/Helvetica findfont 24 scalefont setfont 9 newpath 15 50 moveto \
         [ { 3 array astore aload pop } (AV) cshow ] \
         currentpoint { pop pop 7 } (AV) cshow",
    );
    // no decorator is built for it: the operator stays the real one
    let it = run("{} 4 wp-decorators dup /cshow known exch /xyshow known");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["false", "true"], "{stack:?}");
}

#[test]
fn xshow_family_places_unicode_glyphs_and_ignores_extra_numbers() {
    // one number per glyph, and a Hangul syllable is one glyph (three bytes
    // of UTF-8), so the pen moves by three numbers for six syllables' worth
    // of bytes; extra numbers are allowed
    let font = "/NotoSansKR-Regular findfont 22 scalefont setfont";
    for text in [
        "(\u{c548}\u{c548}\u{c548}) [20 24 28 99 99] xshow",
        "(\u{c548}\u{c548}) [24 -4 22 3 1 1] xyshow",
    ] {
        let program = format!("{font} newpath 10 40 moveto {text}");
        let native = render(&program);
        assert!(ink(&native) > 500, "{text}: {} pixels", ink(&native));
        let warped = render(&format!("{{ {program} }} 20 5 wp-shift 2 wp-run"));
        let moved = render(&format!("20 5 translate {program}"));
        let worst = worst_block_diff(&warped, &moved, 8);
        assert!(worst < 4.0, "{text}: did not follow the warp: {worst}");
        assert_same_as_native(&format!("{font} newpath 10 40 moveto {text} currentpoint"));
    }
}

#[test]
fn xshow_family_bends_with_a_real_warp_and_places_glyphs_in_user_space() {
    // the displacements are user space numbers: under a scale their pen
    // path is scaled, and a warp then bends where the glyphs land
    let text = "(HHH) [20 20 20] xshow";
    let program = format!("/Helvetica findfont 20 scalefont setfont newpath 10 50 moveto {text}");
    let flat = render(&format!("{{ {program} }} {{}} 2 wp-run"));
    let bent = render(&format!("{{ {program} }} 10 120 wp-ripple 2 wp-run"));
    assert!(ink(&bent) > 500);
    assert!(worst_block_diff(&flat, &bent, 8) > 20.0, "the ripple did not bend the text");
    // positions: yshow moves only up the page
    let it = run("/Helvetica findfont 20 scalefont setfont newpath 10 10 moveto \
                  { (HH) [5 7] yshow currentpoint } {} 2 wp-run");
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(stack, ["10.0", "22.0"], "{stack:?}");
}

/// A show-family `text` (at the current point 20 20, with the program's
/// own `9` under it) run under a warp that fails: what the program finds
/// after it catches the error. `run_with` ends the `wp-run` call.
fn show_after_failure(text: &str, run_with: &str) -> String {
    let program = format!(
        "/y 7 def {{ /Helvetica findfont 24 scalefont setfont \
           /snap {{ [ {{2 array astore}} {{2 array astore}} {{6 array astore}} {{[/z]}} \
                      pathforall ] }} bind def \
           1 setlinewidth gsave 9 setlinewidth \
           newpath 5 5 moveto 80 5 lineto 20 20 moveto \
           /pre snap def /d0 countdictstack def \
           9 {{ {text} }} stopped /caught exch def \
           count array astore /after exch def \
           $error /errorname get countdictstack d0 sub snap pre \
           grestore currentlinewidth after caught y \
         }} {run_with}"
    );
    let it = run(&program);
    let s: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    let [name, dicts, path, pre, width, after, caught, y] = &s[..] else {
        panic!("{text}: {s:?}");
    };
    assert_eq!(caught, "true", "{text}: no error reached the program");
    assert_eq!(after, "[9]", "{text}: {run_with}: operand stack");
    assert_eq!(dicts, "0", "{text}: {run_with}: dictionary stack");
    assert_eq!(y, "7", "{text}: the program's y was shadowed");
    assert_eq!(path, pre, "{text}: {run_with}: current path");
    assert_eq!(width, "1.0", "{text}: a gsave was left behind");
    name.clone()
}

#[test]
fn the_show_family_guard_unwinds_by_depth_too() {
    // #1199: the same two gaps as the path painters, in the guard of the
    // show family (wp--guarded): a transform's own mark, and a closure warp's
    // open dictionary (wp-unroll with a glyph point exactly at its centre,
    // here the first point of the A that starts every text but glyphshow's).
    let centre = "/Helvetica findfont 24 scalefont setfont \
                  newpath 20 20 moveto (A) true charpath flattenpath \
                  [ {2 array astore} {pop pop} {pop pop pop pop pop pop} {} pathforall ] \
                  0 get aload pop 1 wp-unroll newpath 2 wp-run-path";
    let mut texts = vec![("show", "(AB) show")];
    texts.extend(SHOW_FAMILY);
    for (op, text) in texts {
        let mark = show_after_failure(text, "{ mark 1 0 div } 2 wp-run-path");
        assert_eq!(mark, "/undefinedresult", "{op}");
        let closure = show_after_failure(
            text,
            "<< /y 99 >> { pop pop y 0 div } wp--closure 2 wp-run",
        );
        assert_eq!(closure, "/undefinedresult", "{op}");
        // extra results: refused, and unwound all the same
        show_after_failure(text, "{ 99 exch } 2 wp-run-path");
        show_after_failure(text, "{ pop } 2 wp-run-path");
        if op != "glyphshow" {
            let centred = show_after_failure(text, centre);
            assert_eq!(centred, "/undefinedresult", "{op}: wp-unroll at its centre");
        }
    }
}

#[test]
fn the_path_painters_still_paint_and_leave_the_real_paths_behind() {
    // an identity warp leaves what the real operators leave: fill and
    // stroke consume the path, clip keeps it, the rect operators leave it
    // alone (rectclip clears it) and every clip shrinks the clip
    for (_, call) in PAINTERS {
        assert_same_as_native(&format!(
            "10 20 translate newpath 0 0 moveto 10 0 20 30 40 50 curveto 40 0 lineto closepath \
             {call} \
             [ {{2 array astore}} {{2 array astore}} {{6 array astore}} {{[/z]}} pathforall ] \
             {{ currentpoint 2 array astore }} stopped pop clippath pathbbox 4 array astore"
        ));
    }
}

#[test]
fn rect_operators_with_bad_operands_or_the_array_form_go_to_the_real_operator() {
    // Not four numbers on the stack: the real operator raises its own error
    // (name, command and the operands it leaves), and the numarray form is
    // drawn unwarped. The warp fails if it is reached, so none of these
    // may reach it.
    for op in ["rectfill", "rectstroke", "rectclip"] {
        for operands in [
            "[0 0 10 10]",
            "1 2",
            "1 2 (a) 4",
            "1 2 3 (a)",
            "",
            "[0 0 10 10] [0 0 5 5]",
        ] {
            let body = format!(
                "newpath 0 0 moveto 9 9 lineto 77 {{ {operands} {op} }} stopped \
                 {{ $error /errorname get $error /command get }} {{ null null }} ifelse"
            );
            let native = stack_text(&body);
            let warped = stack_text(&format!("{{ {body} }} {{ pop pop 1 0 div }} 2 wp-run"));
            assert_eq!(warped, native, "{op}: {operands}");
        }
    }
    // the cases above do raise the plain operator's error, not the warp's
    let it = run("77 { 1 2 (a) 4 rectfill } stopped $error /command get");
    assert_eq!(
        it.operand_stack().last().map(|o| o.repr()).as_deref(),
        Some("rectfill")
    );
}
