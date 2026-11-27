//! `statusdict`, `serverdict`, `internaldict`, `scheck`, `GlobalFontDirectory`
//! and `SharedFontDirectory` (issue #1469): six PLRM names that printer and
//! font prologs probe (`/statusdict where`, `1183615869 internaldict`) and
//! that were `undefined`. Every expected value was measured against
//! Ghostscript 10.07.1 (`gs -dNOSAFER -dNODISPLAY`);
//! `the_probes_agree_with_ghostscript` runs both again when `gs` is
//! installed. The `deviation_*` tests are pscat only, where it differs.
//!
//! What pscat defines is deliberately small: `statusdict`, `serverdict`,
//! `GlobalFontDirectory` and `internaldict`'s dictionary are empty
//! dictionaries that accept `def`, with the types, access and VM attributes gs
//! reports. Nothing here runs a font-program hack or models a printer.

mod common;
use common::CommandDeadline as _;

use std::path::Path;
use std::process::Command;

use pscat::Interp;

fn eval(src: &str) -> Result<Vec<String>, String> {
    let mut it = Interp::with_page(50, 50).expect("test page");
    match it.run_str(src) {
        Ok(()) => {
            let mut out = Vec::new();
            while let Ok(o) = it.pop() {
                out.push(o.repr());
            }
            out.reverse();
            Ok(out)
        }
        Err(e) => Err(format!("{e:?}").to_lowercase()),
    }
}

fn stack(src: &str) -> Vec<String> {
    eval(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// `source` -> the operand stack. An error is an entry of its own below:
/// see `ERRORS`. All values measured with gs 10.07.1.
const PROBES: &[(&str, &[&str])] = &[
    // statusdict: an ordinary local dictionary in systemdict.
    ("statusdict type", &["dicttype"]),
    (
        "statusdict rcheck statusdict wcheck statusdict xcheck",
        &["true", "true", "false"],
    ),
    ("statusdict gcheck statusdict scheck", &["false", "false"]),
    (
        "statusdict systemdict eq statusdict userdict eq statusdict globaldict eq",
        &["false", "false", "false"],
    ),
    ("statusdict statusdict eq", &["true"]),
    (
        "systemdict /statusdict known userdict /statusdict known globaldict /statusdict known",
        &["true", "false", "false"],
    ),
    (
        "/statusdict where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    ("/statusdict where { pop true } { false } ifelse", &["true"]),
    ("systemdict /statusdict get statusdict eq", &["true"]),
    ("statusdict /sk1 7 put statusdict /sk1 get", &["7"]),
    (
        "statusdict /sk2 7 put statusdict /sk2 known statusdict /nosuchkey known",
        &["true", "false"],
    ),
    (
        "statusdict begin /sk3 5 def sk3 end /sk3 where",
        &["5", "false"],
    ),
    (
        "countdictstack statusdict begin countdictstack end countdictstack",
        &["3", "4", "3"],
    ),
    ("statusdict begin currentdict statusdict eq end", &["true"]),
    (
        "statusdict /sk4 1 put statusdict /sk4 undef statusdict /sk4 known",
        &["false"],
    ),
    (
        "true setglobal statusdict gcheck statusdict scheck",
        &["false", "false"],
    ),
    // statusdict is local VM: a restore rolls its contents back.
    (
        "statusdict /rk 1 put save statusdict /rk 2 put statusdict /rn 3 put restore statusdict /rk get statusdict /rn known",
        &["1", "false"],
    ),
    // serverdict.
    ("serverdict type", &["dicttype"]),
    (
        "serverdict rcheck serverdict wcheck serverdict xcheck",
        &["true", "true", "false"],
    ),
    ("serverdict gcheck serverdict scheck", &["false", "false"]),
    (
        "serverdict systemdict eq serverdict userdict eq serverdict statusdict eq",
        &["false", "false", "false"],
    ),
    (
        "systemdict /serverdict known userdict /serverdict known",
        &["true", "false"],
    ),
    (
        "/serverdict where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    ("serverdict /vk1 7 put serverdict /vk1 get", &["7"]),
    (
        "serverdict begin /vk2 5 def vk2 end /vk2 where",
        &["5", "false"],
    ),
    (
        "serverdict /rk 1 put save serverdict /rk 2 put restore serverdict /rk get",
        &["1"],
    ),
    // GlobalFontDirectory and SharedFontDirectory: one global dictionary
    // under two names (wcheck: see `deviation_*`).
    ("GlobalFontDirectory type", &["dicttype"]),
    ("SharedFontDirectory type", &["dicttype"]),
    (
        "GlobalFontDirectory rcheck GlobalFontDirectory xcheck",
        &["true", "false"],
    ),
    ("GlobalFontDirectory SharedFontDirectory eq", &["true"]),
    (
        "GlobalFontDirectory gcheck GlobalFontDirectory scheck SharedFontDirectory gcheck SharedFontDirectory scheck",
        &["true", "true", "true", "true"],
    ),
    (
        "GlobalFontDirectory systemdict eq GlobalFontDirectory globaldict eq GlobalFontDirectory userdict eq GlobalFontDirectory FontDirectory eq",
        &["false", "false", "false", "false"],
    ),
    ("GlobalFontDirectory length", &["0"]),
    ("SharedFontDirectory length", &["0"]),
    ("GlobalFontDirectory /NoSuchFont known", &["false"]),
    (
        "systemdict /GlobalFontDirectory known userdict /GlobalFontDirectory known systemdict /SharedFontDirectory known",
        &["true", "false", "true"],
    ),
    (
        "/GlobalFontDirectory where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    (
        "/SharedFontDirectory where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    // scheck is gcheck: the same answers for every kind of object.
    ("1 scheck", &["true"]),
    ("1.5 scheck", &["true"]),
    ("true scheck", &["true"]),
    ("null scheck", &["true"]),
    ("mark scheck", &["true"]),
    ("/n scheck", &["true"]),
    ("/add load scheck", &["true"]),
    ("save scheck", &["true"]),
    ("(s) scheck", &["false"]),
    ("[1] scheck", &["false"]),
    ("{1} scheck", &["false"]),
    ("<</a 1>> scheck", &["false"]),
    ("1 dict scheck", &["false"]),
    ("userdict scheck", &["false"]),
    ("systemdict scheck", &["true"]),
    ("globaldict scheck", &["true"]),
    ("/scheck load type", &["operatortype"]),
    (
        "/scheck where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    // internaldict: one writable local dictionary, found by the key.
    ("1183615869 internaldict type", &["dicttype"]),
    (
        "1183615869 internaldict rcheck 1183615869 internaldict wcheck 1183615869 internaldict xcheck",
        &["true", "true", "false"],
    ),
    (
        "1183615869 internaldict gcheck 1183615869 internaldict scheck",
        &["false", "false"],
    ),
    ("true setglobal 1183615869 internaldict gcheck", &["false"]),
    (
        "1183615869 internaldict systemdict eq 1183615869 internaldict userdict eq 1183615869 internaldict globaldict eq 1183615869 internaldict statusdict eq 1183615869 internaldict serverdict eq",
        &["false", "false", "false", "false", "false"],
    ),
    (
        "1183615869 internaldict 1183615869 internaldict eq",
        &["true"],
    ),
    ("1183615869.0 internaldict type", &["dicttype"]),
    (
        "1183615869 internaldict /ik 5 put 1183615869 internaldict /ik get",
        &["5"],
    ),
    (
        "1183615869 internaldict begin /ik2 3 def ik2 end 1183615869 internaldict /ik2 get",
        &["3", "3"],
    ),
    (
        "save 1183615869 internaldict /rk 2 put restore 1183615869 internaldict /rk known",
        &["false"],
    ),
    ("/internaldict load type", &["operatortype"]),
    (
        "/internaldict where { systemdict eq } { false } ifelse",
        &["true"],
    ),
    (
        "systemdict /internaldict known userdict /internaldict known",
        &["true", "false"],
    ),
    // What a failed internaldict leaves: the key stays on the stack.
    ("[ { 5 internaldict } stopped ]", &["[5 true]"]),
    ("[ { /a internaldict } stopped ]", &["[/a true]"]),
    ("[ { clear internaldict } stopped ]", &["[true]"]),
];

/// `source` -> the error name it raises, all measured with gs 10.07.1.
/// Only the key 1183615869 (an integer) opens it; everything else, whatever
/// its type, is `invalidaccess`, not `typecheck`.
const ERRORS: &[(&str, &str)] = &[
    ("clear scheck", "stackunderflow"),
    ("clear internaldict", "stackunderflow"),
    ("0 internaldict", "invalidaccess"),
    ("1 internaldict", "invalidaccess"),
    ("-1 internaldict", "invalidaccess"),
    ("1183615868 internaldict", "invalidaccess"),
    ("1183615870 internaldict", "invalidaccess"),
    ("4294967296 internaldict", "invalidaccess"),
    ("1183615872 internaldict", "invalidaccess"),
    ("/a internaldict", "invalidaccess"),
    ("(a) internaldict", "invalidaccess"),
    ("null internaldict", "invalidaccess"),
    ("[1183615869] internaldict", "invalidaccess"),
    ("true internaldict", "invalidaccess"),
    (
        "1183615869 internaldict pop 1183615868 internaldict",
        "invalidaccess",
    ),
];

#[test]
fn the_probes_give_the_values_gs_gives() {
    for (src, want) in PROBES {
        assert_eq!(stack(src), s(want), "{src}");
    }
}

#[test]
fn the_errors_are_the_ones_gs_raises() {
    for (src, want) in ERRORS {
        assert_eq!(eval(src), Err(want.to_string()), "{src}");
    }
}

/// pscat only. gs's `statusdict` holds 87 printer-control entries
/// (`product`, `setpapertray`, `lettertray`, ...), its `serverdict` six
/// (`exitserver`, `execjob`, ...), and `1183615869 internaldict` four. pscat
/// defines none of them: a probe finds the dictionary, `known` says no to a
/// printer name, and `exitserver` stays #1270's.
#[test]
fn deviation_the_dictionaries_start_empty() {
    for src in [
        "statusdict length",
        "serverdict length",
        "1183615869 internaldict length",
    ] {
        assert_eq!(stack(src), s(&["0"]), "{src}");
    }
    assert_eq!(
        stack(
            "statusdict /setpapertray known statusdict /product known statusdict /lettertray known \
             serverdict /exitserver known"
        ),
        s(&["false", "false", "false", "false"])
    );
}

/// pscat only. The key check gs makes compares reals after rounding to
/// single precision, so `1183615870.0` and `1183615869.5` open it there. Here
/// only the integer or a real exactly equal to it does.
#[test]
fn deviation_a_real_near_the_key_does_not_open_internaldict() {
    assert_eq!(
        eval("1183615870.0 internaldict"),
        Err("invalidaccess".into())
    );
    assert_eq!(
        eval("1183615869.5 internaldict"),
        Err("invalidaccess".into())
    );
    assert_eq!(
        stack("1183615869.0 internaldict 1183615869 internaldict eq"),
        s(&["true"])
    );
}

/// pscat only. In gs `GlobalFontDirectory` is read-only (`wcheck` false, a
/// `put` or `def` is `invalidaccess`) and fills as fonts load into global
/// VM (`/Helvetica findfont pop GlobalFontDirectory length` is 2). Here it is
/// an empty dictionary that accepts `def` (as pscat's `FontDirectory` does),
/// and fonts live in `FontDirectory` only.
#[test]
fn deviation_global_font_directory_is_writable_and_stays_empty() {
    assert_eq!(
        stack("GlobalFontDirectory wcheck SharedFontDirectory wcheck"),
        s(&["true", "true"])
    );
    assert_eq!(
        stack(
            "/Helvetica findfont pop GlobalFontDirectory length GlobalFontDirectory /Helvetica known"
        ),
        s(&["0", "false"])
    );
    assert_eq!(
        stack("GlobalFontDirectory /gk 1 put SharedFontDirectory /gk get"),
        s(&["1"])
    );
}

/// The new dictionaries are never on the dictionary stack, and name lookup
/// does not find what a program put in them until it `begin`s one.
#[test]
fn the_dictionaries_are_not_on_the_dictionary_stack() {
    assert_eq!(stack("countdictstack"), s(&["3"]));
    assert_eq!(
        eval("statusdict /only_here 1 put only_here"),
        Err("undefined".into())
    );
}

/// A dictionary that holds itself must not outlive the interpreter (the same
/// cycle `globaldict` is guarded against): `Drop for Interp` empties them.
/// The weak-handle check is the unit test in `src/interp.rs`.
#[test]
fn the_dictionaries_do_not_leak_when_they_hold_themselves() {
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str(
        "statusdict /me statusdict put serverdict /me serverdict put \
         GlobalFontDirectory /me GlobalFontDirectory put \
         1183615869 internaldict dup /me exch put",
    )
    .unwrap();
    drop(it);
}

fn harness(cases: &[String]) -> String {
    let mut src = String::from(
        "/T { mark exch stopped { $error /errorname get == clear } { ] == } ifelse } def\n",
    );
    for c in cases {
        src.push_str(&format!("{{ false setglobal {c} }} T\n"));
    }
    src
}

fn run_pscat(dir: &Path, program: &str) -> String {
    std::fs::write(dir.join("harness.ps"), program).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--headless"])
        .arg("harness.ps")
        .current_dir(dir)
        .output_deadline()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run_gs(dir: &Path, program: &str) -> String {
    std::fs::write(dir.join("harness-gs.ps"), program).unwrap();
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "harness-gs.ps"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_probes_agree_with_ghostscript() {
    if !common::gs_available() {
        return;
    }
    let mut cases: Vec<String> = PROBES.iter().map(|(src, _)| src.to_string()).collect();
    cases.extend(ERRORS.iter().map(|(src, _)| src.to_string()));
    let dir = common::scratch_dir("statusdict-gs");
    let program = harness(&cases);
    let (ours, theirs) = (run_pscat(&dir, &program), run_gs(&dir, &program));
    for (i, (a, b)) in ours.lines().zip(theirs.lines()).enumerate() {
        assert_eq!(a, b, "{}", cases[i]);
    }
    assert_eq!(ours.lines().count(), theirs.lines().count());
}
