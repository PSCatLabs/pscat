//! Read roots (issue #763): `Interp::set_read_roots` confines `(r) file`,
//! `run`, `readreals` and `filenameforall`, with or without execution
//! limits, and the CLI's `--read-root`/`--confine-reads`. Reads stay open by
//! default. The path-escape cases are also unit tests in
//! `src/read_roots.rs`; `pscat-mcp`'s defaults are in `tests/mcp.rs`.

mod common;
use common::CommandDeadline as _;

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::{Interp, Limits, PsError};

/// `base/root` (holding `in.ps`) and `base/outside` (holding `away.ps`).
fn tree(tag: &str) -> (PathBuf, PathBuf) {
    let base = common::scratch_dir(tag);
    let root = base.join("root");
    let outside = base.join("outside");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("in.ps"), "6 7 mul").unwrap();
    std::fs::write(outside.join("away.ps"), "(leaked)").unwrap();
    (root, outside)
}

fn ps(p: &Path) -> String {
    format!("({})", common::ps_path(p))
}

/// The cases every confined interpreter must get right, with or without
/// execution limits.
fn check_confined(it: &mut Interp, root: &Path, outside: &Path) {
    it.run_str(&format!("{} run", ps(&root.join("in.ps"))))
        .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "42");
    it.run_str(&format!("{} (r) file pop", ps(&root.join("in.ps"))))
        .unwrap();
    let away = ps(&outside.join("away.ps"));
    let up = format!("({}/../outside/away.ps)", common::ps_path(root));
    for src in [
        format!("{away} run"),
        format!("{away} (r) file"),
        format!("{away} 1 readreals"),
        format!("{up} run"),
        format!("{} (r) file", ps(&outside.join("missing.ps"))),
    ] {
        assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess), "{src}");
    }
    #[cfg(unix)]
    assert_eq!(
        it.run_str("(/dev/zero) (r) file"),
        Err(PsError::InvalidFileAccess)
    );
    // A missing file inside is still undefinedfilename.
    assert_eq!(
        it.run_str(&format!("{} run", ps(&root.join("nope.ps")))),
        Err(PsError::UndefinedFilename)
    );
    // The embedded lib/ is no disk read.
    it.run_str("(lib/artkit.ps) run").unwrap();
    // filenameforall: inside lists, an absolute template outside yields
    // nothing (no error), and `..` yields nothing.
    let count = |it: &mut Interp, template: String| {
        it.run_str(&format!(
            "0 ({template}) {{ pop 1 add }} 400 string filenameforall"
        ))
        .unwrap();
        it.pop().unwrap().repr()
    };
    assert_eq!(count(it, format!("{}/*", common::ps_path(root))), "1");
    assert_eq!(count(it, format!("{}/*", common::ps_path(outside))), "0");
    assert_eq!(
        count(it, format!("{}/../outside/*", common::ps_path(root))),
        "0"
    );
}

#[test]
fn reads_are_open_by_default() {
    let (_root, outside) = tree("rr-default");
    let mut it = Interp::with_page(50, 50).unwrap();
    assert!(it.read_roots().is_none());
    it.run_str(&format!("{} run", ps(&outside.join("away.ps"))))
        .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "(leaked)");
}

#[test]
fn confined_without_limits() {
    let (root, outside) = tree("rr-plain");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.set_read_roots(&[&root]).unwrap();
    assert_eq!(it.read_roots().map(<[PathBuf]>::len), Some(1));
    check_confined(&mut it, &root, &outside);
    // And lifted again.
    it.clear_read_roots();
    it.run_str(&format!("{} run", ps(&outside.join("away.ps"))))
        .unwrap();
}

#[test]
fn confined_under_execution_limits() {
    // The limited read path (issue #702) is a separate branch.
    let (root, outside) = tree("rr-limited");
    let mut it = Interp::with_page(50, 50).unwrap();
    let mut limits = Limits::none();
    limits.max_steps = Some(1_000_000_000);
    it.set_limits(limits).unwrap();
    it.set_read_roots(&[&root]).unwrap();
    check_confined(&mut it, &root, &outside);
}

#[cfg(unix)]
#[test]
fn symlinks_out_of_a_root_are_refused() {
    let (root, outside) = tree("rr-links");
    std::os::unix::fs::symlink(outside.join("away.ps"), root.join("leak.ps")).unwrap();
    std::os::unix::fs::symlink(&outside, root.join("door")).unwrap();
    std::os::unix::fs::symlink("/dev/zero", root.join("zero")).unwrap();
    let mut it = Interp::with_page(50, 50).unwrap();
    it.set_read_roots(&[&root]).unwrap();
    for f in ["leak.ps", "door/away.ps", "zero"] {
        let src = format!("{} (r) file", ps(&root.join(f)));
        assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess), "{src}");
    }
    it.run_str(&format!(
        "0 ({}/*) {{ pop 1 add }} 400 string filenameforall",
        common::ps_path(&root)
    ))
    .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "1", "only in.ps is listed");
}

#[test]
fn no_roots_leaves_only_the_embedded_lib() {
    let (root, _) = tree("rr-none");
    let mut it = Interp::with_page(50, 50).unwrap();
    it.set_read_roots::<&str>(&[]).unwrap();
    assert_eq!(
        it.run_str(&format!("{} run", ps(&root.join("in.ps")))),
        Err(PsError::InvalidFileAccess)
    );
    it.run_str("(lib/artkit.ps) run").unwrap();
}

#[test]
fn a_root_must_exist() {
    let mut it = Interp::new();
    let missing = common::scratch_dir("rr-missing").join("nope");
    assert!(it.set_read_roots(&[&missing]).is_err());
    assert!(it.read_roots().is_none(), "nothing changed");
}

fn pscat(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(args)
        .output_deadline()
        .expect("run pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn cli_reads_stay_open_and_read_root_confines_them() {
    let (root, outside) = tree("rr-cli");
    let away = format!("{} run =", ps(&outside.join("away.ps")));
    let (ok, stdout, _) = pscat(&["--headless", "-e", &away]);
    assert!(ok && stdout.contains("leaked"), "the CLI is unchanged");
    let root_s = root.to_str().unwrap();
    let (ok, _, stderr) = pscat(&["--read-root", root_s, "--headless", "-e", &away]);
    assert!(!ok && stderr.contains("invalidfileaccess"), "{stderr}");
    let inside = format!("{} run =", ps(&root.join("in.ps")));
    let (ok, stdout, _) = pscat(&["--read-root", root_s, "--headless", "-e", &inside]);
    assert!(ok && stdout.contains("42"), "{stdout}");
    let (ok, _, stderr) = pscat(&["--confine-reads", "--headless", "-e", &inside]);
    assert!(!ok && stderr.contains("invalidfileaccess"), "{stderr}");
    let (ok, _, stderr) = pscat(&["--read-root", "/no/such/dir", "--headless", "-e", "1"]);
    assert!(!ok && stderr.contains("--read-root"), "{stderr}");
}

#[test]
fn handwrite_takes_the_read_root_flags() {
    let dir = common::scratch_dir("rr-handwrite");
    let out = dir.join("note.png");
    let dir_s = dir.to_str().unwrap();
    let (ok, _, stderr) = pscat(&[
        "handwrite",
        "-o",
        out.to_str().unwrap(),
        "--confine-reads",
        "--read-root",
        dir_s,
        "confined",
    ]);
    assert!(ok, "{stderr}");
    assert!(out.exists());
}
