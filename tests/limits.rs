//! Interpreter execution limits (issue #604): steps and time. The memory
//! limit has its own test binary (`limits_memory.rs`), because it counts the
//! whole process's heap and these tests run on parallel threads.
//!
//! No test here measures wall time (AGENTS.md): a time limit is tested by the
//! error it produces, never by how long the run took.

use std::time::Duration;

use pscat::{Interp, Limits, PsError};

mod common;

fn limited(limits: Limits) -> Interp {
    let mut it = Interp::new();
    it.set_limits(limits)
        .expect("limits without memory always apply");
    it
}

fn steps(n: u64) -> Limits {
    Limits::none().with_max_steps(n)
}

#[test]
fn a_runaway_loop_stops_with_a_structured_step_error() {
    let mut it = limited(steps(10_000));
    let err = it.run_str("{ 1 pop } loop").unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    assert_eq!(err.name(), "limitexceeded");
    let report = it.error_report(&err);
    assert!(
        report.starts_with("%%[ Error: limitexceeded (steps); OffendingCommand: ")
            && report.ends_with("; Line: 1 ]%%"),
        "{report}"
    );
}

#[test]
fn loops_with_empty_bodies_are_counted_too() {
    // These never hand the step loop an object, so the iterations themselves
    // have to be counted.
    for src in [
        "{ } loop",
        "1 1 100000000000 { pop } for",
        "1 1 100000000000 { } for",
        "100000000000 { } repeat",
        "{ { } loop } loop",
    ] {
        let mut it = limited(steps(5_000));
        assert_eq!(it.run_str(src), Err(PsError::StepsExceeded), "{src}");
    }
}

#[test]
fn stopped_cannot_catch_a_limit() {
    // A caught limit would let a program carry on past it forever.
    let mut it = limited(steps(5_000));
    assert_eq!(
        it.run_str("{ { } loop } stopped pop /survived true def"),
        Err(PsError::StepsExceeded)
    );
    assert!(it.load("survived").is_none(), "the program was cut off");

    let mut it = limited(steps(5_000));
    assert_eq!(
        it.run_str("{ { { } loop } stopped pop } loop"),
        Err(PsError::StepsExceeded),
        "a loop around `stopped` still terminates"
    );
    // And nothing was recorded for `$error` to replay.
    assert!(it.run_str("$error /errorname known").is_ok());
}

#[test]
fn a_program_within_its_limits_runs_to_the_end() {
    let mut it = limited(steps(10_000_000));
    it.capture_stdout();
    it.run_str("0 1 1 100000 { add } for = ").unwrap();
    assert_eq!(it.captured_stdout(), b"5000050000\n");
    assert!(it.steps_used() > 0, "counted while a limit is set");
}

#[test]
fn a_program_of_exactly_the_cap_is_not_refused() {
    // Seven objects: `1 2 add pop 3 4 add`. Whatever the exact accounting,
    // a cap equal to the count must not fail the program.
    let mut it = limited(steps(7));
    it.run_str("1 2 add pop 3 4 add").unwrap();
}

#[test]
fn limits_restart_with_each_program() {
    let mut it = limited(steps(3_000));
    for _ in 0..10 {
        // Each run uses a good share of the cap; together they'd pass it.
        it.run_str("1 1 1000 { pop } for").unwrap();
    }
}

#[test]
fn no_limits_changes_nothing() {
    let mut it = Interp::new();
    assert!(it.limits().is_none());
    it.run_str("1 1 100000 { pop } for").unwrap();
    assert_eq!(it.steps_used(), 0, "nothing is counted without limits");
    // Taking limits off again restores the old behaviour.
    it.set_limits(steps(10)).unwrap();
    it.set_limits(Limits::none()).unwrap();
    it.run_str("1 1 100000 { pop } for").unwrap();
}

#[test]
fn a_time_limit_stops_a_loop() {
    let mut it = limited(Limits::none().with_max_time(Duration::from_millis(50)));
    assert_eq!(it.run_str("{ } loop"), Err(PsError::TimeExceeded));
    let mut it = limited(Limits::none().with_max_time(Duration::from_millis(50)));
    assert_eq!(it.run_str("{ 1 pop } loop"), Err(PsError::TimeExceeded));
}

#[test]
fn a_memory_limit_without_the_tracking_allocator_is_refused_loudly() {
    // This test binary does not install `TrackingAllocator`.
    let mut it = Interp::new();
    let err = it
        .set_limits(Limits::none().with_max_memory(1 << 20))
        .unwrap_err();
    assert_eq!(err, pscat::LimitsError::AllocatorNotInstalled);
    assert!(it.limits().is_none(), "nothing was changed");
}

#[test]
fn the_page_survives_a_limit() {
    // A stopped run keeps what it drew, as after any other error.
    let mut it = limited(steps(50_000));
    let err = it
        .run_str("1 0 0 setrgbcolor 0 0 200 200 rectfill { } loop")
        .unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    let px = it.gfx().pixmap.pixel(10, 780).expect("pixel");
    assert_eq!((px.red(), px.green(), px.blue()), (255, 0, 0));
}

#[test]
fn the_untrusted_preset_is_on() {
    let l = Limits::untrusted();
    assert!(l.max_steps.is_some() && l.max_time.is_some() && l.max_memory.is_some());
    assert!(!l.is_none());
}

#[test]
fn a_step_limit_of_u64_max_is_just_no_practical_limit() {
    let mut it = limited(steps(u64::MAX));
    it.run_str("1 1 10000 { pop } for").unwrap();
}

#[test]
fn a_program_that_finishes_inside_a_poll_period_is_still_held_to_the_cap() {
    // 1000 empty iterations is fewer than one poll period of the loop counter
    // and ends in the first slice; it must still pass a cap of 10 (Codex
    // review of #604, round 2). A cap that fits it is fine.
    let mut it = limited(steps(10));
    assert_eq!(it.run_str("1000 { } repeat"), Err(PsError::StepsExceeded));
    let mut it = limited(steps(5_000));
    it.run_str("1000 { } repeat").unwrap();
    let mut it = limited(steps(10));
    it.run_str("1 2 add pop").unwrap();
}

#[test]
fn a_cap_binds_the_objects_of_a_program_that_ends_in_one_slice() {
    // The inner loop is handed no more than the steps the program has left, so
    // a short program cannot run past a small cap just because it ends inside
    // one slice. (Loop iterations are charged in blocks, see SECURITY.md.)
    let mut it = limited(steps(10));
    assert_eq!(
        it.run_str("1 2 add pop 3 4 add pop 5 6 7 8 9"),
        Err(PsError::StepsExceeded)
    );
    let mut it = limited(steps(10));
    it.run_str("1 2 add pop 3 4 add pop 5 6").unwrap(); // exactly 10 objects
}

// Whole-file reads (issue #702): `file` and `run` read before the program sees
// a byte, so the limits are looked at while the bytes arrive.
#[cfg(unix)]
mod file_reads {
    use super::*;
    use std::io::Write;
    use std::process::Command;

    use crate::common;

    fn fifo(tag: &str) -> std::path::PathBuf {
        let dir = common::scratch_dir(tag);
        let path = dir.join("f.fifo");
        assert!(
            Command::new("mkfifo")
                .arg(&path)
                .status()
                .expect("mkfifo")
                .success()
        );
        path
    }

    fn ps(path: &std::path::Path) -> String {
        common::ps_path(path)
    }

    #[test]
    fn a_step_limit_stops_a_read_of_dev_zero() {
        // A KiB read is a step, so the cap bounds the read to about 5 MB here.
        let mut it = limited(steps(5_000));
        assert_eq!(
            it.run_str("(/dev/zero) (r) file pop"),
            Err(PsError::StepsExceeded)
        );
        // `run` reads the same way.
        let mut it = limited(steps(5_000));
        assert_eq!(it.run_str("(/dev/zero) run"), Err(PsError::StepsExceeded));
    }

    #[test]
    fn a_read_costs_one_step_per_kib() {
        let dir = common::scratch_dir("limits-kib");
        let path = dir.join("f.bin");
        std::fs::write(&path, vec![7u8; 300 * 1024]).unwrap();
        let src = format!("({}) (r) file pop", ps(&path));

        // The charge itself: 300 KiB is about 300 steps, plus the few objects
        // of the program (not 5 steps, as a count per 64 KiB chunk would be).
        let mut it = limited(steps(1_000_000));
        it.run_str(&src).unwrap();
        let used = it.steps_used();
        assert!((300..=400).contains(&used), "steps used: {used}");

        // And the cap: the same file passes a million-step cap, fails a 100-step one.
        let mut it = limited(steps(100));
        assert_eq!(it.run_str(&src), Err(PsError::StepsExceeded));
    }

    #[test]
    fn a_limit_in_a_read_cannot_be_caught() {
        let mut it = limited(steps(5_000));
        assert_eq!(
            it.run_str("{ (/dev/zero) (r) file pop } stopped pop /survived true def"),
            Err(PsError::StepsExceeded)
        );
    }

    #[test]
    fn a_time_limit_stops_a_read_of_a_fifo_nobody_writes_to() {
        // The open itself blocks; the limits are still looked at.
        let path = fifo("limits-fifo-stuck");
        let mut it = limited(Limits::none().with_max_time(Duration::from_millis(300)));
        let src = format!("({}) (r) file pop", ps(&path));
        assert_eq!(it.run_str(&src), Err(PsError::TimeExceeded));
        // A second program on the same interpreter is not affected by the
        // abandoned reader.
        assert_eq!(it.run_str("1 2 add pop"), Ok(()));
    }

    #[test]
    fn a_time_limit_stops_a_fifo_whose_writer_stalls() {
        let path = fifo("limits-fifo-stall");
        let p = path.clone();
        let writer = std::thread::spawn(move || {
            let mut f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_all(b"some bytes").unwrap();
            // Keep the pipe open and say nothing more: the read has bytes but
            // no end. The limit has to stop it.
            std::thread::sleep(Duration::from_secs(2));
        });
        let mut it = limited(Limits::none().with_max_time(Duration::from_millis(300)));
        let src = format!("({}) (r) file pop", ps(&path));
        assert_eq!(it.run_str(&src), Err(PsError::TimeExceeded));
        writer.join().unwrap();
    }

    #[test]
    fn files_still_read_under_limits() {
        let limits = Limits::none()
            .with_max_steps(1_000_000)
            .with_max_time(Duration::from_secs(60));

        // A FIFO with a writer that finishes: every byte arrives, in order.
        let path = fifo("limits-fifo-ok");
        let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let (p, b) = (path.clone(), body.clone());
        let writer = std::thread::spawn(move || {
            let mut f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            f.write_all(&b).unwrap();
        });
        let mut it = limited(limits);
        it.capture_stdout();
        let src = format!(
            "({}) (r) file 300000 string readstring pop length =",
            ps(&path)
        );
        it.run_str(&src).unwrap();
        writer.join().unwrap();
        assert_eq!(it.captured_stdout(), b"200000\n");

        // A regular file bigger than one chunk, and an empty one.
        let dir = common::scratch_dir("limits-regular");
        let big = dir.join("big.bin");
        std::fs::write(&big, &body).unwrap();
        let mut it = limited(limits);
        it.capture_stdout();
        let src = format!(
            "({}) (r) file 300000 string readstring pop dup length = 123457 get =",
            ps(&big)
        );
        it.run_str(&src).unwrap();
        assert_eq!(
            it.captured_stdout(),
            format!("200000\n{}\n", body[123_457]).as_bytes()
        );
        let mut it = limited(limits);
        it.capture_stdout();
        it.run_str("(/dev/null) (r) file read =").unwrap();
        assert_eq!(it.captured_stdout(), b"false\n");

        // A missing file is still `undefinedfilename`, a directory too.
        for src in ["(/no/such/file) (r) file", "(/) (r) file"] {
            let mut it = limited(limits);
            assert_eq!(it.run_str(src), Err(PsError::UndefinedFilename), "{src}");
        }
    }

    #[test]
    fn a_library_file_still_loads_under_limits() {
        let mut it = limited(
            Limits::none()
                .with_max_steps(100_000_000)
                .with_max_time(Duration::from_secs(60)),
        );
        it.run_str("(lib/artkit.ps) run").unwrap();
    }
}

/// One operator whose single call can do far more work than its operands
/// suggest (#1176). The limits are looked at between interpreter steps, so
/// such an operator is either made linear or looks at them itself. Each
/// worst case runs on a thread under a deadline, so a regression fails the
/// test instead of hanging the suite, and must stop (or finish) within a
/// CPU budget a few hundred times below what it took before the fix.
mod one_operator {
    use super::*;
    use crate::common;

    /// The MCP server's preset with a short clock. Its memory cap needs the
    /// tracking allocator, which this binary does not install (the memory
    /// side is in `limits_memory.rs`), so that one is left off.
    fn untrusted(time: Duration) -> Limits {
        let mut l = Limits::untrusted().with_max_time(time);
        l.max_memory = None;
        l
    }

    /// `f` on its own thread, failed if it has not returned within the
    /// suite's child deadline (60 s).
    fn on_a_deadline<T: Send + 'static>(
        what: &'static str,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> T {
        use std::sync::mpsc::RecvTimeoutError;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name(what.into())
            .spawn(move || {
                let _ = tx.send(f());
            })
            .expect("spawn the test thread");
        match rx.recv_timeout(common::default_deadline()) {
            Ok(v) => v,
            Err(RecvTimeoutError::Timeout) => {
                panic!("{what}: still running at the deadline; the operator does not stop")
            }
            Err(RecvTimeoutError::Disconnected) => panic!("{what}: the test thread panicked"),
        }
    }

    /// Run `src` under `limits` within `budget` of CPU (best of 3), on a
    /// deadline; the program's result and its captured stdout.
    fn run(
        what: &'static str,
        limits: Limits,
        budget: Duration,
        src: &'static str,
    ) -> (Result<(), PsError>, String) {
        on_a_deadline(what, move || {
            common::within_cpu_budget(what, budget, 3, || {
                let mut it = limited(limits);
                it.capture_stdout();
                let r = it.run_str(src);
                (
                    r,
                    String::from_utf8_lossy(it.captured_stdout()).into_owned(),
                )
            })
        })
    }

    /// Run `setup` with no limits (its time does not count), then `src`
    /// under a 100 ms clock, within `budget` of CPU (best of 3), on a
    /// deadline. It must stop with `limitexceeded (time)` blamed on `op`.
    /// The blame is what shows the operator looked at the limits itself:
    /// without that, the check at the end of the slice stops the program
    /// after the operator has returned and blames what ran next (the
    /// `1 pop` appended here), as `search` blamed the `=` after it.
    fn stops_inside(
        what: &'static str,
        setup: String,
        src: &'static str,
        op: &'static str,
        budget: Duration,
    ) {
        stops_inside_with(
            what,
            setup,
            src,
            op,
            budget,
            untrusted(Duration::from_millis(100)),
            PsError::TimeExceeded,
        );
    }

    /// [`stops_inside`] under other `limits`, expecting the error `want`.
    fn stops_inside_with(
        what: &'static str,
        setup: String,
        src: &'static str,
        op: &'static str,
        budget: Duration,
        limits: Limits,
        want: PsError,
    ) {
        let (err, report) = on_a_deadline(what, move || {
            common::within_cpu_budget(what, budget, 3, || {
                let mut it = Interp::new();
                it.run_str(&setup).expect("the setup runs");
                it.set_limits(limits)
                    .expect("limits without memory always apply");
                let err = it
                    .run_str(&format!("{src} 1 pop"))
                    .expect_err("a limit stops it");
                let report = it.error_report(&err);
                (err, report)
            })
        });
        assert_eq!(err, want, "{what}: {report}");
        assert!(
            report.contains(&format!("OffendingCommand: {op};")),
            "{what}: stopped after `{op}` returned, not inside it: {report}"
        );
    }

    #[test]
    fn search_is_linear() {
        // A 16 MiB string searched for an 8 MiB pattern that is not there
        // until its last byte: O(n*m) took about 20 minutes in this one call.
        let (r, out) = run(
            "search",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(2),
            "/s 16777216 string def /p 8388608 string def p 8388607 1 put \
             s p search { (found) = } { (notfound) = } ifelse \
             s 16777215 1 put s p search { length = length = length = } { (notfound) = } ifelse",
        );
        assert_eq!(r, Ok(()));
        assert_eq!(out, "notfound\n8388608\n8388608\n0\n");
    }

    #[test]
    fn bind_walks_a_shared_procedure_once() {
        // Two references to itself: 2^100 views walked before #1176. And
        // sixty levels each holding the one below twice: 2^60.
        let (r, out) = run(
            "bind",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(1),
            "/a [null null /add cvx] cvx def /a load 0 /a load put /a load 1 /a load put \
             /a load bind pop /a load 2 get type = /a load 0 get /a load eq = \
             /p { 1 2 add } def 60 { /p [ /p load dup ] cvx def } repeat /p load bind pop \
             /q /p load def 60 { /q /q load 0 get def } repeat /q load 2 get type =",
        );
        assert_eq!(r, Ok(()));
        // Still bound all the way down, and the cycle still a cycle.
        assert_eq!(out, "operatortype\ntrue\noperatortype\n");
    }

    #[test]
    fn bind_over_overlapping_views_stops_at_the_time_limit() {
        // Every suffix of one array, each a distinct view: n^2/2 elements
        // walked, which the visited set cannot shorten. bind looks at the
        // limits as it goes.
        stops_inside(
            "bind over views",
            "/n 300000 def /S n array def /A n array def \
             0 1 n 1 sub { /i exch def A i S i n i sub getinterval cvx put } for"
                .into(),
            "/A load cvx bind pop",
            "bind",
            Duration::from_secs(2),
        );
    }

    #[test]
    fn bind_with_a_deep_dictionary_stack_stops_inside_bind() {
        // Each unresolved name is looked up through every dictionary on the
        // stack: 200,000 names under 2,000 dictionaries is 4e8 probes in
        // one `bind` (Codex, PR #1186: 60,000 names ran 1.3 s past a 0.1 s
        // limit, blamed on the next operator; the CI runner's `bind` later
        // finished those inside 0.1 s, so the test went red on main, #1219).
        // The walk counts lookups as work.
        stops_inside(
            "bind under a deep dictionary stack",
            "2000 { 1 dict begin } repeat /p [ 200000 { /zz_not_defined cvx } repeat ] cvx def"
                .into(),
            "/p load bind pop",
            "bind",
            Duration::from_secs(2),
        );
    }

    #[test]
    fn a_huge_arc_sweep_stops_at_the_step_limit() {
        // Segments are steps (one per 1024), charged as the loop runs, so a
        // step cap stops a sweep that has no end (Codex, PR #1186: it needed
        // an outside kill under `--max-steps 10`).
        stops_inside_with(
            "arc sweep, steps",
            String::new(),
            "newpath 0 0 10 -1e15 0 arc",
            "arc",
            Duration::from_secs(2),
            steps(10),
            PsError::StepsExceeded,
        );
    }

    #[test]
    fn arc_turns_like_gs_over_whole_and_near_whole_turns() {
        // `ang1 ang2 arc` and `arcn` path element counts (moveto included)
        // from gs 10, over whole turns in both directions and a hundredth of
        // a degree either side of them. Codex found `2160 -2520 arc` drawing
        // a full circle with the first #1176 normalisation (gs and main: 2).
        // pscat's turning is main's loop exactly (`turn_past`, unit-tested
        // bit for bit against it); these pin the picture to gs.
        const GS: &[(f64, f64, u32, u32)] = &[
            (-2520.0, -2520.0, 2, 2),
            (-2520.0, -2160.0, 5, 2),
            (-2520.0, -360.0, 25, 2),
            (-2520.0, 0.0, 29, 2),
            (-2520.0, 360.0, 33, 2),
            (-2520.0, 2160.0, 53, 2),
            (-2520.0, 4680.0, 81, 2),
            (-2160.0, -2520.0, 2, 5),
            (-2160.0, -2160.0, 2, 2),
            (-2160.0, -360.0, 21, 2),
            (-2160.0, 0.0, 25, 2),
            (-2160.0, 360.0, 29, 2),
            (-2160.0, 2160.0, 49, 2),
            (-2160.0, 4680.0, 77, 2),
            (-360.0, -2520.0, 2, 25),
            (-360.0, -2160.0, 2, 21),
            (-360.0, -360.0, 2, 2),
            (-360.0, 0.0, 5, 2),
            (-360.0, 360.0, 9, 2),
            (-360.0, 2160.0, 29, 2),
            (-360.0, 4680.0, 57, 2),
            (0.0, -2520.0, 2, 29),
            (0.0, -2160.0, 2, 25),
            (0.0, -360.0, 2, 5),
            (0.0, 0.0, 2, 2),
            (0.0, 360.0, 5, 2),
            (0.0, 2160.0, 25, 2),
            (0.0, 4680.0, 53, 2),
            (360.0, -2520.0, 2, 33),
            (360.0, -2160.0, 2, 29),
            (360.0, -360.0, 2, 9),
            (360.0, 0.0, 2, 5),
            (360.0, 360.0, 2, 2),
            (360.0, 2160.0, 21, 2),
            (360.0, 4680.0, 49, 2),
            (2160.0, -2520.0, 2, 53),
            (2160.0, -2160.0, 2, 49),
            (2160.0, -360.0, 2, 29),
            (2160.0, 0.0, 2, 25),
            (2160.0, 360.0, 2, 21),
            (2160.0, 2160.0, 2, 2),
            (2160.0, 4680.0, 29, 2),
            (4680.0, -2520.0, 2, 81),
            (4680.0, -2160.0, 2, 77),
            (4680.0, -360.0, 2, 57),
            (4680.0, 0.0, 2, 53),
            (4680.0, 360.0, 2, 49),
            (4680.0, 2160.0, 2, 29),
            (4680.0, 4680.0, 2, 2),
            (-359.99, -2520.0, 5, 26),
            (-360.0, -2519.99, 2, 25),
            (-360.01, -2520.0, 2, 25),
            (-360.0, -2520.01, 5, 26),
            (-359.99, 0.0, 5, 2),
            (-360.0, 0.01, 6, 5),
            (-360.01, 0.0, 6, 5),
            (-360.0, -0.01, 5, 2),
            (-359.99, 2520.0, 33, 2),
            (-360.0, 2520.01, 34, 5),
            (-360.01, 2520.0, 34, 5),
            (-360.0, 2519.99, 33, 2),
            (2160.01, -2520.0, 5, 54),
            (2160.0, -2519.99, 2, 53),
            (2159.99, -2520.0, 2, 53),
            (2160.0, -2520.01, 5, 54),
            (2160.01, 0.0, 5, 26),
            (2160.0, 0.01, 2, 25),
            (2159.99, 0.0, 2, 25),
            (2160.0, -0.01, 5, 26),
            (2160.01, 2520.0, 5, 2),
            (2160.0, 2520.01, 6, 5),
            (2159.99, 2520.0, 6, 5),
            (2160.0, 2519.99, 5, 2),
        ];
        // Where a sweep is a whole number of quarter turns, pscat (main and
        // this branch alike) makes one more segment than gs: its sweep in
        // double precision is a hair over the multiple that gs's single
        // precision lands on. Not a turning difference.
        const ONE_MORE: &[(f64, f64, bool)] = &[
            (-2520.0, 2160.0, true),
            (-360.0, 4680.0, true),
            (0.0, 4680.0, true),
            (2160.0, -2520.0, false),
            (2160.0, 4680.0, true),
            (4680.0, -360.0, false),
            (4680.0, 0.0, false),
            (4680.0, 2160.0, false),
        ];
        let mut src = String::from(
            "/cnt { /n 0 def {pop pop /n n 1 add def} {pop pop /n n 1 add def} \
             {6 {pop} repeat /n n 1 add def} {/n n 1 add def} pathforall n } def\n",
        );
        let mut want = String::new();
        for &(a1, a2, arc, arcn) in GS {
            for (op, gs, ccw) in [("arc", arc, true), ("arcn", arcn, false)] {
                src.push_str(&format!("newpath 100 100 10 {a1} {a2} {op} cnt =\n"));
                let extra = u32::from(ONE_MORE.contains(&(a1, a2, ccw)));
                want.push_str(&format!("{}\n", gs + extra));
            }
        }
        let mut it = limited(untrusted(Duration::from_secs(30)));
        it.capture_stdout();
        it.run_str(&src).unwrap();
        assert_eq!(String::from_utf8_lossy(it.captured_stdout()), want);
    }

    #[test]
    fn arc_turns_a_huge_angle_in_one_step() {
        // gs 10 refuses a positive angle past 2^31 degrees:
        //   $ gs -q -dNODISPLAY -dBATCH -c '0 0 10 2.15e9 0 arc'
        //   Error: /limitcheck in --arc--
        // (`2.14e9 0 arc` and `0 -1e10 arc` draw.) Before #1176 the first
        // two here turned ang2 a full turn at a time: forever for 1e20.
        for src in [
            "newpath 0 0 10 1e20 0 arc",
            "newpath 0 0 10 1e15 0 arc",
            "newpath 0 0 10 0 1e10 arc",
            "newpath 0 0 10 0 1e10 arcn",
            "newpath 0 0 10 0 -1e17 arc",
        ] {
            let (r, _) = run(
                "arc",
                untrusted(Duration::from_secs(30)),
                Duration::from_secs(1),
                src,
            );
            assert_eq!(r, Err(PsError::Limitcheck), "{src}");
        }
        // The element counts gs gives (`pathforall`, moveto included).
        for (args, gs) in [
            ("720 0", 2),
            ("0 720", 9),
            ("3000 0", 4),
            ("1e9 0", 2),
            ("2.14e9 0", 4),
            ("0 -1e10", 2),
            ("0 7200", 81),
        ] {
            let mut it = limited(untrusted(Duration::from_secs(30)));
            it.capture_stdout();
            let src = format!(
                "/n 0 def newpath 0 0 10 {args} arc \
                 {{pop pop /n n 1 add def}} {{pop pop /n n 1 add def}} \
                 {{6 {{pop}} repeat /n n 1 add def}} {{/n n 1 add def}} pathforall n ="
            );
            it.run_str(&src).unwrap();
            assert_eq!(
                it.captured_stdout(),
                format!("{gs}\n").as_bytes(),
                "{args} arc"
            );
        }
    }

    #[test]
    fn a_huge_arc_sweep_stops_at_the_time_limit() {
        // A sweep of 1e15 degrees is 1.1e13 segments in one `arc` (gs runs
        // it until it is killed too); the segment loop looks at the limits.
        stops_inside(
            "arc sweep",
            String::new(),
            "newpath 0 0 10 -1e15 0 arc",
            "arc",
            Duration::from_secs(2),
        );
    }

    #[test]
    fn a_shared_stitching_function_is_a_limitcheck_not_2_to_the_30_parses() {
        let (r, _) = run(
            "shfill",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(1),
            "/f <</FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1>> def \
             30 { /f <</FunctionType 3 /Domain [0 1] /Functions [f f] /Bounds [.5] \
             /Encode [0 1 0 1]>> def } repeat \
             <</ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 1 0] /Function f>> shfill",
        );
        assert_eq!(r, Err(PsError::Limitcheck));
        // A real stitching function, legs shared or not, still parses.
        let (r, _) = run(
            "shfill",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(1),
            "/f <</FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1>> def \
             /g <</FunctionType 3 /Domain [0 1] /Functions [f f f f] /Bounds [.25 .5 .75] \
             /Encode [0 1 1 0 0 1 1 0]>> def \
             <</ShadingType 2 /ColorSpace /DeviceGray /Coords [0 0 100 0] /Function g>> shfill",
        );
        assert_eq!(r, Ok(()));
    }

    /// [`stops_inside_with`] on an interpreter `prep` has set up (an SVG
    /// or PDF recorder), under a `clock` time limit.
    fn stops_inside_on(
        what: &'static str,
        prep: fn(&mut Interp),
        src: &'static str,
        op: &'static str,
        clock: Duration,
        budget: Duration,
    ) {
        let (err, report) = on_a_deadline(what, move || {
            common::within_cpu_budget(what, budget, 3, || {
                let mut it = Interp::new();
                prep(&mut it);
                it.set_limits(untrusted(clock))
                    .expect("limits without memory always apply");
                let err = it
                    .run_str(&format!("{src} 1 pop"))
                    .expect_err("a limit stops it");
                let report = it.error_report(&err);
                (err, report)
            })
        });
        assert_eq!(err, PsError::TimeExceeded, "{what}: {report}");
        assert!(
            report.contains(&format!("OffendingCommand: {op};")),
            "{what}: stopped after `{op}` returned, not inside it: {report}"
        );
    }

    #[test]
    fn a_fill_of_very_many_curves_stops_inside_fill() {
        // 100,000 page-sized curves are 10^8 edges for the rasteriser
        // (#1188: 1.2 s at 72 dpi and 5 s at 300 dpi in one `fill`, then
        // blamed on the next operator). It looks at the limits as it
        // flattens and as it lays edges into bands.
        stops_inside(
            "fill",
            "newpath 0 0 moveto 100000 { 600 800 -200 900 300 0 curveto } repeat".into(),
            "fill",
            "fill",
            Duration::from_secs(2),
        );
    }

    /// The program's current path as `pathforall` sees it: elements.
    fn path_elements(it: &mut Interp) -> String {
        it.set_limits(Limits::none()).expect("no limits");
        it.capture_stdout();
        it.run_str(
            "0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 1 add } { 1 add } \
             pathforall =",
        )
        .expect("pathforall");
        String::from_utf8_lossy(it.captured_stdout())
            .trim()
            .to_string()
    }

    #[test]
    fn a_step_limit_stops_flattenpath_and_fill_before_they_commit() {
        // Codex, PR #1217: the step charge came after the path was replaced
        // and after the fill painted. The whole count is now held against
        // the limit first: the path stays as it was, and nothing is painted.
        let mut it = Interp::new();
        it.run_str("newpath 0 0 moveto 10000000 10000000 -10000000 10000000 0 0 curveto")
            .unwrap();
        it.set_limits(steps(12)).unwrap();
        assert_eq!(it.run_str("flattenpath"), Err(PsError::StepsExceeded));
        assert_eq!(
            path_elements(&mut it),
            "2",
            "moveto and curveto, not chords"
        );

        let mut it = Interp::new();
        it.run_str("0 setgray newpath 0 0 moveto 1000 { 600 800 -200 900 300 0 curveto } repeat")
            .unwrap();
        // About 120,000 edges, over a hundred steps of work.
        it.set_limits(steps(20)).unwrap();
        assert_eq!(it.run_str("fill"), Err(PsError::StepsExceeded));
        let painted = it
            .gfx()
            .pixmap
            .pixels()
            .iter()
            .filter(|p| p.red() != 255)
            .count();
        assert_eq!(painted, 0, "the fill painted nothing");
        // The same fill under a step cap it fits paints.
        let mut it = Interp::new();
        it.run_str("0 setgray newpath 0 0 moveto 1000 { 600 800 -200 900 300 0 curveto } repeat")
            .unwrap();
        it.set_limits(steps(1_000_000)).unwrap();
        it.run_str("fill").unwrap();
        assert!(it.gfx().pixmap.pixels().iter().any(|p| p.red() != 255));
    }

    #[test]
    fn a_fill_of_flat_curves_counts_the_chords_it_drops() {
        // Codex, PR #1217: every chord of a curve along y = 792 is horizontal
        // and dropped, so counting the edges kept saw no work at all; 100,000
        // such curves are 10^8 chords, 100,000 steps of work.
        let mut it = Interp::new();
        it.run_str(
            "newpath 0 792 moveto 100000 { 1e10 792 -1e10 792 0 792 curveto } repeat \
             0 791 lineto closepath",
        )
        .unwrap();
        it.set_limits(steps(100)).unwrap();
        assert_eq!(it.run_str("fill"), Err(PsError::StepsExceeded));
        // And under a clock it stops inside the fill.
        stops_inside(
            "fill of flat curves",
            "newpath 0 792 moveto 100000 { 1e10 792 -1e10 792 0 792 curveto } repeat \
             0 791 lineto closepath"
                .into(),
            "fill",
            "fill",
            Duration::from_secs(2),
        );
    }

    #[test]
    fn limits_set_while_an_image_is_paused_reach_its_recorder() {
        // Codex, PR #1217: the image keeps the limits it began under; a host
        // that pauses it, sets limits and resumes must not run on the old
        // (here: none). The SVG recorder then stops inside `image`.
        let mut it = Interp::new();
        it.enable_svg();
        it.begin_source(
            b"/row 64000 string def 100 100 scale \
              8000 8000 8 [8000 0 0 8000 0 0] { row } image 1 pop",
        );
        // Into the image's data procedure calls, then pause.
        for _ in 0..20 {
            assert!(it.step_n(10).expect("runs"), "still in the image");
        }
        it.set_limits(untrusted(Duration::from_millis(100)))
            .unwrap();
        let err = loop {
            match it.step_n(1000) {
                Ok(true) => {}
                Ok(false) => panic!("the image ran to the end past its limit"),
                Err(e) => break e,
            }
        };
        let report = it.error_report(&err);
        assert_eq!(err, PsError::TimeExceeded, "{report}");
        assert!(report.contains("OffendingCommand: image;"), "{report}");
    }

    #[test]
    fn a_refused_fill_after_showpage_leaves_the_page_alone() {
        // Codex, PR #1217: the page was prepared (the pending erase done, the
        // recorders cleared, a new page marked painted) before the fill was
        // refused, so the export gained a blank last page.
        let mut it = Interp::new();
        it.enable_svg();
        it.run_str("0 setgray 10 10 50 50 rectfill showpage")
            .unwrap();
        it.run_str("newpath 0 0 moveto 1000 { 600 800 -200 900 300 0 curveto } repeat")
            .unwrap();
        it.set_limits(steps(20)).unwrap();
        assert_eq!(it.run_str("fill"), Err(PsError::StepsExceeded));
        // The first page is still on the canvas, its erase still pending.
        let p = it.gfx().pixmap.pixel(30, 792 - 30).expect("in bounds");
        assert_eq!((p.red(), p.green(), p.blue()), (0, 0, 0));
        assert_eq!(it.svg_pages().expect("svg on").len(), 1, "no blank page");
    }

    #[test]
    fn flattenpath_of_huge_curves_stops_inside_flattenpath() {
        // Each curve is 65,536 chords (#1188: 2000 of them were 1.3 s and
        // about 1 GB in one `flattenpath`).
        stops_inside(
            "flattenpath",
            "newpath 0 0 moveto 2000 { 1e20 1e20 -1e20 1e20 0 0 curveto } repeat".into(),
            "flattenpath",
            "flattenpath",
            Duration::from_secs(2),
        );
    }

    #[test]
    fn the_svg_and_pdf_image_recorders_stop_inside_image() {
        // A 64-megapixel image with no data under a time limit alone: the
        // recorder decodes every pixel in one step (#1188: about 0.5 s past
        // a 0.2 s limit, blamed on the next operator). They look once a row.
        let (clock, budget) = (Duration::from_millis(100), Duration::from_secs(2));
        let src = "100 100 scale 8000 8000 8 [8000 0 0 8000 0 0] {()} image";
        stops_inside_on("svg image", Interp::enable_svg, src, "image", clock, budget);
        stops_inside_on("pdf image", Interp::enable_pdf, src, "image", clock, budget);
        // A stenciled one: a 64 x 64 image under a 4096 x 4096 mask is a
        // 16-megapixel composite, about 0.25 s; a shorter clock, so a fast
        // machine still meets the limit inside the image.
        let clock = Duration::from_millis(20);
        let src = "/DeviceRGB setcolorspace 100 100 scale \
            << /ImageType 3 /InterleaveType 3 \
            /DataDict << /ImageType 1 /Width 64 /Height 64 /BitsPerComponent 8 \
              /Decode [0 1 0 1 0 1] /ImageMatrix [64 0 0 -64 0 64] /DataSource {()} >> \
            /MaskDict << /ImageType 1 /Width 4096 /Height 4096 /BitsPerComponent 1 \
              /ImageMatrix [4096 0 0 -4096 0 4096] /DataSource {()} >> >> image";
        stops_inside_on(
            "svg stencil",
            Interp::enable_svg,
            src,
            "image",
            clock,
            budget,
        );
        stops_inside_on(
            "pdf stencil",
            Interp::enable_pdf,
            src,
            "image",
            clock,
            budget,
        );
        // A data procedure runs between the image's steps (Codex, PR #1217):
        // the limit the recorder finds still blames `image`, not the
        // procedure's last operator (`pop`).
        let clock = Duration::from_millis(100);
        let src = "100 100 scale 8000 8000 8 [8000 0 0 8000 0 0] { () dup pop } image";
        stops_inside_on(
            "svg image, proc",
            Interp::enable_svg,
            src,
            "image",
            clock,
            budget,
        );
    }

    #[test]
    fn bind_looks_a_long_name_up_by_its_id() {
        // One 8,000,000-byte name under 2,000 dictionaries: its text was
        // hashed again in every one (#1188: 4 s, under a 0.1 s limit,
        // between two looks at the limits).
        let (r, out) = run(
            "bind long name",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(1),
            "/n 8000000 string cvn def 2000 { 1 dict begin } repeat \
             [ n cvx /add cvx ] cvx bind 1 get type =",
        );
        assert_eq!(r, Ok(()));
        assert_eq!(out, "operatortype\n");
    }

    /// The definitions of a stitching-function ramp of `n` legs (2 stops each).
    fn ramp_setup_of_legs(n: u32) -> String {
        format!(
            "/n {n} def \
            /fns [ 0 1 n 1 sub {{ 2 mod 0 eq \
              {{ << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> }} \
              {{ << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [1 1 0] /N 1 >> }} ifelse }} for ] def \
            /bounds [ 1 1 n 1 sub {{ n div }} for ] def \
            /encode [ 0 1 n 1 sub {{ pop 0 1 }} for ] def"
        )
    }

    /// One `shfill` of the ramp across the page.
    const RAMP_SHFILL: &str = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 612 0] \
        /Function << /FunctionType 3 /Domain [0 1] /Functions fns \
        /Bounds bounds /Encode encode >> >> shfill";

    /// A fresh letter page at `scale` (300 dpi is 300/72), the ramp of `n`
    /// legs set up and then painted under limits: the run's result and the
    /// steps the `shfill` alone used (a `run_str` counts its own).
    fn shfill_steps(n: u32, scale: f32) -> (Result<(), PsError>, u64) {
        let mut it = Interp::with_page_scaled(612, 792, scale).expect("page");
        it.set_limits(untrusted(Duration::from_secs(30)))
            .expect("limits without memory always apply");
        if let Err(e) = it.run_str(&ramp_setup_of_legs(n)) {
            return (Err(e), 0);
        }
        // Each `run_str` under limits counts its steps afresh.
        let r = it.run_str(RAMP_SHFILL);
        (r, it.steps_used())
    }

    #[test]
    fn a_ramp_of_thousands_of_legs_paints_in_bounded_time() {
        // 4095 legs are 8190 stops, and tiny-skia's gradient walks every
        // stop at every pixel (#1188: 5 s in one `shfill` at 300 dpi). The
        // raster gets the ramp resampled to 1024 stops.
        //
        // The check is the work the `shfill` charges against the step limit
        // (#1219: pixels x (16 + stops / 4), a step per 1024 units), not
        // the clock (#1336). A CPU budget flaked on the Windows runner: with
        // no per-thread CPU clock there (`common::thread_cpu_time` falls
        // back to wall time since its first call), its best of three was
        // 2.000 s, 2.357 s and 2.356 s against a 2 s budget, on a render
        // that takes 0.25-0.8 s of CPU on the unix runners. The charge is the same on every machine
        // and profile. Resampled, 4095 legs charge what 511 legs (1022
        // stops, not resampled) do, a ratio of about 1; unresampled they
        // charge about 7.6x. The bound sits between, and goes through
        // `scale_budget` (a Duration helper, used here for a plain number)
        // so `PSCAT_TEST_BUDGET_SCALE` makes it bite. A page at 1 pixel per
        // point keeps the two runs cheap: the ratio does not depend on the
        // page size.
        const MAX_RATIO: f64 = 1.5;
        let (r, thousands) = on_a_deadline("shfill legs steps", || shfill_steps(4095, 1.0));
        assert_eq!(r, Ok(()));
        let (r, ordinary) = on_a_deadline("shfill 511 legs steps", || shfill_steps(511, 1.0));
        assert_eq!(r, Ok(()));
        let ratio = thousands as f64 / ordinary as f64;
        eprintln!(
            "shfill legs: 4095 legs {thousands} steps, 511 legs {ordinary} steps, ratio {ratio:.2}"
        );
        assert!(
            ratio < common::scale_budget(Duration::from_secs_f64(MAX_RATIO)).as_secs_f64(),
            "4095 legs charged {ratio:.2}x the steps of 511 legs ({thousands} vs {ordinary}); \
             resampled to 1024 stops they should charge about the same"
        );
    }

    /// The charge above is computed from the stops the raster is given; this
    /// is the real cost, on the runners whose clock is CPU time. Windows
    /// times wall-clock (`common::thread_cpu_time`), where parallel test
    /// threads and a slow runner made 2 s a coin flip (#1336), so it is not
    /// asserted there.
    #[cfg(unix)]
    #[test]
    fn a_ramp_of_thousands_of_legs_takes_bounded_cpu() {
        let r = on_a_deadline("shfill legs", move || {
            common::within_cpu_budget("shfill legs", Duration::from_secs(2), 3, || {
                // Letter at 300 dpi.
                let mut it = Interp::with_page_scaled(612, 792, 300.0 / 72.0).expect("page");
                it.set_limits(untrusted(Duration::from_secs(30)))
                    .expect("limits without memory always apply");
                it.run_str(&ramp_setup_of_legs(4095))?;
                it.run_str(RAMP_SHFILL)
            })
        });
        assert_eq!(r, Ok(()));
    }

    #[test]
    fn a_type1_glyph_decrypts_only_the_subroutines_it_calls() {
        // Every glyph decrypted every Subrs entry: 1000 of 64 KiB, 200
        // glyphs, took 25 s past a 1 s limit (#1188). Now only subr 0.
        let src: &'static str = Box::leak(
            common::type1_probe(
                0,
                1000,
                65536,
                1,
                "200 { (A) show } repeat currentpoint pop =",
            )
            .into_boxed_str(),
        );
        let (r, out) = run(
            "type1 subrs",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(2),
            src,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(out, "1210.0\n", "200 advances of 6");
        // And one subroutine of 8 MB called 30,000 times was copied each
        // time (240 GB); now it is shared.
        let src: &'static str = Box::leak(
            common::type1_probe(8 << 20, 0, 0, 30_000, "(A) show currentpoint pop =")
                .into_boxed_str(),
        );
        let (r, out) = run(
            "type1 callsubr",
            untrusted(Duration::from_secs(30)),
            Duration::from_secs(2),
            src,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(out, "16.0\n");
    }

    #[test]
    fn type1_glyphs_that_each_decrypt_a_huge_subroutine_stop_inside_show() {
        // Each glyph decrypts the 16 MB subroutine it calls, once: 500 of
        // them were 12 s in one slice of 512 steps (#1188). The glyph looks
        // at the limits before each subroutine it decrypts.
        stops_inside(
            "type1 show",
            common::type1_probe(16_000_000, 0, 0, 1, ""),
            "500 { (A) show } repeat",
            "show",
            Duration::from_secs(2),
        );
    }

    /// A RunLengthDecode stream (each pair 0x81 c is 128 copies of c, with
    /// c changing every 100 pairs) under FlateDecode: about 3000 bytes out
    /// per byte in. Written when Flate lost a byte after any byte that made
    /// more than 512 (#1187), so it never does that.
    fn bomb(pairs: usize) -> (Vec<u8>, usize) {
        use std::io::Write;
        let mut rle = Vec::with_capacity(pairs * 2 + 1);
        let mut x: u32 = 1;
        for i in 0..pairs {
            if i % 100 == 0 {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            }
            rle.extend_from_slice(&[0x81, (x >> 16) as u8]);
        }
        rle.push(0x80);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&rle).expect("compress in memory");
        (z.finish().expect("compress in memory"), pairs * 128)
    }

    #[test]
    fn flushfile_on_a_decode_filter_stops_at_the_time_limit() {
        let dir = common::scratch_dir("limits-flushfile");
        // The same stream, small: it decodes in full (the count proves the
        // big one is a long drain, not an early decode error).
        let (small, small_len) = bomb(10_000);
        let small_path = dir.join("small.z");
        std::fs::write(&small_path, small).expect("write scratch file");
        let mut it = Interp::new();
        it.capture_stdout();
        it.run_str(&format!(
            "/f ({}) (r) file /FlateDecode filter /RunLengthDecode filter def \
             /n 0 def /buf 65536 string def \
             {{ f buf readstring exch length n add /n exch def not {{ exit }} if }} loop n =",
            common::ps_path(&small_path)
        ))
        .unwrap();
        assert_eq!(it.captured_stdout(), format!("{small_len}\n").as_bytes());

        // 4 GiB out of a 150 KB file, in one `flushfile`.
        let (big, _) = bomb(32 << 20);
        let big_path = dir.join("big.z");
        std::fs::write(&big_path, big).expect("write scratch file");
        let setup = format!(
            "/f ({}) (r) file /FlateDecode filter /RunLengthDecode filter def",
            common::ps_path(&big_path)
        );
        stops_inside(
            "flushfile",
            setup,
            "f flushfile",
            "flushfile",
            Duration::from_secs(2),
        );
    }

    /// 4 GiB of spaces from about 150 KB: RunLengthDecode pairs (0x81 0x20
    /// is 128 spaces) under FlateDecode, built once for every test here.
    fn space_bomb() -> &'static [u8] {
        use std::io::Write;
        static BOMB: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        BOMB.get_or_init(|| {
            let pairs = 32usize << 20;
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            let chunk = [0x81u8, 0x20].repeat(1 << 16);
            for _ in 0..pairs / (1 << 16) {
                z.write_all(&chunk).expect("compress in memory");
            }
            z.write_all(&[0x80]).expect("compress in memory");
            z.finish().expect("compress in memory")
        })
    }

    /// The setup that opens [`space_bomb`] as `/f`, with `layers` more
    /// filters over it. It runs before the limits are set, so these tests
    /// also show a filter made earlier obeys them.
    fn bomb_setup(tag: &str, layers: &str) -> String {
        let dir = common::scratch_dir(tag);
        let path = dir.join("spaces.z");
        std::fs::write(&path, space_bomb()).expect("write scratch file");
        format!(
            "/f ({}) (r) file /FlateDecode filter /RunLengthDecode filter {layers} def",
            common::ps_path(&path)
        )
    }

    /// A Flate stream with large flat areas decoded in full after #1187, so
    /// a few KB can expand without bound. Every path that drains a filter
    /// in one call must stop inside it: the image's drain to EOD, the skip
    /// loops of `readhexstring` and of an ASCIIHex filter, the scanner, and
    /// `token`.
    #[test]
    fn draining_a_decompression_bomb_stops_inside_the_operator() {
        let cases: [(&str, &str, &str, &str); 6] = [
            // 10 x 10 samples, then the drain to the end of the data.
            ("image", "", "10 10 8 [10 0 0 10 0 0] f image", "image"),
            (
                "readhexstring",
                "",
                "f 10 string readhexstring",
                "readhexstring",
            ),
            // The hex filter skips the spaces inside one pull.
            ("hexskip", "/ASCIIHexDecode filter", "f read", "read"),
            ("scanner", "", "f cvx exec", "exec"),
            ("token", "", "f token", "token"),
            // `stopped` cannot catch it.
            ("stopped", "", "{ f flushfile } stopped", "flushfile"),
        ];
        for (tag, layers, src, op) in cases {
            stops_inside(
                op,
                bomb_setup(&format!("limits-bomb-{tag}"), layers),
                src,
                op,
                Duration::from_secs(2),
            );
        }
    }

    /// The decoded bytes count as steps, a step per KiB: 4 GiB is four
    /// million, so a step cap stops the drain too, inside it.
    #[test]
    fn a_decode_drain_counts_toward_the_step_cap() {
        stops_inside_with(
            "flushfile",
            bomb_setup("limits-bomb-steps", ""),
            "f flushfile",
            "flushfile",
            Duration::from_secs(2),
            steps(20_000),
            PsError::StepsExceeded,
        );
    }

    /// A raw file has no decode budget, so `flushfile` charges its bytes
    /// itself, a step per KiB (Codex, PR #1208): `currentfile flushfile`
    /// over 1 MiB of spaces under a 20-step cap must stop inside it.
    #[test]
    fn flushfile_on_a_raw_file_counts_its_bytes_as_steps() {
        let mut it = limited(steps(20));
        let err = it
            .run_str(&format!("currentfile flushfile{}", " ".repeat(1 << 20)))
            .expect_err("the step cap stops it");
        let report = it.error_report(&err);
        assert_eq!(err, PsError::StepsExceeded, "{report}");
        assert!(report.contains("OffendingCommand: flushfile;"), "{report}");
    }

    /// A drain far shorter than the 64 KiB between clock checks still
    /// counts against the step cap as it decodes, so the operator that
    /// drained is the one stopped (Codex, PR #1208). 32,000 decoded spaces
    /// are 31 steps; under a 20-step cap `(survived) =` used to run and
    /// take the blame, and `1 0 div` hid the overrun behind
    /// `undefinedresult`.
    #[test]
    fn a_short_decode_drain_past_the_step_cap_stops_inside_the_operator() {
        use std::io::Write;
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&[b' '; 32_000]).expect("compress in memory");
        let hex: String = z
            .finish()
            .expect("compress in memory")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let cases: [(&str, &str); 5] = [
            ("f flushfile (survived) =", "flushfile"),
            ("f flushfile 1 0 div", "flushfile"),
            (
                "f 40000 string readstring pop pop (survived) =",
                "readstring",
            ),
            (
                "f 40000 string readhexstring pop pop (survived) =",
                "readhexstring",
            ),
            ("100 320 8 [100 0 0 320 0 0] f image (survived) =", "image"),
        ];
        for (src, op) in cases {
            stops_inside_with(
                src,
                format!("/f <{hex}> /FlateDecode filter def"),
                src,
                op,
                Duration::from_secs(2),
                steps(20),
                PsError::StepsExceeded,
            );
        }
    }
    // --- #1219: the four one-operator paths #1217 left ---------------------
    mod issue_1219 {
        use super::*;

        /// 100,000 page-sized curves as the current path: `Mask::fill_path`
        /// cannot be stopped and took 1.4 s here at 72 dpi (5.6 s at 300),
        /// whatever the limits said. Built with no limits, as the setup.
        const MANY_CURVES: &str = "newpath 100000 { 0 0 moveto 600 700 -300 900 612 792 curveto \
             0 792 lineto closepath } repeat";

        #[test]
        fn a_clip_of_very_many_curves_stops_inside_clip() {
            for op in ["clip", "eoclip"] {
                stops_inside(op, MANY_CURVES.into(), op, op, Duration::from_millis(1500));
            }
        }

        #[test]
        fn a_nested_clip_of_very_many_curves_stops_inside_clip() {
            // Under a clip already in force the new mask is multiplied by it.
            stops_inside(
                "nested clip",
                format!("0 0 612 792 rectclip {MANY_CURVES}"),
                "clip",
                "clip",
                Duration::from_millis(1500),
            );
        }

        #[test]
        fn a_step_limit_refuses_a_huge_clip_and_leaves_the_clip_alone() {
            // The whole work is held against the step limit before the clip
            // is replaced (the lesson of Codex's review of #1217).
            let (err, report, after) = on_a_deadline("clip steps", || {
                let mut it = Interp::new();
                it.run_str("10 10 100 100 rectclip").expect("a small clip");
                it.run_str(MANY_CURVES).expect("the path");
                it.set_limits(steps(1000)).expect("limits");
                let err = it.run_str("clip").expect_err("the step limit stops it");
                let report = it.error_report(&err);
                it.set_limits(Limits::none()).expect("limits off");
                it.capture_stdout();
                // The clip is still the rectangle and the path still there.
                it.run_str("currentpoint pop = clippath pathbbox 4 { = } repeat")
                    .expect("clippath");
                (
                    err,
                    report,
                    String::from_utf8_lossy(it.captured_stdout()).into_owned(),
                )
            });
            assert_eq!(err, PsError::StepsExceeded, "{report}");
            assert!(report.contains("OffendingCommand: clip;"), "{report}");
            assert_eq!(after, "0.0\n110.0\n110.0\n10.0\n10.0\n", "{after}");
        }

        #[test]
        fn a_refused_rectclip_leaves_the_current_path_and_clip_alone() {
            // `rectclip` builds its rectangles in the current path: a limit
            // that refuses the clip must hand the program's path back
            // (Codex, PR #1232: it was left empty, `currentpoint` 0 0).
            let (err, report, after) = on_a_deadline("rectclip steps", || {
                let mut it = Interp::new();
                it.run_str("10 10 100 100 rectclip").expect("a small clip");
                // 1000 page-sized rectangles: 6 million units of work.
                it.run_str("/R [ 1000 { 0 0 612 792 } repeat ] def")
                    .expect("the rectangles");
                it.set_limits(steps(1000)).expect("limits");
                let err = it
                    .run_str("17 19 moveto R rectclip")
                    .expect_err("the step limit stops it");
                let report = it.error_report(&err);
                it.set_limits(Limits::none()).expect("limits off");
                it.capture_stdout();
                it.run_str("currentpoint exch = = clippath pathbbox 4 { = } repeat")
                    .expect("currentpoint");
                (
                    err,
                    report,
                    String::from_utf8_lossy(it.captured_stdout()).into_owned(),
                )
            });
            assert_eq!(err, PsError::StepsExceeded, "{report}");
            assert!(report.contains("OffendingCommand: rectclip;"), "{report}");
            // The path is the program's, the clip the small rectangle.
            assert_eq!(after, "17.0\n19.0\n110.0\n110.0\n10.0\n10.0\n", "{after}");
        }

        /// Setup for a 4095-leg ramp over a 300 dpi letter page (2550 x
        /// 3300 pixels at one pixel a point): resampled to 1024 stops, a
        /// `shfill` of it was 0.6 s of painting that never read the clock.
        fn ramp_setup() -> String {
            "<< /PageSize [2550 3300] >> setpagedevice /n 4095 def \
             /fns [ 0 1 n 1 sub { 2 mod 0 eq \
               { << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> } \
               { << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [1 1 0] /N 1 >> } ifelse } for ] def \
             /bounds [ 1 1 n 1 sub { n div } for ] def \
             /encode [ 0 1 n 1 sub { pop 0 1 } for ] def"
                .into()
        }

        const RAMP: &str = "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 2550 0] \
             /Function << /FunctionType 3 /Domain [0 1] /Functions fns \
             /Bounds bounds /Encode encode >> >> shfill";

        #[test]
        fn shfill_painting_stops_inside_shfill() {
            stops_inside(
                "shfill",
                ramp_setup(),
                RAMP,
                "shfill",
                Duration::from_millis(1500),
            );
        }

        #[test]
        fn a_step_limit_refuses_a_shfill_before_it_paints() {
            stops_inside_with(
                "shfill steps",
                ramp_setup(),
                RAMP,
                "shfill",
                Duration::from_millis(1500),
                steps(1000),
                PsError::StepsExceeded,
            );
        }

        #[test]
        fn a_shfill_a_limit_stops_paints_nothing() {
            // Under a limit the page is painted on a copy and kept once every
            // strip has painted, as a refused `fill` paints nothing, so the
            // raster, a pending `showpage` erase and the exports agree
            // (Codex, PR #1232: strips already painted stayed, with nothing
            // recorded for SVG or PDF).
            for showpage in [false, true] {
                let (err, px, pages) = on_a_deadline("shfill all or nothing", move || {
                    let mut it = Interp::new();
                    it.enable_svg();
                    it.run_str(&ramp_setup()).expect("setup");
                    it.run_str("1 0 0 setrgbcolor 0 0 2550 3300 rectfill")
                        .expect("red");
                    if showpage {
                        it.run_str("showpage").expect("showpage");
                    }
                    it.set_limits(untrusted(Duration::from_millis(100)))
                        .expect("limits");
                    let err = it.run_str(RAMP).expect_err("the clock stops it");
                    let px = it.gfx().pixmap.pixel(10, 10).expect("pixel");
                    let svg = it.svg_pages().unwrap_or_default().concat();
                    (
                        err,
                        (px.red(), px.green(), px.blue()),
                        svg.matches("<linearGradient").count(),
                    )
                });
                assert_eq!(err, PsError::TimeExceeded);
                assert_eq!(
                    px,
                    (255, 0, 0),
                    "showpage {showpage}: the page is untouched"
                );
                assert_eq!(pages, 0, "showpage {showpage}: nothing in the SVG or PDF");
            }
        }

        /// A radial cone (circles not nested) with its end not extended
        /// paints a two-gradient layer first (#1040): that is polled too,
        /// and a limit inside it still leaves the page untouched.
        const CONE: &str = "<< /ShadingType 3 /ColorSpace /DeviceRGB \
             /Coords [400 1650 100 2000 1650 400] \
             /Function << /FunctionType 3 /Domain [0 1] /Functions fns \
             /Bounds bounds /Encode encode >> >> shfill";

        #[test]
        fn a_cone_shfill_stops_inside_its_layer_and_paints_nothing() {
            stops_inside(
                "shfill cone",
                ramp_setup(),
                CONE,
                "shfill",
                Duration::from_millis(1500),
            );
            let (err, px) = on_a_deadline("shfill cone all or nothing", move || {
                let mut it = Interp::new();
                it.run_str(&ramp_setup()).expect("setup");
                it.run_str("1 0 0 setrgbcolor 0 0 2550 3300 rectfill")
                    .expect("red");
                it.set_limits(untrusted(Duration::from_millis(100)))
                    .expect("limits");
                let err = it.run_str(CONE).expect_err("the clock stops it");
                let px = it.gfx().pixmap.pixel(1000, 1650).expect("pixel");
                (err, (px.red(), px.green(), px.blue()))
            });
            assert_eq!(err, PsError::TimeExceeded);
            assert_eq!(px, (255, 0, 0), "the page is untouched");
        }

        #[test]
        fn type1_subroutines_decrypt_only_the_bytes_a_glyph_can_reach() {
            // One glyph calls a 16 MB subroutine (about 25 ms to decrypt in
            // full) and 200 of them were 5 s. A glyph's operator budget
            // reaches 500 KB of it. A budget test: nothing here stops at a
            // limit, the 30 s clock only keeps a regression from hanging.
            let src: &'static str = Box::leak(
                common::type1_probe(
                    16_000_000,
                    0,
                    0,
                    1,
                    "200 { (A) show } repeat currentpoint pop =",
                )
                .into_boxed_str(),
            );
            let (r, out) = run(
                "type1 huge subr",
                untrusted(Duration::from_secs(30)),
                Duration::from_secs(1),
                src,
            );
            assert_eq!(r, Ok(()));
            assert_eq!(out, "1210.0\n", "200 advances of 6");
        }
    }

    // --- #1235: leftovers of #1219 ------------------------------------------
    mod issue_1235 {
        use super::*;

        // --- `shfill` with a `/BBox` (item 2) -----------------------------

        /// A ramp of 4095 legs on a `side` x `side` page (one pixel a
        /// point), as `ramp_setup` makes one for letter.
        fn bbox_setup(side: usize) -> String {
            format!(
                "<< /PageSize [{side} {side}] >> setpagedevice /n 4095 def \
                 /fns [ 0 1 n 1 sub {{ 2 mod 0 eq \
                   {{ << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> }} \
                   {{ << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [1 1 0] /N 1 >> }} ifelse }} for ] def \
                 /bounds [ 1 1 n 1 sub {{ n div }} for ] def \
                 /encode [ 0 1 n 1 sub {{ pop 0 1 }} for ] def"
            )
        }

        /// The shading, with a box a little inside the page and off the
        /// pixel grid, so its edges are anti-aliased.
        fn bbox_shfill(side: usize, shading: &str) -> String {
            let s = side as f64;
            format!(
                "<< /ShadingType {shading} /ColorSpace /DeviceRGB \
                 /BBox [10.3 20.7 {} {}] \
                 /Function << /FunctionType 3 /Domain [0 1] /Functions fns \
                 /Bounds bounds /Encode encode >> >> shfill",
                s - 9.4,
                s - 10.1
            )
        }

        fn axial(side: usize) -> String {
            bbox_shfill(side, &format!("2 /Coords [0 0 {side} 0]"))
        }

        /// An axial that ends inside the box and extends neither way: each
        /// end is closed by a transparent stop (#1040), the shader tiny-skia
        /// paints slowest.
        fn closed_axial(side: usize) -> String {
            bbox_shfill(
                side,
                &format!("2 /Coords [{} 0 {} 0]", side / 4, side * 3 / 4),
            )
        }

        /// Two circles neither inside the other, the end not extended: a
        /// page-sized layer under a pattern shader (`two_root_layer`, #1040).
        fn cone(side: usize) -> String {
            bbox_shfill(
                side,
                &format!(
                    "3 /Coords [{} {} 10 {} {} {}] /Extend [true false]",
                    side / 4,
                    side / 2,
                    side * 3 / 4,
                    side / 2,
                    side / 8
                ),
            )
        }

        fn leaked(src: String) -> &'static str {
            Box::leak(src.into_boxed_str())
        }

        #[test]
        fn a_bbox_shfill_stops_inside_shfill() {
            // About 1.1 s of painting in one `fill_path` on a 4000 x 4000
            // page that never read the clock (#1219 left it); the box is
            // painted in strips now. The setup's clock is in the budget.
            stops_inside(
                "bbox shfill",
                bbox_setup(4000),
                leaked(axial(4000)),
                "shfill",
                Duration::from_millis(700),
            );
        }

        #[test]
        fn a_bbox_shfill_with_transparent_end_stops_stops_inside_shfill() {
            stops_inside(
                "bbox closed shfill",
                bbox_setup(4000),
                leaked(closed_axial(4000)),
                "shfill",
                Duration::from_millis(700),
            );
        }

        #[test]
        fn a_bbox_cone_shfill_stops_inside_shfill() {
            // The two-root layer is painted under the limits already (#1219,
            // #1040), and is the long part: this stops there with or without
            // the strips, which a pattern shader makes cheap. It pins that
            // the cone with a box stops inside `shfill`, and (with the
            // pixel test below) that the strips over it paint what the one
            // fill did.
            stops_inside(
                "bbox cone shfill",
                bbox_setup(4000),
                leaked(cone(4000)),
                "shfill",
                Duration::from_millis(700),
            );
        }

        #[test]
        fn a_step_limit_refuses_a_bbox_shfill_before_it_paints() {
            stops_inside_with(
                "bbox shfill steps",
                bbox_setup(2000),
                leaked(axial(2000)),
                "shfill",
                Duration::from_millis(1500),
                steps(1000),
                PsError::StepsExceeded,
            );
        }

        #[test]
        fn a_bbox_shfill_a_limit_stops_paints_nothing() {
            // All or nothing under a limit, a pending `showpage` erase and
            // the exports included, as for a shfill with no box.
            for showpage in [false, true] {
                let (err, px, pages) = on_a_deadline("bbox shfill all or nothing", move || {
                    let mut it = Interp::new();
                    it.enable_svg();
                    it.enable_pdf();
                    it.run_str(&bbox_setup(2000)).expect("setup");
                    it.run_str("1 0 0 setrgbcolor 0 0 2000 2000 rectfill")
                        .expect("red");
                    if showpage {
                        it.run_str("showpage").expect("showpage");
                    }
                    it.set_limits(untrusted(Duration::from_millis(50)))
                        .expect("limits");
                    let err = it
                        .run_str(&closed_axial(2000))
                        .expect_err("the clock stops it");
                    let px = it.gfx().pixmap.pixel(1000, 1000).expect("pixel");
                    let svg = it.svg_pages().unwrap_or_default().concat();
                    let pdf = it.pdf_document().unwrap_or_default();
                    (
                        err,
                        (px.red(), px.green(), px.blue()),
                        svg.matches("<linearGradient").count()
                            + String::from_utf8_lossy(&pdf)
                                .matches("/ShadingType")
                                .count(),
                    )
                });
                assert_eq!(err, PsError::TimeExceeded);
                assert_eq!(
                    px,
                    (255, 0, 0),
                    "showpage {showpage}: the page is untouched"
                );
                assert_eq!(pages, 0, "showpage {showpage}: nothing in the SVG or PDF");
            }
        }

        #[test]
        fn a_polled_bbox_shfill_paints_the_pixels_of_the_one_fill() {
            // The strips go through a coverage mask. Without a clip it is
            // the coverage `fill_path` computes: the same bytes. Under a
            // clip the two coverages are rounded together once, where the
            // pipeline rounds each: an edge pixel may differ by one level,
            // and only those. A 1200 x 1200 box of 1026 stops is over the
            // threshold for strips, so a limit that never fires takes them.
            let paint = |clip: &str, limits: Limits, shading: String| -> Vec<u8> {
                let mut it = Interp::new();
                it.run_str(&bbox_setup(1200)).expect("setup");
                it.run_str("0.9 0.9 0.7 setrgbcolor 0 0 1200 1200 rectfill")
                    .expect("paper");
                it.set_limits(limits).expect("limits");
                it.run_str(&format!("{clip} {shading}"))
                    .expect("the shading paints");
                it.gfx().pixmap.data().to_vec()
            };
            let roomy = || Limits::none().with_max_time(Duration::from_secs(600));
            let tilted = "gsave 0.1 rotate 40 10 translate";
            let clip = "gsave 100 100 moveto 1100 200 lineto 900 1100 lineto 50 1000 lineto closepath clip newpath";
            for (what, pre, shading) in [
                ("axial", "", axial(1200)),
                ("axial tilted", tilted, axial(1200)),
                ("closed axial", "", closed_axial(1200)),
                ("closed axial tilted", tilted, closed_axial(1200)),
                ("cone", "", cone(1200)),
            ] {
                let one = paint(pre, Limits::none(), shading.clone());
                let strips = paint(pre, roomy(), shading.clone());
                assert!(one == strips, "{what}: no clip, the same bytes");
                let one = paint(&format!("{pre} {clip}"), Limits::none(), shading.clone());
                let strips = paint(&format!("{pre} {clip}"), roomy(), shading);
                let worst = one
                    .iter()
                    .zip(&strips)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                assert!(worst <= 1, "{what}: under a clip, worst difference {worst}");
            }
        }

        /// A clip of 100,000 page-sized curves made on a 200 x 300 canvas,
        /// then a `gsave` and a smaller page: `cliprestore` falls back to
        /// that clip and remakes its mask for the new canvas (#909). That
        /// was `LimitProbe::NONE`: half a second at this size, 1.3 s at
        /// letter, and 5 s at 300 dpi, whatever the limits said. The setup
        /// runs with no limits.
        const REFIT: &str = "<< /PageSize [200 300] >> setpagedevice \
             newpath 100000 { 0 0 moveto 600 700 -300 900 612 792 curveto \
             0 792 lineto closepath } repeat clip \
             gsave << /PageSize [190 290] >> setpagedevice";

        #[test]
        fn a_clip_refitted_by_cliprestore_stops_inside_cliprestore() {
            stops_inside(
                "cliprestore",
                REFIT.into(),
                "cliprestore",
                "cliprestore",
                // The setup's clip (0.6 s here) is inside the budget too.
                Duration::from_secs(3),
            );
        }

        #[test]
        fn a_step_limit_refuses_a_long_chain_before_the_clip_changes() {
            // Thirty nested clips, each a triangle a fraction of a step
            // alone: every link was given the whole step budget and the
            // clip was assigned before the total was charged (Codex, PR
            // #1238: `--max-steps 50` ended in `limitexceeded (steps)` with
            // the clip already cut to the triangles). The bounds of the clip
            // in force must be the same after the refusal.
            let setup = "0 0 999 999 rectclip \
                 30 { newpath 0 0 moveto 500 0 lineto 0 500 lineto closepath clip } repeat \
                 gsave << /PageSize [999 999] >> setpagedevice";
            let (err, report, before, after, black) =
                on_a_deadline("cliprestore chain", move || {
                    let mut it = Interp::new();
                    it.run_str(setup).expect("the setup runs");
                    it.capture_stdout();
                    let show = "clippath pathbbox 4 { = } repeat";
                    it.run_str(show).expect("clippath");
                    let before = String::from_utf8_lossy(it.captured_stdout()).into_owned();
                    it.set_limits(steps(50)).expect("limits");
                    let err = it
                        .run_str("cliprestore")
                        .expect_err("the step limit stops it");
                    let report = it.error_report(&err);
                    it.set_limits(Limits::none()).expect("limits off");
                    it.run_str(show).expect("clippath");
                    let after = String::from_utf8_lossy(it.captured_stdout()).into_owned();
                    it.run_str("0 setgray 0 0 999 999 rectfill").expect("fill");
                    let black = it
                        .gfx()
                        .pixmap
                        .pixels()
                        .iter()
                        .filter(|p| p.red() == 0 && p.alpha() == 255)
                        .count();
                    (err, report, before, after, black)
                });
            assert_eq!(err, PsError::StepsExceeded, "{report}");
            assert!(
                report.contains("OffendingCommand: cliprestore;"),
                "{report}"
            );
            assert_eq!(after, format!("{before}{before}"), "the clip bounds");
            assert_eq!(black, 999 * 999, "the clip is as it was");
        }

        #[test]
        fn a_step_limit_refuses_a_refit_and_leaves_the_clip_alone() {
            // The refit's work is held against the step limit, and a refused
            // `cliprestore` changes nothing: the clip in force is the whole
            // page (`setpagedevice` reset it), so a black fill paints every
            // pixel, where the refitted clip (the curves' region) misses some.
            let (err, report, black) = on_a_deadline("cliprestore steps", || {
                let mut it = Interp::new();
                it.run_str(REFIT).expect("the setup runs");
                it.set_limits(steps(1000)).expect("limits");
                let err = it
                    .run_str("cliprestore")
                    .expect_err("the step limit stops it");
                let report = it.error_report(&err);
                it.set_limits(Limits::none()).expect("limits off");
                it.run_str("0 setgray 0 0 190 290 rectfill").expect("fill");
                let black = it
                    .gfx()
                    .pixmap
                    .pixels()
                    .iter()
                    .filter(|p| p.red() == 0 && p.alpha() == 255)
                    .count();
                (err, report, black)
            });
            assert_eq!(err, PsError::StepsExceeded, "{report}");
            assert!(
                report.contains("OffendingCommand: cliprestore;"),
                "{report}"
            );
            assert_eq!(black, 190 * 290, "the clip is as it was");
        }
    }
}

/// Step accounting inside one slice (#1213). The limits are read between
/// slices, but an operator that looks at them from the inside (a decode
/// drain, a path walk, a read) used to see the steps the program had left
/// when the slice began, and a slice that ended in a finish or an error
/// never charged its own steps. A program could run past `--max-steps` by
/// up to one slice, unreported.
mod slice_accounting_1213 {
    use super::*;

    /// A zlib stream of `n` spaces, as the hex a `/ASCIIHexDecode` source reads.
    fn zlib_spaces_hex(n: usize) -> String {
        use std::io::Write;
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&vec![b' '; n]).expect("compress in memory");
        z.finish()
            .expect("compress in memory")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// The program's current path as `pathforall` sees it: elements.
    fn path_elements(it: &mut Interp) -> String {
        it.set_limits(Limits::none()).expect("no limits");
        it.capture_stdout();
        it.run_str(
            "0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 1 add } { 1 add } \
             pathforall =",
        )
        .expect("pathforall");
        String::from_utf8_lossy(it.captured_stdout())
            .trim()
            .to_string()
    }

    /// Run `tail` under a 20-step cap after draining `spaces` decoded
    /// spaces (15 steps for 16,000).
    fn drain_then(spaces: usize, tail: &str) -> (Result<(), PsError>, String) {
        let mut it = Interp::new();
        it.run_str(&format!(
            "/f <{}> /FlateDecode filter def",
            zlib_spaces_hex(spaces)
        ))
        .expect("the setup runs");
        it.set_limits(steps(20)).expect("limits apply");
        it.capture_stdout();
        let r = it.run_str(&format!("f flushfile {tail}"));
        let report = match &r {
            Err(e) => it.error_report(e),
            Ok(()) => String::new(),
        };
        let out = String::from_utf8_lossy(it.captured_stdout()).to_string();
        (r, format!("{report} stdout: {out}"))
    }

    #[test]
    fn a_drain_and_a_few_objects_past_the_cap_end_in_the_step_limit() {
        // 16,000 spaces are 15 steps, under the cap of 20 on their own; the
        // objects that follow take it past. Both programs used to finish
        // clean or with their own error.
        let (r, report) = drain_then(16_000, "1 2 add pop 1 2 add pop 1 2 add pop (survived) =");
        assert_eq!(r, Err(PsError::StepsExceeded), "{report}");
        assert!(report.contains("limitexceeded (steps)"), "{report}");

        let (r, report) = drain_then(16_000, "1 2 add pop 1 2 add pop 1 2 add pop 1 0 div");
        assert_eq!(r, Err(PsError::StepsExceeded), "{report}");
        assert!(report.contains("limitexceeded (steps)"), "{report}");
        assert!(!report.contains("undefinedresult"), "{report}");
    }

    #[test]
    fn a_drain_inside_the_cap_still_finishes() {
        // The same drain with few objects: 15 steps and 4 objects, 20 or fewer.
        let (r, report) = drain_then(16_000, "(survived) =");
        assert_eq!(r, Ok(()), "{report}");
        assert!(report.contains("survived"), "{report}");
    }

    #[test]
    fn a_program_of_exactly_the_cap_passes_and_one_more_object_fails() {
        // Ten objects under a cap of ten: the pass that finds the program
        // done is not a step.
        let mut it = limited(steps(10));
        assert_eq!(it.run_str("1 pop 1 pop 1 pop 1 pop 1 pop"), Ok(()));
        assert_eq!(it.steps_used(), 10, "the count is exact for a straight run");
        let mut it = limited(steps(10));
        assert_eq!(
            it.run_str("1 pop 1 pop 1 pop 1 pop 1 pop 1"),
            Err(PsError::StepsExceeded)
        );
        // The same at the end of a program that fails: the failing step
        // counts, and over the cap it is the step limit that is reported.
        let mut it = limited(steps(10));
        assert_eq!(
            it.run_str("1 pop 1 pop 1 pop 1 1 0 div"),
            Err(PsError::UndefinedResult)
        );
        let mut it = limited(steps(10));
        assert_eq!(
            it.run_str("1 pop 1 pop 1 pop 1 pop 1 0 div"),
            Err(PsError::StepsExceeded)
        );
    }

    #[test]
    fn the_count_does_not_depend_on_how_the_program_is_sliced() {
        let src = "1 1 3000 { pop } for 500 { 1 2 add pop } repeat 20 { 1 pop } repeat";
        let mut used = Vec::new();
        for slice in [1usize, 7, 512, 4096, 1 << 20] {
            let mut it = limited(steps(1_000_000));
            it.begin_source(src.as_bytes());
            while it.step_n(slice).expect("within the cap") {}
            used.push(it.steps_used());
        }
        assert!(
            used.iter().all(|&u| u == used[0]),
            "steps by slice: {used:?}"
        );
        assert!(used[0] > 3000, "{used:?}");
    }

    #[test]
    fn a_playground_frame_re_armed_each_call_never_trips_its_own_cap() {
        // `ps_set_max_steps` before each `ps_step`: the cap restarts, and a
        // frame well inside it must never stop.
        for src in ["1 1 1000000000 { pop } for", "{ 1 pop } loop"] {
            let mut it = Interp::new();
            it.set_limits(steps(20_000)).unwrap();
            it.begin_source(src.as_bytes());
            for frame in 0..50 {
                it.set_limits(steps(20_000)).unwrap();
                assert_eq!(it.step_n(4000), Ok(true), "{src}: frame {frame}");
            }
        }
    }

    #[test]
    fn a_failing_flattenpath_leaves_the_path_it_was_given() {
        // Objects run earlier in the same slice use up steps, so the
        // preflight must be held to what is left now (the #1217 review): it
        // passed against the start-of-slice figure, replaced the path, and
        // only then did the slice raise the limit.
        let mut it = Interp::new();
        it.run_str("newpath 0 0 moveto 30 { 1e20 1e20 -1e20 1e20 0 0 curveto } repeat")
            .unwrap();
        it.set_limits(steps(2000)).unwrap();
        let err = it
            .run_str("500 { 1 pop } repeat flattenpath")
            .expect_err("the step cap stops it");
        let report = it.error_report(&err);
        assert_eq!(err, PsError::StepsExceeded, "{report}");
        assert!(
            report.contains("OffendingCommand: flattenpath;"),
            "refused by flattenpath's own look, not after it: {report}"
        );
        assert_eq!(path_elements(&mut it), "31", "moveto and 30 curvetos");
    }

    #[test]
    fn a_fused_run_counts_the_same_as_running_each_element() {
        // `--profile-ops` never fuses a conditional (#864): the steps used
        // are the same either way.
        let src = "{ 1 1 50 { dup 25 lt { pop 0 } if dup 40 gt { 1 } { 2 } ifelse pop pop } for } bind exec";
        let used = |profiled: bool| {
            let mut it = limited(steps(1_000_000));
            if profiled {
                it.enable_profile_ops();
            }
            it.run_str(src).expect("within the cap");
            it.steps_used()
        };
        assert_eq!(used(false), used(true));
        assert!(used(false) > 700);
    }
}

/// Decoding a long binary object sequence looks at the limits as it goes
/// (#1123). A sequence can be 64 MiB and decode to eight million objects,
/// about a second of work, all in one scanner step: without a look inside it,
/// `--max-time` would be noticed only after it. Measured by the error, never
/// by the clock (this file's header).
mod binary_sequences_1123 {
    use super::*;

    /// A long-header sequence file of eight top-level arrays that share one
    /// block of 65535 integers: 524280 objects from 5 MiB of mostly padding.
    /// The program that scans it only pushes eight arrays.
    fn many_objects_file(tag: &str) -> String {
        let (arrays, block, total) = (8usize, 65_535usize, 5usize << 20);
        let mut v = vec![0x80, 0];
        v.extend_from_slice(&(arrays as u16).to_be_bytes());
        v.extend_from_slice(&(total as u32).to_be_bytes());
        for _ in 0..arrays {
            v.extend_from_slice(&[9, 0, 0xff, 0xff]);
            v.extend_from_slice(&((arrays * 8) as u32).to_be_bytes());
        }
        for i in 0..block {
            v.extend_from_slice(&[1, 0, 0, 0]);
            v.extend_from_slice(&(i as u32).to_be_bytes());
        }
        v.resize(total, 0);
        let path = common::scratch_dir(tag).join("big.bos");
        std::fs::write(&path, v).unwrap();
        format!("({}) run", common::ps_path(&path))
    }

    /// Stopped inside the decode: none of the eight arrays was pushed.
    fn assert_no_array_pushed(it: &Interp) {
        let n = it
            .operand_stack()
            .iter()
            .filter(|o| o.type_name() == "arraytype")
            .count();
        assert_eq!(n, 0, "arrays reached the stack: the decode ran to its end");
    }

    #[test]
    fn the_sequence_decodes_with_no_limits_or_roomy_ones() {
        let src = many_objects_file("bos-1123-ok");
        let mut it = Interp::new();
        it.run_str(&format!("{src} count =")).unwrap();
        let mut it = limited(steps(1_000_000));
        it.run_str(&src).expect("a roomy cap lets it decode");
        let mut it = limited(Limits::none().with_max_time(Duration::from_secs(600)));
        it.run_str(&src).expect("so does a roomy clock");
    }

    #[test]
    fn a_step_cap_stops_a_long_decode() {
        // Reading the 5 MiB file is 5120 steps (a KiB each), and 524280 slots
        // at 8 bytes are 4095 more. The cap lets the read through and not
        // the decode: without a look inside the decode this would pass.
        let src = many_objects_file("bos-1123-steps");
        let mut it = limited(steps(6_000));
        assert_eq!(it.run_str(&src), Err(PsError::StepsExceeded));
        assert_no_array_pushed(&it);
    }

    #[test]
    fn a_time_limit_stops_a_long_decode() {
        let src = many_objects_file("bos-1123-time");
        let mut it = limited(Limits::none().with_max_time(Duration::from_millis(1)));
        assert_eq!(it.run_str(&src), Err(PsError::TimeExceeded));
        assert_no_array_pushed(&it);
        // The interpreter is still usable afterwards.
        let mut it = limited(Limits::none().with_max_time(Duration::from_secs(600)));
        assert_eq!(it.run_str("1 2 add pop"), Ok(()));
    }
}
