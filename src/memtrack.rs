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
//! interpreter's share: other threads' allocations are in it. The cost is
//! one relaxed atomic add or subtract per allocation or free, which the
//! `pscat` binary pays and a library user who does not install it does not.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

/// Bytes allocated and not yet freed, since the process started.
static LIVE: AtomicUsize = AtomicUsize::new(0);
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
        LIVE.fetch_sub(layout.size(), Relaxed);
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's contract for `realloc` is passed through.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            let old = layout.size();
            if new_size >= old {
                LIVE.fetch_add(new_size - old, Relaxed);
            } else {
                LIVE.fetch_sub(old - new_size, Relaxed);
            }
        }
        new_ptr
    }
}

#[inline]
fn note_alloc(size: usize) {
    LIVE.fetch_add(size, Relaxed);
    // A load and a predictable branch; the store happens once.
    if !INSTALLED.load(Relaxed) {
        INSTALLED.store(true, Relaxed);
    }
}

/// True once [`TrackingAllocator`] is the global allocator. It becomes true
/// at the first allocation, long before `main` runs.
pub fn installed() -> bool {
    INSTALLED.load(Relaxed)
}

/// Bytes of heap allocated and not yet freed. Zero when
/// [`TrackingAllocator`] is not installed.
pub fn live_bytes() -> usize {
    LIVE.load(Relaxed)
}
