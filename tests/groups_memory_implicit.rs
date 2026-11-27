//! A transparency group closed with no operator to report it (#1421, Codex
//! round 1 on PR #1462): the composite asks `--max-memory` for its list of
//! painted rows before it makes it. When the cap cannot hold it the group
//! is not composited, and the run has to stop with `limitexceeded (memory)`
//! as `endgroup` does, not carry on with the group's marks silently lost.
//!
//! The memory limit counts the whole process's heap, so this is a test
//! binary of its own with one test (as `tests/groups_memory.rs` is).

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const KIB: usize = 1 << 10;

/// A 4 x 8000 page: a layer is 125 KiB and its list of painted rows 62.5
/// KiB. A cap of the layer plus 40 KiB lets the group begin and refuses the
/// list when the group closes.
fn run(src: &str) -> Result<(), PsError> {
    let mut it = Interp::with_page(4, 8000).expect("page");
    it.set_limits(Limits::none().with_max_memory(125 * KIB + 40 * KIB))
        .expect("the allocator is installed");
    it.run_str(src)
}

#[test]
fn a_group_closed_without_endgroup_that_cannot_hold_its_row_list_stops_the_run() {
    let body = "<< >> begingroup 1 0 0 setrgbcolor 0 0 4 1 rectfill";
    let mut wrong = Vec::new();
    for (what, src) in [
        ("endgroup", format!("{body} endgroup")),
        ("the end of the program", body.to_string()),
        ("showpage", format!("{body} showpage 1 1 add pop")),
        ("copypage", format!("{body} copypage 1 1 add pop")),
        ("erasepage", format!("{body} erasepage 1 1 add pop")),
        ("restore", format!("save {body} restore 1 1 add pop")),
        (
            "a stop out of the group",
            format!("{{ {body} stop }} stopped pop 1 1 add pop"),
        ),
    ] {
        let got = run(&src);
        if got != Err(PsError::MemoryExceeded) {
            wrong.push(format!("{what}: {got:?}"));
        }
    }
    assert!(wrong.is_empty(), "no limitexceeded (memory) for: {wrong:?}");
}
