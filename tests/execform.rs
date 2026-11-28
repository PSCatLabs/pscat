//! `execform` and the `Form` resource category (#1271).
//!
//! `CASES` is run by pscat and, when `gs` is installed, by Ghostscript
//! 10.07.1 (`-dNODISPLAY -dNOSAFER`), and both must print the pinned line:
//! `!errorname operand-count` for a failure, the operand stack for a
//! success (`D` is a dictionary). The table is the specification of the
//! order `execform` checks a form in (`Resource/Init/gs_lev2.ps`
//! `.execform1`: `/FormType`, `/Matrix` and `concat`, `/BBox` and `rectclip`,
//! `/PaintProc`, then `/Implementation`) and of what it leaves on the stack.
//!
//! Where pscat deliberately differs from gs, the case is not in the table
//! and has a test of its own below:
//!
//! * After an error gs puts back the *number* of operands the form had, not
//!   the operands: an error raised after its internal `matrix currentmatrix`
//!   leaves the saved CTM where the form was. pscat restores the operands
//!   themselves (`errors_put_every_operand_back`).
//! * gs's clean-up of a PaintProc that leaves junk scans down for a
//!   dictionary with `/Implementation`, and a dictionary without one leaves
//!   counters on the stack. pscat skips such a dictionary.
//! * gs names `--.execform1--` as the offending command for most errors,
//!   an artefact of its operator being a PostScript procedure. pscat names
//!   `execform`.

use std::process::Command;

mod common;

use common::CommandDeadline as _;
use pscat::{Interp, Limits, PsError};
use tiny_skia::Pixmap;

/// `(name, body, what gs prints)`. `initgraphics` and a cleared stack
/// start each body.
const CASES: &[(&str, &str, &str)] = &[
    ("empty stack", r#"execform"#, "!stackunderflow 0"),
    ("an integer", r#"1 execform"#, "!typecheck 1"),
    ("a name", r#"/foo execform"#, "!typecheck 1"),
    (
        "operands under a bad top are kept",
        r#"1 2 /foo execform"#,
        "!typecheck 3",
    ),
    ("an array", r#"[1 2] execform"#, "!typecheck 1"),
    ("a string", r#"(abc) execform"#, "!typecheck 1"),
    ("null", r#"null execform"#, "!typecheck 1"),
    ("an empty dict", r#"1 dict execform"#, "!undefined 1"),
    (
        "a good form",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "a good form leaves the operands under it",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "7 8",
    ),
    (
        "FormType real 1.0",
        r#"<< /FormType 1.0 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "FormType 1e0",
        r#"<< /FormType 1e0 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "FormType 0",
        r#"<< /FormType 0 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType 2",
        r#"<< /FormType 2 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType -1",
        r#"<< /FormType -1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType 1.5",
        r#"<< /FormType 1.5 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType a name",
        r#"<< /FormType /a /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType a string",
        r#"<< /FormType (1) /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType null",
        r#"<< /FormType null /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType true",
        r#"<< /FormType true /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType an array",
        r#"<< /FormType [1] /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "FormType huge",
        r#"<< /FormType 1000000000000 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "no FormType",
        r#"<< /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "no BBox",
        r#"<< /FormType 1 /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "no Matrix",
        r#"<< /FormType 1 /BBox [0 0 10 10] /PaintProc {pop} >> execform"#,
        "!undefined 1",
    ),
    (
        "no PaintProc",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] >> execform"#,
        "!undefined 1",
    ),
    (
        "Matrix 0 numbers",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [] /PaintProc {pop} >> execform"#,
        "!rangecheck 1",
    ),
    (
        "Matrix 5 numbers",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0] /PaintProc {pop} >> execform"#,
        "!rangecheck 1",
    ),
    (
        "Matrix 7 numbers",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0 0] /PaintProc {pop} >> execform"#,
        "!rangecheck 1",
    ),
    (
        "Matrix a string",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix (abcdef) /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix a name",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix /a /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix null",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix null /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix a non-number",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 (a)] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix null element",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 null] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix singular",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [0 0 0 0 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "Matrix scaled",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [2 0 0 2 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "Matrix a procedure",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix {1 0 0 1 0 0} /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "Matrix rotated",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [0 1 -1 0 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "Matrix a dict",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix << >> /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "Matrix an integer",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix 5 /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox 0 numbers",
        r#"<< /FormType 1 /BBox [] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!stackunderflow 1",
    ),
    (
        "BBox 1 number",
        r#"<< /FormType 1 /BBox [0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!stackunderflow 1",
    ),
    (
        "BBox 2 numbers",
        r#"<< /FormType 1 /BBox [0 0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox 3 numbers",
        r#"<< /FormType 1 /BBox [0 0 1] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox 5 numbers",
        r#"<< /FormType 1 /BBox [0 0 1 1 7] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox 6 numbers",
        r#"<< /FormType 1 /BBox [0 0 1 1 7 8] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox a string",
        r#"<< /FormType 1 /BBox (abcd) /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox a name",
        r#"<< /FormType 1 /BBox /foo /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox an integer",
        r#"<< /FormType 1 /BBox 5 /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox null",
        r#"<< /FormType 1 /BBox null /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox a non-number",
        r#"<< /FormType 1 /BBox [0 0 (a) 1] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox a name element",
        r#"<< /FormType 1 /BBox [/a 0 1 1] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox a procedure",
        r#"<< /FormType 1 /BBox {0 0 1 1} /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox packed",
        r#"<< /FormType 1 /BBox [0 0 1 1] cvx /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox inverted",
        r#"<< /FormType 1 /BBox [10 10 0 0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox real",
        r#"<< /FormType 1 /BBox [0.5 0.5 9.5 9.5] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox a dict",
        r#"<< /FormType 1 /BBox << >> /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox null element",
        r#"<< /FormType 1 /BBox [0 0 null 1] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox boolean element",
        r#"<< /FormType 1 /BBox [0 0 true 1] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 1",
    ),
    (
        "BBox empty",
        r#"<< /FormType 1 /BBox [0 0 0 0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox negative",
        r#"<< /FormType 1 /BBox [-5 -5 5 5] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "",
    ),
    (
        "BBox 0 numbers with operands under",
        r#"1 2 << /FormType 1 /BBox [] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 3",
    ),
    (
        "BBox 1 number with an operand under",
        r#"1 << /FormType 1 /BBox [0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 2",
    ),
    (
        "BBox 1 number with two operands under",
        r#"1 2 << /FormType 1 /BBox [0] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform"#,
        "!typecheck 3",
    ),
    (
        "PaintProc a name",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc /foo >> execform"#,
        "D",
    ),
    (
        "PaintProc an executable name",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc /nonesuch cvx >> execform"#,
        "!undefined 1",
    ),
    (
        "PaintProc a string",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc (1 2 add) >> execform"#,
        "D",
    ),
    (
        "PaintProc an executable string",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc (1 pop) cvx >> execform"#,
        "D",
    ),
    (
        "PaintProc an array",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc [1 2] >> execform"#,
        "D",
    ),
    (
        "PaintProc null",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc null >> execform"#,
        "D",
    ),
    (
        "PaintProc an integer",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc 1 >> execform"#,
        "D",
    ),
    (
        "PaintProc empty",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {} >> execform"#,
        "D",
    ),
    (
        "PaintProc pushes junk",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {1 2 3} >> execform"#,
        "D",
    ),
    (
        "PaintProc a literal procedure",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc [/pop] cvx >> execform"#,
        "D",
    ),
    (
        "a PaintProc that leaves a value",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop 99} >> execform"#,
        "7 8",
    ),
    (
        "a PaintProc that adds",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop 1 2 add} >> execform"#,
        "7 8",
    ),
    (
        "a PaintProc that fails",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop 1 0 div} >> execform"#,
        "!undefinedresult 3",
    ),
    (
        "a PaintProc that fails with an operand under",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop pop 1 0 div} >> execform"#,
        "!undefinedresult 3",
    ),
    (
        "a PaintProc that stops",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop stop} >> execform"#,
        "!stop 3",
    ),
    (
        "a PaintProc that exits a loop",
        r#"7 8 {<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop exit} >> execform} loop"#,
        "!invalidexit 3",
    ),
    (
        "a PaintProc that exits with no loop",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop exit} >> execform"#,
        "!invalidexit 1",
    ),
    (
        "the PaintProc sees the form twice",
        r#"7 8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {count 1 index 2 index eq 3 1 roll pop pop} >> execform"#,
        "7 8",
    ),
    (
        "the PaintProc gets the form dict",
        r#"7 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {/FormType get 7 add 1 sub} >> execform"#,
        "7",
    ),
    (
        "execform adds /Implementation null",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> dup execform /Implementation get type"#,
        "nulltype",
    ),
    (
        "Implementation is added before the PaintProc runs",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {dup /Implementation known exch pop} >> execform"#,
        "",
    ),
    (
        "Implementation already null",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} /Implementation null >> execform"#,
        "",
    ),
    (
        "Implementation an integer",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} /Implementation 5 >> execform"#,
        "!typecheck 1",
    ),
    (
        "a form twice",
        r#"/f << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def f execform f execform"#,
        "",
    ),
    (
        "XUID is ignored",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} /XUID [1 2 3] >> execform"#,
        "",
    ),
    (
        "a form defined as a resource",
        r#"/f << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f /Form findresource execform"#,
        "",
    ),
    (
        "the Matrix reaches the PaintProc",
        r#"matrix setmatrix 2 3 scale << /FormType 1 /BBox [0 0 10 10] /Matrix [5 0 0 7 1 2] /PaintProc {pop pm} >> execform"#,
        "10 0 0 21 2 6",
    ),
    (
        "the CTM comes back",
        r#"matrix setmatrix 2 3 scale << /FormType 1 /BBox [0 0 10 10] /Matrix [5 0 0 7 0 0] /PaintProc {pop} >> execform pm"#,
        "2 0 0 3 0 0",
    ),
    (
        "the clip comes back",
        r#"matrix setmatrix 0 0 100 100 rectclip << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform pb"#,
        "100 100 0 0",
    ),
    (
        "the clip inside is the BBox",
        r#"matrix setmatrix << /FormType 1 /BBox [2 3 12 15] /Matrix [1 0 0 1 0 0] /PaintProc {pop pb} >> execform"#,
        "15 12 3 2",
    ),
    (
        "the clip inside is the BBox under a Matrix",
        r#"matrix setmatrix << /FormType 1 /BBox [2 3 12 15] /Matrix [2 0 0 2 10 10] /PaintProc {pop pb} >> execform"#,
        "15 12 3 2",
    ),
    (
        "the path comes back",
        r#"matrix setmatrix 10 10 moveto 20 20 lineto << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform currentpoint exch cvi 4 string cvs print ( ) print cvi 4 string cvs print"#,
        "20 20",
    ),
    (
        "a colour set inside is undone",
        r#"0.5 setgray << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop 1 setgray} >> execform currentgray 4 string cvs print"#,
        "0.5",
    ),
    (
        "a gsave left open stays",
        r#"matrix setmatrix 2 2 scale << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop gsave 5 5 scale} >> execform pm grestore pm"#,
        "2 0 0 2 0 0 2 0 0 2 0 0",
    ),
    (
        "a grestore inside takes the caller's state",
        r#"matrix setmatrix gsave 2 2 scale << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop grestore} >> execform pm"#,
        "1 0 0 1 0 0",
    ),
    (
        "defineresource /Form: a dict",
        r#"/f1 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource dup /FormType get b pop"#,
        "1",
    ),
    (
        "defineresource /Form: the key is the instance's",
        r#"/f2 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f2 /Form findresource /FormType get b"#,
        "1",
    ),
    (
        "defineresource /Form: an integer",
        r#"/f3 1 /Form defineresource"#,
        "!typecheck 3",
    ),
    (
        "defineresource /Form: an array",
        r#"/f3 [1] /Form defineresource"#,
        "!typecheck 3",
    ),
    (
        "defineresource /Form: a string",
        r#"/f3 (a) /Form defineresource"#,
        "!typecheck 3",
    ),
    (
        "defineresource /Form: a procedure",
        r#"/f3 {pop} /Form defineresource"#,
        "!typecheck 3",
    ),
    (
        "defineresource /Form: null",
        r#"/f3 null /Form defineresource"#,
        "!typecheck 3",
    ),
    (
        "defineresource /Form: a name key is kept",
        r#"/f4 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f4 /Form resourcestatus"#,
        "0 -1 true",
    ),
    (
        "defineresource /Form: a string key",
        r#"(f5) << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f5 /Form resourcestatus"#,
        "0 -1 true",
    ),
    (
        "defineresource /Form: an integer key",
        r#"5 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop 5 /Form resourcestatus"#,
        "0 -1 true",
    ),
    (
        "defineresource /Form: replaces",
        r#"/f6 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f6 << /FormType 1.0 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f6 /Form findresource /FormType get cvi b"#,
        "1",
    ),
    (
        "findresource /Form: missing",
        r#"/nonesuchform /Form findresource"#,
        "!undefinedresource 2",
    ),
    (
        "resourcestatus /Form: missing",
        r#"/nonesuchform /Form resourcestatus"#,
        "false",
    ),
    (
        "undefineresource /Form",
        r#"/f7 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f7 /Form undefineresource /f7 /Form resourcestatus"#,
        "false",
    ),
    (
        "undefineresource /Form: missing",
        r#"/nonesuchform /Form undefineresource"#,
        "",
    ),
    (
        "findresource /Form then execform",
        r#"/f8 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f8 /Form findresource execform"#,
        "",
    ),
    (
        "findresource /Form then execform twice",
        r#"7 /f9 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /f9 /Form findresource dup execform execform"#,
        "7",
    ),
    (
        "a form is found by the key it was defined under",
        r#"/fa << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> /Form defineresource pop /fb /fa /Form findresource /Form defineresource pop /fb /Form findresource /fa /Form findresource eq b"#,
        "true",
    ),
    (
        "a save left open inside",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop save pop} >> execform"#,
        "",
    ),
    (
        "a save and restore inside",
        r#"<< /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop save restore} >> execform"#,
        "",
    ),
    (
        "a restore of an outer save made before the form",
        r#"save /sv exch def /f9 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def f9 /Matrix get pop sv restore"#,
        "",
    ),
    (
        "execform inside a save",
        r#"save << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> execform restore"#,
        "",
    ),
    (
        "execform inside a save restores the /Implementation key",
        r#"/ff << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def save ff execform restore ff /Implementation known b"#,
        "false",
    ),
    (
        "execform outside a save keeps the /Implementation key",
        r#"/fg << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def fg execform fg /Implementation known b"#,
        "true",
    ),
    (
        "a PaintProc that runs a form",
        r#"/in << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop in execform} >> execform"#,
        "",
    ),
    (
        "a PaintProc that runs a form and leaves its own",
        r#"7 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop in execform 9} >> execform"#,
        "7",
    ),
    (
        "a PaintProc that runs a form that fails",
        r#"/in << /FormType 1 /BBox [0 0 10 10] /Matrix [1] /PaintProc {pop} >> def 7 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop in execform} >> execform"#,
        "!rangecheck 2",
    ),
    (
        "a PaintProc that catches a failing form",
        r#"/in << /FormType 1 /BBox [0 0 10 10] /Matrix [1] /PaintProc {pop} >> def 7 << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop {in execform} stopped pop} >> execform"#,
        "7",
    ),
];

/// Plain PostScript, valid in pscat and in gs.
const PRELUDE: &str = "/b { dup type /booleantype eq { 8 string cvs } if \
dup type /integertype eq { 8 string cvs } if \
dup type /nametype eq { 32 string cvs } if \
print ( ) print } def \
/dump { count array astore { dup type /dicttype eq { pop (D) } \
{ dup type /integertype eq { 8 string cvs } { dup type /nametype eq { 16 string cvs } \
{ dup type /booleantype eq { 8 string cvs } { dup type /stringtype eq {} { pop (?) } ifelse } ifelse } ifelse } ifelse } ifelse \
print ( ) print } forall } def \
/pm { matrix currentmatrix { cvi 6 string cvs print ( ) print } forall } def \
/pb { clippath pathbbox 4 { cvi 6 string cvs print ( ) print } repeat } def\n";

fn program() -> String {
    let mut src = String::from(PRELUDE);
    for (name, body, _) in CASES {
        src.push_str(&format!(
            "({name}: ) print {{ initgraphics clear $error /newerror false put {body} dump }} stopped \
             {{ (!) print $error /newerror get {{ $error /errorname get b }} {{ (stop) print ( ) print }} ifelse \
             count b }} if clear (\\n) print\n"
        ));
    }
    src
}

fn tidy(out: &str) -> Vec<String> {
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect()
}

fn want() -> Vec<String> {
    CASES
        .iter()
        .map(|(name, _, out)| format!("{name}: {out}").trim_end().to_string())
        .collect()
}

#[test]
fn pscat_prints_the_lines_gs_printed() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&program())
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    let got = tidy(&String::from_utf8_lossy(it.captured_stdout()));
    assert_eq!(got, want());
}

#[test]
fn ghostscript_prints_the_pinned_lines() {
    if !common::gs_available() {
        return;
    }
    let dir = common::scratch_dir("execform_gs");
    let ps = dir.join("cases.ps");
    std::fs::write(&ps, program()).expect("write ps");
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
    assert_eq!(tidy(&String::from_utf8_lossy(&out.stdout)), want());
}

/// Run `src`, return what it printed.
fn printed(src: &str) -> String {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    String::from_utf8_lossy(it.captured_stdout()).into_owned()
}

/// A form that does nothing, defined as `/F`.
const F: &str =
    "/F << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] /PaintProc {pop} >> def\n";

/// A program that runs `7 8 F execform` with `proc` as the PaintProc and
/// prints whether the operand stack is, afterwards, exactly `7 8 F` (the
/// very dictionary) and whether the program was stopped.
fn restored(paintproc: &str) -> String {
    // The PaintProc starts by saying it ran, so `execform` failing before it
    // (or not being there at all) is not mistaken for a restore.
    let inner = &paintproc[1..paintproc.len() - 1];
    printed(&format!(
        "/ran false def /F << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] \
         /PaintProc {{ /ran true def {inner} }} >> def \
         7 8 F {{ execform }} stopped {{ (stopped ) print }} {{ (ran ) print }} ifelse \
         ran =only ( ) print \
         count 3 eq {{ F eq 3 1 roll 8 eq 3 1 roll 7 eq and and }} {{ count =only ( ) print false }} ifelse ="
    ))
}

#[test]
fn errors_put_every_operand_back() {
    // gs puts back the count only; the contents it leaves are whatever
    // its internals left (`[7 8 8]`, the saved CTM). pscat puts back the
    // operands themselves, the form included, whatever the PaintProc did.
    for paintproc in [
        "{pop 1 0 div}",
        "{pop pop 1 0 div}",
        "{pop pop pop 1 0 div}",
        "{pop pop pop pop 1 0 div}",
        "{pop stop}",
        "{pop pop pop stop}",
        "{pop exit}",
        "{pop pop pop pop pop}",
        "{pop 99 1 0 div}",
        "{pop /nonesuch cvx exec}",
        "{1 2 3 4 5 6 7 8 9 10 pop 1 0 div}",
    ] {
        assert_eq!(restored(paintproc), "stopped true true\n", "{paintproc}");
    }
}

#[test]
fn a_paintproc_that_pops_past_the_saved_operands_leaves_the_stack_it_left() {
    // The operands put back on an error are the top 256 of the stack. A
    // PaintProc that pops deeper cannot be undone, and must not mix the old
    // operands into what is left: the stack is what the PaintProc left.
    let out = printed(
        "/G << /FormType 1 /BBox [0 0 10 10] /Matrix [1 0 0 1 0 0] \
            /PaintProc { 290 { pop } repeat 1 0 div } >> def \
         300 { 7 } repeat G { execform } stopped { (stopped ) print } if \
         count =only ( ) print 0 eq exch 1 eq and =",
    );
    // 302 operands (the form twice), 290 popped, the `div`'s two put back by `div` itself.
    assert_eq!(out, "stopped 14 true\n");
}

#[test]
fn a_form_that_calls_itself_stops_at_a_depth_limit_and_puts_everything_back() {
    let mut it = Interp::new();
    it.capture_stdout();
    it.run_str(&format!(
        "{F} F /PaintProc {{ dup execform }} put \
         2 2 scale 7 F {{ execform }} stopped {{ $error /errorname get = }} if \
         count = matrix currentmatrix 0 get cvi = clear"
    ))
    .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    // gs: `execstackoverflow`, the form left on the stack.
    assert_eq!(
        String::from_utf8_lossy(it.captured_stdout()),
        "execstackoverflow\n2\n2\n"
    );
    assert_eq!(it.gsave_depth(), 0, "every level's gsave is undone");
    // The depth is counted down: a form still runs.
    it.run_str(&format!("{F} F execform")).expect("a form runs");
}

#[test]
fn a_form_that_calls_itself_unstopped_reports_the_overflow() {
    let mut it = Interp::new();
    it.run_str(&format!("{F} F /PaintProc {{ dup execform }} put"))
        .expect("define");
    assert_eq!(
        it.run_str("F execform"),
        Err(PsError::ExecStackOverflow),
        "the overflow stops the program"
    );
    assert_eq!(it.gsave_depth(), 0);
}

#[test]
fn forms_nest_as_deep_as_a_program_can_want() {
    // Ten levels of a form that paints a form.
    let out = printed(&format!(
        "{F} /n 0 def F /PaintProc {{ pop /n n 1 add def n 10 lt {{ F execform }} if }} put \
         F execform n ="
    ));
    assert_eq!(out, "10\n");
}

#[test]
fn a_loop_of_forms_with_exit_around_it_is_invalid_exit_inside() {
    // `exit` cannot leave the PaintProc's `stopped` context (gs: invalidexit).
    let out = printed(&format!(
        "{F} F /PaintProc {{ pop exit }} put \
         {{ F execform (not reached) = }} stopped {{ $error /errorname get = }} if"
    ));
    assert_eq!(out, "invalidexit\n");
}

#[test]
fn a_form_leaves_the_graphics_state_as_it_found_it() {
    let out = printed(
        "/pm { matrix currentmatrix { cvi 6 string cvs print ( ) print } forall } def \
         matrix setmatrix 0.25 setgray 3 setlinewidth [4 2] 1 setdash 10 10 moveto 50 50 lineto \
         /F << /FormType 1 /BBox [0 0 10 10] /Matrix [2 0 0 2 5 5] \
            /PaintProc { pop 1 0 0 setrgbcolor 9 setlinewidth [] 0 setdash 1 1 translate \
                         newpath 0 0 5 5 rectfill 7 7 moveto } >> def \
         F execform \
         currentgray 4 string cvs print ( ) print currentlinewidth cvi 4 string cvs print ( ) print \
         currentdash exch length 4 string cvs print ( ) print \
         currentpoint exch cvi 4 string cvs print ( ) print cvi 4 string cvs print ( ) print \
         pm",
    );
    assert_eq!(out.trim(), "0.25 3 2 50 50 1 0 0 1 0 0");
}

/// A page the size of the pictures below.
const W: u32 = 300;
const H: u32 = 220;

/// A badge that reaches past its `/BBox` on purpose, so the clip shows.
const BADGE: &str = "
/Badge << /FormType 1 /BBox [0 0 40 40] /Matrix [1 0 0 1 0 0]
  /PaintProc { pop
    0.9 0.3 0.1 setrgbcolor 20 20 18 0 360 arc fill
    0.1 0.2 0.6 setrgbcolor 3 setlinewidth 5 5 30 30 rectstroke
    0 setgray 1 setlinewidth 8 8 moveto 32 32 lineto stroke
    0.2 0.7 0.2 setrgbcolor 6 setlinewidth -10 20 moveto 50 20 lineto stroke
  } >> def
";

/// The badge, as `execform` is defined to paint it (PLRM): `gsave`,
/// `concat` the `/Matrix`, clip to the `/BBox`, run the `/PaintProc`,
/// `grestore`. A page that does this by hand and a page that calls
/// `execform` must have the same pixels.
const BADGE_BY_HAND: &str = "
/Badge << /FormType 1 /BBox [0 0 40 40] /Matrix [1 0 0 1 0 0]
  /PaintProc { pop
    0.9 0.3 0.1 setrgbcolor 20 20 18 0 360 arc fill
    0.1 0.2 0.6 setrgbcolor 3 setlinewidth 5 5 30 30 rectstroke
    0 setgray 1 setlinewidth 8 8 moveto 32 32 lineto stroke
    0.2 0.7 0.2 setrgbcolor 6 setlinewidth -10 20 moveto 50 20 lineto stroke
  } >> def
/execform { gsave dup /Matrix get concat dup /BBox get aload pop
  exch 3 index sub exch 2 index sub rectclip dup /PaintProc get exec grestore } def
";

/// Three badges at three scales, two of them clipped by the page edge's
/// neighbours, on a tinted ground so the clip shows.
const THREE_SCALES: &str = "
0.95 0.95 0.85 setrgbcolor 0 0 300 220 rectfill
gsave 15 150 translate Badge execform grestore
gsave 75 120 translate 1.5 1.5 scale Badge execform grestore
gsave 150 60 translate 3 3 scale Badge execform grestore
";

/// Twice, at two scales, and a second form with a `/Matrix` that rotates and
/// shifts, under a clip the caller set.
const TWICE_AND_MATRIX: &str = "
0.9 setgray 0 0 300 220 rectfill
gsave 10 10 moveto 100 10 lineto 100 100 lineto 10 100 lineto closepath clip
gsave 20 20 translate 2 2 scale Badge execform grestore
gsave 60 50 translate 1 1 scale Badge execform grestore
grestore
/Tilted << /FormType 1 /BBox [0 0 60 30] /Matrix [0.866 0.5 -0.5 0.866 200 40]
  /PaintProc { pop 0.2 0.4 0.9 setrgbcolor 0 0 60 30 rectfill 1 setgray 5 5 50 20 rectfill
               0 setgray 10 15 5 0 360 arc fill 70 15 moveto -20 15 lineto stroke } >> def
gsave 20 0 translate Tilted execform grestore
gsave 1.3 1 scale 80 130 translate Tilted execform grestore
";

fn ours(src: &str) -> Pixmap {
    let mut it = Interp::with_page(W, H).expect("page");
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.gfx().pixmap.clone()
}

fn gs_png(ps: &str, name: &str) -> Pixmap {
    let dir = common::scratch_dir("execform_pixels");
    let src = dir.join(format!("{name}.ps"));
    let out = dir.join(format!("{name}.png"));
    std::fs::write(&src, format!("%!PS\n{ps}\nshowpage\n")).expect("write input");
    let status = Command::new("gs")
        .args([
            "-q",
            "-dNOPAUSE",
            "-dBATCH",
            "-dSAFER",
            "-sDEVICE=png16m",
            "-dGraphicsAlphaBits=4",
            &format!("-g{W}x{H}"),
            "-r72",
        ])
        .arg(format!("-o{}", out.display()))
        .arg(&src)
        .status()
        .expect("run gs");
    assert!(status.success(), "gs rejected {name}");
    let data = std::fs::read(&out).expect("read gs png");
    let _ = std::fs::remove_dir_all(&dir);
    Pixmap::decode_png(&data).expect("decode png")
}

/// Mean absolute difference of 10-pixel block averages, out of 255.
fn mean_block_diff(a: &Pixmap, b: &Pixmap) -> f64 {
    const BLOCK: u32 = 10;
    let down = |pm: &Pixmap| {
        let (w, h) = (pm.width(), pm.height());
        let (bw, bh) = (w / BLOCK, h / BLOCK);
        let mut cells = vec![[0f64; 3]; (bw * bh) as usize];
        let data = pm.data();
        for y in 0..bh * BLOCK {
            for x in 0..bw * BLOCK {
                let i = ((y * w + x) * 4) as usize;
                let cell = &mut cells[((y / BLOCK) * bw + x / BLOCK) as usize];
                for (ch, c) in cell.iter_mut().enumerate() {
                    *c += f64::from(data[i + ch]);
                }
            }
        }
        cells
    };
    let n = f64::from(BLOCK * BLOCK);
    let (da, db) = (down(a), down(b));
    let total: f64 = da
        .iter()
        .zip(&db)
        .map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs() / n).sum::<f64>())
        .sum();
    total / (da.len() * 3) as f64
}

fn painted(pm: &Pixmap) -> usize {
    pm.data()
        .chunks(4)
        .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
        .count()
}

#[test]
fn a_form_paints_what_the_plrm_says_the_four_steps_paint() {
    // execform and gsave/concat/rectclip/PaintProc/grestore written out are
    // the same pixels, exactly.
    for (name, page) in [("three", THREE_SCALES), ("twice", TWICE_AND_MATRIX)] {
        let with = ours(&format!("{BADGE}{page}"));
        let by_hand = ours(&format!("{BADGE_BY_HAND}{page}"));
        assert!(painted(&with) > 2000, "{name}: something is painted");
        assert_eq!(with.data(), by_hand.data(), "{name}");
    }
}

#[test]
fn a_form_is_clipped_to_its_bbox() {
    // The green bar reaches 10 pt past each side of the BBox: its ends must
    // not show. Pixel (12, y) of the first badge is inside, (5, y) outside.
    let pm = ours(&format!(
        "{BADGE}1 setgray 0 0 300 220 rectfill gsave 100 100 translate Badge execform grestore"
    ));
    let at = |x: u32, y: u32| {
        let p = pm.pixel(x, y).expect("pixel");
        (p.red(), p.green(), p.blue())
    };
    // The bar is at y = 120 up, so image row 220 - 120 = 100.
    assert_eq!(at(95, 100), (255, 255, 255), "left of the BBox");
    assert_eq!(at(145, 100), (255, 255, 255), "right of the BBox");
    assert_ne!(at(105, 100), (255, 255, 255), "inside the BBox");
}

#[test]
fn the_pictures_match_gs() {
    if !common::gs_available() {
        return;
    }
    for (name, page) in [
        ("three_scales", THREE_SCALES),
        ("twice_and_matrix", TWICE_AND_MATRIX),
    ] {
        let src = format!("{BADGE}{page}");
        let diff = mean_block_diff(&ours(&src), &gs_png(&src, name));
        eprintln!("{name}: mean block diff {diff:.3}");
        assert!(diff < 3.0, "{name}: mean block diff {diff:.2} against gs");
    }
}

#[test]
fn programs_that_do_not_use_execform_define_it_but_leave_the_page_alone() {
    // A page with no forms is the same with `execform` defined or not.
    let plain = "0.5 setgray 20 20 100 100 rectfill gsave 3 setlinewidth 0 0 1 setrgbcolor 30 30 moveto 200 150 lineto stroke grestore";
    let a = ours(plain);
    let b = ours(&format!("/execform {{ pop }} def {plain}"));
    assert_eq!(a.data(), b.data());
}

fn limited(limits: Limits) -> Interp {
    let mut it = Interp::new();
    it.set_limits(limits)
        .expect("limits without memory always apply");
    it
}

#[test]
fn a_loop_of_forms_hits_the_step_limit_and_leaves_the_state_whole() {
    let mut it = limited(Limits::none().with_max_steps(200_000));
    it.run_str(&format!("{F} 7 8")).expect("setup");
    assert_eq!(
        it.run_str("{ F execform } loop"),
        Err(PsError::StepsExceeded)
    );
    // The stop came inside or between forms: no gsave is left open.
    assert_eq!(it.gsave_depth(), 0);
}

#[test]
fn a_form_in_a_time_limited_run_is_stopped_with_the_graphics_state_back() {
    let mut it = limited(Limits::none().with_max_time(std::time::Duration::from_millis(50)));
    it.run_str(&format!("{F} 2 2 scale")).expect("setup");
    assert_eq!(
        it.run_str("{ F execform } loop"),
        Err(PsError::TimeExceeded)
    );
    assert_eq!(it.gsave_depth(), 0);
}

#[test]
fn a_stop_inside_the_paintproc_gives_back_the_state_and_the_operands() {
    // A step limit that fires inside the PaintProc (a long loop there) is
    // uncatchable and unwinds through the form: the graphics state comes
    // back (`gsave_depth` is 0) and the operands are the ones it had.
    let mut it = limited(Limits::none().with_max_steps(50_000));
    it.run_str(
        "/F << /FormType 1 /BBox [0 0 10 10] /Matrix [3 0 0 3 0 0] \
         /PaintProc { pop 2 2 scale { } loop } >> def 7 8 F",
    )
    .expect("setup");
    assert_eq!(it.run_str("execform"), Err(PsError::StepsExceeded));
    assert_eq!(it.gsave_depth(), 0);
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str("count = matrix currentmatrix 0 get cvi = clear")
        .expect("state is whole");
    assert_eq!(String::from_utf8_lossy(it.captured_stdout()), "3\n1\n");
}

#[test]
fn a_huge_stack_is_scanned_under_the_step_limit_before_the_scan() {
    // A PaintProc that takes both copies of the form and leaves a hundred
    // thousand operands makes the clean-up scan them all for the form. The
    // scan is charged to --max-steps before each chunk of it is read, so a
    // loop of such forms stops after a few dozen, not after thousands.
    let mut it = limited(Limits::none().with_max_steps(30_000));
    it.run_str(&format!(
        "{F} F /PaintProc {{ pop pop 100000 array aload pop }} put /n 0 def"
    ))
    .expect("setup");
    assert_eq!(
        it.run_str("{ /n n 1 add def F execform } loop"),
        Err(PsError::StepsExceeded)
    );
    assert_eq!(it.gsave_depth(), 0);
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str("n =").expect("n");
    let n: u64 = String::from_utf8_lossy(it.captured_stdout())
        .trim()
        .parse()
        .expect("a count");
    assert!((2..100).contains(&n), "{n} forms before the limit");
}
