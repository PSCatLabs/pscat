//! The Learn page's examples (#252): every program in `site/learn/` runs in
//! the command-line pscat without an error and prints exactly its `.out`
//! file (nothing, when there is none), and every one that draws puts ink
//! on the page. When Ghostscript is installed (gs-test style), each must
//! print the same in Ghostscript too: the page teaches PostScript, not
//! pscat, so its outputs must be what any interpreter gives.
//!
//! The same files are checked in the browser build by
//! `scripts/check_learn_examples.mjs` (run by `build_site.sh`), and placed
//! into `site/learn.html` by `scripts/site_examples.py`.

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::Interp;

fn examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("site/learn");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("site/learn")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "ps"))
        .collect();
    v.sort();
    assert!(v.len() >= 10, "expected the Learn examples in {dir:?}");
    v
}

fn expected_output(ps: &Path) -> String {
    std::fs::read_to_string(ps.with_extension("out")).unwrap_or_default()
}

/// (w, h) from `%%BoundingBox: 0 0 W H`, for the examples that draw.
fn page_size(source: &str) -> Option<(u32, u32)> {
    source.lines().find_map(|l| {
        let rest = l.strip_prefix("%%BoundingBox:")?;
        let n: Vec<u32> = rest
            .split_whitespace()
            .map(|t| t.parse().ok())
            .collect::<Option<_>>()?;
        (n.len() == 4 && n[0] == 0 && n[1] == 0).then(|| (n[2], n[3]))
    })
}

#[test]
fn every_example_prints_what_its_out_file_says() {
    for ps in examples() {
        let run = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .arg("--headless")
            .arg(&ps)
            .output()
            .expect("run pscat");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            run.status.success() && stderr.is_empty(),
            "{ps:?} failed: {stderr}"
        );
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected_output(&ps),
            "{ps:?} printed something other than its .out file"
        );
    }
}

#[test]
fn every_drawing_example_draws() {
    for ps in examples() {
        let source = std::fs::read_to_string(&ps).expect("read");
        let Some((w, h)) = page_size(&source) else {
            assert!(
                ps.with_extension("out").exists(),
                "{ps:?} neither draws (no %%BoundingBox) nor prints (no .out)"
            );
            continue;
        };
        let mut it = Interp::with_page(w, h).expect("page");
        it.run_str(&source)
            .unwrap_or_else(|e| panic!("{ps:?}: {e}"));
        let px = it.gfx().pixmap.data();
        let inked = px
            .chunks(4)
            .filter(|p| p[0] < 230 || p[1] < 230 || p[2] < 230)
            .count();
        assert!(inked > 50, "{ps:?} left the page blank");
    }
}

#[test]
fn examples_agree_with_ghostscript() {
    if Command::new("gs").arg("--version").output().is_err() {
        eprintln!("skipping: gs not installed");
        return;
    }
    for ps in examples() {
        let run = Command::new("gs")
            .args(["-q", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE", "-dSAFER"])
            .arg(&ps)
            .output()
            .expect("run gs");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            expected_output(&ps),
            "{ps:?}: Ghostscript prints something else"
        );
    }
}

/// Every example is on the page, and every marker on the page names one.
#[test]
fn learn_page_shows_every_example() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let page = std::fs::read_to_string(root.join("site/learn.html")).expect("learn.html");
    let mut marked: Vec<String> = page
        .split("<!--@example ")
        .skip(1)
        .map(|s| {
            let marker = &s[..s.find("-->").expect("unclosed marker")];
            marker.split_whitespace().next().unwrap_or("").to_string()
        })
        .collect();
    marked.sort();
    let files: Vec<String> = examples()
        .iter()
        .map(|p| p.file_stem().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(marked, files, "site/learn.html markers vs site/learn/*.ps");
}

/// What the browser build relies on: with capture on, printing goes to a
/// buffer, capped so a runaway loop cannot grow it without limit.
#[test]
fn captured_stdout_collects_output_and_is_capped() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str("3 4 add == (x) print 1 2 pstack").expect("run");
    assert_eq!(String::from_utf8_lossy(it.captured_stdout()), "7\nx2\n1\n");

    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str("0 1 100000 { == } for").expect("run");
    let out = it.captured_stdout();
    assert!(out.len() <= pscat::interp::STDOUT_CAPTURE_LIMIT + 64);
    assert!(String::from_utf8_lossy(out).ends_with("output truncated: more than 64 KB\n"));

    // off by default
    assert!(Interp::new().captured_stdout().is_empty());
}
