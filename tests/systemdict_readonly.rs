//! `systemdict` is read-only to a program (#1337): `put`, `def` (after
//! `systemdict begin`), `store` of a name systemdict defines, `undef` and a
//! `copy` into it are `invalidaccess`, as in Ghostscript. Reading it, and
//! `store` of a name nothing defines (which binds in the current dictionary),
//! still work. Every case is run by pscat, compared with the line pinned from
//! Ghostscript 10.07.1, and (when `gs` is installed) run through `gs` too.
//!
//! Each line is the error name (or nothing), then the operand-stack depth and
//! the dictionary-stack depth the failed block left, so a refused write is
//! also pinned to leave the operands exactly where gs leaves them.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// `(name, body, what gs prints)`.
const CASES: &[(&str, &str, &str)] = &[
    (
        "put a new name",
        "systemdict /zq1 1 put",
        "invalidaccess 3 3",
    ),
    (
        "put over an operator",
        "systemdict /add 1 put",
        "invalidaccess 3 3",
    ),
    (
        "put an integer key",
        "systemdict 5 1 put",
        "invalidaccess 3 3",
    ),
    (
        "def after systemdict begin",
        "systemdict begin /zq2 2 def",
        "invalidaccess 2 4",
    ),
    (
        "def of a procedure over an operator",
        "systemdict begin /add { } def",
        "invalidaccess 2 4",
    ),
    (
        "def of a non-name key",
        "systemdict begin 5 2 def",
        "invalidaccess 2 4",
    ),
    (
        "store over an operator",
        "/add 1 store",
        "invalidaccess 2 3",
    ),
    (
        "store a procedure over an operator",
        "/add { } store",
        "invalidaccess 2 3",
    ),
    (
        "undef an operator",
        "systemdict /add undef",
        "invalidaccess 2 3",
    ),
    (
        "copy into systemdict",
        "<< /zq3 1 >> systemdict copy",
        "invalidaccess 2 3",
    ),
    // What keeps working.
    (
        "store of an unknown name binds in userdict",
        "/zq4 1 store userdict /zq4 known b systemdict /zq4 known b",
        "true false 0 3",
    ),
    (
        "store of a name userdict defines stays in userdict",
        "/zq5 1 def /zq5 2 store zq5 b",
        "2 0 3",
    ),
    ("def in userdict", "/zq6 3 def zq6 b", "3 0 3"),
    ("put in userdict", "userdict /zq7 4 put zq7 b", "4 0 3"),
    (
        "only systemdict is not writable",
        "systemdict wcheck b userdict wcheck b 3 dict wcheck b",
        "false true true 0 3",
    ),
    (
        "systemdict reads",
        "systemdict /add get type b systemdict /add known b systemdict /zq8 known b",
        "operatortype true false 0 3",
    ),
    (
        "an operator still runs after a refused store",
        "{ /add 1 store } stopped pop clear 1 2 add b",
        "3 0 3",
    ),
    (
        "the stopped error name",
        "{ systemdict /zq9 1 put } stopped b $error /errorname get b",
        "true invalidaccess 3 3",
    ),
];

/// Plain PostScript, valid in pscat and in gs. A failed block's operands stay
/// on the stack (`count` is read before `clear`); `cleardictstack` undoes a
/// `systemdict begin`.
const PRELUDE: &str = "/b { dup type /booleantype eq { 8 string cvs } if \
dup type /integertype eq { 8 string cvs } if \
dup type /nametype eq { 32 string cvs } if print ( ) print } def\n";

fn program() -> String {
    let mut src = String::from(PRELUDE);
    for (name, body, _) in CASES {
        src.push_str(&format!(
            "({name}: ) print {{ {body} }} stopped {{ $error /errorname get b }} if \
count b countdictstack b clear cleardictstack (\\n) print\n"
        ));
    }
    src
}

fn tidy(out: &str) -> Vec<String> {
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn want() -> Vec<String> {
    CASES
        .iter()
        .map(|(name, _, out)| format!("{name}: {out}"))
        .collect()
}

#[test]
fn pscat_prints_the_lines_gs_printed() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&program())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let got = tidy(&String::from_utf8_lossy(it.captured_stdout()));
    assert_eq!(got, want());
}

#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("systemdict_readonly_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, program()).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(tidy(&String::from_utf8_lossy(&out.stdout)), want());
}

/// A refused write leaves systemdict as it was.
#[test]
fn a_refused_write_leaves_systemdict_unchanged() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(
        "systemdict length /n exch def \
         { systemdict /zq 1 put } stopped pop \
         { /add 1 store } stopped pop \
         { systemdict /add undef } stopped pop \
         { systemdict begin /zq 1 def } stopped pop cleardictstack \
         systemdict length n eq = \
         systemdict /add get /add load eq =",
    )
    .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let got = tidy(&String::from_utf8_lossy(it.captured_stdout()));
    assert_eq!(got, ["true", "true"]);
}
