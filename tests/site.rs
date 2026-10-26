//! The Stage 17 site build: `scripts/build_site.sh` must assemble a
//! complete `_site/` — pages, wasm, playground sources, renders.
//! Skips gracefully when the wasm target isn't installed (gs-test
//! style); the Pages workflow runs the same script in CI.
//!
//! Safe beside other cargo runs and other copies of this test (#767): the
//! site is built into its own scratch directory, not the shared `_site/`
//! (a second run deleted it mid-build), and the script renders with this
//! test's own binary instead of nesting a `cargo build --release`. The
//! wasm build is the one nested cargo call left; it only waits on cargo's
//! lock. Nothing here has a timeout or reads the clock.

use std::process::Command;

mod common;

#[test]
fn build_site_assembles_everything() {
    let targets = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output();
    if !targets.is_ok_and(|o| {
        o.status.success() && String::from_utf8_lossy(&o.stdout).contains("wasm32-unknown-unknown")
    }) {
        eprintln!("skipping site test: wasm32-unknown-unknown target not installed");
        return;
    }

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = common::scratch_dir("site");
    let build = Command::new(root.join("scripts/build_site.sh"))
        .env("PSCAT_SITE_OUT", &out)
        .env("PSCAT_BIN", env!("CARGO_BIN_EXE_pscat"))
        .output()
        .expect("run build_site.sh");
    assert!(
        build.status.success(),
        "site build failed:\n{}",
        String::from_utf8_lossy(&build.stderr)
    );

    for page in [
        "index.html",
        "playground.html",
        "gallery.html",
        "architecture.html",
        "extending.html",
        "javascript.html",
        "agents.html",
        "guides.html",
        "contribute.html",
        "learn.html",
        "fonts.html",
        "charts.html",
        "style.css",
        "source-viewer.js",
        "pscat.js",
        "pscat.wasm",
    ] {
        assert!(out.join(page).exists(), "missing {page}");
    }
    // Every example the playground picker offers must be staged.
    let playground = std::fs::read_to_string(out.join("playground.html")).expect("read");
    for line in playground.lines() {
        if let Some(rest) = line.trim().strip_prefix("<option value=\"") {
            let name = rest.split('"').next().expect("option value");
            if !name.is_empty() {
                assert!(
                    out.join("examples").join(name).exists(),
                    "picker offers {name} but it isn't staged"
                );
            }
        }
    }
    // Every render the gallery shows must exist.
    let gallery = std::fs::read_to_string(out.join("gallery.html")).expect("read");
    for piece in gallery.split("assets/renders/").skip(1) {
        let name = piece.split('"').next().expect("img src");
        assert!(
            out.join("assets/renders").join(name).exists(),
            "gallery shows {name} but it wasn't rendered"
        );
    }
    // The `pscat chalk` boards on the front page (#849) must be drawn.
    let index = std::fs::read_to_string(out.join("index.html")).expect("read");
    assert!(index.contains("<pre><code>pscat chalk "), "no chalk commands");
    assert!(!index.contains("<!--@chalk-showcase-->"), "chalk marker left");
    let boards = index.split("assets/chalk/").skip(1).count();
    assert!(boards >= 3, "front page shows only {boards} chalk boards");
    for piece in index.split("assets/chalk/").skip(1) {
        let name = piece.split('"').next().expect("img src");
        assert!(
            out.join("assets/chalk").join(name).exists(),
            "front page shows chalk board {name} but it wasn't rendered"
        );
    }
    // One generated page per gallery piece (#251), linked from the gallery.
    let pieces = std::fs::read_to_string(root.join("gallery/manifest.json")).expect("manifest");
    let manifest: serde_json::Value = serde_json::from_str(&pieces).expect("manifest is JSON");
    for p in manifest["pieces"].as_array().expect("pieces") {
        let id = p["id"].as_str().expect("id").replace('_', "-");
        let page = format!("gallery-{id}.html");
        assert!(out.join(&page).exists(), "missing piece page {page}");
        assert!(gallery.contains(&page), "gallery.html does not link {page}");
    }
    // "View source" (#461): every piece and specimen has its program staged
    // where the window fetches it, byte for byte.
    for e in manifest["pieces"]
        .as_array()
        .expect("pieces")
        .iter()
        .chain(manifest["specimens"].as_array().expect("specimens"))
    {
        let src = e["source"].as_str().expect("source");
        let staged = std::fs::read(out.join("source").join(src))
            .unwrap_or_else(|_| panic!("source/{src} was not staged"));
        assert_eq!(staged, std::fs::read(root.join(src)).expect("read source"));
    }
    // Links, navigation and orphans (#250): the same check CI runs.
    let check = Command::new("python3")
        .arg(root.join("scripts/check_site_links.py"))
        .arg(&out)
        .output()
        .expect("run check_site_links.py");
    assert!(
        check.status.success(),
        "site link check failed:\n{}",
        String::from_utf8_lossy(&check.stdout)
    );
    // Only a passing run cleans up; a failing one leaves the site to look at.
    let _ = std::fs::remove_dir_all(&out);
}
