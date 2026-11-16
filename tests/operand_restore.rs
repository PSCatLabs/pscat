//! A failing operator leaves the operand stack as it was before it ran,
//! and `$error /command` names that operator (#1047, #1074). Every case runs
//! under `stopped` with `77` on the stack first, and prints the stack it
//! leaves and `$error /command`; each line is pinned from Ghostscript
//! 10.07.1 and, when `gs` is installed, checked against it too.
//!
//! The cases cover the operator families: arithmetic and stack operators,
//! the loop family and `exec`/`if`/`stopped` with an error inside the
//! procedure they run (the inner operator's operands come back, the outer
//! one's do not), the show family and `kshow`, `pathforall`, images,
//! errordict handlers, bound and unbound procedures, and the checks `if`
//! and `ifelse` make. Lines ending `cmd=/--none--` are programs that do not
//! fail (`T` puts `/--none--` in `$error /command` before each case).

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// `(name, body, what gs prints)`.
const CASES: &[(&str, &str, &str)] = &[
    ("add-typecheck", r"1 (a) add", "77 1 (a) true cmd=--add--"),
    ("add-underflow", r"add", "77 true cmd=--add--"),
    ("add-1-underflow", r"1 add", "78 false cmd=/--none--"),
    (
        "image3",
        r"<< /ImageType 3 >> image",
        "77 -dict- true cmd=--image--",
    ),
    (
        "rectfill-2",
        r"1 2 rectfill",
        "77 1 2 true cmd=--rectfill--",
    ),
    (
        "rectfill-str",
        r"1 2 (a) 4 rectfill",
        "77 1 2 (a) 4 true cmd=--rectfill--",
    ),
    (
        "astore-underflow",
        r"1 2 3 5 array astore",
        "77 1 2 3 [5] true cmd=--astore--",
    ),
    ("get-range", r"[1 2] 5 get", "77 [2] 5 true cmd=--get--"),
    ("put-range", r"[1 2] 5 0 put", "77 [2] 5 0 true cmd=--put--"),
    ("div0", r"1 0 div", "77 1 0 true cmd=--div--"),
    ("idiv0", r"1 0 idiv", "77 1 0 true cmd=--idiv--"),
    (
        "bound-proc",
        r"/f { 1 0 div } bind def f",
        "77 1 0 true cmd=--div--",
    ),
    (
        "unbound-proc",
        r"/g { 1 0 div } def g",
        "77 1 0 true cmd=--div--",
    ),
    (
        "moveto-type",
        r"(a) 1 moveto",
        "77 (a) 1 true cmd=--moveto--",
    ),
    (
        "lineto-nocp",
        r"newpath 1 2 lineto",
        "77 1 2 true cmd=--lineto--",
    ),
    ("exch-under", r"exch", "77 true cmd=--exch--"),
    (
        "roll-under",
        r"1 2 5 1 roll",
        "77 1 2 5 1 true cmd=--roll--",
    ),
    ("index-range", r"1 2 5 index", "77 1 2 5 true cmd=--index--"),
    ("copy-range", r"1 2 5 copy", "77 1 2 5 true cmd=--copy--"),
    (
        "for-type",
        r"1 (a) 3 {} for",
        "77 1 (a) 3 {0} true cmd=--for--",
    ),
    (
        "for-inner",
        r"1 1 3 { (x) add } for",
        "77 1 (x) true cmd=--add--",
    ),
    (
        "forall-inner",
        r"[1 2] { (x) add } forall",
        "77 1 (x) true cmd=--add--",
    ),
    (
        "repeat-inner",
        r"2 { 9 (x) add } repeat",
        "77 9 (x) true cmd=--add--",
    ),
    (
        "repeat-type",
        r"(a) {} repeat",
        "77 (a) {0} true cmd=--repeat--",
    ),
    (
        "exec-inner",
        r"5 { (x) add } exec",
        "77 5 (x) true cmd=--add--",
    ),
    ("if-type", r"1 {} if", "77 1 {0} true cmd=--if--"),
    (
        "ifelse-type",
        r"true 1 {} ifelse",
        "77 true 1 {0} true cmd=--ifelse--",
    ),
    (
        "stopped-inner",
        r"5 { (x) add } stopped pop",
        "77 5 (x) false cmd=--add--",
    ),
    ("scale-type", r"1 (a) scale", "77 1 (a) true cmd=--scale--"),
    (
        "setrgb-type",
        r"1 2 (a) setrgbcolor",
        "77 1 2 (a) true cmd=--setrgbcolor--",
    ),
    (
        "show-nocp",
        r"newpath /Helvetica 12 selectfont (abc) show",
        "77 (abc) true cmd=--show--",
    ),
    (
        "show-type",
        r"0 0 moveto /Helvetica 12 selectfont 5 show",
        "77 5 true cmd=--show--",
    ),
    (
        "kshow-inner",
        r"0 0 moveto /Helvetica 12 selectfont { (x) add } (abc) kshow",
        "77 97 98 (x) true cmd=--add--",
    ),
    ("string-range", r"-1 string", "77 -1 true cmd=--string--"),
    ("array-range", r"-1 array", "77 -1 true cmd=--array--"),
    ("begin-type", r"5 begin", "77 5 true cmd=--begin--"),
    ("end-under", r"end", "77 true cmd=--end--"),
    (
        "cleartomark",
        r"1 2 cleartomark",
        "77 1 2 true cmd=--cleartomark--",
    ),
    (
        "counttomark",
        r"1 2 counttomark",
        "77 1 2 true cmd=--counttomark--",
    ),
    (
        "getinterval",
        r"(abc) 1 9 getinterval",
        "77 (abc) 1 9 true cmd=--getinterval--",
    ),
    (
        "putinterval",
        r"(abc) 2 (xyz) putinterval",
        "77 (abc) 2 (xyz) true cmd=--putinterval--",
    ),
    ("aload-type", r"5 aload", "77 5 true cmd=--aload--"),
    ("concat-type", r"5 concat", "77 5 true cmd=--concat--"),
    (
        "transform-type",
        r"1 (a) transform",
        "77 1 (a) true cmd=--transform--",
    ),
    (
        "arc-type",
        r"0 0 10 0 (a) arc",
        "77 0 0 10 0 (a) true cmd=--arc--",
    ),
    (
        "curveto-nocp",
        r"newpath 1 2 3 4 5 6 curveto",
        "77 1 2 3 4 5 6 true cmd=--curveto--",
    ),
    (
        "setdash-type",
        r"5 0 setdash",
        "77 5 0 true cmd=--setdash--",
    ),
    (
        "pathforall-inner",
        r"newpath 0 0 moveto 1 1 lineto { (x) add } { pop pop } { } { } pathforall",
        "77 0.0 0.0 (x) true cmd=--add--",
    ),
    (
        "xshow",
        r"0 0 moveto /Helvetica 12 selectfont (ab) [1] xshow",
        "77 (ab) [1] true cmd=--xshow--",
    ),
    ("token", r"5 token", "77 5 true cmd=--token--"),
    (
        "search",
        r"(abc) 5 search",
        "77 (abc) 5 true cmd=--search--",
    ),
    ("exp", r"-1 0.5 exp", "77 -1 0.5 true cmd=--exp--"),
    ("sqrt", r"-1 sqrt", "77 -1 true cmd=--sqrt--"),
    ("ln", r"0 ln", "77 0 true cmd=--ln--"),
    ("cvn", r"5 cvn", "77 5 true cmd=--cvn--"),
    ("cvs-type", r"123 5 cvs", "77 123 5 true cmd=--cvs--"),
    (
        "matrix-ops",
        r"1 2 [1 0 0 1 0] translate",
        "77 1 2 [5] true cmd=--translate--",
    ),
    ("neg", r"(a) neg", "77 (a) true cmd=--neg--"),
    ("not", r"(a) not", "77 (a) true cmd=--not--"),
    ("dict-range", r"-1 dict", "77 -1 true cmd=--dict--"),
    ("known", r"5 /a known", "77 5 /a true cmd=--known--"),
    (
        "reraise-cmd",
        r"errordict /rangecheck get exec",
        "true cmd=77",
    ),
    (
        "image-proc-bad",
        r"1 1 8 [1 0 0 1 0 0] { 5 } image",
        "77 5 true cmd=--image--",
    ),
    (
        "cvx-exec-str",
        r"4 (1 (a) add) cvx exec",
        "77 4 1 (a) true cmd=--add--",
    ),
    (
        "stringwidth-type",
        r"/Helvetica 12 selectfont 5 stringwidth",
        "77 5 true cmd=--stringwidth--",
    ),
    ("setfont-type", r"5 setfont", "77 5 true cmd=--setfont--"),
    (
        "scalefont-type",
        r"/Helvetica findfont (a) scalefont",
        "77 -dict- (a) true cmd=--scalefont--",
    ),
    (
        "widthshow-inner",
        r"0 0 moveto /Helvetica 12 selectfont 1 2 (a) 4 (ab) widthshow",
        "77 1 2 (a) 4 (ab) true cmd=--widthshow--",
    ),
    (
        "loop-inner",
        r"{ 3 (x) add } loop",
        "77 3 (x) true cmd=--add--",
    ),
    ("exit-invalid", r"exit", "77 true cmd=--exit--"),
    (
        "stop-nest",
        r"3 { 4 (y) sub } stopped pop 5 (z) mul",
        "77 3 4 (y) 5 (z) true cmd=--mul--",
    ),
    (
        "bind-cmd",
        r"/h { 1 (q) sub } bind def 6 h",
        "77 6 1 (q) true cmd=--sub--",
    ),
    (
        "execstack",
        r"3 { 2 (q) mul } 0 get",
        "77 3 2 false cmd=/--none--",
    ),
    ("index-neg", r"1 -1 index", "77 1 -1 true cmd=--index--"),
    (
        "rectfill-arr",
        r"[1 2 3] rectfill",
        "77 [3] true cmd=--rectfill--",
    ),
    (
        "rectstroke-arr",
        r"[1 2 3 4 5] rectstroke",
        "77 [5] true cmd=--rectstroke--",
    ),
    (
        "rectstroke-mtx",
        r"1 2 3 4 [1 0 0] rectstroke",
        "77 1 2 3 4 [3] true cmd=--rectstroke--",
    ),
    (
        "currentpoint",
        r"newpath currentpoint",
        "77 true cmd=--currentpoint--",
    ),
    ("pop-under", r"pop pop", "true cmd=--pop--"),
    ("dup-under", r"clear dup", "true cmd=--dup--"),
    (
        "bound-curveto",
        r"/f { newpath 0 0 moveto 1 2 3 4 curveto } bind def f",
        "77 1 2 3 4 true cmd=--curveto--",
    ),
    (
        "unbound-curveto",
        r"/f { newpath 0 0 moveto 1 2 3 4 curveto } def f",
        "77 1 2 3 4 true cmd=--curveto--",
    ),
    (
        "local-s",
        r"/g { 10 dict begin /s 5 def newpath 0 0 moveto s s s s curveto end } bind def g",
        "77 5 5 5 5 true cmd=--curveto--",
    ),
    (
        "bound-div-tail",
        r"/f { 1 0 div } bind def f",
        "77 1 0 true cmd=--div--",
    ),
    (
        "bound-div-mid",
        r"/f { 1 0 div 5 } bind def f",
        "77 1 0 true cmd=--div--",
    ),
    (
        "exec-op",
        r"1 (a) /add load exec",
        "77 1 (a) true cmd=--add--",
    ),
    ("exec-undef", r"5 /nosuch cvx exec", "77 5 true cmd=nosuch"),
    (
        "load-undef",
        r"/nosuch load",
        "77 /nosuch true cmd=--load--",
    ),
    ("undefined", r"nosuch", "77 true cmd=nosuch"),
    (
        "reraise",
        r"5 errordict /rangecheck get exec",
        "77 true cmd=5",
    ),
    ("if-int", r"true 5 if", "77 true 5 true cmd=--if--"),
    ("if-litarr", r"true [1] if", "77 true [1] true cmd=--if--"),
    ("if-name", r"true /pop if", "77 true /pop true cmd=--if--"),
    (
        "if-op",
        r"3 true /pop load if",
        "77 3 true --pop-- true cmd=--if--",
    ),
    (
        "ifelse-int",
        r"true 5 {} ifelse",
        "77 true 5 {0} true cmd=--ifelse--",
    ),
    (
        "ifelse-int2",
        r"false {} 5 ifelse",
        "77 false {0} 5 true cmd=--ifelse--",
    ),
    (
        "ifelse-litarr",
        r"true [1] {2} ifelse",
        "77 true [1] {1} true cmd=--ifelse--",
    ),
    (
        "ifelse-ok",
        r"false {1} {2} ifelse",
        "77 2 false cmd=/--none--",
    ),
    ("if-false-int", r"false 5 if", "77 false 5 true cmd=--if--"),
    (
        "rectclip-str",
        r"1 2 (a) 4 rectclip",
        "77 1 2 (a) 4 true cmd=--rectclip--",
    ),
    (
        "rectclip-2",
        r"1 2 rectclip",
        "77 1 2 true cmd=--rectclip--",
    ),
    (
        "put-readonly",
        r"[1 2] readonly 0 5 put",
        "77 [2] 0 5 true cmd=--put--",
    ),
    ("and-type", r"true 1 and", "77 true 1 true cmd=--and--"),
    ("not-type", r"(a) not", "77 (a) true cmd=--not--"),
    ("floor-type", r"(a) floor", "77 (a) true cmd=--floor--"),
    ("cvi-type", r"/a cvi", "77 /a true cmd=--cvi--"),
    ("known-type", r"5 /k known", "77 5 /k true cmd=--known--"),
    (
        "get-dict-undef",
        r"1 dict /k get",
        "77 -dict- /k true cmd=--get--",
    ),
    (
        "setdash-arr",
        r"[1 (a)] 0 setdash",
        "77 [2] 0 true cmd=--setdash--",
    ),
    (
        "setcmyk-type",
        r"1 2 3 (a) setcmykcolor",
        "77 1 2 3 (a) true cmd=--setcmykcolor--",
    ),
    (
        "rcurveto-nocp",
        r"newpath 1 2 3 4 5 6 rcurveto",
        "77 1 2 3 4 5 6 true cmd=--rcurveto--",
    ),
    (
        "exec-nested-op",
        r"1 (a) true /add load if",
        "77 1 (a) true --add-- true cmd=--if--",
    ),
    // A bound image's bad data names the operator that began the image (gs).
    (
        "image-bound-bad-data",
        r"/f { 1 1 8 [1 0 0 1 0 0] (zz) /ASCIIHexDecode filter image } bind def f",
        "77 true cmd=--image--",
    ),
    (
        "imagemask-bound-bad-data",
        r"/f { 1 1 true [1 0 0 1 0 0] (zz) /ASCIIHexDecode filter imagemask } bind def f",
        "77 true cmd=--imagemask--",
    ),
    ("dict-odd", r"<< 1 >>", "77 -mark- 1 true cmd=-->>--"),
    (
        "dict-nullkey",
        r"<< null 1 >>",
        "77 -mark- null 1 true cmd=-->>--",
    ),
    ("arr-nomark", r"1 2 ]", "77 1 2 true cmd=--]--"),
    ("dict-nomark", r"1 2 >>", "77 1 2 true cmd=-->>--"),
    (
        "cshow-inner",
        r"0 0 moveto /Helvetica 12 selectfont { pop pop cvi 1 (x) add } (ab) cshow",
        "77 97 1 (x) true cmd=--add--",
    ),
    (
        "fill-ok-then-fail",
        r"newpath 0 0 moveto 1 1 lineto fill 1 (a) add",
        "77 1 (a) true cmd=--add--",
    ),
];

/// Plain PostScript, valid in pscat and in gs: `P` prints any object the
/// same way in both (`==` differs on reals and has no `==only` in pscat),
/// and `T` runs one case.
const PRELUDE: &str = r"
/P { dup type /operatortype eq { (--) print 64 string cvs print (--) print } {
     dup type /nametype eq { dup xcheck not { (/) print } if 64 string cvs print } {
     dup type /stringtype eq { (\() print print (\)) print } {
     dup type /arraytype eq { dup xcheck { ({) print length 3 string cvs print (}) print }
       { ([) print length 3 string cvs print (]) print } ifelse } {
     dup type /dicttype eq { pop (-dict-) print } {
     dup type /marktype eq { pop (-mark-) print } {
     dup type /nulltype eq { pop (null) print } {
     64 string cvs print } ifelse } ifelse } ifelse } ifelse } ifelse } ifelse } ifelse ( ) print } def
/T { /TP exch def print (: ) print $error /command /--none-- put clear 77 /TP load stopped
  count array astore { P } forall
  (cmd=) print $error /command get P (\n) print } def
";

fn program(cases: &[(&str, &str, &str)]) -> String {
    let mut src = String::from(PRELUDE);
    for (name, body, _) in cases {
        src.push_str(&format!("({name}) {{ {body} }} T\n"));
    }
    src
}

fn tidy(out: &str) -> Vec<String> {
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn want(cases: &[(&str, &str, &str)]) -> Vec<String> {
    cases
        .iter()
        .map(|(name, _, out)| format!("{name}: {out}"))
        .collect()
}

fn pscat_lines(src: &str) -> Vec<String> {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    tidy(&String::from_utf8_lossy(it.captured_stdout()))
}

#[test]
fn pscat_prints_the_lines_gs_printed() {
    let got = pscat_lines(&program(CASES));
    let want = want(CASES);
    for (g, w) in got.iter().zip(&want) {
        assert_eq!(g, w);
    }
    assert_eq!(got.len(), want.len());
}

#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("operand_restore_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, program(CASES)).expect("write ps");
    let out = Command::new("gs")
        .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH"])
        .arg(&ps)
        .output_deadline()
        .expect("run gs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        out.status.success(),
        "gs failed: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert_eq!(tidy(&String::from_utf8_lossy(&out.stdout)), want(CASES));
}

/// A deliberate difference: an error inside an image's data procedure.
/// Ghostscript names the image operator for it, whatever failed inside
/// (`77 1 (x) true cmd=--image--`, gs 10.07.1); pscat names the operator
/// that failed, as it does inside any other procedure. The stack is the same.
#[test]
fn an_error_inside_an_image_data_procedure_names_the_inner_operator() {
    let cases = [(
        "image-proc-inner",
        "1 1 8 [1 0 0 1 0 0] { 1 (x) add } image",
        "77 1 (x) true cmd=--add--",
    )];
    assert_eq!(pscat_lines(&program(&cases)), want(&cases));
}

/// The report of an error nothing catches names the operator too (#1074):
/// before, a bound procedure's error was reported under the procedure's
/// name, and one after a local was looked up under the local's.
#[test]
fn an_uncaught_error_names_the_operator() {
    for src in [
        "/f { newpath 0 0 moveto 1 2 3 4 curveto } bind def f",
        "/f { newpath 0 0 moveto 1 2 3 4 curveto } def f",
        "/g { 10 dict begin /s 5 def newpath 0 0 moveto s s s s curveto end } bind def g",
    ] {
        let mut it = Interp::new();
        let err = it.run_str(src).expect_err(src);
        let report = it.error_report(&err);
        assert!(
            report.contains("stackunderflow; OffendingCommand: curveto;"),
            "{src}: {report}"
        );
    }
}

/// A deliberate difference: a Separation tint transform that returns a
/// non-number. The result stays on the stack in pscat (`77 (x)`); gs leaves
/// its own internal state there and names an internal operator
/// (`77 [4] -mark- null null null (x) true cmd=/--@anonymous_operator--`,
/// gs 10.07.1). pscat names the last name the transform ran.
#[test]
fn a_tint_transform_with_a_bad_result_leaves_it_on_the_stack() {
    let cases = [(
        "sep-tint-bad",
        "[/Separation /Foo /DeviceRGB { pop (x) }] setcolorspace 0.5 setcolor",
        "77 (x) true cmd=pop",
    )];
    assert_eq!(pscat_lines(&program(&cases)), want(&cases));
}

/// `handleerror` names a failed operator as the report does (`div`), not
/// `--nostringval--` (#1047, Codex review round 1; gs prints
/// `Error: /undefinedresult in --div--`).
#[test]
fn handleerror_names_the_failed_operator() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["-e", "{ 1 0 div } stopped pop handleerror"])
        .output_deadline()
        .expect("run pscat");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "%%[ Error: undefinedresult; OffendingCommand: div ]%%\n"
    );
}

/// `if`, `ifelse` and `exec` check for room on the execution stack before
/// they take their operands, so an `execstackoverflow` leaves them, as in
/// Ghostscript. (gs grows its execution stack far past pscat's limit, so
/// these are pscat's own: the recursion is not in tail position, or it would
/// run in constant depth.)
#[test]
fn an_execstackoverflow_leaves_the_operands() {
    for (def, want) in [
        (
            "/r { true { r } if 0 pop } def",
            "/execstackoverflow --if-- {r} true",
        ),
        (
            "/r { true { r } if 0 pop } bind def",
            "/execstackoverflow --if-- {r} true",
        ),
        (
            "/r { true { r } { } ifelse 0 pop } def",
            "/execstackoverflow --ifelse-- {} {r} true",
        ),
        (
            "/r { { r } exec 0 pop } def",
            "/execstackoverflow --exec-- {r}",
        ),
    ] {
        let mut it = Interp::new();
        it.capture_stdout();
        it.run_str(&format!(
            "{def} {{ r }} stopped pop $error /errorname get == $error /command get == \
             count {{ == }} repeat"
        ))
        .unwrap_or_else(|e| panic!("{def}: {}", it.error_report(&e)));
        assert_eq!(
            tidy(&String::from_utf8_lossy(it.captured_stdout())).join(" "),
            want,
            "{def}"
        );
    }
}
