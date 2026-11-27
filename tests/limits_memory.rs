//! The memory limit (issue #604). It counts the whole process's heap, so it
//! lives in a test binary of its own with one test: nothing else allocates
//! alongside it. Installs the tracking allocator, as the `pscat` binary does.

use pscat::{Interp, Limits, PsError};

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

// `lead`, then `count` of `byte` as RunLengthDecode runs, then
// `tail`, deflated, as hex for a string literal.
fn flate_rle_hex(lead: &[u8], byte: u8, count: usize, tail: &[u8]) -> String {
    use std::io::Write;
    let literal = |rle: &mut Vec<u8>, bytes: &[u8]| {
        if !bytes.is_empty() {
            rle.push(bytes.len() as u8 - 1);
            rle.extend(bytes);
        }
    };
    let mut rle = Vec::new();
    literal(&mut rle, lead);
    rle.extend([0x81, byte].repeat(count / 128));
    literal(&mut rle, tail);
    rle.push(0x80);
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(&rle).unwrap();
    z.finish()
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
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
    //
    // A limited rasteriser stops keeping its edges once there are more of
    // them than a page-sized accumulator would take (#1235), so on a page
    // of up to 2^25 cells these two programs need 31 MiB, not 500, and finish
    // under 100. The edge list is still policed where the page is bigger than
    // that (6000 x 6000 here: the accumulator would be 1.5 GB).
    const CURVES: &str = "newpath 0 0 moveto 100000 { 600 800 -200 900 300 0 curveto } repeat";
    for op in ["fill", "clip"] {
        let mut it = Interp::new();
        it.run_str("<< /PageSize [6000 6000] >> setpagedevice")
            .unwrap();
        it.set_limits(Limits::none().with_max_memory(100 * MIB))
            .expect("the allocator is installed");
        // Scaled up so the path's bounds are the whole page.
        let err = it
            .run_str(&format!("10 10 scale {CURVES} {op}"))
            .unwrap_err();
        assert_eq!(err, PsError::MemoryExceeded, "{op} on a big page");
        // `clip` (#1219) is over the work a stoppable rasteriser takes it,
        // and stops at the cap inside `clip` as `fill` does.
        let report = it.error_report(&err);
        assert!(
            report.contains(&format!("OffendingCommand: {op};")),
            "{report}"
        );
        drop(it);
        // The default page: the edges are laid as they are made.
        let mut it = limited(100);
        it.run_str(&format!("{CURVES} {op}"))
            .unwrap_or_else(|e| panic!("{op} streams its edges: {e:?}"));
        drop(it);
    }
    // The MCP preset's 1 GiB held 100,000 curves clipped at 300 dpi only
    // just (the edges were 830 MiB, #1235): now 160. Half of it is plenty.
    let mut it = limited(400);
    it.run_str(&format!(
        "<< /PageSize [2550 3300] >> setpagedevice 300 72 div dup scale {CURVES} clip"
    ))
    .expect("a 300 dpi clip of 100,000 curves fits 400 MiB");
    drop(it);

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

    // Copying a path of a million straight lines is a second path that size
    // (Codex, PR #1217: under a 40 MiB cap flattenpath ran to 71 MB). The
    // copy counts toward the polls; the path stays as it was.
    let mut it = Interp::new();
    it.run_str("newpath 0 0 moveto 1000000 { 1 1 lineto } repeat")
        .unwrap();
    it.set_limits(Limits::none().with_max_memory(20 * MIB))
        .expect("the allocator is installed");
    assert_eq!(it.run_str("flattenpath"), Err(PsError::MemoryExceeded));
    it.set_limits(Limits::none()).unwrap();
    it.capture_stdout();
    it.run_str(
        "0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 1 add } { 1 add } pathforall =",
    )
    .unwrap();
    assert_eq!(it.captured_stdout(), b"1000001\n");
    drop(it);

    // A Type 1 glyph that calls a 16 MB subroutine (Codex, PR #1217): its
    // decrypted copy was made, used and freed inside one step, past a 20 MiB
    // cap. The decryption is held against the cap before it is made. Only
    // the bytes the glyph's operator budget can reach are decrypted (#1219),
    // about 500 KB of it, so 20 MiB holds it and 512 KiB does not.
    for (cap, want) in [(20 * MIB, Ok(())), (MIB / 2, Err(PsError::MemoryExceeded))] {
        let mut it = Interp::new();
        it.run_str(&common::type1_probe(16_000_000, 0, 0, 1, ""))
            .unwrap();
        it.set_limits(Limits::none().with_max_memory(cap))
            .expect("the allocator is installed");
        assert_eq!(it.run_str("(A) show (survived) ="), want, "cap {cap}");
        drop(it);
    }

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

    // With no memory limit at all, only a clock, the retired list had nothing
    // to size it by: 64 million retired pixels were 1.3 GB (2.2 GB on a
    // slower machine), a shared host's whole share, before the clock stopped
    // it (#1235). Any limit now holds the list to a fixed ceiling, past which
    // the interpreter does the whole image: the same pixels, under the clock.
    {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let mut it = Interp::new();
        it.set_limits(Limits::none().with_max_time(std::time::Duration::from_secs(2)))
            .expect("a time limit always applies");
        let before = pscat::memtrack::live_bytes();
        let peak = std::sync::Arc::new(AtomicUsize::new(before));
        let done = std::sync::Arc::new(AtomicBool::new(false));
        let sampler = {
            let (peak, done) = (peak.clone(), done.clone());
            std::thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    peak.fetch_max(pscat::memtrack::live_bytes(), Ordering::Relaxed);
                    std::thread::yield_now();
                }
            })
        };
        let r = it.run_str(
            "gsave 4000 4000 scale 4000 4000 [4000 0 0 -4000 0 4000] \
             { /j exch def /i exch def i 3037000500 mul dup mul pop i 2000 div } 1 kernelimage grestore",
        );
        done.store(true, Ordering::Relaxed);
        sampler.join().unwrap();
        let grew = peak.load(Ordering::Relaxed).saturating_sub(before);
        assert_eq!(r, Err(PsError::TimeExceeded));
        assert!(
            grew < 300 * MIB,
            "the retired list is held to its ceiling: peak growth {} MiB",
            grew / MIB
        );
        drop(it);
    }

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
        /// What the stream starts with, and `count` bytes of `byte` after it,
        /// then `tail`.
        lead: &'static [u8],
        byte: u8,
        count: usize,
        tail: &'static [u8],
        cap_mib: usize,
        /// Peak growth must stay within the cap plus this, in MiB.
        margin_mib: Option<usize>,
        programs: &'static [&'static str],
    }
    let bombs = [
        Bomb {
            lead: b"",
            byte: b'a',
            count: 4 << 30,
            tail: b"",
            cap_mib: 32,
            margin_mib: None,
            programs: &["15000 15000 8 [15000 0 0 15000 0 0] f image"],
        },
        Bomb {
            lead: b"(",
            byte: b'a',
            count: 4 << 30,
            tail: b"",
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token", "{ f token } stopped"],
        },
        Bomb {
            lead: b"<",
            byte: b'a',
            count: 4 << 30,
            tail: b"",
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token"],
        },
        Bomb {
            lead: b"/",
            byte: b'a',
            count: 4 << 30,
            tail: b"",
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f cvx exec", "f token"],
        },
        // An ASCII85 string whose last, partial group is the growth that
        // takes the buffer from 32 MiB to 64: 8,388,608 `z` fill it.
        Bomb {
            lead: b"<~",
            byte: b'z',
            count: 8 << 20,
            tail: b"!!~>",
            cap_mib: 40,
            margin_mib: Some(2),
            programs: &["f token"],
        },
    ];
    for bomb in &bombs {
        let hex = flate_rle_hex(bomb.lead, bomb.byte, bomb.count, bomb.tail);
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

    // The scanner's other allocations, each refused deterministically (Codex,
    // PR #1221): no sampling, the program either stops inside the operator
    // that was scanning or it does not. Every one of these ran to its end
    // before the check was added.
    {
        use std::io::Write;
        let run = |cap: usize, src: &str| {
            let mut it = Interp::new();
            it.set_limits(
                Limits::none()
                    .with_max_memory(cap)
                    .with_max_time(std::time::Duration::from_secs(20)),
            )
            .expect("the allocator is installed");
            let r = it.run_str(src);
            let report = r.as_ref().err().map(|e| it.error_report(e));
            (r, report.unwrap_or_default())
        };
        let refused = |cap: usize, src: &str, op: &str| {
            let (r, report) = run(cap, src);
            assert_eq!(r, Err(PsError::MemoryExceeded), "{src:.200}");
            assert!(
                report.contains(&format!("OffendingCommand: {op};")),
                "{src:.200}: {report}"
            );
        };

        // The last, partial ASCII85 group is a growth of its own: 8,388,608
        // `z` are 32 MiB, exactly the buffer, and the `!!` after them take
        // it to 64.
        let hex = flate_rle_hex(b"<~", b'z', 8 << 20, b"!!~>");
        let f = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def");
        refused(40 * MIB, &format!("{f} f token pop pop"), "token");
        refused(40 * MIB, &format!("{f} f cvx exec"), "exec");

        // A binary object sequence's header can claim 4 GB, and the
        // scanner reserves up to 1 MiB for it before the bytes are there.
        // The header alone, with 750,000 bytes already held under a 1 MiB
        // cap, was an allocation past the cap and then a catchable
        // `syntaxerror`.
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(&[0x80, 0x00, 0x00, 0x01, 0x00, 0x10, 0x00, 0x00])
            .unwrap();
        let hex: String = z
            .finish()
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        refused(
            MIB,
            &format!("/s 750000 string def /f <{hex}> /FlateDecode filter def f token"),
            "token",
        );

        // A token in a byte source (a string run as code) has no filter to
        // ask for it: a 16 MB string, the copy the scanner reads, and the
        // token's own buffer are 46 MB under a 40 MiB cap.
        let held = "/s 16000000 string def s 0 40 put s 15999999 41 put";
        refused(40 * MIB, &format!("{held} s cvx exec pop"), "exec");
        refused(40 * MIB, &format!("{held} s token pop pop pop"), "token");

        // A name is interned, which copies it, and the buffer it was read
        // into is still live until then: 32 MiB twice, over 40.
        let hex = flate_rle_hex(b"/", b'a', 32 * MIB, b"");
        let f = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def");
        refused(40 * MIB, &format!("{f} f token"), "token");
        // A name that is invalid UTF-8 is repaired into a copy, up to three
        // times its size, and that is what is interned: 8 MiB of 0xff are
        // 24 MiB of text, held twice with its copy, over 40.
        let hex = flate_rle_hex(b"/", 0xff, 8 * MIB, b"");
        let f = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def");
        refused(40 * MIB, &format!("{f} f token"), "token");
        // A number is not interned, whatever its length: 4 MiB of `0` is the
        // integer 0 under a cap too small for a name that long, with its copy.
        let hex = flate_rle_hex(b"", b'0', 4 * MIB, b"");
        let f = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def");
        let (r, report) = run(6 * MIB, &format!("{f} f token pop 0 ne {{ 1 0 div }} if"));
        assert_eq!(r, Ok(()), "{report}");
        // And one that fits, with its copy, is not refused.
        let hex = flate_rle_hex(b"/", b'a', 10 * MIB, b"");
        let f = format!("/f <{hex}> /FlateDecode filter /RunLengthDecode filter def");
        let (r, report) = run(40 * MIB, &format!("{f} f token pop"));
        assert_eq!(r, Ok(()), "{report}");
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

    // A translucent fill's 16-bit shadow rows (#731) are checked against
    // the headroom left before the fill paints (Codex round 1 on #1306):
    // with most of the cap already held, the fill that needs a page of
    // shadow is refused and the page is left as it was. With the heap
    // free, the same fill paints.
    {
        let cap = 40 * MIB;
        let fill = |held: usize| {
            let mut it = Interp::with_page(1000, 1000).expect("page");
            it.set_limits(Limits::none().with_max_memory(cap))
                .expect("the allocator is installed");
            let src = format!(
                "/held [ 1 1 {held} {{ pop 60000 string }} for ] def \
                 0 0 0 setrgbcolor 0.5 setalpha 0 0 1000 1000 rectfill"
            );
            let r = it.run_str(&src);
            let report = r.as_ref().err().map(|e| it.error_report(e));
            let px = it.gfx().pixmap.pixel(500, 500).expect("pixel");
            (r, report.unwrap_or_default(), px.red())
        };
        let (r, _, red) = fill(1);
        assert_eq!(r, Ok(()), "with headroom the fill runs");
        assert!(red < 200, "and paints: {red}");
        // 8 MB of shadow (1000 x 1000 x 8) is a fifth of the cap, so the
        // shadow is allowed; 600 strings of 60 kB (36 MB held) leave under
        // 6 MB for it.
        let (r, report, red) = fill(600);
        assert_eq!(r, Err(PsError::MemoryExceeded), "{report}");
        assert!(report.contains("OffendingCommand: rectfill;"), "{report}");
        assert_eq!(red, 255, "the refused fill left the page white");

        // A second page's shadow replaces the first's, which the erase
        // frees first, so a run whose first translucent page fits fits its
        // second too (Codex round 2 on #1306: the preflight counted both).
        let mut it = Interp::with_page(1000, 1000).expect("page");
        it.set_limits(Limits::none().with_max_memory(cap))
            .expect("the allocator is installed");
        let page = "0 0 0 setrgbcolor 0.5 setalpha 0 0 1000 1000 rectfill";
        let r = it.run_str(&format!(
            "/held [ 1 1 450 {{ pop 60000 string }} for ] def {page} showpage {page}"
        ));
        assert_eq!(r, Ok(()), "the second page fits as the first did");
    }

    // `upath` and `uappend` ask the heap cap before they make what they
    // make (#1264): a million-segment path would be a ten-million-object
    // array, and a user path of 1.2 million elements 77 MB of path. Both are
    // refused with the path as it was, and the interpreter runs on.
    let mut it = Interp::new();
    it.run_str(
        "newpath 0 0 moveto 500000 { 1 1 rlineto -1 0 rlineto } repeat \
         /up 1200005 array def up 0 { 0 0 100 100 setbbox } putinterval \
         /piece { 1 1 moveto 2 2 lineto } cvlit def \
         0 1 199999 { 6 mul 5 add up exch piece putinterval } for",
    )
    .expect("setup");
    it.set_limits(Limits::none().with_max_memory(50 * MIB))
        .expect("the allocator is installed");
    assert_eq!(it.run_str("false upath pop"), Err(PsError::MemoryExceeded));
    assert_eq!(it.run_str("up uappend"), Err(PsError::MemoryExceeded));
    it.set_limits(Limits::none()).expect("no limits");
    it.capture_stdout();
    it.run_str(
        "0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 1 add } { 1 add } pathforall =",
    )
    .expect("the path is still there");
    assert_eq!(it.captured_stdout(), b"1000001\n");

    // `stroke` and `strokepath` drop a zero-length segment of a subpath
    // that has length (#1224), which makes a transient copy of the path. It
    // is held against the cap before it is made (Codex, PR #1454: a path
    // of 3,000,000 lines stroked under 120 MiB took its ~100 MB copy past a
    // poll), and a refusal leaves the path and the graphics state as they
    // were.
    for op in ["stroke", "strokepath"] {
        let mut it = Interp::new();
        it.run_str(
            "newpath 0 0 moveto 0 0 lineto 3000000 { 1 0 rlineto } repeat \
             3 setlinewidth 1 setlinecap",
        )
        .unwrap();
        it.set_limits(Limits::none().with_max_memory(80 * MIB))
            .expect("the allocator is installed");
        let before = pscat::memtrack::live_bytes();
        let err = it.run_str(op).unwrap_err();
        assert_eq!(err, PsError::MemoryExceeded, "{op}");
        let report = it.error_report(&err);
        assert!(
            report.contains(&format!("OffendingCommand: {op};")),
            "{op}: {report}"
        );
        // Refused before the copy was made, not freed after it.
        let grown = pscat::memtrack::live_bytes().saturating_sub(before);
        assert!(grown < 8 * MIB, "{op}: the copy was made ({grown} bytes)");
        it.set_limits(Limits::none()).unwrap();
        it.capture_stdout();
        it.run_str(
            "0 { pop pop 1 add } { pop pop 1 add } { 6 { pop } repeat 1 add } { 1 add } \
             pathforall = currentlinewidth = currentlinecap =",
        )
        .unwrap();
        assert_eq!(
            it.captured_stdout(),
            b"3000002\n3.0\n1\n",
            "{op}: path and state as they were"
        );
        drop(it);
    }
}
