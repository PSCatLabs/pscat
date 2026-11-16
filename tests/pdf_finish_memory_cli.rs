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
    let pdf = dir.join("doc.pdf");
    let _ = std::fs::remove_file(&pdf);
    let (ok, stderr) = run_to(src, mib, &pdf, &dir);
    (ok, stderr, std::fs::read(&pdf).ok())
}

/// The same, writing to `out` (a path of the test's choosing); the source
/// is a file in `dir`.
fn run_to(src: &str, mib: u32, out: &std::path::Path, dir: &std::path::Path) -> (bool, String) {
    let doc = dir.join("doc.ps");
    std::fs::write(&doc, src).expect("write doc");
    let out = Command::new(env!("CARGO_BIN_EXE_pscat"))
        .args(["--max-memory", &mib.to_string(), "--pdf"])
        .arg(out)
        .arg(&doc)
        .output_deadline()
        .expect("run pscat");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
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

#[test]
fn a_run_a_limit_stopped_still_writes_its_page() {
    // The run is stopped at its cap, so the writer works in the margin: the
    // page drawn before the stop is written, as it was before this limit.
    let src = "0.2 0.4 0.8 setrgbcolor 50 50 300 300 rectfill \
               /keep 100000 array def 0 1 99999 { keep exch 100000 array put } for";
    let (ok, stderr, file) = run(src, 50);
    assert!(!ok, "{stderr}");
    assert!(
        stderr.contains("limitexceeded (memory); OffendingCommand: array"),
        "{stderr}"
    );
    assert!(file.is_some_and(|f| f.ends_with(b"%%EOF\n")), "{stderr}");
}

#[test]
fn a_long_non_ascii_title_is_written_a_piece_at_a_time() {
    // A DSC title is held once, but as UTF-16 hex it is four bytes of file
    // for each of its own: building that value before writing it would need
    // several times the room the writer has (Codex, PR #1236).
    let mut src = b"%%Title: ".to_vec();
    src.extend(std::iter::repeat_n(b'A', 12 << 20));
    src.extend("\u{e9}\nshowpage\n".bytes());
    let src = String::from_utf8(src).unwrap();
    let (ok, stderr, file) = run(&src, 30);
    assert!(ok, "{stderr}");
    let file = file.expect("the file is written");
    assert!(file.len() > 48 << 20, "{} bytes", file.len());
    assert!(
        file.windows(10).any(|w| w == b"/Title <FE"),
        "the title is a UTF-16 hex string"
    );
}

/// A refused write leaves an existing file, and the file a symlink points
/// to, exactly as they were; a good write through a symlink updates the
/// target and keeps the link (Codex, PR #1236).
#[cfg(unix)]
#[test]
fn a_refused_write_leaves_the_existing_file_and_a_symlinked_target_alone() {
    let src = incompressible();
    let dir = common::scratch_dir("pdf_finish_memory_dest");
    let old = b"%PDF-1.4\nan earlier document\n%%EOF\n";

    let plain = dir.join("plain.pdf");
    std::fs::write(&plain, old).unwrap();
    let (ok, stderr) = run_to(&src, 44, &plain, &dir);
    assert!(!ok, "{stderr}");
    assert!(stderr.contains("limitexceeded (memory)"), "{stderr}");
    assert_eq!(std::fs::read(&plain).unwrap(), old, "a plain file");

    let real = dir.join("real");
    std::fs::create_dir_all(&real).unwrap();
    let target = real.join("target.pdf");
    std::fs::write(&target, old).unwrap();
    let link = dir.join("link.pdf");
    std::os::unix::fs::symlink("real/target.pdf", &link).unwrap();
    let (ok, stderr) = run_to(&src, 44, &link, &dir);
    assert!(!ok, "{stderr}");
    assert_eq!(std::fs::read(&target).unwrap(), old, "the link's target");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());

    // Nothing is left beside them but themselves.
    let mut left: Vec<String> = std::fs::read_dir(&real)
        .unwrap()
        .chain(std::fs::read_dir(&dir).unwrap())
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert!(
        left.iter().all(|n| !n.contains(".tmp")),
        "a temporary file is left: {left:?}"
    );

    // With room, the write goes through the link to the target.
    let (ok, stderr) = run_to(&src, 72, &link, &dir);
    assert!(ok, "{stderr}");
    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    let written = std::fs::read(&target).unwrap();
    assert!(written.starts_with(b"%PDF-1.4\n") && written.ends_with(b"%%EOF\n"));
    assert!(
        std::fs::read_dir(&real).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp"))
    );
}
