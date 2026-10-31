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
