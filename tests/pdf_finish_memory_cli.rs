//! `pscat --pdf` under `--max-memory` (#1107): the document is written to
//! the file as it is built, and one that cannot be written inside the limit
//! is refused as a memory limit error with no file left behind.

use std::process::Command;

mod common;

use common::CommandDeadline as _;

/// 8 MiB of NUL bytes in an array object: four bytes of file each.
const NULS: &str = "/s 1048576 string def \
    [ /_objdef {x} /type /array /OBJ pdfmark \
    1 1 8 { pop [ {x} s /APPEND pdfmark } for showpage";

/// 20 MiB deflate cannot shrink: a 40000-byte pseudo-random block, 525 times.
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

/// Run `src` as `pscat --max-memory MIB --pdf out.pdf`: the exit status,
/// stderr, and the file if there is one.
fn run(src: &str, mib: u32) -> (bool, String, Option<Vec<u8>>) {
    let dir = common::scratch_dir("pdf_finish_memory");
    let doc = dir.join("doc.ps");
    let pdf = dir.join("doc.pdf");
    std::fs::write(&doc, src).expect("write doc");
    let _ = std::fs::remove_file(&pdf);
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--max-memory", &mib.to_string(), "--pdf"])
        .arg(&pdf)
        .arg(&doc)
        .output_deadline()
        .expect("run pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        std::fs::read(&pdf).ok(),
    )
}

#[test]
fn a_document_four_times_what_it_stores_is_written_inside_the_limit() {
    // 8 MiB stored, a 32 MiB file, 16 MiB allowed (and 16 MiB more for the
    // writer): the file does not fit, but it is written, not held.
    let (ok, stderr, file) = run(NULS, 16);
    assert!(ok, "{stderr}");
    let file = file.expect("the file is written");
    assert!(file.len() > 32 << 20, "{} bytes", file.len());
    assert!(file.starts_with(b"%PDF-1.4\n") && file.ends_with(b"%%EOF\n"));
}

#[test]
fn a_document_that_cannot_be_packed_inside_the_limit_is_refused() {
    let src = incompressible();
    let (ok, stderr, file) = run(&src, 44);
    assert!(!ok, "{stderr}");
    assert!(
        file.is_none(),
        "no file is left behind ({} bytes)",
        file.map_or(0, |f| f.len())
    );
    // The line the MCP server reads back as a `limit_exceeded` with
    // `limit: memory`, and a sentence for a person.
    assert!(
        stderr.contains("%%[ Error: limitexceeded (memory); OffendingCommand: --pdf ]%%"),
        "{stderr}"
    );
    assert!(stderr.contains("--max-memory"), "{stderr}");
    // The same program with room for the packed stream is written.
    let (ok, stderr, file) = run(&src, 72);
    assert!(ok, "{stderr}");
    assert!(file.is_some_and(|f| f.ends_with(b"%%EOF\n")));
}
