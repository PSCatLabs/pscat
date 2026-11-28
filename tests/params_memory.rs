//! The parameter operators under the memory limit (#1270). A string
//! parameter is copied when it is set and again each time it is read, so a
//! program holding a big string could ask for megabytes in one step: the copy
//! is refused against `--max-memory` before it is made. It counts the whole
//! process's heap, so this binary has exactly one test and nothing else
//! allocates alongside it (the shape of `user_objects_memory.rs`).

use pscat::{Interp, Limits, PsError};

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// The tracking allocator, plus the highest live byte count seen since the
/// last `reset_peak`: a refusal "before the copy is made" is a peak that never
/// grew by the copy, which a final heap size cannot show (the copy may be
/// freed again).
struct Peak;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(n: usize) {
    let live = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for Peak {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        grew(l.size());
        unsafe { pscat::TrackingAllocator.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        grew(l.size());
        unsafe { pscat::TrackingAllocator.alloc_zeroed(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Relaxed);
        unsafe { pscat::TrackingAllocator.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        LIVE.fetch_sub(l.size(), Relaxed);
        grew(new);
        unsafe { pscat::TrackingAllocator.realloc(p, l, new) }
    }
}

#[global_allocator]
static ALLOC: Peak = Peak;

/// Forget the peak so far; the next `peak_growth` is measured from here.
fn reset_peak() -> usize {
    let live = LIVE.load(Relaxed);
    PEAK.store(live, Relaxed);
    live
}

fn peak_growth(from: usize) -> usize {
    PEAK.load(Relaxed).saturating_sub(from)
}

const MIB: usize = 1 << 20;

fn read_back(it: &mut Interp, src: &str) -> String {
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str(src).expect("the interpreter still runs");
    String::from_utf8_lossy(it.captured_stdout())
        .trim()
        .to_string()
}

#[test]
fn a_string_parameter_is_copied_only_with_room_for_the_copy() {
    assert!(pscat::memtrack::installed());
    let mut it = Interp::new();
    it.run_str("/big 8388608 string def").expect("a big string");

    // Setting: the copy would take 8 MiB, the limit leaves 2.
    it.set_limits(Limits::none().with_max_memory(2 * MIB))
        .expect("the allocator is installed");
    for set in [
        "<< /JobName big >> setuserparams",
        "<< /PrinterName big >> setsystemparams",
        "<< /LicenseID big >> setsystemparams",
        "<< /CurInputDevice big >> setsystemparams",
    ] {
        let from = reset_peak();
        assert_eq!(it.run_str(set), Err(PsError::MemoryExceeded), "{set}");
        // Refused before the copy: the 8 MiB was never allocated, not
        // allocated and freed.
        assert!(
            peak_growth(from) < MIB,
            "{set}: peak grew {}",
            peak_growth(from)
        );
        // The operand the refusal gave back is the dictionary: the stack is
        // as it was before the operator.
        assert_eq!(
            read_back(&mut it, "count = clear"),
            "1",
            "{set}: the dictionary operand is back"
        );
        it.set_limits(Limits::none().with_max_memory(2 * MIB))
            .expect("the allocator is installed");
    }
    assert_eq!(
        read_back(&mut it, "currentuserparams /JobName get length ="),
        "0",
        "a refused set left JobName alone"
    );

    // With room for the copy it is made.
    it.set_limits(Limits::none().with_max_memory(32 * MIB))
        .expect("the allocator is installed");
    it.run_str("<< /JobName big >> setuserparams")
        .expect("room for the copy");

    // Reading: the string comes back as a fresh copy, refused the same way.
    it.set_limits(Limits::none().with_max_memory(2 * MIB))
        .expect("the allocator is installed");
    let from = reset_peak();
    assert_eq!(
        it.run_str("currentuserparams"),
        Err(PsError::MemoryExceeded)
    );
    assert!(peak_growth(from) < MIB, "the read made the copy");
    assert_eq!(read_back(&mut it, "count ="), "0", "nothing was pushed");
    it.set_limits(Limits::none().with_max_memory(32 * MIB))
        .expect("the allocator is installed");
    it.run_str("currentuserparams /JobName get length =")
        .expect("room for the copy");
}
