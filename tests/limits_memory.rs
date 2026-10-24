//! The memory limit (issue #604). It counts the whole process's heap, so it
//! lives in a test binary of its own with one test: nothing else allocates
//! alongside it. Installs the tracking allocator, as the `pscat` binary does.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

fn limited(mib: usize) -> Interp {
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_memory(mib * MIB))
        .expect("the allocator is installed");
    it
}

#[test]
fn memory_growth_is_capped() {
    // The allocator is counting from the start of the process.
    assert!(pscat::memtrack::installed());
    assert!(pscat::memtrack::live_bytes() > 0);

    // One big allocation is refused before it is made.
    let mut it = limited(50);
    assert_eq!(
        it.run_str("10000000 array pop"),
        Err(PsError::MemoryExceeded)
    );
    let mut it = limited(50);
    assert_eq!(
        it.run_str("40000000 string pop"),
        Err(PsError::Limitcheck),
        "over the 16M object cap stays a plain limitcheck"
    );

    // Allocations that are kept add up: 100 MB-ish of arrays in an array.
    let mut it = limited(100);
    assert_eq!(
        it.run_str("/keep 100000 array def 0 1 99999 { keep exch 100000 array put } for"),
        Err(PsError::MemoryExceeded)
    );

    // The operand stack, the graphics-state stack and the path.
    for src in [
        "{ 1 } loop",
        "{ gsave } loop",
        "0 0 moveto { 1 1 rlineto } loop",
        "1 1 100000000 { } for",
    ] {
        let mut it = limited(100);
        assert_eq!(it.run_str(src), Err(PsError::MemoryExceeded), "{src}");
    }

    // A program that stays under the cap runs normally, and temporary
    // arrays that are freed do not add up.
    let mut it = limited(100);
    it.capture_stdout();
    it.run_str("1 1 2000 { pop 10000 array pop } for (done) print")
        .unwrap();
    assert_eq!(it.captured_stdout(), b"done");
}
