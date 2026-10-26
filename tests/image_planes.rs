//! 16-bit samples, `colorimage` with `multi true` and `/MultipleDataSources
//! true` (#781). A wide sample or a separate source must come out exactly as
//! the same picture written as interleaved 8-bit data, in PNG, SVG and PDF
//! alike (the outputs are 8-bit RGB throughout; wider samples are reduced to
//! 8 bits). Samples below are `n * 257`, which scale to `n` exactly.

use pscat::{Interp, PsError};

fn page() -> Interp {
    Interp::with_page(100, 100).expect("test page")
}

fn pixel(it: &Interp, x: u32, y: u32) -> (u8, u8, u8) {
    let p = it.gfx().pixmap.pixel(x, y).expect("pixel in bounds");
    (p.red(), p.green(), p.blue())
}

/// Run `src` and return the canvas plus the SVG and PDF it recorded.
fn render(src: &str) -> (Vec<u8>, String, Vec<u8>) {
    let mut it = page();
    it.gfx_mut().enable_svg();
    it.gfx_mut().enable_pdf();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let svg = it.gfx().svg_pages().expect("svg")[0].clone();
    let pdf = it.gfx().pdf_document().expect("pdf");
    (it.gfx().pixmap.data().to_vec(), svg, pdf)
}

/// Two renders agree in the canvas, the SVG and the PDF.
fn same(a: &str, b: &str) {
    let (pa, sa, da) = render(a);
    let (pb, sb, db) = render(b);
    assert!(pa == pb, "PNG rasters differ");
    assert_eq!(sa, sb, "SVG differs");
    assert!(da == db, "PDF differs");
    assert!(
        pa.chunks(4).any(|p| p[..3] != [255, 255, 255]),
        "the picture is blank"
    );
}

// 2x2 RGB, row 0: red, green; row 1: blue, mid grey. 8-bit and 16-bit forms.
const RGB8: &str = "<ff0000 00ff00 0000ff 808080>";
const RGB16: &str = "<ffff00000000 0000ffff0000 00000000ffff 808080808080>";
const R8: &str = "<ff00 0080>";
const G8: &str = "<00ff 0080>";
const B8: &str = "<0000 ff80>";
const R16: &str = "<ffff0000 00008080>";
const G16: &str = "<0000ffff 00008080>";
const B16: &str = "<00000000 ffff8080>";

#[test]
fn sixteen_bit_colorimage_matches_eight_bit() {
    same(
        &format!("10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {RGB8} false 3 colorimage"),
        &format!("10 10 translate 80 80 scale 2 2 16 [2 0 0 -2 0 2] {RGB16} false 3 colorimage"),
    );
}

#[test]
fn sixteen_bit_dict_image_reads_big_endian_pairs_in_every_space() {
    // Gray: 0x0000 0xffff 0x8080 0x4040 (row 0 black, white; row 1 grey, dark).
    let it = {
        let mut it = page();
        it.run_str(
            "/DeviceGray setcolorspace 10 10 translate 80 80 scale
             << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 16
                /ImageMatrix [2 0 0 -2 0 2] /DataSource <0000ffff 80804040> >> image",
        )
        .expect("runs");
        it
    };
    assert_eq!(pixel(&it, 30, 30), (0, 0, 0));
    assert_eq!(pixel(&it, 70, 30), (255, 255, 255));
    assert_eq!(pixel(&it, 30, 70), (128, 128, 128));
    assert_eq!(pixel(&it, 70, 70), (64, 64, 64));
    // The low byte counts: 0x0001 is not 0x0000 only at 16 bits' resolution,
    // so a high byte alone must not decide a sample (0x00ff is about 1/256).
    let mut it = page();
    it.run_str(
        "/DeviceGray setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 16
            /ImageMatrix [1 0 0 -1 0 1] /DataSource <7fff> >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 50, 50), (127, 127, 127), "0x7fff / 65535 * 255");
    // Decode inverts a 16-bit sample.
    let mut it = page();
    it.run_str(
        "/DeviceGray setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 16 /Decode [1 0]
            /ImageMatrix [1 0 0 -1 0 1] /DataSource <ffff> >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 50, 50), (0, 0, 0));
}

#[test]
fn level_one_image_operand_takes_sixteen_bits() {
    // width height bits matrix datasrc image (gs accepts 16 here too).
    let mut it = page();
    it.run_str("10 10 translate 80 80 scale 2 1 16 [2 0 0 -1 0 1] <0000ffff> image")
        .expect("runs");
    assert_eq!(pixel(&it, 30, 50), (0, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (255, 255, 255));
}

#[test]
fn sixteen_bit_cmyk_and_indexed_and_twelve_bit() {
    same(
        "/DeviceCMYK setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <ff000040 800000ff> >> image",
        "/DeviceCMYK setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 16
            /ImageMatrix [2 0 0 -1 0 1]
            /DataSource <ffff000000004040 80800000 0000ffff> >> image",
    );
    // 12 bits: 0x111 * k is exactly 0x11 * k at 8 bits (two samples a byte
    // and a half each: 111 222 333 444 | 555 666 777 888).
    same(
        "/DeviceCMYK setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <11223344 55667788> >> image",
        "/DeviceCMYK setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 12
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <111222 333444 555666 777888> >> image",
    );
    // Indexed: the 16-bit sample is the palette index.
    let mut it = page();
    it.run_str(
        "[/Indexed /DeviceRGB 1 <ff0000 0000ff>] setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 16
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <00000001> >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 0, 255));
    // 12 bits (a PLRM Level 2 depth): 0xfff 0x000 packed as ff f0 00.
    let mut it = page();
    it.run_str(
        "/DeviceGray setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 12
            /ImageMatrix [2 0 0 -1 0 1] /DataSource <fff000> >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 30, 50), (255, 255, 255));
    assert_eq!(pixel(&it, 70, 50), (0, 0, 0));
}

#[test]
fn multi_source_colorimage_matches_one_interleaved_source() {
    same(
        &format!("10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {RGB8} false 3 colorimage"),
        &format!(
            "10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {R8} {G8} {B8} true 3 colorimage"
        ),
    );
    // The first source is the first component: swapping red and blue swaps
    // the colours.
    let mut it = page();
    it.run_str(&format!(
        "10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {R8} {G8} {B8} true 3 colorimage"
    ))
    .expect("runs");
    assert_eq!(pixel(&it, 30, 30), (255, 0, 0), "red from the first source");
    assert_eq!(pixel(&it, 70, 30), (0, 255, 0));
    assert_eq!(pixel(&it, 30, 70), (0, 0, 255), "blue from the third");
    assert_eq!(pixel(&it, 70, 70), (128, 128, 128));
}

#[test]
fn multi_source_sixteen_bit_and_cmyk_and_gray() {
    same(
        &format!(
            "10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {R8} {G8} {B8} true 3 colorimage"
        ),
        &format!(
            "10 10 translate 80 80 scale 2 2 16 [2 0 0 -2 0 2] {R16} {G16} {B16} true 3 colorimage"
        ),
    );
    // CMYK, four sources (K last), against the interleaved form.
    same(
        "10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1]
         <ff00> <00ff> <0000> <4080> true 4 colorimage",
        "10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1]
         <ff000040 00ff0080> false 4 colorimage",
    );
    // One component: `multi true` with ncomp 1 is the plain gray image.
    same(
        "10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1] <00ff> true 1 colorimage",
        "10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1] <00ff> false 1 colorimage",
    );
}

#[test]
fn multiple_data_sources_dict_matches_colorimage_multi() {
    same(
        &format!(
            "10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2] {R8} {G8} {B8} true 3 colorimage"
        ),
        &format!(
            "/DeviceRGB setcolorspace 10 10 translate 80 80 scale
             << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
                /ImageMatrix [2 0 0 -2 0 2] /MultipleDataSources true
                /DataSource [{R8} {G8} {B8}] >> image"
        ),
    );
    // Decode applies per component after the planes are read.
    let mut it = page();
    it.run_str(
        "/DeviceRGB setcolorspace 10 10 translate 80 80 scale
         << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
            /ImageMatrix [1 0 0 -1 0 1] /MultipleDataSources true
            /Decode [1 0 0 1 0 1] /DataSource [<ff> <ff> <00>] >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 50, 50), (0, 255, 0));
}

#[test]
fn multiple_data_sources_inside_imagetype3_data_dict() {
    let mut it = page();
    it.run_str(
        "/DeviceRGB setcolorspace 10 10 translate 80 80 scale
         << /ImageType 3 /InterleaveType 3
            /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
               /ImageMatrix [2 0 0 -1 0 1] /MultipleDataSources true
               /DataSource [<ff00> <00ff> <0000>] >>
            /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1
               /Decode [1 0] /ImageMatrix [2 0 0 -1 0 1] /DataSource <80> >> >> image",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0), "mask passes the left cell");
    assert_eq!(pixel(&it, 70, 50), (255, 255, 255), "and blocks the right");
}

// ---- the order sources are read in -------------------------------------------

#[test]
fn procedures_are_called_component_by_component_for_each_row() {
    // gs calls red, green, blue, red, green, blue ... one row each; so do we.
    // Three named procedures, each with its own row counter and a shared log.
    let mut it = page();
    it.run_str(
        "/log [] def
         /cR 0 def /cG 0 def /cB 0 def
         /pR { /log [ log aload pop /R ] def cR 2 ge { () } { /cR cR 1 add def (\\377\\377) } ifelse } def
         /pG { /log [ log aload pop /G ] def cG 2 ge { () } { /cG cG 1 add def (\\377\\377) } ifelse } def
         /pB { /log [ log aload pop /B ] def cB 2 ge { () } { /cB cB 1 add def (\\377\\377) } ifelse } def
         10 10 translate 80 80 scale 2 2 8 [2 0 0 -2 0 2]
         /pR load /pG load /pB load true 3 colorimage
         log",
    )
    .expect("runs");
    assert_eq!(it.pop().expect("log").repr(), "[/R /G /B /R /G /B]");
}

#[test]
fn inline_filters_are_read_one_after_another_from_the_program() {
    // Three ASCIIHex filters on currentfile: red's data first, then green's,
    // then blue's, each ended by its own `>`, then the program resumes.
    let mut it = page();
    it.run_str(
        "10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1]
         currentfile /ASCIIHexDecode filter
         currentfile /ASCIIHexDecode filter
         currentfile /ASCIIHexDecode filter true 3 colorimage
         ff00>
         00ff>
         0000>
         initmatrix 0 0 1 setrgbcolor 4 setlinewidth newpath 2 95 moveto 8 95 lineto stroke",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 255, 0));
    assert_eq!(
        pixel(&it, 5, 5),
        (0, 0, 255),
        "program resumed after the data"
    );
}

#[test]
fn short_planes_read_as_zero_like_a_short_single_source() {
    let mut it = page();
    it.run_str("10 10 translate 80 80 scale 2 1 8 [2 0 0 -1 0 1] <ff> <ff> <> true 3 colorimage")
        .expect("runs");
    // Left cell: red and green from data, blue missing (0): yellow.
    assert_eq!(pixel(&it, 30, 50), (255, 255, 0));
}

// ---- errors, as gs 10.07 gives them -------------------------------------------

#[test]
fn argument_errors_match_gs() {
    let run = |src: &str| page().run_str(src);
    // A wrong number of sources for the component count.
    assert_eq!(
        run(
            "/DeviceRGB setcolorspace << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
             /MultipleDataSources true /ImageMatrix [2 0 0 -2 0 2]
             /DataSource [<ff00ff00> <00ff00ff>] >> image"
        ),
        Err(PsError::Rangecheck)
    );
    // MultipleDataSources false with an array (not a procedure).
    assert_eq!(
        run(
            "/DeviceRGB setcolorspace << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
             /MultipleDataSources false /ImageMatrix [2 0 0 -2 0 2]
             /DataSource [<ff00ff00> <00ff00ff> <0000ffff>] >> image"
        ),
        Err(PsError::Typecheck)
    );
    // MultipleDataSources true with a single string.
    assert_eq!(
        run(
            "/DeviceRGB setcolorspace << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
             /MultipleDataSources true /ImageMatrix [2 0 0 -2 0 2]
             /DataSource <ff00ff00> >> image"
        ),
        Err(PsError::Typecheck)
    );
    // colorimage takes 1, 3 or 4 components.
    assert_eq!(
        run("2 2 8 [2 0 0 -2 0 2] <ff00ff00> <00ff00ff> true 2 colorimage"),
        Err(PsError::Rangecheck)
    );
    // Depths other than 1 2 4 8 12 16.
    assert_eq!(
        run("2 2 3 [2 0 0 -2 0 2] {<00>} false 1 colorimage"),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        run("2 2 32 [2 0 0 -2 0 2] {<00>} image"),
        Err(PsError::Rangecheck)
    );
}
