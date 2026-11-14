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

    // A stenciled image is flattened to a composite expanded to the mask's
    // resolution (#1188): a 64 x 64 image under a 4096 x 4096 mask is 16
    // Mpixel, 200 MB for SVG and 140 MB for PDF with every buffer alive at
    // once, which passed a 1 MiB cap when counted at the image's own size.
    let stencil = |w: usize| {
        format!(
            "/DeviceRGB setcolorspace 100 100 scale \
             << /ImageType 3 /InterleaveType 3 \
             /DataDict << /ImageType 1 /Width 64 /Height 64 /BitsPerComponent 8 \
               /Decode [0 1 0 1 0 1] /ImageMatrix [64 0 0 -64 0 64] /DataSource {{()}} >> \
             /MaskDict << /ImageType 1 /Width {w} /Height {w} /BitsPerComponent 1 \
               /ImageMatrix [{w} 0 0 -{w} 0 {w}] /DataSource {{()}} >> >> image"
        )
    };
    for svg in [true, false] {
        let mut it = limited(50);
        if svg {
            it.enable_svg();
        } else {
            it.enable_pdf();
        }
        assert_eq!(
            it.run_str(&stencil(4096)),
            Err(PsError::MemoryExceeded),
            "svg {svg}"
        );
        // The same image under a mask of its own size fits.
        let mut it = limited(50);
        if svg {
            it.enable_svg();
        } else {
            it.enable_pdf();
        }
        it.run_str(&stencil(64)).unwrap();
    }

    // `fill` and `flattenpath` look at the cap as their edge lists grow
    // (#1188). The fill's 10^8 edges were freed again before the next look
    // at the limits, so a 100 MiB cap never saw them; the flattened path
    // replaced the current one and stayed, about 1 GB.
    let mut it = limited(100);
    assert_eq!(
        it.run_str("newpath 0 0 moveto 100000 { 600 800 -200 900 300 0 curveto } repeat fill"),
        Err(PsError::MemoryExceeded)
    );
    let before = pscat::memtrack::live_bytes();
    let mut it = limited(100);
    assert_eq!(
        it.run_str(
            "newpath 0 0 moveto 2000 { 1e20 1e20 -1e20 1e20 0 0 curveto } repeat flattenpath"
        ),
        Err(PsError::MemoryExceeded)
    );
    let kept = pscat::memtrack::live_bytes().saturating_sub(before);
    assert!(kept < 100 * MIB, "the flattened path is not kept: {kept}");
    drop(it);

    // A kernelimage whose every pixel retires to the interpreter (#1188):
    // its list of 4 million retired pixels was 64 MB, made with no look at
    // the cap, before the interpreter's copy of it (256 MB) was refused.
    // Once the list would not fit, the interpreter does the whole image
    // instead, under the cap, until the clock stops it.
    let mut it = Interp::new();
    it.set_limits(
        Limits::none()
            .with_max_memory(50 * MIB)
            .with_max_time(std::time::Duration::from_millis(300)),
    )
    .expect("the allocator is installed");
    assert_eq!(
        it.run_str(
            "gsave 600 600 scale 2000 2000 [2000 0 0 -2000 0 2000] \
             { /j exch def /i exch def i 3037000500 mul dup mul pop i 2000 div } 1 kernelimage grestore"
        ),
        Err(PsError::TimeExceeded)
    );

    // A decompression bomb read into something that grows (#1187): 4 GiB
    // from a 150 KB FlateDecode + RunLengthDecode string, read as a 225 MB
    // image's samples, and as one string token. The filters look at the
    // heap as they decode and a read asks before its buffer grows, so the
    // cap stops both inside the one step, well short of their size: a
    // thread samples the heap while it runs. (Stopped only after the
    // operator, the image would have grown to 225 MB first and the token
    // to gigabytes.) The time limit is a backstop: a regression is a time
    // error, not a hang.
    for (lead, src) in [
        (&b""[..], "15000 15000 8 [15000 0 0 15000 0 0] f image"),
        (&[0x00, b'('][..], "f cvx exec"),
    ] {
        use std::io::Write;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let mut rle = lead.to_vec();
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
        let src = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def {src}");
        let mut it = Interp::new();
        it.set_limits(
            Limits::none()
                .with_max_memory(32 * MIB)
                .with_max_time(std::time::Duration::from_secs(20)),
        )
        .expect("the allocator is installed");
        let before = pscat::memtrack::live_bytes();
        let peak = std::sync::Arc::new(AtomicUsize::new(before));
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let sampler = {
            let (peak, done) = (peak.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    peak.fetch_max(pscat::memtrack::live_bytes(), Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_micros(200));
                }
            })
        };
        let result = it.run_str(&src);
        done.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        let what = &src[src.len() - 40..];
        assert_eq!(result, Err(PsError::MemoryExceeded), "{what}");
        let grew = peak.load(Ordering::Relaxed).saturating_sub(before);
        // The cap, plus a buffer's doubling while the old copy is live.
        assert!(grew < 4 * 32 * MIB, "{what}: peak growth {grew}");
        drop(it);
        let after = pscat::memtrack::live_bytes();
        assert!(
            after < before + 16 * MIB,
            "{what}: the buffers are freed again"
        );
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
