//! `readreals` / `writereals` beyond the path-and-count forms (issue #773):
//! `/all`, an element offset, and the file-object forms. The core forms are
//! in `tests/realsio.rs`.

mod common;

use std::path::{Path, PathBuf};

use pscat::{Interp, PsError, Value};

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

fn top_reals(it: &Interp) -> Vec<f64> {
    let Value::Array(a) = &it.operand_stack().last().expect("an array").value else {
        panic!("not an array");
    };
    (0..a.len())
        .map(|i| match a.get(i).unwrap().value {
            Value::Real(r) => r,
            _ => panic!("not a real"),
        })
        .collect()
}

fn bytes_of(vals: &[f64]) -> Vec<u8> {
    vals.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Write `0.5 1.5 ... (n-0.5)` as binary64 to `name` under `root`.
fn seeded(root: &Path, name: &str, n: usize) -> PathBuf {
    let f = root.join(name);
    let vals: Vec<f64> = (0..n).map(|i| i as f64 + 0.5).collect();
    std::fs::write(&f, bytes_of(&vals)).unwrap();
    f
}

#[test]
fn all_reads_every_value() {
    let (root, mut it) = setup("rrf-all");
    let f = seeded(&root, "a.bin", 5);
    let p = ps(&f);
    it.run_str(&format!("{p} /all readreals")).unwrap();
    assert_eq!(top_reals(&it), [0.5, 1.5, 2.5, 3.5, 4.5]);
    // The format goes after the count, as with an integer.
    it.run_str(&format!("{p} /all /f32 readreals length"))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 10);
    it.run_str(&format!("{p} /all /f64 readreals length"))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 5);
    // A trailing partial value is ignored, as with an integer count.
    let mut b = std::fs::read(&f).unwrap();
    b.extend_from_slice(&[9, 9, 9]);
    std::fs::write(&f, b).unwrap();
    it.run_str(&format!("{p} /all readreals length")).unwrap();
    assert_eq!(it.pop_int().unwrap(), 5);
    // An empty file is an empty array.
    let e = root.join("e.bin");
    std::fs::write(&e, b"").unwrap();
    it.run_str(&format!("{} /all readreals length", ps(&e)))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 0);
    // `/all` gives back what `writereals` wrote, bit for bit.
    it.run_str(&format!(
        "[0.1 -0.0 1e308 10 mul 3] {p} writereals {p} /all readreals"
    ))
    .unwrap();
    let got = top_reals(&it);
    assert_eq!(got.len(), 4);
    assert!(got[1].is_sign_negative() && got[2].is_infinite());
}

#[test]
fn minus_one_is_not_all() {
    let (root, mut it) = setup("rrf-neg");
    let p = ps(&seeded(&root, "n.bin", 3));
    assert_eq!(
        it.run_str(&format!("{p} -1 readreals")),
        Err(PsError::Rangecheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} /everything readreals")),
        Err(PsError::Rangecheck)
    );
    // `/f32` alone is no count.
    assert_eq!(
        it.run_str(&format!("{p} /f32 readreals")),
        Err(PsError::Typecheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} (x) readreals")),
        Err(PsError::Typecheck)
    );
}

#[test]
fn offset_reads_a_slice() {
    let (root, mut it) = setup("rrf-off");
    let p = ps(&seeded(&root, "o.bin", 10));
    it.run_str(&format!("{p} 3 4 readreals")).unwrap();
    assert_eq!(top_reals(&it), [3.5, 4.5, 5.5, 6.5]);
    it.run_str(&format!("{p} 7 /all readreals")).unwrap();
    assert_eq!(top_reals(&it), [7.5, 8.5, 9.5]);
    it.run_str(&format!("{p} 0 2 readreals")).unwrap();
    assert_eq!(top_reals(&it), [0.5, 1.5]);
    // The last value, and an empty slice at the very end.
    it.run_str(&format!("{p} 9 1 readreals")).unwrap();
    assert_eq!(top_reals(&it), [9.5]);
    it.run_str(&format!("{p} 10 0 readreals length")).unwrap();
    assert_eq!(it.pop_int().unwrap(), 0);
    it.run_str(&format!("{p} 10 /all readreals length"))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 0);
    // The offset counts values of the chosen format: /f32 values are 4 bytes.
    it.run_str(&format!("{p} 2 1 /f32 readreals")).unwrap();
    assert_eq!(top_reals(&it).len(), 1);
    // Past the end, short, negative, huge: rangecheck, never a panic.
    for src in [
        format!("{p} 11 /all readreals"),
        format!("{p} 11 0 readreals"),
        format!("{p} 8 3 readreals"),
        format!("{p} -1 2 readreals"),
        format!("{p} -1 /all readreals"),
        format!("{p} 9223372036854775807 1 readreals"),
        format!("{p} 9223372036854775807 /all readreals"),
    ] {
        assert_eq!(it.run_str(&src), Err(PsError::Rangecheck), "{src}");
    }
    // A count over the array cap is limitcheck, as it always was.
    assert_eq!(
        it.run_str(&format!("{p} 1 9223372036854775807 readreals")),
        Err(PsError::Limitcheck)
    );
    // A missing source is stackunderflow, not a guess.
    assert_eq!(
        it.run_str("clear 5 5 readreals"),
        Err(PsError::StackUnderflow)
    );
}

#[test]
fn file_object_reads_advance_the_position() {
    let (root, mut it) = setup("rrf-read");
    let f = seeded(&root, "r.bin", 8);
    it.run_str(&format!("/h {} (r) file def", ps(&f))).unwrap();
    it.run_str("h 3 readreals h fileposition").unwrap();
    assert_eq!(it.pop_int().unwrap(), 24);
    assert_eq!(top_reals(&it), [0.5, 1.5, 2.5]);
    it.run_str("pop h 2 /f32 readreals pop h fileposition")
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 32);
    // Positioned reads compose with setfileposition (in bytes).
    it.run_str("h 40 setfileposition h 2 readreals").unwrap();
    assert_eq!(top_reals(&it), [5.5, 6.5]);
    it.run_str("pop h /all readreals h fileposition").unwrap();
    assert_eq!(it.pop_int().unwrap(), 64);
    assert_eq!(top_reals(&it), [7.5]);
    // At the end: /all is empty, a count is rangecheck.
    it.run_str("pop h /all readreals length").unwrap();
    assert_eq!(it.pop_int().unwrap(), 0);
    assert_eq!(it.run_str("h 1 readreals"), Err(PsError::Rangecheck));
    // A short read consumes nothing.
    it.run_str("h 60 setfileposition").unwrap();
    assert_eq!(it.run_str("h 1 readreals"), Err(PsError::Rangecheck));
    it.run_str("h fileposition").unwrap();
    assert_eq!(it.pop_int().unwrap(), 60);
    // /all takes whole values and leaves a partial one unread.
    it.run_str("h /all readreals length h fileposition")
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 60);
    assert_eq!(it.pop_int().unwrap(), 0);
    // No offset with a file (setfileposition is the way).
    assert_eq!(it.run_str("h 1 1 readreals"), Err(PsError::Typecheck));
    // Closed files and output files are ioerror.
    it.run_str("h closefile").unwrap();
    assert_eq!(it.run_str("h 1 readreals"), Err(PsError::Io));
    assert_eq!(it.run_str("h /all readreals"), Err(PsError::Io));
    it.run_str(&format!("/w {} (w) file def", ps(&root.join("w.bin"))))
        .unwrap();
    assert_eq!(it.run_str("w 1 readreals"), Err(PsError::Io));
    assert_eq!(it.run_str("w 0 readreals"), Err(PsError::Io));
    it.run_str("w closefile").unwrap();
    // Limits on the count apply to a file too.
    it.run_str(&format!("/g {} (r) file def", ps(&f))).unwrap();
    assert_eq!(
        it.run_str("g 100000000 readreals"),
        Err(PsError::Limitcheck)
    );
    assert_eq!(it.run_str("g -1 readreals"), Err(PsError::Rangecheck));
}

#[test]
fn file_reads_follow_the_file_across_lines_and_filters() {
    let (_, mut it) = setup("rrf-filter");
    // An ASCIIHex filter has no known length: /all drains it.
    let hex: String = bytes_of(&[1.5, 2.5, 3.5])
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    it.run_str(&format!("({hex}>) /ASCIIHexDecode filter /all readreals"))
        .unwrap();
    assert_eq!(top_reals(&it), [1.5, 2.5, 3.5]);
    it.run_str(&format!("({hex}>) /ASCIIHexDecode filter 3 readreals"))
        .unwrap();
    assert_eq!(top_reals(&it), [1.5, 2.5, 3.5]);
    assert_eq!(
        it.run_str(&format!("({hex}>) /ASCIIHexDecode filter 4 readreals")),
        Err(PsError::Rangecheck)
    );
    // A positioned filter is ioerror for setfileposition, but reads work.
    it.run_str(&format!(
        "({hex}>) /ASCIIHexDecode filter dup 1 readreals pop 2 readreals"
    ))
    .unwrap();
    assert_eq!(top_reals(&it), [2.5, 3.5]);
}

#[test]
fn currentfile_inline_data_keeps_line_numbers() {
    let mut it = Interp::with_page(50, 50).unwrap();
    // Values whose bytes are CR, LF and CRLF, so a bulk read that skipped
    // the scanner's line count would put the error on the wrong line.
    let vals = [
        f64::from_bits(0x0a0d_0a0a_0d0d_0a0b),
        f64::from_bits(0x0d0a_0a0a_0d0a_0a0d),
    ];
    let mut src = b"currentfile 2 readreals\n".to_vec();
    src.extend_from_slice(&bytes_of(&vals));
    src.extend_from_slice(b"\n%%\nfrobnicate");
    let want_line = {
        let (mut line, mut prev_cr) = (1, false);
        for &b in &src {
            match b {
                b'\r' => line += 1,
                b'\n' if !prev_cr => line += 1,
                _ => {}
            }
            prev_cr = b == b'\r';
        }
        line
    };
    let err = it.run_source(&src).unwrap_err();
    assert!(matches!(err, PsError::Undefined(_)), "{err:?}");
    let report = it.error_report(&err);
    assert!(report.contains(&format!("Line: {want_line} ")), "{report}");
    // And the values arrived intact.
    let Value::Array(a) = &it.operand_stack().last().unwrap().value else {
        panic!("{:?}", it.operand_stack().len());
    };
    assert_eq!(a.len(), 2);
}

#[test]
fn write_to_a_file_object_appends_and_composes() {
    let (root, mut it) = setup("rrf-write");
    let f = root.join("w.bin");
    let p = ps(&f);
    // Two arrays in one open file, no closefile: the file holds both, in
    // order, once flushed (a read flushes first).
    it.run_str(&format!(
        "/h {p} (w) file def [1 2] h writereals [3] h /f32 writereals h flushfile"
    ))
    .unwrap();
    let mut want = bytes_of(&[1.0, 2.0]);
    want.extend_from_slice(&3.0f32.to_le_bytes());
    assert_eq!(std::fs::read(&f).unwrap(), want);
    it.run_str("h fileposition").unwrap();
    assert_eq!(it.pop_int().unwrap(), 20);
    // A read sees the buffered writes without a flush or a close.
    it.run_str(&format!("[4] h writereals {p} 3 readreals"))
        .unwrap();
    let got = top_reals(&it);
    assert_eq!(got[..2], [1.0, 2.0]);
    assert_eq!(got.len(), 3);
    it.run_str("h closefile").unwrap();
    assert_eq!(std::fs::read(&f).unwrap().len(), 28);
    // An append file starts after what the path held.
    it.run_str(&format!(
        "/a {p} (a) file def [5 6] a writereals a closefile"
    ))
    .unwrap();
    let mut got = bytes_of(&[1.0, 2.0]);
    got.extend_from_slice(&3.0f32.to_le_bytes());
    got.extend_from_slice(&4.0f64.to_le_bytes());
    got.extend_from_slice(&bytes_of(&[5.0, 6.0]));
    assert_eq!(std::fs::read(&f).unwrap(), got);
    // After setfileposition a write goes where the position says.
    it.run_str(&format!(
        "/u {p} (w) file def [1 2 3] u writereals u 8 setfileposition [9] u writereals u closefile"
    ))
    .unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), bytes_of(&[1.0, 9.0, 3.0]));
    // Round trip through the file-object forms alone.
    it.run_str(&format!(
        "/u {p} (w) file def [0.25 -0.0 7] u writereals u closefile \
         /r {p} (r) file def r 1 readreals pop r /all readreals"
    ))
    .unwrap();
    assert_eq!(top_reals(&it).len(), 2);
}

#[test]
fn write_to_a_file_object_errors() {
    let (root, mut it) = setup("rrf-werr");
    let f = root.join("e.bin");
    let p = ps(&f);
    it.run_str(&format!("/h {p} (w) file def /r {p} (r) file def"))
        .unwrap();
    // Bad element: typecheck, and not a byte written.
    assert_eq!(it.run_str("[1 (a)] h writereals"), Err(PsError::Typecheck));
    it.run_str("h flushfile").unwrap();
    assert_eq!(std::fs::read(&f).unwrap().len(), 0);
    // Wrong operand types.
    assert_eq!(it.run_str("[1] 5 writereals"), Err(PsError::Typecheck));
    assert_eq!(it.run_str("5 h writereals"), Err(PsError::Typecheck));
    assert_eq!(
        it.run_str("[1] h /f16 writereals"),
        Err(PsError::Rangecheck)
    );
    // An input file, and a closed file, are ioerror - even for an empty array.
    assert_eq!(it.run_str("[1] r writereals"), Err(PsError::Io));
    assert_eq!(it.run_str("[] r writereals"), Err(PsError::Io));
    it.run_str("h closefile").unwrap();
    assert_eq!(it.run_str("[1] h writereals"), Err(PsError::Io));
    assert_eq!(it.run_str("[] h writereals"), Err(PsError::Io));
}

#[test]
fn write_to_stdout_goes_through_the_interpreter() {
    let mut it = Interp::with_page(50, 50).unwrap();
    it.capture_stdout();
    it.run_str("(a) print [1 2] (%stdout) (w) file writereals (b) print [3] (%stdout) (w) file /f32 writereals")
        .unwrap();
    let mut want = b"a".to_vec();
    want.extend_from_slice(&bytes_of(&[1.0, 2.0]));
    want.push(b'b');
    want.extend_from_slice(&3.0f32.to_le_bytes());
    assert_eq!(it.captured_stdout(), want);
}

#[test]
fn write_byte_cap_counts_file_object_writes() {
    let (root, mut it) = setup("rrf-cap");
    let f = root.join("c.bin");
    let p = ps(&f);
    it.set_max_write_bytes(Some(40));
    it.run_str(&format!(
        "/h {p} (w) file def h (0123456789) writestring [1 2] h writereals"
    ))
    .unwrap();
    // 26 bytes used; 3 more doubles is 24: refused whole, nothing written.
    assert_eq!(it.run_str("[1 2 3] h writereals"), Err(PsError::Limitcheck));
    // The refusal wrote none of it, and 14 are left: one double fits.
    it.run_str("[1] h writereals h closefile").unwrap();
    assert_eq!(std::fs::read(&f).unwrap().len(), 10 + 16 + 8);
    // %stdout is not counted.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.capture_stdout();
    it.set_max_write_bytes(Some(1));
    it.run_str("[1 2 3] (%stdout) (w) file writereals").unwrap();
    assert_eq!(it.captured_stdout().len(), 24);
}

#[test]
fn append_file_with_two_writes_reads_back_with_all() {
    let (root, mut it) = setup("rrf-append");
    let f = root.join("ap.bin");
    let p = ps(&f);
    it.run_str(&format!(
        "/a {p} (a) file def [1 2] a writereals [3 4 5] a writereals {p} /all readreals"
    ))
    .unwrap();
    assert_eq!(top_reals(&it), [1.0, 2.0, 3.0, 4.0, 5.0]);
    it.run_str(&format!("{p} 2 /all readreals")).unwrap();
    assert_eq!(top_reals(&it), [3.0, 4.0, 5.0]);
}

#[test]
fn cli_appends_and_flushes_at_exit() {
    let root = common::scratch_dir("rrf-cli");
    let f = root.join("c.bin");
    let prog = format!(
        "/h {p} (w) file def [1 2 3] h writereals [4] h writereals",
        p = ps(&f)
    );
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--allow-write")
        .arg(&root)
        .args(["--headless", "-e", &prog])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        std::fs::read(&f).unwrap(),
        bytes_of(&[1.0, 2.0, 3.0, 4.0]),
        "the open file is flushed at exit"
    );
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .arg("--allow-write")
        .arg(&root)
        .args(["--headless", "-e"])
        .arg(format!("{p} /all readreals length =", p = ps(&f)))
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "4\n");
}

#[test]
fn step_limits_stop_the_new_forms() {
    let (root, mut it) = setup("rrf-steps");
    let p = ps(&root.join("big.bin"));
    // 200k reals is 1.5 MiB: well over 100 steps (one per KiB).
    it.run_str(&format!(
        "/a 200000 array def 0 1 199999 {{ a exch 1.5 put }} for a {p} writereals"
    ))
    .unwrap();
    // (set up before the limit, then the program that must stop)
    let cases = [
        (String::new(), format!("{p} /all readreals pop")),
        (String::new(), format!("{p} 10 /all readreals pop")),
        (
            format!("/h {p} (r) file def"),
            "h /all readreals pop".to_string(),
        ),
        (
            format!("/h {p} (r) file def"),
            "h 200000 readreals pop".to_string(),
        ),
        (
            format!("/h {} (a) file def", ps(&root.join("app.bin"))),
            "a h writereals".to_string(),
        ),
        (
            "/h (%stdout) (w) file def".to_string(),
            "a h writereals".to_string(),
        ),
    ];
    for (prelude, program) in cases {
        it.run_str("clear").unwrap();
        it.run_str(&prelude).unwrap();
        it.set_limits(pscat::Limits::none().with_max_steps(100))
            .unwrap();
        assert_eq!(
            it.run_str(&program),
            Err(PsError::StepsExceeded),
            "{program}"
        );
        it.set_limits(pscat::Limits::none()).unwrap();
    }
}
