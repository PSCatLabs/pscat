//! The work that walks a dictionary's table is charged for the empty slots
//! the table keeps, not only for the live entries (#1422). `undef` never
//! shrinks a dictionary, so one that held hundreds of thousands of entries
//! and was emptied is still that slow to iterate; its few live entries
//! must not hide that from the error report's name scan or from the host
//! copy (`pop_value`, `lookup`).
//!
//! A test binary of its own: each dictionary built here interns a few
//! hundred thousand names, which the other test binaries need not carry.

use pscat::{Interp, Limits, PsError};

/// Entries defined and removed to make a dictionary sparse. The table
/// grows to 2^19 buckets (458,752 usable slots), so the empty slots cost
/// more than the name scan's whole budget (65,536 entries at four slots
/// each) and more than 100 steps of host copy.
const STALE: usize = 300_000;

/// PostScript that makes `/name` a dictionary that once held [`STALE`]
/// entries and now holds none.
fn sparse(name: &str) -> String {
    format!(
        "/{name} 1 dict def {name} begin
         0 1 {last} {{ 12 string cvs cvn 0 def }} for
         end
         0 1 {last} {{ 12 string cvs cvn {name} exch undef }} for",
        last = STALE - 1
    )
}

/// What an uncaught error in `/p { ... }`, defined in the dictionary
/// `holder` and run with it on the dictionary stack, names the frame.
fn frame_name(prelude: &str, holder: &str) -> Option<String> {
    let mut interp = Interp::new();
    interp.run_str(prelude).unwrap();
    interp
        .run_str(&format!("{holder} begin /p {{ 1 pop (x) 1 add pop }} def"))
        .unwrap();
    let err = interp.run_str("p").unwrap_err();
    assert_eq!(err, PsError::Typecheck);
    let trace = interp.error_details(&err).backtrace;
    assert_eq!(trace.len(), 1, "{trace:?}");
    trace[0].procedure.clone()
}

#[test]
fn the_name_scan_is_charged_for_a_dictionarys_retained_slots() {
    // Control: a dictionary that never held much names its procedure.
    assert_eq!(
        frame_name("/small 8 dict def", "small").as_deref(),
        Some("p")
    );
    // A dictionary that held a few thousand entries and lost them still
    // does: its empty slots are well inside the budget.
    assert_eq!(
        frame_name(
            "/modest 1 dict def modest begin
             0 1 19999 { 12 string cvs cvn 0 def } for end
             0 1 19999 { 12 string cvs cvn modest exch undef } for",
            "modest"
        )
        .as_deref(),
        Some("p")
    );
    // One whose retained table costs more to walk than the scan's budget
    // is not walked: the procedure is reported without a name, as for any
    // dictionary the scan cannot afford, and the error report stays cheap.
    // (Before #1422 it was named: the walk was free.)
    assert_eq!(frame_name(&sparse("stale"), "stale"), None);
}

#[test]
fn a_host_copy_of_a_sparse_dictionary_is_charged_for_the_slots_it_passes_over() {
    let mut interp = Interp::new();
    interp
        .register_operator("grab", |i| i.pop_value().map(|_| ()))
        .unwrap();
    interp.run_str(&sparse("stale")).unwrap();
    interp.run_str("/small << /a 1 /b 2 >> def").unwrap();
    // Well over the step cap if the copy is charged for the 458,752 slots
    // it walks (112 steps), well under it for a live dictionary of two.
    interp
        .set_limits(Limits::none().with_max_steps(60))
        .unwrap();
    interp.run_str("small grab").unwrap();
    assert_eq!(
        interp.run_str("stale grab").unwrap_err(),
        PsError::StepsExceeded
    );
    // The copy of an empty dictionary is not refused as too large: the
    // slots are steps, not nodes, so the node cap does not see them.
    interp
        .set_limits(Limits::none().with_max_steps(10_000))
        .unwrap();
    interp.run_str("stale grab").unwrap();
}

#[test]
fn lookup_of_a_sparse_dictionary_is_charged_at_the_next_look() {
    let mut interp = Interp::new();
    interp
        .register_operator("peek", |i| {
            i.lookup("stale");
            Ok(())
        })
        .unwrap();
    interp.run_str(&sparse("stale")).unwrap();
    interp
        .set_limits(Limits::none().with_max_steps(60))
        .unwrap();
    // `lookup` charges at the program's next look at its limits (the end of
    // the slice at the latest); the cap is still held.
    assert_eq!(
        interp
            .run_str("peek 1 pop 1 pop 1 pop")
            .and_then(|()| interp.run_str("1 100 { pop 1 } repeat pop"))
            .unwrap_err(),
        PsError::StepsExceeded
    );
}
