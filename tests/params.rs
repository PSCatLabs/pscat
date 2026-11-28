//! Parameter, cache, job-control, overprint, smoothness, shared-VM and
//! device-parameter operators (issue #1270): `setuserparams`,
//! `currentuserparams`, `setsystemparams`, `currentsystemparams`,
//! `setdevparams`, `currentdevparams`, `cachestatus`, `setcachelimit`,
//! `setcacheparams`, `currentcacheparams`, `setucacheparams`,
//! `ucachestatus`, `setvmthreshold`, `vmreclaim`, `startjob`, `exitserver`
//! (in `serverdict`), `setshared`, `currentshared`, `shareddict`,
//! `setoverprint`, `currentoverprint`, `setsmoothness`, `currentsmoothness`.
//!
//! `tests/data/params_cases.txt` holds one case per line: a PostScript body,
//! a tab, and what Ghostscript 10.07.1 (`gs -q -dNODISPLAY -dNOSAFER -dBATCH
//! -dNOPAUSE`) printed for it under the harness below: the operand stack left
//! (bottom first) and the error name, or `null`. A case that errors shows the
//! operands the error left on the stack. `the_cases_give_what_gs_gives` runs
//! each case in a fresh interpreter and compares; `the_cases_agree_with_
//! ghostscript` re-measures the file with `gs` when it is installed.
//!
//! `DEVIATIONS` are the cases where pscat answers differently on purpose (the
//! sandbox refuses what gs allows, a gs-only parameter is not modelled); each
//! carries gs's line and pscat's, and `notes/1270-parameter-operators.md`
//! says why.

mod common;

use std::collections::BTreeMap;

use pscat::Interp;

/// Wraps a case: run it under `stopped`, print the operands it left and the
/// error name, undo its `save`, and leave the stack empty for the next case.
/// A case that leaves a dictionary or a `begin` behind cannot leak into the
/// next one (`cleardictstack`), nor can the parameters, which `restore`
/// rolls back with the rest of VM.
const HARNESS: &str = "/T { save exch stopped { $error /errorname get } { null } ifelse\n \
     count 1 sub array astore == cleardictstack restore clear } def\n";

fn cases() -> Vec<(String, String)> {
    let path = format!("{}/tests/data/params_cases.txt", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let (src, want) = l.split_once('\t').expect("a tab between body and result");
            (src.to_string(), want.to_string())
        })
        .collect()
}

/// `(body, what gs prints, what pscat prints)`.
const DEVIATIONS: &[(&str, &str, &str)] = &[];

/// The line pscat prints for `src`, in a fresh interpreter.
fn pscat_line(src: &str) -> String {
    let mut it = Interp::with_page(50, 50).expect("test page");
    it.capture_stdout();
    let program = format!("{HARNESS}{{ {src} }} T\n");
    it.run_str(&program)
        .unwrap_or_else(|e| panic!("{src}: the harness itself failed: {e:?}"));
    String::from_utf8_lossy(it.captured_stdout())
        .trim_end()
        .to_string()
}

#[test]
fn the_cases_give_what_gs_gives() {
    let deviations: BTreeMap<&str, &str> = DEVIATIONS.iter().map(|(s, _, p)| (*s, *p)).collect();
    let mut failures = Vec::new();
    for (src, gs) in cases() {
        let want = deviations.get(src.as_str()).copied().unwrap_or(&gs);
        let got = pscat_line(&src);
        if got != want {
            failures.push(format!("{src}\n    want {want}\n    got  {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_deviation_names_a_case_and_differs_from_gs() {
    let cases: BTreeMap<String, String> = cases().into_iter().collect();
    for (src, gs, pscat) in DEVIATIONS {
        assert_eq!(
            cases.get(*src).map(String::as_str),
            Some(*gs),
            "{src}: not a case, or gs's line is not the file's"
        );
        assert_ne!(gs, pscat, "{src}: not a deviation");
    }
}

/// A case whose body touches a system parameter is measured in its own gs
/// process: `save` does not roll those back, and a password set in one case
/// would refuse the next.
fn needs_own_process(src: &str) -> bool {
    ["systemparams", "setcacheparams", "MaxFontCache", "startjob", "exitserver"]
        .iter()
        .any(|k| src.contains(k))
}

fn gs_lines(program: &str) -> Vec<String> {
    use common::CommandDeadline as _;
    let dir = common::scratch_dir("params-gs");
    std::fs::write(dir.join("p.ps"), program).unwrap();
    let out = std::process::Command::new("gs")
        .args(["-q", "-dNODISPLAY", "-dNOSAFER", "-dBATCH", "-dNOPAUSE", "p.ps"])
        .current_dir(&dir)
        .output_deadline()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn the_cases_agree_with_ghostscript() {
    if !common::gs_available() {
        return;
    }
    let all = cases();
    let (own, shared): (Vec<_>, Vec<_>) = all.iter().partition(|(s, _)| needs_own_process(s));
    let mut program = String::from(HARNESS);
    for (src, _) in &shared {
        program.push_str(&format!("{{ {src} }} T\n"));
    }
    let lines = gs_lines(&program);
    // One result line per case, or the comparison below would pair the wrong ones.
    assert_eq!(lines.len(), shared.len(), "gs printed a line per case");
    for ((src, want), got) in shared.iter().zip(&lines) {
        assert_eq!(got, want, "{src}");
    }
    for (src, want) in own {
        let got = gs_lines(&format!("{HARNESS}{{ {src} }} T\n"));
        assert_eq!(got, [want.clone()], "{src}");
    }
}
