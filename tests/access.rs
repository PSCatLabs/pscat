//! Write access on arrays (#1189): `bind` makes the procedures it enters
//! read-only, `readonly` and `packedarray` do the same, and a write to one
//! is `invalidaccess`. Every case is run by pscat, compared with the line
//! pinned from Ghostscript 10.07.1, and (when `gs` is installed) run through
//! `gs` too.
//!
//! Note the first case: `bind` does not touch the procedure it is given.
//! Only the procedures nested in it become read-only, so
//! `/p { 1 add } bind def /p load wcheck` is `true` in both interpreters.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// `(name, body, what gs prints)`. A body prints with `b` (a boolean or any
/// word) and an error it raises prints its name in place of the rest.
const CASES: &[(&str, &str, &str)] = &[
    // --- wcheck after bind
    ("top level stays writable", "/p { 1 add } bind def /p load wcheck b", "true"),
    ("bare bind stays writable", "{ 1 add } bind wcheck b", "true"),
    (
        "nested is read-only, top is not",
        "/p { { 1 add } } bind def /p load wcheck b /p load 0 get wcheck b",
        "true false",
    ),
    (
        "nested two deep",
        "/p { { { 1 add } } } bind def /p load 0 get wcheck b /p load 0 get 0 get wcheck b",
        "false false",
    ),
    (
        "nested inside an if",
        "/p { 1 add { 2 add } if } bind def /p load 2 get wcheck b /p load 2 get rcheck b /p load 2 get xcheck b",
        "false true true",
    ),
    (
        "bound twice over the same slot",
        "/x { 1 add } def /p [ /x load /x load ] cvx def /p load bind pop /p load 1 get wcheck b",
        "false",
    ),
    (
        "the dictionary's own reference stays writable",
        "/in { 1 add } def /p [ /in load ] cvx def /p load bind pop /in load wcheck b /p load 0 get wcheck b",
        "true false",
    ),
    (
        "an interval of a nested procedure",
        "/p { { 1 add } } bind def /p load 0 get dup 0 1 getinterval wcheck b cvlit wcheck b",
        "false false",
    ),
    (
        "aload of a bound procedure",
        "/a { { 1 add } { 2 add } } bind def /a load aload pop wcheck b wcheck b",
        "false false",
    ),
    // --- procedures that were already read-only
    (
        "a read-only procedure is not entered",
        "{ { 1 add } } readonly bind 0 get wcheck b",
        "true",
    ),
    (
        "nor are its names bound",
        "{ add } readonly bind 0 get type b",
        "nametype",
    ),
    (
        "a read-only nested procedure is not entered",
        "/r { { 1 add } } readonly def /p [ /r load ] cvx def /p load bind pop /p load 0 get 0 get wcheck b",
        "true",
    ),
    // --- packed arrays, readonly, copies
    ("packedarray is read-only", "1 2 2 packedarray wcheck b", "false"),
    (
        "bind of a packed procedure",
        "{ 1 add } aload pop 2 packedarray cvx bind wcheck b",
        "false",
    ),
    (
        "cvx and getinterval keep it",
        "[ 1 2 ] readonly dup 0 1 getinterval wcheck b dup cvx wcheck b cvx cvlit wcheck b",
        "false false false",
    ),
    (
        "a copy into a fresh array is writable",
        "[ 1 2 ] readonly dup length array copy wcheck b",
        "true",
    ),
    ("readonly is not identity", "[ 1 2 ] readonly [ 1 2 ] eq b", "false"),
    ("eq ignores the attribute", "[ 1 2 ] readonly dup eq b", "true"),
    // --- the writes
    (
        "put into a nested bound procedure",
        "/p { { 1 add } } bind def /p load 0 get 0 1 put (done) b",
        "invalidaccess",
    ),
    (
        "put into the top level is allowed",
        "/p { 1 add } bind def /p load 0 1 put (done) b",
        "done",
    ),
    ("put into a read-only array", "5 array readonly 0 1 put (done) b", "invalidaccess"),
    ("astore into one", "5 array readonly astore (done) b", "invalidaccess"),
    ("copy into one", "[ 1 2 ] 3 array readonly copy pop (done) b", "invalidaccess"),
    (
        "putinterval from a read-only source",
        "[ 1 2 3 ] 0 [ 7 ] readonly putinterval (done) b",
        "done",
    ),
    (
        "putinterval into a read-only one",
        "[ 1 2 3 ] readonly 0 [ 7 ] putinterval (done) b",
        "invalidaccess",
    ),
    (
        "stopped sees the name",
        "/p { { 1 add } } bind def { /p load 0 get 0 1 put } stopped b $error /errorname get b",
        "true invalidaccess",
    ),
];

/// Plain PostScript, valid in pscat and in gs.
const PRELUDE: &str = "/b { dup type /booleantype eq { 8 string cvs } if \
dup type /nametype eq { 32 string cvs } if print ( ) print } def\n";

fn program() -> String {
    let mut src = String::from(PRELUDE);
    for (name, body, _) in CASES {
        // The error name goes in place of whatever the body would print
        // after the failing operator; `clear` keeps the next case clean.
        src.push_str(&format!(
            "({name}: ) print {{ {body} }} stopped {{ $error /errorname get b }} if clear (\\n) print\n"
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
    let dir = common::scratch_dir("access_gs");
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

/// A restore un-binds, flag and all: the journalled array comes back with
/// the slot writable again.
#[test]
fn restore_makes_the_nested_procedure_writable_again() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(
        "/p { { 1 add } } def save /s exch def /p load bind pop \
         /p load 0 get wcheck = s restore /p load 0 get wcheck =",
    )
    .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let got = tidy(&String::from_utf8_lossy(it.captured_stdout()));
    assert_eq!(got, ["false", "true"]);
}

/// The attribute rides in padding: the object is no bigger than it was, so
/// the allocation accounting that multiplies by its size is unchanged.
#[test]
fn the_access_flag_does_not_grow_an_object() {
    assert_eq!(
        std::mem::size_of::<pscat::Object>(),
        std::mem::size_of::<(pscat::Value, bool)>()
    );
}
