//! The memory limit (issue #604). It counts the whole process's heap, so it
//! lives in a test binary of its own with one test: nothing else allocates
//! alongside it. Installs the tracking allocator, as the `pscat` binary does.

use pscat::{Interp, Limits, PsError};

#[cfg(unix)]
mod common;

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

    // A whole-file read counts as it grows (issue #702): `/dev/zero` never
    // ends, so the cap is what stops it, well short of a gigabyte.
    #[cfg(unix)]
    {
        let mut it = limited(100);
        let before = pscat::memtrack::live_bytes();
        assert_eq!(
            it.run_str("(/dev/zero) (r) file pop"),
            Err(PsError::MemoryExceeded)
        );
        // Overshoot is at most one buffer doubling: the cap is 100 MiB.
        let peak_growth = pscat::memtrack::live_bytes().saturating_sub(before);
        assert!(peak_growth < 100 * MIB, "the read's buffer is freed again");

        // A regular file bigger than the cap is refused up front, from its
        // length, before any of it is read.
        let path = common::scratch_dir("limits-mem").join("big.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(200 * MIB as u64)
            .unwrap();
        let mut it = limited(100);
        let src = format!("({}) (r) file pop", common::ps_path(&path));
        let result = it.run_str(&src);
        let _ = std::fs::remove_file(&path);
        assert_eq!(result, Err(PsError::MemoryExceeded));

        // One that fits reads through in full, however many chunks it takes:
        // its buffer is sized once, so no doubling is refused halfway.
        let path = common::scratch_dir("limits-mem").join("fits.bin");
        std::fs::File::create(&path)
            .unwrap()
            .set_len(150 * MIB as u64)
            .unwrap();
        let mut it = limited(200);
        let src = format!(
            "({}) (r) file 1 string readstring pop length =",
            common::ps_path(&path)
        );
        it.capture_stdout();
        let result = it.run_str(&src);
        let _ = std::fs::remove_file(&path);
        assert_eq!(result, Ok(()));

        // A stream that has to double its buffer is held to the growth it
        // really makes: 40 MiB through a pipe, buffer 32 -> 64 MiB, fits 90.
        let fifo = common::scratch_dir("limits-mem").join("s.fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let p = fifo.clone();
        let writer = std::thread::spawn(move || {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            let block = vec![1u8; MIB];
            for _ in 0..40 {
                f.write_all(&block).unwrap();
            }
        });
        let mut it = limited(90);
        let src = format!(
            "({}) (r) file 1 string readstring pop length =",
            common::ps_path(&fifo)
        );
        let result = it.run_str(&src);
        writer.join().unwrap();
        assert_eq!(result, Ok(()));
    }
}
