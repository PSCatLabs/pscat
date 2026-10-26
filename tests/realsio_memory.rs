//! `readreals` and the memory limit (issue #735). The limit counts the
//! whole process's heap, so this lives in a test binary of its own, with
//! the tracking allocator installed as in `tests/limits_memory.rs`.

use pscat::{Interp, Limits, PsError};

mod common;

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

fn limited(mib: usize, root: &std::path::Path) -> Interp {
    let mut it = Interp::new();
    it.set_write_roots(&[root]).unwrap();
    it.set_limits(Limits::none().with_max_memory(mib * MIB))
        .expect("the allocator is installed");
    it
}

#[test]
fn realsio_respects_the_memory_limit() {
    let root = common::scratch_dir("rr-mem");
    let f = root.join("m.bin");
    let p = format!("({})", f.to_str().unwrap().replace('\\', "\\\\"));
    // A one-element file: asking for 10M values is refused before the
    // file is read or the array made (not a rangecheck for the short file).
    let mut it = limited(100, &root);
    it.run_str(&format!("[1] {p} writereals")).unwrap();
    assert_eq!(
        it.run_str(&format!("{p} 10000000 readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    // A real, sized file that does not fit: 3M values is 24 MB of bytes
    // and 3M objects on top, over 50 MiB.
    let mut big = Vec::new();
    for i in 0..3_000_000u32 {
        big.extend_from_slice(&f64::from(i).to_le_bytes());
    }
    std::fs::write(&f, &big).unwrap();
    let mut it = limited(50, &root);
    assert_eq!(
        it.run_str(&format!("{p} 3000000 readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    // The same read fits in a roomier limit, and writing it back never
    // copies the array.
    let mut it = limited(400, &root);
    it.run_str(&format!("{p} 3000000 readreals dup {p} writereals length"))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 3_000_000);
    assert_eq!(std::fs::read(&f).unwrap(), big);
}
