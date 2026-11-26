//! Painting a host raster under the current transformation (#1407):
//! `Interp::draw_page` places a `Page` the host holds as `image` places a
//! program's samples, through the CTM, clip, `setalpha` and `setblendmode`,
//! on the layer of an open transparency group, and into SVG and PDF output.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pscat::host::{MAX_EMBEDDED_RASTER_PIXELS, Sampling};
use pscat::{Interp, Limits, Page, PsError};

const SIZE: u32 = 20;
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn canvas() -> Interp {
    Interp::with_page(SIZE, SIZE).expect("valid page size")
}

/// A page whose pixel at column `x`, row `y` is `f(x, y)`, given as straight
/// RGBA.
fn layer(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Page {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b, a] = f(x, y);
            let pre = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
            data.extend_from_slice(&[pre(r), pre(g), pre(b), a]);
        }
    }
    Page::from_premultiplied_rgba8(w, h, data).expect("valid size")
}

/// Four distinct opaque quadrants of a 2 x 2 raster: red, green on the first
/// row; blue, yellow on the second.
fn quadrants() -> Page {
    layer(2, 2, |x, y| match (x, y) {
        (0, 0) => [255, 0, 0, 255],
        (1, 0) => [0, 255, 0, 255],
        (0, 1) => [0, 0, 255, 255],
        _ => [255, 255, 0, 255],
    })
}

fn at(interp: &Interp, x: usize, y: usize) -> [u8; 4] {
    let (w, _) = interp.canvas_size();
    let i = (y * w as usize + x) * 4;
    let c = interp.canvas_premultiplied_rgba8();
    [c[i], c[i + 1], c[i + 2], c[i + 3]]
}

fn pixels(interp: &Interp) -> Vec<u8> {
    interp.canvas_premultiplied_rgba8().to_vec()
}

#[test]
fn a_one_to_one_placement_reproduces_the_raster() {
    let src = layer(SIZE, SIZE, |x, y| [(x * 12) as u8, (y * 12) as u8, 77, 255]);
    let mut interp = canvas();
    interp.run_str("20 20 scale").unwrap();
    interp.draw_page(&src, Sampling::Nearest).unwrap();
    assert_eq!(pixels(&interp), src.premultiplied_rgba8());
}

#[test]
fn the_first_row_is_the_top_and_agrees_with_image() {
    let mut interp = canvas();
    interp.run_str("20 20 scale").unwrap();
    interp.draw_page(&quadrants(), Sampling::Nearest).unwrap();
    assert_eq!(at(&interp, 2, 2), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 17, 2), [0, 255, 0, 255]);
    assert_eq!(at(&interp, 2, 17), [0, 0, 255, 255]);
    assert_eq!(at(&interp, 17, 17), [255, 255, 0, 255]);

    // The same samples through `image`, with the same transformation.
    let mut reference = canvas();
    reference
        .run_str(
            "20 20 scale 2 2 8 [2 0 0 -2 0 2] <ff0000 00ff00 0000ff ffff00> false 3 colorimage",
        )
        .unwrap();
    assert_eq!(pixels(&interp), pixels(&reference));
}

#[test]
fn a_rotated_scaled_placement_lands_where_image_puts_it() {
    let setup = "10 10 translate 30 rotate 12 12 scale -0.5 -0.5 translate";
    let mut interp = canvas();
    interp.run_str(setup).unwrap();
    interp.draw_page(&quadrants(), Sampling::Nearest).unwrap();

    let mut reference = canvas();
    reference
        .run_str(&format!(
            "{setup} 2 2 8 [2 0 0 -2 0 2] <ff0000 00ff00 0000ff ffff00> false 3 colorimage"
        ))
        .unwrap();
    // Away from the footprint's edge (which `image` leaves unsmoothed and
    // the placement anti-aliases) the two agree; the corners of the canvas
    // are untouched by both.
    let (mine, theirs) = (pixels(&interp), pixels(&reference));
    assert_eq!(at(&interp, 0, 0), WHITE);
    assert_eq!(at(&interp, 19, 19), WHITE);
    // Wherever the placement paints a whole quadrant colour (a pixel the
    // footprint covers entirely), `image` painted the same colour: they
    // sample alike. Only the anti-aliased ring of edge pixels differs.
    let pure = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 0, 255],
    ];
    let (mut whole, mut disagree, mut painted) = (0, 0, 0);
    for (a, b) in mine.chunks(4).zip(theirs.chunks(4)) {
        painted += usize::from(a != WHITE);
        if pure.contains(&[a[0], a[1], a[2], a[3]]) {
            whole += 1;
            disagree += usize::from(a != b);
        }
    }
    assert!(
        painted > 100 && whole > 80,
        "painted {painted}, whole {whole}"
    );
    assert_eq!(disagree, 0, "{disagree} of {whole} whole pixels differ");
    // Each quadrant's colour is where the rotation puts it.
    assert_eq!(at(&interp, 10, 13), at(&reference, 10, 13));
    assert_eq!(at(&interp, 7, 7), at(&reference, 7, 7));
    assert_eq!(at(&interp, 13, 7), at(&reference, 13, 7));
}

#[test]
fn the_clip_applies_a_rectangle_and_a_curve() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    let mut interp = canvas();
    interp.run_str("0 0 10 20 rectclip 20 20 scale").unwrap();
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    assert_eq!(at(&interp, 3, 10), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 15, 10), WHITE);

    let mut interp = canvas();
    interp
        .run_str("gsave 10 10 6 0 360 arc clip 20 20 scale")
        .unwrap();
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    assert_eq!(at(&interp, 10, 10), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 1, 1), WHITE);
    assert_eq!(at(&interp, 18, 18), WHITE);
}

#[test]
fn alpha_and_the_rasters_own_transparency_compose() {
    let src = layer(2, 1, |x, _| {
        if x == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 0, 255, 0]
        }
    });
    let mut interp = canvas();
    interp.run_str("0.5 setalpha 20 20 scale").unwrap();
    interp.draw_page(&src, Sampling::Nearest).unwrap();
    // Half-strength red over white; the transparent half paints nothing.
    let [r, g, b, a] = at(&interp, 3, 10);
    assert_eq!((r, a), (255, 255));
    assert!(
        (126..=129).contains(&g) && (126..=129).contains(&b),
        "{g} {b}"
    );
    assert_eq!(at(&interp, 15, 10), WHITE);

    // A half-transparent pixel over white, at full alpha.
    let half = layer(1, 1, |_, _| [0, 0, 0, 128]);
    let mut interp = canvas();
    interp.run_str("20 20 scale").unwrap();
    interp.draw_page(&half, Sampling::Nearest).unwrap();
    let [r, _, _, a] = at(&interp, 3, 3);
    assert_eq!(a, 255);
    assert!((126..=129).contains(&r), "{r}");
}

#[test]
fn the_blend_mode_applies() {
    let grey = layer(1, 1, |_, _| [128, 128, 128, 255]);
    let mut interp = canvas();
    interp
        .run_str("0.5 0.5 0.5 setrgbcolor 0 0 20 20 rectfill /Multiply setblendmode 20 20 scale")
        .unwrap();
    let before = at(&interp, 3, 3);
    interp.draw_page(&grey, Sampling::Nearest).unwrap();
    let after = at(&interp, 3, 3);
    // Multiply darkens: about 0.5 * 0.5.
    assert!(after[0] < before[0] - 40, "{before:?} -> {after:?}");
    assert!((55..=75).contains(&after[0]), "{after:?}");
}

#[test]
fn bilinear_blends_where_nearest_keeps_the_edge() {
    let bw = layer(2, 1, |x, _| {
        if x == 0 {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    });
    let mut near = canvas();
    near.run_str("20 20 scale").unwrap();
    near.draw_page(&bw, Sampling::Nearest).unwrap();
    let mut smooth = canvas();
    smooth.run_str("20 20 scale").unwrap();
    smooth.draw_page(&bw, Sampling::Bilinear).unwrap();
    // Nearest: a hard step at column 10. Bilinear: grey around it.
    assert_eq!(at(&near, 9, 5), [0, 0, 0, 255]);
    assert_eq!(at(&near, 10, 5), WHITE);
    let (a, b) = (at(&smooth, 9, 5)[0], at(&smooth, 10, 5)[0]);
    assert!(a > 20 && a < 235 && b > 20 && b < 235 && a < b, "{a} {b}");
}

#[test]
fn nothing_painted_is_not_an_error() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    for setup in [
        "0 0 scale",
        "1e30 0 scale",
        "100 100 translate 20 20 scale",
        "-100 -100 translate 20 20 scale",
        "nulldevice 20 20 scale",
    ] {
        let mut interp = canvas();
        interp.run_str(setup).unwrap();
        interp.draw_page(&red, Sampling::Nearest).unwrap();
        assert!(pixels(&interp).chunks(4).all(|p| p == WHITE), "{setup}");
    }
}

#[test]
fn an_enormous_transformation_paints_and_does_not_crash() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    for setup in [
        "-1e9 -1e9 translate 2e9 2e9 scale",
        "1e9 1e9 scale",
        "-5e8 -5e8 translate 1e9 1e9 scale 45 rotate",
        "1e30 1e30 scale",
    ] {
        let mut interp = canvas();
        interp.run_str(setup).unwrap();
        interp.draw_page(&red, Sampling::Nearest).unwrap();
        interp.draw_page(&red, Sampling::Bilinear).unwrap();
    }
    // A footprint that covers the canvas paints all of it.
    let mut interp = canvas();
    interp.run_str("-1e9 -1e9 translate 2e9 2e9 scale").unwrap();
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    assert!(pixels(&interp).chunks(4).all(|p| p == [255, 0, 0, 255]));
}

#[test]
fn a_host_operator_places_a_layer_at_the_programs_pose() {
    let mut interp = canvas();
    let src = Rc::new(quadrants());
    interp
        .register_operator("place", move |i| i.draw_page(&src, Sampling::Nearest))
        .unwrap();
    interp
        .run_str("gsave 10 10 translate 90 rotate 8 8 scale -0.5 -0.5 translate place grestore")
        .unwrap();
    // Turned a quarter: the red (first row, first column) quadrant is now
    // at the lower left of the square it was upper left in.
    assert_eq!(at(&interp, 7, 13), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 13, 13), [0, 0, 255, 255]);
    assert_eq!(at(&interp, 0, 0), WHITE);
}

// ---- groups ----

#[test]
fn a_raster_drawn_in_a_group_goes_to_its_layer_and_the_host_sees_the_page_below() {
    let seen = Rc::new(Cell::new([0u8; 4]));
    let mut interp = canvas();
    let s = seen.clone();
    interp
        .register_operator("peek", move |i| {
            s.set(i.canvas_premultiplied_rgba8()[..4].try_into().unwrap());
            Ok(())
        })
        .unwrap();
    let src = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    interp
        .register_operator("place", move |i| i.draw_page(&src, Sampling::Nearest))
        .unwrap();
    interp
        .run_str("<< /Alpha 0.5 >> begingroup 20 20 scale place peek endgroup")
        .unwrap();
    // While the group was open the page below was untouched ...
    assert_eq!(seen.get(), WHITE);
    // ... and the group composited the raster at its own alpha.
    let [r, g, b, a] = at(&interp, 5, 5);
    assert_eq!((r, a), (255, 255));
    assert!(
        (126..=129).contains(&g) && (126..=129).contains(&b),
        "{g} {b}"
    );
}

#[test]
fn a_raster_drawn_in_a_group_is_clipped_by_the_clip_at_begingroup() {
    let red = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = canvas();
    interp
        .register_operator("place", move |i| i.draw_page(&red, Sampling::Nearest))
        .unwrap();
    interp
        .run_str("0 0 10 20 rectclip << >> begingroup 20 20 scale place endgroup")
        .unwrap();
    assert_eq!(at(&interp, 3, 10), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 15, 10), WHITE);
}

#[test]
fn a_host_raster_can_be_a_page_read_back_from_the_canvas() {
    // psanim's loop: paint a layer, take it, place it twice at poses.
    let mut interp = canvas();
    interp
        .run_str("1 0 0 setrgbcolor 0 0 20 20 rectfill")
        .unwrap();
    let held =
        Page::from_premultiplied_rgba8(SIZE, SIZE, interp.canvas_premultiplied_rgba8().to_vec())
            .unwrap();
    interp.run_str("1 setgray 0 0 20 20 rectfill").unwrap();
    interp.run_str("gsave 10 0 translate 10 20 scale").unwrap();
    interp.draw_page(&held, Sampling::Bilinear).unwrap();
    interp.run_str("grestore").unwrap();
    assert_eq!(at(&interp, 15, 10), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 4, 10), WHITE);
}

// ---- patterns ----

#[test]
fn a_raster_drawn_in_a_pattern_cell_paints_the_tile() {
    let src = Rc::new(quadrants());
    let mut interp = canvas();
    interp
        .register_operator("place", move |i| i.draw_page(&src, Sampling::Nearest))
        .unwrap();
    interp
        .run_str(
            "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
                /PaintProc { pop gsave 10 10 scale place grestore } >> matrix makepattern
             setpattern 0 0 20 20 rectfill",
        )
        .unwrap();
    // The tile repeats: the same colours every 10 pixels in each direction.
    for (x, y) in [(2, 2), (7, 2), (2, 7), (7, 7)] {
        assert_ne!(at(&interp, x, y), WHITE, "({x}, {y})");
        assert_eq!(at(&interp, x, y), at(&interp, x + 10, y), "({x}, {y})");
        assert_eq!(at(&interp, x, y), at(&interp, x, y + 10), "({x}, {y})");
    }
    assert_ne!(at(&interp, 2, 2), at(&interp, 7, 2));
}

#[test]
fn an_uncoloured_pattern_gives_a_raster_its_own_colour() {
    // As for `image` in an uncoloured `PaintProc`: the tile keeps the
    // raster's coverage and the colour set for the pattern replaces its own.
    let src = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = canvas();
    interp
        .register_operator("place", move |i| i.draw_page(&src, Sampling::Nearest))
        .unwrap();
    interp
        .run_str(
            "/P << /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
                /PaintProc { pop gsave 10 10 scale place grestore } >>
              matrix makepattern def
             [/Pattern /DeviceRGB] setcolorspace 0 0 1 P setcolor 0 0 20 20 rectfill",
        )
        .unwrap();
    assert!(pixels(&interp).chunks(4).all(|p| p == [0, 0, 255, 255]));
}

// ---- SVG and PDF ----

#[test]
fn svg_output_embeds_the_raster_at_its_placement() {
    let mut interp = canvas();
    interp.enable_svg();
    interp
        .run_str("5 5 translate 30 rotate 10 10 scale")
        .unwrap();
    interp.draw_page(&quadrants(), Sampling::Nearest).unwrap();
    interp.draw_page(&quadrants(), Sampling::Bilinear).unwrap();
    let svg = interp.svg_pages().unwrap().concat();
    assert_eq!(svg.matches("<image").count(), 2, "{svg}");
    assert!(svg.contains("data:image/png;base64,"));
    assert!(svg.contains("matrix("));
    // The nearest placement is marked as pixelated, the bilinear one is not.
    assert_eq!(svg.matches("pixelated").count(), 1);
}

#[test]
fn svg_output_carries_the_clip_alpha_and_group() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 128]);
    let mut interp = canvas();
    interp.enable_svg();
    interp
        .run_str("0 0 10 20 rectclip 0.5 setalpha << >> begingroup 20 20 scale")
        .unwrap();
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    interp.run_str("endgroup").unwrap();
    let svg = interp.svg_pages().unwrap().concat();
    assert!(svg.contains("<image"), "{svg}");
    assert!(svg.contains("clip"), "{svg}");
    assert!(svg.contains("opacity"), "{svg}");
}

#[test]
fn pdf_output_embeds_the_raster_with_a_soft_mask_only_when_it_has_transparency() {
    let mut opaque = canvas();
    opaque.enable_pdf();
    opaque.run_str("20 20 scale").unwrap();
    opaque.draw_page(&quadrants(), Sampling::Nearest).unwrap();
    let pdf = String::from_utf8_lossy(&opaque.pdf_document().unwrap()).into_owned();
    assert!(pdf.contains("/Subtype /Image"), "{pdf}");
    assert!(!pdf.contains("/SMask"));

    let ghost = layer(2, 1, |x, _| [10, 20, 30, if x == 0 { 255 } else { 0 }]);
    let mut clear = canvas();
    clear.enable_pdf();
    clear
        .run_str("0.5 setalpha << >> begingroup 20 20 scale")
        .unwrap();
    clear.draw_page(&ghost, Sampling::Bilinear).unwrap();
    clear.run_str("endgroup").unwrap();
    let pdf = String::from_utf8_lossy(&clear.pdf_document().unwrap()).into_owned();
    assert!(pdf.contains("/SMask"), "{pdf}");
    assert!(pdf.contains("/Interpolate true"));
}

#[test]
fn a_raster_over_the_embedding_limit_is_refused_whole_while_recording() {
    let side = (MAX_EMBEDDED_RASTER_PIXELS as f64).sqrt() as u32 + 1;
    let big = Page::transparent(side, side).unwrap();
    assert!(u64::from(side) * u64::from(side) > MAX_EMBEDDED_RASTER_PIXELS as u64);
    for record in ["svg", "pdf"] {
        let mut interp = canvas();
        match record {
            "svg" => interp.enable_svg(),
            _ => interp.enable_pdf(),
        }
        interp.run_str("20 20 scale").unwrap();
        assert_eq!(
            interp.draw_page(&big, Sampling::Nearest),
            Err(PsError::Limitcheck),
            "{record}"
        );
        assert!(pixels(&interp).chunks(4).all(|p| p == WHITE));
        if record == "svg" {
            assert!(!interp.svg_pages().unwrap().concat().contains("<image"));
        } else {
            let pdf = String::from_utf8_lossy(&interp.pdf_document().unwrap()).into_owned();
            assert!(!pdf.contains("/Subtype /Image"));
        }
    }
    // Without a recorder the page is painted in place, with no copy.
    let mut interp = canvas();
    interp.run_str("20 20 scale").unwrap();
    interp.draw_page(&big, Sampling::Nearest).unwrap();
}

// ---- limits ----

/// The step cap stops a placement before a pixel changes; the charge follows
/// the pixels reached, not the size of the transformation.
#[test]
fn a_step_cap_stops_a_placement_before_it_paints() {
    let red = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = Interp::with_page(2000, 2000).unwrap();
    let r = red.clone();
    interp
        .register_operator("place", move |i| i.draw_page(&r, Sampling::Nearest))
        .unwrap();
    // 2000 x 2000 pixels is about 3,900 steps to place.
    interp
        .set_limits(Limits::none().with_max_steps(1_000))
        .unwrap();
    let before = pixels(&interp);
    assert_eq!(
        interp.run_str("2000 2000 scale place"),
        Err(PsError::StepsExceeded)
    );
    assert_eq!(pixels(&interp), before);

    // Without the cap it paints; with a cap that covers it, so does it.
    interp
        .set_limits(Limits::none().with_max_steps(20_000))
        .unwrap();
    interp.run_str("2000 2000 scale place").unwrap();
    assert_eq!(at(&interp, 1000, 1000), [255, 0, 0, 255]);
    // A transformation a million times too big costs what the canvas costs.
    let mut interp = Interp::with_page(2000, 2000).unwrap();
    let r = red.clone();
    interp
        .register_operator("place", move |i| i.draw_page(&r, Sampling::Nearest))
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_steps(20_000))
        .unwrap();
    interp
        .run_str("-1e9 -1e9 translate 2e9 2e9 scale place")
        .unwrap();
    assert_eq!(at(&interp, 1000, 1000), [255, 0, 0, 255]);
}

#[test]
fn many_placements_meet_the_step_cap() {
    let red = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = Interp::with_page(2000, 2000).unwrap();
    interp
        .register_operator("place", move |i| i.draw_page(&red, Sampling::Nearest))
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_steps(50_000))
        .unwrap();
    assert_eq!(
        interp.run_str("2000 2000 scale 100000 { place } repeat"),
        Err(PsError::StepsExceeded)
    );
}

#[test]
fn the_interrupt_flag_stops_a_placement() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    let stop = Arc::new(AtomicBool::new(false));
    let mut interp = canvas();
    interp.set_interrupt(Some(stop.clone()));
    interp.run_str("20 20 scale").unwrap();
    stop.store(true, Ordering::Relaxed);
    assert_eq!(
        interp.draw_page(&red, Sampling::Nearest),
        Err(PsError::Interrupted)
    );
    assert!(pixels(&interp).chunks(4).all(|p| p == WHITE));
    stop.store(false, Ordering::Relaxed);
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    assert_eq!(at(&interp, 3, 3), [255, 0, 0, 255]);
}

#[test]
fn a_showpage_before_the_placement_still_erases_first() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    let mut interp = canvas();
    interp
        .run_str("0 0 1 setrgbcolor 0 0 20 20 rectfill showpage")
        .unwrap();
    interp.run_str("20 20 scale").unwrap();
    interp.draw_page(&red, Sampling::Nearest).unwrap();
    assert_eq!(at(&interp, 3, 3), [255, 0, 0, 255]);
    assert_eq!(interp.page_count(), 2);
}

// ---- Codex round 1 ----

/// Codex finding 3: an uncoloured pattern's tile is the raster's coverage in
/// the pattern's colour on the canvas; SVG and PDF must record that, not the
/// raster's own colours (SVG) or nothing (PDF).
#[test]
fn an_uncoloured_pattern_records_the_coverage_in_svg_and_pdf() {
    let red = Rc::new(layer(2, 2, |x, y| {
        // Opaque on the diagonal, clear elsewhere.
        if x == y {
            [255, 0, 0, 255]
        } else {
            [255, 0, 0, 0]
        }
    }));
    let program =
        "/P << /PatternType 1 /PaintType 2 /TilingType 1 /BBox [0 0 10 10] /XStep 10 /YStep 10
            /PaintProc { pop gsave 10 10 scale place grestore } >> matrix makepattern def
         [/Pattern /DeviceRGB] setcolorspace 0 0 1 P setcolor 0 0 20 20 rectfill";
    let run = |record: &str| {
        let mut interp = canvas();
        let src = red.clone();
        interp
            .register_operator("place", move |i| i.draw_page(&src, Sampling::Nearest))
            .unwrap();
        match record {
            "svg" => interp.enable_svg(),
            "pdf" => interp.enable_pdf(),
            _ => {}
        }
        interp.run_str(program).unwrap();
        interp
    };
    // The canvas: blue where the raster has coverage, white elsewhere, no red.
    let png = run("png");
    assert!(
        pixels(&png)
            .chunks(4)
            .all(|p| p == WHITE || p == [0, 0, 255, 255])
    );
    assert!(pixels(&png).chunks(4).any(|p| p == [0, 0, 255, 255]));
    assert!(pixels(&png).chunks(4).any(|p| p == WHITE));

    // SVG: a mask made of the coverage, filled with the tile's colour.
    let svg = run("svg").svg_pages().unwrap().concat();
    assert!(svg.contains("<mask"), "{svg}");
    assert!(svg.contains("currentColor"), "{svg}");
    // The only `<image>` is the stencil inside its `<mask>`.
    assert_eq!(
        svg.matches("<image").count(),
        svg.matches("<mask").count(),
        "{svg}"
    );

    // PDF: an image mask in the tile, which holds no colour image.
    let pdf = String::from_utf8_lossy(&run("pdf").pdf_document().unwrap()).into_owned();
    assert!(pdf.contains("/ImageMask true"), "{pdf}");
}

/// Codex finding 4: under `--bands` the fill paints only the band's rows,
/// which is the work that is charged.
#[test]
fn a_banded_placement_fills_only_the_bands_rows() {
    let red = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = Interp::with_page(100, 100).unwrap();
    interp.gfx_mut().set_band(3, 10); // rows 30..40
    let r = red.clone();
    interp
        .register_operator("place", move |i| i.draw_page(&r, Sampling::Nearest))
        .unwrap();
    interp.run_str("100 100 scale place").unwrap();
    for y in 0..100 {
        let want = if (30..40).contains(&y) {
            [255, 0, 0, 255]
        } else {
            WHITE
        };
        assert_eq!(at(&interp, 50, y), want, "row {y}");
    }
    // The same rows as an unbanded placement paints them.
    let mut whole = Interp::with_page(100, 100).unwrap();
    whole.run_str("100 100 scale").unwrap();
    whole.draw_page(&red, Sampling::Nearest).unwrap();
    assert_eq!(at(&whole, 50, 35), at(&interp, 50, 35));
}

#[test]
fn a_banded_placement_is_charged_for_the_rows_it_fills() {
    let red = Rc::new(layer(1, 1, |_, _| [255, 0, 0, 255]));
    let mut interp = Interp::with_page(3000, 3000).unwrap();
    interp.gfx_mut().set_band(1500, 3000); // one row
    interp
        .register_operator("place", move |i| i.draw_page(&red, Sampling::Nearest))
        .unwrap();
    // A row is 3000 pixels, about 3 steps; the whole canvas would be 8,800.
    interp
        .set_limits(Limits::none().with_max_steps(5_000))
        .unwrap();
    interp
        .run_str("3000 3000 scale 200 { place } repeat")
        .unwrap();
    assert_eq!(at(&interp, 10, 1500), [255, 0, 0, 255]);
    assert_eq!(at(&interp, 10, 1499), WHITE);
}

/// A raster of noise: slow to encode, so a limit can fire part way.
fn noise(side: u32) -> Page {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut data = Vec::with_capacity((side * side * 4) as usize);
    for _ in 0..side * side {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        data.extend_from_slice(&[state as u8, (state >> 8) as u8, (state >> 16) as u8, 255]);
    }
    Page::from_premultiplied_rgba8(side, side, data).unwrap()
}

/// Codex finding 5: a deadline that passes during the recorders' copies stops
/// the placement before the canvas or the recorder is written.
#[test]
fn a_deadline_that_passes_during_the_copies_stops_the_placement() {
    let big = Rc::new(noise(3000));
    let mut interp = canvas();
    interp.enable_svg();
    let b = big.clone();
    interp
        .register_operator("place", move |i| i.draw_page(&b, Sampling::Nearest))
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_time(std::time::Duration::from_millis(20)))
        .unwrap();
    assert_eq!(
        interp.run_str("20 20 scale place"),
        Err(PsError::TimeExceeded)
    );
    assert!(pixels(&interp).chunks(4).all(|p| p == WHITE));
    assert!(!interp.svg_pages().unwrap().concat().contains("<image"));
}

/// The same for the interrupt flag, raised by another thread while the
/// copies are being made.
#[test]
fn the_interrupt_flag_raised_during_the_copies_stops_the_placement() {
    let big = Rc::new(noise(3000));
    let stop = Arc::new(AtomicBool::new(false));
    let mut interp = canvas();
    interp.enable_svg();
    interp.enable_pdf();
    interp.set_interrupt(Some(stop.clone()));
    let b = big.clone();
    interp
        .register_operator("place", move |i| i.draw_page(&b, Sampling::Nearest))
        .unwrap();
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            stop.store(true, Ordering::Relaxed);
        })
    };
    assert_eq!(
        interp.run_str("20 20 scale place"),
        Err(PsError::Interrupted)
    );
    raiser.join().unwrap();
    assert!(pixels(&interp).chunks(4).all(|p| p == WHITE));
}

/// Codex finding 6: a placement that covers the canvas covers all of it, at
/// scales where cutting the corners' edges in device space loses everything.
#[test]
fn a_covering_placement_covers_the_canvas_at_any_scale() {
    let red = layer(1, 1, |_, _| [255, 0, 0, 255]);
    for setup in [
        "-1e9 -1e9 translate 2e9 2e9 scale",
        "-1e20 -1e20 translate 2e20 2e20 scale",
        "-1e30 -1e30 translate 2e30 2e30 scale",
        "10 10 translate 45 rotate -1e20 -1e20 translate 2e20 2e20 scale",
        "10 10 translate 30 rotate -1e30 -1e30 translate 2e30 2e30 scale",
        "-1e20 1e20 translate 2e20 -2e20 scale",
    ] {
        for sampling in [Sampling::Nearest, Sampling::Bilinear] {
            let mut interp = canvas();
            interp.run_str(setup).unwrap();
            interp.draw_page(&red, sampling).unwrap();
            let missing = pixels(&interp)
                .chunks(4)
                .filter(|p| *p != [255, 0, 0, 255])
                .count();
            assert_eq!(missing, 0, "{setup} {sampling:?}: {missing} pixels missed");
        }
    }
}

#[test]
fn the_blend_mode_and_alpha_reach_svg_and_pdf() {
    let grey = layer(1, 1, |_, _| [128, 128, 128, 200]);
    let mut interp = canvas();
    interp.enable_svg();
    interp.enable_pdf();
    interp
        .run_str("0.5 setalpha /Multiply setblendmode 20 20 scale")
        .unwrap();
    interp.draw_page(&grey, Sampling::Nearest).unwrap();
    let svg = interp.svg_pages().unwrap().concat();
    assert!(svg.contains("multiply"), "{svg}");
    assert!(svg.contains("opacity=\"0.5\""), "{svg}");
    let pdf = String::from_utf8_lossy(&interp.pdf_document().unwrap()).into_owned();
    assert!(pdf.contains("/BM /Multiply"), "{pdf}");
}
