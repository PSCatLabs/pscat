//! The photo-to-line-etching library (issue #15, `lib/etching.ps`):
//! JPEG header scanning (`et-dims`) and the tone-driven hatch renderer
//! (`et-draw`). Ink placement is a deterministic function of the
//! source photo's decoded samples (no randomness involved), so unlike
//! `lib/handscript.ps`'s jittered ink this is asserted on directly —
//! coverage should track darkness, and known-bad input should fail
//! cleanly rather than hang or panic.

use pscat::{Interp, PsError};

mod common;

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/etching.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("etching.ps failed: {}", it.error_report(&e)));
    it
}

fn dims(it: &mut Interp, photo: &str) -> (i64, i64, i64) {
    it.run_str(&format!("clear << /Photo ({photo}) >> et-dims"))
        .unwrap_or_else(|e| panic!("et-dims failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let parse = |i: usize| -> i64 {
        stack[i]
            .repr()
            .parse()
            .unwrap_or_else(|_| panic!("expected an integer, got {}", stack[i].repr()))
    };
    let n = stack.len();
    assert_eq!(n, 3, "et-dims should leave width height ncomp");
    (parse(n - 3), parse(n - 2), parse(n - 1))
}

#[test]
fn et_dims_reads_known_fixtures() {
    let mut it = with_lib(10, 10);
    assert_eq!(dims(&mut it, "tests/data/gray8.jpg"), (8, 8, 1));
    assert_eq!(dims(&mut it, "tests/data/red4.jpg"), (4, 4, 3));
    assert_eq!(dims(&mut it, "tests/data/sphere.jpg"), (200, 150, 3));
}

#[test]
fn et_dims_rejects_non_jpeg_data() {
    let mut it = with_lib(10, 10);
    let tmp = common::scratch_dir("etching").join("not_a_jpeg.jpg");
    std::fs::write(&tmp, b"not a jpeg at all, just text").unwrap();
    let err = it
        .run_str(&format!("<< /Photo ({}) >> et-dims", tmp.to_str().unwrap()))
        .expect_err("garbage input must not parse as a JPEG");
    assert!(
        matches!(&err, PsError::Undefined(name) if name.contains("et-jpeg-not-a-jpeg")),
        "expected the et-jpeg-not-a-jpeg guard, got {err:?}"
    );
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn et_dims_rejects_truncated_jpeg() {
    let mut it = with_lib(10, 10);
    let tmp = common::scratch_dir("etching").join("truncated.jpg");
    std::fs::write(&tmp, [0xFF, 0xD8, 0xFF]).unwrap();
    let err = it
        .run_str(&format!("<< /Photo ({}) >> et-dims", tmp.to_str().unwrap()))
        .expect_err("a file cut off mid-header must not hang or panic");
    assert!(
        matches!(&err, PsError::Undefined(name) if name.contains("et-jpeg-truncated")),
        "expected the et-jpeg-truncated guard, got {err:?}"
    );
    std::fs::remove_file(&tmp).ok();
}

#[test]
fn et_dims_rejects_a_corrupt_segment_length() {
    // A marker segment's length field includes itself, so it must be
    // >= 2; a corrupted length of 0 or 1 would otherwise send `len 2
    // sub` negative into `et-readn`/`et-skip`'s `string`, which raises
    // a raw rangecheck instead of a named, catchable JPEG error like
    // every other malformed-input case in this scan.
    let mut it = with_lib(10, 10);
    let tmp = common::scratch_dir("etching").join("bad_length.jpg");
    std::fs::write(&tmp, [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x01]).unwrap();
    let err = it
        .run_str(&format!("<< /Photo ({}) >> et-dims", tmp.to_str().unwrap()))
        .expect_err("a marker with length < 2 must not raise a raw rangecheck");
    assert!(
        matches!(&err, PsError::Undefined(name) if name.contains("et-jpeg-bad-marker")),
        "expected the et-jpeg-bad-marker guard, got {err:?}"
    );
    std::fs::remove_file(&tmp).ok();
}

/// Count non-paper pixels in a page-space rectangle — a proxy for ink
/// coverage that doesn't depend on exact stroke placement.
fn ink_coverage(it: &Interp, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let gfx = it.gfx();
    let mut inked = 0u32;
    let mut total = 0u32;
    for y in y0..y1 {
        for x in x0..x1 {
            let Some(p) = gfx.pixmap.pixel(x, y) else {
                continue;
            };
            total += 1;
            // Paper is a light off-white; anything noticeably darker
            // than that is a hatch stroke.
            if p.red() < 200 {
                inked += 1;
            }
        }
    }
    f64::from(inked) / f64::from(total.max(1))
}

#[test]
fn darker_regions_get_denser_hatching() {
    // sphere.jpg (tests/data/sphere.jpg): a vertical-gradient backdrop,
    // darkest along the top edge, lightest near the bottom. Sample
    // corner strips clear of the sphere itself (which sits roughly
    // x 40..150 of a 200-wide page) so only the backdrop is measured.
    let mut it = with_lib(200, 150);
    it.run_str(
        "<< /Photo (tests/data/sphere.jpg) /PageWidth 200 /PageHeight 150 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    let dark_strip = ink_coverage(&it, 0, 0, 35, 15);
    let light_strip = ink_coverage(&it, 0, 135, 35, 150);
    assert!(
        dark_strip > light_strip,
        "the darker backdrop band should hatch denser: dark={dark_strip} light={light_strip}"
    );
}

#[test]
fn et_draw_does_not_leak_the_operand_stack() {
    // Regression test for a real bug found by issue #17's lint mode
    // against a real example (examples/etching_demo.ps): et-hatch's
    // in-bounds check computes an x-in-range and a y-in-range boolean
    // but only combined them with one `and` where it needed two,
    // leaking one stray boolean per sample point -- tens of thousands
    // over a real hatch pass.
    let mut it = with_lib(200, 150);
    it.run_str(
        "<< /Photo (tests/data/sphere.jpg) /PageWidth 200 /PageHeight 150 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "{} item(s) leaked on the operand stack",
        it.operand_stack().len()
    );
}

#[test]
fn a_bright_highlight_stays_mostly_paper() {
    // The sphere's specular highlight (built into the generator around
    // its lit side) should be too bright to hatch at all — the
    // /Threshold2 default of 0.55 and the primary pass's own low floor
    // both gate on darkness, so pure highlight stays paper-white.
    let mut it = with_lib(200, 150);
    it.run_str(
        "<< /Photo (tests/data/sphere.jpg) /PageWidth 200 /PageHeight 150 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    // The highlight sits upper-left of the ball's center.
    let highlight = ink_coverage(&it, 60, 35, 100, 58);
    assert!(
        highlight < 0.25,
        "the specular highlight should render mostly as paper, got {highlight}"
    );
}

#[test]
fn photo_orientation_is_not_flipped() {
    // tests/data/topdark.jpg: dark top rows over a light field. The
    // decoded sample buffer's rows run top-down (row 0 first out of
    // the JPEG) but PostScript's y runs bottom-up — a caught-in-review
    // bug rendered every photo upside down until et-hatch's sy mapping
    // flipped for that. ink_coverage's y here is raster space (row 0 =
    // top of the PNG, same as gfx.pixmap always uses), so a correctly
    // oriented render must be denser near the top than the bottom.
    let mut it = with_lib(64, 64);
    it.run_str(
        "<< /Photo (tests/data/topdark.jpg) /PageWidth 64 /PageHeight 64 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    let top = ink_coverage(&it, 0, 0, 64, 20);
    let bottom = ink_coverage(&it, 0, 44, 64, 64);
    assert!(
        top > bottom,
        "the dark source rows are at the top; rendered top should be denser: top={top} bottom={bottom}"
    );
}

#[test]
fn et_dims_honors_exif_orientation_swap() {
    // tests/data/cornerdark_exif6.jpg: raw SOF is 80x40 (see the
    // generator comment on the fixture itself), tagged with EXIF
    // Orientation 6 (rotate 90 CW) — a 90/270-rotation orientation, so
    // the *displayed* size swaps to 40x80.
    let mut it = with_lib(10, 10);
    assert_eq!(
        dims(&mut it, "tests/data/cornerdark_exif6.jpg"),
        (40, 80, 1)
    );
}

#[test]
fn et_draw_honors_exif_orientation_rotation() {
    // tests/data/cornerdark_exif6.jpg: a dark block confined to the
    // raw top-left quadrant only (not a full-width/height stripe) —
    // deliberately asymmetric in both axes, because orientations
    // 5/6/7/8 each rotate the raw top-left corner to a *different* one
    // of the four display corners (verified by hand against
    // et-orient-map's transforms: 5->top-left, 6->top-right,
    // 7->bottom-right, 8->bottom-left). A stripe-only fixture can't
    // tell a 5/6/7/8 mixup from a correct orientation 6 — this one
    // can, since only orientation 6 puts the dark block in the
    // top-right corner asserted below.
    let mut it = with_lib(40, 80);
    it.run_str(
        "<< /Photo (tests/data/cornerdark_exif6.jpg) /PageWidth 40 /PageHeight 80 >> et-draw showpage",
    )
    .unwrap_or_else(|e| panic!("et-draw failed: {}", it.error_report(&e)));
    let top_right = ink_coverage(&it, 30, 0, 40, 20);
    let top_left = ink_coverage(&it, 0, 0, 20, 20);
    let bottom_left = ink_coverage(&it, 0, 60, 20, 80);
    let bottom_right = ink_coverage(&it, 20, 60, 40, 80);
    assert!(
        top_right > top_left && top_right > bottom_left && top_right > bottom_right,
        "orientation 6 should rotate the raw top-left corner to the display top-right corner: \
         top_right={top_right} top_left={top_left} bottom_left={bottom_left} bottom_right={bottom_right}"
    );
}

/// Runs `<< opts >> et-tonemap` and returns the grid as rows of darkness,
/// checking every row has the same length.
fn tonemap(it: &mut Interp, opts: &str) -> Vec<Vec<f64>> {
    it.run_str(&format!(
        "clear << {opts} >> et-tonemap \
         dup {{ length }} forall count 1 sub array astore exch \
         {{ aload pop }} forall"
    ))
    .unwrap_or_else(|e| panic!("et-tonemap failed: {}", it.error_report(&e)));
    let stack: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    let num = |s: &str| -> f64 {
        s.parse()
            .unwrap_or_else(|_| panic!("expected a number, got {s}"))
    };
    // stack[0] is the array of row lengths; the cell values follow.
    let lens: Vec<usize> = stack[0]
        .trim_matches(|c| c == '[' || c == ']')
        .split_whitespace()
        .map(|s| num(s) as usize)
        .collect();
    let values: Vec<f64> = stack[1..].iter().map(|s| num(s)).collect();
    assert_eq!(lens.iter().sum::<usize>(), values.len(), "cell count");
    let mut rows = Vec::new();
    let mut at = 0;
    for n in lens {
        rows.push(values[at..at + n].to_vec());
        at += n;
    }
    it.run_str("clear").unwrap();
    rows
}

#[test]
fn et_tonemap_returns_a_grid_of_darkness_in_image_order() {
    // tests/data/topdark.jpg: dark top rows over a light field. Row 0 of
    // the map is the top of the photo (image order, unlike PostScript's
    // y), so the first row must be darker than the last.
    let mut it = with_lib(64, 64);
    let rows = tonemap(
        &mut it,
        "/Photo (tests/data/topdark.jpg) /Columns 5 /Rows 4",
    );
    assert_eq!(rows.len(), 4, "one array per row");
    assert!(rows.iter().all(|r| r.len() == 5), "five cells per row");
    assert!(
        rows.iter().flatten().all(|v| (0.0..=1.0).contains(v)),
        "darkness lies in [0,1]: {rows:?}"
    );
    let mean = |r: &Vec<f64>| r.iter().sum::<f64>() / r.len() as f64;
    assert!(
        mean(&rows[0]) > mean(&rows[3]) + 0.3,
        "the dark source rows are at the top: {rows:?}"
    );
}

#[test]
fn et_tonemap_maps_a_flat_photo_flat() {
    // gray8.jpg is one flat grey: every cell is its darkness, whatever the
    // grid or the sample count, and the operand stack is left clean.
    let mut it = with_lib(64, 64);
    let a = tonemap(
        &mut it,
        "/Photo (tests/data/gray8.jpg) /Columns 3 /Rows 3 /Samples 1",
    );
    let b = tonemap(
        &mut it,
        "/Photo (tests/data/gray8.jpg) /Columns 2 /Rows 5 /Samples 4",
    );
    let first = a[0][0];
    for v in a.iter().chain(b.iter()).flatten() {
        assert!(
            (v - first).abs() < 0.02,
            "a flat photo maps flat: {a:?} {b:?}"
        );
    }
    it.run_str("<< /Photo (tests/data/gray8.jpg) >> et-tonemap pop")
        .unwrap_or_else(|e| panic!("et-tonemap failed: {}", it.error_report(&e)));
    assert!(
        it.operand_stack().is_empty(),
        "et-tonemap leaves only its grid"
    );
}

#[test]
fn et_tonemap_honors_exif_orientation() {
    // cornerdark_exif6.jpg: orientation 6 puts the raw top-left dark
    // block in the displayed top-right corner, as
    // et_draw_honors_exif_orientation_rotation asserts for et-draw. The
    // map is 40 x 80 displayed, so on a 4 x 4 grid the top-right cell
    // (row 0, last column) must be the darkest.
    let mut it = with_lib(64, 64);
    let rows = tonemap(
        &mut it,
        "/Photo (tests/data/cornerdark_exif6.jpg) /Columns 4 /Rows 4",
    );
    let corner = rows[0][3];
    for (j, row) in rows.iter().enumerate() {
        for (i, v) in row.iter().enumerate() {
            if (j, i) != (0, 3) {
                assert!(corner > *v + 0.1, "top-right should be darkest: {rows:?}");
            }
        }
    }
}

#[test]
fn et_tonemap_rejects_bad_sizes() {
    for (opts, err) in [
        ("/Columns 4", "et-tonemap-needs-photo"),
        (
            "/Photo (tests/data/gray8.jpg) /Columns 0",
            "et-tonemap-sizes-must-be-positive",
        ),
        (
            "/Photo (tests/data/gray8.jpg) /Rows 2.5",
            "et-tonemap-sizes-must-be-integers",
        ),
    ] {
        let mut it = with_lib(64, 64);
        let e = it
            .run_str(&format!("<< {opts} >> et-tonemap"))
            .expect_err("bad options must raise");
        let report = it.error_report(&e);
        assert!(report.contains(err), "{opts}: expected {err}, got {report}");
    }
    // A refused call leaves the dict stack as it found it.
    let mut it = with_lib(64, 64);
    it.run_str(
        "countdictstack /cd0 exch def \
         { << /Photo (tests/data/gray8.jpg) /Rows 0 >> et-tonemap } stopped pop \
         clear countdictstack cd0 eq",
    )
    .unwrap_or_else(|e| panic!("probe failed: {}", it.error_report(&e)));
    assert_eq!(
        it.operand_stack().last().map(|o| o.repr()),
        Some("true".to_string()),
        "a refused et-tonemap must not leave EtchLayout on the dict stack"
    );
}
