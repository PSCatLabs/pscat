//! artkit, paintkit, drykit, drychalk, drycharcoal and mathkit bind their
//! procedures (issues #307, #318, #326, #237).
//!
//! Ghostscript looks up every executable name in a procedure body at run
//! time, through the dictionary stack, and a userdict that already holds
//! the libraries' several hundred names makes that slow. `bind` replaces
//! each name that is an operator when the library loads with the operator
//! itself, which costs the lookup nothing: alpine_lake went from 30 s to
//! 10 s in gs, and the paintkit loading demo from 5.7 s to 2.9 s (notes/307).
//!
//! The libraries' procedures must stay bound. A new procedure written as
//! `} def` instead of `} bind def` is correct but quietly puts the
//! lookups back. This test loads each library, then walks every
//! executable array the library added to userdict (nested procedures
//! included) and fails for any that still holds an executable name which
//! resolves to an operator. A control procedure that is deliberately
//! unbound proves the walk can tell. It runs in pscat always, and in
//! Ghostscript when `gs` is installed (CI has none).
//!
//! drychalk gained about 7% (examples/drychalk_specimen.ps, 2.38 s to 2.22 s).
//! drycrayon gained under 1% and the older kits are too cheap in gs to
//! measure, so they stay unbound (notes/318-bind-drychalk.md). drycharcoal
//! (#326) gained little (notes/326-bind-drycharcoal.md) but is bound with
//! drykit and drychalk; it needs hatchkit, which is not bound.
//! mathkit (#237) was written bound from the start; it needs handscript
//! and strokeglyphs, which are not bound, so those, like hatchkit, load
//! before the snapshot and the walk never sees them.
//! typewriter (#211) was written bound from the start; its generated
//! outline table (lib/typewriter_outlines.ps) binds each glyph procedure as
//! it is read and holds them in a dict the walk does not enter, so it loads
//! as a prerequisite.

use pscat::Interp;

const LIBS: [&str; 7] = [
    "lib/artkit.ps",
    "lib/paintkit.ps",
    "lib/drykit.ps",
    "lib/drychalk.ps",
    "lib/drycharcoal.ps",
    "lib/mathkit.ps",
    "lib/typewriter.ps",
];

/// Libraries a checked one needs but that are not themselves checked:
/// loaded before the snapshot, so their names are part of the baseline.
const PREREQS: [&str; 4] = [
    "lib/handscript.ps",
    "lib/strokeglyphs.ps",
    "lib/hatchkit.ps",
    "lib/typewriter_outlines.ps",
];

/// Defines the walker, snapshots userdict, and leaves its checks for
/// `EPILOGUE`. Everything it defines is defined before the snapshot, so
/// the walk never visits its own procedures.
const PROLOGUE: &str = "
/bkKey null def
/bkNames 400 dict def
/bkNote { bkNames bkKey true put pop } def
% proc -   (notes bkKey when an executable name in it, or in a procedure
% nested in it, is an operator now)
/bkWalk {
    {
        dup type /nametype eq 1 index xcheck and {
            dup where { pop dup load type /operatortype eq { bkNote } { pop } ifelse } { pop } ifelse
        } {
            dup type /arraytype eq 1 index xcheck and { bkWalk } { pop } ifelse
        } ifelse
    } forall
} def
/bkSnap userdict dup length 40 add dict copy def
";

/// The control, then the walk, then the names found, as an array of
/// strings left on the stack.
const EPILOGUE: &str = "
/bkControl { 1 2 add pop } def
userdict {
    exch dup bkSnap exch known { pop pop } {
        /bkKey exch def
        dup type /arraytype eq 1 index xcheck and { bkWalk } { pop } ifelse
    } ifelse
} forall
[ bkNames { pop 64 string cvs } forall ]
";

#[test]
fn library_procedures_are_bound() {
    let mut it = Interp::new();
    for path in PREREQS {
        let text = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&text)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it.run_str(PROLOGUE).expect("prologue");
    for path in LIBS {
        let text = std::fs::read(path).unwrap_or_else(|e| panic!("{path}: {e}"));
        it.run_source(&text)
            .unwrap_or_else(|e| panic!("{path} failed to load: {}", it.error_report(&e)));
    }
    it.run_str(EPILOGUE).expect("epilogue");
    let found = it.operand_stack().last().expect("the names").repr();
    assert_eq!(
        found, "[(bkControl)]",
        "procedures that still look operators up by name: write bind before def"
    );
}

fn gs_available() -> bool {
    std::process::Command::new("gs")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The same walk under Ghostscript, where `bind` also resolves its own
/// internal operators: a name the library defines later that gs already
/// has as an operator would be bound to the wrong thing. Local only.
#[test]
fn library_procedures_are_bound_in_ghostscript() {
    if !gs_available() {
        eprintln!("skipping gs bind check: gs not installed");
        return;
    }
    let mut src = String::new();
    for path in PREREQS {
        src.push_str(&std::fs::read_to_string(path).expect("read prerequisite"));
        src.push('\n');
    }
    src.push_str(PROLOGUE);
    for path in LIBS {
        src.push_str(&std::fs::read_to_string(path).expect("read lib"));
        src.push('\n');
    }
    src.push_str(EPILOGUE);
    src.push_str("\n==\n");
    let file = std::env::temp_dir().join(format!("pscat-libbind-{}.ps", std::process::id()));
    std::fs::write(&file, src).expect("write");
    let out = std::process::Command::new("gs")
        .args(["-dNOPAUSE", "-dBATCH", "-q", "-sDEVICE=nullpage"])
        .arg(&file)
        .output()
        .expect("run gs");
    let _ = std::fs::remove_file(&file);
    assert!(
        out.status.success(),
        "gs rejected the driver: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "[(bkControl)]",
        "gs still sees unbound operator names"
    );
}
