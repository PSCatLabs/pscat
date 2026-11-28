//! The `gsave` depth as host API (#1497): `Interp::gsave_depth`. A host
//! operator named `probe` logs it from inside the program, so each test reads
//! what the interpreter says at the moment a level is pushed or popped, by
//! whatever route, as psanim does when it runs a caller's body. Only the
//! supported surface (`pscat::host`) is used.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pscat::{Interp, Limits};

/// An interpreter with `probe` logging the depth each time it runs.
fn interp(size: u32) -> (Interp, Rc<RefCell<Vec<usize>>>) {
    let mut it = Interp::with_page(size, size).expect("valid page size");
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    it.register_operator("probe", move |i| {
        sink.borrow_mut().push(i.gsave_depth());
        Ok(())
    })
    .expect("registers");
    (it, log)
}

/// Run `src` and give the depths its `probe`s read.
fn read(src: &str) -> Vec<usize> {
    let (mut it, log) = interp(40);
    it.run_str(src).expect("runs");
    log.take()
}

const PATTERN: &str = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20]
        /XStep 20 /YStep 20 /PaintProc { pop BODY } >> matrix makepattern";

fn pattern(body: &str) -> String {
    PATTERN.replace("BODY", body)
}

#[test]
fn a_fresh_interpreter_is_at_zero() {
    let (it, _) = interp(10);
    assert_eq!(it.gsave_depth(), 0);
    assert_eq!(read("probe"), [0]);
}

/// `gsave` adds a level, `grestore` takes one off.
#[test]
fn depth_follows_gsave_and_grestore() {
    assert_eq!(
        read("probe gsave probe gsave probe grestore probe grestore probe"),
        [0, 1, 2, 1, 0]
    );
}

/// `save` does not push a level: `gsave`s opened after it, and only those,
/// are what `restore` drops.
#[test]
fn save_pushes_no_level_and_restore_drops_what_came_after() {
    assert_eq!(
        read("probe save probe gsave probe restore probe"),
        [0, 0, 1, 0]
    );
    // A `gsave` before the `save` survives the `restore`.
    assert_eq!(
        read("gsave save gsave gsave probe restore probe grestore probe"),
        [3, 1, 0]
    );
    // Nested saves.
    assert_eq!(
        read("save gsave save gsave gsave probe restore probe restore probe"),
        [3, 1, 0]
    );
}

/// `grestore` has three floors: the innermost `save`'s depth, a group's own
/// level, and 0. At each it restores the state and pops nothing.
#[test]
fn grestore_below_the_depth_stops_at_a_floor() {
    // No save, no group: down to 0 and no lower.
    assert_eq!(
        read("probe grestore probe gsave probe grestore probe grestore probe"),
        [0, 0, 1, 0, 0]
    );
    // Under a `save`, the floor is the depth the `save` found (here 1).
    assert_eq!(
        read("gsave save gsave probe grestore probe grestore probe grestore probe restore probe"),
        [2, 1, 1, 1, 1]
    );
    // Inside a group the floor is the group's level (d + 1), and `endgroup`
    // still lands on d.
    assert_eq!(
        read(
            "gsave << >> begingroup gsave probe grestore probe grestore probe grestore probe
             endgroup probe"
        ),
        [3, 2, 2, 2, 1]
    );
}

/// `grestoreall` pops to the same floors in one go.
#[test]
fn grestoreall_pops_to_the_floor() {
    assert_eq!(read("gsave gsave gsave probe grestoreall probe"), [3, 0]);
    // Stops at the innermost `save`.
    assert_eq!(
        read("gsave save gsave gsave probe grestoreall probe restore probe"),
        [3, 1, 1]
    );
    // Stops at a group's level and keeps it; `endgroup` then lands on d.
    assert_eq!(
        read(
            "gsave << >> begingroup gsave gsave probe grestoreall probe endgroup probe
             grestoreall probe"
        ),
        [4, 2, 1, 0]
    );
}

/// `endgroup` truncates the stack to the group's level and pops that too:
/// d before `begingroup`, d + 1 in the body, d + 1 + k after the body's k
/// unmatched `gsave`s, and d after `endgroup`. The k levels are lost.
#[test]
fn endgroup_truncates_to_where_the_group_began() {
    for d in 0..3_usize {
        let setup = "gsave ".repeat(d);
        for k in 0..4_usize {
            let body = "gsave ".repeat(k);
            let r = read(&format!(
                "{setup} probe << >> begingroup probe {body} probe endgroup probe"
            ));
            assert_eq!(r, [d, d + 1, d + 1 + k, d], "d={d} k={k}");
        }
    }
    // A body that balanced its `gsave`s ends at the same place.
    assert_eq!(
        read("gsave << >> begingroup gsave gsave grestore grestore probe endgroup probe"),
        [2, 1]
    );
    // And one that restored too much cannot go below the group's level.
    assert_eq!(
        read("gsave << >> begingroup grestore grestore grestore probe endgroup probe"),
        [2, 1]
    );
}

/// The host's own use (psanim): read before the body and after `endgroup`,
/// and carry `after - before` levels across. Here the body's net is 0 even
/// though it left levels open inside the group.
#[test]
fn a_body_that_left_gsaves_open_loses_them_at_endgroup() {
    let r = read(
        "gsave probe
         << >> begingroup { gsave gsave gsave } exec probe endgroup probe
         { gsave gsave } exec probe",
    );
    // before = 1, after endgroup = 1: the three the body opened are gone,
    // and the same body outside a group keeps its two.
    assert_eq!(r, [1, 5, 1, 3]);
}

#[test]
fn nested_groups_each_return_to_their_own_start() {
    let r = read(
        "gsave << >> begingroup gsave << >> begingroup gsave probe
         endgroup probe endgroup probe",
    );
    // Inner group began at 3 (1 + group + gsave), so it ends at 3; the outer
    // began at 1.
    assert_eq!(r, [5, 3, 1]);
}

/// Whatever else closes a group puts the depth back to where `begingroup`
/// found it.
#[test]
fn a_group_closed_another_way_takes_its_levels_with_it() {
    for closer in ["erasepage", "showpage", "copypage"] {
        let r = read(&format!(
            "gsave << >> begingroup gsave probe {closer} probe"
        ));
        assert_eq!(r, [3, 1], "{closer}");
    }
    // `restore` past it.
    assert_eq!(
        read("gsave save << >> begingroup gsave probe restore probe"),
        [3, 1]
    );
}

/// Nothing else moves the depth: not the page operators, `initgraphics`,
/// `setpagedevice`.
#[test]
fn page_operators_leave_the_depth_alone() {
    for op in [
        "showpage",
        "copypage",
        "erasepage",
        "initgraphics",
        "<< /PageSize [40 40] >> setpagedevice",
    ] {
        assert_eq!(read(&format!("gsave gsave {op} probe")), [2], "{op}");
    }
}

/// A group opened while nothing paints has no layer, but its level is pushed
/// and truncated like any other.
#[test]
fn nulldevice_takes_levels_like_any_other_state() {
    let r = read(
        "gsave nulldevice probe << >> begingroup probe gsave probe endgroup probe grestore probe",
    );
    assert_eq!(r, [1, 2, 3, 1, 0]);
}

/// A caught error (`stopped`) does not unwind the stack, as it does not close
/// a group: the levels opened before the error are still there, and the
/// program goes on.
#[test]
fn a_caught_error_leaves_the_levels_where_they_are() {
    assert_eq!(
        read("gsave { gsave gsave probe nosuchop } stopped pop probe"),
        [3, 3]
    );
    // `stop` is the same.
    assert_eq!(
        read("gsave { gsave gsave probe stop } stopped pop probe"),
        [3, 3]
    );
    // Caught inside a group: the group is still open, and its `endgroup`
    // takes the levels the failed body left.
    assert_eq!(
        read(
            "gsave << >> begingroup { gsave gsave nosuchop } stopped pop probe
             endgroup probe"
        ),
        [4, 1]
    );
}

/// An uncaught error ends the run, and the interpreter does not unwind the
/// stack: the levels stay, and the next run starts from them. Only a group
/// open at the end is closed.
#[test]
fn an_uncaught_error_leaves_the_levels_and_the_next_run_starts_from_them() {
    let (mut it, log) = interp(20);
    let e = it.run_str("gsave gsave probe nosuchop");
    assert_eq!(e.map_err(|e| e.name()), Err("undefined"));
    assert_eq!(log.borrow().as_slice(), [2]);
    assert_eq!(it.gsave_depth(), 2);
    it.run_str("probe gsave probe").expect("runs");
    assert_eq!(log.borrow().as_slice(), [2, 2, 3]);
    assert_eq!(it.gsave_depth(), 3);
    // The same for a program that just ends with levels open.
    it.run_str("gsave").expect("runs");
    assert_eq!(it.gsave_depth(), 4);
    it.run_str("0 1 9 { pop grestore } for").expect("runs");
    assert_eq!(it.gsave_depth(), 0);
}

/// The end of a run, an uncaught error, a limit and the interrupt flag each
/// close the groups open (#1494), so the depth is back to what it was before
/// the first of them.
#[test]
fn a_group_open_when_the_run_stops_is_closed_with_its_levels() {
    // Normal end.
    let (mut it, _) = interp(20);
    it.run_str("gsave << >> begingroup gsave gsave")
        .expect("runs");
    assert_eq!(it.gsave_depth(), 1);
    // Uncaught error.
    let (mut it, log) = interp(20);
    let e = it.run_str("gsave << >> begingroup gsave probe nosuchop");
    assert!(e.is_err());
    assert_eq!(log.borrow().as_slice(), [3]);
    assert_eq!(it.gsave_depth(), 1);
    // A step limit.
    let (mut it, _) = interp(20);
    it.set_limits(Limits::none().with_max_steps(200))
        .expect("limits");
    let e = it.run_str("gsave gsave << >> begingroup gsave { } loop");
    assert!(e.is_err());
    assert_eq!(it.gsave_depth(), 2);
    // The interrupt flag.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    let (mut it, _) = interp(20);
    let flag = Arc::new(AtomicBool::new(false));
    it.set_interrupt(Some(flag.clone()));
    let f = flag.clone();
    it.register_operator("trip", move |_| {
        f.store(true, Ordering::SeqCst);
        Ok(())
    })
    .expect("registers");
    let e = it.run_str("gsave << >> begingroup gsave trip { } loop");
    assert_eq!(e, Err(pscat::PsError::Interrupted));
    assert_eq!(it.gsave_depth(), 1);
}

/// A program run in slices: the depth is the program's so far between them.
#[test]
fn the_depth_is_readable_between_slices() {
    let (mut it, _) = interp(20);
    it.begin_source(b"gsave gsave << >> begingroup gsave 1 1 add pop grestore");
    let mut seen = vec![it.gsave_depth()];
    while it.step_n(1).expect("steps") {
        seen.push(it.gsave_depth());
    }
    assert_eq!(seen.first(), Some(&0));
    assert_eq!(seen.iter().max(), Some(&4));
    // Never above 4, rising by one at a time; the last `grestore` brought it
    // to 3, and finishing the program closed the group, which took it to the
    // 2 `gsave`s outside it.
    assert!(seen.windows(2).all(|w| w[1] <= w[0] + 1), "{seen:?}");
    assert_eq!(seen.last(), Some(&3), "{seen:?}");
    assert_eq!(it.gsave_depth(), 2);
}

/// A pattern's `PaintProc` runs one level above the depth the fill began at,
/// sees the page's levels too, and the levels it leaves open are dropped when
/// it ends.
#[test]
fn a_paintproc_runs_one_level_up_and_its_leftovers_are_dropped() {
    let p = pattern("probe gsave probe gsave probe");
    let r = read(&format!(
        "gsave gsave probe {p} setpattern 0 0 30 30 rectfill probe"
    ));
    assert_eq!(r, [2, 3, 4, 5, 2]);
    // A balanced `PaintProc` reads the same inside and leaves the page's
    // depth as it found it.
    let p = pattern("probe gsave probe grestore probe");
    let r = read(&format!("{p} setpattern 0 0 30 30 rectfill probe"));
    assert_eq!(r, [1, 2, 1, 0]);
}

/// A group inside a `PaintProc` follows the same rule on its own start, and
/// one the page has open around it is untouched.
#[test]
fn a_group_inside_a_paintproc_and_one_around_it() {
    let p = pattern("probe << >> begingroup probe gsave endgroup probe");
    let r = read(&format!(
        "gsave << >> begingroup gsave {p} setpattern 0 0 30 30 rectfill probe
         endgroup probe"
    ));
    // Page: gsave 1, begingroup 2, gsave 3. PaintProc: 4; its group makes 5;
    // after its `endgroup` 4. After the fill the page is at 3 again; the
    // page's `endgroup` lands on 1.
    assert_eq!(r, [4, 5, 4, 3, 1]);
}

/// A Type 3 glyph's `BuildChar` runs one level up, as a `PaintProc` does.
#[test]
fn a_type3_buildchar_runs_one_level_up() {
    let t3 = "/T3 7 dict def T3 begin /FontType 3 def /FontMatrix [0.001 0 0 0.001 0 0] def
        /FontBBox [0 0 1000 1000] def /Encoding 256 array def
        0 1 255 { Encoding exch /.notdef put } for Encoding 65 /box put
        /BuildChar { pop pop probe gsave probe 500 0 setcharwidth } def end
        /T3 T3 definefont pop /T3 findfont 20 scalefont setfont";
    let r = read(&format!("{t3} gsave probe 5 5 moveto (A) show probe"));
    // 1 before; BuildChar at 2, a `gsave` in it 3; the glyph's levels are
    // dropped at its end.
    assert_eq!(r, [1, 2, 3, 1]);
}

/// The queries are reads: no step, and no pixel, is spent or changed by
/// asking. `steps_used` counts only under a limit, so one is set and the
/// count is asserted to be real.
#[test]
fn asking_costs_no_steps_and_changes_no_pixels() {
    let src = "1 0 0 setrgbcolor 0 0 20 20 rectfill
        gsave << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 5 5 20 20 rectfill probe
        gsave 0 1 0 setrgbcolor 10 10 20 20 rectfill probe grestore probe
        endgroup probe grestore probe";
    let mut results = Vec::new();
    for ask in [false, true] {
        let mut it = Interp::with_page(40, 40).expect("size");
        it.set_limits(Limits::none().with_max_steps(1_000_000))
            .expect("limits");
        let asks = Rc::new(Cell::new(0_u32));
        let a = asks.clone();
        it.register_operator("probe", move |i| {
            if ask {
                for _ in 0..1000 {
                    std::hint::black_box(i.gsave_depth());
                }
                a.set(a.get() + 1);
            }
            Ok(())
        })
        .expect("registers");
        it.run_str(src).expect("runs");
        results.push((
            it.steps_used(),
            it.canvas_premultiplied_rgba8().to_vec(),
            asks.get(),
        ));
    }
    assert!(
        results[0].0 > 20,
        "the steps are really counted: {}",
        results[0].0
    );
    assert_eq!(results[0].0, results[1].0, "steps");
    assert_eq!(results[0].1, results[1].1, "pixels");
    assert_eq!(results[1].2, 5, "every probe ran");

    // From Rust, between slices: the step counter does not move.
    let (mut it, _) = interp(20);
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("limits");
    it.begin_source(b"gsave 1 1 add pop");
    let mut slices = 0;
    while it.gsave_depth() == 0 {
        assert!(it.step_n(1).expect("steps"), "program ended early");
        slices += 1;
        assert!(slices < 100);
    }
    let used = it.steps_used();
    assert!(used > 0, "the steps are really counted");
    for _ in 0..10_000 {
        std::hint::black_box(it.gsave_depth());
    }
    assert_eq!(it.steps_used(), used);
}

/// Under a tight limit too: a query is not counted against `--max-steps`.
#[test]
fn asking_is_free_under_the_step_limit() {
    let mut it = Interp::with_page(10, 10).expect("size");
    it.register_operator("probe", |i| {
        for _ in 0..100_000 {
            std::hint::black_box(i.gsave_depth());
        }
        Ok(())
    })
    .expect("registers");
    it.set_limits(Limits::none().with_max_steps(40))
        .expect("limits");
    it.run_str("gsave probe probe probe grestore")
        .expect("runs");
    // The steps are really counted, and are far below the limit.
    assert!((1..=40).contains(&it.steps_used()), "{}", it.steps_used());
}
