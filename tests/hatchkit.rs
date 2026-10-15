//! The reusable hatching library (issue #49, `lib/hatchkit.ps`): a
//! single operator, `hatch`, that fills whatever region is currently
//! clipped with parallel line strokes. Geometry (candidate line and
//! sample counts) is fully deterministic from /BBox/Spacing/Angles, so
//! the pre-flight safety limits are asserted on directly; anything
//! that actually draws is asserted on ink coverage, this codebase's
//! usual corpus policy for rand-driven output (see tests/artkit.rs's
//! scatter tests, the closest sibling to this one).

use pscat::{Interp, PsError};

mod common;

fn with_lib(w: u32, h: u32) -> Interp {
    let lib = std::fs::read("lib/hatchkit.ps").expect("library present");
    let mut it = Interp::with_page(w, h).expect("page");
    it.run_source(&lib)
        .unwrap_or_else(|e| panic!("hatchkit.ps failed: {}", it.error_report(&e)));
    it
}

fn run(it: &mut Interp, src: &str) {
    it.run_str(src)
        .unwrap_or_else(|e| panic!("eval of {src:?} failed: {}", it.error_report(&e)));
    // `hatch`'s own contract is `opts hatch -`; a leftover operand
    // here would be exactly the kind of leak (a /Density proc that
    // doesn't consume both its operands, say) docs/dev/HANDOFF.md records
    // `--lint` catching in `et-hatch` and `tfdrawline` -- assert it
    // directly rather than relying on `--lint` being run separately.
    assert!(
        it.operand_stack().is_empty(),
        "{src:?} left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

fn ink_count(it: &Interp) -> usize {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .filter(|p| p.red() < 128)
        .count()
}

/// The self-documenting-undefined-name error idiom this codebase uses
/// throughout (`hatch-spacing-must-be-positive` and friends) -- same
/// helper shape as tests/artkit.rs's `scatter_err`.
fn hatch_err(src: &str) -> String {
    let mut it = with_lib(100, 100);
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(name) => name,
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn hatchkit_loads_without_drawing_anything() {
    let it = with_lib(100, 100);
    assert_eq!(ink_count(&it), 0, "loading hatchkit put ink on the page");
    assert!(!it.gfx().page_shown);
    assert!(
        it.operand_stack().is_empty(),
        "loading hatchkit left {:?} on the operand stack",
        it.operand_stack()
            .iter()
            .map(|o| o.repr())
            .collect::<Vec<_>>()
    );
}

#[test]
fn flat_hatch_fills_the_clip_and_nothing_outside_it() {
    let mut it = with_lib(200, 200);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 40 40 moveto 160 40 lineto 160 160 lineto 40 160 lineto closepath clip \
         << /Angle 30 /Spacing 4 /Seed 1 >> hatch",
    );
    assert!(ink_count(&it) > 0, "hatch drew nothing inside the clip");

    let pm = &it.gfx().pixmap;
    let w = pm.width();
    for (i, p) in pm.pixels().iter().enumerate() {
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        let outside = !(40..160).contains(&x) || !(40..160).contains(&y);
        if outside {
            assert!(
                p.red() >= 250,
                "ink at ({x},{y}), outside the [40,160)x[40,160) clip"
            );
        }
    }
}

#[test]
fn hatch_clips_to_a_concave_region() {
    // An arrow/chevron: concave, and its bounding box's own corners
    // (near (0,0) and (100,100)) sit outside the shape entirely --
    // exactly the case a bbox-only region would get wrong, and the
    // one `hatch` never attempts on its own (it leans on the real
    // `clip`, see the library's own header).
    let mut it = with_lib(120, 120);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 50 90 lineto 90 10 lineto 50 40 lineto closepath clip \
         << /Angle 10 /Spacing 2 /Seed 4 >> hatch",
    );
    assert!(ink_count(&it) > 0, "nothing drawn inside the chevron");

    let pm = &it.gfx().pixmap;
    // (5,5) and (115,115), in device pixels, sit outside the bbox
    // entirely -- a trivial check the clip is active at all.
    assert!(pm.pixel(5, 5).unwrap().red() >= 250);
    assert!(pm.pixel(115, 115).unwrap().red() >= 250);
    // The bbox corners near (10,90) [top-left-ish] and (90,90)
    // [top-right-ish] in user space sit in the bbox but outside the
    // chevron's own notch -- the concave case a bbox-only region
    // would wrongly ink. User (12,88) -> device (12, 120-88=32).
    assert!(
        pm.pixel(12, 32).unwrap().red() >= 250,
        "ink landed outside the concave chevron, inside its bbox"
    );
}

#[test]
fn seeded_hatch_is_reproducible() {
    let src = "0 0 0 setrgbcolor 1 setlinecap \
        newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
        << /Angle 25 /Spacing 5 /Seed 7 /Wobble 1.2 /Dropout 0.2 /Width [0.5 2] /Trim [0.1 0.3] >> hatch";
    let mut a = with_lib(100, 100);
    run(&mut a, src);
    let mut b = with_lib(100, 100);
    run(&mut b, src);
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "same seed and options should reproduce pixel-for-pixel"
    );
}

#[test]
fn different_seeds_produce_different_arrangements() {
    let src = |seed: u32| {
        format!(
            "0 0 0 setrgbcolor 1 setlinecap \
             newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
             << /Angle 25 /Spacing 5 /Seed {seed} /Wobble 1.5 /Dropout 0.3 >> hatch"
        )
    };
    let mut a = with_lib(100, 100);
    run(&mut a, &src(1));
    let mut b = with_lib(100, 100);
    run(&mut b, &src(2));
    assert_ne!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "different seeds should draw different arrangements"
    );
}

#[test]
fn bbox_default_matches_an_explicit_bbox_of_the_same_path() {
    let mut a = with_lib(100, 100);
    run(
        &mut a,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 25 /Spacing 5 /Seed 7 >> hatch",
    );
    let mut b = with_lib(100, 100);
    run(
        &mut b,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /BBox [10 10 90 90] /Angle 25 /Spacing 5 /Seed 7 >> hatch",
    );
    assert_eq!(
        a.gfx().pixmap.data(),
        b.gfx().pixmap.data(),
        "defaulting /BBox from pathbbox should match passing the same box explicitly"
    );
}

#[test]
fn dropout_reduces_ink() {
    let src = |dropout: f64| {
        format!(
            "0 0 0 setrgbcolor 1 setlinecap \
             newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
             << /Angle 30 /Spacing 3 /Seed 5 /Dropout {dropout} >> hatch"
        )
    };
    let mut none = with_lib(100, 100);
    run(&mut none, &src(0.0));
    let mut most = with_lib(100, 100);
    run(&mut most, &src(0.85));
    assert!(
        ink_count(&most) < ink_count(&none) / 2,
        "high dropout ({}) should draw noticeably less ink than none ({})",
        ink_count(&most),
        ink_count(&none)
    );
}

#[test]
fn angles_layers_each_pass() {
    let mut single = with_lib(100, 100);
    run(
        &mut single,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 0 /Spacing 6 /Seed 9 >> hatch",
    );
    let mut cross = with_lib(100, 100);
    run(
        &mut cross,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angles [0 90] /Spacing 6 /Seed 9 >> hatch",
    );
    assert!(
        ink_count(&cross) > ink_count(&single),
        "a two-angle /Angles pass should ink more than a single angle"
    );
}

#[test]
fn density_at_or_below_threshold_draws_no_ink() {
    // A hard left/right split: x < 50 gets density 0 (at the default
    // /DensityThreshold, no ink at all), x >= 50 gets density 1.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip \
         << /Angle 20 /Spacing 2 /Seed 3 /Width [0.5 2] \
            /Density { pop 50 lt { 0.0 } { 1.0 } ifelse } >> hatch",
    );
    let pm = &it.gfx().pixmap;
    let mut left = 0usize;
    let mut right = 0usize;
    for y in 0..100u32 {
        for x in 0..100u32 {
            let dark = pm.pixel(x, y).unwrap().red() < 250;
            if x < 45 {
                if dark {
                    left += 1;
                }
            } else if x >= 55 && dark {
                right += 1;
            }
        }
    }
    assert_eq!(left, 0, "the below-threshold half should have no ink");
    assert!(right > 0, "the above-threshold half should be inked");
}

#[test]
fn density_return_value_is_clamped_not_rejected() {
    // A callback returning wildly out-of-range numbers must not error
    // or hang -- only ever change tone, never the amount of geometry
    // attempted (the library's own documented bound on /Density).
    let mut high = with_lib(60, 60);
    run(
        &mut high,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Width [0.5 2] /DensityThreshold 0.9 \
            /Density { pop pop 999 } >> hatch",
    );
    assert!(
        ink_count(&high) > 0,
        "a callback returning far above 1 should clamp to full ink, not vanish"
    );

    let mut low = with_lib(60, 60);
    run(
        &mut low,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Width [0.5 2] \
            /Density { pop pop -999 } >> hatch",
    );
    assert_eq!(
        ink_count(&low),
        0,
        "a callback returning far below 0 should clamp to no ink, not error"
    );
}

#[test]
fn density_proc_that_leaks_an_operand_is_visible_on_the_stack() {
    // /Density's contract (the library's own docs) is the same one
    // scatter's /Mark and /Weight carry: it must consume both of its
    // operands. A proc that only pops one leaks the other per sample
    // -- not silently swallowed by `hatch`, which is what makes
    // `--lint` able to catch it (docs/dev/HANDOFF.md records real leaks of
    // exactly this shape it found in et-hatch and tfdrawline).
    let mut it = with_lib(60, 60);
    it.run_str(
        "newpath 5 5 moveto 55 5 lineto 55 55 lineto 5 55 lineto closepath clip \
         << /Angle 15 /Spacing 3 /Seed 2 /Density { pop 0.5 } >> hatch",
    )
    .expect("a leaking Density proc should not itself error");
    assert!(
        !it.operand_stack().is_empty(),
        "a Density proc that only pops one operand should leave the other behind"
    );
}

#[test]
fn spacing_must_be_positive() {
    assert_eq!(
        hatch_err(
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath clip << /Spacing 0 >> hatch"
        ),
        "hatch-spacing-must-be-positive"
    );
    assert_eq!(
        hatch_err(
            "newpath 0 0 moveto 10 0 lineto 10 10 lineto closepath clip << /Spacing -2 >> hatch"
        ),
        "hatch-spacing-must-be-positive"
    );
}

#[test]
fn degenerate_bbox_is_rejected() {
    assert_eq!(
        hatch_err("<< /BBox [10 10 10 50] >> hatch"),
        "hatch-bbox-degenerate",
        "zero width"
    );
    assert_eq!(
        hatch_err("<< /BBox [10 50 50 10] >> hatch"),
        "hatch-bbox-degenerate",
        "inverted y"
    );
}

#[test]
fn density_must_be_a_callable_proc() {
    assert_eq!(
        hatch_err("<< /BBox [0 0 10 10] /Density 3 >> hatch"),
        "hatch-density-must-be-callable"
    );
}

#[test]
fn max_lines_rejects_before_drawing_anything() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str("<< /BBox [0 0 1000 1000] /Spacing 1 /MaxLines 10 >> hatch")
        .unwrap_err();
    match err {
        PsError::Undefined(name) => assert_eq!(name, "hatch-line-count-exceeds-safety-limit"),
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected /MaxLines call must not have drawn anything first"
    );
}

#[test]
fn max_samples_rejects_before_drawing_anything() {
    let mut it = with_lib(100, 100);
    let err = it
        .run_str(
            "<< /BBox [0 0 1000 1000] /Spacing 20 /MaxSamples 5 \
              /Density { pop pop 1 } >> hatch",
        )
        .unwrap_err();
    match err {
        PsError::Undefined(name) => assert_eq!(name, "hatch-sample-count-exceeds-safety-limit"),
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
    assert_eq!(
        ink_count(&it),
        0,
        "a rejected /MaxSamples call must not have drawn anything first"
    );
}

// The next three tests are regressions from a Codex review of PR #121
// (this issue's own PR): all three were real bypasses of the
// documented safety limits, none caught by the 17 tests above.

#[test]
fn out_of_range_trim_is_rejected() {
    // /Trim [-100 0.1] can draw a *negative* trim fraction, which
    // lengthens a line's span instead of shortening it -- sampling
    // past what /MaxSamples's pre-flight estimate (bounded on the
    // assumption /Trim only ever shrinks) accounted for. Validating
    // the range up front closes that regardless of /MaxSamples.
    assert_eq!(
        hatch_err(
            "<< /BBox [0 0 200 200] /Spacing 20 /Trim [-100 0.1] \
              /Density { pop pop 1 } >> hatch"
        ),
        "hatch-trim-must-be-ordered-fractions-in-0-1"
    );
    assert_eq!(
        hatch_err("<< /BBox [0 0 200 200] /Spacing 20 /Trim [0.6 0.4] >> hatch"),
        "hatch-trim-must-be-ordered-fractions-in-0-1",
        "inverted range (lo > hi)"
    );
}

#[test]
fn mutating_the_angles_array_mid_call_does_not_bypass_max_lines() {
    // The exact shape a Codex review found: /Angles pointing at a
    // caller-owned array, mutated by a /Density callback (called
    // mid-drawing-pass) *after* the pre-flight budget was computed
    // from the array's original contents but *before* the mutated
    // angle would be swept -- with the static [0 45] equivalent
    // rejected by /MaxLines but the live-array version silently
    // sweeping the mutated 45 anyway, unbudgeted. `hatch` now takes a
    // private copy of /Angles up front, so the fix's observable
    // property is that the mutation has *no effect at all*: the call
    // succeeds (the un-mutated [0 0] budget was always within
    // /MaxLines) and draws pixel-identically to a plain, static
    // `/Angles [0 0]` call -- not the [0 45] cross-hatch a successful
    // mutation would have produced.
    let mut mutated = with_lib(100, 100);
    run(
        &mut mutated,
        "0 0 0 setrgbcolor 1 setlinecap \
         /A [0 0] def \
         newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles A /Spacing 60 /MaxLines 5 \
            /Density { A 1 45 put pop pop 1 } >> hatch",
    );
    let mut baseline = with_lib(100, 100);
    run(
        &mut baseline,
        "0 0 0 setrgbcolor 1 setlinecap \
         newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles [0 0] /Spacing 60 /MaxLines 5 \
            /Density { pop pop 1 } >> hatch",
    );
    assert_eq!(
        mutated.gfx().pixmap.data(),
        baseline.gfx().pixmap.data(),
        "mutating the caller's own /Angles array mid-call should have no effect on what's drawn"
    );

    // And the static equivalent of the *intended* bypass ([0 45], the
    // value the mutation tried to smuggle in) is still correctly
    // rejected -- confirming this isn't passing merely because
    // /MaxLines 5 is too loose to ever fire for this /BBox/Spacing.
    let err = hatch_err(
        "newpath 0 0 moveto 1000 0 lineto 1000 100 lineto 0 100 lineto closepath clip \
         << /BBox [0 0 1000 100] /Angles [0 45] /Spacing 60 /MaxLines 5 >> hatch",
    );
    assert_eq!(err, "hatch-line-count-exceeds-safety-limit");
}

#[test]
fn thin_axis_aligned_region_still_draws() {
    // A region thinner than /Spacing, at a plain axis-aligned angle,
    // used to compute its sweep normal via an independent
    // cos/sin(angle+90) trig call rather than deriving it from the
    // already-computed direction vector -- two separate floating-point
    // trig evaluations are not guaranteed exactly orthogonal, and for
    // a thin-enough box that sub-ulp slack placed the sole candidate
    // offset just outside the box's true projection, so hkclipseg
    // rejected it and the pass silently drew nothing.
    // A 1-unit-tall region only gets one hairline-thin stroke, whose
    // anti-aliased coverage per pixel can land right at (this build's
    // observed value: exactly 128) the ink_count threshold other
    // tests use -- so check for *any* deviation from pure white
    // instead of a "dark enough" threshold; the property under test
    // is whether hkclipseg accepted the candidate at all, not how
    // dark the resulting stroke looks.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 100 1] /Angle 0 /Spacing 6 >> hatch",
    );
    let any_ink = it.gfx().pixmap.pixels().iter().any(|p| p.red() < 255);
    assert!(
        any_ink,
        "a thin axis-aligned region should still get at least one stroke"
    );
}

// The next two tests are round-2 regressions from a second Codex
// review of PR #121, after the first round's three fixes landed:
// `hstep`/`hspacing`/`htrimlo`/`htrimhi` were internal working state
// that gates a loop bound, but weren't actually `hk`-prefixed despite
// the library's own docs claiming that was the reserved contract --
// so a /Density callback (documented as forbidden from touching any
// `h`-prefixed name, but a caller relying on the *narrower*, actually
// enforced-by-naming "hk-" claim would reasonably believe otherwise)
// could clobber one and change a *later* line or angle's own budgeted
// work, bypassing /MaxLines or /MaxSamples for the rest of the call.
// Renamed to `hkspacing`/`hkstep`/`hktrimlo`/`hktrimhi` so the
// existing "don't touch hk- names" contract actually covers them.

#[test]
fn clobbering_hstep_from_density_does_not_change_the_real_sampling_step() {
    // `hkstep` (the real internal name after the fix) is still
    // documented as off-limits and a caller redefining it is still
    // undefined behavior -- but a caller touching the *unprefixed*
    // `hstep` name specifically (a name the pre-fix docs' "hk-"
    // contract never actually covered) must have no effect at all,
    // since it no longer aliases anything hatch reads.
    let mut it = with_lib(60, 60);
    it.run_str(
        "/n 0 def \
         << /BBox [0 0 50 50] /Angle 0 /Spacing 25 /MaxSamples 15 \
            /Density { /hstep 0.1 def /n n 1 add def pop pop 1 } >> hatch \
         n",
    )
    .unwrap_or_else(|e| panic!("hatch failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let n: i64 = stack
        .last()
        .expect("n left on the stack")
        .repr()
        .parse()
        .expect("n should be an integer");
    assert_eq!(
        n, 15,
        "redefining the unprefixed /hstep should not change the real budgeted sample count"
    );
}

#[test]
fn clobbering_trim_bounds_from_density_does_not_bypass_max_samples() {
    // Same shape as the /Trim input-validation test above, but via
    // global corruption mid-call rather than malformed initial input
    // -- /Trim's own validation only runs once, so if the bound
    // variables it validated aren't the same ones later code reads,
    // a /Density callback can reintroduce exactly the negative-trim
    // span-growth bug the validation was meant to close.
    // A plain successful `run()` (which also asserts an empty operand
    // stack afterward) is the assertion here: clobbering the
    // unprefixed names must have no effect, so this must behave
    // exactly like the equivalent call with no /Density at all --
    // not error, and not silently sample far more than /MaxSamples.
    let mut it = with_lib(60, 60);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 50 50] /Angle 0 /Spacing 25 /MaxSamples 15 \
            /Density { /htrimhi 999 def /htrimlo -999 def pop pop 1 } >> hatch",
    );
}

// The next three tests are round-3 regressions from a third Codex
// review of PR #121, after rounds 1-2's fixes landed. Two are
// non-adversarial bugs (an ordinary caller can hit either with no
// /Density callback at all); the third — bbox bounds living outside
// the reserved `hk-` prefix, the same shape as round 2's finding — is
// deliberately *not* fixed. Renaming cannot close that class: a
// callback that redefines the *already-hk-prefixed* `hkstep` directly
// still bypasses the cap (`clobbering_hstep_from_density_...` above,
// tested against the pre-fix unprefixed name, makes the same point).
// PostScript has no private namespace, so this stays a documented
// contract -- `lib/artkit.ps`'s `scatter` ships with the identical
// exposure for `/Mark`/`/Weight` against its own sc-/sq-/si- prefixes
// (see docs/archive/NOTES.md's issue #49 entry for the full disposition).

#[test]
fn preflight_line_count_matches_the_real_drawing_loop_exactly() {
    // A config where PostScript's own floating-point `for` (summing
    // /Spacing repeatedly) used to take one more trip than
    // cvi(span/spacing)+1 predicted -- an *ordinary* caller hits this,
    // no adversarial /Density needed. The fix makes the drawing loop
    // integer-indexed, deriving each line from `hkmin + i*hkspacing`
    // rather than accumulating, so the real trip count now equals the
    // pre-flight formula by construction. Asserted directly: exactly
    // /MaxSamples calls, not one more.
    let mut it = with_lib(60, 60);
    it.run_str(
        "/n 0 def \
         << /BBox [0 0 3.3 0.1] /Angle 0 /Spacing 0.55 /MaxSamples 12 \
            /Density { /n n 1 add def pop pop 1 } >> hatch \
         n",
    )
    .unwrap_or_else(|e| panic!("hatch failed: {}", it.error_report(&e)));
    let stack = it.operand_stack();
    let n: i64 = stack
        .last()
        .expect("n left on the stack")
        .repr()
        .parse()
        .expect("n should be an integer");
    assert_eq!(
        n, 12,
        "the real /Density call count should exactly match the pre-flight estimate"
    );
}

#[test]
fn dropout_of_exactly_1_drops_every_line() {
    // hkfrnd can return exactly 1.0, which a bare `hkfrnd hdropout lt`
    // turns into "never dropped" even at the documented-certain
    // /Dropout of 1 -- the exact trap lib/artkit.ps's scodds already
    // documents and guards against; hatchkit's own dropout roll now
    // mirrors that pattern. A seed exists (found by the review) where
    // the sole candidate line in a one-line-tall region survives a
    // /Dropout of 1 without this fix.
    let mut it = with_lib(20, 20);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [0 0 10 10] /Seed 230538014 /Dropout 1 >> hatch",
    );
    assert_eq!(
        ink_count(&it),
        0,
        "/Dropout 1 should drop every candidate line, drawing nothing"
    );
}

#[test]
fn bbox_must_have_exactly_four_elements() {
    // `aload pop` silently accepts an oversized array, reading its
    // *last* four elements as coordinates and leaving the rest on the
    // operand stack -- violating hatch's own `opts hatch -` contract.
    assert_eq!(
        hatch_err("<< /BBox [99 0 0 10 10] >> hatch"),
        "hatch-bbox-must-have-four-elements"
    );
}

/// Runs `src` expecting the self-documenting guard `name`, and asserts the
/// side-effecting procedure it carries (`/gotexecuted true def`) never ran.
/// Same shape as tests/artkit.rs's helper of that name (#123).
fn assert_rejected_without_running(src: &str, name: &str) {
    let mut it = with_lib(100, 100);
    it.run_str("/gotexecuted false def /gotfn { /gotexecuted true def } def")
        .expect("setup");
    match it.run_str(src).unwrap_err() {
        PsError::Undefined(got) => assert_eq!(got, name, "wrong guard for {src:?}"),
        other => panic!("expected the guard {name} for {src:?}, got {other}"),
    }
    it.run_str("gotexecuted").expect("read the flag");
    let flag = it.operand_stack().last().map(|o| o.repr());
    assert_eq!(
        flag.as_deref(),
        Some("false"),
        "the malformed operand in {src:?} was executed instead of being rejected"
    );
}

/// A procedure that leaves a number (the worst case: before #259 it was
/// accepted and re-run on every bare reference), an operator, and an
/// executable name.
const MALFORMED: [&str; 3] = ["{ /gotexecuted true def 1 }", "/add load", "/gotfn cvx"];

#[test]
fn hatch_never_executes_a_malformed_options_operand() {
    // #259: `hatch` bound its options with `def` and then type-checked them
    // through the name, which runs a procedure, operator or executable
    // name -- the side effect happened, then a raw `stackunderflow`
    // replaced the named error.
    for opts in MALFORMED.iter().chain(&["5", "(opts)", "[ 1 2 ]"]) {
        assert_rejected_without_running(
            &format!("0 0 50 50 rectclip {opts} hatch"),
            "hatch-opts-must-be-a-dict",
        );
    }
}

#[test]
fn hatch_never_executes_a_malformed_numeric_option() {
    for (key, guard) in [
        ("Angle", "hatch-angle-must-be-a-number"),
        ("Spacing", "hatch-spacing-must-be-a-number"),
        ("Wobble", "hatch-wobble-must-be-a-number"),
        ("Dropout", "hatch-dropout-must-be-a-number"),
        (
            "DensityThreshold",
            "hatch-density-threshold-must-be-a-number",
        ),
        ("Seed", "hatch-seed-must-be-a-number"),
        ("MaxLines", "hatch-maxlines-must-be-a-number"),
        ("MaxSamples", "hatch-maxsamples-must-be-a-number"),
    ] {
        for value in MALFORMED.iter().chain(&["(text)", "[ 1 ]"]) {
            assert_rejected_without_running(
                &format!("<< /BBox [0 0 50 50] /{key} {value} >> hatch"),
                guard,
            );
        }
    }
}

#[test]
fn hatch_never_executes_a_malformed_array_option() {
    let bbox = "/BBox [0 0 50 50]";
    // /BBox: the array itself, and each of its four elements. An executable
    // array is a procedure, so it is refused rather than read as numbers.
    for bad in MALFORMED {
        assert_rejected_without_running(
            &format!("<< /BBox {bad} >> hatch"),
            "hatch-bbox-must-be-four-numbers",
        );
        for i in 0..4 {
            let mut parts = ["0", "0", "50", "50"];
            parts[i] = bad;
            assert_rejected_without_running(
                &format!("<< /BBox [ {} ] >> hatch", parts.join(" ")),
                "hatch-bbox-must-be-four-numbers",
            );
        }
    }
    assert_rejected_without_running(
        "<< /BBox { /gotexecuted true def 0 0 50 50 } >> hatch",
        "hatch-bbox-must-be-four-numbers",
    );
    // The existing length guard keeps its name.
    assert_rejected_without_running(
        "<< /BBox [ 0 0 50 50 60 ] >> hatch",
        "hatch-bbox-must-have-four-elements",
    );
    for bad in MALFORMED {
        assert_rejected_without_running(
            &format!("<< {bbox} /Angles {bad} >> hatch"),
            "hatch-angles-must-be-an-array-of-numbers",
        );
        assert_rejected_without_running(
            &format!("<< {bbox} /Angles [ 0 {bad} ] >> hatch"),
            "hatch-angles-must-be-an-array-of-numbers",
        );
        assert_rejected_without_running(
            &format!("<< {bbox} /Width {bad} >> hatch"),
            "hatch-width-must-be-a-number-or-a-pair",
        );
        for pair in [format!("[ {bad} 2 ]"), format!("[ 1 {bad} ]")] {
            assert_rejected_without_running(
                &format!("<< {bbox} /Width {pair} >> hatch"),
                "hatch-width-must-be-a-number-or-a-pair",
            );
        }
        assert_rejected_without_running(
            &format!("<< {bbox} /Trim {bad} >> hatch"),
            "hatch-trim-must-be-two-numbers",
        );
        for pair in [format!("[ {bad} 0.5 ]"), format!("[ 0 {bad} ]")] {
            assert_rejected_without_running(
                &format!("<< {bbox} /Trim {pair} >> hatch"),
                "hatch-trim-must-be-two-numbers",
            );
        }
    }
    // An executable *array* in each of the pair slots is a procedure too.
    for (key, guard) in [
        ("Angles", "hatch-angles-must-be-an-array-of-numbers"),
        ("Width", "hatch-width-must-be-a-number-or-a-pair"),
        ("Trim", "hatch-trim-must-be-two-numbers"),
    ] {
        assert_rejected_without_running(
            &format!("<< {bbox} /{key} {{ /gotexecuted true def 0 0.1 }} >> hatch"),
            guard,
        );
    }
    // A wrong-length pair is named, not a raw rangecheck.
    assert_rejected_without_running(
        &format!("<< {bbox} /Width [ 1 2 3 ] >> hatch"),
        "hatch-width-must-be-a-number-or-a-pair",
    );
    assert_rejected_without_running(
        &format!("<< {bbox} /Trim [ 0.1 ] >> hatch"),
        "hatch-trim-must-be-two-numbers",
    );
}

#[test]
fn a_rejected_hatch_leaves_the_random_stream_and_graphics_state_alone() {
    // Every operand check runs before `srand` and `gsave`: a rejected
    // call must not have reseeded the caller's stream, or left a `gsave`
    // behind (checked through the line width a drawing pass would set).
    let mut it = with_lib(100, 100);
    it.run_str("5 srand rand /first exch def 5 srand 7 setlinewidth")
        .expect("setup");
    let err = it
        .run_str("<< /BBox [0 0 50 50] /Seed 9 /Spacing { 1 } >> hatch")
        .unwrap_err();
    assert!(matches!(err, PsError::Undefined(ref n) if n == "hatch-spacing-must-be-a-number"));
    it.run_str("rand first eq currentlinewidth 7 eq")
        .expect("compare");
    let top: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert!(
        top.ends_with(&["true".to_string(), "true".to_string()]),
        "the caller's seed-5 stream or line width was disturbed by a rejected call: {top:?}"
    );
}

#[test]
fn well_formed_options_are_untouched_by_the_operand_checks() {
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "newpath 0 0 moveto 90 0 lineto 90 90 lineto closepath clip \
         << /Angles [ 0 90 ] /Spacing 5 /Width [ 1 2 ] /Wobble 0.5 /Dropout 0.1 \
            /Trim [ 0 0.2 ] /DensityThreshold 0.1 /Seed 4 /MaxLines 5000 \
            /MaxSamples 100000 /Density { pop pop 0.8 } >> hatch",
    );
    assert!(ink_count(&it) > 0);
}

// The next two tests are round-4 regressions from a fourth Codex
// review of PR #121, after rounds 1-3's fixes landed. Both are
// non-adversarial: an ordinary caller hits either with no /Density
// callback and no deliberately malformed input, just a particular
// /BBox/Angle/Spacing combination landing on a floating-point edge.

#[test]
fn preflight_uses_the_same_centered_projections_as_drawing() {
    // The pre-flight budget computed its corner projections raw
    // (uncentered), while the drawing loop centered them (subtracting
    // the bbox-center's own projection) -- the same mathematical
    // difference, but raw and centered subtraction round differently
    // in floating point for a large-magnitude /BBox, so the two loops'
    // own line counts could actually disagree. This exact
    // /BBox/Angle/Spacing/Wobble/Seed combination used to pass
    // /MaxLines 1 and /MaxSamples 1 at pre-flight but have the drawing
    // loop compute two candidate lines and call /Density twice.
    let mut it = with_lib(60, 60);
    let err = it
        .run_str(
            "/n 0 def \
             << /BBox [-102.45945071801543 154.9295410513878 33.34343981174436 175.85219124668143] \
                /Angle -99.51648412272334 /Spacing 137.39315161176085 /Wobble 1 /Seed 1 \
                /MaxLines 1 /MaxSamples 1 /Density { /n n 1 add def pop pop 1 } >> hatch",
        )
        .unwrap_err();
    match err {
        PsError::Undefined(name) => assert_eq!(
            name, "hatch-line-count-exceeds-safety-limit",
            "pre-flight should now see the same 2-line count the drawing loop would produce"
        ),
        other => panic!("expected a self-documenting undefined name, got {other}"),
    }
}

#[test]
fn thin_near_axis_aligned_region_still_draws() {
    // A region thinner than /Spacing places its sole candidate line at
    // its swept range's own boundary (hkmin), tangent to the bbox --
    // at a near-axis-aligned angle, floating-point roundoff could then
    // make hkclipseg reject that tangent intersection, silently
    // drawing nothing even though a plain 0-degree hatch over the same
    // region draws fine. Centering the candidate (or candidates) a
    // little inside the swept range instead of flush against its start
    // fixes it.
    let mut it = with_lib(120, 21);
    run(
        &mut it,
        "0 0 0 setrgbcolor 1 setlinecap \
         << /BBox [10 10 110 11] /Angle 0.0004 /Spacing 6 >> hatch",
    );
    assert!(
        ink_count(&it) > 0,
        "a thin, near-axis-aligned region should still draw at least one stroke"
    );
}

// ---- /Mark (#271): a path hook so another library can ink the lines ----

fn pixels(it: &Interp) -> Vec<(u8, u8, u8, u8)> {
    it.gfx()
        .pixmap
        .pixels()
        .iter()
        .map(|p| (p.red(), p.green(), p.blue(), p.alpha()))
        .collect()
}

/// The next `rand` after running `src` from a fresh `7 srand`, and the page.
fn next_rand_after(src: &str) -> (String, Vec<(u8, u8, u8, u8)>) {
    let mut it = with_lib(100, 100);
    run(&mut it, "0 0 100 100 rectclip 7 srand");
    run(&mut it, src);
    it.run_str("rand").expect("rand");
    let r = it.operand_stack().last().map(|o| o.repr()).unwrap();
    (r, pixels(&it))
}

const ALL_ROLLS: &str = "/BBox [5 5 95 95] /Angles [20 110] /Spacing 4 /Width [0.5 2] \
     /Wobble 1.5 /Dropout 0.25 /Trim [0.05 0.3]";

#[test]
fn mark_does_not_change_the_number_of_random_draws() {
    // HANDOFF's rule: keep the number of draws fixed. No /Seed here: a
    // seeded call restores the caller's stream, which would make the
    // comparison vacuous. /Mark appends instead of stroking, but every roll
    // (dropout, wobble, trim, ranged width) happens as before.
    let (base, _) = next_rand_after(&format!("<< {ALL_ROLLS} >> hatch"));
    // The control: a different option really does move the stream.
    let (moved, _) = next_rand_after(&format!("<< {ALL_ROLLS} /Dropout 0.5 >> hatch"));
    assert_ne!(base, moved, "the control should change the next rand");
    for mark in ["{ stroke }", "{ newpath }", "{ hatchwidths pop stroke }"] {
        let (got, _) = next_rand_after(&format!("<< {ALL_ROLLS} /Mark {mark} >> hatch"));
        assert_eq!(got, base, "/Mark {mark} changed the random stream");
    }
    // /Density: the run-splitting emission sites.
    let dens = "/Density { exch pop 100 div } /Width [0.5 2] /Wobble 1 /Trim [0 0.2]";
    let (base, _) = next_rand_after(&format!("<< /BBox [5 5 95 95] /Spacing 5 {dens} >> hatch"));
    let (got, _) = next_rand_after(&format!(
        "<< /BBox [5 5 95 95] /Spacing 5 {dens} /Mark {{ stroke }} >> hatch"
    ));
    assert_eq!(got, base, "/Mark changed the random stream under /Density");
}

#[test]
fn mark_stroke_matches_the_default_render_when_widths_agree() {
    // One angle, a numeric /Width equal to the caller's linewidth, no
    // /Density: the appended path stroked once is the default render.
    for opts in [
        "/BBox [5 5 95 95] /Angle 30 /Spacing 4 /Width 1 /Wobble 1 /Dropout 0.2 /Trim [0.1 0.3]",
        "/BBox [5 5 95 95] /Angle 0 /Spacing 7",
    ] {
        let (rand_a, page_a) =
            next_rand_after(&format!("0 setgray 1 setlinewidth << {opts} >> hatch"));
        let (rand_b, page_b) = next_rand_after(&format!(
            "0 setgray 1 setlinewidth << {opts} /Mark {{ stroke }} >> hatch"
        ));
        assert_eq!(rand_a, rand_b);
        assert!(page_a.iter().any(|p| p.0 < 128), "default drew nothing");
        assert_eq!(page_a, page_b, "{opts}: /Mark {{ stroke }} differs");
    }
    // Several angles and a ranged width: crossings anti-alias differently
    // in one stroke than in separate ones, so compare ink, not bytes.
    let opts = format!("<< {ALL_ROLLS} >> hatch");
    let (_, a) = next_rand_after(&format!("0 setgray 1 setlinewidth {opts}"));
    let (_, b) = next_rand_after(&format!(
        "0 setgray 1 setlinewidth << {ALL_ROLLS} /Mark {{ stroke }} >> hatch"
    ));
    let ink = |p: &[(u8, u8, u8, u8)]| p.iter().filter(|p| p.0 < 128).count() as f64;
    let (ia, ib) = (ink(&a), ink(&b));
    assert!(ia > 100.0 && ib > 100.0);
    assert!((ia - ib).abs() / ia < 0.5, "ink {ia} vs {ib}");
}

#[test]
fn mark_runs_once_and_hands_over_the_lines_not_the_region() {
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "/calls 0 def /box [0 0 0 0] def \
         20 30 moveto 80 30 lineto 80 70 lineto 20 70 lineto closepath clip \
         << /Angle 0 /Spacing 10 /Seed 2 /Mark { \
              /calls calls 1 add def \
              /widths hatchwidths def \
              pathbbox 4 array astore /box exch def \
              newpath } >> hatch \
         pathbbox 4 array astore /after exch def",
    );
    // `/Mark` ran once. Its path was the hatch lines (inside the 20..80 x 30..70
    // region, none of them the outline), and `hatch` gave the caller's region
    // path back afterwards.
    it.run_str("calls").unwrap();
    assert_eq!(it.operand_stack().last().unwrap().repr(), "1");
    it.run_str("widths length").unwrap();
    assert_eq!(it.operand_stack().last().unwrap().repr(), "5");
    for (i, want) in [20.0, 30.0, 80.0, 70.0].iter().enumerate() {
        it.run_str(&format!("after {i} get")).unwrap();
        let got: f64 = it.operand_stack().last().unwrap().repr().parse().unwrap();
        assert!(
            (got - want).abs() < 1e-6,
            "region path not restored: {i} {got}"
        );
    }
    // The hook's path spans the region's x extent, at y = lines only
    // (four candidate lines from 30.0..70.0 inset by the centering).
    it.run_str("box 1 get box 3 get").unwrap();
    let st = it.operand_stack();
    let hi: f64 = st[st.len() - 1].repr().parse().unwrap();
    let lo: f64 = st[st.len() - 2].repr().parse().unwrap();
    assert!(lo >= 30.0 && hi <= 70.0 && hi > lo, "{lo} {hi}");
}

#[test]
fn mark_receives_one_width_per_appended_line() {
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 100 100 rectclip \
         << /BBox [0 0 100 100] /Angle 0 /Spacing 2 /Width [1 3] /Seed 4 \
            /Mark { /w hatchwidths def stroke } >> hatch",
    );
    it.run_str("w length w { dup 1 lt exch 3 gt or } forall")
        .unwrap();
    let st: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    // 100/2 + 1 candidate lines; more than 64, so the width array grew.
    assert_eq!(st[0], "51", "{st:?}");
    assert!(st[1..].iter().all(|b| b == "false"), "a width left [1 3]");
    // A /Density call yields one width per constant-bucket run.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 100 100 rectclip \
         << /BBox [0 0 100 100] /Angle 0 /Spacing 25 /Width [1 3] \
            /Density { pop 50 div } /Mark { /w hatchwidths def stroke } >> hatch",
    );
    it.run_str("w length").unwrap();
    let n: i64 = it.operand_stack().last().unwrap().repr().parse().unwrap();
    assert!(
        n > 5,
        "density runs should add runs beyond the 5 lines: {n}"
    );
}

#[test]
fn mark_with_no_surviving_line_still_runs_once_with_nothing() {
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "/calls 0 def 0 0 100 100 rectclip \
         << /BBox [0 0 100 100] /Dropout 1 /Mark { /calls calls 1 add def \
            /w hatchwidths def stroke } >> hatch",
    );
    it.run_str("calls w length").unwrap();
    let st: Vec<String> = it.operand_stack().iter().map(|o| o.repr()).collect();
    assert_eq!(st, ["1", "0"]);
    assert_eq!(ink_count(&it), 0);
}

#[test]
fn mark_rolls_come_from_the_callers_restored_stream() {
    // /Seed restores the caller's stream before /Mark runs, so a rand the
    // hook calls is the caller's next rand and is not wiped by the restore.
    let mut it = with_lib(100, 100);
    it.run_str("5 srand << /BBox [0 0 50 50] /Seed 9 /Mark { rand pop } >> hatch rand")
        .expect("run");
    let hooked = it.operand_stack().last().unwrap().repr();
    let mut it = with_lib(100, 100);
    it.run_str("5 srand rand pop rand").expect("run");
    let want = it.operand_stack().last().unwrap().repr();
    assert_eq!(hooked, want);
}

#[test]
fn a_bad_mark_is_rejected_before_the_stream_or_graphics_state_moves() {
    for bad in [
        "5",
        "(s)",
        "/stroke",
        "[ 1 2 ]",
        "/stroke load",
        "/gotfn cvx",
    ] {
        assert_rejected_without_running(
            &format!("<< /BBox [0 0 50 50] /Mark {bad} >> hatch"),
            "hatch-mark-must-be-a-procedure",
        );
        // Checked before `srand` and `gsave`: the caller's stream is untouched.
        let mut it = with_lib(100, 100);
        it.run_str(&format!(
            "3 srand << /BBox [0 0 50 50] /Seed 9 /Mark {bad} >> hatch"
        ))
        .unwrap_err();
        it.run_str("rand").unwrap();
        let after = it.operand_stack().last().unwrap().repr();
        let mut it = with_lib(100, 100);
        it.run_str("3 srand rand").unwrap();
        assert_eq!(after, it.operand_stack().last().unwrap().repr(), "{bad}");
    }
    // Null is the same as unset.
    let mut it = with_lib(100, 100);
    run(
        &mut it,
        "0 0 100 100 rectclip << /BBox [0 0 100 100] /Mark null >> hatch",
    );
    assert!(ink_count(&it) > 0);
}

#[test]
fn ghostscript_accepts_a_mark_procedure_under_packing() {
    // Under `true setpacking` Ghostscript types a procedure literal
    // `packedarraytype`, which the /Mark guard must accept (Codex review,
    // PR #300). This interpreter never packs, so only real gs can show it.
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs packing check: gs not installed");
        return;
    }
    let dir = common::scratch_dir("pscat-hatch-mark");
    std::fs::create_dir_all(&dir).unwrap();
    let ps = dir.join("mark_packed.ps");
    std::fs::write(
        &ps,
        "(lib/hatchkit.ps) run\n\
         true setpacking\n\
         /marks 0 def\n\
         0 0 50 50 rectclip\n\
         << /BBox [0 0 50 50] /Spacing 5 /Mark { /marks marks 1 add def stroke } >> hatch\n\
         marks 1 ne { /marks_not_one_after_hatch undefined } if\n\
         showpage\n",
    )
    .unwrap();
    let out = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=nullpage",
        ])
        .arg(&ps)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs rejected a packed /Mark procedure: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn ghostscript_accepts_the_hatching_specimen_sheet() {
    // The acceptance criterion itself -- the specimen page runs
    // unchanged in both interpreters. `-dNOSAFER` is needed because
    // the file does `(lib/hatchkit.ps) run` from disk, which gs's
    // default sandbox blocks (same reasoning as every other sibling
    // library's own version of this test).
    let gs_ok = std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !gs_ok {
        eprintln!("skipping gs compatibility check: gs not installed");
        return;
    }
    let status = std::process::Command::new("gs")
        .args([
            "-dNOSAFER",
            "-dNOPAUSE",
            "-dBATCH",
            "-q",
            "-sDEVICE=png16m",
            "-g900x360",
            "-r72",
            "-o/dev/null",
            "examples/hatching.ps",
        ])
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected examples/hatching.ps");
}
