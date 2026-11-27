//! The Level 2 user-object operators (#1265): `defineuserobject`,
//! `execuserobject` and `undefineuserobject`. Every case below is run by
//! pscat, compared with the line pinned from Ghostscript 10.07.1, and (when
//! `gs` is installed) run through `gs` too, so the table is both the
//! specification and its evidence. The reference is Ghostscript's own
//! PostScript definition of the three (`Resource/Init/gs_dps2.ps`), which
//! copies Adobe's array growth: the first array has `max(index + 1, 10)`
//! slots; a later index at or past the length makes a new array of twice
//! the index when the index equals the length, else `index + 1`; an index
//! over 65535 is `rangecheck`.
//!
//! A failing case prints `!errorname command`: the error name and the
//! offending command, both as `gs` reports them. One difference is
//! deliberate and pinned separately (`an_error_inside_the_object_blames_...`):
//! when the *object* run by `execuserobject` fails, `gs` names
//! `execuserobject` (its operator is a PostScript procedure there), while
//! pscat names the command that failed, as `exec` does.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// `(name, body, what gs prints)`. `userdict /UserObjects undef` runs
/// before each body, so a case starts with no array.
const CASES: &[(&str, &str, &str)] = &[
    (
        "define then exec a procedure",
        r#"0 {1 2 add} defineuserobject 0 execuserobject b"#,
        "3",
    ),
    (
        "exec pushes a literal",
        r#"5 (hi) defineuserobject 5 execuserobject b"#,
        "hi",
    ),
    (
        "exec pushes a name",
        r#"5 /foo defineuserobject 5 execuserobject b"#,
        "foo",
    ),
    (
        "exec runs an executable name",
        r#"0 /add cvx defineuserobject 3 4 0 execuserobject b"#,
        "7",
    ),
    (
        "exec runs an executable string",
        r#"0 (1 2 add b) cvx defineuserobject 0 execuserobject"#,
        "3",
    ),
    (
        "an empty slot is null",
        r#"1 {} defineuserobject 5 execuserobject type b"#,
        "nulltype",
    ),
    (
        "exec leaves the operands under it",
        r#"0 {1} defineuserobject 7 0 execuserobject count b"#,
        "2",
    ),
    (
        "UserObjects is absent until the first define",
        r#"userdict /UserObjects known b 0 1 defineuserobject userdict /UserObjects known b"#,
        "false true",
    ),
    (
        "first array has ten slots",
        r#"0 1 defineuserobject userdict /UserObjects get length b "#,
        "10",
    ),
    (
        "first array at index 9",
        r#"9 1 defineuserobject userdict /UserObjects get length b "#,
        "10",
    ),
    (
        "first array at index 10",
        r#"10 1 defineuserobject userdict /UserObjects get length b "#,
        "11",
    ),
    (
        "first array at index 100",
        r#"100 1 defineuserobject userdict /UserObjects get length b "#,
        "101",
    ),
    (
        "first array at the largest index",
        r#"65535 1 defineuserobject userdict /UserObjects get length b "#,
        "65536",
    ),
    (
        "the largest index is usable",
        r#"65535 (z) defineuserobject 65535 execuserobject b"#,
        "z",
    ),
    (
        "growth sequence",
        r#"9 1 defineuserobject userdict /UserObjects get length b 10 1 defineuserobject userdict /UserObjects get length b 11 1 defineuserobject userdict /UserObjects get length b 20 1 defineuserobject userdict /UserObjects get length b 22 1 defineuserobject userdict /UserObjects get length b 23 1 defineuserobject userdict /UserObjects get length b 25 1 defineuserobject userdict /UserObjects get length b 100 1 defineuserobject userdict /UserObjects get length b "#,
        "10 20 20 40 40 40 40 101",
    ),
    (
        "growth keeps the old slots",
        r#"3 (a) defineuserobject 50 (b) defineuserobject 3 execuserobject b 50 execuserobject b 4 execuserobject type b"#,
        "a b nulltype",
    ),
    (
        "growth makes a new array",
        r#"0 1 defineuserobject userdict /UserObjects get /old exch def 12 1 defineuserobject old length b userdict /UserObjects get length b"#,
        "10 13",
    ),
    (
        "define in range writes in place",
        r#"0 1 defineuserobject userdict /UserObjects get /old exch def 3 (x) defineuserobject old 3 get b userdict /UserObjects get old eq b"#,
        "x true",
    ),
    (
        "redefine replaces",
        r#"2 (a) defineuserobject 2 (b) defineuserobject 2 execuserobject b"#,
        "b",
    ),
    (
        "undefine empties the slot",
        r#"2 (a) defineuserobject 2 undefineuserobject 2 execuserobject type b userdict /UserObjects get length b "#,
        "nulltype 10",
    ),
    (
        "undefine twice",
        r#"2 (a) defineuserobject 2 undefineuserobject 2 undefineuserobject 2 execuserobject type b"#,
        "nulltype",
    ),
    (
        "a procedure runs where exec runs it",
        r#"0 {exit} defineuserobject 3 { 0 execuserobject (no) print } repeat (done) print"#,
        "done",
    ),
    (
        "an object defined by reference shares it",
        r#"/a [1 2] def 0 a defineuserobject a 0 99 put 0 execuserobject 0 get b"#,
        "99",
    ),
    (
        "index too big",
        r#"65536 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "index too big leaves no array",
        r#"{ 65536 {} defineuserobject } stopped pop clear userdict /UserObjects known b"#,
        "false",
    ),
    (
        "huge index",
        r#"1000000000 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "huge index leaves an existing array",
        r#"0 1 defineuserobject { 1000000000 {} defineuserobject } stopped pop clear userdict /UserObjects get length b "#,
        "10",
    ),
    (
        "negative index",
        r#"-1 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "negative index still makes the array",
        r#"{ -1 {} defineuserobject } stopped pop clear userdict /UserObjects get length b "#,
        "10",
    ),
    (
        "negative index on an existing array",
        r#"0 1 defineuserobject -1 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "define index is a string",
        r#"(a) {} defineuserobject"#,
        "!typecheck defineuserobject",
    ),
    (
        "define index is a name",
        r#"/a {} defineuserobject"#,
        "!typecheck defineuserobject",
    ),
    (
        "define real index in range",
        r#"3.0 {} defineuserobject"#,
        "!typecheck defineuserobject",
    ),
    (
        "define real index makes the array when it is small",
        r#"{ 3.0 {} defineuserobject } stopped pop clear userdict /UserObjects known b"#,
        "true",
    ),
    (
        "define real index does not when it is not",
        r#"{ 20.0 {} defineuserobject } stopped pop clear userdict /UserObjects known b"#,
        "false",
    ),
    (
        "define real index too big",
        r#"100000.0 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "define real index leaves an existing array",
        r#"5 1 defineuserobject { 30.0 {} defineuserobject } stopped pop clear userdict /UserObjects get length b "#,
        "10",
    ),
    (
        "define operands stay",
        r#"{ 1.5 (v) defineuserobject } stopped pop count b"#,
        "2",
    ),
    (
        "define underflow",
        r#"1 defineuserobject"#,
        "!stackunderflow defineuserobject",
    ),
    (
        "define empty",
        r#"defineuserobject"#,
        "!stackunderflow defineuserobject",
    ),
    (
        "exec index is real",
        r#"0 {} defineuserobject 0.0 execuserobject"#,
        "!typecheck execuserobject",
    ),
    (
        "exec index is a string",
        r#"(a) execuserobject"#,
        "!typecheck execuserobject",
    ),
    (
        "exec index is null",
        r#"0 {} defineuserobject null execuserobject"#,
        "!typecheck execuserobject",
    ),
    (
        "exec before any define",
        r#"3 execuserobject"#,
        "!undefined execuserobject",
    ),
    (
        "exec out of range",
        r#"1 {} defineuserobject 10 execuserobject"#,
        "!rangecheck execuserobject",
    ),
    (
        "exec out of range, negative",
        r#"1 {} defineuserobject -1 execuserobject"#,
        "!rangecheck execuserobject",
    ),
    (
        "exec underflow",
        r#"execuserobject"#,
        "!stackunderflow execuserobject",
    ),
    (
        "exec error leaves the index",
        r#"{ 1 {} defineuserobject 99 execuserobject } stopped pop count b"#,
        "1",
    ),
    (
        "undefine before any define",
        r#"3 undefineuserobject"#,
        "!undefined undefineuserobject",
    ),
    (
        "undefine out of range",
        r#"1 {} defineuserobject 10 undefineuserobject"#,
        "!rangecheck undefineuserobject",
    ),
    (
        "undefine negative",
        r#"1 {} defineuserobject -1 undefineuserobject"#,
        "!rangecheck undefineuserobject",
    ),
    (
        "undefine index is a string",
        r#"(a) undefineuserobject"#,
        "!typecheck undefineuserobject",
    ),
    (
        "undefine underflow",
        r#"undefineuserobject"#,
        "!stackunderflow undefineuserobject",
    ),
    (
        "exec of an undefined name inside",
        r#"0 {nosuchname} defineuserobject { 0 execuserobject } stopped { $error /errorname get b } if"#,
        "undefined",
    ),
    (
        "exec index is gone when the object fails",
        r#"0 {nosuchname} defineuserobject { 5 0 execuserobject } stopped pop count b"#,
        "1",
    ),
    (
        "UserObjects is not an array",
        r#"userdict /UserObjects 5 put 0 {} defineuserobject"#,
        "!typecheck defineuserobject",
    ),
    (
        "exec when UserObjects is not an array",
        r#"userdict /UserObjects 5 put 0 execuserobject"#,
        "!typecheck execuserobject",
    ),
    (
        "undefine when UserObjects is not an array",
        r#"userdict /UserObjects 5 put 0 undefineuserobject"#,
        "!typecheck undefineuserobject",
    ),
    (
        "a user array of three grows by one past the end",
        r#"userdict /UserObjects 3 array put 5 (x) defineuserobject userdict /UserObjects get length b "#,
        "6",
    ),
    (
        "a user array of three doubles at its length",
        r#"userdict /UserObjects 3 array put 3 (x) defineuserobject userdict /UserObjects get length b "#,
        "6",
    ),
    (
        "an empty user array",
        r#"userdict /UserObjects 0 array put 0 {} defineuserobject"#,
        "!rangecheck defineuserobject",
    ),
    (
        "an empty user array stays replaced",
        r#"userdict /UserObjects 0 array put { 0 {} defineuserobject } stopped pop clear userdict /UserObjects get length b "#,
        "0",
    ),
    (
        "a read-only array refuses a define",
        r#"userdict /UserObjects 3 array readonly put 0 (x) defineuserobject"#,
        "!invalidaccess defineuserobject",
    ),
    (
        "a read-only array refuses a define, negative",
        r#"userdict /UserObjects 3 array readonly put -1 (x) defineuserobject"#,
        "!invalidaccess defineuserobject",
    ),
    (
        "a read-only array grows into a fresh one",
        r#"userdict /UserObjects 3 array readonly put 5 (x) defineuserobject userdict /UserObjects get length b userdict /UserObjects get wcheck b"#,
        "6 true",
    ),
    (
        "a read-only array refuses an undefine",
        r#"userdict /UserObjects 3 array readonly put 0 undefineuserobject"#,
        "!invalidaccess undefineuserobject",
    ),
    (
        "a read-only array refuses an undefine past its end",
        r#"userdict /UserObjects 3 array readonly put 5 undefineuserobject"#,
        "!invalidaccess undefineuserobject",
    ),
    (
        "a read-only array can be executed from",
        r#"userdict /UserObjects [1 2 3] readonly put 1 execuserobject b"#,
        "2",
    ),
    (
        "a user array is run",
        r#"userdict /UserObjects [ {7} {8} ] put 1 execuserobject b"#,
        "8",
    ),
    (
        "only userdict is searched",
        r#"1 dict begin /UserObjects 3 array def 0 (x) defineuserobject userdict /UserObjects known b currentdict /UserObjects get 0 get type b end"#,
        "true nulltype",
    ),
    (
        "an inner dictionary does not hide it",
        r#"<< /UserObjects 3 array >> begin 0 execuserobject end"#,
        "!undefined execuserobject",
    ),
    (
        "restore drops a first array",
        r#"save 5 (x) defineuserobject restore userdict /UserObjects known b"#,
        "false",
    ),
    (
        "restore undoes a write in place",
        r#"5 (x) defineuserobject save 6 (y) defineuserobject 5 (z) defineuserobject restore 5 execuserobject b 6 execuserobject type b"#,
        "x nulltype",
    ),
    (
        "restore undoes an undefine",
        r#"5 (x) defineuserobject save 5 undefineuserobject restore 5 execuserobject b"#,
        "x",
    ),
    (
        "restore undoes a growth",
        r#"5 (x) defineuserobject save 30 (y) defineuserobject userdict /UserObjects get length b restore userdict /UserObjects get length b "#,
        "31 10",
    ),
    (
        "restore keeps the old array's slots",
        r#"5 (x) defineuserobject save 30 (y) defineuserobject restore 5 execuserobject b 30 execuserobject"#,
        "x !rangecheck execuserobject",
    ),
    (
        "exec recursion",
        r#"0 {0 execuserobject 1} defineuserobject 0 execuserobject"#,
        "!execstackoverflow execuserobject",
    ),
];

/// Plain PostScript, valid in pscat and in gs.
const PRELUDE: &str = "/b { dup type /booleantype eq { 8 string cvs } if \
dup type /integertype eq { 8 string cvs } if \
dup type /nametype eq { 32 string cvs } if \
print ( ) print } def \
/cmd { $error /command get 32 string cvs } def\n";

fn program() -> String {
    let mut src = String::from(PRELUDE);
    for (name, body, _) in CASES {
        src.push_str(&format!(
            "({name}: ) print {{ userdict /UserObjects undef {body} }} stopped \
             {{ (!) print $error /errorname get b cmd b }} if clear (\\n) print\n"
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
        .map(|(name, _, out)| format!("{name}: {out}").trim_end().to_string())
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
    let dir = common::scratch_dir("user_objects_gs");
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

/// Run `src`, then return the stdout it printed.
fn printed(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

/// gs 10.07.1: `0 {x} defineuserobject 0 execuserobject` reports
/// `/undefined in --execuserobject--`, an artefact of its operator being a
/// PostScript procedure. The PLRM defines `execuserobject` as
/// `userdict /UserObjects get exch get exec`, so pscat blames what `exec`
/// would: the name that is not defined, or the operator that failed.
#[test]
fn an_error_inside_the_object_blames_the_command_that_failed() {
    let out = printed(
        "/cmd { $error /command get 32 string cvs print ( ) print } def \
         0 { nosuchname } defineuserobject { 0 execuserobject } stopped pop cmd clear \
         0 { pop } defineuserobject { 0 execuserobject } stopped pop cmd clear \
         0 { 1 0 idiv } defineuserobject { 0 execuserobject } stopped pop cmd",
    );
    assert_eq!(out.trim(), "nosuchname pop idiv");
}

/// gs 10.07.1, `0 {0 execuserobject 1} defineuserobject { 0 execuserobject }
/// stopped`: `/execstackoverflow in --execuserobject--` with the index `0`
/// still on the operand stack, because the object never started to run. A
/// failure inside an object that did start has used the index up.
///
/// (Without the trailing `1` the recursion is a tail call and never fills
/// the execution stack, in pscat as for `/f { f } def f` in both
/// interpreters. gs stops it only because its `execuserobject` is a
/// procedure whose return frame is not a tail call; a step limit stops it
/// here: `user_objects_limits.rs`.)
#[test]
fn the_index_stays_when_the_object_never_runs() {
    let out = printed(
        "0 {0 execuserobject 1} defineuserobject { 0 execuserobject } stopped pop \
         $error /errorname get 32 string cvs print ( ) print count 8 string cvs print ( ) print ==",
    );
    assert_eq!(out.trim(), "execstackoverflow 1 0");
}

/// A restore that rolls back the first `defineuserobject` takes the array
/// with it, and the next one starts again with ten slots.
#[test]
fn a_restored_first_define_starts_over() {
    let out = printed(
        "save 40 1 defineuserobject restore 0 1 defineuserobject \
         userdict /UserObjects get length ==",
    );
    assert_eq!(out.trim(), "10");
}
