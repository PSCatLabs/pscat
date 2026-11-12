//! Binary object sequences (issue #733): `setobjectformat`,
//! `currentobjectformat`, `printobject`, `writeobject`, and the scanner
//! reading a sequence back (token types 128 to 131).

mod common;

use std::path::{Path, PathBuf};

use pscat::{Interp, PsError};

fn setup(tag: &str) -> (PathBuf, Interp) {
    let root = common::scratch_dir(tag);
    let mut it = Interp::with_page(50, 50).expect("page");
    it.set_write_roots(&[&root]).expect("root exists");
    (root, it)
}

fn ps(p: &Path) -> String {
    let s = p.to_str().unwrap();
    assert!(!s.contains(['(', ')']), "{s}");
    format!("({})", s.replace('\\', "\\\\"))
}

/// What `src` prints with `printobject`, run in a fresh interpreter.
fn printed(src: &str) -> Vec<u8> {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.capture_stdout();
    it.run_str(src).unwrap_or_else(|e| panic!("{src}: {e}"));
    it.captured_stdout().to_vec()
}

fn err(src: &str) -> PsError {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.capture_stdout();
    it.run_str(src).expect_err(src)
}

/// The operand stack, top last, as `==` would print it.
fn stack(it: &Interp) -> Vec<String> {
    it.operand_stack().iter().map(|o| o.repr()).collect()
}

/// Objects whose bytes and round trips are checked. All reals are exact
/// in binary32.
const OBJECTS: &[&str] = &[
    "42",
    "-2147483648",
    "2147483647",
    "-2.5",
    "0.25",
    "true",
    "null",
    "mark",
    "(hello)",
    "()",
    "/abc",
    "/abc cvx",
    "[]",
    "[1 2.5 /abc (hi) [3 4] true null]",
    "[[1 [2] (s1)] [3 (s2)] (s3) [/n [4]]]",
    "[[[]] /a cvx (x) ()]",
    "{add 3 {x} (s)}",
    "[1 2 3] cvx",
    "[0.5 -0.5 1e10 -7]",
];

#[test]
fn default_format_and_setobjectformat() {
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str("currentobjectformat 3 setobjectformat currentobjectformat 0 setobjectformat currentobjectformat")
        .unwrap();
    assert_eq!(stack(&it), ["2", "3", "0"]);
    for bad in ["5", "-1", "100"] {
        assert_eq!(err(&format!("{bad} setobjectformat")), PsError::Rangecheck);
    }
    assert_eq!(err("1.5 setobjectformat"), PsError::Typecheck);
    assert_eq!(err("(a) setobjectformat"), PsError::Typecheck);
    assert_eq!(err("setobjectformat"), PsError::StackUnderflow);
}

#[test]
fn format_zero_is_undefined_like_ghostscript() {
    assert_eq!(err("0 setobjectformat 1 0 printobject").name(), "undefined");
    assert_eq!(
        err("0 setobjectformat (%stdout) (w) file 1 0 writeobject").name(),
        "undefined"
    );
}

#[test]
fn bytes_for_a_known_object() {
    // gs: 1 setobjectformat 42 7 printobject | xxd
    assert_eq!(
        printed("1 setobjectformat 42 7 printobject"),
        [0x80, 1, 0, 12, 1, 7, 0, 0, 0, 0, 0, 42]
    );
    assert_eq!(
        printed("2 setobjectformat 42 7 printobject"),
        [0x81, 1, 12, 0, 1, 7, 0, 0, 42, 0, 0, 0]
    );
    // Native formats write what the IEEE ones do, under their own token.
    assert_eq!(
        printed("3 setobjectformat -2.5 0 printobject"),
        [0x82, 1, 0, 12, 2, 0, 0, 0, 0xc0, 0x20, 0, 0]
    );
    assert_eq!(
        printed("4 setobjectformat 1.5 0 printobject"),
        [0x83, 1, 12, 0, 2, 0, 0, 0, 0, 0, 0xc0, 0x3f]
    );
    // [1 2.5 /abc (hi) [3 4] true null], laid out breadth first.
    let got = printed("1 setobjectformat [1 2.5 /abc (hi) [3 4] true null] 0 printobject");
    let want: Vec<u8> = [
        &[0x80, 1, 0, 0x59][..],
        &[0x09, 0, 0, 7, 0, 0, 0, 8],
        &[0x01, 0, 0, 0, 0, 0, 0, 1],
        &[0x02, 0, 0, 0, 0x40, 0x20, 0, 0],
        &[0x03, 0, 0, 3, 0, 0, 0, 0x50],
        &[0x05, 0, 0, 2, 0, 0, 0, 0x53],
        &[0x09, 0, 0, 2, 0, 0, 0, 0x40],
        &[0x04, 0, 0, 0, 0, 0, 0, 1],
        &[0x00, 0, 0, 0, 0, 0, 0, 0],
        &[0x01, 0, 0, 0, 0, 0, 0, 3],
        &[0x01, 0, 0, 0, 0, 0, 0, 4],
        b"abchi",
    ]
    .concat();
    assert_eq!(got, want);
}

/// Every object in every format reads back the same, through `token` on a
/// string, `run` on a file, and as the executed form.
#[test]
fn printobject_output_scans_back() {
    for fmt in 1..=4 {
        for obj in OBJECTS {
            let mut it = Interp::with_page(50, 50).unwrap();
            it.capture_stdout();
            it.run_str(&format!("{fmt} setobjectformat {obj} 9 printobject"))
                .unwrap();
            let bytes = it.captured_stdout().to_vec();
            // What the object looks like to `==`.
            let mut want = Interp::with_page(50, 50).unwrap();
            want.run_str(obj).unwrap();
            let want = stack(&want);

            let mut it = Interp::with_page(50, 50).unwrap();
            it.run_str(&format!("({}) token", escaped(&bytes))).unwrap();
            // true, then the executable array {obj}, remainder below.
            let st = stack(&it);
            assert_eq!(st.last().unwrap(), "true", "{fmt} {obj}");
            assert_eq!(
                st[st.len() - 2],
                format!("{{{}}}", want.join(" ")),
                "format {fmt}, {obj}"
            );
            assert_eq!(st[st.len() - 3], "()", "no remainder: {fmt} {obj}");
        }
    }
}

/// A PostScript string literal for arbitrary bytes.
fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:03o}")).collect()
}

#[test]
fn writeobject_to_a_file_and_run_it_back() {
    let (root, mut it) = setup("bo-file");
    let f = ps(&root.join("a.bos"));
    it.run_str(&format!(
        "1 setobjectformat {f} (w) file dup [1 2.5 /abc (hi) [3 4 [5]] true null] 3 writeobject \
         dup 77 5 writeobject closefile"
    ))
    .unwrap();
    let bytes = std::fs::read(root.join("a.bos")).unwrap();
    assert_eq!(bytes[0], 0x80);
    // Two sequences back to back.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("{f} run")).unwrap();
    assert_eq!(stack(&it), ["[1 2.5 /abc (hi) [3 4 [5]] true null]", "77"]);
    // `token` from a file object reads one sequence and leaves the rest.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("{f} (r) file dup token pop exch token pop"))
        .unwrap();
    assert_eq!(stack(&it).len(), 2);
}

#[test]
fn tag_goes_in_the_top_level_object_only() {
    let bytes = printed("1 setobjectformat [1 2] 200 printobject");
    assert_eq!(bytes[5], 200);
    assert_eq!(bytes[4 + 8 + 1], 0);
    assert_eq!(bytes[4 + 16 + 1], 0);
}

/// A sequence over 65535 bytes needs the long header. (Written to a file:
/// captured stdout is truncated at 64 KiB.)
#[test]
fn long_header_round_trips() {
    let (root, mut it) = setup("bo-long");
    let f = ps(&root.join("long.bos"));
    it.run_str(&format!(
        "/s 30000 string def 1 setobjectformat {f} (w) file dup [s s s (end)] 0 writeobject closefile"
    ))
    .unwrap();
    let bytes = std::fs::read(root.join("long.bos")).unwrap();
    assert_eq!(bytes.len(), 8 + 5 * 8 + 90_003);
    assert_eq!(bytes[0], 0x80);
    assert_eq!(bytes[1], 0, "long header");
    assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]), 1);
    assert_eq!(
        u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize,
        bytes.len()
    );
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!(
        "{f} (r) file token pop dup length exch 0 get length"
    ))
    .unwrap();
    // One top-level object: an array of 4.
    assert_eq!(stack(&it), ["1", "4"]);
}

#[test]
fn unsupported_and_oversize_objects() {
    use PsError::*;
    for (src, want) in [
        ("1 dict 0 printobject", Typecheck),
        ("(%stdout) (w) file 0 printobject", Typecheck),
        ("systemdict /add get 0 printobject", Typecheck),
        ("save 0 printobject", Typecheck),
        ("[1 << >>] 0 printobject", Typecheck),
        ("2147483648 0 printobject", Limitcheck),
        ("-2147483649 0 printobject", Limitcheck),
        ("1e39 0 printobject", Limitcheck),
        ("1e308 dup mul 0 printobject", Limitcheck),
        ("1e30 dup dup mul mul 0 printobject", Limitcheck),
        ("0 0 div 0 printobject", UndefinedResult),
        ("65536 string 0 printobject", Limitcheck),
        ("65536 array 0 printobject", Limitcheck),
        ("65536 string cvn 0 printobject", Limitcheck),
        ("() cvn 0 printobject", Limitcheck),
        ("1 256 printobject", Rangecheck),
        ("1 -1 printobject", Rangecheck),
        ("1 1.5 printobject", Typecheck),
        ("1 (a) printobject", Typecheck),
        ("1 printobject", StackUnderflow),
        ("printobject", StackUnderflow),
        // writeobject operand checks.
        ("1 1 0 writeobject", Typecheck),
        ("1 0 writeobject", StackUnderflow),
        ("(%stdout) (w) file 1 300 writeobject", Rangecheck),
        ("(%stdout) (w) file dup closefile 1 0 writeobject", Io),
    ] {
        assert_eq!(err(src), want, "{src}");
    }
}

#[test]
fn deep_nesting_and_cycles_are_limitchecks_not_crashes() {
    assert_eq!(
        err("/a [] def 600 { [ a ] /a exch def } repeat a 0 printobject"),
        PsError::Limitcheck
    );
    // An array that contains itself.
    assert_eq!(
        err("/a 1 array def a 0 a put a 0 printobject"),
        PsError::Limitcheck
    );
    // 65535 references to one 65535-element array: 4 billion objects as a tree.
    assert_eq!(
        err(
            "/b 65535 array def /a 65535 array def 0 1 65534 { b exch a put } for \
             b 0 printobject"
        ),
        PsError::Limitcheck
    );
}

#[test]
fn writing_needs_a_writable_file() {
    // No roots: the file cannot be opened, so there is nothing to write to.
    assert_eq!(
        err("(/tmp/pscat-binobj-never) (w) file 1 0 writeobject"),
        PsError::InvalidFileAccess
    );
    // %stdout and %stderr need no root.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.capture_stdout();
    it.run_str("1 setobjectformat (%stdout) (w) file 42 0 writeobject")
        .unwrap();
    assert_eq!(
        it.captured_stdout(),
        [0x80, 1, 0, 12, 1, 0, 0, 0, 0, 0, 0, 42]
    );
}

// ---------------------------------------------------------------- scanner

/// A short-header sequence of these slots and characters.
fn seq(slots: &[[u8; 8]], chars: &[u8]) -> Vec<u8> {
    let total = 4 + slots.len() * 8 + chars.len();
    let mut v = vec![0x80, slots.len() as u8, (total >> 8) as u8, total as u8];
    for s in slots {
        v.extend_from_slice(s);
    }
    v.extend_from_slice(chars);
    v
}

#[test]
fn scanner_reads_a_hand_built_sequence() {
    // Two top-level objects, 7 and 8: executing it pushes both.
    let s = seq(&[[1, 0, 0, 0, 0, 0, 0, 7], [1, 0, 0, 0, 0, 0, 0, 8]], b"");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("1 ({} 99) cvx exec", escaped(&s)))
        .unwrap();
    assert_eq!(stack(&it), ["1", "7", "8", "99"]);
    // `token` gives the executable array, and the text after it is left
    // alone (the byte after the sequence is not a delimiter to eat).
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({} 99) token", escaped(&s))).unwrap();
    assert_eq!(stack(&it), ["( 99)", "{7 8}", "true"]);
    // Inside braces it is a nested procedure, as in Ghostscript.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({{ {} }}) token pop", escaped(&s)))
        .unwrap();
    assert_eq!(stack(&it), ["()", "{{7 8}}"]);
    // An executable name runs; a literal one is pushed.
    let s = seq(
        &[[0x83, 0, 0, 3, 0, 0, 0, 16], [3, 0, 0, 1, 0, 0, 0, 19]],
        b"addx",
    );
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("1 2 ({} ) cvx exec", escaped(&s)))
        .unwrap();
    assert_eq!(stack(&it), ["3", "/x"]);
}

#[test]
fn scanner_reads_fixed_point_and_immediate_names() {
    // 1.5 as 0x180 over 2^8.
    let s = seq(&[[2, 0, 0, 8, 0, 0, 1, 0x80]], b"");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({}) token pop", escaped(&s))).unwrap();
    assert_eq!(stack(&it), ["()", "{1.5}"]);
    // Type 6: replaced by the name's value when scanned.
    let s = seq(&[[6, 0, 0, 3, 0, 0, 0, 8]], b"foo");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("/foo 5 def ({}) token pop", escaped(&s)))
        .unwrap();
    assert_eq!(stack(&it), ["()", "{5}"]);
    assert_eq!(err(&format!("({}) token", escaped(&s))).name(), "undefined");
}

#[test]
fn malformed_sequences_are_syntax_errors() {
    let good = seq(&[[1, 0, 0, 0, 0, 0, 0, 7]], b"");
    let mut cases: Vec<(&str, Vec<u8>)> = vec![
        ("truncated body", good[..good.len() - 1].to_vec()),
        ("truncated header", vec![0x80, 1]),
        ("lone token byte", vec![0x80]),
        ("string past the end", seq(&[[5, 0, 0, 9, 0, 0, 0, 8]], b"")),
        (
            "array past the end",
            seq(&[[9, 0, 0, 200, 0, 0, 0, 8]], b""),
        ),
        (
            "array that contains itself",
            seq(&[[9, 0, 0, 1, 0, 0, 0, 0]], b""),
        ),
        ("reserved type 7", seq(&[[7, 0, 0, 0, 0, 0, 0, 0]], b"")),
        ("reserved type 8", seq(&[[8, 0, 0, 0, 0, 0, 0, 0]], b"")),
        ("system name index", seq(&[[3, 0, 0, 0, 0, 0, 0, 7]], b"")),
        (
            "real fraction bits over 32",
            seq(&[[2, 0, 0, 40, 0, 0, 0, 7]], b""),
        ),
        (
            "more objects than the length holds",
            vec![0x80, 5, 0, 12, 1, 0, 0, 0, 0, 0, 0, 7],
        ),
        (
            "long header claiming 4 GB",
            vec![0x80, 0, 0, 1, 0xff, 0xff, 0xff, 0xff, 1, 0, 0, 0],
        ),
    ];
    // A sequence shorter than its own header.
    cases.push(("total below header", vec![0x80, 1, 0, 2, 0, 0, 0, 0]));
    for (what, bytes) in cases {
        let got = err(&format!("({}) token", escaped(&bytes)));
        assert_eq!(got.name(), "syntaxerror", "{what}: {got:?}");
    }
    // A real that is not a number.
    let nan = seq(&[[2, 0, 0, 0, 0x7f, 0xc0, 0, 0]], b"");
    assert_eq!(
        err(&format!("({}) token", escaped(&nan))),
        PsError::UndefinedResult
    );
}

#[test]
fn a_high_byte_inside_a_name_is_still_a_name() {
    // Only a token that *starts* with 128 to 131 is a sequence.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str("(abc\\200def) token").unwrap();
    let st = stack(&it);
    assert_eq!(st.len(), 3, "{st:?}");
    assert_eq!(st[0], "()", "the whole word is the name");
    assert_eq!(st[2], "true");
}

#[test]
fn format_zero_turns_binary_scanning_off() {
    // With format 0 a token that starts with byte 128 is an ordinary name,
    // as in Ghostscript; with any other format it is a sequence.
    let s = seq(&[[1, 0, 0, 0, 0, 0, 0, 7]], b"");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!(
        "0 setobjectformat ({}) token pop type",
        escaped(&s)
    ))
    .unwrap();
    assert_eq!(stack(&it).last().unwrap(), "nametype");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!(
        "2 setobjectformat ({}) token pop type",
        escaped(&s)
    ))
    .unwrap();
    assert_eq!(stack(&it), ["()", "arraytype"]);
}

#[test]
fn a_scanned_executable_string_runs() {
    // Slot type 5 with the executable bit: the string "1 2" runs, so its
    // numbers are pushed rather than the string itself.
    let s = seq(&[[0x85, 0, 0, 3, 0, 0, 0, 8]], b"1 2");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({}) cvx exec", escaped(&s))).unwrap();
    assert_eq!(stack(&it), ["1", "2"]);
    // A literal string is pushed.
    let s = seq(&[[0x05, 0, 0, 3, 0, 0, 0, 8]], b"1 2");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({}) cvx exec", escaped(&s))).unwrap();
    assert_eq!(stack(&it), ["(1 2)"]);
    // And what printobject writes for an executable string runs the same way.
    let bytes = printed("(3 4 add) cvx 0 printobject");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(&format!("({}) cvx exec", escaped(&bytes)))
        .unwrap();
    assert_eq!(stack(&it), ["7"]);
}

#[test]
fn restore_brings_back_the_object_format() {
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str("save 4 setobjectformat currentobjectformat exch restore currentobjectformat")
        .unwrap();
    assert_eq!(stack(&it), ["4", "2"]);
}

#[test]
fn strings_that_share_characters_run_out_of_budget() {
    // An array of four strings of 40 bytes that all point at the same 40
    // characters: 160 bytes of copies from an 80-byte body.
    let slots = [
        [9, 0, 0, 4, 0, 0, 0, 8],
        [5, 0, 0, 40, 0, 0, 0, 40],
        [5, 0, 0, 40, 0, 0, 0, 40],
        [5, 0, 0, 40, 0, 0, 0, 40],
        [5, 0, 0, 40, 0, 0, 0, 40],
    ];
    let mut s = seq(&slots, &[b'x'; 40]);
    s[1] = 1; // one top-level object, the array
    let mut it = Interp::with_page(50, 50).unwrap();
    let e = it
        .run_str(&format!("({}) cvx exec", escaped(&s)))
        .expect_err("shared characters");
    assert!(matches!(e, PsError::Syntax(_)), "{e:?}");
}

#[test]
fn writer_stops_a_huge_array_of_one_big_string_early() {
    // 65535 references to one 65535-byte string would be 4 GiB of copies.
    let e = err(
        "/s 65535 string def 65535 array dup 0 1 65534 { 1 index exch s put } for 0 printobject",
    );
    assert!(matches!(e, PsError::Limitcheck), "{e:?}");
}

#[test]
fn an_error_inside_a_sequence_reports_its_own_line() {
    // `/nope` as an executable name, scanned straight from the program
    // source on line 3: the report names line 3, not the line of `1 pop`.
    let s = seq(&[[0x83, 0, 0, 4, 0, 0, 0, 8]], b"nope");
    let mut src = b"%!PS\n1 pop\n".to_vec();
    src.extend_from_slice(&s);
    let mut it = Interp::with_page(50, 50).unwrap();
    let e = it.run_source(&src).expect_err("undefined");
    assert!(
        it.error_report(&e).contains("Line: 3"),
        "{}",
        it.error_report(&e)
    );
}

// ------------------------------------------------------------ Ghostscript

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|o| o.status.success())
}

fn gs(prog: &str) -> Vec<u8> {
    let out = std::process::Command::new("gs")
        .args([
            "-q",
            "-dNOSAFER",
            "-dNODISPLAY",
            "-dBATCH",
            "-dNOPAUSE",
            "-c",
            prog,
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run gs");
    assert!(out.status.success(), "gs rejected {prog}");
    out.stdout
}

/// The bytes gs writes for the same object, format and tag (local only;
/// CI has no gs). gs's own long header starts with two wrong bytes, so
/// only sequences under 65535 bytes are compared.
#[test]
fn ghostscript_writes_the_same_bytes() {
    if !gs_available() {
        eprintln!("skipping gs byte comparison: gs not installed");
        return;
    }
    for fmt in 1..=4 {
        for obj in OBJECTS {
            let prog = format!("{fmt} setobjectformat {obj} 7 printobject");
            assert_eq!(printed(&prog), gs(&prog), "format {fmt}, {obj}");
        }
    }
}

/// gs reads what pscat writes, and pscat reads what gs writes, including
/// the long header (which pscat writes per the PLRM).
#[test]
fn ghostscript_reads_what_pscat_writes() {
    if !gs_available() {
        eprintln!("skipping gs read-back check: gs not installed");
        return;
    }
    for (fmt, obj) in [
        (1, "[1 2.5 /abc (hi) [3 4] true null]"),
        (2, "{add 3 {x} (s)}"),
        (3, "[[1 [2] (s1)] [3 (s2)] (s3) [/n [4]]]"),
        (4, "-2.5"),
    ] {
        let bytes = printed(&format!("{fmt} setobjectformat {obj} 0 printobject"));
        let prog = format!("({}) token pop ==", escaped(&bytes));
        let got = String::from_utf8(gs(&prog)).unwrap();
        // `{obj}` for gs's executable array of the one top-level object.
        let mut want = Interp::with_page(50, 50).unwrap();
        want.capture_stdout();
        want.run_str(&format!("{obj} ==")).unwrap();
        let want = String::from_utf8_lossy(want.captured_stdout())
            .trim()
            .to_string();
        assert_eq!(got.trim(), format!("{{{want}}}"), "format {fmt}, {obj}");
    }
    // The long header: gs reads pscat's output (a file, since captured
    // stdout is cut at 64 KiB).
    let (root, mut it) = setup("bo-gs-long");
    let f = ps(&root.join("long.bos"));
    it.run_str(&format!(
        "/s 30000 string def 1 setobjectformat {f} (w) file dup [s s s (end)] 0 writeobject closefile"
    ))
    .unwrap();
    let bytes = std::fs::read(root.join("long.bos")).unwrap();
    assert_eq!(bytes[1], 0);
    let got = String::from_utf8(gs(&format!(
        "{f} (r) file token pop dup length =  0 get length ="
    )))
    .unwrap();
    assert_eq!(got.split_whitespace().collect::<Vec<_>>(), ["1", "4"]);
}
