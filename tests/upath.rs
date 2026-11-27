//! Hit testing and user paths (#1264, PLRM 3rd ed.): `infill`, `ineofill`,
//! `instroke`, `inufill`, `inueofill`, `inustroke`, `uappend`, `upath`,
//! `ufill`, `ueofill`, `ustroke`, `ustrokepath`, `ucache` and `setbbox`.
//! Ghostscript 10.07.1 is the reference: each case below was run through gs
//! and its output is pinned here. `ghostscript_prints_the_pinned_output`
//! re-checks every case where gs is installed (CI requires it).
//!
//! gs runs with stroke adjustment off (the prelude says `false
//! setstrokeadjust`): with it on, gs moves a stroke's edges onto pixel
//! boundaries and answers `instroke` for a pixel its adjusted edge only
//! touches, which pscat, that does not adjust, does not.

mod common;

use common::CommandDeadline as _;
use pscat::Interp;

/// Plain PostScript, valid in pscat and in gs. `dump` prints the current
/// path's `pathforall` elements and the current point; `T` runs a
/// procedure under `stopped` and prints the error name and the operands
/// left; `ar` prints a user path; `pb` prints `pathbbox` in 1/256ths;
/// `pf pe ps pu pue pus` print T or F for each hit test.
const PRELUDE: &str = r#"/pa { 32 string cvs print ( ) print } def
/dump { { 2 array astore (m ) print { pa } forall } { 2 array astore (l ) print { pa } forall }
        { 6 array astore (c ) print { pa } forall } { (z ) print } pathforall
        { currentpoint 2 array astore (@ ) print { pa } forall } stopped { (nocp) print } if
        (\n) print } def
/T { stopped { (ERR ) print $error /errorname get 32 string cvs print ( n=) print count 32 string cvs print ( ) print clear }
          { (ok ) print clear } ifelse } def
/ar { dup xcheck { (x) } { (l) } ifelse print ( ) print
      { dup type /nametype eq { 32 string cvs print ( ) print } { pa } ifelse } forall (\n) print } def
/pb { pathbbox 4 array astore { 256 mul round cvi pa } forall (\n) print } def
/pf { infill { (T) } { (F) } ifelse print } def
/pe { ineofill { (T) } { (F) } ifelse print } def
/ps { instroke { (T) } { (F) } ifelse print } def
/pu { inufill { (T) } { (F) } ifelse print } def
/pue { inueofill { (T) } { (F) } ifelse print } def
/pus { inustroke { (T) } { (F) } ifelse print } def
/nl { (\n) print } def
/SQ { 0 0 100 100 setbbox 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath } cvlit def
/q { 256 mul round cvi pa } def
false setstrokeadjust
"#;

/// (what, program, gs's output).
const CASES: &[(&str, &str, &str)] = &[
    (
        "a user path appends its moves and lines",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "closepath keeps the subpath's start as the current point",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto closepath } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 z @ 1.0 1.0 \n",
    ),
    (
        "relative moves and lines",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto 2 2 rmoveto 1 1 rlineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 m 7.0 7.0 l 8.0 8.0 @ 8.0 8.0 \n",
    ),
    (
        "curveto and rcurveto",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 1 1 2 2 3 3 curveto 1 0 2 0 3 1 rcurveto } uappend dump"#,
        "m 1.0 1.0 c 1.0 1.0 2.0 2.0 3.0 3.0 c 4.0 3.0 5.0 3.0 6.0 4.0 @ 6.0 4.0 \n",
    ),
    (
        "a lineto after closepath starts a subpath at the old start",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 1 lineto 5 5 lineto closepath 8 8 lineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 1.0 l 5.0 5.0 z m 1.0 1.0 l 8.0 8.0 @ 8.0 8.0 \n",
    ),
    (
        "two movetos make one",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto 1 2 moveto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 m 1.0 2.0 @ 1.0 2.0 \n",
    ),
    (
        "it extends the current path",
        r#"newpath 3 3 moveto 4 4 lineto { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 3.0 3.0 l 4.0 4.0 m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "it follows a closed path with a moveto",
        r#"newpath 3 3 moveto 4 4 lineto closepath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 3.0 3.0 l 4.0 4.0 z m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "the coordinates are in the current user space",
        r#"newpath 2 4 scale { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "under a translate",
        r#"newpath 100 200 translate { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a box with only setbbox adds nothing",
        r#"newpath { 0 0 10 10 setbbox } uappend dump"#,
        "nocp\n",
    ),
    (
        "setbbox then closepath adds nothing",
        r#"newpath { 0 0 10 10 setbbox closepath } uappend dump"#,
        "nocp\n",
    ),
    (
        "moveto closepath",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto closepath } uappend dump"#,
        "m 1.0 1.0 z @ 1.0 1.0 \n",
    ),
    (
        "ucache may come first",
        r#"newpath { ucache 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a point on the box's corner is inside it",
        r#"newpath { 0 0 10 10 setbbox 10 10 moveto 0 0 lineto } uappend dump"#,
        "m 10.0 10.0 l 0.0 0.0 @ 0.0 0.0 \n",
    ),
    (
        "a negative box",
        r#"newpath { -5 -5 5 5 setbbox -5 -5 moveto 5 5 lineto } uappend dump"#,
        "m -5.0 -5.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a real box",
        r#"newpath { 0.5 0.5 10.5 10.5 setbbox 1 1 moveto 10.5 10.5 lineto } uappend dump"#,
        "m 1.0 1.0 l 10.5 10.5 @ 10.5 10.5 \n",
    ),
    (
        "a zero-size box",
        r#"newpath { 5 5 5 5 setbbox 5 5 moveto 5 5 lineto } uappend dump"#,
        "m 5.0 5.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a literal array of executable names",
        r#"newpath [ 0 0 10 10 /setbbox cvx 1 1 /moveto cvx 5 5 /lineto cvx ] uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a bound procedure",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } bind uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a read-only procedure",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } readonly uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a literal procedure",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } cvlit uappend dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "a user path run with exec leaves its box on the path",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto } exec dump"#,
        "m 1.0 1.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "uappend does not keep the box",
        r#"newpath { 0 0 10 10 setbbox 1 1 moveto } uappend 50 50 lineto dump"#,
        "m 1.0 1.0 l 50.0 50.0 @ 50.0 50.0 \n",
    ),
    (
        "the box is not kept when uappend is the second",
        r#"newpath 50 50 moveto { 0 0 100 100 setbbox 1 1 moveto } uappend 500 500 lineto dump"#,
        "m 1.0 1.0 l 500.0 500.0 @ 500.0 500.0 \n",
    ),
    (
        "arc appends a curve (control points to 1/8)",
        r#"newpath { 0 0 10 10 setbbox 5 5 3 0 90 arc } uappend { q q (m) print } { q q (l) print } { 6 { 8 mul round cvi pa } repeat (c) print } { (z) print } pathforall nl"#,
        "1280 2048 m64 40 64 53 53 64 c\n",
    ),
    (
        "uappend: no setbbox",
        r#"newpath 50 50 moveto 60 60 lineto { { 1 1 moveto 5 5 lineto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a point past the box",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 50 5 lineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a control point past the box",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 20 20 2 2 3 3 curveto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a relative move past the box",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 5 5 moveto 20 0 rlineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a swapped box (x)",
        r#"newpath 50 50 moveto 60 60 lineto { { 10 0 0 10 setbbox 5 5 moveto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a swapped box (y)",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 10 10 0 setbbox 5 5 moveto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an arc past the box",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 4 4 setbbox 0 0 5 0 90 arc } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: ucache last",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto ucache } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: ucache after the box",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox ucache 1 1 moveto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: another operator",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto 0 setgray } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: add",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto 1 add } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: one operand for lineto",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 lineto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a string operand",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto (a) 5 lineto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an extra operand",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 9 1 1 moveto 5 5 lineto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: trailing numbers",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 5 lineto 3 4 } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an operand for closepath",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 7 closepath } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an operand for ucache",
        r#"newpath 50 50 moveto 60 60 lineto { { 3 ucache 0 0 10 10 setbbox } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: five operands for setbbox",
        r#"newpath 50 50 moveto 60 60 lineto { { 7 0 0 10 10 setbbox 1 1 moveto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: three operands for setbbox",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 10 10 setbbox } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: numbers before setbbox",
        r#"newpath 50 50 moveto 60 60 lineto { { 1 2 0 0 10 10 setbbox } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: setbbox twice",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 0 0 10 10 setbbox } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: setbbox after a moveto",
        r#"newpath 50 50 moveto 60 60 lineto { { 1 1 moveto 0 0 10 10 setbbox 5 5 lineto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an empty array",
        r#"newpath 50 50 moveto 60 60 lineto { { } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a lineto with no current point",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 5 5 lineto 7 7 lineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an rlineto with no current point",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 5 5 rlineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an rmoveto first",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 rmoveto 1 1 rlineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an arct with no current point",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 5 1 5 5 2 arct } uappend } T dump"#,
        "ERR rangecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: arcto is not a user path operator",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto 5 1 5 5 2 arcto } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a nested procedure",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox { 1 1 moveto } } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a boolean",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 moveto true } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an executed operator object",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 1 1 //moveto exec } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: literal names in an executable array",
        r#"newpath 50 50 moveto 60 60 lineto { [ 0 0 10 10 /setbbox 1 1 /moveto 5 5 /lineto ] cvx uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a string",
        r#"newpath 50 50 moveto 60 60 lineto { (0 0 10 10 setbbox 1 1 moveto) uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: an integer",
        r#"newpath 50 50 moveto 60 60 lineto { 5 uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a name",
        r#"newpath 50 50 moveto 60 60 lineto { /abc uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: null",
        r#"newpath 50 50 moveto 60 60 lineto { null uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a dictionary",
        r#"newpath 50 50 moveto 60 60 lineto { 1 dict uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: a bad arc operand",
        r#"newpath 50 50 moveto 60 60 lineto { { -10 -10 10 10 setbbox 0 0 (a) 0 90 arc } uappend } T dump"#,
        "ERR typecheck n=1 m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "uappend: nothing on the stack",
        r#"newpath { uappend } T dump"#,
        "ERR stackunderflow n=0 nocp\n",
    ),
    (
        "uappend: a refused path leaves a trailing moveto as it was",
        r#"newpath 5 5 moveto { { 0 0 10 10 setbbox 1 1 moveto 50 50 lineto } uappend } T dump"#,
        "ERR rangecheck n=1 m 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "upath of a path of lines and a curve",
        r#"newpath 10 20 moveto 30 40 lineto 50 20 60 70 80 30 curveto closepath true upath ar"#,
        "x ucache 10.0 20.0 80.0 70.0 setbbox 10.0 20.0 moveto 30.0 40.0 lineto 50.0 20.0 60.0 70.0 80.0 30.0 curveto closepath \n",
    ),
    (
        "upath with no ucache",
        r#"newpath 10 20 moveto 30 40 lineto 50 20 60 70 80 30 curveto closepath false upath ar"#,
        "x 10.0 20.0 80.0 70.0 setbbox 10.0 20.0 moveto 30.0 40.0 lineto 50.0 20.0 60.0 70.0 80.0 30.0 curveto closepath \n",
    ),
    (
        "upath of the empty path",
        r#"newpath false upath ar"#,
        "x 0.0 0.0 0.0 0.0 setbbox \n",
    ),
    (
        "upath of the empty path with ucache",
        r#"newpath true upath ar"#,
        "x ucache 0.0 0.0 0.0 0.0 setbbox \n",
    ),
    (
        "upath of a lone moveto",
        r#"newpath 5 5 moveto false upath ar"#,
        "x 5.0 5.0 5.0 5.0 setbbox 5.0 5.0 moveto \n",
    ),
    (
        "upath of a zero-length line",
        r#"newpath 5 5 moveto 5 5 lineto false upath ar"#,
        "x 5.0 5.0 5.0 5.0 setbbox 5.0 5.0 moveto 5.0 5.0 lineto \n",
    ),
    (
        "upath of two subpaths",
        r#"newpath 10 20 moveto 30 40 lineto closepath 100 100 moveto 120 150 lineto false upath ar"#,
        "x 10.0 20.0 120.0 150.0 setbbox 10.0 20.0 moveto 30.0 40.0 lineto closepath 100.0 100.0 moveto 120.0 150.0 lineto \n",
    ),
    (
        "a lineto after closepath comes back with its moveto",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto closepath 5 5 lineto false upath ar"#,
        "x 5.0 5.0 20.0 20.0 setbbox 10.0 10.0 moveto 20.0 10.0 lineto 20.0 20.0 lineto closepath 10.0 10.0 moveto 5.0 5.0 lineto \n",
    ),
    (
        "upath in a scaled space",
        r#"newpath 2 4 scale 10 20 moveto 30 40 lineto false upath ar"#,
        "x 10.0 20.0 30.0 40.0 setbbox 10.0 20.0 moveto 30.0 40.0 lineto \n",
    ),
    (
        "upath after a scale changes its numbers",
        r#"newpath 10 20 moveto 30 40 lineto 2 4 scale false upath ar"#,
        "x 5.0 5.0 15.0 10.0 setbbox 5.0 5.0 moveto 15.0 10.0 lineto \n",
    ),
    (
        "the array is writable, readable and executable, with real numbers",
        r#"newpath 1 1 moveto 2 2 lineto false upath dup wcheck exch dup rcheck exch dup xcheck exch 1 get type pstack nl clear"#,
        "realtype\ntrue\ntrue\ntrue\n\n",
    ),
    (
        "upath then uappend gives the path back",
        r#"newpath 10 20 moveto 30 40 lineto 50 20 60 70 80 30 curveto closepath 100 100 moveto 120 150 lineto false upath newpath uappend dump"#,
        "m 10.0 20.0 l 30.0 40.0 c 50.0 20.0 60.0 70.0 80.0 30.0 z m 100.0 100.0 l 120.0 150.0 @ 120.0 150.0 \n",
    ),
    (
        "upath then exec gives the path back",
        r#"newpath 10 20 moveto 30 40 lineto 50 20 60 70 80 30 curveto closepath true upath newpath exec dump"#,
        "m 10.0 20.0 l 30.0 40.0 c 50.0 20.0 60.0 70.0 80.0 30.0 z @ 10.0 20.0 \n",
    ),
    (
        "upath: a number is not a flag",
        r#"newpath { 1 upath } T"#,
        "ERR typecheck n=1 ",
    ),
    (
        "upath: no flag",
        r#"newpath { upath } T"#,
        "ERR stackunderflow n=0 ",
    ),
    (
        "upath leaves the path",
        r#"newpath 10 20 moveto 30 40 lineto false upath pop dump"#,
        "m 10.0 20.0 l 30.0 40.0 @ 30.0 40.0 \n",
    ),
    (
        "setbbox: pathbbox answers the box with 3/256 to spare",
        r#"newpath 0 0 100 100 setbbox 10 10 moveto 20 20 lineto pb"#,
        "-3 -3 25603 25603 \n",
    ),
    (
        "setbbox: a box on an empty path",
        r#"newpath 0 0 10 10 setbbox pb"#,
        "-3 -3 2563 2563 \n",
    ),
    (
        "setbbox: a real box",
        r#"newpath 0.5 1.5 2.5 3.5 setbbox pb"#,
        "125 381 643 899 \n",
    ),
    (
        "setbbox: a point past the box",
        r#"newpath 0 0 10 10 setbbox { 20 20 moveto } T dump"#,
        "ERR rangecheck n=2 nocp\n",
    ),
    (
        "setbbox: a lineto past the box",
        r#"newpath 0 0 10 10 setbbox 5 5 moveto { 20 20 lineto } T dump"#,
        "ERR rangecheck n=2 m 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "setbbox: an rmoveto past the box",
        r#"newpath 0 0 10 10 setbbox 5 5 moveto { 20 0 rmoveto } T dump"#,
        "ERR rangecheck n=2 m 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "setbbox: a curveto past the box",
        r#"newpath 0 0 10 10 setbbox 5 5 moveto { 6 6 7 7 20 20 curveto } T dump"#,
        "ERR rangecheck n=6 m 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "setbbox: an arc past the box",
        r#"newpath 0 0 10 10 setbbox { 5 5 20 0 90 arc } T dump"#,
        "ERR rangecheck n=5 nocp\n",
    ),
    (
        "setbbox: a point on the 3/256 rim",
        r#"newpath 0 0 10 10 setbbox 10 10 moveto 10.01171875 10 lineto currentpoint q q nl"#,
        "2560 2563 \n",
    ),
    (
        "setbbox: a point past the 3/256 rim",
        r#"newpath 0 0 10 10 setbbox 10 10 moveto { 10.015625 10 lineto } T dump"#,
        "ERR rangecheck n=2 m 10.0 10.0 @ 10.0 10.0 \n",
    ),
    (
        "setbbox: newpath forgets the box",
        r#"newpath 0 0 10 10 setbbox newpath 50 50 moveto dump"#,
        "m 50.0 50.0 @ 50.0 50.0 \n",
    ),
    (
        "setbbox: grestore gives the old box back",
        r#"newpath gsave 0 0 10 10 setbbox grestore 50 50 moveto dump"#,
        "m 50.0 50.0 @ 50.0 50.0 \n",
    ),
    (
        "setbbox: gsave keeps the box in force",
        r#"newpath 0 0 10 10 setbbox gsave { 50 50 moveto } T dump"#,
        "ERR rangecheck n=2 nocp\n",
    ),
    (
        "setbbox: a box smaller than the path leaves the path",
        r#"newpath 50 50 moveto 60 60 lineto 0 0 10 10 setbbox 5 5 lineto dump"#,
        "m 50.0 50.0 l 60.0 60.0 l 5.0 5.0 @ 5.0 5.0 \n",
    ),
    (
        "setbbox: swapped x",
        r#"newpath { 10 10 0 0 setbbox } T"#,
        "ERR rangecheck n=4 ",
    ),
    (
        "setbbox: swapped y",
        r#"newpath { 0 10 10 0 setbbox } T"#,
        "ERR rangecheck n=4 ",
    ),
    (
        "setbbox: three operands",
        r#"newpath { 0 10 10 setbbox } T"#,
        "ERR stackunderflow n=3 ",
    ),
    (
        "setbbox: a string",
        r#"newpath { 0 0 (a) 10 setbbox } T"#,
        "ERR typecheck n=4 ",
    ),
    (
        "setbbox: the box is in device space (a translate after)",
        r#"newpath 0 0 10 10 setbbox 10 10 translate { 5 5 moveto } T dump"#,
        "ERR rangecheck n=2 nocp\n",
    ),
    (
        "setbbox: the box is mapped by the CTM",
        r#"newpath 2 2 scale 0 0 10 10 setbbox 5 5 moveto 6 6 lineto { 11 5 lineto } T dump"#,
        "ERR rangecheck n=2 m 5.0 5.0 l 6.0 6.0 @ 6.0 6.0 \n",
    ),
    (
        "setbbox leaves the path",
        r#"newpath 1 1 moveto 2 2 lineto 0 0 10 10 setbbox dump"#,
        "m 1.0 1.0 l 2.0 2.0 @ 2.0 2.0 \n",
    ),
    (
        "setbbox: the box on a path is what upath reports",
        r#"newpath 0 0 100 100 setbbox 10 10 moveto 20 20 lineto false upath dup 0 get 256 mul round cvi pa 2 get 256 mul round cvi pa nl"#,
        "-3 25603 \n",
    ),
    (
        "ucache is an operator that does nothing",
        r#"newpath 1 2 ucache count pstack nl clear"#,
        "2\n2\n1\n\n",
    ),
    (
        "infill: pixels of a pixel-aligned box (x edges)",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 8.9 15 pf 9 15 pf 9.99 15 pf 10 15 pf 10.01 15 pf 15 15 pf 19.99 15 pf 20 15 pf 20.01 15 pf 21 15 pf nl"#,
        "FFFTTTTFFF\n",
    ),
    (
        "infill: pixels of a pixel-aligned box (y edges: the page's y is flipped)",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 15 8.9 pf 15 9.99 pf 15 10 pf 15 10.01 pf 15 15 pf 15 20 pf 15 20.01 pf 15 21 pf nl"#,
        "FFFTTTFF\n",
    ),
    (
        "infill: the box's corners",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 10 10 pf 10 10.01 pf 19.99 20 pf 20 20 pf 9.99 9.99 pf nl"#,
        "FTTFF\n",
    ),
    (
        "infill: an open path is closed for the fill",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 15 12 pf 12 18 pf 18 12 pf nl"#,
        "TFT\n",
    ),
    ("infill: no path", r#"newpath 5 5 pf nl"#, "F\n"),
    (
        "infill: a lone moveto",
        r#"newpath 5 5 moveto 5 5 pf nl"#,
        "F\n",
    ),
    (
        "infill and ineofill on a ring wound one way",
        r#"newpath 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath 20 20 pf 20 20 pe 5 20 pf 5 20 pe 45 20 pf 45 20 pe nl"#,
        "TFTTFF\n",
    ),
    (
        "infill and ineofill on a ring wound back",
        r#"newpath 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 10 30 lineto 30 30 lineto 30 10 lineto closepath 20 20 pf 20 20 pe 5 20 pf 5 20 pe nl"#,
        "FFTT\n",
    ),
    (
        "infill: a triangle's slanted edge (any part of the pixel counts)",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto closepath 18 12 pf 12 18 pf 14.5 14.5 pf 14.9 14.5 pf 15.2 14.5 pf 14.5 15.5 pf nl"#,
        "TFTTTF\n",
    ),
    (
        "infill: a curve",
        r#"newpath 10 10 moveto 10 30 30 30 30 10 curveto closepath 20 15 pf 20 25 pf 20 35 pf 12 11 pf 11 25 pf 29 25 pf 5 5 pf nl"#,
        "TTFTFFF\n",
    ),
    (
        "infill: a circle",
        r#"newpath 50 50 20 0 360 arc closepath 50 50 pf 65 50 pf 69.5 50 pf 71.5 50 pf 50 69 pf 50 71.5 pf 64 64 pf 66 66 pf nl"#,
        "TTTFTFTF\n",
    ),
    (
        "infill: the box is not painted by being tested",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 15 15 pf nl dump"#,
        "T\nm 10.0 10.0 l 20.0 10.0 l 20.0 20.0 l 10.0 20.0 z @ 10.0 10.0 \n",
    ),
    (
        "infill leaves the operand stack with just the answer",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 1 2 3 4 infill count pstack nl clear"#,
        "3\nfalse\n2\n1\n\n",
    ),
    (
        "infill ignores the clip",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath gsave 0 0 12 12 rectclip newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 18 18 pf 11 11 pf grestore nl"#,
        "TT\n",
    ),
    (
        "infill under a scale",
        r#"newpath 3 3 scale 1 1 moveto 3 1 lineto 3 3 lineto 1 3 lineto closepath 2 2 pf 3.2 2 pf 3.4 2 pf 0.5 2 pf nl"#,
        "TFFF\n",
    ),
    (
        "infill under a rotation",
        r#"newpath 45 rotate 0 0 moveto 20 0 lineto 20 20 lineto 0 20 lineto closepath 10 10 pf 14 14 pf 1 -1 pf 0.5 -0.1 pf nl"#,
        "TTFT\n",
    ),
    (
        "infill under a translate",
        r#"newpath 100 200 translate 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 15 15 pf 115 215 pf nl"#,
        "TF\n",
    ),
    (
        "infill: a point far away",
        r#"newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 1e20 1e20 pf -1e20 1e20 pf 1e30 0 pf nl"#,
        "FFF\n",
    ),
    (
        "infill: a path far from the origin",
        r#"newpath -100 -100 moveto -50 -100 lineto -50 -50 lineto closepath -60 -90 pf -90 -60 pf nl"#,
        "TF\n",
    ),
    (
        "infill: a point on a scaled-to-nothing page",
        r#"newpath 0 0 scale 1 1 moveto 1 1 pf nl"#,
        "F\n",
    ),
    (
        "infill and nulldevice",
        r#"gsave nulldevice newpath 10 10 moveto 20 10 lineto 20 20 lineto 10 20 lineto closepath 15 15 pf grestore nl"#,
        "T\n",
    ),
    (
        "infill: operand errors",
        r#"newpath { 15 infill } T { (a) 15 infill } T { 15 (a) infill } T { infill } T { [1 2] infill } T { 5 infill } T nl"#,
        "ERR stackunderflow n=1 ERR typecheck n=2 ERR typecheck n=2 ERR stackunderflow n=0 ERR typecheck n=1 ERR stackunderflow n=1 \n",
    ),
    (
        "ineofill: operand errors",
        r#"newpath { 15 ineofill } T { (a) 15 ineofill } T { ineofill } T nl"#,
        "ERR stackunderflow n=1 ERR typecheck n=2 ERR stackunderflow n=0 \n",
    ),
    (
        "ineofill on a path that overlaps itself",
        r#"newpath 0 0 moveto 20 0 lineto 20 20 lineto 0 20 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath 5 5 pe 15 15 pe 25 25 pe 15 15 pf 5 25 pe 25 5 pe nl"#,
        "TFTTFF\n",
    ),
    (
        "instroke: a thick line (points well in and out of the band)",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 30 10 ps 30 11 ps 30 9 ps 30 14 ps 30 6 ps 30 13 ps 30 7 ps nl"#,
        "TTTFFFF\n",
    ),
    (
        "instroke: along the line",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 20 10 ps 49 10 ps 9 10 ps 51 10 ps 52 10 ps 5 10 ps nl"#,
        "TTFFFF\n",
    ),
    (
        "instroke: butt caps stop at the end",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 0 setlinecap 9.5 10 ps 10.5 10 ps 8 10 ps nl"#,
        "FTF\n",
    ),
    (
        "instroke: square caps reach half a width past",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 2 setlinecap 8.5 10 ps 11 10 ps 7 10 ps 51.5 10 ps 53 10 ps nl"#,
        "TTFTF\n",
    ),
    (
        "instroke: round caps",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 1 setlinecap 8.5 10 ps 8.5 11.5 ps 7 10 ps 51.5 10 ps 53 10 ps 9 11.9 ps nl"#,
        "TTFTFT\n",
    ),
    (
        "instroke: dashes",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth [4 4] 0 setdash 12 10 ps 17 10 ps 21 10 ps 25 10 ps 29 10 ps nl"#,
        "TFTFT\n",
    ),
    (
        "instroke: dashes with a phase",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth [4 4] 2 setdash 11 10 ps 15 10 ps 19 10 ps 23 10 ps nl"#,
        "TFTF\n",
    ),
    (
        "instroke: a hairline",
        r#"newpath 10 10 moveto 50 10 lineto 0 setlinewidth 30 10 ps 30 12 ps 30 13 ps nl"#,
        "TFF\n",
    ),
    (
        "instroke: a closed square's edges and its hollow middle",
        r#"newpath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath 2 setlinewidth 20 10 ps 20 20 ps 10 20 ps 30 20 ps 20 30 ps 20 14 ps 20 5 ps nl"#,
        "TFTTTFF\n",
    ),
    (
        "instroke: miter, round and bevel corners",
        r#"newpath 10 10 moveto 30 10 lineto 30 30 lineto 4 setlinewidth 31.5 8.5 ps 1 setlinejoin 31.5 8.5 ps 31.9 8.1 ps 2 setlinejoin 31.5 8.5 ps 30.5 9.5 ps 31 8 ps 0 setlinejoin 32 8 ps 32.5 8 ps nl"#,
        "TTTFTFFF\n",
    ),
    (
        "instroke: a round join",
        r#"newpath 10 10 moveto 30 10 lineto 30 30 lineto 4 setlinewidth 1 setlinejoin 31.2 8.8 ps 31.8 8.2 ps 0 setlinejoin 31.2 8.8 ps nl"#,
        "TTT\n",
    ),
    (
        "instroke: a curve",
        r#"newpath 10 10 moveto 10 30 30 30 30 10 curveto 2 setlinewidth 20 25 ps 20 20 ps 20 15 ps 20 30 ps 20 22.5 ps 10 20 ps 9 20 ps nl"#,
        "TFFFFTF\n",
    ),
    ("instroke: no path", r#"newpath 5 5 ps nl"#, "F\n"),
    (
        "instroke: a lone moveto draws nothing",
        r#"newpath 5 5 moveto 1 setlinewidth 5 5 ps nl"#,
        "F\n",
    ),
    (
        "instroke: a zero-length line is a dot under round caps",
        r#"newpath 5 5 moveto 5 5 lineto 4 setlinewidth 1 setlinecap 5 5 ps 6 5 ps 8 5 ps 0 setlinecap 5 5 ps nl"#,
        "TTFF\n",
    ),
    (
        "instroke: a zero-length subpath under square caps is nothing",
        r#"newpath 5 5 moveto 5 5 lineto 4 setlinewidth 2 setlinecap 5 5 ps 6 5 ps nl"#,
        "FF\n",
    ),
    (
        "instroke ignores the current point and path state",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 30 10 ps nl dump"#,
        "T\nm 10.0 10.0 l 50.0 10.0 @ 50.0 10.0 \n",
    ),
    (
        "instroke under a scale (the width scales)",
        r#"newpath 2 2 scale 5 5 moveto 25 5 lineto 2 setlinewidth 15 5 ps 15 6 ps 15 7.5 ps 15 3 ps nl"#,
        "TTFF\n",
    ),
    (
        "instroke under a nonuniform scale",
        r#"newpath 1 3 scale 10 10 moveto 50 10 lineto 2 setlinewidth 30 10 ps 30 11 ps 30 12 ps 30 8.5 ps 30 6.8 ps nl"#,
        "TTFFF\n",
    ),
    (
        "instroke ignores the clip",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth gsave 0 0 12 12 rectclip newpath 10 10 moveto 50 10 lineto 30 10 ps grestore nl"#,
        "T\n",
    ),
    (
        "instroke: far away",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 1e20 1e20 ps -1e20 1e20 ps nl"#,
        "FF\n",
    ),
    (
        "instroke: operand errors",
        r#"newpath { 15 instroke } T { (a) 15 instroke } T { instroke } T nl"#,
        "ERR stackunderflow n=1 ERR typecheck n=2 ERR stackunderflow n=0 \n",
    ),
    (
        "instroke under a singular matrix is false",
        r#"newpath 10 10 moveto 50 10 lineto 4 setlinewidth 0 0 scale 30 10 ps nl"#,
        "F\n",
    ),
    (
        "inufill: a point against a user path",
        r#"newpath 15 15 SQ pu 25 15 SQ pu 10.5 10.5 SQ pu 9.5 10.5 SQ pu 15 21 SQ pu nl"#,
        "TFTFF\n",
    ),
    (
        "inueofill and inufill on a ring",
        r#"newpath 20 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath } pu 20 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath } pue 5 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath } pu 5 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 30 10 lineto 30 30 lineto 10 30 lineto closepath } pue 20 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 10 30 lineto 30 30 lineto 30 10 lineto closepath } pu 20 20 { 0 0 100 100 setbbox 0 0 moveto 40 0 lineto 40 40 lineto 0 40 lineto closepath 10 10 moveto 10 30 lineto 30 30 lineto 30 10 lineto closepath } pue nl"#,
        "TFTTFF\n",
    ),
    (
        "inufill does not touch the current path",
        r#"newpath 1 1 moveto 2 2 lineto 15 15 SQ pu nl dump"#,
        "T\nm 1.0 1.0 l 2.0 2.0 @ 2.0 2.0 \n",
    ),
    (
        "inufill ignores the current path",
        r#"newpath 100 100 moveto 150 150 lineto 150 100 lineto 15 15 SQ pu nl"#,
        "T\n",
    ),
    (
        "inufill uses the current user space",
        r#"newpath 2 2 scale 15 15 SQ pu 7.5 7.5 SQ pu 30 30 SQ pu nl"#,
        "TFF\n",
    ),
    (
        "inufill: errors",
        r#"newpath { 15 15 5 inufill } T { 15 15 (a) inufill } T { 15 SQ inufill } T { SQ inufill } T { 15 15 { 0 0 1 1 setbbox 5 5 moveto } inufill } T { 15 15 { 5 5 moveto } inufill } T { (a) 15 SQ inufill } T nl"#,
        "ERR typecheck n=3 ERR typecheck n=3 ERR stackunderflow n=2 ERR stackunderflow n=1 ERR rangecheck n=3 ERR typecheck n=3 ERR typecheck n=3 \n",
    ),
    (
        "inueofill: errors",
        r#"newpath { 15 15 5 inueofill } T { 15 (a) SQ inueofill } T nl"#,
        "ERR typecheck n=3 ERR typecheck n=3 \n",
    ),
    (
        "inustroke: a point against a user path's stroke",
        r#"newpath 4 setlinewidth 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus 30 11 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus 30 14 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus 30 6 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus 30 20 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus nl"#,
        "TTFFF\n",
    ),
    (
        "inustroke ignores the current path",
        r#"newpath 100 100 moveto 150 150 lineto 4 setlinewidth 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus nl"#,
        "T\n",
    ),
    (
        "inustroke leaves the current path alone",
        r#"newpath 1 1 moveto 2 2 lineto 4 setlinewidth 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus nl dump"#,
        "T\nm 1.0 1.0 l 2.0 2.0 @ 2.0 2.0 \n",
    ),
    (
        "inustroke with a matrix: the pen is shaped, the point is in the old space",
        r#"newpath 4 setlinewidth 30 15.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 3 0 0] pus 30 17 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 3 0 0] pus 30 5.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 3 0 0] pus 30 12.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } pus 30 12.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 3 0 0] pus nl"#,
        "TFTFT\n",
    ),
    (
        "inustroke with a matrix and a scale",
        r#"newpath 4 setlinewidth 30 17.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [2 0 0 2 0 0] pus 30 12 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [2 0 0 2 0 0] pus 30 13.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [2 0 0 2 0 0] pus 30 15 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [2 0 0 2 0 0] pus nl"#,
        "FTTF\n",
    ),
    (
        "inustroke with an identity matrix",
        r#"newpath 4 setlinewidth 30 11.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 1 0 0] pus 30 14 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 1 0 0] pus nl"#,
        "TF\n",
    ),
    (
        "inustroke: the matrix does not move the path",
        r#"newpath 4 setlinewidth 30 11.5 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 0 0 1 100 100] pus nl"#,
        "T\n",
    ),
    (
        "inustroke: errors",
        r#"newpath { 30 10 5 inustroke } T { 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } 5 inustroke } T { 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 2 3 4 5 (a)] inustroke } T { 30 10 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [1 2 3] inustroke } T { 30 10 inustroke } T nl"#,
        "ERR typecheck n=3 ERR typecheck n=4 ERR typecheck n=4 ERR typecheck n=4 ERR typecheck n=2 \n",
    ),
    (
        "inustroke with a singular matrix",
        r#"newpath 4 setlinewidth 30 11 { 0 0 100 100 setbbox 10 10 moveto 50 10 lineto } [0 0 0 0 0 0] pus nl"#,
        "F\n",
    ),
    (
        "ufill leaves the current path and point",
        r#"newpath 50 50 moveto 60 60 lineto SQ ufill dump"#,
        "m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "ueofill leaves the current path and point",
        r#"newpath 50 50 moveto 60 60 lineto SQ ueofill dump"#,
        "m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "ustroke leaves the current path and point",
        r#"newpath 50 50 moveto 60 60 lineto SQ ustroke dump"#,
        "m 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "ustroke with a matrix leaves the CTM and the path",
        r#"newpath 50 50 moveto 60 60 lineto SQ [2 0 0 1 0 0] ustroke matrix currentmatrix { pa } forall nl dump"#,
        "1.0 0.0 0.0 -1.0 0.0 792.0 \nm 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "ufill leaves the CTM",
        r#"newpath 2 2 scale SQ ufill matrix currentmatrix { pa } forall nl"#,
        "2.0 0.0 0.0 -2.0 0.0 792.0 \n",
    ),
    (
        "ufill of the empty path",
        r#"newpath { 0 0 10 10 setbbox } ufill count pstack nl dump"#,
        "0\n\nnocp\n",
    ),
    (
        "ufill takes only the user path",
        r#"newpath SQ ufill count pstack nl"#,
        "0\n\n",
    ),
    (
        "ufill: errors leave the stack and the path",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 20 20 moveto } ufill } T { 5 ufill } T { ufill } T { { 0 0 10 10 setbbox 20 20 moveto } ustroke } T { SQ 5 ustroke } T { SQ [1 2 3] ustroke } T { SQ [1 2 3 4 5 (a)] ustroke } T nl dump"#,
        "ERR rangecheck n=1 ERR typecheck n=1 ERR stackunderflow n=0 ERR rangecheck n=1 ERR typecheck n=2 ERR typecheck n=2 ERR typecheck n=2 \nm 50.0 50.0 l 60.0 60.0 @ 60.0 60.0 \n",
    ),
    (
        "ustrokepath replaces the current path with the outline's box",
        r#"newpath 50 50 moveto 60 60 lineto 1 setlinewidth SQ ustrokepath pb"#,
        "2432 2432 5248 5248 \n",
    ),
    (
        "ustrokepath at width 2",
        r#"newpath 2 setlinewidth { 0 0 100 100 setbbox 10 10 moveto 20 10 lineto } ustrokepath pb"#,
        "2560 2304 5120 2816 \n",
    ),
    (
        "ustrokepath: round caps",
        r#"newpath 2 setlinewidth 1 setlinecap { 0 0 100 100 setbbox 10 10 moveto 20 10 lineto } ustrokepath pb"#,
        "2304 2304 5376 2816 \n",
    ),
    (
        "ustrokepath leaves the CTM",
        r#"newpath 2 setlinewidth SQ ustrokepath matrix currentmatrix { pa } forall nl"#,
        "1.0 0.0 0.0 -1.0 0.0 792.0 \n",
    ),
    (
        "ustrokepath with a translating matrix",
        r#"newpath 2 setlinewidth { 0 0 100 100 setbbox 10 10 moveto 20 10 lineto } [1 0 0 1 5 7] ustrokepath pb"#,
        "2560 2304 5120 2816 \n",
    ),
    (
        "ustrokepath: errors leave the path",
        r#"newpath 50 50 moveto 60 60 lineto { { 0 0 10 10 setbbox 20 20 moveto } ustrokepath } T { 5 ustrokepath } T { ustrokepath } T nl 1 setlinewidth pb"#,
        "ERR rangecheck n=1 ERR typecheck n=1 ERR stackunderflow n=0 \n12800 12800 15360 15360 \n",
    ),
];

/// What pscat prints for `src` after the prelude, or the error that stopped
/// it.
fn pscat_out(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    match it.run_str(&format!("{PRELUDE}\n{src}\n")) {
        Ok(()) => String::from_utf8_lossy(it.captured_stdout()).into_owned(),
        Err(e) => format!(
            "<stopped: {}> after {}",
            it.error_report(&e),
            String::from_utf8_lossy(it.captured_stdout())
        ),
    }
}

#[test]
fn user_paths_and_hit_tests_match_the_pinned_ghostscript_output() {
    let mut wrong = Vec::new();
    for (what, src, want) in CASES {
        let got = pscat_out(src);
        if &got != want {
            wrong.push(format!(
                "{what}\n  program: {src}\n  gs:      {want:?}\n  pscat:   {got:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} cases differ:\n{}",
        wrong.len(),
        CASES.len(),
        wrong.join("\n")
    );
}

/// The pinned output is gs's own.
#[test]
fn ghostscript_prints_the_pinned_output() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("pscat-upath-gs");
    for (i, (what, src, want)) in CASES.iter().enumerate() {
        let file = dir.join(format!("c{i}.ps"));
        std::fs::write(&file, format!("{PRELUDE}\n{src}\n")).expect("write");
        let out = std::process::Command::new("gs")
            .args(["-q", "-dNOSAFER", "-dNODISPLAY", "-dBATCH", "-dNOPAUSE"])
            .arg(&file)
            .stdin(std::process::Stdio::null())
            .output_deadline()
            .expect("run gs");
        let got = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(&got, want, "gs, {what}: {src}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
