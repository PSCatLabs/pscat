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
    // The allocator is counting. Other threads' counts can lag by up to
    // 64 KiB each (#844), so hold something bigger than that first.
    assert!(pscat::memtrack::installed());
    let held = vec![1u8; 1 << 20];
    assert!(pscat::memtrack::live_bytes() > 0);
    drop(held);

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

    // Memory one thread allocates and another frees (issue #844: each
    // thread folds its count in every 64 KiB). The thread exits with its
    // allocations still unfolded, so the free shows up first; the count
    // must stay sane, not wrap to near 2^64.
    let before = pscat::memtrack::live_bytes();
    let kept = std::thread::spawn(|| (0..60).map(|_| vec![7u8; 1000]).collect::<Vec<_>>())
        .join()
        .unwrap();
    drop(kept);
    let after = pscat::memtrack::live_bytes();
    assert!(after < 1 << 40, "no wraparound: {after}");
    assert!(
        after.abs_diff(before) < 2 * 64 * 1024,
        "within one batch either way: {before} -> {after}"
    );

    // A thread that folds before it exits leaves an exact count behind.
    let before = pscat::memtrack::live_bytes();
    let leaked = std::thread::spawn(|| {
        let v: &'static mut [u8] = Box::leak(vec![1u8; 10_000].into_boxed_slice());
        pscat::memtrack::fold_thread();
        v.len()
    })
    .join()
    .unwrap();
    let grew = pscat::memtrack::live_bytes() - before;
    assert!(
        (leaked..leaked + 4096).contains(&grew),
        "the leaked 10 kB is counted: {grew}"
    );

    // Frees made while a thread's thread-locals are destroyed are counted
    // too, with `fold_at_thread_exit` (`--bands` runs every band on a fresh
    // thread whose name interner is freed that way). Each thread here
    // folds, then frees 20 kB as it exits; 200 of them, uncounted, would
    // read as 4 MB.
    thread_local! {
        static HELD: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    let before = pscat::memtrack::live_bytes();
    for _ in 0..200 {
        std::thread::spawn(|| {
            pscat::memtrack::fold_at_thread_exit();
            HELD.with(|h| *h.borrow_mut() = vec![3u8; 20_000]);
            // As a band does at its end; the 20 kB is freed after this.
            pscat::memtrack::fold_thread();
        })
        .join()
        .unwrap();
    }
    let after = pscat::memtrack::live_bytes();
    assert!(
        after < before + 256 * 1024,
        "thread-exit frees are counted: {before} -> {after}"
    );
}
