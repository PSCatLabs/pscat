//! image / imagemask / colorimage — Stage 7 task 4. Orientation and
//! mask polarity are pinned against Ghostscript (see the imagemask
//! polarity note in ops/image.rs).

use pscat::{Interp, PsError};

fn run(src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

const BLACK: (u8, u8, u8) = (0, 0, 0);
const WHITE: (u8, u8, u8) = (255, 255, 255);

/// 2x2 gray checker: top row black/white, bottom row white/black,
/// occupying user (10..90)^2. Matrix [2 0 0 -2 0 2] puts sample row 0
/// at the top, PostScript-style.
const CHECKER: &str = "10 10 translate 80 80 scale
     2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image";

#[test]
fn level1_image_paints_the_checker() {
    let it = run(CHECKER);
    // User top-left (30,70) = device (30,30): sample (0,0) = 00.
    assert_eq!(pixel(&it, 30, 30), BLACK);
    assert_eq!(pixel(&it, 70, 30), WHITE);
    assert_eq!(pixel(&it, 30, 70), WHITE);
    assert_eq!(pixel(&it, 70, 70), BLACK);
    // Outside the unit square: untouched.
    assert_eq!(pixel(&it, 5, 5), WHITE);
}

#[test]
fn image_respects_rotation_and_clip() {
    let it = run(
        "newpath 50 0 moveto 100 0 lineto 100 100 lineto 50 100 lineto closepath clip newpath
         50 50 translate 30 rotate -40 -40 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image",
    );
    // Left of the clip boundary must be untouched.
    for y in [20, 50, 80] {
        assert_eq!(pixel(&it, 30, y), WHITE, "clipped at (30,{y})");
    }
    // Something dark must have landed on the right side.
    let dark = (50..100)
        .flat_map(|x| (0..100).map(move |y| (x, y)))
        .filter(|&(x, y)| pixel(&it, x, y).0 < 100)
        .count();
    assert!(dark > 100, "rotated image painted {dark} dark px");
}

#[test]
fn imagemask_polarity_true_paints_ones() {
    // Samples <80> on a 2x1 mask: [1, 0]. Pinned against gs: with
    // true, the '1' half paints in the current color.
    let it = run("1 0 0 setrgbcolor 10 40 translate 80 20 scale
         2 1 true [2 0 0 -1 0 1] {<80>} imagemask");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0), "sample 1 painted");
    assert_eq!(pixel(&it, 70, 50), WHITE, "sample 0 masked out");
}

#[test]
fn imagemask_polarity_false_paints_zeros() {
    let it = run("0 0 1 setrgbcolor 10 40 translate 80 20 scale
         2 1 false [2 0 0 -1 0 1] {<80>} imagemask");
    assert_eq!(pixel(&it, 30, 50), WHITE);
    assert_eq!(pixel(&it, 70, 50), (0, 0, 255));
}

#[test]
fn colorimage_rgb_interleaved() {
    let it = run("10 10 translate 80 80 scale
         2 1 8 [2 0 0 -1 0 1] {<ff0000 0000ff>} false 3 colorimage");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 0, 255));
}

#[test]
fn dict_image_uses_colorspace_and_decode() {
    // DeviceRGB via setcolorspace; Decode [1 0 1 0 1 0] inverts.
    let it = run("/DeviceRGB setcolorspace
         10 10 translate 80 80 scale
         << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
            /ImageMatrix [1 0 0 -1 0 1]
            /Decode [1 0 1 0 1 0]
            /DataSource <00ff00>
         >> image");
    // 00ff00 inverted = ff00ff.
    assert_eq!(pixel(&it, 50, 50), (255, 0, 255));
}

#[test]
fn file_source_reads_inline_data() {
    // The classic: image data inline in the program, through a filter
    // on currentfile. The scanner must resume cleanly afterwards.
    let it = run("10 10 translate 80 80 scale
         1 1 8 [1 0 0 -1 0 1]
         currentfile /ASCIIHexDecode filter image
         00>
         initmatrix 0 0 1 setrgbcolor 4 setlinewidth
         newpath 2 95 moveto 8 95 lineto stroke");
    assert_eq!(pixel(&it, 50, 50), BLACK, "inline sample painted");
    assert_eq!(pixel(&it, 5, 5), (0, 0, 255), "program resumed after data");
}

#[test]
fn proc_source_called_per_chunk() {
    // The proc returns one row per call; a counter proves it ran
    // once per scanline.
    let mut it = run("/calls 0 def
         /row 2 string def
         10 10 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2]
         { /calls calls 1 add def
           calls 1 eq { (\\000\\377) } { (\\377\\000) } ifelse }
         image
         calls");
    assert_eq!(it.pop().expect("calls").repr(), "2");
    assert_eq!(pixel(&it, 30, 30), BLACK);
    assert_eq!(pixel(&it, 70, 70), BLACK);
}

#[test]
fn empty_string_from_proc_ends_the_image_early() {
    // EOD after one of two rows: the second row reads as sample 0
    // (black) — premature end renders what arrived, no error.
    let it = run("/first true def
         10 10 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2]
         { first { /first false def (\\377\\377) } { () } ifelse }
         image");
    assert_eq!(pixel(&it, 30, 30), WHITE, "delivered row");
    assert_eq!(pixel(&it, 30, 70), BLACK, "missing row reads as 0");
}

#[test]
fn bitmap_fonts_work_via_imagemask_in_buildchar() {
    // The classic bitmap-font construction: a Type 3 glyph procedure
    // that stamps its raster with imagemask. Image frame inside a
    // BuildChar frame inside a show frame — all nested machinery.
    let it = run("/BitFont 6 dict def
         BitFont begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for
           Encoding 97 /blk put
           /BuildChar {
             exch begin
               1000 0 0 0 800 800 setcachedevice
               800 800 scale
               2 2 true [2 0 0 -2 0 2] {<c0 00>} imagemask
             end
           } def
         end
         /BitFont BitFont definefont pop
         /BitFont findfont 60 scalefont setfont
         20 20 moveto (a) show");
    // <c0> on 2x2 = rows [1 1] / [0 0]: the raster's top half paints.
    // Glyph occupies user (20..68)^2; top half = user y 44..68.
    assert_eq!(pixel(&it, 40, 100 - 60), BLACK, "top half stamped");
    assert_eq!(pixel(&it, 40, 100 - 30), WHITE, "bottom half clear");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn dct_source_feeds_image() {
    // 8x8 mid-gray JPEG (tests/data fixture) as an image DataSource,
    // through the classic hex→DCT filter chain.
    let jpeg = hex(include_bytes!("data/gray8.jpg"));
    let it = run(&format!(
        "10 10 translate 80 80 scale
         8 8 8 [8 0 0 -8 0 8]
         ({jpeg}) /ASCIIHexDecode filter /DCTDecode filter image"
    ));
    let (r, g, b) = pixel(&it, 50, 50);
    assert_eq!((r, g), (g, b), "gray stays gray");
    assert!((i32::from(r) - 128).abs() <= 8, "mid-gray, got {r}");
    assert_eq!(pixel(&it, 5, 5), WHITE, "outside untouched");
}

#[test]
fn dct_source_feeds_colorimage() {
    let jpeg = hex(include_bytes!("data/red4.jpg"));
    let it = run(&format!(
        "10 10 translate 80 80 scale
         4 4 8 [4 0 0 -4 0 4]
         ({jpeg}) /ASCIIHexDecode filter /DCTDecode filter
         false 3 colorimage"
    ));
    let (r, g, b) = pixel(&it, 50, 50);
    assert!(r >= 240 && g <= 15 && b <= 15, "red, got {r} {g} {b}");
}

#[test]
fn exit_inside_data_proc_is_invalidexit() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("2 2 8 [2 0 0 -2 0 2] {exit} image"),
        Err(PsError::InvalidExit)
    );
}

#[test]
fn image_argument_errors() {
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("2 2 3 [2 0 0 -2 0 2] {<00>} image"),
        Err(PsError::Rangecheck),
        "bits must be 1/2/4/8/12/16"
    );
    assert_eq!(
        it.run_str("2 2 3 [2 0 0 -2 0 2] {<00>} false 1 colorimage"),
        Err(PsError::Rangecheck),
        "bits must be 1/2/4/8/12/16"
    );
}

// ---- #534: any invertible ImageMatrix, not only the `w h scale` idiom ----
//
// The PLRM maps the source rectangle [0,w]×[0,h] through ImageMatrix⁻¹
// into user space, so `10 10 scale … [1 0 0 -1 0 8]` and `[0.1 0 0
// -0.1 0 8]` with no scale at all cover the same 80×80 square as the
// usual `80 80 scale … [8 0 0 -8 0 8]`. pscat used to clip every image
// to the user unit square, which painted one cell. Every expectation
// below was read off Ghostscript 10.07 at 72 dpi on the same 100×100
// page (png16m, no anti-aliasing): ink counts are gs's, and each
// unscaled form must be pixel-identical to its standard-idiom twin,
// which gs renders identically too.

/// An 8×8 asymmetric "F" in a frame, so a flip or transpose shows.
const F_MASK: &str = "<FF 81 BD A1 B9 A1 A1 FF>";
/// A 4×4 8-bit gray ramp.
const RAMP: &str = "<00 40 80 c0 ff 00 40 80 c0 ff 00 40 80 c0 ff 20>";

fn ink(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| (p.red(), p.green(), p.blue()) != WHITE)
        .count()
}

fn assert_same_render(a: &str, b: &str, gs_ink: usize) {
    let (ia, ib) = (run(a), run(b));
    assert_eq!(ink(&ia), gs_ink, "gs ink count for {a}");
    assert!(
        ia.gfx().pixmap.data() == ib.gfx().pixmap.data(),
        "{a}\n  must render as\n{b}"
    );
}

#[test]
fn imagemask_unscaled_matrices_cover_the_whole_mask() {
    let std = format!("10 10 translate 80 80 scale 8 8 true [8 0 0 -8 0 8] {{{F_MASK}}} imagemask");
    assert_eq!(ink(&run(&std)), 3800, "gs ink for the standard idiom");
    for form in [
        // one source pixel per user unit, then 10 10 scale (the issue)
        format!("10 10 translate 10 10 scale 8 8 true [1 0 0 -1 0 8] {{{F_MASK}}} imagemask"),
        // no scale at all: the matrix alone sizes the image
        format!("10 10 translate 8 8 true [0.1 0 0 -0.1 0 8] {{{F_MASK}}} imagemask"),
        // the Level 2 dict form of the same
        format!(
            "10 10 translate << /ImageType 1 /Width 8 /Height 8 /BitsPerComponent 1
               /Decode [1 0] /ImageMatrix [0.1 0 0 -0.1 0 8] /DataSource {F_MASK} >> imagemask"
        ),
    ] {
        assert_same_render(&form, &std, 3800);
    }
    // Unflipped (row 0 at the bottom).
    assert_same_render(
        &format!("10 10 translate 10 10 scale 8 8 true [1 0 0 1 0 0] {{{F_MASK}}} imagemask"),
        &format!("10 10 translate 80 80 scale 8 8 true [8 0 0 8 0 0] {{{F_MASK}}} imagemask"),
        3800,
    );
    // A rotated CTM: same pixels as the rotated standard idiom.
    assert_same_render(
        &format!(
            "50 50 translate 30 rotate -40 -40 translate 10 10 scale
             8 8 true [1 0 0 -1 0 8] {{{F_MASK}}} imagemask"
        ),
        &format!(
            "50 50 translate 30 rotate -40 -40 translate 80 80 scale
             8 8 true [8 0 0 -8 0 8] {{{F_MASK}}} imagemask"
        ),
        3700, // gs: 3701, the same one-pixel edge as the standard idiom
    );
}

#[test]
fn imagemask_offset_skew_color_and_polarity_match_gs() {
    // Translated source: columns 0-1 fall left of the origin and are cut.
    let it = run(&format!(
        "10 10 translate 10 10 scale 8 8 true [1 0 0 -1 -2 8] {{{F_MASK}}} imagemask"
    ));
    assert_eq!(ink(&it), 3000);
    assert_eq!(pixel(&it, 15, 15), WHITE);
    assert_eq!(pixel(&it, 85, 15), BLACK);
    // A skewed, non-unit matrix.
    let it = run(&format!(
        "20 10 translate 8 8 true [0.125 0 0.05 -0.125 0 8] {{{F_MASK}}} imagemask"
    ));
    assert_eq!(ink(&it), 2393);
    assert_eq!(pixel(&it, 35, 45), BLACK);
    assert_eq!(pixel(&it, 15, 15), WHITE);
    // The current color, and polarity false (paints the 0s).
    let it = run(&format!(
        "10 10 translate 0 1 0 setrgbcolor 8 8 true [0.1 0 0 -0.1 0 8] {{{F_MASK}}} imagemask"
    ));
    assert_eq!(ink(&it), 3800);
    assert_eq!(pixel(&it, 15, 15), (0, 255, 0));
    let it = run(&format!(
        "10 10 translate 8 8 false [0.1 0 0 -0.1 0 8] {{{F_MASK}}} imagemask"
    ));
    assert_eq!(ink(&it), 2600);
    assert_eq!(pixel(&it, 15, 15), WHITE, "frame is 1s: masked out");
}

#[test]
fn image_and_colorimage_unscaled_matrices_match_gs() {
    let std = format!("10 10 translate 80 80 scale 4 4 8 [4 0 0 -4 0 4] {{{RAMP}}} image");
    assert_eq!(ink(&run(&std)), 5200);
    for form in [
        format!("10 10 translate 20 20 scale 4 4 8 [1 0 0 -1 0 4] {{{RAMP}}} image"),
        format!("10 10 translate 4 4 8 [0.05 0 0 -0.05 0 4] {{{RAMP}}} image"),
        format!(
            "10 10 translate /DeviceGray setcolorspace << /ImageType 1 /Width 4 /Height 4
               /BitsPerComponent 8 /Decode [0 1] /ImageMatrix [0.05 0 0 -0.05 0 4]
               /DataSource {RAMP} >> image"
        ),
    ] {
        assert_same_render(&form, &std, 5200);
    }
    // Transposed, no scale: gs reads the ramp down columns.
    assert_same_render(
        &format!("10 10 translate 4 4 8 [0 0.05 0.05 0 0 0] {{{RAMP}}} image"),
        &format!("10 10 translate 80 80 scale 4 4 8 [0 4 4 0 0 0] {{{RAMP}}} image"),
        5200,
    );
    let it = run("10 10 translate 4 4 8 [0.05 0 0 -0.05 0 4]
         {<ff0000 00ff00 0000ff 000000 808080 ff00ff 00ffff ffff00
           ff0000 00ff00 0000ff 000000 808080 ff00ff 00ffff ffff00>}
         false 3 colorimage");
    assert_eq!(ink(&it), 6400);
    assert_eq!(pixel(&it, 15, 15), (255, 0, 0));
    assert_eq!(pixel(&it, 85, 85), (255, 255, 0));
    assert_eq!(pixel(&it, 50, 50), (0, 0, 255));
}

#[test]
fn singular_image_matrix_is_undefinedresult_before_any_data() {
    // gs: undefinedresult, and the data procedure never runs.
    for src in [
        "/n 0 def 8 8 true [0 0 0 0 0 0] { /n n 1 add def <00> } imagemask",
        "/n 0 def 4 4 8 [1 2 2 4 0 0] { /n n 1 add def <00> } image",
        "/n 0 def << /ImageType 1 /Width 4 /Height 4 /BitsPerComponent 8 /Decode [0 1]
           /ImageMatrix [0 0 0 0 0 0] /DataSource { /n n 1 add def <00> } >> image",
    ] {
        let mut it = Interp::with_page(100, 100).expect("page");
        assert_eq!(it.run_str(src), Err(PsError::UndefinedResult), "{src}");
        it.run_str("n 0 eq { } { (data proc ran) = stop } ifelse")
            .expect("data proc must not run");
    }
    // gs quirk, matched: Level 1 image/colorimage with no samples return
    // before the matrix check; imagemask and the dict form still check.
    let mut it = Interp::with_page(100, 100).expect("page");
    for ok in [
        "0 4 8 [0 0 0 0 0 0] {} image",
        "4 0 8 [0 0 0 0 0 0] {} false 3 colorimage",
    ] {
        assert_eq!(it.run_str(ok), Ok(()), "{ok}");
    }
    for bad in [
        "0 0 true [0 0 0 0 0 0] {} imagemask",
        "<< /ImageType 1 /Width 0 /Height 4 /BitsPerComponent 8 /Decode [0 1]
           /ImageMatrix [0 0 0 0 0 0] /DataSource {} >> image",
    ] {
        assert_eq!(it.run_str(bad), Err(PsError::UndefinedResult), "{bad}");
    }
}

// ---- setalpha / setblendmode reach images (#766) ---------------------------

#[test]
fn image_takes_alpha_and_resets_with_grestore() {
    // The black checker cells at 0.5 over white are mid grey; the white
    // cells stay white; after `grestore` the image is opaque again.
    let it = run(&format!("gsave 0.5 setalpha {CHECKER} grestore"));
    let (r, g, b) = pixel(&it, 30, 30);
    assert!((126..=129).contains(&r) && r == g && g == b, "{r} {g} {b}");
    assert_eq!(pixel(&it, 70, 30), WHITE);
    let it = run(&format!("gsave 0.5 setalpha grestore {CHECKER}"));
    assert_eq!(pixel(&it, 30, 30), BLACK);
}

#[test]
fn imagemask_takes_alpha_and_multiply() {
    // A red stencil over a cyan ground (0,255,255). At alpha 0.5 Normal
    // gives (128,128,128); Multiply of pure red on cyan is black.
    let ground = "0 1 1 setrgbcolor 0 0 100 100 rectfill 1 0 0 setrgbcolor";
    let mask = "10 10 translate 80 80 scale 1 1 true [1 0 0 -1 0 1] {<80>} imagemask";
    let it = run(&format!("{ground} 0.5 setalpha {mask}"));
    let (r, g, b) = pixel(&it, 50, 50);
    assert!(
        (126..=129).contains(&r) && (126..=129).contains(&g) && (126..=129).contains(&b),
        "{r} {g} {b}"
    );
    assert_eq!(pixel(&it, 5, 5), (0, 255, 255));
    let it = run(&format!("{ground} /Multiply setblendmode {mask}"));
    assert_eq!(pixel(&it, 50, 50), BLACK);
}

#[test]
fn translucent_image_on_a_transparent_page_stays_translucent() {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.gfx_mut().set_transparent_background();
    it.run_str(&format!("0.5 setalpha {CHECKER}"))
        .expect("runs");
    let p = it.gfx().pixmap.pixel(30, 30).unwrap();
    assert!((126..=129).contains(&p.alpha()), "alpha {}", p.alpha());
}

#[test]
fn bitmap_glyph_stamp_takes_alpha_and_does_not_leak_it() {
    // A bitmap-font glyph (imagemask in a Type 3 BuildChar) is text, and
    // text takes alpha: 0.5 gives mid grey, and the same glyph shown at
    // alpha 1 straight after is fully black (nothing cached or kept).
    let font = "/BitFont 6 dict def
         BitFont begin
           /FontType 3 def
           /FontMatrix [0.001 0 0 0.001 0 0] def
           /Encoding 256 array def 0 1 255 { Encoding exch /.notdef put } for
           Encoding 97 /blk put
           /BuildChar {
             exch begin
               1000 0 0 0 800 800 setcachedevice
               800 800 scale
               2 2 true [2 0 0 -2 0 2] {<c0 00>} imagemask
             end
           } def
         end
         /BitFont BitFont definefont pop
         /BitFont findfont 40 scalefont setfont";
    let it = run(&format!("{font} 0.5 setalpha 10 10 moveto (a) show"));
    let (r, g, b) = pixel(&it, 25, 100 - 35);
    assert!((126..=129).contains(&r) && r == g && g == b, "{r} {g} {b}");
    let it = run(&format!(
        "{font} 0.5 setalpha 10 10 moveto (a) show 1 setalpha 10 10 moveto (a) show"
    ));
    assert_eq!(pixel(&it, 25, 100 - 35), BLACK);
}
