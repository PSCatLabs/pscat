//! `closepath` on an already-closed subpath is a no-op (#1178, PLRM 3rd ed.),
//! as it is when there is no current point. Ghostscript is the reference:
//! each case was run through gs 10.07.1 and its `pathforall` trace pinned
//! here; `ghostscript_prints_the_pinned_traces` re-checks them where gs is
//! installed (CI requires it).

use pscat::Interp;

mod common;

/// Plain PostScript, valid in pscat and in gs: `dump` prints the current
/// path's `pathforall` elements as letters, then the current point (or
/// `nocp`).
const PRELUDE: &str = "\
/dump { { pop pop (m ) print } { pop pop (l ) print } \
        { pop pop pop pop pop pop (c ) print } { (z ) print } pathforall \
        { currentpoint exch 64 string cvs print ( ) print 64 string cvs print } stopped \
        { (nocp) print } if (\\n) print } def \
/newcase { newpath } def\n";

/// (what, program, gs's trace).
const CASES: &[(&str, &str, &str)] = &[
    (
        "the issue's probe: a second closepath adds nothing",
        "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath dump",
        "m l l z 0.0 0.0\n",
    ),
    (
        "three closepaths are still one",
        "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath closepath dump",
        "m l l z 0.0 0.0\n",
    ),
    (
        "closepath closepath then lineto starts a fresh subpath",
        "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath 20 20 lineto dump",
        "m l l z m l 20.0 20.0\n",
    ),
    (
        "closepath closepath then moveto",
        "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath 20 20 moveto 30 30 lineto dump",
        "m l l z m l 30.0 30.0\n",
    ),
    (
        "the new subpath can be closed again, once",
        "newcase 0 0 moveto 10 0 lineto closepath 5 5 lineto closepath closepath dump",
        "m l z m l z 0.0 0.0\n",
    ),
    (
        "a lone moveto closes once",
        "newcase 0 0 moveto closepath closepath dump",
        "m z 0.0 0.0\n",
    ),
    (
        "two subpaths each close once",
        "newcase 0 0 moveto 1 1 lineto closepath closepath 5 5 moveto 6 6 lineto closepath closepath dump",
        "m l z m l z 5.0 5.0\n",
    ),
    (
        "closepath with no current point adds nothing",
        "newcase closepath closepath dump",
        "nocp\n",
    ),
    (
        "closepath after rlineto from a closed subpath",
        "newcase 0 0 moveto 1 1 lineto closepath closepath 3 3 rlineto closepath closepath dump",
        "m l z m l z 0.0 0.0\n",
    ),
    (
        "a curve subpath closes once",
        "newcase 0 0 moveto 1 1 2 2 3 0 curveto closepath closepath dump",
        "m c z 0.0 0.0\n",
    ),
];

fn pscat_trace(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&format!("{PRELUDE}{src}"))
        .unwrap_or_else(|e| panic!("pscat failed on {src:?}: {e}"));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

#[test]
fn closepath_on_a_closed_subpath_is_a_no_op() {
    for (what, src, want) in CASES {
        assert_eq!(&pscat_trace(src), want, "{what}: {src}");
    }
}

#[test]
fn flattenpath_and_stroke_keep_a_single_close() {
    let double = "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath flattenpath dump";
    let single = "newcase 0 0 moveto 10 0 lineto 10 10 lineto closepath flattenpath dump";
    assert_eq!(pscat_trace(double), pscat_trace(single));
    let double = "newcase 5 setlinewidth 0 0 moveto 10 0 lineto 10 10 lineto closepath closepath strokepath dump";
    let single =
        "newcase 5 setlinewidth 0 0 moveto 10 0 lineto 10 10 lineto closepath strokepath dump";
    assert_eq!(pscat_trace(double), pscat_trace(single));
}

#[test]
fn a_doubled_closepath_renders_like_a_single_one() {
    let render = |src: &str| {
        let mut it = Interp::new();
        it.run_str(src).expect("run");
        it.gfx().pixmap.data().to_vec()
    };
    let body = |close: &str| {
        format!(
            "8 setlinewidth 1 setlinejoin 100 100 moveto 300 100 lineto 200 300 lineto {close} stroke"
        )
    };
    assert_eq!(
        render(&body("closepath")),
        render(&body("closepath closepath"))
    );
    let fill = |close: &str| format!("100 100 moveto 300 100 lineto 200 300 lineto {close} fill");
    assert_eq!(
        render(&fill("closepath")),
        render(&fill("closepath closepath"))
    );
}

#[test]
fn svg_and_pdf_carry_a_single_close() {
    let export = |close: &str| {
        let mut it = Interp::new();
        it.enable_svg();
        it.enable_pdf();
        it.run_str(&format!(
            "4 setlinewidth 100 100 moveto 300 100 lineto 200 300 lineto {close} stroke \
             100 400 moveto 300 400 lineto 200 600 lineto {close} fill showpage"
        ))
        .expect("run");
        (
            it.svg_pages().expect("svg"),
            it.pdf_document().expect("pdf"),
        )
    };
    let (svg1, pdf1) = export("closepath");
    let (svg2, pdf2) = export("closepath closepath");
    assert_eq!(svg1, svg2);
    assert_eq!(pdf1, pdf2);
    assert!(svg1.iter().all(|p| !p.contains("Z Z") && !p.contains("ZZ")));
}

/// The pinned traces are gs's own.
#[test]
fn ghostscript_prints_the_pinned_traces() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("pscat-closepath-noop-gs");
    for (i, (what, src, want)) in CASES.iter().enumerate() {
        let file = dir.join(format!("c{i}.ps"));
        std::fs::write(&file, format!("{PRELUDE}{src}\n")).expect("write");
        let out = std::process::Command::new("gs")
            .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("run gs");
        assert!(out.status.success(), "gs rejected {what}");
        let got = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(&got, want, "gs, {what}: {src}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
