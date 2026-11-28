//! The 16-bit fill shadow and the host (#1506): what `swap_canvas` does to it
//! (cases (a) and (b) of the issue, pinned as they are today) and
//! `Interp::drop_fill_shadow`, the explicit way to drop it. A host operator
//! named `dropshadow` calls the new method from inside the running program
//! and logs what it saw, as the #1497 tests read `gsave_depth`, so each test
//! exercises the call where a host makes it: mid-run, in a group, in a
//! `PaintProc`. `swaptrick` is the swap-out-and-back the call replaces.
//! Only the supported surface (`pscat::host`, `Interp`, `Page`) is used.

use std::cell::RefCell;
use std::rc::Rc;

use pscat::{Interp, Limits, Page};

const N: u32 = 64;
/// An opaque black page, without a shadow (an opaque fill writes the 8-bit
/// page only).
const BLACK: &str = "0 setgray 0 0 64 64 rectfill ";
/// One faint white fill of the whole page.
const FILL: &str = "1 setgray 0.02 setalpha 0 0 64 64 rectfill ";
/// One faint white glyph.
const GLYPH: &str =
    "1 setgray 0.02 setalpha /Helvetica findfont 60 scalefont setfont 4 8 moveto (W) show ";

/// What `dropshadow` saw each time it ran: what the call returned, and
/// whether the canvas bytes were the same before and after it.
type Seen = Rc<RefCell<Vec<(bool, bool)>>>;

fn new_interp() -> (Interp, Seen) {
    let mut it = Interp::with_page(N, N).expect("valid page size");
    let seen: Seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    it.register_operator("dropshadow", move |i| {
        let before = i.canvas_premultiplied_rgba8().to_vec();
        let returned = i.drop_fill_shadow();
        let same = before == i.canvas_premultiplied_rgba8();
        log.borrow_mut().push((returned, same));
        Ok(())
    })
    .expect("registers");
    it.register_operator("swaptrick", |i| {
        let layer = Page::transparent(N, N).expect("size");
        // Handed back as an error inside a PaintProc with no page group.
        if let Ok(held) = i.swap_canvas(layer) {
            i.swap_canvas(held).expect("same size");
        }
        Ok(())
    })
    .expect("registers");
    (it, seen)
}

/// How a program reaches the point where the shadow may be dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    /// Nothing: the shadow lives on.
    Keep,
    /// `Interp::drop_fill_shadow` (host operator `dropshadow`).
    Drop,
    /// Swap the canvas out and back (host operator `swaptrick`).
    Swap,
}

impl Way {
    fn op(self) -> &'static str {
        match self {
            Way::Keep => "",
            Way::Drop => "dropshadow ",
            Way::Swap => "swaptrick ",
        }
    }
}

/// Run `template` with each `@` replaced by `way`'s operator; the canvas it
/// ends with and what `dropshadow` saw.
fn render(template: &str, way: Way) -> (Vec<u8>, Vec<(bool, bool)>) {
    let (mut it, seen) = new_interp();
    it.run_str(&template.replace('@', way.op())).expect("runs");
    let canvas = it.canvas_premultiplied_rgba8().to_vec();
    let log = seen.borrow().clone();
    (canvas, log)
}

/// (pixels that differ, the largest difference in one channel).
fn differing(a: &[u8], b: &[u8]) -> (usize, i32) {
    assert_eq!(a.len(), b.len());
    let n = a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x != y).count();
    let max = a
        .iter()
        .zip(b)
        .map(|(x, y)| (i32::from(*x) - i32::from(*y)).abs())
        .max()
        .unwrap_or(0);
    (n, max)
}

/// `a` and `b` are different renders, and by at most one level; how many
/// pixels differ.
fn assert_off_by_a_level(a: &[u8], b: &[u8]) -> usize {
    let (n, max) = differing(a, b);
    assert!(n > 0, "the two renders were expected to differ");
    assert_eq!(max, 1, "{n} pixels differ, by more than a level");
    n
}

fn swap_out_and_back(it: &mut Interp) {
    let layer = Page::transparent(N, N).expect("size");
    let held = it.swap_canvas(layer).expect("same size");
    it.swap_canvas(held).expect("same size");
}

/// A fresh interpreter whose canvas is `pixels` (no shadow: it never painted).
fn from_pixels(pixels: &[u8]) -> Interp {
    let mut it = Interp::with_page(N, N).expect("valid page size");
    let page = Page::from_premultiplied_rgba8(N, N, pixels.to_vec()).expect("valid size");
    it.swap_canvas(page).expect("same size");
    it
}

// --- what swap_canvas does today (pinned) -----------------------------------

/// Case (a): 5 faint fills, swap out and back, one more: every one of the
/// 4096 pixels is one level off the same painting with no swap.
#[test]
fn case_a_swap_canvas_changes_how_the_next_fill_rounds() {
    let mut plain = Interp::with_page(N, N).expect("size");
    plain
        .run_str(&format!("{BLACK}{}", FILL.repeat(6)))
        .expect("runs");
    let mut swapped = Interp::with_page(N, N).expect("size");
    swapped
        .run_str(&format!("{BLACK}{}", FILL.repeat(5)))
        .expect("runs");
    let before = swapped.canvas_premultiplied_rgba8().to_vec();
    swap_out_and_back(&mut swapped);
    assert_eq!(swapped.canvas_premultiplied_rgba8(), &before[..]);
    swapped.run_str(FILL).expect("runs");
    let n = assert_off_by_a_level(
        plain.canvas_premultiplied_rgba8(),
        swapped.canvas_premultiplied_rgba8(),
    );
    assert_eq!(n, 4096);
}

/// Case (b): one faint fill, swap, one faint glyph: some pixels (37 for this
/// glyph) are one level off.
#[test]
fn case_b_swap_canvas_changes_how_the_next_glyph_rounds() {
    let mut plain = Interp::with_page(N, N).expect("size");
    plain
        .run_str(&format!("{BLACK}{FILL}{GLYPH}"))
        .expect("runs");
    let mut swapped = Interp::with_page(N, N).expect("size");
    swapped.run_str(&format!("{BLACK}{FILL}")).expect("runs");
    swap_out_and_back(&mut swapped);
    swapped.run_str(GLYPH).expect("runs");
    let n = assert_off_by_a_level(
        plain.canvas_premultiplied_rgba8(),
        swapped.canvas_premultiplied_rgba8(),
    );
    assert_eq!(n, 37);
}

// --- the explicit drop --------------------------------------------------------

/// With the drop, case (a) and case (b) come out exactly as with the swap
/// trick (and so differ from the no-drop painting by the same 4096 and 37
/// pixels), and equal a fresh interpreter handed the same 8-bit pixels.
#[test]
fn the_drop_does_what_the_swap_did_to_cases_a_and_b() {
    for (build, last, count) in [
        (FILL.repeat(5), FILL.to_string(), 4096),
        (FILL.to_string(), GLYPH.to_string(), 37),
    ] {
        let template = format!("{BLACK}{build} @ {last}");
        let keep = render(&template, Way::Keep).0;
        let (dropped, seen) = render(&template, Way::Drop);
        let swapped = render(&template, Way::Swap).0;
        assert_eq!(dropped, swapped, "drop is the swap trick, for the shadow");
        assert_eq!(assert_off_by_a_level(&keep, &dropped), count);
        assert_eq!(seen, [(true, true)], "returned true, pixels unchanged");

        // A fresh interpreter handed the pixels as they stood at the call.
        let (mut base, _) = new_interp();
        base.run_str(&format!("{BLACK}{build}")).expect("runs");
        let mut fresh = from_pixels(base.canvas_premultiplied_rgba8());
        fresh.run_str(&last).expect("runs");
        assert_eq!(fresh.canvas_premultiplied_rgba8(), &dropped[..]);
    }
}

/// What the drop changes in later painting. A 64x64 black page with five
/// faint fills on it, the drop (or not), then one more mark: fills, `rectfill`s
/// and glyphs that blend differ (Multiply even at alpha 1, and the
/// anti-aliased edge pixels of an opaque circle, glyph or off-grid
/// rectangle), and a stroke, an image and an opaque rectangle on whole pixels
/// do not. Each "does not" leg also checks that the mark changed pixels, so
/// it is not vacuous.
#[test]
fn what_the_drop_changes_and_what_it_does_not() {
    let build = format!("{BLACK}{}", FILL.repeat(5));
    let only_build = render(&build, Way::Keep).0;
    let sweep = |last: &str| {
        let t = format!("{build} @ {last}");
        let keep = render(&t, Way::Keep).0;
        let dropped = render(&t, Way::Drop).0;
        (
            differing(&keep, &dropped).0,
            differing(&only_build, &keep).0,
        )
    };
    // Translucent fills, rectfill, glyphs: every pixel they cover differs.
    assert_eq!(sweep(FILL), (4096, 4096));
    assert_eq!(
        sweep("1 setgray 0.02 setalpha 0 0 moveto 64 0 lineto 64 64 lineto 0 64 lineto fill "),
        (4096, 4096)
    );
    assert!(sweep(GLYPH).0 > 0);
    // Multiply at alpha 1 over the build-up: some pixels differ.
    assert!(sweep("0.5 setgray 1 setalpha /Multiply setblendmode 0 0 64 64 rectfill ").0 > 0);
    // The anti-aliased edge of an opaque fill or glyph goes through the
    // shadow too: only those pixels differ (measured: 32, 108 and 3).
    let (edge, painted) = sweep("0 1 0 setrgbcolor 1 setalpha 32 32 20.3 0 360 arc fill ");
    assert_eq!(edge, 32);
    assert!(painted > edge);
    let (edge, _) = sweep(
        "0 1 0 setrgbcolor 1 setalpha /Helvetica findfont 60 scalefont setfont 4 8 moveto (W) show ",
    );
    assert_eq!(edge, 108);
    assert_eq!(
        sweep("0 1 0 setrgbcolor 1 setalpha 0.5 0.5 40.25 40.25 rectfill ").0,
        3
    );
    // Whole pixels of an opaque Normal fill: no difference, and it painted.
    let (diff, painted) = sweep("0 1 0 setrgbcolor 1 setalpha 0 0 40 40 rectfill ");
    assert_eq!((diff, painted), (0, 1600));
    // A stroke and an image write the 8-bit canvas only: no difference,
    // though both painted.
    let (diff, painted) =
        sweep("1 setgray 0.02 setalpha 10 setlinewidth 0 32 moveto 64 32 lineto stroke ");
    assert_eq!(diff, 0);
    assert!(painted > 0, "the stroke painted");
    let (diff, painted) = sweep(
        "gsave 0.02 setalpha 64 64 scale 2 2 8 [2 0 0 -2 0 2] <ff00 00ff> false 1 colorimage grestore ",
    );
    assert_eq!(diff, 0);
    assert!(painted > 0, "the image painted");
}

/// 1000 fills of white at alpha 0.002 over black reach 221 in 16 bits, the
/// figure the rustdoc quotes. A drop every 100 fills restarts from 8 bits
/// each time and falls short, and a drop before every fill leaves 5.
#[test]
fn the_shadow_reaches_what_8_bits_cannot_and_a_drop_costs_it() {
    let faint = "1 setgray 0.002 setalpha 0 0 64 64 rectfill ";
    let (kept, _) = render(&format!("{BLACK}{}", faint.repeat(1000)), Way::Keep);
    assert_eq!(kept[0], 221);
    let t = format!("{BLACK}{}", format!("{} @ ", faint.repeat(100)).repeat(10));
    let (dropped, seen) = render(&t, Way::Drop);
    assert_eq!(seen.len(), 10);
    assert!(dropped[0] < kept[0], "{} < {}", dropped[0], kept[0]);
    // Dropped before every fill, every fill rounds from 8 bits.
    let each = format!("{BLACK}{}", format!("@ {faint}").repeat(1000));
    assert_eq!(render(&each, Way::Drop).0[0], 5);
}

/// Calling it again, or with nothing translucent painted, does nothing.
#[test]
fn it_is_idempotent_and_harmless_without_a_shadow() {
    let (mut it, _) = new_interp();
    assert!(it.drop_fill_shadow());
    assert!(it.drop_fill_shadow());
    let once = render(&format!("{BLACK}{} @ {FILL}", FILL.repeat(5)), Way::Drop).0;
    let thrice = render(
        &format!("{BLACK}{} @ @ @ {FILL}", FILL.repeat(5)),
        Way::Drop,
    )
    .0;
    assert_eq!(once, thrice);
    // Before anything was painted: the same page as without the call.
    let keep = render(&format!("@ {BLACK}{}", FILL.repeat(6)), Way::Keep).0;
    let dropped = render(&format!("@ {BLACK}{}", FILL.repeat(6)), Way::Drop).0;
    assert_eq!(keep, dropped);
}

/// Called from Rust between the slices of a program run in steps, and
/// between runs: the same as the operator's call at that point.
#[test]
fn it_can_be_called_from_rust_between_slices_and_runs() {
    let build = format!("{BLACK}{}", FILL.repeat(5));
    let want = render(&format!("{build} @ {FILL}"), Way::Drop).0;
    let keep = render(&format!("{build} {FILL}"), Way::Keep).0;
    assert_ne!(want, keep);
    for slice in [1, 7, 100_000] {
        let (mut it, _) = new_interp();
        it.begin_source(build.as_bytes());
        while it.step_n(slice).expect("steps") {}
        assert!(it.drop_fill_shadow());
        it.begin_source(FILL.as_bytes());
        while it.step_n(slice).expect("steps") {}
        assert_eq!(it.canvas_premultiplied_rgba8(), &want[..], "slice {slice}");
    }
    let (mut it, _) = new_interp();
    it.run_str(&build).expect("runs");
    assert!(it.drop_fill_shadow());
    it.run_str(FILL).expect("runs");
    assert_eq!(it.canvas_premultiplied_rgba8(), &want[..]);
}

// --- a pure effect, free against the limits ---------------------------------

/// The call changes no pixel (the operator checks before and after, in the
/// middle of the run), and the whole run's cost against the limits is the
/// same with 1000 calls per probe as with none. `steps_used` counts only
/// under a limit, so one is set and the count is asserted to be real.
#[test]
fn the_drop_is_a_pure_effect_and_costs_nothing() {
    let src = format!(
        "{BLACK}{} probe {FILL} probe << /Alpha 0.5 >> begingroup {FILL} probe 0 1 0 setrgbcolor
         5 5 30 30 rectfill probe endgroup probe",
        FILL.repeat(3)
    );
    let mut runs = Vec::new();
    for hammer in [false, true] {
        let mut it = Interp::with_page(N, N).expect("size");
        it.set_limits(Limits::none().with_max_steps(1_000_000))
            .expect("limits");
        let asks = Rc::new(RefCell::new(0_u32));
        let a = asks.clone();
        it.register_operator("probe", move |i| {
            if hammer {
                let before = i.canvas_premultiplied_rgba8().to_vec();
                for _ in 0..1000 {
                    assert!(i.drop_fill_shadow());
                }
                assert_eq!(i.canvas_premultiplied_rgba8(), &before[..]);
                *a.borrow_mut() += 1;
            }
            Ok(())
        })
        .expect("registers");
        it.run_str(&src).expect("runs");
        runs.push((it.steps_used(), asks.take()));
    }
    assert!(
        runs[0].0 > 20,
        "the steps are really counted: {}",
        runs[0].0
    );
    assert_eq!(runs[0].0, runs[1].0, "steps");
    assert_eq!(runs[1].1, 5, "every probe ran");

    // Without a limit nothing is counted, so a steps_used of 0 there says
    // nothing: this is why the run above sets one.
    let mut it = Interp::with_page(N, N).expect("size");
    it.run_str(&format!("{BLACK}{FILL}")).expect("runs");
    it.drop_fill_shadow();
    assert_eq!(it.steps_used(), 0);

    // Under a limit far below what 100000 calls could cost, if they cost.
    let mut it = Interp::with_page(10, 10).expect("size");
    it.register_operator("probe", |i| {
        for _ in 0..100_000 {
            std::hint::black_box(i.drop_fill_shadow());
        }
        Ok(())
    })
    .expect("registers");
    it.set_limits(Limits::none().with_max_steps(40))
        .expect("limits");
    it.run_str("1 setgray 0.5 setalpha 0 0 10 10 rectfill probe probe probe")
        .expect("runs");
    assert!((1..=40).contains(&it.steps_used()), "{}", it.steps_used());

    // From Rust between slices, the counter does not move.
    let mut it = Interp::with_page(10, 10).expect("size");
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("limits");
    it.begin_source(b"0.5 setalpha 0 0 10 10 rectfill 1000 { 1 1 add pop } repeat");
    assert!(it.step_n(40).expect("steps"));
    let used = it.steps_used();
    assert!(used > 0, "the steps are really counted");
    for _ in 0..10_000 {
        assert!(it.drop_fill_shadow());
    }
    assert_eq!(it.steps_used(), used);
}

// --- after showpage ---------------------------------------------------------

/// `showpage` arms an erase that the next mark carries out. Unlike the swap
/// trick, which cancels it and paints on the old page, the drop leaves it
/// armed: the next mark still starts a fresh page.
#[test]
fn the_drop_leaves_a_pending_erase_armed_unlike_the_swap_trick() {
    let t = format!(
        "{BLACK}{} showpage @ 1 0 0 setrgbcolor 1 setalpha 0 0 8 8 rectfill ",
        FILL.repeat(5)
    );
    let keep = render(&t, Way::Keep).0;
    let (dropped, seen) = render(&t, Way::Drop);
    let swapped = render(&t, Way::Swap).0;
    // The pixels at the call are the old page (the erase is lazy), and
    // unchanged by it.
    assert_eq!(seen, [(true, true)]);
    // After the next mark the page is the erased one: white, a red square.
    assert_eq!(keep, dropped);
    let px = |c: &[u8], x: usize, y: usize| c[(y * N as usize + x) * 4..][..4].to_vec();
    assert_eq!(px(&dropped, 40, 40), [255, 255, 255, 255]);
    assert_eq!(px(&dropped, 2, 60), [255, 0, 0, 255]);
    // The swap trick cancelled the erase: the build-up is still there.
    assert_ne!(px(&swapped, 40, 40), [255, 255, 255, 255]);
}

/// Nor does it mark the page painted: after `showpage` and a drop the
/// program has painted nothing, so no trailing page is counted, where the
/// swap trick makes the old canvas a page of its own.
#[test]
fn the_drop_does_not_make_a_trailing_page_unlike_the_swap_trick() {
    let t = format!("{BLACK}{} showpage @", FILL.repeat(5));
    let pages = |way| {
        let (mut it, _) = new_interp();
        it.run_str(&t.replace('@', way_op(way))).expect("runs");
        it.pages().len()
    };
    assert_eq!(pages(Way::Keep), 1);
    assert_eq!(pages(Way::Drop), 1);
    assert_eq!(pages(Way::Swap), 2);
}

fn way_op(way: Way) -> &'static str {
    way.op()
}

// --- groups -----------------------------------------------------------------

/// The faint group a build-up on the page is composited under: `Alpha` makes
/// the composite read the page's shadow as its destination.
const GROUP_OVER: &str =
    "1 setalpha << /Alpha 0.02 >> begingroup 1 setgray 0 0 64 64 rectfill endgroup ";

/// With a group open the drop acts on the page below the group (the canvas
/// `swap_canvas` exchanges): dropping inside the group is dropping before it
/// and is the swap trick inside it, and differs from keeping the shadow.
#[test]
fn inside_a_group_the_page_below_loses_its_shadow() {
    let inside = format!(
        "{BLACK}{} 1 setalpha << /Alpha 0.02 >> begingroup @ 1 setgray 0 0 64 64 rectfill endgroup ",
        FILL.repeat(5)
    );
    let before = format!("{BLACK}{} @ {GROUP_OVER}", FILL.repeat(5));
    let keep = render(&inside, Way::Keep).0;
    let (dropped, seen) = render(&inside, Way::Drop);
    let swapped = render(&inside, Way::Swap).0;
    assert_eq!(seen, [(true, true)]);
    assert_eq!(dropped, swapped, "the swap trick, inside the group");
    assert_eq!(dropped, render(&before, Way::Drop).0, "as if before it");
    assert_eq!(keep, render(&before, Way::Keep).0);
    assert_off_by_a_level(&keep, &dropped);
}

/// Nested groups: the canvas is the page below the OUTERMOST group, however
/// deep the call is, and the same holds for the swap trick.
#[test]
fn in_nested_groups_it_is_the_outermost_pages_shadow() {
    let nested = format!(
        "{BLACK}{} 1 setalpha << /Alpha 0.5 >> begingroup << /Alpha 0.04 >> begingroup @ \
         1 setgray 0 0 64 64 rectfill endgroup endgroup ",
        FILL.repeat(5)
    );
    let keep = render(&nested, Way::Keep).0;
    let (dropped, seen) = render(&nested, Way::Drop);
    let swapped = render(&nested, Way::Swap).0;
    assert_eq!(seen, [(true, true)]);
    assert_eq!(dropped, swapped);
    assert_off_by_a_level(&keep, &dropped);
    // Dropped from the outer group, the same.
    let outer = nested
        .replace("begingroup @", "begingroup")
        .replace("<< /Alpha 0.04 >>", "@ << /Alpha 0.04 >>");
    assert_eq!(render(&outer, Way::Drop).0, dropped);
}

/// A group's own layer keeps its shadow. The page below has none to lose
/// (opaque black); a faint build-up is painted on the layer; a call made in
/// that group, or in a group nested in it, changes nothing (the swap trick,
/// which exchanges only the page below, changes nothing either).
#[test]
fn a_layers_own_shadow_is_not_touched() {
    for t in [
        // In the group the build-up was painted in.
        format!(
            "{BLACK}{} << /Alpha 0.9 >> begingroup {} @ {FILL} endgroup ",
            "1 setalpha",
            FILL.repeat(5)
        ),
        // In a group nested in it.
        format!(
            "{BLACK}{} << /Alpha 0.9 >> begingroup {} << /Alpha 0.5 >> begingroup @ {FILL}
             endgroup endgroup ",
            "1 setalpha",
            FILL.repeat(5)
        ),
    ] {
        let keep = render(&t, Way::Keep).0;
        let (dropped, seen) = render(&t, Way::Drop);
        assert_eq!(seen, [(true, true)]);
        assert_eq!(dropped, keep);
        assert_eq!(render(&t, Way::Swap).0, keep);
    }
}

/// A group opened while nothing paints has no layer: the canvas is the page.
#[test]
fn a_group_opened_after_nulldevice_displaces_nothing() {
    let (mut it, seen) = new_interp();
    it.run_str("gsave nulldevice << >> begingroup dropshadow endgroup grestore")
        .expect("runs");
    assert_eq!(*seen.borrow(), [(true, true)]);
}

// --- inside a PaintProc -----------------------------------------------------

/// A swap that is refused (the wrong size) leaves the shadow alone.
#[test]
fn a_refused_swap_leaves_the_shadow_alone() {
    let t = format!("{BLACK}{} @ {FILL}", FILL.repeat(5));
    let keep = render(&t, Way::Keep).0;
    let (mut it, _) = new_interp();
    it.run_str(&format!("{BLACK}{}", FILL.repeat(5)))
        .expect("runs");
    assert!(
        it.swap_canvas(Page::transparent(N + 1, N).expect("size"))
            .is_err()
    );
    it.run_str(FILL).expect("runs");
    assert_eq!(it.canvas_premultiplied_rgba8(), &keep[..]);
}

/// A tiling pattern whose `PaintProc` runs `BODY` and paints a white tile.
const PATTERN: &str = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 64 64]
        /XStep 64 /YStep 64 /PaintProc { pop BODY 1 setgray 0 0 64 64 rectfill } >> matrix makepattern
        setpattern 0 0 64 64 rectfill ";

fn pattern(body: &str) -> String {
    PATTERN.replace("BODY", body)
}

/// Inside a `PaintProc` with no page group open there is no canvas: the call
/// returns false and drops nothing (the render equals one with no call, and
/// differs from one that dropped outside the `PaintProc`), and the swap
/// trick is handed the page back.
#[test]
fn in_a_paintproc_there_is_no_canvas_to_drop() {
    let build = format!("{BLACK}{}", FILL.repeat(5));
    let inside = format!("{build}{} {FILL}", pattern("@"));
    let outside = format!("{build}@ {} {FILL}", pattern(""));
    let keep = render(&inside, Way::Keep).0;
    let (dropped, seen) = render(&inside, Way::Drop);
    assert_eq!(seen, [(false, true)], "false: no canvas; pixels unchanged");
    assert_eq!(dropped, keep, "nothing was dropped");
    assert_eq!(render(&inside, Way::Swap).0, keep, "swap is refused too");
    // A real drop around the pattern fill gives a different render.
    let out = render(&outside, Way::Drop).0;
    assert_eq!(render(&outside, Way::Keep).0, keep);
    assert_off_by_a_level(&keep, &out);
}

/// A `PaintProc` run under a page group: the canvas is the page below the
/// group, which is dropped as for any call made in the group.
#[test]
fn in_a_paintproc_under_a_group_the_page_below_is_dropped() {
    let inside = format!(
        "{BLACK}{} 1 setalpha << /Alpha 0.02 >> begingroup {} endgroup ",
        FILL.repeat(5),
        pattern("@")
    );
    let before = format!("{BLACK}{} @ {GROUP_OVER}", FILL.repeat(5));
    let keep = render(&inside, Way::Keep).0;
    let (dropped, seen) = render(&inside, Way::Drop);
    assert_eq!(seen, [(true, true)]);
    assert_eq!(dropped, render(&before, Way::Drop).0);
    assert_eq!(keep, render(&before, Way::Keep).0);
    assert_off_by_a_level(&keep, &dropped);
    // The swap trick is not refused here either: it reaches the same page.
    assert_eq!(render(&inside, Way::Swap).0, dropped);
    // A group the PaintProc opens itself is the tile's, not a page group:
    // with no page group the call is still false.
    let own = pattern("<< >> begingroup @ endgroup");
    let (_, seen) = render(&format!("{BLACK}{own}"), Way::Drop);
    assert_eq!(seen, [(false, true)]);
}
