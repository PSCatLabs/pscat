//! The open transparency groups as host API (#1494): `open_group_count` and
//! `innermost_group_serial`. A host operator named `probe` logs both from
//! inside the program, so each test reads what the interpreter says at the
//! moment a group opens or closes, by whatever route. Only the supported
//! surface (`pscat::host`) is used, as psanim's layers would.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pscat::{Interp, Limits, Page};

/// One reading: the group count and the innermost serial (0 for none).
type Reading = (usize, u64);

/// An interpreter with `probe` logging a reading each time it runs.
fn interp(size: u32) -> (Interp, Rc<RefCell<Vec<Reading>>>) {
    let mut it = Interp::with_page(size, size).expect("valid page size");
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    it.register_operator("probe", move |i| {
        sink.borrow_mut().push((
            i.open_group_count(),
            i.innermost_group_serial().unwrap_or(0),
        ));
        Ok(())
    })
    .expect("registers");
    (it, log)
}

fn counts(log: &RefCell<Vec<Reading>>) -> Vec<usize> {
    log.borrow().iter().map(|r| r.0).collect()
}

/// Run `src` and give the readings its `probe`s took.
fn read(src: &str) -> Vec<Reading> {
    let (mut it, log) = interp(40);
    it.run_str(src).expect("runs");
    let out = log.borrow().clone();
    out
}

#[test]
fn no_group_is_zero_and_none() {
    let (it, _) = interp(10);
    assert_eq!(it.open_group_count(), 0);
    assert_eq!(it.innermost_group_serial(), None);
    assert_eq!(read("probe"), [(0, 0)]);
}

/// Issue (a): a group opened through `systemdict /begingroup get exec` is
/// counted, which a prelude wrapper never saw.
#[test]
fn a_group_opened_through_systemdict_exec_is_counted() {
    let r = read(
        "probe << >> systemdict /begingroup get exec probe
         systemdict /endgroup get exec probe",
    );
    assert_eq!(counts_of(&r), [0, 1, 0]);
    assert_eq!((r[1].1, r[2].1), (1, 0));
    // And through `//begingroup`, a procedure and `bind`.
    let r = read(
        "/g { << >> //begingroup } bind def /e { endgroup } def
         g probe e probe g g probe",
    );
    assert_eq!(counts_of(&r), [1, 0, 2]);
}

fn counts_of(r: &[Reading]) -> Vec<usize> {
    r.iter().map(|r| r.0).collect()
}

/// Issue (b): `erasepage`, `showpage`, `copypage` and `restore` end the
/// groups the program left open, and the count follows.
#[test]
fn a_group_ended_implicitly_is_no_longer_counted() {
    for closer in ["erasepage", "showpage", "copypage"] {
        let r = read(&format!(
            "<< >> begingroup << >> begingroup probe {closer} probe"
        ));
        assert_eq!(counts_of(&r), [2, 0], "{closer}");
        assert_eq!(r[1].1, 0, "{closer}");
    }
    // `restore` ends the groups opened since its `save`, and only those.
    let r = read(
        "<< >> begingroup save << >> begingroup << >> begingroup probe
         restore probe endgroup probe",
    );
    assert_eq!(counts_of(&r), [3, 1, 0]);
    // The group that survived the restore is the first one opened.
    assert_eq!(r[1].1, 1);
}

/// A group `endgroup` closes goes at once; `endgroup` with none open is a
/// no-op and leaves the count at 0.
#[test]
fn endgroup_with_none_open_changes_nothing() {
    let r = read("endgroup probe << >> begingroup endgroup endgroup probe");
    assert_eq!(counts_of(&r), [0, 0]);
}

#[test]
fn nested_groups_count_and_name_the_innermost() {
    let r = read(
        "<< >> begingroup probe << >> begingroup probe << >> begingroup probe
         endgroup probe endgroup probe endgroup probe",
    );
    assert_eq!(counts_of(&r), [1, 2, 3, 2, 1, 0]);
    let serials: Vec<u64> = r.iter().map(|r| r.1).collect();
    // Opened as serials 1, 2, 3; closing walks back out through them.
    assert_eq!(serials, [1, 2, 3, 2, 1, 0]);
}

/// The serial: the same group keeps it, a group reopened at the same depth
/// has a different one, and it never repeats after a `restore` or a page
/// boundary, nor across runs.
#[test]
fn the_serial_names_the_group_and_never_repeats() {
    let r = read(
        "<< >> begingroup probe probe
         <<>> begingroup probe endgroup probe endgroup
         <<>> begingroup probe endgroup",
    );
    // The same group twice, then a nested one, back to the first (same
    // serial as before), and a different group reopened at depth 1.
    let s: Vec<u64> = r.iter().map(|r| r.1).collect();
    assert_eq!(counts_of(&r), [1, 1, 2, 1, 1]);
    assert_eq!(s[0], s[1]);
    assert_eq!(s[3], s[0]);
    assert_ne!(s[2], s[0]);
    assert_ne!(s[4], s[0]);
    assert_ne!(s[4], s[2]);

    // Closed by `restore`, reopened at the same depth: a new serial. A
    // page boundary and `erasepage` are no different.
    let r = read(
        "save << >> begingroup probe restore
         << >> begingroup probe erasepage
         << >> begingroup probe showpage
         << >> begingroup probe endgroup probe",
    );
    let s: Vec<u64> = r.iter().map(|r| r.1).collect();
    assert_eq!(s[..4], [1, 2, 3, 4]);
    assert_eq!(s[4], 0);
    let mut seen = s[..4].to_vec();
    seen.dedup();
    assert_eq!(seen.len(), 4);

    // Across runs on one interpreter: the counter is the interpreter's.
    let (mut it, log) = interp(20);
    it.run_str("<< >> begingroup probe endgroup").expect("runs");
    it.run_str("<< >> begingroup probe").expect("runs"); // closed with the run
    it.run_str("<< >> begingroup probe endgroup").expect("runs");
    let s: Vec<u64> = log.borrow().iter().map(|r| r.1).collect();
    assert_eq!(s, [1, 2, 3]);
    assert_eq!(it.innermost_group_serial(), None);
}

/// A group that did not open (a refused `begingroup`) takes no serial:
/// the count of groups opened is of the ones that were.
#[test]
fn a_refused_begingroup_takes_no_serial() {
    let (mut it, log) = interp(20);
    // 64 are allowed; the 65th is `limitcheck`, a caught error.
    it.run_str(
        "64 { << >> begingroup } repeat { << >> begingroup } stopped pop probe
         64 { endgroup } repeat probe << >> begingroup probe",
    )
    .expect("runs");
    let r = log.borrow().clone();
    assert_eq!(counts_of(&r), [64, 0, 1]);
    assert_eq!((r[0].1, r[2].1), (64, 65));
}

/// A caught error does not close a group, as it does not close a `gsave`:
/// `stopped` leaves it open and the program goes on inside it. The
/// uncaught ones (an error, a limit, the interrupt flag) end the program,
/// and with it the groups, so nothing is open once the run returns.
#[test]
fn error_recovery_closes_by_the_interpreters_own_rules() {
    // Caught: still open, and the count is what the interpreter says.
    let r = read("{ << >> begingroup nosuchop } stopped pop probe endgroup probe");
    assert_eq!(counts_of(&r), [1, 0]);

    // Uncaught: the run fails and the groups are gone with it.
    let (mut it, log) = interp(20);
    let e = it.run_str("<< >> begingroup << >> begingroup probe nosuchop");
    assert_eq!(e.map_err(|e| e.name()), Err("undefined"));
    assert_eq!(counts(&log), [2]);
    assert_eq!(it.open_group_count(), 0);
    assert_eq!(it.innermost_group_serial(), None);
    // ... and the interpreter is usable, with fresh serials.
    it.run_str("<< >> begingroup probe").expect("runs");
    assert_eq!(log.borrow()[1], (1, 3));

    // A limit stops the run: the same.
    let (mut it, _) = interp(20);
    it.set_limits(Limits::none().with_max_steps(200))
        .expect("limits");
    let e = it.run_str("<< >> begingroup << >> begingroup { } loop");
    assert!(e.is_err());
    assert_eq!(it.open_group_count(), 0);
    assert_eq!(it.innermost_group_serial(), None);
}

/// The interrupt flag stops the run: the groups go with it.
#[test]
fn an_interrupted_run_leaves_no_group_counted() {
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
    let e = it.run_str("<< >> begingroup << >> begingroup trip { } loop");
    assert_eq!(e, Err(pscat::PsError::Interrupted));
    assert_eq!(it.open_group_count(), 0);
    assert_eq!(it.innermost_group_serial(), None);
}

/// A group left open by a program run in slices stays open between them,
/// and is what the host sees; finishing the program closes it.
#[test]
fn a_group_left_open_by_a_slice_is_counted_between_slices() {
    let (mut it, _) = interp(20);
    it.begin_source(
        b"<< >> begingroup << >> begingroup 1 0 0 setrgbcolor 0 0 5 5 rectfill 3 3 add pop",
    );
    let mut slices = 0;
    while it.open_group_count() < 2 {
        assert!(it.step_n(1).expect("steps"), "program ended early");
        slices += 1;
        assert!(slices < 100);
    }
    assert_eq!(it.open_group_count(), 2);
    let serial = it.innermost_group_serial();
    assert_eq!(serial, Some(2));
    // Another slice that leaves it open does not change the answer.
    assert!(it.step_n(1).expect("steps"));
    assert_eq!(
        (it.open_group_count(), it.innermost_group_serial()),
        (2, serial)
    );
    // The host's own `swap_canvas` here addresses the page below the groups,
    // which is what the count says.
    while it.step_n(1000).expect("steps") {}
    assert_eq!(it.open_group_count(), 0);
    assert_eq!(it.innermost_group_serial(), None);
}

/// A group opened while nothing paints has no layer; it is counted all
/// the same (as `endgroup` counts it), and `swap_canvas` swaps the page
/// itself there. Documented on `open_group_count`.
#[test]
fn a_group_opened_under_nulldevice_is_counted() {
    let r = read("gsave nulldevice << >> begingroup probe endgroup probe grestore");
    assert_eq!(counts_of(&r), [1, 0]);
}

const PATTERN: &str = "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 20 20]
        /XStep 20 /YStep 20 /PaintProc { pop BODY } >> matrix makepattern";

fn pattern(body: &str) -> String {
    PATTERN.replace("BODY", body)
}

/// Inside a pattern's `PaintProc` with no page group open: 0 and `None`,
/// and the `PaintProc`'s own group is not the page's, so it is not counted
/// (though it took a serial).
#[test]
fn a_paintproc_with_no_page_group_reads_zero() {
    let p = pattern("probe << >> begingroup probe 0 0 5 5 rectfill endgroup probe");
    let r = read(&format!("{p} setpattern 0 0 30 30 rectfill probe"));
    assert!(r.len() >= 4, "{r:?}");
    assert!(r[..3].iter().all(|&x| x == (0, 0)), "{r:?}");
    assert_eq!(r.last(), Some(&(0, 0)));
    // The PaintProc's group took serial 1, so the next page group is 2.
    let (mut it, log) = interp(30);
    it.run_str(&format!(
        "{p} setpattern 0 0 30 30 rectfill << >> begingroup probe"
    ))
    .expect("runs");
    assert_eq!(*log.borrow().last().unwrap(), (1, 2));
}

/// Inside a `PaintProc` the count and the serial are the page's: the page
/// group opened before, whose layer `swap_canvas` would address. A PaintProc
/// `endgroup` with none of its own open is a no-op, so the answer holds, and
/// the group is still the page's after the fill.
#[test]
fn a_paintproc_sees_the_pages_group_and_its_endgroup_is_a_no_op() {
    let p = pattern("probe endgroup probe << >> begingroup probe endgroup endgroup probe");
    let (mut it, log) = interp(30);
    it.run_str(&format!(
        "<< >> begingroup {p} setpattern 0 0 30 30 rectfill probe endgroup probe"
    ))
    .expect("runs");
    let r = log.borrow().clone();
    // The PaintProc may run once per tile use; every reading inside it is
    // the page's group (count 1, serial 1), whatever it did.
    assert!(r.len() >= 5, "{r:?}");
    let n = r.len();
    assert!(r[..n - 1].iter().all(|&x| x == (1, 1)), "{r:?}");
    // The page group is still open after the fill (the PaintProc's
    // endgroups did not close it): the reading just before the page's own
    // `endgroup` is (1, 1), and after it none.
    assert_eq!(r[n - 1], (0, 0));
}

/// Nested page groups seen from the `PaintProc`: the innermost page group.
#[test]
fn a_paintproc_reports_the_innermost_page_group() {
    let p = pattern("probe");
    let (mut it, log) = interp(30);
    it.run_str(&format!(
        "<< >> begingroup << >> begingroup {p} setpattern 0 0 30 30 rectfill"
    ))
    .expect("runs");
    let r = log.borrow().clone();
    assert!(!r.is_empty());
    assert!(r.iter().all(|&x| x == (2, 2)), "{r:?}");
}

/// `swap_canvas` while a group is open exchanges the page below the
/// group's layer, so what is painted does not show on the swapped-in page
/// until `endgroup`: the count above 0 is the host's warning of exactly
/// this. With none open it shows at once.
#[test]
fn the_count_says_when_swap_canvas_addresses_the_page_below() {
    for open in [false, true] {
        let (mut it, _) = interp(8);
        let seen = Rc::new(Cell::new(0_u8));
        let s = seen.clone();
        it.register_operator("peek", move |i| {
            // The first alpha byte of the canvas the host sees.
            s.set(i.canvas_premultiplied_rgba8()[3]);
            Ok(())
        })
        .expect("registers");
        let layer = Page::transparent(8, 8).expect("size");
        it.run_str("1 0 0 setrgbcolor 0 0 8 8 rectfill")
            .expect("runs");
        // The host swaps in a transparent page between runs; slices keep
        // the group open across the swap.
        let begin = if open { "<< >> begingroup" } else { "" };
        it.begin_source(format!("{begin} 0 0 1 setrgbcolor 0 0 8 8 rectfill peek").as_bytes());
        // Run the group's opening (if any), then swap.
        let mut n = 0;
        while it.open_group_count() < usize::from(open) {
            assert!(it.step_n(1).expect("steps"));
            n += 1;
            assert!(n < 50);
        }
        assert_eq!(it.open_group_count(), usize::from(open));
        let below = it.swap_canvas(layer).expect("same size");
        // `below` is the red page either way.
        assert_eq!(&below.rgba8()[..4], &[255, 0, 0, 255], "open={open}");
        while it.step_n(1000).expect("steps") {}
        // Inside the program, after the blue fill: the swapped-in page has
        // the blue only when no group was holding it back.
        assert_eq!(seen.get(), if open { 0 } else { 255 }, "open={open}");
        // After `endgroup` (the end of the run closes it) the blue is on it.
        assert_eq!(&it.canvas_premultiplied_rgba8()[..4], &[0, 0, 255, 255]);
    }
}

/// The queries are reads: no step, and no pixel, is spent or changed by
/// asking.
#[test]
fn asking_costs_no_steps_and_changes_no_pixels() {
    let src = "1 0 0 setrgbcolor 0 0 20 20 rectfill
        << /Alpha 0.5 >> begingroup 0 0 1 setrgbcolor 5 5 20 20 rectfill probe
        << >> begingroup 0 1 0 setrgbcolor 10 10 20 20 rectfill probe endgroup probe
        endgroup probe";
    let mut results = Vec::new();
    for ask in [false, true] {
        let mut it = Interp::with_page(40, 40).expect("size");
        // `steps_used` counts only under limits.
        it.set_limits(Limits::none().with_max_steps(1_000_000))
            .expect("limits");
        let asks = Rc::new(Cell::new(0_u32));
        let a = asks.clone();
        it.register_operator("probe", move |i| {
            if ask {
                // Many times, to show it adds nothing.
                for _ in 0..1000 {
                    std::hint::black_box((i.open_group_count(), i.innermost_group_serial()));
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
    assert_eq!(results[1].2, 3 + 1);

    // From Rust, between slices: the step counter does not move.
    let (mut it, _) = interp(20);
    it.set_limits(Limits::none().with_max_steps(1_000_000))
        .expect("limits");
    it.begin_source(b"<< >> begingroup 1 1 add pop");
    while it.open_group_count() == 0 {
        it.step_n(1).expect("steps");
    }
    let used = it.steps_used();
    assert!(used > 0, "the steps are really counted");
    for _ in 0..10_000 {
        std::hint::black_box((it.open_group_count(), it.innermost_group_serial()));
    }
    assert_eq!(it.steps_used(), used);
}

/// Under limits too: a query is not counted against `--max-steps`.
#[test]
fn asking_is_free_under_the_step_limit() {
    let mut it = Interp::with_page(10, 10).expect("size");
    it.register_operator("probe", |i| {
        for _ in 0..100_000 {
            std::hint::black_box((i.open_group_count(), i.innermost_group_serial()));
        }
        Ok(())
    })
    .expect("registers");
    it.set_limits(Limits::none().with_max_steps(40))
        .expect("limits");
    // 3 probes and a group, well inside 40 steps: 100000 queries each do
    // not trip it.
    it.run_str("<< >> begingroup probe probe probe endgroup")
        .expect("runs");
}
