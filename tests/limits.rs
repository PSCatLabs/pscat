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
        let (err, report) = on_a_deadline(what, move || {
            common::within_cpu_budget(what, budget, 3, || {
                let mut it = Interp::new();
                it.run_str(&setup).expect("the setup runs");
                it.set_limits(untrusted(Duration::from_millis(100)))
                    .expect("limits without memory always apply");
                let err = it
                    .run_str(&format!("{src} 1 pop"))
                    .expect_err("the time limit stops it");
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

    /// A RunLengthDecode stream (each pair 0x81 c is 128 copies of c, with
    /// c changing every 100 pairs) under FlateDecode: about 3000 bytes out
    /// per byte in. Flate never makes more than 512 bytes of one input byte
    /// here, which pscat's decoder truncates past (a separate bug).
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
}
