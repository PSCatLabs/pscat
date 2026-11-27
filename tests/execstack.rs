//! `countexecstack`, `execstack` and `dictstack` (issue #1468).
//!
//! Ghostscript's execution stack starts 38 entries deep at the top of a
//! file (its own startup procedures and `stopped` contexts), so absolute
//! counts do not carry over: the cases print counts relative to the top
//! level (`B`). `PARITY` lines are pinned from gs 10.07.1 and, when `gs`
//! is installed, checked against it too. `OWN` lines are where pscat shows
//! a frame differently on purpose (`notes/1468-execstack-dictstack.md`),
//! with what gs printed beside each.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::{Interp, Limits, PsError};

/// Defines `B` (the count at the top level) and `top` (the execution stack
/// as the caller of `top` sees it: `top` is a tail call, so its own frame
/// is gone by the time `execstack` runs).
const PRELUDE: &str = "/B countexecstack def\n/top { countexecstack array execstack } def\n";

/// `(name, body, what gs prints)`: each body prints one line.
const PARITY: &[(&str, &str, &str)] = &[
    // countexecstack in each kind of context, less the top level's.
    ("proc", "/p { countexecstack B sub = 0 pop } def p", "1"),
    ("exec", "{ countexecstack B sub = } exec", "1"),
    ("tail", "{ countexecstack } exec B sub =", "0"),
    (
        "nested",
        "/p1 { p2 0 pop } def /p2 { countexecstack B sub = 0 pop } def p1",
        "2",
    ),
    (
        "forall-array",
        "[1] { pop countexecstack B sub = } forall",
        "4",
    ),
    (
        "forall-string",
        "(a) { pop countexecstack B sub = } forall",
        "4",
    ),
    ("for", "1 1 1 { pop countexecstack B sub = } for", "6"),
    ("repeat", "1 { countexecstack B sub = } repeat", "4"),
    ("loop", "{ countexecstack B sub = exit } loop", "3"),
    ("type", "countexecstack type =", "integertype"),
    // `countexecstack` sizes an array `execstack` fills exactly.
    (
        "exact",
        "{ countexecstack dup array execstack length eq = } exec",
        "true",
    ),
    (
        "one-short",
        "{ countexecstack 1 sub array execstack 0 pop } stopped = clear",
        "true",
    ),
    // What execstack shows: a procedure is its unexecuted rest, a loop
    // its control values and body, an executable string its unread text.
    (
        "proc-rest",
        "/p { top dup length 1 sub get == 0 pop } def p",
        "{dup length 1 sub get == 0 pop}",
    ),
    (
        "for-values",
        "1 2 9 { pop top dup length 6 sub 3 getinterval == exit } for",
        "[3 2 9]",
    ),
    (
        "for-body",
        "1 2 9 { pop top dup length 3 sub get == exit } for",
        "{pop top dup length 3 sub get == exit}",
    ),
    (
        "repeat-count",
        "5 { top dup length 4 sub get == exit } repeat",
        "4",
    ),
    (
        "forall-rest",
        "[1 2 3] { pop top dup length 4 sub get == exit } forall",
        "[2 3]",
    ),
    (
        "forall-string-rest",
        "(abc) { pop top dup length 4 sub get == exit } forall",
        "(bc)",
    ),
    (
        "loop-body",
        "{ top dup length 3 sub get == exit } loop",
        "{top dup length 3 sub get == exit}",
    ),
    (
        "string-rest",
        "(top dup length 1 sub get == 0 pop) cvx exec",
        "(dup length 1 sub get == 0 pop)",
    ),
    (
        "subarray",
        "100 array dup execstack dup length exch 0 get 3 -1 roll 0 get eq exch countexecstack eq and =",
        "true",
    ),
    (
        "result-attrs",
        "100 array execstack dup rcheck exch dup wcheck exch xcheck 3 array astore ==",
        "[true true false]",
    ),
    // Errors leave the operands, in gs's order: type, then size, then access.
    (
        "rangecheck",
        "7 { 2 array execstack } stopped $error /errorname get 4 array astore == clear",
        "[7 [null null] true /rangecheck]",
    ),
    (
        "typecheck",
        "7 { 5 execstack } stopped $error /errorname get 4 array astore == clear",
        "[7 5 true /typecheck]",
    ),
    (
        "underflow",
        "{ execstack } stopped $error /errorname get 2 array astore == clear",
        "[true /stackunderflow]",
    ),
    (
        "readonly",
        "{ 100 array readonly execstack } stopped $error /errorname get 3 array astore 1 get = clear",
        "true",
    ),
    (
        "readonly-name",
        "{ 100 array readonly execstack } stopped pop $error /errorname get == clear",
        "/invalidaccess",
    ),
    (
        "readonly-short",
        "{ 1 array readonly execstack } stopped pop $error /errorname get == clear",
        "/rangecheck",
    ),
    (
        "noaccess",
        "{ 100 array noaccess execstack } stopped pop $error /errorname get == clear",
        "/invalidaccess",
    ),
    // dictstack: systemdict, globaldict, userdict, then what `begin` pushed.
    ("dictstack-len", "10 array dictstack length =", "3"),
    (
        "dictstack-order",
        "10 array dictstack aload pop userdict eq exch globaldict eq 3 -1 roll systemdict eq 3 array astore ==",
        "[true true true]",
    ),
    (
        "dictstack-begin",
        "5 dict dup begin 10 array dictstack dup length exch dup length 1 sub get 3 -1 roll eq 2 array astore == end",
        "[4 true]",
    ),
    (
        "dictstack-exact",
        "1 dict begin countdictstack array dictstack length countdictstack eq = end",
        "true",
    ),
    (
        "dictstack-wcheck",
        "10 array dictstack { wcheck } forall 3 array astore ==",
        "[false true true]",
    ),
    (
        "dictstack-xcheck",
        "10 array dictstack { xcheck } forall 3 array astore ==",
        "[false false false]",
    ),
    (
        "dictstack-result-attrs",
        "10 array dictstack dup wcheck exch xcheck 2 array astore ==",
        "[true false]",
    ),
    (
        "dictstack-rangecheck",
        "7 { 2 array dictstack } stopped $error /errorname get 4 array astore == clear",
        "[7 [null null] true /rangecheck]",
    ),
    (
        "dictstack-typecheck",
        "7 { 5 dictstack } stopped $error /errorname get 4 array astore == clear",
        "[7 5 true /typecheck]",
    ),
    (
        "dictstack-readonly",
        "{ 10 array readonly dictstack } stopped pop $error /errorname get == clear",
        "/invalidaccess",
    ),
    (
        "dictstack-readonly-short",
        "{ 1 array readonly dictstack } stopped pop $error /errorname get == clear",
        "/rangecheck",
    ),
];

/// Where pscat differs from gs on purpose: `(name, body, pscat, gs)`.
const OWN: &[(&str, &str, &str, &str)] = &[
    // gs plants `false 1 --%stopped_push--` (an internal error mask); pscat
    // shows the one `stopped` boundary as the `stopped` operator.
    (
        "stopped",
        "{ countexecstack B sub = } stopped pop",
        "2",
        "4",
    ),
    (
        "stopped-entry",
        "{ top dup length 2 sub get == } stopped pop",
        "--stopped--",
        "--%stopped_push--",
    ),
    // gs keeps an internal slot index (`-dict- {proc} 2 --%dict_continue--`).
    (
        "forall-dict",
        "1 dict dup /a 1 put { pop pop countexecstack B sub = } forall",
        "4",
        "5",
    ),
    // gs keeps its path enumerator (`(gs_path_enum) --%path_continue--`).
    (
        "pathforall",
        "newpath 0 0 moveto { pop pop countexecstack B sub = } { } { } { } pathforall",
        "6",
        "7",
    ),
    // The continuations are gs-internal operators; pscat shows the loop's own.
    (
        "repeat-entry",
        "1 { top dup length 2 sub get == } repeat",
        "--repeat--",
        "--%repeat_continue--",
    ),
    (
        "for-entry",
        "1 1 1 { pop top dup length 2 sub get == } for",
        "--for--",
        "--%for_pos_int_continue--",
    ),
    (
        "forall-entry",
        "[1] { pop top dup length 2 sub get == } forall",
        "--forall--",
        "--%array_continue--",
    ),
    (
        "loop-entry",
        "{ top dup length 2 sub get == exit } loop",
        "--loop--",
        "--%loop_continue--",
    ),
    // A frame does not carry its procedure's access, so every view
    // execstack makes of one is read-only (gs: the procedure's own).
    (
        "proc-rest-wcheck",
        "/p { top dup length 1 sub get wcheck = 0 pop } def p",
        "false",
        "true",
    ),
];

fn program(cases: &[(&str, &str)]) -> String {
    let mut src = String::from(PRELUDE);
    for (name, body) in cases {
        src.push_str(&format!("({name}: ) print {body}\n"));
    }
    src
}

fn parity_program() -> String {
    program(&PARITY.iter().map(|(n, b, _)| (*n, *b)).collect::<Vec<_>>())
}

fn lines(out: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(out)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn pscat_lines(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    lines(it.captured_stdout())
}

fn gs_lines(src: &str) -> Vec<String> {
    let dir = common::scratch_dir("execstack_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, src).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    lines(&out.stdout)
}

fn want<'a>(cases: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<String> {
    cases.map(|(n, out)| format!("{n}: {out}")).collect()
}

fn assert_lines(got: &[String], want: &[String]) {
    for (g, w) in got.iter().zip(want) {
        assert_eq!(g, w);
    }
    assert_eq!(got.len(), want.len(), "{got:?}");
}

#[test]
fn pscat_prints_the_lines_gs_printed() {
    let got = pscat_lines(&parity_program());
    assert_lines(&got, &want(PARITY.iter().map(|(n, _, o)| (*n, *o))));
}

#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !common::gs_available() {
        return;
    }
    let got = gs_lines(&parity_program());
    assert_lines(&got, &want(PARITY.iter().map(|(n, _, o)| (*n, *o))));
}

#[test]
fn pscat_shows_its_own_frames_as_pinned() {
    let src = program(&OWN.iter().map(|(n, b, _, _)| (*n, *b)).collect::<Vec<_>>());
    let got = pscat_lines(&src);
    assert_lines(&got, &want(OWN.iter().map(|(n, _, o, _)| (*n, *o))));
}

#[test]
fn ghostscript_shows_the_differences_as_quoted() {
    if !common::gs_available() {
        return;
    }
    let src = program(&OWN.iter().map(|(n, b, _, _)| (*n, *b)).collect::<Vec<_>>());
    let got = gs_lines(&src);
    assert_lines(&got, &want(OWN.iter().map(|(n, _, _, g)| (*n, *g))));
}

/// `countexecstack` sizes the array `execstack` fills in every context,
/// and the entries are never more than the frames' worth.
#[test]
fn countexecstack_always_sizes_execstack_exactly() {
    let check = "countexecstack dup array execstack length eq =";
    let one_short = "{ countexecstack 1 sub array execstack 0 pop } stopped pop \
                     $error /errorname get =";
    for ctx in [
        "{ X 0 pop } exec",
        "{ X } stopped pop",
        "[1 2] { pop X exit } forall",
        "(ab) { pop X exit } forall",
        "1 dict dup /a 1 put { pop pop X } forall",
        "1 2 9 { pop X exit } for",
        "0.5 0.5 2 { pop X exit } for",
        "3 { X exit } repeat",
        "{ X exit } loop",
        "newpath 0 0 moveto { pop pop X } { } { } { } pathforall",
        "(X 0 pop) cvx exec",
        "/r { dup 0 gt { 1 sub r 0 pop } { pop X } ifelse } def 500 r",
    ] {
        for (body, want) in [(check, "true"), (one_short, "rangecheck")] {
            let src = format!("{PRELUDE}{}", ctx.replace('X', body));
            assert_eq!(pscat_lines(&src), [want], "{ctx}");
        }
    }
}

#[test]
fn a_run_file_shows_as_its_file() {
    let dir = common::scratch_dir("execstack_run");
    let inner = dir.join("inner.ps");
    std::fs::write(
        &inner,
        "countexecstack array execstack dup length 1 sub get dup type = \
         currentfile eq =\n",
    )
    .expect("write inner");
    let src = format!("({}) run", common::ps_path(&inner));
    assert_eq!(pscat_lines(&src), ["filetype", "true"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--profile` plants a frame under every procedure called by name; it has
/// no PostScript form and must not change what the program sees.
#[test]
fn profiling_does_not_change_the_count() {
    let src = "/p { countexecstack = 0 pop } def p /q { p 0 pop } def q \
               { countexecstack array execstack length = } exec";
    let run = |extra: &[&str]| {
        let mut args = vec!["--headless"];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-e", src]);
        let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .args(&args)
            .output_deadline()
            .expect("run pscat");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    assert_eq!(run(&["--profile"]), run(&[]));
}

/// A recursion this deep (`r` keeps one frame per level) makes the walk
/// 9,000 frames long.
const DEEP: &str = "/r { dup 0 gt { 1 sub r 0 pop } { pop K } ifelse } def 9000 r";

fn run_with_steps(src: &str, steps: u64) -> Result<(), PsError> {
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_steps(steps))
        .expect("set limits");
    it.run_str(src)
}

/// Each operator is charged for the frames (or dictionaries) it walks: a
/// loop of them over a deep stack runs out of steps where the same loop
/// without them does not.
#[test]
fn the_walk_is_charged_to_max_steps() {
    let cap = 600_000;
    for (k, what) in [
        ("200 { countexecstack pop } repeat", "countexecstack"),
        (
            "200 { countexecstack array execstack pop } repeat",
            "execstack",
        ),
    ] {
        let base = DEEP.replace('K', "200 { 0 pop } repeat");
        run_with_steps(&base, cap).unwrap_or_else(|e| panic!("baseline: {e:?}"));
        let got = run_with_steps(&DEEP.replace('K', k), cap);
        assert!(
            matches!(got, Err(PsError::StepsExceeded)),
            "{what}: {got:?}"
        );
    }
    let nest = "100000 { userdict begin } repeat ";
    run_with_steps(
        &format!("{nest}50 {{ countdictstack array pop }} repeat"),
        cap,
    )
    .unwrap_or_else(|e| panic!("dict baseline: {e:?}"));
    let got = run_with_steps(
        &format!("{nest}50 {{ countdictstack array dictstack pop }} repeat"),
        cap,
    );
    assert!(
        matches!(got, Err(PsError::StepsExceeded)),
        "dictstack: {got:?}"
    );
}

/// A stop leaves the array as it was: the charge comes before the write.
#[test]
fn a_stop_leaves_the_array_unwritten() {
    for (src, probe) in [
        (
            DEEP.replace('K', "/A 9100 array def A execstack"),
            "A 0 get type =",
        ),
        (
            "/A 100100 array def 100000 { userdict begin } repeat A dictstack".to_string(),
            "A 0 get type =",
        ),
    ] {
        let mut it = Interp::new();
        // Enough for the setup, not for the operator's charge.
        let setup = run_steps_needed(&src);
        it.set_limits(Limits::none().with_max_steps(setup + 100))
            .expect("set limits");
        let got = it.run_str(&src);
        assert!(matches!(got, Err(PsError::StepsExceeded)), "{src}: {got:?}");
        it.set_limits(Limits::none()).expect("clear limits");
        it.capture_stdout();
        it.run_str(probe).expect("probe");
        assert_eq!(lines(it.captured_stdout()), ["nulltype"], "{src}");
    }
}

/// The steps a program takes with its last operator replaced by `pop`.
fn run_steps_needed(src: &str) -> u64 {
    let (head, _) = src.rsplit_once(' ').expect("an operator at the end");
    let mut it = Interp::new();
    // Steps are counted only under a limit.
    it.set_limits(Limits::none().with_max_steps(u64::MAX / 2))
        .expect("set limits");
    it.run_str(&format!("{head} pop")).expect("setup runs");
    it.steps_used()
}

/// `execstack` copies the unread text of every executable string on the
/// stack, and the copy is held against `--max-memory` before it is made:
/// thirty 1 MiB strings fit in the cap, a second copy of them does not.
#[test]
fn the_string_copies_are_held_against_max_memory() {
    let src = |k: &str| {
        format!(
            "/S 1048576 string def S 0 (r) putinterval S cvx /S exch def /n 0 def \
             /r {{ /n n 1 add def n 30 lt {{ S exec }} {{ {k} }} ifelse }} def r"
        )
    };
    let run = |k: &str| {
        Command::new(env!("CARGO_BIN_EXE_pscat"))
            .args(["--headless", "--max-memory", "50", "-e", &src(k)])
            .output_deadline()
            .expect("run pscat")
    };
    let base = run("countexecstack array pop");
    assert!(
        base.status.success(),
        "{}",
        String::from_utf8_lossy(&base.stderr)
    );
    let out = run("countexecstack array execstack pop");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(err.contains("limitexceeded (memory)"), "{err}");
    assert!(err.contains("execstack"), "{err}");
}
