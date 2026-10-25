//! Writing files (issue #740): `(w)`/`(a) file` inside the write roots,
//! `%stdout`/`%stderr`, `write`/`writestring`/`writehexstring`,
//! `renamefile`, `deletefile`, `filenameforall`, the CLI's
//! `--allow-write`, and `pscat-mcp`, which never writes. The path-escape
//! cases (`..`, symlinks, hard links) are unit tests in
//! `src/write_roots.rs`; the ones here run through PostScript.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::{Interp, PsError};

/// A scratch root, and an interpreter allowed to write in it.
fn setup(tag: &str) -> (PathBuf, Interp) {
    let root = common::scratch_dir(tag);
    let mut it = Interp::with_page(50, 50).expect("page");
    it.set_write_roots(&[&root]).expect("root exists");
    (root, it)
}

/// A PostScript string literal for a path (scratch paths have no
/// parens; Windows backslashes are escaped).
fn ps(p: &Path) -> String {
    let s = p.to_str().unwrap();
    assert!(!s.contains(['(', ')']), "{s}");
    format!("({})", s.replace('\\', "\\\\"))
}

#[test]
fn no_write_roots_by_default() {
    let root = common::scratch_dir("fw-default");
    let mut it = Interp::with_page(50, 50).unwrap();
    assert!(it.write_roots().is_empty());
    let f = ps(&root.join("x.txt"));
    for src in [
        format!("{f} (w) file"),
        format!("{f} (a) file"),
        format!("{f} deletefile"),
        format!("{f} {f} renamefile"),
    ] {
        assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess), "{src}");
    }
    assert!(!root.join("x.txt").exists());
}

#[test]
fn write_then_append_then_read_back() {
    let (root, mut it) = setup("fw-roundtrip");
    let f = ps(&root.join("data.txt"));
    it.run_str(&format!(
        "{f} (w) file dup (hello ) writestring dup <ff00> writehexstring \
         dup 10 write closefile \
         {f} (a) file dup (more) writestring closefile"
    ))
    .unwrap();
    assert_eq!(
        std::fs::read(root.join("data.txt")).unwrap(),
        b"hello ff00\nmore"
    );
    // `(w)` truncates an existing file.
    it.run_str(&format!("{f} (w) file dup (x) writestring closefile"))
        .unwrap();
    assert_eq!(std::fs::read(root.join("data.txt")).unwrap(), b"x");
    // And what was written reads back as a program.
    it.run_str(&format!(
        "{f} (w) file dup (6 7 mul) writestring closefile {f} run"
    ))
    .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "42");
}

#[test]
fn write_low_byte_and_unclosed_file_is_on_disk() {
    let (root, mut it) = setup("fw-byte");
    let f = ps(&root.join("b.bin"));
    // No closefile: writes go straight through, since the CLI exits
    // without dropping the interpreter.
    it.run_str(&format!("{f} (w) file dup 321 write -1 write"))
        .unwrap();
    assert_eq!(std::fs::read(root.join("b.bin")).unwrap(), [65, 255]);
}

#[test]
fn bad_modes_and_names_are_refused() {
    let (root, mut it) = setup("fw-modes");
    let f = ps(&root.join("m.txt"));
    for mode in ["r+", "w+", "a+", "x", ""] {
        assert_eq!(
            it.run_str(&format!("{f} ({mode}) file")),
            Err(PsError::InvalidFileAccess),
            "{mode}"
        );
    }
    let dotdot = ps(&root.join("..").join("escaped.txt"));
    assert_eq!(
        it.run_str(&format!("{dotdot} (w) file")),
        Err(PsError::InvalidFileAccess)
    );
    assert!(!root.parent().unwrap().join("escaped.txt").exists());
    // Not UTF-8: refused rather than converted to some other name.
    let dir = ps(&root);
    let mut src = format!("{}/", &dir[..dir.len() - 1]).into_bytes();
    src.extend_from_slice(b"\xff) (w) file");
    assert_eq!(it.run_source(&src), Err(PsError::InvalidFileAccess));
    assert_eq!(
        it.run_str("(%printer) (w) file"),
        Err(PsError::UndefinedFilename)
    );
}

#[test]
fn reading_an_output_file_or_writing_a_closed_one_is_ioerror() {
    let (root, mut it) = setup("fw-io");
    let f = ps(&root.join("io.txt"));
    assert_eq!(it.run_str(&format!("{f} (w) file read")), Err(PsError::Io));
    assert_eq!(
        it.run_str(&format!("{f} (w) file dup closefile (x) writestring")),
        Err(PsError::Io)
    );
    // An input file can't be written.
    assert_eq!(
        it.run_str(&format!("{f} (r) file (x) writestring")),
        Err(PsError::Io)
    );
    it.run_str(&format!("{f} (w) file dup closefile status"))
        .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "false");
}

#[test]
fn stdout_and_stderr_need_no_root() {
    let mut it = Interp::with_page(50, 50).unwrap();
    it.capture_stdout();
    it.run_str(
        "(a) print (%stdout) (w) file dup (b) writestring dup <0a> writehexstring \
         dup flushfile closefile (c) print flush \
         (%stderr) (w) file dup () writestring closefile",
    )
    .unwrap();
    // In order with `print`, and closing `%stdout` doesn't stop `print`.
    assert_eq!(it.captured_stdout(), b"ab0ac");
}

#[test]
fn renamefile_and_deletefile() {
    let (root, mut it) = setup("fw-rename");
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("a.txt"), b"a").unwrap();
    let (a, b) = (ps(&root.join("a.txt")), ps(&root.join("sub/b.txt")));
    it.run_str(&format!("{a} {b} renamefile")).unwrap();
    assert!(!root.join("a.txt").exists());
    assert_eq!(std::fs::read(root.join("sub/b.txt")).unwrap(), b"a");
    assert_eq!(
        it.run_str(&format!("{a} {b} renamefile")),
        Err(PsError::UndefinedFilename)
    );
    it.run_str(&format!("{b} deletefile")).unwrap();
    assert!(!root.join("sub/b.txt").exists());
    assert_eq!(
        it.run_str(&format!("{b} deletefile")),
        Err(PsError::UndefinedFilename)
    );
    // A directory is not a file to delete.
    assert_eq!(
        it.run_str(&format!("{} deletefile", ps(&root.join("sub")))),
        Err(PsError::InvalidFileAccess)
    );
    // Outside the root, either end.
    let outside = common::scratch_dir("fw-rename-outside");
    std::fs::write(outside.join("o.txt"), b"o").unwrap();
    std::fs::write(root.join("in.txt"), b"i").unwrap();
    let (o, i) = (ps(&outside.join("o.txt")), ps(&root.join("in.txt")));
    let (o2, i2) = (ps(&outside.join("o2.txt")), ps(&root.join("i2.txt")));
    for src in [
        format!("{o} deletefile"),
        format!("{o} {i2} renamefile"),
        format!("{i} {o2} renamefile"),
    ] {
        assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess), "{src}");
    }
    assert!(outside.join("o.txt").exists() && root.join("in.txt").exists());
}

#[cfg(unix)]
#[test]
fn symlink_escapes_through_postscript() {
    use std::os::unix::fs::symlink;
    let (root, mut it) = setup("fw-symlink");
    let outside = common::scratch_dir("fw-symlink-outside");
    std::fs::write(outside.join("victim.txt"), b"keep").unwrap();
    symlink(outside.join("victim.txt"), root.join("link.txt")).unwrap();
    symlink(&outside, root.join("door")).unwrap();
    symlink(outside.join("made.txt"), root.join("dangle")).unwrap();
    for name in ["link.txt", "door/new.txt", "dangle"] {
        for mode in ["w", "a"] {
            let src = format!("{} ({mode}) file", ps(&root.join(name)));
            assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess), "{src}");
        }
    }
    assert_eq!(
        it.run_str(&format!(
            "{} {} renamefile",
            ps(&root.join("door/victim.txt")),
            ps(&root.join("got.txt"))
        )),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(std::fs::read(outside.join("victim.txt")).unwrap(), b"keep");
    assert!(!outside.join("new.txt").exists() && !outside.join("made.txt").exists());
    // Deleting the in-root link removes the link, not its target.
    it.run_str(&format!("{} deletefile", ps(&root.join("link.txt"))))
        .unwrap();
    assert!(outside.join("victim.txt").exists());
}

// Unix only: `\\` is filenameforall's quoting character, so a template
// spelled with Windows separators is not a path pattern.
#[cfg(unix)]
#[test]
fn filenameforall_lists_matches_in_order() {
    let (root, mut it) = setup("fw-forall");
    for f in ["b.txt", "a.txt", "c.dat", "ab.txt"] {
        std::fs::write(root.join(f), b"").unwrap();
    }
    std::fs::create_dir(root.join("d.txt")).unwrap(); // directories are skipped
    let dir = root.display();
    it.run_str(&format!(
        "[ ({dir}/*.txt) {{ dup length string copy }} 200 string filenameforall ]"
    ))
    .unwrap();
    let want = format!("[({dir}/a.txt) ({dir}/ab.txt) ({dir}/b.txt)]");
    assert_eq!(it.pop().unwrap().repr(), want);
    // `?` is one character; `exit` leaves early.
    it.run_str(&format!(
        "0 ({dir}/?.txt) {{ pop 1 add }} 200 string filenameforall \
         0 ({dir}/*) {{ pop 1 add exit }} 200 string filenameforall"
    ))
    .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "1");
    assert_eq!(it.pop().unwrap().repr(), "2");
    // A scratch string too short for a name is a rangecheck.
    assert_eq!(
        it.run_str(&format!("({dir}/*.txt) {{ pop }} 3 string filenameforall")),
        Err(PsError::Rangecheck)
    );
    // Redefining `copy` doesn't change filenameforall.
    it.run_str(&format!(
        "/copy {{ pop pop (bad) }} def [ ({dir}/a.*) {{ length }} 200 string filenameforall ]"
    ))
    .unwrap();
    let len = format!("{dir}/a.txt").len();
    assert_eq!(it.pop().unwrap().repr(), format!("[{len}]"));
}

#[test]
fn filenameforall_reaches_the_embedded_library() {
    // Relative templates list where `run` looks, the embedded lib/ copy
    // included, so this holds in an install with no lib/ on disk.
    let mut it = Interp::with_page(50, 50).unwrap();
    it.run_str("[ (lib/artk?t.ps) { dup length string copy } 100 string filenameforall ]")
        .unwrap();
    assert_eq!(it.pop().unwrap().repr(), "[(lib/artkit.ps)]");
}

#[test]
fn cli_allow_write() {
    let root = common::scratch_dir("fw-cli");
    let f = root.join("out.txt");
    let prog = format!("{} (w) file dup (cli) writestring closefile", ps(&f));
    let pscat = env!("CARGO_BIN_EXE_pscat");

    let refused = Command::new(pscat)
        .args(["--headless", "-e", &prog])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    let said = [refused.stdout, refused.stderr].concat();
    assert!(String::from_utf8_lossy(&said).contains("invalidfileaccess"));
    assert!(!f.exists());

    let other = common::scratch_dir("fw-cli-other");
    let ok = Command::new(pscat)
        .arg("--allow-write")
        .arg(&other)
        .arg("--allow-write")
        .arg(&root)
        .args(["--headless", "-e", &prog])
        .output()
        .unwrap();
    assert!(ok.status.success(), "{ok:?}");
    assert_eq!(std::fs::read(&f).unwrap(), b"cli");

    let missing = Command::new(pscat)
        .args(["--allow-write"])
        .arg(root.join("missing"))
        .args(["--headless", "-e", "1"])
        .output()
        .unwrap();
    assert!(!missing.status.success());

    let help = Command::new(pscat).arg("--help").output().unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("--allow-write DIR"));
}

/// `pscat-mcp` never gives a program write roots: a write into the
/// server's own working directory (its `out_path` root) is refused from
/// both tools that run PostScript, and nothing lands on disk.
#[test]
fn pscat_mcp_never_writes_files() {
    use serde_json::{Value, json};
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    let cwd = common::scratch_dir("fw-mcp");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pscat-mcp"))
        .current_dir(&cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    // The handshake: initialize, then the notification (no reply).
    let init = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                   "clientInfo": {"name": "test", "version": "0"}}});
    writeln!(stdin, "{init}").unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    assert!(line.contains("serverInfo"), "{line}");
    let mut call = |msg: Value| -> Value {
        writeln!(stdin, "{msg}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };

    let abs = cwd.join("abs.txt");
    for (tool, extra) in [
        ("eval_postscript", json!({})),
        ("render_postscript", json!({"width": 20, "height": 20})),
    ] {
        for target in ["(rel.txt)".to_string(), ps(&abs)] {
            let mut args = extra.clone();
            args["source"] = json!(format!("{target} (w) file (x) writestring"));
            let resp = call(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": tool, "arguments": args}}));
            // eval fails as a tool error; render still returns its page
            // with the PostScript error as text.
            let result = &resp["result"];
            let text = result["content"][0]["text"].as_str().unwrap_or_default();
            assert!(
                text.contains("invalidfileaccess"),
                "{tool} {target}: {resp}"
            );
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    assert!(!cwd.join("rel.txt").exists() && !abs.exists());
}
