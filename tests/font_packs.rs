//! Font packs (issue #599), through the real `pscat` binary: precedence
//! between a `fonts/catalog/` override and an installed pack, the
//! pinned/user-selected report, install from a pack built by
//! `scripts/font_packs.py`, and that script's own checks.
//!
//! The binary runs with `PSCAT_HOME` and `HOME` pointed at scratch
//! directories. Test builds also see the checkout's `fonts/catalog/`
//! (debug-assertion builds keep the checkout heuristics), so the
//! precedence tests use a face name the checkout does not have.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;

use pscat::fontpacks::pinned::PINNED_INDEX;
use sha2::{Digest, Sha256};

const PSCAT: &str = env!("CARGO_BIN_EXE_pscat");

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// `pscat` with a scratch PSCAT_HOME and HOME, from a scratch cwd.
fn pscat(home: &Path, args: &[&str]) -> (bool, String, String) {
    let work = home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    let out = Command::new(PSCAT)
        .args(args)
        .current_dir(&work)
        .env("PSCAT_HOME", home)
        .env("HOME", home.join("user"))
        .env("XDG_DATA_HOME", home.join("user/data"))
        .env_remove("PSCAT_ROOT")
        .env_remove("PSCAT_LIB")
        .env_remove("APPDATA")
        .output()
        .expect("run pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn tar_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in entries {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        out.extend_from_slice(&h);
        out.extend_from_slice(data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

/// A one-file pack `name` at `tag` holding `file` with `bytes`.
fn pack(name: &str, tag: &str, file: &str, bytes: &[u8]) -> Vec<u8> {
    let manifest = serde_json::json!({
        "manifest_schema": 1, "pack": name, "tag": tag,
        "files": [{ "path": file, "kind": "font", "license": "OFL-1.1",
                    "copyright": "test", "sha256": sha(bytes), "size": bytes.len() }],
    })
    .to_string();
    let tar = tar_of(&[(file, bytes), ("manifest.json", manifest.as_bytes())]);
    ruzstd::encoding::compress_to_vec(&tar[..], ruzstd::encoding::CompressionLevel::Fastest)
}

/// Which face `face` resolved to, told apart by advance width: "i" is
/// 600/1000 em in Liberation Mono, narrower in Serif, 222 in the
/// Helvetica substitute (FontName is the file stem either way).
fn face_of(home: &Path, face: &str) -> &'static str {
    let (ok, out, err) = pscat(
        home,
        &[
            "-e",
            &format!("/{face} findfont 1000 scalefont setfont (i) stringwidth pop round cvi =="),
        ],
    );
    assert!(ok, "{err}");
    match out.trim() {
        "600" => "mono",
        "278" => "serif",
        "222" => "substitute",
        other => panic!("unexpected width {other}"),
    }
}

#[test]
fn an_override_beats_an_installed_pack_and_status_is_reported() {
    let home = common::scratch_dir("fontpack-precedence");
    let serif = std::fs::read(root().join("fonts/LiberationSerif-Regular.ttf")).unwrap();
    let mono = std::fs::read(root().join("fonts/LiberationMono-Regular.ttf")).unwrap();
    let file = home.join("zz.tar.zst");
    std::fs::write(
        &file,
        pack(
            "zztest",
            "fonts-2026.10",
            "Zztest/Zztest-Regular.ttf",
            &serif,
        ),
    )
    .unwrap();

    let (ok, out, err) = pscat(
        &home,
        &["fonts", "install", "--from", file.to_str().unwrap()],
    );
    assert!(ok, "{err}");
    assert!(
        out.contains("installed zztest fonts-2026.10 (user-selected)"),
        "{out}"
    );
    assert!(
        home.join("fonts/packs/zztest/Zztest/Zztest-Regular.ttf")
            .is_file()
    );
    assert_eq!(face_of(&home, "Zztest"), "serif");

    // An explicit fonts/catalog/ override wins over the pack.
    let over = home.join("fonts/catalog/Zztest");
    std::fs::create_dir_all(&over).unwrap();
    std::fs::write(over.join("Zztest-Regular.ttf"), &mono).unwrap();
    assert_eq!(face_of(&home, "Zztest"), "mono");

    // Both are reported: the override and the pack, user-selected.
    let (ok, out, _) = pscat(&home, &["--fonts"]);
    assert!(ok);
    assert!(out.contains("# font-override "), "{out}");
    assert!(
        out.lines()
            .any(|l| l.starts_with("# font-pack zztest fonts-2026.10 user-selected ")),
        "{out}"
    );
    let (ok, out, _) = pscat(&home, &["--capabilities"]);
    assert!(ok);
    let caps: serde_json::Value = serde_json::from_str(&out).unwrap();
    let fp = &caps["font_packs"];
    assert_eq!(fp["installed"][0]["name"], "zztest");
    assert_eq!(fp["installed"][0]["status"], "user-selected");
    assert!(fp["overrides"].as_array().unwrap().iter().any(|o| {
        o["path"]
            .as_str()
            .unwrap()
            .starts_with(home.to_str().unwrap())
    }));
    // The build's own pin, whatever it is (none until a fonts-* release
    // is pinned): an index that doesn't match it stays user-selected.
    let pinned = match PINNED_INDEX {
        None => serde_json::Value::Null,
        Some(p) => serde_json::json!({"tag": p.tag, "sha256": p.sha256}),
    };
    assert_eq!(fp["pinned_index"], pinned);

    // Without the override the pack serves again; without the pack a
    // name no pack table lists substitutes as before.
    std::fs::remove_dir_all(home.join("fonts/catalog")).unwrap();
    assert_eq!(face_of(&home, "Zztest"), "serif");
    let (ok, out, err) = pscat(&home, &["fonts", "remove", "zztest"]);
    assert!(ok, "{err}");
    assert!(out.contains("removed zztest"));
    assert_eq!(face_of(&home, "Zztest"), "substitute");
}

#[test]
fn a_tampered_pack_is_refused_with_nothing_left_behind() {
    let home = common::scratch_dir("fontpack-tamper");
    let serif = std::fs::read(root().join("fonts/LiberationSerif-Regular.ttf")).unwrap();
    let mut good = pack(
        "zztest",
        "fonts-2026.10",
        "Zztest/Zztest-Regular.ttf",
        &serif,
    );
    // Flip a byte inside the compressed data: either zstd or the
    // per-file checksum must catch it.
    let mid = good.len() / 2;
    good[mid] ^= 0xff;
    let file = home.join("bad.tar.zst");
    std::fs::write(&file, &good).unwrap();
    let (ok, _, err) = pscat(
        &home,
        &["fonts", "install", "--from", file.to_str().unwrap()],
    );
    assert!(!ok, "a tampered pack installed");
    assert!(err.starts_with("pscat fonts: "), "{err}");
    let packs = home.join("fonts/packs");
    let left: Vec<_> = std::fs::read_dir(&packs)
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

#[test]
fn fonts_list_reports_the_builds_pin() {
    let home = common::scratch_dir("fontpack-pin");
    let (ok, out, _) = pscat(&home, &["fonts", "list"]);
    assert!(ok);
    match PINNED_INDEX {
        None => assert!(out.contains("pinned release: none"), "{out}"),
        Some(p) => assert!(out.contains(p.tag), "{out}"),
    }
    assert!(out.contains("not installed: extras"), "{out}");
}

#[test]
fn bare_install_without_a_pin_says_so_and_touches_nothing() {
    // A pinned build would download the pinned release here; that path
    // is covered by the release-fonts pinned smoke test, not offline tests.
    if PINNED_INDEX.is_some() {
        return;
    }
    let home = common::scratch_dir("fontpack-nopin");
    let (ok, _, err) = pscat(&home, &["fonts", "install"]);
    assert!(!ok);
    assert!(err.contains("pins no font-pack release"), "{err}");
    assert!(!home.join("fonts/packs").exists());
    let (ok, out, _) = pscat(&home, &["fonts", "list"]);
    assert!(ok);
    assert!(out.contains("pinned release: none"), "{out}");
    assert!(out.contains("not installed: extras"), "{out}");
}

#[test]
fn fonts_subcommand_rejects_bad_usage() {
    let home = common::scratch_dir("fontpack-usage");
    for args in [
        &["fonts", "frobnicate"][..],
        &["fonts", "install", "--version", "v0.6.0"],
        &[
            "fonts",
            "install",
            "--version",
            "fonts-2026.10",
            "--from",
            "x",
        ],
        &["fonts", "install", "--pack"],
        &["fonts", "remove", "--all"],
    ] {
        let (ok, _, err) = pscat(&home, args);
        assert!(!ok, "{args:?} succeeded");
        assert!(err.starts_with("pscat fonts: "), "{args:?}: {err}");
    }
    let (ok, out, _) = pscat(&home, &["fonts", "help"]);
    assert!(ok && out.contains("pscat fonts install"));
}

/// findfont clears its hint on every call, so a missing face caught by
/// `stopped` cannot label a later, unrelated error. (In a checkout every
/// pack face is on disk; the hint itself is unit-tested in src/font.rs
/// and src/interp.rs, and end to end by scripts/font_pack_smoke.sh.)
#[test]
fn a_caught_missing_face_leaves_no_hint_on_a_later_error() {
    let home = common::scratch_dir("fontpack-hint");
    let (ok, out, err) = pscat(
        &home,
        &[
            "-e",
            "{ /Symbol findfont } stopped pop /Helvetica findfont pop 1 0 div",
        ],
    );
    assert!(!ok);
    let report = format!("{out}{err}");
    assert!(report.contains("undefinedresult"), "{report}");
    assert!(!report.contains("font pack"), "{report}");
}

#[test]
fn font_packs_script_checks_and_self_tests_pass() {
    for (script, args) in [
        ("font_packs.py", &["check"][..]),
        ("test_font_packs.py", &[][..]),
    ] {
        let out = Command::new("python3")
            .arg(root().join("scripts").join(script))
            .args(args)
            .current_dir(root())
            .output()
            .expect("run python3");
        assert!(
            out.status.success(),
            "scripts/{script} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// The real builder's output (python ustar + the zstd CLI at -19)
/// installs through the Rust reader. Skipped without a `zstd` CLI.
#[test]
fn a_pack_built_by_the_script_installs() {
    if Command::new("zstd").arg("--version").output().is_err() {
        eprintln!("skipping: no zstd CLI");
        return;
    }
    let home = common::scratch_dir("fontpack-script");
    let out_dir: PathBuf = home.join("dist");
    let out = Command::new("python3")
        .arg(root().join("scripts/font_packs.py"))
        .args(["build", "--tag", "fonts-2026.10", "--out"])
        .arg(&out_dir)
        .args(["--pack", "texgyre"])
        .output()
        .expect("run font_packs.py");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let index = out_dir.join("font-index.json");
    let (ok, out, err) = pscat(
        &home,
        &["fonts", "install", "--from", index.to_str().unwrap()],
    );
    assert!(ok, "{err}");
    assert!(out.contains("installed texgyre fonts-2026.10"), "{out}");
    let (ok, out, err) = pscat(&home, &["fonts", "list"]);
    assert!(ok, "{err}");
    assert!(out.contains("27 files verified"), "{out}");
    let m: serde_json::Value = serde_json::from_slice(
        &std::fs::read(home.join("fonts/packs/texgyre/manifest.json")).unwrap(),
    )
    .unwrap();
    let files = m["files"].as_array().unwrap();
    assert!(files.iter().all(|f| f["license"] == "LPPL-1.3c"));
    assert!(
        files
            .iter()
            .any(|f| f["path"] == "TeXGyre-pagella/GUST-FONT-LICENSE.txt")
    );
}
