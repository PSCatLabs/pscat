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

    // The SVG and PDF recorders decode a whole image into a buffer of their
    // own, width x height whatever the data supplies (#1176): a 30000 x
    // 30000 image with no data was 3.6 GB and 9e8 pixels in one step. Its
    // buffer is refused before the image starts; a modest one still runs.
    for svg in [true, false] {
        let mut it = limited(100);
        if svg {
            it.enable_svg();
        } else {
            it.enable_pdf();
        }
        assert_eq!(
            it.run_str("30000 30000 8 [30000 0 0 30000 0 0] {()} image"),
            Err(PsError::MemoryExceeded),
            "svg {svg}"
        );
        let mut it = limited(100);
        if svg {
            it.enable_svg();
        } else {
            it.enable_pdf();
        }
        it.run_str("100 100 scale 64 64 8 [64 0 0 64 0 0] {(abcdefgh)} image")
            .unwrap();
    }

    // Each kind is charged what its recorder really builds (Codex, PR
    // #1186): a PDF imagemask keeps its packed bits, 1.1 MB for 3000 x
    // 3000, where a colour image of that size decodes to 27 MB of RGB. Under
    // a 20 MiB cap the mask runs and the colour image is refused.
    let mut it = limited(20);
    it.enable_pdf();
    it.run_str("100 100 scale 3000 3000 true [3000 0 0 3000 0 0] {()} imagemask")
        .unwrap();
    let mut it = limited(20);
    it.enable_pdf();
    assert_eq!(
        it.run_str("100 100 scale 3000 3000 8 [3000 0 0 3000 0 0] {()} image"),
        Err(PsError::MemoryExceeded)
    );

    // A decompression bomb read into something that grows (#1187): 4 GiB
    // from a 150 KB FlateDecode + RunLengthDecode string, read as a 225 MB
    // image's samples, and as one token (a string, a hex string, a name).
    // The filters look at the heap as they decode and a read asks before
    // its buffer grows, so the cap stops them inside the one step, well
    // short of their size: a thread samples the heap while it runs.
    // (Stopped only after the operator, the image would have grown to
    // 225 MB first and the token to gigabytes.) The time limit is a
    // backstop: a regression is a time error, not a hang.
    //
    // A token's buffer doubles, and the decoder looks at the heap only
    // every 64 KiB it decodes (#1209), so a token that checked only there
    // would double past the cap: from 32 MiB to 64 MiB under a 40 MiB cap,
    // 60 percent over. The scanner asks before each doubling, so the
    // buffer stops at 32 MiB, and the peak stays within a margin of the
    // cap, not just under some multiple of it. The cap is not a power of
    // two on purpose: at 32 MiB the unchecked buffer stops at the cap.
    struct Bomb {
        lead: &'static [u8],
        cap_mib: usize,
        /// Peak growth must stay within the cap plus this, in MiB.
        margin_mib: Option<usize>,
        programs: &'static [&'static str],
    }
    let bombs = [
        Bomb {
            lead: b"",
            cap_mib: 32,
            margin_mib: None,
            programs: &["15000 15000 8 [15000 0 0 15000 0 0] f image"],
        },
        Bomb {
            lead: &[0x00, b'('],
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token", "{ f token } stopped"],
        },
        Bomb {
            lead: &[0x00, b'<'],
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token"],
        },
        Bomb {
            lead: &[0x00, b'/'],
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token"],
        },
    ];
    for bomb in &bombs {
        use std::io::Write;
        let mut rle = bomb.lead.to_vec();
        rle.extend([0x81, b'a'].repeat(32 << 20));
        rle.push(0x80);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&rle).unwrap();
        drop(rle);
        let hex: String = z
            .finish()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        for program in bomb.programs {
            let src =
                format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def {program}");
            let cap = bomb.cap_mib * MIB;
            let what = format!("{:?} {program}", bomb.lead);
            let (result, grew) = {
                use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
                let mut it = Interp::new();
                it.set_limits(
                    Limits::none()
                        .with_max_memory(cap)
                        .with_max_time(std::time::Duration::from_secs(20)),
                )
                .expect("the allocator is installed");
                let before = pscat::memtrack::live_bytes();
                let peak = std::sync::Arc::new(AtomicUsize::new(before));
                let done = std::sync::Arc::new(AtomicBool::new(false));
                // No sleeping: a doubled buffer lives for well under a
                // millisecond before the decoder's own check takes it.
                let sampler = {
                    let (peak, done) = (peak.clone(), done.clone());
                    std::thread::spawn(move || {
                        while !done.load(Ordering::Relaxed) {
                            peak.fetch_max(pscat::memtrack::live_bytes(), Ordering::Relaxed);
                            std::thread::yield_now();
                        }
                    })
                };
                let result = it.run_str(&src);
                done.store(true, Ordering::Relaxed);
                sampler.join().unwrap();
                let grew = peak.load(Ordering::Relaxed).saturating_sub(before);
                drop(it);
                let after = pscat::memtrack::live_bytes();
                assert!(
                    after < before + 16 * MIB,
                    "{what}: the buffers are freed again"
                );
                (result, grew)
            };
            assert_eq!(result, Err(PsError::MemoryExceeded), "{what}");
            match bomb.margin_mib {
                // The cap, plus a buffer's doubling while the old copy is live.
                None => assert!(grew < 4 * cap, "{what}: peak growth {grew}"),
                Some(margin) => assert!(
                    grew <= cap + margin * MIB,
                    "{what}: peak growth {grew} is over the {cap} cap by more than {margin} MiB"
                ),
            }
        }
    }

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
