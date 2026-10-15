//! `scripts/identity_check.sh --base-bin/--head-bin` (issue #335): the base
//! tree must render with the base binary and the head tree with the head
//! binary, and `--control` must prove the binaries differ.
//!
//! The "binaries" are shell wrappers around the real pscat that log the
//! working directory they ran in, so the tests can check the binary-to-tree
//! pairing and not only the exit codes. The base tree is a temp directory
//! holding a copy of one tracked example that loads no library.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod common;

const EXAMPLE: &str = "examples/sierpinski.ps";
/// A second tracked example that loads no library: the one the wrapper fails on.
const OTHER: &str = "examples/koch_snowflake.ps";

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A scratch directory under the system temp dir (the script refuses a
/// scratch directory inside either tree, and `target/` is inside the head tree).
struct Scratch(PathBuf);
impl Scratch {
    fn new(tag: &str) -> Scratch {
        // `scratch_dir` already steps out of target/ when TMPDIR points into it.
        let dir = common::scratch_dir(&format!("identity_check_{tag}"));
        Scratch(dir.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A wrapper that records its cwd in `log`, then runs the real pscat. With
/// `shrink`, the dpi is halved for any file whose name contains `shrink`
/// (so the PNG differs), which stands in for "this binary behaves differently".
/// With `fail = Some((pattern, rc, only_in))`, a file whose name contains
/// `pattern` still renders (so a canvas is written, as pscat does after an
/// error) but the wrapper then prints a message naming the output path and
/// exits `rc`; `only_in` limits that to runs whose cwd ends with it.
fn wrapper(path: &Path, log: &Path, shrink: &str, fail: Option<(&str, i32, &str)>) {
    let (fpat, frc, fin) = fail.unwrap_or(("", 0, ""));
    let script = format!(
        "#!/bin/bash\n\
         echo \"$PWD\" >> '{log}'\n\
         args=(\"$@\")\n\
         if [ -n '{shrink}' ]; then\n\
           case \"$*\" in *'{shrink}'*)\n\
             for i in \"${{!args[@]}}\"; do [ \"${{args[$i]}}\" = 72 ] && args[$i]=36; done ;;\n\
           esac\n\
         fi\n\
         if [ -n '{fpat}' ]; then\n\
           case \"$*\" in *'{fpat}'*)\n\
             case \"$PWD\" in *'{fin}') '{real}' \"${{args[@]}}\" >/dev/null 2>&1\n\
               echo \"%%[ Error: undefinedfilename; wrote $*]%%\"\n\
               exit {frc} ;;\n\
             esac ;;\n\
           esac\n\
         fi\n\
         exec '{real}' \"${{args[@]}}\"\n",
        log = log.display(),
        shrink = shrink,
        fpat = fpat,
        fin = fin,
        frc = frc,
        real = env!("CARGO_BIN_EXE_pscat"),
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

struct Setup {
    s: Scratch,
    base_tree: PathBuf,
    base_bin: PathBuf,
    head_bin: PathBuf,
    base_log: PathBuf,
    head_log: PathBuf,
    files: PathBuf,
    control: PathBuf,
}

fn setup(tag: &str, base_shrinks: &str) -> Setup {
    setup_with(tag, base_shrinks, None, None)
}

/// `base_fail` / `head_fail`: the failure the base / head binary adds (see `wrapper`).
fn setup_with(
    tag: &str,
    base_shrinks: &str,
    base_fail: Option<(&str, i32, &str)>,
    head_fail: Option<(&str, i32, &str)>,
) -> Setup {
    let s = Scratch::new(tag);
    let base_tree = s.0.join("base_tree");
    fs::create_dir_all(base_tree.join("examples")).unwrap();
    fs::copy(root().join(EXAMPLE), base_tree.join(EXAMPLE)).unwrap();
    fs::copy(root().join(OTHER), base_tree.join(OTHER)).unwrap();
    let (base_bin, head_bin) = (s.0.join("base_pscat"), s.0.join("head_pscat"));
    let (base_log, head_log) = (s.0.join("base.log"), s.0.join("head.log"));
    wrapper(&base_bin, &base_log, base_shrinks, base_fail);
    wrapper(&head_bin, &head_log, "", head_fail);
    let files = s.0.join("files.txt");
    fs::write(&files, format!("{EXAMPLE}\n{OTHER}\n")).unwrap();
    let control = s.0.join("control.ps");
    fs::write(&control, "0 0 100 100 rectfill showpage\n").unwrap();
    Setup {
        s,
        base_tree,
        base_bin,
        head_bin,
        base_log,
        head_log,
        files,
        control,
    }
}

fn run(su: &Setup, extra: &[&str]) -> Output {
    Command::new(root().join("scripts/identity_check.sh"))
        .arg(&su.base_tree)
        .args(["--files"])
        .arg(&su.files)
        .arg("--out")
        .arg(su.s.0.join("out"))
        .args(extra)
        .output()
        .expect("run identity_check.sh")
}

fn text(o: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

#[test]
fn each_tree_renders_with_its_own_binary_and_the_control_differs() {
    // The base binary shrinks the control's dpi, the head binary does not.
    let su = setup("pair", "control.ps");
    let o = run(
        &su,
        &[
            "--base-bin",
            su.base_bin.to_str().unwrap(),
            "--head-bin",
            su.head_bin.to_str().unwrap(),
            "--control",
            su.control.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    let t = text(&o);
    assert!(t.contains("identical  examples/sierpinski.ps"), "{t}");
    assert!(
        t.contains("control    differs between the trees, as it must"),
        "{t}"
    );
    // The pairing: two examples + control per tree, each in its own tree only.
    let base_cwds = lines(&su.base_log);
    let head_cwds = lines(&su.head_log);
    assert_eq!(base_cwds.len(), 3, "{base_cwds:?}");
    assert_eq!(head_cwds.len(), 3, "{head_cwds:?}");
    let base_tree = su.base_tree.canonicalize().unwrap().display().to_string();
    let head_tree = root().canonicalize().unwrap().display().to_string();
    assert!(base_cwds.iter().all(|c| *c == base_tree), "{base_cwds:?}");
    assert!(head_cwds.iter().all(|c| *c == head_tree), "{head_cwds:?}");
}

#[test]
fn a_control_that_renders_identically_exits_2() {
    // Two wrappers around the same pscat: nothing differs, the control cannot.
    let su = setup("ctl", "");
    let o = run(
        &su,
        &[
            "--base-bin",
            su.base_bin.to_str().unwrap(),
            "--head-bin",
            su.head_bin.to_str().unwrap(),
            "--control",
            su.control.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(2), "{}", text(&o));
    assert!(text(&o).contains("CONTROL FAILED"), "{}", text(&o));
    assert!(
        text(&o).contains("only the head binary has"),
        "{}",
        text(&o)
    );
}

#[test]
fn a_binary_that_renders_a_file_differently_exits_1() {
    let su = setup("diff", "sierpinski");
    let o = run(
        &su,
        &[
            "--base-bin",
            su.base_bin.to_str().unwrap(),
            "--head-bin",
            su.head_bin.to_str().unwrap(),
        ],
    );
    assert_eq!(o.status.code(), Some(1), "{}", text(&o));
    assert!(
        text(&o).contains("DIFFERS    examples/sierpinski.ps"),
        "{}",
        text(&o)
    );
    assert!(text(&o).contains("no --control given"), "{}", text(&o));
}

#[test]
fn the_flags_are_validated() {
    let su = setup("usage", "");
    let (b, h) = (su.base_bin.to_str().unwrap(), su.head_bin.to_str().unwrap());
    let one = run(&su, &["--base-bin", b]);
    assert_eq!(one.status.code(), Some(64), "{}", text(&one));
    assert!(text(&one).contains("go together"), "{}", text(&one));
    let both_kinds = run(&su, &[h, "--base-bin", b, "--head-bin", h]);
    assert_eq!(both_kinds.status.code(), Some(64), "{}", text(&both_kinds));
    assert!(
        text(&both_kinds).contains("alternatives"),
        "{}",
        text(&both_kinds)
    );
    let missing = run(&su, &["--base-bin", "/nonexistent/pscat", "--head-bin", h]);
    assert_eq!(missing.status.code(), Some(64), "{}", text(&missing));
    // Nothing was rendered for any of them.
    assert!(lines(&su.base_log).is_empty() && lines(&su.head_log).is_empty());
}

#[test]
fn the_positional_binary_still_renders_both_trees() {
    let su = setup("single", "");
    let o = run(&su, &[su.head_bin.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(0), "{}", text(&o));
    assert!(text(&o).contains("identical  examples/sierpinski.ps"));
    // One binary served both trees, each from its own working directory.
    assert!(lines(&su.base_log).is_empty());
    let cwds = lines(&su.head_log);
    let base_tree = su.base_tree.canonicalize().unwrap().display().to_string();
    let head_tree = root().canonicalize().unwrap().display().to_string();
    assert_eq!(cwds.len(), 4, "{cwds:?}");
    assert!(
        cwds.contains(&base_tree) && cwds.contains(&head_tree),
        "{cwds:?}"
    );
}

fn both_bins<'a>(su: &'a Setup) -> Vec<&'a str> {
    vec![
        "--base-bin",
        su.base_bin.to_str().unwrap(),
        "--head-bin",
        su.head_bin.to_str().unwrap(),
    ]
}

#[test]
fn a_file_that_fails_identically_in_both_trees_is_skipped_with_a_warning() {
    // #504: tw_proto.ps fails the same way in both trees; that is the file's
    // own problem, not a difference, so the run must not exit 1 for it.
    let fail = Some(("koch_snowflake", 3, ""));
    let su = setup_with("skip", "", fail, fail);
    let mut args = both_bins(&su);
    args.extend(["--control", su.control.to_str().unwrap()]);
    // The control needs a difference: make the base shrink it.
    wrapper(&su.base_bin, &su.base_log, "control.ps", fail);
    let o = run(&su, &args);
    let t = text(&o);
    assert_eq!(o.status.code(), Some(0), "{t}");
    assert!(t.contains("SKIPPED    examples/koch_snowflake.ps"), "{t}");
    assert!(t.contains("identical  examples/sierpinski.ps"), "{t}");
    assert!(
        t.contains("WARNING: 1 file(s) failed identically in both trees and were NOT compared"),
        "{t}"
    );
    assert!(t.contains("1 skipped"), "{t}");
    assert!(!t.contains("ERROR"), "{t}");
}

#[test]
fn a_real_difference_still_exits_1_next_to_a_skipped_file() {
    // The control for the skip: the same skipped file, plus a page that really differs.
    let fail = Some(("koch_snowflake", 3, ""));
    let su = setup_with("skipdiff", "sierpinski", fail, fail);
    let o = run(&su, &both_bins(&su));
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(t.contains("DIFFERS    examples/sierpinski.ps"), "{t}");
    assert!(t.contains("SKIPPED    examples/koch_snowflake.ps"), "{t}");
}

#[test]
fn a_failure_in_one_tree_only_is_still_an_error() {
    let su = setup_with("headonly", "", None, Some(("koch_snowflake", 3, "")));
    let o = run(&su, &both_bins(&su));
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(
        t.contains("ERROR      examples/koch_snowflake.ps (pscat exited 3 in the branch tree"),
        "{t}"
    );
    assert!(!t.contains("SKIPPED"), "{t}");
    let su = setup_with("baseonly", "", Some(("koch_snowflake", 3, "")), None);
    let o = run(&su, &both_bins(&su));
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(
        t.contains("ERROR      examples/koch_snowflake.ps (pscat exited 3 in the base tree"),
        "{t}"
    );
}

#[test]
fn failures_that_differ_between_the_trees_are_still_errors() {
    // Same file failing in both trees, but with another exit status: not "identical".
    let su = setup_with(
        "otherrc",
        "",
        Some(("koch_snowflake", 3, "")),
        Some(("koch_snowflake", 4, "")),
    );
    let o = run(&su, &both_bins(&su));
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(t.contains("ERROR      examples/koch_snowflake.ps"), "{t}");
    assert!(!t.contains("SKIPPED"), "{t}");
}

#[test]
fn a_run_where_every_file_is_skipped_compared_nothing_and_exits_1() {
    let fail = Some(("examples/", 3, ""));
    let su = setup_with("allskip", "", fail, fail);
    let o = run(&su, &both_bins(&su));
    let t = text(&o);
    assert_eq!(o.status.code(), Some(1), "{t}");
    assert!(
        t.contains("every file was skipped, so nothing was compared"),
        "{t}"
    );
}
