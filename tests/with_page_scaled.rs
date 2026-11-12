//! `Interp::with_page_scaled` refuses a scale that cannot size a canvas
//! (#1133): a NaN or 0 used to build a degenerate 1x1 page and render blank
//! instead of failing.

use pscat::Interp;

#[test]
fn a_scale_that_is_not_positive_and_finite_is_refused() {
    for scale in [
        0.0,
        -0.0,
        -1.0,
        -2.5,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert!(
            Interp::with_page_scaled(612, 792, scale).is_none(),
            "scale {scale} should be refused"
        );
    }
}

#[test]
fn a_valid_scale_still_sizes_the_page_and_the_ceiling_still_applies() {
    let it = Interp::with_page_scaled(200, 100, 2.0).expect("scale 2");
    let px = &it.gfx().pixmap;
    assert_eq!((px.width(), px.height()), (400, 200));
    // A tiny positive scale is valid: it rounds up to a 1-pixel minimum.
    let it = Interp::with_page_scaled(10, 10, 0.01).expect("scale 0.01");
    assert_eq!((it.gfx().pixmap.width(), it.gfx().pixmap.height()), (1, 1));
    // The 8000-pixel ceiling is a separate refusal.
    assert!(Interp::with_page_scaled(612, 792, 11.0).is_none());
    assert!(Interp::with_page_scaled(612, 792, 10.0).is_some());
    // `with_page` is scale 1.
    assert!(Interp::with_page(612, 792).is_some());
}
