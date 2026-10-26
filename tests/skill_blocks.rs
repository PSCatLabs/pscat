//! The plugin skills' PostScript code blocks run on a bare install
//! (issues #727, #728, #729).
//!
//! The skills (`plugin/skills/**/*.md`, `skills/` is a symlink to it) are
//! what an agent reads before it writes PostScript, and plugin users have
//! no font packs: a starter sketch that asks for `/Palatino-Italic` failed
//! on every bare install until #729. So every fenced block whose info
//! string starts with `postscript` is run through the real `pscat` binary
//! with nothing installed, and the test fails on any of:
//!
//! - a non-zero exit, or a `%%[ Error` line;
//! - the font-pack stand-in note on stderr (`font ... is in the ... font
//!   pack, which is not installed`). Since #728 a pack face no longer
//!   raises an error, so without this check a block that only works with
//!   `pscat fonts install` would pass.
//!
//! ## Marking a block as partial
//!
//! A block that cannot run on its own (a fragment that needs the caller's
//! path or dict, a signature listing, a file that exists only in the
//! repository) says so in its fence: write ```` ```postscript partial ````.
//! It is shown as a normal PostScript block and is not run. A fence with
//! no language, or any other (`sh`, `text`), is never run, so a block that
//! is not PostScript needs no mark. Prefer making a block runnable to
//! marking it: the point is that what the skills show works.
//!
//! ## What "bare" means here
//!
//! `PSCAT_HOME`, `HOME`, `XDG_DATA_HOME` and `APPDATA` point at an empty
//! scratch directory, the working directory is another empty one, and
//! `PSCAT_NO_CHECKOUT=1` hides the checkout's `lib/` and `fonts/catalog/`
//! (a debug-assertion build, which is how tests run, otherwise finds
//! them through the crate root). `lib/` still resolves from the copy
//! compiled into the binary, as it does for a released `pscat`. A
//! sentinel asks for an extras-pack face first and fails the test if it
//! resolves, so a leaky environment cannot make the guard pass without
//! checking anything.

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

/// One runnable block: where it came from and its source.
#[derive(Debug, PartialEq, Eq)]
struct Block {
    file: String,
    line: usize,
    code: String,
}

/// The runnable `postscript` blocks of one Markdown file. A fence is a
/// line starting (after indentation) with three backticks; its info
/// string's first word is the language, the second may be `partial`.
/// Fences nest inside list items, so a block's own indentation is
/// stripped from its lines.
fn blocks_of(file: &str, text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    // (indent, runnable, first line, code so far) while inside a fence.
    let mut open: Option<(usize, bool, usize, String)> = None;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        match &mut open {
            None => {
                if let Some(info) = trimmed.strip_prefix("```") {
                    let mut words = info.split_whitespace();
                    let lang = words.next().unwrap_or("");
                    let partial = words.any(|w| w == "partial");
                    let runnable = lang.eq_ignore_ascii_case("postscript") && !partial;
                    open = Some((indent, runnable, i + 2, String::new()));
                }
            }
            Some((fence_indent, runnable, first, code)) => {
                if trimmed.starts_with("```") {
                    if *runnable {
                        out.push(Block {
                            file: file.to_string(),
                            line: *first,
                            code: std::mem::take(code),
                        });
                    }
                    open = None;
                } else {
                    let strip = indent.min(*fence_indent);
                    code.push_str(&line[strip..]);
                    code.push('\n');
                }
            }
        }
    }
    assert!(open.is_none(), "{file}: a code fence is never closed");
    out
}

fn markdown_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            markdown_under(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

struct Run {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Run `code` through `pscat --headless -` with no checkout and no font
/// packs. `tag` only names the scratch directories.
fn run_bare(tag: &str, code: &str) -> Run {
    let home = common::scratch_dir(&format!("skill-home-{tag}"));
    let work = common::scratch_dir(&format!("skill-work-{tag}"));
    let mut child = Command::new(PSCAT)
        .args(["--headless", "-"])
        .current_dir(&work)
        .env("PSCAT_HOME", &home)
        .env("HOME", home.join("user"))
        .env("XDG_DATA_HOME", home.join("user/data"))
        .env("APPDATA", home.join("user/appdata"))
        .env("PSCAT_NO_CHECKOUT", "1")
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn pscat");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(code.as_bytes())
        .expect("write program");
    let out = child.wait_with_output().expect("wait for pscat");
    Run {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("plugin/skills")
}

#[test]
fn the_environment_has_no_font_packs() {
    // Bangers is in the extras pack. Bare, it must stand in as Helvetica
    // and say so; if it resolves to the real face, the checkout (or an
    // installed pack) leaked in and the skill-block test below proves
    // nothing. Palatino and Symbol are compiled in (issue #757), so they
    // resolve, silently, even here.
    let r = run_bare(
        "sentinel",
        "/Bangers findfont /FontName get ==\n\
         /Palatino-Roman findfont /FontName get ==\n/Symbol findfont /FontName get ==\n",
    );
    assert!(r.ok, "{}", r.stderr);
    assert_eq!(
        r.stdout.split_whitespace().collect::<Vec<_>>(),
        [
            "/Helvetica",
            "/texgyrepagella-regular",
            "/StandardSymbolsPS"
        ],
        "stderr: {}",
        r.stderr
    );
    assert_eq!(r.stderr.trim().lines().count(), 1, "{}", r.stderr);
    assert!(
        r.stderr.contains("/Bangers is in the extras font pack"),
        "{}",
        r.stderr
    );
}

#[test]
fn every_skill_postscript_block_runs_on_a_bare_install() {
    let mut files = Vec::new();
    markdown_under(&skills_dir(), &mut files);
    assert!(files.len() >= 5, "found skills: {files:?}");
    let mut blocks = Vec::new();
    for f in &files {
        let rel = f
            .strip_prefix(env!("CARGO_MANIFEST_DIR"))
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(f).unwrap();
        blocks.extend(blocks_of(&rel, &text));
    }
    // The starter sketch and the pscat skill's examples must be in the run,
    // so renaming a fence cannot quietly empty the guard.
    for must in [
        "plugin/skills/psart/SKILL.md",
        "plugin/skills/pscat/SKILL.md",
    ] {
        assert!(
            blocks.iter().any(|b| b.file == must),
            "{must} has no runnable postscript block"
        );
    }
    let mut failures = Vec::new();
    for (n, b) in blocks.iter().enumerate() {
        let r = run_bare(&format!("block{n}"), &b.code);
        let mut why = Vec::new();
        if !r.ok {
            why.push("exit status is not 0");
        }
        if r.stdout.contains("%%[ Error") || r.stderr.contains("%%[ Error") {
            why.push("PostScript error");
        }
        if r.stderr.contains("font pack, which is not installed") {
            why.push(
                "asks for a face from a font pack (use a built-in: Helvetica, Times, Courier)",
            );
        }
        if !why.is_empty() {
            failures.push(format!(
                "{}:{}: {}\n--- stderr\n{}--- program\n{}",
                b.file,
                b.line,
                why.join("; "),
                r.stderr,
                b.code
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} skill blocks fail on a bare install (mark a fragment ```postscript partial):\n\n{}",
        failures.len(),
        blocks.len(),
        failures.join("\n")
    );
}

#[test]
fn block_extraction_follows_the_fence_rules() {
    let md = "\
intro
```postscript
1 2 add pop
```
```sh
echo not postscript
```
```postscript partial
fragment needing the caller
```
```
untagged
```
- item

  ```PostScript
  indented 1
    deeper
  ```
";
    let got = blocks_of("x.md", md);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!((got[0].line, got[0].code.as_str()), (3, "1 2 add pop\n"));
    assert_eq!(got[1].code, "indented 1\n  deeper\n");
}
