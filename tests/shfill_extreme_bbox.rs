//! `shfill` of an axial or radial shading with an extreme `/BBox` (#1324):
//! tiny-skia's anti-aliased fill panicked on a box whose corners lie around
//! 1e10 device pixels away, which gs paints as a box that covers the page.

use pscat::Interp;

const SIZE: u32 = 120;

fn shading(kind: u32, bbox: &str) -> String {
    let coords = if kind == 2 {
        "[20 20 100 100]"
    } else {
        "[60 60 5 60 60 55]"
    };
    format!(
        "<< /ShadingType {kind} /ColorSpace /DeviceRGB /Coords {coords} /BBox {bbox}
           /Extend [true true]
           /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill"
    )
}

/// The page's pixels after `setup`, then the shading with `bbox`.
fn pixels(setup: &str, kind: u32, bbox: &str) -> Vec<u8> {
    let mut it = Interp::with_page(SIZE, SIZE).expect("page");
    it.run_str(&format!("{setup} {}", shading(kind, bbox)))
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.gfx().pixmap.data().to_vec()
}

#[test]
fn an_extreme_bbox_paints_what_a_page_sized_one_does() {
    for kind in [2, 3] {
        let want = pixels("", kind, "[-100 -100 220 220]");
        assert!(want.iter().any(|&b| b != 0), "type {kind} painted");
        for bbox in [
            "[-1e10 -1e10 1e10 1e10]",
            "[-1e9 -1e9 1e9 1e9]",
            "[-1e5 -1e5 1e5 1e5]",
            "[-1e10 -100 1e10 220]",
        ] {
            let got = pixels("", kind, bbox);
            assert_eq!(got, want, "type {kind} /BBox {bbox}");
        }
    }
}

#[test]
fn an_extreme_bbox_under_a_rotated_ctm_does_not_panic() {
    // The reference box stays inside the guard rectangle (so is not cut) and
    // still covers the page: rotated about the page centre its corners reach
    // 141 px from it and its edges stay 100 px away, past the page's 85.
    for kind in [2, 3] {
        let setup = "60 60 translate 30 rotate -60 -60 translate";
        let want = pixels(setup, kind, "[-40 -40 160 160]");
        let got = pixels(setup, kind, "[-1e10 -1e10 1e10 1e10]");
        assert_eq!(got, want, "type {kind}");
    }
}

#[test]
fn an_extreme_bbox_far_from_the_page_paints_nothing() {
    let blank = Interp::with_page(SIZE, SIZE).expect("page");
    for kind in [2, 3] {
        let got = pixels("", kind, "[1e9 1e9 1e10 1e10]");
        assert_eq!(got, blank.gfx().pixmap.data(), "type {kind}");
    }
}
