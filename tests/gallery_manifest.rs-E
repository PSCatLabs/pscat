//! The gallery manifest (#251): `gallery/manifest.json` is the one list of what
//! the site gallery, the README highlights and `gallery/show.sh` show, and it
//! must stay in step with `gallery/*.ps`, their renders, and the generated
//! parts of the READMEs. The checks live in `scripts/gallery.py` (stdlib
//! Python, so CI needs nothing installed); this test runs them under
//! `cargo test`, so every pull request is gated on them.

use std::path::Path;
use std::process::Command;

fn run(script: &str, args: &[&str]) -> (bool, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new("python3")
        .arg(root.join("scripts").join(script))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("run python3 scripts/{script}: {e}"));
    (
        out.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

#[test]
fn every_gallery_piece_has_a_manifest_entry_and_a_render() {
    let (ok, text) = run("gallery.py", &["check"]);
    assert!(
        ok,
        "gallery manifest check failed (scripts/gallery.py check; \
         `scripts/gallery.py sync` refreshes the generated READMEs):\n{text}"
    );
}

#[test]
fn gallery_script_self_test_passes() {
    let (ok, text) = run("test_gallery.py", &[]);
    assert!(ok, "scripts/test_gallery.py failed:\n{text}");
}
