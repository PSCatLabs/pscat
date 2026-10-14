//! `pathbbox` (issue #288): the user-space box of the path's device-space
//! bounding box, as Ghostscript computes it. Under a rotation or skew that
//! is larger than the tight user-space box, so the checks are:
//!
//! * hard-coded gs values (run everywhere, CI has no Ghostscript),
//! * a live gs matrix of CTMs x paths (skipped when `gs` is missing).

use std::process::Command;

use pscat::Interp;

fn bbox(src: &str) -> Option<[f64; 4]> {
    let mut it = Interp::new();
    if it.run_str(&format!("{src} pathbbox")).is_err() {
        return None;
    }
    let v: Vec<f64> = it
        .operand_stack()
        .iter()
        .map(|o| o.repr().parse().expect("number"))
        .collect();
    Some([v[0], v[1], v[2], v[3]])
}

fn near(a: [f64; 4], b: [f64; 4], tol: f64) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol)
}

fn gs_available() -> bool {
    Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn gs_bbox(src: &str) -> Option<[f64; 4]> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("pscat_pathbbox_{}_{n}.ps", std::process::id()));
    std::fs::write(
        &path,
        format!(
            "false setstrokeadjust {{ {src} pathbbox 4 array astore == }} stopped {{ (err) = }} if\n"
        ),
    )
    .unwrap();
    let out = Command::new("gs")
        .args(["-q", "-dNOPAUSE", "-dBATCH", "-dNODISPLAY"])
        .arg(&path)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let line = text.lines().next().unwrap_or("").trim().to_string();
    if line == "err" {
        return None;
    }
    let nums: Vec<f64> = line
        .trim_matches(['[', ']'])
        .split_whitespace()
        .map(|n| n.parse().unwrap_or_else(|_| panic!("gs said {text:?}")))
        .collect();
    Some([nums[0], nums[1], nums[2], nums[3]])
}

#[test]
fn rotated_ctm_gives_the_user_box_of_the_device_box() {
    // The issue's case, pinned against gs: the tight box would be
    // 0 0 100 50.
    let b =
        bbox("200 200 translate 30 rotate newpath 0 0 moveto 100 0 lineto 100 50 lineto").unwrap();
    assert!(near(b, [0.0, -43.30, 121.65, 80.80], 0.01), "{b:?}");
}

#[test]
fn curve_control_points_count_until_flattened() {
    // gs-pinned: 180.80 unflattened (control points), 150.00 flattened.
    let c = "200 200 translate 30 rotate newpath 0 0 moveto 100 100 100 -50 150 0 curveto";
    let b = bbox(c).unwrap();
    assert!(near(b, [0.0, -64.95, 180.80, 118.30], 0.01), "{b:?}");
    let f = bbox(&format!("{c} flattenpath")).unwrap();
    assert!(near(f, [0.0, -64.95, 150.00, 64.95], 0.05), "{f:?}");
}

#[test]
fn axis_aligned_ctms_are_unchanged() {
    let b = bbox("100 50 translate 2 3 scale newpath 1 1 moveto 11 6 lineto").unwrap();
    assert!(near(b, [1.0, 1.0, 11.0, 6.0], 1e-4), "{b:?}");
    // A flip (negative scale) still reports lower-left first.
    let b = bbox("-1 1 scale newpath 1 1 moveto 11 6 lineto").unwrap();
    assert!(near(b, [1.0, 1.0, 11.0, 6.0], 1e-4), "{b:?}");
    let b = bbox("90 rotate newpath 1 1 moveto 11 6 lineto").unwrap();
    assert!(near(b, [1.0, 1.0, 11.0, 6.0], 1e-4), "{b:?}");
}

#[test]
fn lone_moveto_and_moveto_closepath_count() {
    // gs-pinned.
    let b = bbox("newpath 10 20 moveto").unwrap();
    assert!(near(b, [10.0, 20.0, 10.0, 20.0], 1e-4), "{b:?}");
    let b = bbox("newpath 10 20 moveto 5 5 rmoveto closepath").unwrap();
    assert!(near(b, [15.0, 25.0, 15.0, 25.0], 1e-4), "{b:?}");
}

#[test]
fn trailing_moveto_still_counts_known_deviation() {
    // gs ignores a moveto with nothing after it (0 0 100 0 here); pscat
    // counts it. Not changed with #288 because it moves committed
    // charpath-text renders; tracked in #316.
    let b = bbox("newpath 0 0 moveto 100 0 lineto 200 200 moveto").unwrap();
    assert!(near(b, [0.0, 0.0, 200.0, 200.0], 1e-4), "{b:?}");
}

#[test]
fn empty_path_is_nocurrentpoint() {
    assert!(bbox("newpath").is_none());
}

const CTMS: &[(&str, &str)] = &[
    ("identity", ""),
    ("translate", "200 100 translate"),
    ("rotate 30", "200 200 translate 30 rotate"),
    ("rotate 45", "200 200 translate 45 rotate"),
    ("rotate 90", "200 200 translate 90 rotate"),
    ("rotate -20", "200 200 translate -20 rotate"),
    ("scale", "200 200 translate 2 0.5 scale"),
    (
        "scale then rotate",
        "200 200 translate 1.5 1 scale 30 rotate",
    ),
    (
        "rotate then scale",
        "200 200 translate 30 rotate 0.5 2 scale",
    ),
    ("skew", "[1 0 0.5 1 100 100] concat"),
    ("general matrix", "[1 0.3 -0.4 1.2 200 100] concat"),
    ("flip", "[-1 0 0 1 300 100] concat"),
];

/// (name, path, tolerance). Plain paths agree to gs's fixed-point noise;
/// flattened curves and stroke outlines differ in how gs and tiny-skia
/// approximate curves (see notes/264-strokepath.md).
const PATHS: &[(&str, &str, f64)] = &[
    (
        "lines",
        "newpath 0 0 moveto 100 0 lineto 100 50 lineto",
        0.05,
    ),
    (
        "curve",
        "newpath 0 0 moveto 100 100 100 -50 150 0 curveto",
        0.05,
    ),
    (
        "flattened curve",
        "newpath 0 0 moveto 100 100 100 -50 150 0 curveto flattenpath",
        0.5,
    ),
    ("lone moveto", "newpath 10 20 moveto", 0.05),
    (
        "moveto closepath",
        "newpath 10 20 moveto 5 5 rmoveto closepath",
        0.05,
    ),
    ("circle", "newpath 50 50 20 0 360 arc", 0.05),
    (
        "two subpaths",
        "newpath 0 0 moveto 100 0 lineto 100 50 lineto closepath 20 20 moveto 30 40 lineto",
        0.05,
    ),
    ("empty", "newpath", 0.0),
    (
        "strokepath of lines",
        "newpath 0 0 moveto 100 0 lineto 100 50 lineto 10 setlinewidth 0 setlinejoin strokepath",
        0.05,
    ),
    (
        "strokepath of a curve",
        "newpath 0 0 moveto 100 100 100 -50 150 0 curveto 8 setlinewidth strokepath",
        4.0,
    ),
];

#[test]
fn pathbbox_matches_ghostscript_over_a_matrix() {
    if !gs_available() {
        eprintln!("skipping: ghostscript (gs) not installed");
        return;
    }
    for (cname, ctm) in CTMS {
        // Stroke width is the scalar sqrt|det| (a round pen), so gs's
        // elliptical pen differs under anisotropic CTMs; those compare
        // paths only. Known deviation, see docs/dev/HANDOFF.md.
        let aniso = matches!(
            *cname,
            "scale" | "scale then rotate" | "rotate then scale" | "skew" | "general matrix"
        );
        for (pname, path, tol) in PATHS {
            if aniso && pname.starts_with("strokepath") {
                continue;
            }
            let src = format!("{ctm} {path}");
            match (bbox(&src), gs_bbox(&src)) {
                (Some(a), Some(b)) => {
                    assert!(
                        near(a, b, *tol),
                        "{cname} / {pname}: pscat {a:?} vs gs {b:?}"
                    )
                }
                (None, None) => {}
                (a, b) => panic!("{cname} / {pname}: pscat {a:?} vs gs {b:?}"),
            }
        }
    }
}
