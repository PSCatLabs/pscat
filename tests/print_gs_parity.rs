//! `=`, `==`, `=only`, `==only`, `stack` and `cvs` against Ghostscript 10.07.1
//! (#1428): an operator prints its name, a string's bytes above 127 go out
//! raw, and an exponent is spelled `1e+20`. Each expected line below was
//! taken from a gs 10.07.1 run, and the `gs_*` tests run the same program
//! through a real `gs` when one is installed.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

fn run_bytes(src: &str) -> Vec<u8> {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.captured_stdout().to_vec()
}

fn run(src: &str) -> String {
    String::from_utf8_lossy(&run_bytes(src)).into_owned()
}

/// Item 1. gs: `/add load =` prints `add`, `==` prints `--add--`, `stack`
/// prints `add`, `cvs` gives `add`. An executable name prints as itself.
#[test]
fn an_operator_prints_its_name_under_the_text_forms() {
    assert_eq!(run("/add load ="), "add\n");
    assert_eq!(run("/add load =only (|) ="), "add|\n");
    assert_eq!(run("/add load =="), "--add--\n");
    assert_eq!(run("/add load ==only (|) ="), "--add--|\n");
    assert_eq!(run("{add} 0 get ="), "add\n");
    assert_eq!(run("{add} 0 get =="), "add\n");
    assert_eq!(run("/add load stack"), "add\n");
    assert_eq!(run("/add load pstack"), "--add--\n");
    assert_eq!(run("/add load 20 string cvs ="), "add\n");
}

/// The case the issue names: the failed operator `$error /command` holds.
#[test]
fn the_command_in_error_is_printed_by_name() {
    assert_eq!(
        run("{ 1 0 div } stopped pop $error /command get ="),
        "div\n"
    );
}

/// What does not have a string value still prints `--nostringval--`
/// (gs: `mark =`, `null =`, `[1 2] =`, `{1 2} =`, `<< >> =`).
#[test]
fn values_with_no_string_form_still_print_nostringval() {
    assert_eq!(
        run("mark = null = [1 2] = {1 2} = << >> ="),
        "--nostringval--\n".repeat(5)
    );
}

/// Item 2. gs writes the raw bytes of `(a\351b\377\200)` under `=`, `=only`,
/// `stack` and `cvs`, and escapes them under `==`.
#[test]
fn a_string_with_high_bytes_is_written_raw() {
    let raw: &[u8] = b"a\xe9b\xff\x80";
    let mut want = raw.to_vec();
    want.push(b'\n');
    assert_eq!(run_bytes("(a\\351b\\377\\200) ="), want);
    assert_eq!(run_bytes("(a\\351b\\377\\200) stack"), want);
    assert_eq!(run_bytes("(a\\351b\\377\\200) =only"), raw);
    assert_eq!(
        run_bytes("(a\\351b\\377\\200) 10 string cvs ="),
        want,
        "cvs copies the bytes"
    );
    assert_eq!(run("(a\\351b\\377\\200) =="), "(a\\351b\\377\\200)\n");
    assert_eq!(run("(a\\351b) 10 string cvs =="), "(a\\351b)\n");
}

/// A string `stack` shows is cut on bytes, not decoded.
#[test]
fn stack_cuts_a_string_on_bytes() {
    let out = run_bytes("5000 string dup 0 1 4999 { 1 index exch 233 put } for stack");
    assert!(out.starts_with(b"\xe9\xe9"));
    assert!(out.ends_with(b" ... truncated\n"));
}

/// `cvs` into a buffer that is the string itself (one storage for both
/// operands) copies first, so it neither panics nor changes the bytes.
#[test]
fn cvs_of_a_string_into_itself_is_safe() {
    assert_eq!(
        run_bytes("(a\\351b) dup dup cvs ="),
        b"a\xe9b\n".to_vec(),
        "same storage"
    );
}

/// A string longer than the buffer is `rangecheck`, and is refused before
/// a copy of it is made: an 8 MiB string into a 4-byte buffer.
#[test]
fn cvs_of_a_long_string_is_a_rangecheck() {
    assert_eq!(
        run("{ 8388608 string 4 string cvs } stopped { $error /errorname get = } if"),
        "rangecheck\n"
    );
}

/// Item 3, the part gs's `=` and `==` agree on: an exponent has a sign and
/// at least two digits. Every line is from gs 10.07.1 (`1e20 =` is
/// `1e+20`, `1e-5 ==` is `1e-05`, `-1e20 ==` is `-1e+20`, `3.4e38 =` is
/// `3.4e+38`, `1.5e-10 ==` is `1.5e-10`).
#[test]
fn an_exponent_is_spelled_as_gs_spells_it() {
    for (v, want) in [
        ("1e20", "1e+20"),
        ("-1e20", "-1e+20"),
        ("1e30", "1e+30"),
        ("1e19", "1e+19"),
        ("1e16", "1e+16"),
        ("3.4e38", "3.4e+38"),
        ("1e-5", "1e-05"),
        ("1.5e-5", "1.5e-05"),
        ("1.5e-10", "1.5e-10"),
        ("1e-100", "1e-100"),
        ("1e100", "1e+100"),
    ] {
        for form in ["=", "==", "=only", "==only"] {
            let nl = if form.ends_with("only") { "" } else { "\n" };
            assert_eq!(
                run(&format!("{v} {form}")),
                format!("{want}{nl}"),
                "{v} {form}"
            );
        }
        assert_eq!(
            run(&format!("{v} 20 string cvs =")),
            format!("{want}\n"),
            "{v} cvs"
        );
        assert_eq!(run(&format!("{v} stack")), format!("{want}\n"), "{v} stack");
        assert_eq!(
            run(&format!("{v} pstack")),
            format!("{want}\n"),
            "{v} pstack"
        );
    }
    // What was printed reads back as the same number.
    assert_eq!(run("1e20 20 string cvs cvr 1e20 eq ="), "true\n");
    assert_eq!(run("1e-5 20 string cvs cvr 1e-5 eq ="), "true\n");
}

/// What this change leaves alone, by design. gs prints a real as a float32
/// and its two printers disagree with each other on the same number
/// (`123456789.5 =` is `1.23457e+08`, `123456789.5 ==` is `123456792.0`,
/// `123456.7 =` is `123457.0`, `1e6 =` is `1e+06`, `1e15 =` is `1e+15`,
/// `0.1 0.2 add =` is `0.3`). pscat's reals are f64: it prints every digit
/// and writes a value below 1e16 in plain decimal.
#[test]
fn pscat_keeps_its_own_digits_and_its_own_switch_to_an_exponent() {
    assert_eq!(run("123456789.5 ="), "123456789.5\n");
    assert_eq!(run("123456789.5 =="), "123456789.5\n");
    assert_eq!(run("123456.7 ="), "123456.7\n");
    assert_eq!(run("1e6 ="), "1000000.0\n");
    assert_eq!(run("1e15 =="), "1000000000000000.0\n");
    assert_eq!(run("0.1 0.2 add ="), "0.30000000000000004\n");
}

/// The cases above where gs and pscat print the same bytes, through a real
/// `gs`: the program and the expected text are the same.
#[test]
fn gs_prints_the_same_bytes() {
    if !common::gs_available() {
        return;
    }
    let src = "\
/add load = /add load =only (|\\n) print /add load == /add load ==only (|\\n) print \
{add} 0 get = {add} 0 get == /add load stack /add load pstack /add load 20 string cvs = \
(a\\351b\\377\\200) = (a\\351b\\377\\200) =only (|\\n) print (a\\351b\\377\\200) == \
(a\\351b\\377\\200) stack (a\\351b\\377\\200) 10 string cvs = \
1e20 = 1e20 == -1e20 = 1e30 = 3.4e38 == 1e-5 = 1e-5 == 1.5e-5 == 1.5e-10 = 1e16 = \
1e20 20 string cvs = 1e-5 pstack 1e20 stack clear \
mark = null = [1 2] = {1 2} = << >> =\n";
    let dir = common::scratch_dir("print_gs_parity");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, src).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(out.status.success(), "gs failed");
    assert_eq!(run_bytes(src), out.stdout);
}
