//! The host API's virtual files and raster placement under the memory cap
//! (#1407). The cap counts the whole process's heap, so this is a test binary
//! of its own, as `tests/host_api_memory.rs` is.

use pscat::host::Sampling;
use pscat::{Interp, Limits, Page, PsError};

use std::sync::Mutex;

/// The cap counts the whole process's heap, so these tests must not overlap.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

/// A copy of a virtual file that would pass the memory cap is refused before
/// it is made, uncatchably, and the interpreter is usable afterwards.
#[test]
fn a_virtual_file_copy_is_held_to_the_memory_cap() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut it = Interp::with_page(10, 10).unwrap();
    let mut big = Vec::new();
    while big.len() < 4 << 20 {
        big.extend_from_slice(b"% padding padding padding padding padding\n");
    }
    it.add_virtual_file("big.ps", big).unwrap();
    it.add_virtual_file("small.ps", b"1").unwrap();
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .unwrap();
    assert_eq!(
        it.run_str("{ (big.ps) run } stopped"),
        Err(PsError::MemoryExceeded)
    );
    it.run_str("(small.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 1);
}

/// The copies SVG and PDF recording make of a host raster are checked against
/// the cap before they are made; the canvas is left as it was and PNG-only
/// painting (which copies nothing) still works under the same cap.
#[test]
fn a_raster_the_recorders_cannot_copy_is_refused_before_anything_changes() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let layer = Page::from_premultiplied_rgba8(800, 800, vec![255; 800 * 800 * 4]).unwrap();
    for record in ["svg", "pdf"] {
        let mut it = Interp::with_page(10, 10).unwrap();
        match record {
            "svg" => it.enable_svg(),
            _ => it.enable_pdf(),
        }
        it.set_limits(Limits::none().with_max_memory(1 << 20))
            .unwrap();
        it.run_str("10 10 scale").unwrap();
        let before = it.canvas_premultiplied_rgba8().to_vec();
        assert_eq!(
            it.draw_page(&layer, Sampling::Nearest),
            Err(PsError::MemoryExceeded),
            "{record}"
        );
        assert_eq!(it.canvas_premultiplied_rgba8(), &before[..], "{record}");
    }
    let mut it = Interp::with_page(10, 10).unwrap();
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .unwrap();
    it.run_str("10 10 scale").unwrap();
    it.draw_page(&layer, Sampling::Nearest).unwrap();
    assert_eq!(&it.canvas_premultiplied_rgba8()[..4], &[255; 4]);
}

/// A borrowed buffer is copied only after the memory cap has been asked, so a
/// registration the cap refuses allocates nothing (Codex, PR #1432 round 1).
#[test]
fn a_borrowed_registration_is_refused_by_the_memory_cap_before_the_copy() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let big = vec![b' '; pscat::host::MAX_VIRTUAL_FILE_BYTES];
    let mut it = Interp::with_page(10, 10).unwrap();
    it.set_limits(Limits::none().with_max_memory(1 << 20))
        .unwrap();
    assert_eq!(
        it.add_virtual_file("big.ps", &big[..]),
        Err(PsError::MemoryExceeded)
    );
    // Nothing was registered; an owned buffer is moved in, not copied.
    assert_eq!(it.run_str("(big.ps) run"), Err(PsError::UndefinedFilename));
    it.add_virtual_file("big.ps", big).unwrap();
}
