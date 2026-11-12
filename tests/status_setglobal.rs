//! `string status` (issue #952) and `setglobal`/`currentglobal` (issue
//! #1146). Every expected value was measured against Ghostscript 10.07.1
//! (`gs -dNOSAFER -dNODISPLAY`); `*_agree_with_ghostscript` runs both again
//! when `gs` is installed. The read-root cases are the security property
//! `pscat-mcp` relies on: `status` must answer a name outside the roots
//! exactly as it answers a missing file.

mod common;
use common::CommandDeadline as _;

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::{Interp, Limits, PsError};

/// Run `src` in a fresh interpreter and give the operand stack, bottom
/// first, as `repr`s, or the PostScript error's name.
fn eval(it: &mut Interp, src: &str) -> Result<Vec<String>, String> {
    match it.run_str(src) {
        Ok(()) => {
            let mut out = Vec::new();
            while let Ok(o) = it.pop() {
                out.push(o.repr());
            }
            out.reverse();
            Ok(out)
        }
        Err(e) => {
            let _ = it.run_str("clear");
            Err(error_name(&e))
        }
    }
}

fn error_name(e: &PsError) -> String {
    format!("{e:?}").to_lowercase()
}

fn fresh() -> Interp {
    Interp::with_page(50, 50).expect("test page")
}

fn stack(src: &str) -> Vec<String> {
    eval(&mut fresh(), src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn error(src: &str) -> String {
    eval(&mut fresh(), src).expect_err(src)
}

fn s(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// ---- setglobal / currentglobal ----

#[test]
fn the_mode_starts_local_and_reads_back_what_was_set() {
    assert_eq!(stack("currentglobal"), s(&["false"]));
    assert_eq!(stack("true setglobal currentglobal"), s(&["true"]));
    assert_eq!(
        stack("true setglobal false setglobal currentglobal"),
        s(&["false"])
    );
}

#[test]
fn the_bracketing_idiom_round_trips() {
    // How prologs use it: remember, go global, define, put it back.
    let src = "currentglobal true setglobal /Res 42 def setglobal currentglobal Res";
    assert_eq!(stack(src), s(&["false", "42"]));
    let src = "true setglobal currentglobal false setglobal setglobal currentglobal";
    assert_eq!(stack(src), s(&["true"]));
}

#[test]
fn setglobal_and_currentglobal_errors() {
    assert_eq!(error("clear setglobal"), "stackunderflow");
    assert_eq!(error("1 setglobal"), "typecheck");
    assert_eq!(error("(x) setglobal"), "typecheck");
    assert_eq!(error("null setglobal"), "typecheck");
    // A failed setglobal leaves the mode alone.
    assert_eq!(
        stack("true setglobal { (x) setglobal } stopped pop clear currentglobal"),
        s(&["true"])
    );
}

/// `source` -> what `currentglobal` reads afterwards, all measured with gs.
const SAVE_RESTORE: &[(&str, &str)] = &[
    // The mode after a restore is global only if it is global now and was
    // at every save the restore discards: gs never raises it, and does not
    // reset a local mode to the saved global one as the PLRM reads.
    ("true setglobal save false setglobal restore", "false"),
    ("true setglobal save true setglobal restore", "true"),
    ("true setglobal save restore", "true"),
    ("false setglobal save true setglobal restore", "false"),
    ("save true setglobal restore", "false"),
    ("false setglobal save restore", "false"),
    // Nested: restoring the outer save over an inner one saved while local.
    (
        "true setglobal /s save def false setglobal save pop true setglobal s restore",
        "false",
    ),
    (
        "true setglobal /s save def true setglobal save pop true setglobal s restore",
        "true",
    ),
    (
        "true setglobal /s save def true setglobal save pop false setglobal s restore",
        "false",
    ),
    (
        "false setglobal /s save def true setglobal save pop true setglobal s restore",
        "false",
    ),
    // Only save/restore moves it: gsave, grestore, begin, end, exec don't.
    ("true setglobal gsave false setglobal grestore", "false"),
    ("false setglobal gsave true setglobal grestore", "true"),
    ("true setglobal 1 dict begin end", "true"),
    ("{ true setglobal } exec", "true"),
];

#[test]
fn restore_treats_the_mode_as_gs_does() {
    for (src, want) in SAVE_RESTORE {
        assert_eq!(stack(&format!("{src} currentglobal")), s(&[want]), "{src}");
    }
}

#[test]
fn a_failed_restore_leaves_the_mode_and_vmstatus_alone() {
    // `invalidrestore` (a stale save) changes nothing.
    assert_eq!(
        stack(
            "/s save def s restore true setglobal \
             { s restore } stopped pop clear currentglobal"
        ),
        s(&["true"])
    );
    // The mode does not change what vmstatus reports.
    assert_eq!(
        stack("vmstatus pop pop true setglobal vmstatus pop pop eq"),
        s(&["true"])
    );
}

// ---- string status ----

struct Fixture {
    dir: PathBuf,
}

/// `dir` holds `f` (1500 bytes), `empty`, `sub/` and, on unix, `link -> f`
/// and `dangling -> nope`.
fn fixture(tag: &str) -> Fixture {
    let dir = common::scratch_dir(tag);
    std::fs::write(dir.join("f"), vec![b'x'; 1500]).unwrap();
    std::fs::write(dir.join("empty"), b"").unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("f", dir.join("link")).unwrap();
        std::os::unix::fs::symlink("nope", dir.join("dangling")).unwrap();
    }
    Fixture { dir }
}

fn abs(p: &Path) -> String {
    common::ps_path(p)
}

/// The four numbers `status` should give for a file on disk.
#[cfg(unix)]
fn expected_fields(p: &Path) -> Vec<String> {
    use std::os::unix::fs::MetadataExt;
    let m = std::fs::metadata(p).unwrap();
    vec![
        m.blocks().to_string(),
        m.size().to_string(),
        m.mtime().to_string(),
        m.ctime().to_string(),
        "true".to_string(),
    ]
}

#[cfg(unix)]
#[test]
fn an_existing_file_gives_pages_bytes_referenced_created_true() {
    let fx = fixture("status-fields");
    let mut it = fresh();
    for name in ["f", "empty"] {
        let path = fx.dir.join(name);
        let got = eval(&mut it, &format!("({}) status", abs(&path))).unwrap();
        assert_eq!(got, expected_fields(&path), "{name}");
    }
    let got = eval(&mut it, &format!("({}) status", abs(&fx.dir.join("f")))).unwrap();
    assert_eq!(got[1], "1500", "bytes");
    // An empty file uses no blocks, a 1500 byte one at least 3 (512-byte).
    let empty = eval(&mut it, &format!("({}) status", abs(&fx.dir.join("empty")))).unwrap();
    assert_eq!(empty[..2], s(&["0", "0"])[..]);
    assert!(got[0].parse::<i64>().unwrap() >= 3, "{got:?}");
    // referenced is the modification time: set it, read it back.
    let when = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_577_865_600);
    std::fs::File::options()
        .write(true)
        .open(fx.dir.join("f"))
        .unwrap()
        .set_modified(when)
        .unwrap();
    let got = eval(&mut it, &format!("({}) status", abs(&fx.dir.join("f")))).unwrap();
    assert_eq!(got[2], "1577865600", "referenced");
}

#[cfg(unix)]
#[test]
fn links_follow_and_dangling_links_are_missing() {
    let fx = fixture("status-links");
    let mut it = fresh();
    let link = fx.dir.join("link");
    assert_eq!(
        eval(&mut it, &format!("({}) status", abs(&link))).unwrap(),
        expected_fields(&fx.dir.join("f"))
    );
    assert_eq!(
        eval(
            &mut it,
            &format!("({}) status", abs(&fx.dir.join("dangling")))
        )
        .unwrap(),
        s(&["false"])
    );
}

#[test]
fn missing_directory_empty_and_device_names() {
    let fx = fixture("status-misc");
    let mut it = fresh();
    for name in [
        abs(&fx.dir.join("nope")),
        abs(&fx.dir.join("no/such/dir/x")),
        // A directory is not something `run` could read; gs says true.
        abs(&fx.dir.join("sub")),
        String::new(),
        "%stdout".to_string(),
        "%stdin".to_string(),
    ] {
        assert_eq!(
            eval(&mut it, &format!("({name}) status")).unwrap(),
            s(&["false"]),
            "{name:?}"
        );
    }
    // A name with a NUL byte can't be a file (gs cuts it at the NUL).
    assert_eq!(eval(&mut it, "(f\\000x) status").unwrap(), s(&["false"]));
}

/// A file `file`/`run` cannot open is not something to probe-then-run.
/// (gs reports `true` for it: it only stats.)
#[cfg(unix)]
#[test]
fn an_unreadable_file_is_missing() {
    use std::os::unix::fs::PermissionsExt;
    let fx = fixture("status-unreadable");
    let path = fx.dir.join("secret");
    std::fs::write(&path, "x").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0)).unwrap();
    if std::fs::File::open(&path).is_ok() {
        eprintln!("skipping: this user can open a mode 000 file (root)");
        return;
    }
    let mut it = fresh();
    assert_eq!(
        eval(&mut it, &format!("({}) status", abs(&path))).unwrap(),
        s(&["false"])
    );
    assert_eq!(
        eval(&mut it, &format!("({}) (r) file", abs(&path))),
        Err("undefinedfilename".to_string())
    );
}

#[test]
fn a_file_object_still_answers_open_or_closed() {
    let fx = fixture("status-fileobj");
    let name = abs(&fx.dir.join("f"));
    assert_eq!(stack(&format!("({name}) (r) file status")), s(&["true"]));
    assert_eq!(
        stack(&format!("({name}) (r) file dup closefile status")),
        s(&["false"])
    );
}

#[test]
fn status_errors() {
    assert_eq!(error("clear status"), "stackunderflow");
    for src in ["42 status", "null status", "/f status", "[(f)] status"] {
        assert_eq!(error(src), "typecheck", "{src}");
    }
}

#[test]
fn the_acceptance_probe_runs_the_file_only_when_it_exists() {
    let fx = fixture("status-probe");
    std::fs::write(fx.dir.join("lib.ps"), "/LoadedMark 7 def").unwrap();
    let probe = |name: &str| {
        format!(
            "/LoadedMark 0 def ({}) status {{ pop pop pop pop ({}) run }} if LoadedMark",
            abs(&fx.dir.join(name)),
            abs(&fx.dir.join(name)),
        )
    };
    assert_eq!(stack(&probe("lib.ps")), s(&["7"]));
    assert_eq!(stack(&probe("absent.ps")), s(&["0"]));
}

/// `lib/` is served from a copy embedded in the binary, which `run` finds
/// when no file on disk does (here: reads confined to no directory at all);
/// `status` agrees with it, with no times to report.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn the_embedded_lib_is_found_like_run_finds_it() {
    let mut it = fresh();
    it.set_read_roots::<&str>(&[]).unwrap();
    let got = eval(&mut it, "(lib/artkit.ps) status").unwrap();
    assert_eq!(got.len(), 5, "{got:?}");
    assert_eq!(got[4], "true");
    assert_eq!(
        got[2..4],
        s(&["0", "0"])[..],
        "no times for an embedded copy"
    );
    let bytes: usize = got[1].parse().unwrap();
    assert!(bytes > 1000, "{got:?}");
    assert_eq!(got[0], bytes.div_ceil(512).to_string());
    assert_eq!(
        eval(&mut it, "(lib/no-such-library.ps) status").unwrap(),
        s(&["false"])
    );
    // `run` finds the same name.
    it.run_str("(lib/artkit.ps) run").unwrap();
}

// ---- read roots ----

/// `base/root` (holding `in.ps`, `sub/deep.ps`), `base/outside` (holding
/// `away.ps`), and on unix the links that lead out of the root.
struct Tree {
    root: PathBuf,
    outside: PathBuf,
}

fn tree(tag: &str) -> Tree {
    let base = common::scratch_dir(tag);
    let root = base.join("root");
    let outside = base.join("outside");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(root.join("in.ps"), "6 7 mul").unwrap();
    std::fs::write(root.join("sub/deep.ps"), "1").unwrap();
    std::fs::write(outside.join("away.ps"), "(leaked)").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(outside.join("away.ps"), root.join("out-existing")).unwrap();
        symlink(outside.join("missing.ps"), root.join("out-missing")).unwrap();
        symlink(&outside, root.join("door")).unwrap();
        symlink("in.ps", root.join("inside-link")).unwrap();
    }
    Tree { root, outside }
}

/// Names a confined program must see as missing, existing or not.
fn outside_names(t: &Tree) -> Vec<String> {
    let mut names = vec![
        abs(&t.outside.join("away.ps")),
        abs(&t.outside.join("missing.ps")),
        abs(&t.outside),
        format!("{}/../outside/away.ps", abs(&t.root)),
        format!("{}/../outside/missing.ps", abs(&t.root)),
        "../outside/away.ps".to_string(),
        "/etc/hosts".to_string(),
        "/dev/null".to_string(),
    ];
    #[cfg(unix)]
    names.extend(
        [
            "out-existing",
            "out-missing",
            "door/away.ps",
            "door/missing.ps",
        ]
        .iter()
        .map(|n| abs(&t.root.join(n))),
    );
    names
}

/// The cases a confined interpreter must get right.
fn check_confined(it: &mut Interp, t: &Tree) {
    // Inside: the file, a file in a subdirectory, and a link that stays in.
    for (name, bytes) in [("in.ps", "7"), ("sub/deep.ps", "1")] {
        let got = eval(it, &format!("({}) status", abs(&t.root.join(name)))).unwrap();
        assert_eq!(got.len(), 5, "{name}: {got:?}");
        assert_eq!(got[1], bytes, "{name}");
        assert_eq!(got[4], "true");
    }
    #[cfg(unix)]
    {
        let got = eval(
            it,
            &format!("({}) status", abs(&t.root.join("inside-link"))),
        )
        .unwrap();
        assert_eq!(got[1], "7", "{got:?}");
    }
    // Missing inside the root: false, as before.
    assert_eq!(
        eval(it, &format!("({}) status", abs(&t.root.join("nope.ps")))).unwrap(),
        s(&["false"])
    );
    // Outside: `false`, with no error, for what exists and what doesn't
    // alike, whichever way the name gets out. `file` is where the two
    // would differ; `status` must not let a program tell.
    for name in outside_names(t) {
        let src = format!("({name}) status");
        assert_eq!(eval(it, &src).unwrap(), s(&["false"]), "{src}");
    }
    // The embedded lib/ is no disk read.
    assert_eq!(eval(it, "(lib/artkit.ps) status").unwrap().len(), 5);
    // Nothing is left on the stack or changed by the probes, and the probe
    // gives away nothing `file` would not: `file` still refuses these.
    assert_eq!(
        eval(
            it,
            &format!("({}) (r) file", abs(&t.outside.join("away.ps")))
        ),
        Err("invalidfileaccess".to_string())
    );
}

#[test]
fn a_name_outside_the_read_roots_is_missing() {
    let t = tree("status-roots");
    let mut it = fresh();
    it.set_read_roots(&[&t.root]).unwrap();
    check_confined(&mut it, &t);
}

#[test]
fn the_same_under_execution_limits() {
    let t = tree("status-roots-limits");
    let mut it = fresh();
    let mut limits = Limits::none();
    limits.max_steps = Some(1_000_000_000);
    it.set_limits(limits).unwrap();
    it.set_read_roots(&[&t.root]).unwrap();
    check_confined(&mut it, &t);
}

#[test]
fn without_roots_the_cli_default_reads_anywhere() {
    // Reads are open by default (the CLI): outside is not special then.
    let t = tree("status-open");
    let mut it = fresh();
    let got = eval(
        &mut it,
        &format!("({}) status", abs(&t.outside.join("away.ps"))),
    )
    .unwrap();
    assert_eq!(got.len(), 5, "{got:?}");
    assert_eq!(got[1], "8");
    // And an empty root list confines to the embedded lib/ alone.
    it.set_read_roots::<&str>(&[]).unwrap();
    for name in [abs(&t.root.join("in.ps")), abs(&t.outside.join("away.ps"))] {
        assert_eq!(
            eval(&mut it, &format!("({name}) status")).unwrap(),
            s(&["false"]),
            "{name}"
        );
    }
    assert_eq!(eval(&mut it, "(lib/artkit.ps) status").unwrap().len(), 5);
}

#[test]
fn the_confined_cli_answers_the_same() {
    let t = tree("status-cli");
    let program = format!(
        "mark ({}) status ] ==\nmark ({}) status ] ==\nmark ({}) status ] ==\n",
        abs(&t.root.join("in.ps")),
        abs(&t.outside.join("away.ps")),
        abs(&t.outside.join("missing.ps")),
    );
    let file = t.root.join("probe.ps");
    std::fs::write(&file, program).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--headless", "--read-root"])
        .arg(&t.root)
        .arg(&file)
        .output_deadline()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        3,
        "{text} {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        lines[0].starts_with("[") && lines[0].ends_with(" true]"),
        "{lines:?}"
    );
    assert_eq!(lines[1], "[false]");
    assert_eq!(lines[2], "[false]");
}

// ---- Ghostscript ----

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A program that prints, for each case, `[ stack ]` or `/errorname`.
fn harness(cases: &[String]) -> String {
    let mut src = String::from(
        "/T { mark exch stopped { $error /errorname get == clear } { ] == } ifelse } def\n",
    );
    for c in cases {
        src.push_str(&format!("{{ {c} }} T\n"));
    }
    src
}

fn run_pscat(dir: &Path, program: &str) -> String {
    let file = dir.join("harness.ps");
    std::fs::write(&file, program).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--headless"])
        .arg("harness.ps")
        .current_dir(dir)
        .output_deadline()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run_gs(dir: &Path, program: &str) -> String {
    let file = dir.join("harness-gs.ps");
    std::fs::write(&file, program).unwrap();
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "harness-gs.ps"])
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn setglobal_and_currentglobal_agree_with_ghostscript() {
    if !gs_available() {
        eprintln!("skipping: gs not installed");
        return;
    }
    let mut cases: Vec<String> = [
        "currentglobal",
        "true setglobal currentglobal",
        "false setglobal currentglobal",
        "clear setglobal",
        "1 setglobal",
        "(x) setglobal",
        "null setglobal",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    cases.extend(
        SAVE_RESTORE
            .iter()
            .map(|(src, _)| format!("{src} currentglobal")),
    );
    let dir = common::scratch_dir("setglobal-gs");
    let program = harness(&cases);
    let (ours, theirs) = (run_pscat(&dir, &program), run_gs(&dir, &program));
    for (i, (a, b)) in ours.lines().zip(theirs.lines()).enumerate() {
        assert_eq!(a, b, "{}", cases[i]);
    }
    assert_eq!(ours.lines().count(), theirs.lines().count());
}

#[cfg(unix)]
#[test]
fn status_agrees_with_ghostscript() {
    if !gs_available() {
        eprintln!("skipping: gs not installed");
        return;
    }
    let fx = fixture("status-gs");
    let cases: Vec<String> = [
        "(f) status",
        "(./f) status",
        "(empty) status",
        "(link) status",
        "(dangling) status",
        "(nope) status",
        "(sub/nope) status",
        "(%stdout) status",
        "() status",
        "(f) (r) file status",
        "(f) (r) file dup closefile status",
        "42 status",
        "null status",
        "/f status",
        "clear status",
    ]
    .iter()
    .map(|c| c.to_string())
    .collect();
    let program = harness(&cases);
    let (ours, theirs) = (run_pscat(&fx.dir, &program), run_gs(&fx.dir, &program));
    // `pages` is st_blocks in both; the times come from the same stat.
    for (i, (a, b)) in ours.lines().zip(theirs.lines()).enumerate() {
        assert_eq!(a, b, "{}", cases[i]);
    }
    assert_eq!(ours.lines().count(), cases.len());
    assert_eq!(theirs.lines().count(), cases.len());
}
