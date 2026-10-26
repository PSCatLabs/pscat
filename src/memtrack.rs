//! Heap accounting for [`Limits::max_memory`](crate::Limits::max_memory):
//! [`TrackingAllocator`].
//!
//! A library cannot see how much memory a program has used unless something
//! counts. `TrackingAllocator` wraps the system allocator and keeps one
//! number, the bytes currently allocated by the whole process. Install it in
//! your binary to use a memory limit:
//!
//! ```
//! #[global_allocator]
//! static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;
//!
//! let mut interp = pscat::Interp::new();
//! interp.set_limits(pscat::Limits::none().with_max_memory(64 << 20))?;
//! # Ok::<(), pscat::LimitsError>(())
//! ```
//!
//! The count is the process heap, not the resident set and not one
//! interpreter's share: other threads' allocations are in it.
//!
//! Each thread keeps its own running total of what it allocated and freed,
//! and folds it into the shared count once it passes 64 KiB either way
//! (issue #844). One shared atomic touched on every allocation made threads
//! that allocate a lot (`pscat --bands`) fight over one cache line: four
//! band threads used about 2.8 times the CPU of four processes. So the
//! shared count can lag by up to 64 KiB per thread that is running, or that
//! exited without calling [`fold_thread`]. [`live_bytes`] folds the calling
//! thread first, so a single-threaded host reads an exact number. The cost
//! is a thread-local add per allocation or free, which the `pscat` binary
//! pays and a library user who does not install the allocator does not.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::Relaxed};

/// Bytes allocated and not yet freed, since the process started, as far as
/// threads have folded their counts in. Signed: memory one thread allocated
/// and another freed can show up here as a free before its allocation does.
static LIVE: AtomicIsize = AtomicIsize::new(0);
/// Set by the first allocation that went through [`TrackingAllocator`].
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// A global allocator that counts live heap bytes, so a
/// [`Limits::max_memory`](crate::Limits::max_memory) can be enforced. Wraps
/// the system allocator; see the [module docs](self).
pub struct TrackingAllocator;

// SAFETY: every method forwards to `System` with the same arguments and
// returns its result unchanged. The only addition is bookkeeping in two
// atomics, which never touches the allocation.
unsafe impl GlobalAlloc for TrackingAllocator {
    #[inline]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed through.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            note_alloc(layout.size());
        }
        ptr
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc_zeroed` is passed through.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            note_alloc(layout.size());
        }
        ptr
    }

    #[inline]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` and `layout` are the caller's, from a prior `alloc`.
        unsafe { System.dealloc(ptr, layout) };
        note(-(layout.size() as isize));
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's contract for `realloc` is passed through.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            note(new_size as isize - layout.size() as isize);
        }
        new_ptr
    }
}

#[inline]
fn note_alloc(size: usize) {
    note(size as isize);
    // A load and a predictable branch; the store happens once.
    if !INSTALLED.load(Relaxed) {
        INSTALLED.store(true, Relaxed);
    }
}

/// Bytes a thread may gain or lose before it folds its count into [`LIVE`].
const BATCH: isize = 64 << 10;

thread_local! {
    /// This thread's allocations and frees not yet folded into [`LIVE`].
    static PENDING: Cell<isize> = const { Cell::new(0) };
}

#[inline]
fn note(delta: isize) {
    // `try_with`: never fails for a `const` slot with no destructor, but if
    // it ever did, count straight into the shared total.
    let folded = PENDING.try_with(|p| {
        let d = p.get() + delta;
        if d.unsigned_abs() < BATCH as usize {
            p.set(d);
            0
        } else {
            p.set(0);
            d
        }
    });
    let d = folded.unwrap_or(delta);
    if d != 0 {
        LIVE.fetch_add(d, Relaxed);
    }
}

/// Fold the calling thread's uncounted allocations and frees into the shared
/// count. A thread that is about to exit calls it so that nothing it did is
/// left out; `pscat --bands` calls it at the end of every worker.
pub fn fold_thread() {
    let d = PENDING.try_with(|p| p.replace(0)).unwrap_or(0);
    if d != 0 {
        LIVE.fetch_add(d, Relaxed);
    }
}

/// True once [`TrackingAllocator`] is the global allocator. It becomes true
/// at the first allocation, long before `main` runs.
pub fn installed() -> bool {
    INSTALLED.load(Relaxed)
}

/// Bytes of heap allocated and not yet freed. Zero when
/// [`TrackingAllocator`] is not installed. Exact for the calling thread,
/// whose count is folded in first; other running threads' counts can lag
/// by up to 64 KiB each (see the [module docs](self)).
pub fn live_bytes() -> usize {
    fold_thread();
    LIVE.load(Relaxed).max(0) as usize
}
