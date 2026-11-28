//! `execform` under the memory limit (#1271). It counts the whole process's
//! heap, so this binary has exactly one test and nothing else allocates
//! alongside it (the same shape as `limits_memory.rs`).
//!
//! Each refusal comes before the thing it refuses is made, and leaves the
//! operands, the graphics state and the `gsave` depth as they were.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const FORM: &str =
    "/F << /FormType 1 /BBox [0 0 10 10] /Matrix [2 0 0 2 0 0] /PaintProc {pop} >> def\n";

/// The operand count, the form's x scale and the `gsave` depth, read with
/// no limit.
fn state(it: &mut Interp) -> (usize, i64, usize) {
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str("count =only ( ) print matrix currentmatrix 0 get cvi =")
        .expect("the interpreter still runs");
    let out = String::from_utf8_lossy(it.captured_stdout()).into_owned();
    let mut words = out.split_whitespace();
    let count = words.next().and_then(|w| w.parse().ok()).expect("count");
    let sx = words.next().and_then(|w| w.parse().ok()).expect("scale");
    (count, sx, it.gsave_depth())
}

#[test]
fn execform_refuses_what_it_would_allocate_before_it_allocates() {
    assert!(pscat::memtrack::installed());

    // The clip mask: a 3000 x 3000 page's mask is 9 MB, and the limit
    // leaves 1 MB. The form is refused before the `gsave` or the mask.
    let mut it = Interp::with_page(3000, 3000).expect("page");
    it.run_str(FORM).expect("form");
    it.run_str("3 3 scale 7 8").expect("operands");
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("F execform"),
        Err(PsError::MemoryExceeded),
        "clip mask"
    );
    // Operands 7 8 and the form, scale 3, nothing left open.
    assert_eq!(state(&mut it), (3, 3, 0));

    // The copy of the operands the form keeps for an error: 256 operands
    // are 10 KB here, the limit 4 KB. By then the `gsave`, the `concat` and
    // the clip are done, and are undone.
    let mut it = Interp::with_page(20, 20).expect("page");
    it.run_str(FORM).expect("form");
    it.run_str("3 3 scale 300 { 7 }  repeat").expect("operands");
    it.set_limits(Limits::none().with_max_memory(4096))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("F execform"),
        Err(PsError::MemoryExceeded),
        "operand copy"
    );
    assert_eq!(state(&mut it), (301, 3, 0));

    // The journal's copy of a big form dictionary, the first write to it in
    // a `save` (the form gets its `/Implementation`).
    let mut it = Interp::with_page(20, 20).expect("page");
    it.run_str(FORM).expect("form");
    it.run_str("1 1 50000 { 20 string cvs cvn 0 F 3 1 roll put } for 3 3 scale")
        .expect("a big form");
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("save pop F execform"),
        Err(PsError::MemoryExceeded),
        "journal"
    );
    let (count, sx, depth) = state(&mut it);
    assert_eq!((sx, depth), (3, 0));
    assert_eq!(count, 1, "the form");
    it.run_str("F /Implementation known =")
        .expect("form is intact");

    // The same forms fit when there is room.
    let mut it = Interp::with_page(20, 20).expect("page");
    it.run_str(FORM).expect("form");
    it.set_limits(Limits::none().with_max_memory(8 << 20))
        .expect("the allocator is installed");
    it.run_str("300 { 7 } repeat F execform").expect("fits");
}
