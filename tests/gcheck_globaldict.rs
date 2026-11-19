//! `gcheck` and `globaldict` (issue #1159), the Level 2 VM probes next to
//! `setglobal`/`currentglobal` (#1146, `tests/status_setglobal.rs`). Every
//! expected value was measured against Ghostscript 10.07.1
//! (`gs -dNOSAFER -dNODISPLAY`); `the_probes_agree_with_ghostscript` runs both
//! again when `gs` is installed (the `deviation_*` tests are pscat only).
//!
//! pscat has one VM, so the model is: `globaldict` and `systemdict` are the
//! composites that are in global VM, every other string, array, dictionary
//! and gstate is local. The one place Ghostscript differs is a composite made
//! while `currentglobal` is true (`deviation_*` below).

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

/// `source` -> the operand stack, all measured with gs.
const PROBES: &[(&str, &[&str])] = &[
    // Simple objects are always global.
    ("1 gcheck", &["true"]),
    ("1.5 gcheck", &["true"]),
    ("true gcheck", &["true"]),
    ("null gcheck", &["true"]),
    ("mark gcheck", &["true"]),
    ("/n gcheck", &["true"]),
    ("/add load gcheck", &["true"]),
    ("save gcheck", &["true"]),
    ("(%stdout) (w) file gcheck", &["true"]),
    // Composites made in the (default) local mode are local.
    ("(s) gcheck", &["false"]),
    ("[1] gcheck", &["false"]),
    ("{1} gcheck", &["false"]),
    ("<</a 1>> gcheck", &["false"]),
    ("1 dict gcheck", &["false"]),
    ("matrix gcheck", &["false"]),
    ("gstate gcheck", &["false"]),
    ("userdict gcheck", &["false"]),
    ("errordict gcheck", &["false"]),
    // The dictionaries that stand in for global VM.
    ("systemdict gcheck", &["true"]),
    ("globaldict gcheck", &["true"]),
    // globaldict.
    ("globaldict type", &["dicttype"]),
    ("globaldict length", &["0"]),
    (
        "globaldict rcheck globaldict wcheck globaldict xcheck",
        &["true", "true", "false"],
    ),
    ("globaldict userdict eq", &["false"]),
    ("globaldict systemdict eq", &["false"]),
    ("globaldict globaldict eq", &["true"]),
    ("globaldict /k 7 put globaldict /k get", &["7"]),
    // On the dictionary stack: bare names find its entries, and it counts.
    ("globaldict /gx 5 put gx", &["5"]),
    ("globaldict begin /Pg 11 def end Pg", &["11"]),
    ("countdictstack", &["3"]),
    ("currentdict globaldict eq", &["false"]),
    ("globaldict begin currentdict globaldict eq end", &["true"]),
    (
        "globaldict /gx 5 put /gx where { globaldict eq } { false } ifelse",
        &["true"],
    ),
    ("/ux 1 def globaldict /ux 2 put ux", &["1"]),
    ("cleardictstack countdictstack", &["3"]),
    (
        "{ end end end end } stopped { $error /errorname get /dictstackunderflow eq } { false } ifelse",
        &["true"],
    ),
    (
        "globaldict /k 7 put globaldict /k undef globaldict /k known",
        &["false"],
    ),
    (
        "globaldict begin /x 3 def x end globaldict /x get",
        &["3", "3"],
    ),
    ("/globaldict where { pop true } { false } ifelse", &["true"]),
    ("/gcheck where { pop true } { false } ifelse", &["true"]),
    // The bracketing idiom prologs use.
    (
        "currentglobal true setglobal globaldict begin /Res 42 def end setglobal currentglobal globaldict /Res get",
        &["false", "42"],
    ),
    // Global VM is not rolled back by restore, whatever mode the save was in.
    (
        "save globaldict /sv 1 put restore globaldict /sv known",
        &["true"],
    ),
    (
        "true setglobal save globaldict /sv 1 put restore globaldict /sv known",
        &["true"],
    ),
    // ...but userdict is.
    (
        "save userdict /sv 1 put restore userdict /sv known",
        &["false"],
    ),
    // Errors.
    ("clear gcheck", &["stackunderflow"]),
];

#[test]
fn the_probes_give_the_values_gs_gives() {
    for (src, want) in PROBES {
        if want == &["stackunderflow"] {
            assert_eq!(eval(src), Err("stackunderflow".to_string()), "{src}");
        } else {
            assert_eq!(stack(src), s(want), "{src}");
        }
    }
}

/// pscat only (gs: `invalidaccess` for a local array stored in globaldict).
/// globaldict's own slots outlive a restore; an array it holds is still
/// rolled back like any array.
#[test]
fn deviation_globaldict_is_exempt_from_restore_but_what_it_holds_is_not() {
    assert_eq!(
        stack(
            "globaldict /a [1 2] put save globaldict /a get 0 99 put restore globaldict /a get 0 get"
        ),
        s(&["1"])
    );
}

#[test]
fn globaldict_does_not_leak_when_it_holds_itself() {
    // `Drop for Interp` empties it; a self-reference must not hang or panic.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str("globaldict /me globaldict put").unwrap();
    drop(it);
}

/// Where pscat differs from gs, pinned so the difference stays deliberate:
/// one VM means a composite made in global mode is still reported local.
#[test]
fn deviation_a_composite_made_in_global_mode_is_reported_local() {
    // gs says true for every one of these.
    for src in [
        "true setglobal 3 array false setglobal gcheck",
        "true setglobal 3 dict false setglobal gcheck",
        "true setglobal 3 string false setglobal gcheck",
        "true setglobal (abc) false setglobal gcheck",
    ] {
        assert_eq!(stack(src), s(&["false"]), "{src}");
    }
}

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
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
    if !gs_available() {
        eprintln!("skipping: gs not installed");
        return;
    }
    let cases: Vec<String> = PROBES.iter().map(|(src, _)| src.to_string()).collect();
    let dir = common::scratch_dir("gcheck-gs");
    let program = harness(&cases);
    let (ours, theirs) = (run_pscat(&dir, &program), run_gs(&dir, &program));
    for (i, (a, b)) in ours.lines().zip(theirs.lines()).enumerate() {
        assert_eq!(a, b, "{}", cases[i]);
    }
    assert_eq!(ours.lines().count(), theirs.lines().count());
}
