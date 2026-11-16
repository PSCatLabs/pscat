//! Writing a PDF stays inside `--max-memory` (#1107). Before it, `finish`
//! built every object and then a copy of the whole file, so a document that
//! fit while running could take several times its stored size to write.
//!
//! It counts the whole process's heap, so everything runs in one test of this
//! binary (a second test would count its allocations too), with the tracking
//! allocator installed as the `pscat` binary does.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

use pscat::pdf::PdfWriteError;
use pscat::{Interp, Limits};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

/// The highest heap level a helper thread sees while `work` runs.
fn peak_during<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let stop = Arc::new(AtomicBool::new(false));
    let peak = Arc::new(AtomicUsize::new(0));
    let watcher = {
        let (stop, peak) = (stop.clone(), peak.clone());
        std::thread::spawn(move || {
            while !stop.load(Relaxed) {
                peak.fetch_max(pscat::memtrack::live_bytes(), Relaxed);
                std::thread::sleep(std::time::Duration::from_micros(200));
            }
        })
    };
    let out = work();
    stop.store(true, Relaxed);
    watcher.join().unwrap();
    (out, peak.load(Relaxed))
}

/// A sink that keeps nothing: a length and a hash of what it saw, the same
/// however the bytes are split across writes.
struct Hasher {
    len: usize,
    hash: u64,
    /// The last few bytes, until there are eight to take.
    carry: Vec<u8>,
}

impl Hasher {
    fn new() -> Self {
        Hasher {
            len: 0,
            hash: 0xcbf2_9ce4_8422_2325,
            carry: Vec::new(),
        }
    }
    fn word(&mut self, w: &[u8]) {
        let w = u64::from_le_bytes(w.try_into().unwrap());
        self.hash = (self.hash ^ w)
            .wrapping_mul(0x100_0000_01b3)
            .rotate_left(23);
    }
    fn result(mut self) -> (usize, u64) {
        let mut carry = std::mem::take(&mut self.carry);
        carry.resize(8, 0xff);
        self.word(&carry);
        (self.len, self.hash)
    }
    fn of(bytes: &[u8]) -> (usize, u64) {
        let mut h = Hasher::new();
        h.write_all(bytes).unwrap();
        h.result()
    }
}

impl Write for Hasher {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        let len = data.len();
        self.len += len;
        let mut data = data;
        // Eight bytes at a time: the debug build counts every byte.
        while !self.carry.is_empty() && self.carry.len() < 8 && !data.is_empty() {
            self.carry.push(data[0]);
            data = &data[1..];
        }
        if self.carry.len() == 8 {
            let c = std::mem::take(&mut self.carry);
            self.word(&c);
        }
        let mut words = data.chunks_exact(8);
        for w in words.by_ref() {
            self.word(w);
        }
        self.carry.extend_from_slice(words.remainder());
        Ok(len)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn recording(limit_mib: usize, src: &str) -> Interp {
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_pdf();
    it.set_limits(Limits::none().with_max_memory(limit_mib * MIB))
        .expect("the allocator is installed");
    if let Err(e) = it.run_str(src) {
        panic!(
            "the program must run inside the limit: {}",
            it.error_report(&e)
        );
    }
    it
}

/// 8 MiB of NUL bytes held by an array object: each is four bytes of file
/// (`\000`), so the file is 32 MiB.
const NULS: &str = "/s 1048576 string def \
    [ /_objdef {x} /type /array /OBJ pdfmark \
    1 1 8 { pop [ {x} s /APPEND pdfmark } for showpage";

/// 20 MiB of bytes deflate cannot shrink: a 40000-byte pseudo-random block,
/// put 525 times (a repeat 40000 bytes back is outside deflate's 32 KiB
/// window).
fn incompressible() -> String {
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    let hex: String = (0..40_000)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            format!("{:02x}", (x >> 24) as u8)
        })
        .collect();
    format!(
        "/r <{hex}> def [ /_objdef {{s}} /type /stream /OBJ pdfmark \
         1 1 525 {{ pop [ {{s}} r /PUT pdfmark }} for showpage"
    )
}

#[test]
fn writing_a_pdf_stays_inside_the_memory_limit() {
    assert!(pscat::memtrack::installed());

    // 1. A document that is mostly one big escaped string. The file is four
    //    times what is stored; written to a sink that keeps nothing, the
    //    writer holds a chunk, not the file.
    let it = recording(16, NULS);
    let base_level = pscat::memtrack::live_bytes();
    let (written, peak) = peak_during(|| {
        let mut sink = Hasher::new();
        let r = it.write_pdf(&mut sink).expect("recording");
        (r, sink)
    });
    let (result, sink) = written;
    result.expect("written");
    assert!(sink.len > 32 * MIB, "the file is {} bytes", sink.len);
    assert!(
        peak.saturating_sub(base_level) < 8 * MIB,
        "writing to a sink grew the heap by {} bytes",
        peak.saturating_sub(base_level)
    );
    // Those bytes are the ones `pdf_document` has always returned.
    let whole = it.pdf_document().expect("recording");
    assert_eq!(sink.result(), Hasher::of(&whole));
    drop(whole);

    // 2. The same document as a `Vec` does not fit in 16 MiB (and the 16 MiB the writer may add): it is refused
    //    before the Vec takes the room, and never past the limit by more
    //    than the one growth step it was asked about.
    let before = pscat::memtrack::live_bytes();
    let (refused, peak) = peak_during(|| it.pdf_document_limited().expect("recording"));
    assert!(
        matches!(refused, Err(PdfWriteError::Memory)),
        "a 32 MiB Vec under a 16 + 16 MiB limit: {:?}",
        refused.map(|v| v.len())
    );
    assert!(
        peak.saturating_sub(before) < 32 * MIB,
        "refused only after growing the heap by {} bytes",
        peak.saturating_sub(before)
    );
    drop(it);

    // 3. The same program under a bigger limit gives the same bytes through
    //    every path.
    let it = recording(96, NULS);
    let vec = it
        .pdf_document_limited()
        .expect("recording")
        .expect("fits in 96 MiB");
    assert_eq!(vec.len(), it.pdf_document().expect("recording").len());
    drop(vec);
    drop(it);

    // 4. A refusal for a working buffer: the program keeps 20 MiB that
    //    deflate cannot shrink, so packing it takes another 20, past the
    //    44 MiB limit (the run itself grows the heap by about 41: the stored data has doubled its way to 32) and the 16 MiB
    //    the writer may add. The sink got a
    //    partial file; the error says why.
    let src = incompressible();
    let it = recording(44, &src);
    let mut sink = Hasher::new();
    let refused = it.write_pdf(&mut sink).expect("recording");
    assert!(
        matches!(refused, Err(PdfWriteError::Memory)),
        "packing 20 MiB under a 44 + 16 MiB limit: {refused:?}"
    );
    assert!(sink.len < 4 * MIB, "wrote {} bytes", sink.len);
    drop(it);

    // 5. With room it writes, and a limit that is not set never refuses.
    let it = recording(72, &src);
    let mut sink = Hasher::new();
    it.write_pdf(&mut sink).expect("recording").expect("fits");
    let whole = it.pdf_document().expect("recording");
    assert_eq!(sink.result(), Hasher::of(&whole));
    drop(it);
    let mut it = Interp::with_page(100, 100).expect("page");
    it.gfx_mut().enable_pdf();
    it.run_str(&src).expect("runs");
    assert!(it.pdf_document_limited().expect("recording").is_ok());
}
