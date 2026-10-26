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

/// `/all`, offsets and the file-object read (issue #773) answer to the same
/// limits: the result is `check_alloc`ed before it is made.
#[test]
fn count_less_and_file_reads_respect_the_limits() {
    let root = common::scratch_dir("rr-mem-all");
    let f = root.join("m.bin");
    let p = format!("({})", f.to_str().unwrap().replace('\\', "\\\\"));
    let mut big = Vec::new();
    for i in 0..3_000_000u32 {
        big.extend_from_slice(&f64::from(i).to_le_bytes());
    }
    std::fs::write(&f, &big).unwrap();
    // The file's bytes fit in 50 MiB; the array of 3M objects does not.
    let mut it = limited(50, &root);
    assert_eq!(
        it.run_str(&format!("{p} /all readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    assert_eq!(
        it.run_str(&format!("{p} 1 /all readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    let mut it = limited(50, &root);
    assert_eq!(
        it.run_str(&format!("{p} (r) file /all readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    let mut it = limited(50, &root);
    assert_eq!(
        it.run_str(&format!("{p} (r) file 3000000 readreals pop")),
        Err(PsError::MemoryExceeded)
    );
    // A slice that fits is fine under the same limit.
    let mut it = limited(50, &root);
    it.run_str(&format!("{p} 2999990 /all readreals length"))
        .unwrap();
    assert_eq!(it.pop_int().unwrap(), 10);
    it.run_str(&format!(
        "{p} (r) file dup 2999990 8 mul setfileposition /all readreals length"
    ))
    .unwrap();
    assert_eq!(it.pop_int().unwrap(), 10);
    // And the whole file in a roomier one, through every form.
    let mut it = limited(400, &root);
    it.run_str(&format!("{p} /all readreals length")).unwrap();
    assert_eq!(it.pop_int().unwrap(), 3_000_000);
    it.run_str(&format!(
        "{p} (r) file /all readreals dup length exch 2999999 get"
    ))
    .unwrap();
    assert_eq!(it.pop_f64().unwrap(), 2_999_999.0);
    assert_eq!(it.pop_int().unwrap(), 3_000_000);
    // Writing the array to a file object never copies it.
    let copy = format!(
        "({})",
        root.join("copy.bin")
            .to_str()
            .unwrap()
            .replace('\\', "\\\\")
    );
    it.run_str(&format!("{p} /all readreals {copy} (w) file writereals"))
        .unwrap();
    assert_eq!(std::fs::read(root.join("copy.bin")).unwrap(), big);
}

/// More than the 16M-element array cap is `limitcheck`, not a cut-short
/// array, however the values are asked for.
#[test]
fn all_over_the_array_cap_is_limitcheck() {
    let root = common::scratch_dir("rr-cap");
    let f = root.join("c.bin");
    // One value over 16,777,216 binary32 values; zeros, and sparse on disk.
    let file = std::fs::File::create(&f).unwrap();
    file.set_len((16_777_216 + 1) * 4).unwrap();
    drop(file);
    let p = format!("({})", f.to_str().unwrap().replace('\\', "\\\\"));
    let mut it = limited(4000, &root);
    assert_eq!(
        it.run_str(&format!("{p} /all /f32 readreals pop")),
        Err(PsError::Limitcheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} (r) file /all /f32 readreals pop")),
        Err(PsError::Limitcheck)
    );
    assert_eq!(
        it.run_str(&format!("{p} 0 /all /f32 readreals pop")),
        Err(PsError::Limitcheck)
    );
}
