//! Executable strings and files run as source (issue #1321, after #1122):
//! `exit` inside one leaves the loop around it, `noaccess` forbids running
//! one, and `//name` of one runs it. Each program is checked against the
//! output below, which is what Ghostscript 10.07.1 prints for it
//! (`gs -q -dNODISPLAY -dNOSAFER -dBATCH -dNOPAUSE -c '<wrapped program>'`);
//! when `gs` is installed it is run on every program too.

mod common;

use std::path::Path;

use pscat::Interp;

/// The tail every program ends with: the stack, bottom first, one `==` per
/// object.
const SHOW: &str = "count 1 sub -1 0 { index == } for";

/// `prog` under `stopped`: an error prints its name and the depth the stack
/// is left at (then clears it), so the operand an error leaves behind is
/// compared too. The stack holds no `noaccess` string when it is printed:
/// pscat prints one as `(7)`, gs as `-string-`.
fn stopped(prog: &str) -> String {
    format!(
        "{{ {prog} }} stopped {{ $error /errorname get == count == clear }} {{ {SHOW} }} ifelse"
    )
}

/// `prog` as a string run by `stopped`, for a `//name` that must raise an
/// error the test can catch.
fn in_string(prog: &str) -> String {
    format!(
        "({prog}) cvx stopped {{ $error /errorname get == count == clear }} {{ {SHOW} }} ifelse"
    )
}

/// `prog` as top-level source.
fn top(prog: &str) -> String {
    format!("{prog} {SHOW}")
}

fn pscat(prog: &str) -> String {
    let mut it = Interp::with_page(50, 50).expect("page");
    it.capture_stdout();
    it.run_str(prog)
        .unwrap_or_else(|e| panic!("pscat: {prog}: {e}"));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

fn gs(prog: &str) -> String {
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
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn check(cases: &[(String, &str)]) {
    let have_gs = common::gs_available();
    for (prog, want) in cases {
        assert_eq!(pscat(prog), *want, "pscat: {prog}");
        if have_gs {
            assert_eq!(gs(prog), *want, "gs: {prog}");
        }
    }
}

/// An `exit` met inside an executable string or file, however it got
/// there, ends the innermost loop. A `stopped`, a `run` file and the
/// outside of any loop are still boundaries (`invalidexit`).
#[test]
fn exit_inside_an_executable_string_leaves_the_loop() {
    let cases: Vec<(String, &str)> = [
        ("1 { [ (exit) cvx ] cvx exec 2 } loop 3", "1\n3\n"),
        ("1 { (exit) cvx exec 2 } loop 3", "1\n3\n"),
        ("1 { (1 2 exit 3) cvx exec 9 } loop 4", "1\n1\n2\n4\n"),
        ("1 { ((exit) cvx exec) cvx exec 2 } loop 3", "1\n3\n"),
        ("3 { [ (exit) cvx ] cvx exec 2 } repeat 3", "3\n"),
        ("1 1 3 { pop (exit) cvx exec 2 } for 3", "3\n"),
        ("[ 1 2 3 ] { pop (exit) cvx exec 2 } forall 3", "3\n"),
        // No loop to leave.
        ("(exit) cvx exec", "/invalidexit\n0\n"),
        ("[ (exit) cvx ] cvx exec", "/invalidexit\n0\n"),
        // A `stopped` is a boundary: it catches the `invalidexit`, and the
        // loop around it goes on.
        (
            "3 { { (exit) cvx exec } stopped pop 2 } repeat",
            "2\n2\n2\n",
        ),
        (
            "1 { { [ (exit) cvx ] cvx exec } stopped pop 2 exit } loop 3",
            "1\n2\n3\n",
        ),
    ]
    .into_iter()
    .map(|(p, w)| (stopped(p), w))
    .collect();
    check(&cases);
}

/// The same through an executable file, and the `run` boundary.
#[test]
fn exit_inside_an_executable_file() {
    let root = common::scratch_dir("exec-strings-files");
    let exit_ps = root.join("exit.ps");
    std::fs::write(&exit_ps, "exit\n").unwrap();
    let f = ps_path(&exit_ps);
    let cases: Vec<(String, &str)> = vec![
        // An executable file `exit` is in does not stop it.
        (
            stopped(&format!("1 {{ {f} (r) file cvx exec 2 }} loop 3")),
            "1\n3\n",
        ),
        (
            stopped(&format!("1 {{ [ {f} (r) file cvx ] cvx exec 2 }} loop 3")),
            "1\n3\n",
        ),
        // `run` is a source boundary.
        (
            stopped(&format!("1 {{ {f} run 2 }} loop 3")),
            "/invalidexit\n1\n",
        ),
    ];
    check(&cases);
}

/// `noaccess` forbids running a string or file as source, however it is
/// reached; `readonly` and `executeonly` do not.
#[test]
fn a_noaccess_executable_string_raises_invalidaccess() {
    let cases: Vec<(String, &str)> = [
        // `exec` refuses before it takes the operand: it stays.
        ("(7) cvx noaccess exec", "/invalidaccess\n1\n"),
        ("(7) cvx noaccess dup exec", "/invalidaccess\n2\n"),
        ("(7) noaccess cvx exec", "/invalidaccess\n1\n"),
        ("(7) cvx noaccess cvlit cvx exec", "/invalidaccess\n1\n"),
        // As an element of a procedure, the string has moved on: nothing
        // is left.
        ("[ (7) cvx noaccess ] cvx exec", "/invalidaccess\n0\n"),
        ("/n (7) cvx noaccess def n", "/invalidaccess\n0\n"),
        (
            "/n (7) cvx noaccess def [ /n cvx ] cvx exec",
            "/invalidaccess\n0\n",
        ),
        ("(7) cvx executeonly exec", "7\n"),
        ("(7) cvx readonly exec", "7\n"),
        // Only the reference that was restricted.
        ("(7) cvx noaccess pop (8) cvx exec", "8\n"),
        // The reference keeps it through the stack and container operators.
        (
            "(7) cvx noaccess 1 array astore 0 get exec",
            "/invalidaccess\n1\n",
        ),
        (
            "/d 1 dict def d /k (7) cvx noaccess put d /k get exec",
            "/invalidaccess\n1\n",
        ),
        (
            "save (7) cvx noaccess exch restore exec",
            "/invalidaccess\n1\n",
        ),
        ("(77) cvx noaccess cvlit cvx exec", "/invalidaccess\n1\n"),
        // `copy` reads and writes: gs refuses either end, before it checks
        // the length, and leaves both operands.
        ("(7) (x) cvx noaccess copy exec", "/invalidaccess\n2\n"),
        ("(x) cvx noaccess (7) copy exec", "/invalidaccess\n2\n"),
        ("(abc) cvx noaccess (7) copy", "/invalidaccess\n2\n"),
        ("(7) (abcdef) cvx noaccess copy", "/invalidaccess\n2\n"),
        // Restricting it further is refused; `noaccess` again is not.
        ("(7) cvx noaccess readonly 1", "/invalidaccess\n1\n"),
        ("(7) cvx noaccess executeonly 1", "/invalidaccess\n1\n"),
        ("(7) cvx noaccess noaccess 1 exch pop", "1\n"),
    ]
    .into_iter()
    .map(|(p, w)| (stopped(p), w))
    .collect();
    check(&cases);
}

/// `cvs`, `cvrs`, `search`, `anchorsearch` and `token` read or write the
/// string, so gs refuses a `noaccess` one outright (`invalidaccess`) and
/// leaves every operand where it was; none of them hands back a view
/// carrying the flag (#1362). Each refusal comes before the check that would
/// have raised `rangecheck`. `cvrs` is the exception: a bad radix wins, and
/// its number comes after the access check. The depth printed after the name
/// is the operand count left.
#[test]
fn a_noaccess_string_is_refused_by_cvs_cvrs_search_anchorsearch_and_token() {
    let cases: Vec<(String, &str)> = [
        // `cvs`: the buffer or the string being converted.
        ("7 (x) noaccess cvs", "/invalidaccess\n2\n"),
        ("7 (x) noaccess cvs cvx exec", "/invalidaccess\n2\n"),
        ("12345 (x) noaccess cvs", "/invalidaccess\n2\n"),
        ("(abcdef) noaccess 2 string cvs", "/invalidaccess\n2\n"),
        ("(abcdef) 2 string noaccess cvs", "/invalidaccess\n2\n"),
        ("(7) noaccess 10 string cvs", "/invalidaccess\n2\n"),
        ("(a) noaccess (a) noaccess cvs", "/invalidaccess\n2\n"),
        // The same calls on a reference that is not `noaccess`.
        ("(abcdef) 2 string cvs", "/rangecheck\n2\n"),
        ("7 (xy) dup noaccess pop cvs", "(7)\n"),
        ("(ab) dup noaccess pop 4 string cvs", "(ab)\n"),
        // `cvrs`.
        ("255 16 (xx) noaccess cvrs", "/invalidaccess\n3\n"),
        ("255 16 (x) noaccess cvrs", "/invalidaccess\n3\n"),
        ("(a) 10 (xx) noaccess cvrs", "/invalidaccess\n3\n"),
        ("999 40 (x) noaccess cvrs", "/rangecheck\n3\n"),
        ("255 1 (x) noaccess cvrs", "/rangecheck\n3\n"),
        ("255 (a) (xx) noaccess cvrs", "/typecheck\n3\n"),
        ("255 16 (xx) dup noaccess pop cvrs", "(FF)\n"),
        // `search`: either operand, found or not.
        ("(7) noaccess (z) search", "/invalidaccess\n2\n"),
        (
            "(7) noaccess (z) search pop cvx exec",
            "/invalidaccess\n2\n",
        ),
        ("(abcdef) noaccess (cd) search", "/invalidaccess\n2\n"),
        ("(abc) (b) noaccess search", "/invalidaccess\n2\n"),
        ("() noaccess () search", "/invalidaccess\n2\n"),
        ("(abcdef) (cd) search", "(ef)\n(cd)\n(ab)\ntrue\n"),
        ("(abcdef) dup noaccess pop (z) search", "(abcdef)\nfalse\n"),
        // `anchorsearch`.
        ("(abc) noaccess (a) anchorsearch", "/invalidaccess\n2\n"),
        ("(abc) (a) noaccess anchorsearch", "/invalidaccess\n2\n"),
        ("(abc) noaccess (z) anchorsearch", "/invalidaccess\n2\n"),
        ("(abc) () noaccess anchorsearch", "/invalidaccess\n2\n"),
        ("(abc) (a) anchorsearch", "(bc)\n(a)\ntrue\n"),
        // `token`: a `noaccess` string, executable or not.
        ("(7 8) noaccess token", "/invalidaccess\n1\n"),
        ("(7 8) noaccess cvx token", "/invalidaccess\n1\n"),
        ("(7 8) dup noaccess pop token", "(8)\n7\ntrue\n"),
    ]
    .into_iter()
    .map(|(p, w)| (stopped(p), w))
    .collect();
    check(&cases);
}

#[test]
fn a_noaccess_executable_file_raises_invalidaccess() {
    let root = common::scratch_dir("exec-strings-noaccess-file");
    let add_ps = root.join("add.ps");
    std::fs::write(&add_ps, "7 8 add\n").unwrap();
    let f = ps_path(&add_ps);
    let cases: Vec<(String, &str)> = vec![
        (
            stopped(&format!("{f} (r) file cvx noaccess exec")),
            "/invalidaccess\n1\n",
        ),
        (
            stopped(&format!("[ {f} (r) file cvx noaccess ] cvx exec")),
            "/invalidaccess\n0\n",
        ),
        (stopped(&format!("{f} (r) file cvx exec")), "15\n"),
    ];
    check(&cases);
}

/// `//name` met in a scanned stream whose value is an executable string runs
/// it, as it does inside a procedure.
///
/// A multi-token string is not pinned against gs: there gs runs the first
/// token and pushes an arbitrary integer for the rest (`/s (7 8 add) cvx
/// def //s pstack` prints `502392012220 7`, a different number each run),
/// which no interpreter should copy. Inside a procedure, which gs gets right,
/// the whole string runs (`{ //s } exec` is 15).
#[test]
fn an_immediate_name_whose_value_is_an_executable_string_runs_it() {
    let tops: Vec<(String, &str)> = [
        ("/s (7) cvx def //s", "7\n"),
        ("/s (7) def //s", "(7)\n"),
        ("/s (7) cvx def //s //s", "7\n7\n"),
        ("/s (add) cvx def 1 2 //s", "3\n"),
        ("/s (count) cvx def 5 6 //s", "5\n6\n2\n"),
        // An executable array is pushed, not run.
        ("/s {1 2} def //s", "{1 2}\n"),
        ("/s (7 8 add) cvx def { //s } exec", "15\n"),
        ("/s (7 8 add) cvx def { //s }", "{(7 8 add)}\n"),
        ("/s (exit) cvx def 1 { //s 2 } loop 3", "1\n3\n"),
        ("/s (7) cvx executeonly def //s", "7\n"),
    ]
    .into_iter()
    .map(|(p, w)| (top(p), w))
    .collect();
    check(&tops);
    // And a `noaccess` one refuses.
    let refused: Vec<(String, &str)> = [
        ("/s (7) cvx noaccess def //s", "/invalidaccess\n0\n"),
        ("/s (7) cvx noaccess def 1 //s", "/invalidaccess\n1\n"),
    ]
    .into_iter()
    .map(|(p, w)| (in_string(p), w))
    .collect();
    check(&refused);
    // The whole multi-token string runs where gs's result is unstable.
    assert_eq!(pscat(&top("/s (7 8 add) cvx def //s")), "15\n");
    assert_eq!(pscat(&top("/s (7 8) cvx def //s")), "7\n8\n");
}

/// Tail recursion through a string: gs loops forever in constant space, and
/// pscat stops with `execstackoverflow` (#1321 item 3, a documented
/// deviation: only the error differs). Pinned so a change shows.
#[test]
fn tail_recursion_through_a_string_is_a_documented_deviation() {
    let got = pscat(&stopped("/f [ (f) cvx ] cvx def f"));
    assert_eq!(got, "/execstackoverflow\n0\n");
}

fn ps_path(p: &Path) -> String {
    let s = p.to_str().unwrap();
    assert!(!s.contains(['(', ')']), "{s}");
    format!("({})", s.replace('\\', "\\\\"))
}
