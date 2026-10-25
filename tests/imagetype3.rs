//! `ImageType 3` masked images (#739), InterleaveType 3. Polarity and
//! placement were read off Ghostscript 10.07 (no MaskDict Decode paints
//! the mask's 1s; [1 0] the 1s; [0 1] the 0s).

use pscat::{Interp, PsError};
use std::process::Command;

mod common;

const BG: (u8, u8, u8) = (255, 255, 255);

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

/// The issue's repro: a 2x1 RGB image (red, green) under a 2x1 mask
/// <80> (pixels 1, 0), Decode [1 0]. `image` is applied to `dict`, drawn
/// 80x20 at (10,40).
fn t3(data_extra: &str, mask_extra: &str, mask_data: &str) -> String {
    format!(
        "10 40 translate 80 20 scale /DeviceRGB setcolorspace
         << /ImageType 3 /InterleaveType 3
            /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
               /Decode [0 1 0 1 0 1] /ImageMatrix [2 0 0 -1 0 1]
               /DataSource <ff000000ff00> {data_extra} >>
            /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1
               /ImageMatrix [2 0 0 -1 0 1] /DataSource {mask_data} {mask_extra} >> >>
         image"
    )
}

#[test]
fn issue_repro_runs_and_masks() {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.run_str(
        "{ << /ImageType 3 /InterleaveType 3
   /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8 /Decode [0 1 0 1 0 1] /ImageMatrix [2 0 0 -1 0 1] /DataSource <ff000000ff00> >>
   /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1 /Decode [1 0] /ImageMatrix [2 0 0 -1 0 1] /DataSource <80> >> >>
/DeviceRGB setcolorspace 100 100 scale image } stopped { (failed) } { (ok) } ifelse",
    )
    .expect("runs");
    assert_eq!(pixel(&it, 25, 50), (255, 0, 0), "mask 1 paints the image");
    assert_eq!(pixel(&it, 75, 50), BG, "mask 0 leaves the page");
    // The dict operand is consumed; only the (ok) string is left.
    assert_eq!(it.run_str("count 1 eq {} {1 0 div} ifelse"), Ok(()));
}

#[test]
fn mask_polarity_follows_maskdict_decode() {
    let ones = run(&t3("", "/Decode [1 0]", "<80>"));
    assert_eq!(pixel(&ones, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&ones, 70, 50), BG);
    let zeros = run(&t3("", "/Decode [0 1]", "<80>"));
    assert_eq!(pixel(&zeros, 30, 50), BG);
    assert_eq!(pixel(&zeros, 70, 50), (0, 255, 0));
    // gs-pinned: no Decode at all paints the 1s.
    let absent = run(&t3("", "", "<80>"));
    assert_eq!(pixel(&absent, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&absent, 70, 50), BG);
}

#[test]
fn mask_and_image_may_differ_in_resolution() {
    // A 2x2 color image under an 8x8 mask: a diamond-ish stencil whose
    // cells are finer than the image's. Drawn 80x80 at (10,10), so each
    // mask cell is 10x10 px and each image cell 40x40.
    let it = run("10 10 translate 80 80 scale /DeviceRGB setcolorspace
         << /ImageType 3 /InterleaveType 3
            /DataDict << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
               /ImageMatrix [2 0 0 -2 0 2] /DataSource <ff0000 00ff00 0000ff ffff00> >>
            /MaskDict << /ImageType 1 /Width 8 /Height 8 /BitsPerComponent 1
               /Decode [1 0] /ImageMatrix [8 0 0 -8 0 8]
               /DataSource <18 3c 7e ff ff 7e 3c 18> >> >>
         image");
    // Device y = 100 - user y. Mask row 0 (top) is 0x18: cols 3,4 only.
    assert_eq!(pixel(&it, 15, 15), BG, "top-left corner is masked out");
    assert_eq!(
        pixel(&it, 45, 15),
        (255, 0, 0),
        "mask col 3 over the red cell"
    );
    assert_eq!(
        pixel(&it, 55, 15),
        (0, 255, 0),
        "mask col 4 over the green cell"
    );
    assert_eq!(
        pixel(&it, 15, 45),
        (255, 0, 0),
        "full mask row shows the image"
    );
    assert_eq!(pixel(&it, 85, 45), (0, 255, 0));
    assert_eq!(
        pixel(&it, 15, 55),
        (0, 0, 255),
        "image row 1 starts at y=50"
    );
    assert_eq!(pixel(&it, 15, 85), BG, "bottom-left corner masked out");
}

#[test]
fn both_data_sources_can_be_procedures() {
    // gs reads the mask's source first, then the image's (probed with
    // two procedures). One shared counter makes that order the spec.
    let it = run(
        "/n 0 def
         10 40 translate 80 20 scale /DeviceRGB setcolorspace
         << /ImageType 3 /InterleaveType 3
            /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
               /ImageMatrix [2 0 0 -1 0 1]
               /DataSource { /n n 1 add def n 2 eq { <ff0000> } { n 3 eq { <00ff00> } { () } ifelse } ifelse } >>
            /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1
               /Decode [1 0] /ImageMatrix [2 0 0 -1 0 1]
               /DataSource { /n n 1 add def n 1 eq { <c0> } { () } ifelse } >> >>
         image",
    );
    assert_eq!(pixel(&it, 30, 50), (255, 0, 0));
    assert_eq!(pixel(&it, 70, 50), (0, 255, 0), "both planes painted");
}

#[test]
fn a_filtered_mask_is_drained_before_the_image_reads() {
    // The mask is `currentfile /ASCIIHexDecode filter`, the image reads the
    // same file through its own filter. The mask has only 1 byte of
    // samples, so its filter has not seen its `>` yet when the image starts.
    let it = run("10 40 translate 80 20 scale /DeviceRGB setcolorspace
         << /ImageType 3 /InterleaveType 3
            /DataDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 8
               /ImageMatrix [2 0 0 -1 0 1] /DataSource currentfile /ASCIIHexDecode filter >>
            /MaskDict << /ImageType 1 /Width 2 /Height 1 /BitsPerComponent 1
               /Decode [1 0] /ImageMatrix [2 0 0 -1 0 1]
               /DataSource currentfile /ASCIIHexDecode filter >> >> image
         80> ff000000ff00>");
    assert_eq!(
        pixel(&it, 30, 50),
        (255, 0, 0),
        "image data not eaten by the mask's EOD"
    );
    assert_eq!(pixel(&it, 70, 50), BG);
}

#[test]
fn svg_keeps_a_mask_edge_that_falls_inside_an_image_cell() {
    // A solid 1x1 image under a 1x1 mask shifted half a cell: only the
    // right half of the image shows. Equal densities, so the composite
    // must still be finer than 1x1 to say so.
    let src = "10 10 translate 80 80 scale /DeviceRGB setcolorspace
        << /ImageType 3 /InterleaveType 3
           /DataDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
              /ImageMatrix [1 0 0 -1 0 1] /DataSource <ff0000> >>
           /MaskDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 1
              /Decode [1 0] /ImageMatrix [1 0 0 -1 -0.5 1] /DataSource <80> >> >> image";
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(src).expect("runs");
    let svg = &it.gfx().svg_pages().expect("recording")[0];
    assert!(svg.contains("<image width=\"4\" height=\"4\""), "{svg}");
    assert_eq!(pixel(&it, 20, 50), BG, "left half is masked on the canvas");
    assert_eq!(pixel(&it, 80, 50), (255, 0, 0));
}

#[test]
fn interleave_types_1_and_2_are_a_documented_gap() {
    for (ty, want) in [(1, PsError::Limitcheck), (2, PsError::Limitcheck)] {
        let mut it = Interp::with_page(100, 100).expect("page");
        let src = format!(
            "/DeviceRGB setcolorspace << /ImageType 3 /InterleaveType {ty}
              /DataDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
                 /ImageMatrix [1 0 0 -1 0 1] /DataSource <ff0000> >>
              /MaskDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 1
                 /ImageMatrix [1 0 0 -1 0 1] /DataSource <80> >> >> image"
        );
        assert_eq!(it.run_str(&src), Err(want), "InterleaveType {ty}");
    }
    let mut it = Interp::with_page(100, 100).expect("page");
    assert_eq!(
        it.run_str("<< /ImageType 3 /InterleaveType 7 >> image"),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        it.run_str("<< /ImageType 3 /InterleaveType 3 /DataDict 1 >> image"),
        Err(PsError::Typecheck)
    );
    // A mask has one bit per sample.
    let src = "/DeviceRGB setcolorspace << /ImageType 3 /InterleaveType 3
        /DataDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
           /ImageMatrix [1 0 0 -1 0 1] /DataSource <ff0000> >>
        /MaskDict << /ImageType 1 /Width 1 /Height 1 /BitsPerComponent 8
           /ImageMatrix [1 0 0 -1 0 1] /DataSource <80> >> >> image";
    assert_eq!(it.run_str(src), Err(PsError::Rangecheck));
}

// ---- backends ----------------------------------------------------------

const BACKEND_PROG: &str = "0.9 0.9 0.6 setrgbcolor 0 0 100 100 rectfill
    10 10 translate 80 80 scale /DeviceRGB setcolorspace
    << /ImageType 3 /InterleaveType 3
       /DataDict << /ImageType 1 /Width 2 /Height 2 /BitsPerComponent 8
          /ImageMatrix [2 0 0 -2 0 2] /DataSource <ff0000 00ff00 0000ff ffff00> >>
       /MaskDict << /ImageType 1 /Width 8 /Height 8 /BitsPerComponent 1
          /Decode [1 0] /ImageMatrix [8 0 0 -8 0 8]
          /DataSource <18 3c 7e ff ff 7e 3c 18> >> >> image";

#[test]
fn svg_embeds_one_composited_png_at_the_masks_resolution() {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_svg();
    it.run_str(BACKEND_PROG).expect("runs");
    let svg = &it.gfx().svg_pages().expect("recording")[0];
    // 2x2 image expanded 4x to cover the 8x8 mask.
    assert!(svg.contains("<image width=\"8\" height=\"8\""), "{svg}");
    assert_eq!(svg.matches("<image ").count(), 1);
}

#[test]
fn pdf_carries_a_soft_mask_and_gs_agrees_with_the_canvas() {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_str(BACKEND_PROG).expect("runs");
    let pdf = it.gfx().pdf_document().expect("recording");
    let text = String::from_utf8_lossy(&pdf);
    assert!(text.contains("/SMask "), "image has a soft mask");
    assert!(text.contains("/ColorSpace /DeviceGray"), "mask plane");
    // Only the colour image is listed as a page resource.
    assert_eq!(
        text.matches("/Subtype /Image").count(),
        2,
        "image + mask plane"
    );
    assert!(
        text.contains("/XObject << /Im1 ") && !text.contains("/Im0 "),
        "only the colour image is a page resource: {text}"
    );
    if !Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        eprintln!("skipping gs raster check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat_it3");
    let (pdf_path, png_path) = (dir.join("t.pdf"), dir.join("t.png"));
    std::fs::write(&pdf_path, &pdf).expect("write pdf");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-sDEVICE=png16m",
            "-g100x100",
            "-r72",
        ])
        .arg(format!("-o{}", png_path.display()))
        .arg(&pdf_path)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected our PDF");
    let theirs =
        tiny_skia::Pixmap::decode_png(&std::fs::read(&png_path).expect("png")).expect("decode");
    let _ = std::fs::remove_file(&pdf_path);
    let _ = std::fs::remove_file(&png_path);
    let mut bad = 0;
    for y in 0..100 {
        for x in 0..100 {
            let a = it.gfx().pixmap.pixel(x, y).expect("px");
            let b = theirs.pixel(x, y).expect("px");
            let d = |p: u8, q: u8| i32::from(p).abs_diff(i32::from(q));
            if d(a.red(), b.red()) + d(a.green(), b.green()) + d(a.blue(), b.blue()) > 60 {
                bad += 1;
            }
        }
    }
    assert!(
        bad < 40,
        "{bad} pixels differ between canvas and gs's PDF render"
    );
}
