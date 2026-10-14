//! The landscape techniques cookbook (`docs/guides/landscape-cookbook.md`,
//! issue #186): every `postscript` block in it is a complete recipe that
//! must keep rendering, and every recipe has a committed thumbnail.
//!
//! A block's first line is `% recipe: <name>`; its thumbnail is
//! `docs/guides/landscape/<name>.png`, rendered at 2x (720x480 px) from the
//! 360x240 pt page every snippet is sized for. The pictures are not compared
//! (a stale illustration is not a failing recipe, and two platforms' raster
//! output differs in the last bit); the recipes are run in-process, so a
//! broken one is named and a slow one is caught by CPU time, not wall time.
//!
//! Regenerate the thumbnails after editing a snippet:
//! `COOKBOOK_WRITE_THUMBS=1 cargo test --profile ci --test landscape_cookbook`

mod common;

use common::within_cpu_budget;
use pscat::Interp;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

const DOC: &str = "docs/guides/landscape-cookbook.md";
const THUMBS: &str = "docs/guides/landscape";
const PAGE_W: u32 = 360;
const PAGE_H: u32 = 240;
/// Every recipe the cookbook promises, so deleting one is a failure.
const EXPECTED: [&str; 11] = [
    "sky",
    "clouds",
    "mountains",
    "mist",
    "forest",
    "evergreens",
    "bushes",
    "water",
    "reflection",
    "bank",
    "grass",
];

struct Recipe {
    name: String,
    src: String,
}

fn recipes() -> Vec<Recipe> {
    let doc = std::fs::read_to_string(DOC).unwrap_or_else(|e| panic!("{DOC}: {e}"));
    let mut out = Vec::new();
    let mut open: Option<Vec<&str>> = None;
    for line in doc.lines() {
        match (&mut open, line.trim_end()) {
            (None, "```postscript") => open = Some(Vec::new()),
            (Some(body), "```") => {
                let first = body.first().copied().unwrap_or("");
                let name = first
                    .strip_prefix("% recipe: ")
                    .unwrap_or_else(|| {
                        panic!(
                            "a postscript block must start with `% recipe: <name>`, got {first:?}"
                        )
                    })
                    .trim()
                    .to_string();
                out.push(Recipe {
                    name,
                    src: body.join("\n") + "\n",
                });
                open = None;
            }
            (Some(body), _) => body.push(line),
            (None, _) => {}
        }
    }
    assert!(open.is_none(), "{DOC}: an unterminated code fence");
    out
}

fn thumb_path(name: &str) -> PathBuf {
    PathBuf::from(THUMBS).join(format!("{name}.png"))
}

fn run_recipe(r: &Recipe) -> Interp {
    let mut it = Interp::with_page(PAGE_W, PAGE_H).expect("page");
    if let Err(e) = it.run_source(r.src.as_bytes()) {
        panic!("recipe {}: {}", r.name, it.error_report(&e));
    }
    it
}

#[test]
fn every_recipe_renders_clean_and_inside_its_budget() {
    let all = recipes();
    for r in &all {
        // Each snippet is checked as pscat's own `--lint` would: a blank
        // page, an unbalanced gsave and a stack leak are all findings.
        within_cpu_budget(
            &format!("recipe {}", r.name),
            Duration::from_secs(2),
            3,
            || {
                let it = run_recipe(r);
                let findings = it.lint(true);
                assert!(
                    findings.is_empty(),
                    "recipe {}: lint: {:?}",
                    r.name,
                    findings
                        .iter()
                        .map(|f| format!("{}: {}", f.check, f.message))
                        .collect::<Vec<_>>()
                );
                assert!(
                    it.operand_stack().is_empty(),
                    "recipe {} leaves the operand stack dirty",
                    r.name
                );
                // A picture, not a blank page or one flat fill.
                let distinct: HashSet<u32> = it
                    .gfx()
                    .pixmap
                    .pixels()
                    .iter()
                    .map(|p| u32::from_le_bytes([p.red(), p.green(), p.blue(), p.alpha()]))
                    .collect();
                assert!(
                    distinct.len() > 200,
                    "recipe {} draws only {} distinct colours",
                    r.name,
                    distinct.len()
                );
            },
        );
    }
}

fn write_thumbnails(all: &[Recipe]) {
    let dir = std::env::temp_dir().join(format!("landscape-cookbook-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    for r in all {
        let ps = dir.join(format!("{}.ps", r.name));
        std::fs::write(&ps, &r.src).expect("write recipe");
        let out = thumb_path(&r.name);
        std::fs::create_dir_all(THUMBS).expect("thumbnail dir");
        let status = Command::new(env!("CARGO_BIN_EXE_pscat"))
            .args([
                "--page",
                &format!("{PAGE_W}x{PAGE_H}"),
                "--dpi",
                "144",
                "--png",
            ])
            .arg(&out)
            .arg(&ps)
            .status()
            .expect("run pscat");
        assert!(status.success(), "pscat failed on recipe {}", r.name);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn recipes_are_named_once_and_each_has_a_current_thumbnail() {
    let all = recipes();
    if std::env::var_os("COOKBOOK_WRITE_THUMBS").is_some() {
        write_thumbnails(&all);
    }
    let names: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
    for want in EXPECTED {
        assert!(names.contains(&want), "the cookbook lost recipe {want:?}");
    }
    let unique: HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "duplicate recipe names: {names:?}"
    );

    let doc = std::fs::read_to_string(DOC).expect("doc");
    for r in &all {
        assert!(
            r.name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "recipe name {:?} must be lowercase letters, digits and hyphens",
            r.name
        );
        let png = std::fs::read(thumb_path(&r.name)).unwrap_or_else(|e| {
            panic!(
                "recipe {}: no thumbnail ({e}); run with COOKBOOK_WRITE_THUMBS=1",
                r.name
            )
        });
        assert_eq!(
            &png[..8],
            b"\x89PNG\r\n\x1a\n",
            "recipe {}: not a PNG",
            r.name
        );
        let (w, h) = (
            u32::from_be_bytes([png[16], png[17], png[18], png[19]]),
            u32::from_be_bytes([png[20], png[21], png[22], png[23]]),
        );
        assert_eq!(
            (w, h),
            (PAGE_W * 2, PAGE_H * 2),
            "recipe {}: thumbnail is {w}x{h}, want 2x the page",
            r.name
        );
        assert!(
            doc.contains(&format!("](landscape/{}.png)", r.name)),
            "recipe {}: the doc does not show its thumbnail",
            r.name
        );
    }
    // No orphans: a renamed recipe must not leave its old picture behind.
    for entry in std::fs::read_dir(THUMBS).expect("thumbnail dir") {
        let file = entry.expect("dir entry").file_name();
        let file = file.to_string_lossy();
        let stem = file.strip_suffix(".png").unwrap_or(&file);
        assert!(
            names.contains(&stem),
            "{THUMBS}/{file} belongs to no recipe"
        );
    }
}
