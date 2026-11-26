//! Virtual files (#1407): `Interp::add_virtual_file` serves named sources to
//! `run`, `(name) (r) file`, `readreals` and `status` from memory. Each test
//! uses only the supported surface, as an embedding application would.

use std::cell::Cell;
use std::rc::Rc;

use pscat::host::{MAX_VIRTUAL_FILE_BYTES, MAX_VIRTUAL_FILES, MAX_VIRTUAL_NAME_BYTES};
use pscat::{Interp, Limits, PsError};

mod common;

/// The top string operand, as text.
fn text(it: &mut Interp) -> String {
    String::from_utf8(it.pop_string().expect("a string")).expect("utf-8")
}

/// A path as a PostScript string literal.
fn lit(p: &std::path::Path) -> String {
    format!("({})", common::ps_path(p))
}

fn interp() -> Interp {
    Interp::with_page(20, 20).expect("valid page size")
}

#[test]
fn run_loads_a_virtual_file_with_no_disk() {
    let mut it = interp();
    it.add_virtual_file("components/foo.ps", b"/foo { 7 } def")
        .unwrap();
    it.run_str("(components/foo.ps) run foo").unwrap();
    assert_eq!(it.pop_int().unwrap(), 7);
    // A second program runs it again; the file is not consumed.
    it.run_str("(components/foo.ps) run foo 1 add").unwrap();
    assert_eq!(it.pop_int().unwrap(), 8);
}

#[test]
fn file_reads_the_bytes_and_status_sees_the_file() {
    let mut it = interp();
    it.add_virtual_file("data/blob.bin", vec![1u8, 2, 3, 4, 5])
        .unwrap();
    it.run_str("(data/blob.bin) (r) file 3 string readstring")
        .unwrap();
    assert!(it.pop_bool().unwrap());
    assert_eq!(it.pop_string().unwrap(), [1, 2, 3]);
    it.run_str("(data/blob.bin) status").unwrap();
    assert!(it.pop_bool().unwrap());
    assert_eq!(it.pop_int().unwrap(), 0); // created
    assert_eq!(it.pop_int().unwrap(), 0); // referenced
    assert_eq!(it.pop_int().unwrap(), 5); // bytes
    assert_eq!(it.pop_int().unwrap(), 1); // pages (512-byte blocks)
    it.run_str("(data/nothing.bin) status").unwrap();
    assert!(!it.pop_bool().unwrap());
}

#[test]
fn readreals_reads_a_virtual_file() {
    let mut it = interp();
    let bytes: Vec<u8> = [1.5f32, 2.5].iter().flat_map(|v| v.to_le_bytes()).collect();
    it.add_virtual_file("data/reals.bin", bytes).unwrap();
    it.run_str("(data/reals.bin) /all /f32 readreals").unwrap();
    let values = it.pop_numbers().unwrap();
    assert_eq!(values.len(), 2);
    assert!(values.iter().all(|v| *v == 1.5 || *v == 2.5), "{values:?}");
}

#[test]
fn a_leading_dot_slash_is_the_same_name_and_registering_again_replaces() {
    let mut it = interp();
    it.add_virtual_file("./a.ps", b"1").unwrap();
    it.run_str("(a.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 1);
    it.add_virtual_file("a.ps", b"2").unwrap();
    it.run_str("(./a.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 2);
}

#[test]
fn a_removed_file_is_gone_and_an_unregistered_name_is_undefinedfilename() {
    let mut it = interp();
    it.add_virtual_file("gone.ps", b"1").unwrap();
    assert!(it.remove_virtual_file("./gone.ps"));
    assert!(!it.remove_virtual_file("gone.ps"));
    assert_eq!(it.run_str("(gone.ps) run"), Err(PsError::UndefinedFilename));
    assert_eq!(
        it.run_str("(nothing/here.ps) (r) file"),
        Err(PsError::UndefinedFilename)
    );
}

#[test]
fn a_virtual_name_shadows_disk_and_the_embedded_lib_and_others_fall_through() {
    let dir = common::scratch_dir("virtual-shadow");
    std::fs::write(dir.join("on_disk.ps"), "(disk)").unwrap();
    let on_disk = lit(&dir.join("on_disk.ps"));
    let mut it = interp();

    // Unregistered: the disk file, then the embedded lib, as before.
    it.run_str(&format!("{on_disk} run")).unwrap();
    assert_eq!(text(&mut it), "disk");
    it.run_str("(lib/functional.ps) run").unwrap();

    // Registered under the same names: the host's bytes win.
    // Registered under the spelling the program uses (`/` separators).
    it.add_virtual_file(&common::ps_path(&dir.join("on_disk.ps")), b"(memory)")
        .unwrap();
    it.add_virtual_file("lib/functional.ps", b"/replaced true def")
        .unwrap();
    it.run_str(&format!("{on_disk} run")).unwrap();
    assert_eq!(text(&mut it), "memory");
    it.run_str("/replaced where { pop (no) } { (yes) } ifelse")
        .unwrap();
    assert_eq!(text(&mut it), "yes");
    it.run_str("(lib/functional.ps) run replaced").unwrap();
    assert!(it.pop_bool().unwrap());

    // Take them away and the originals are back.
    assert!(it.remove_virtual_file("lib/functional.ps"));
    it.run_str("(lib/functional.ps) run").unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_roots_do_not_gate_a_virtual_file_but_still_gate_everything_else() {
    let dir = common::scratch_dir("virtual-roots");
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(dir.join("outside.ps"), "1").unwrap();
    let outside = dir.join("outside.ps");
    let mut it = interp();
    it.set_read_roots(&[&root]).unwrap();
    it.add_virtual_file("components/foo.ps", b"42").unwrap();
    let known = dir.join("known.ps");
    it.add_virtual_file(&common::ps_path(&known), b"43")
        .unwrap();

    it.run_str("(components/foo.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 42);
    it.run_str(&format!("{} run", lit(&known))).unwrap();
    assert_eq!(it.pop_int().unwrap(), 43);

    // Names nobody registered keep the read-root rules.
    let src = format!("{} run", lit(&outside));
    assert_eq!(it.run_str(&src), Err(PsError::InvalidFileAccess));
    let other = format!("{} run", lit(&dir.join("other.ps")));
    assert_eq!(it.run_str(&other), Err(PsError::InvalidFileAccess));
    assert_eq!(
        it.run_str("(components/../x.ps) run"),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(
        it.run_str("(components/missing.ps) run"),
        Err(PsError::UndefinedFilename)
    );
    // `status` never says what exists outside the roots.
    let src = format!("{} status", lit(&outside));
    it.run_str(&src).unwrap();
    assert!(!it.pop_bool().unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_write_to_a_virtual_name_still_goes_to_the_write_policy() {
    let mut it = interp();
    it.add_virtual_file("out.txt", b"x").unwrap();
    assert_eq!(
        it.run_str("(out.txt) (w) file"),
        Err(PsError::InvalidFileAccess)
    );
    // The file it names is untouched.
    it.run_str("(out.txt) (r) file read").unwrap();
    assert!(it.pop_bool().unwrap());
    assert_eq!(it.pop_int().unwrap(), i64::from(b'x'));
}

#[test]
fn bad_names_and_oversize_files_are_refused_and_change_nothing() {
    let mut it = interp();
    assert_eq!(it.add_virtual_file("", b"x"), Err(PsError::Rangecheck));
    assert_eq!(it.add_virtual_file("a\0b", b"x"), Err(PsError::Rangecheck));
    assert_eq!(
        it.add_virtual_file("%stdout", b"x"),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(
        it.add_virtual_file("../x.ps", b"x"),
        Err(PsError::InvalidFileAccess)
    );
    assert_eq!(
        it.add_virtual_file("a/../b.ps", b"x"),
        Err(PsError::InvalidFileAccess)
    );

    it.add_virtual_file("ok.ps", b"1").unwrap();
    let too_big = vec![b' '; MAX_VIRTUAL_FILE_BYTES + 1];
    assert_eq!(
        it.add_virtual_file("ok.ps", too_big),
        Err(PsError::Limitcheck)
    );
    // The earlier registration is still there, and the largest file fits.
    it.run_str("(ok.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 1);
    it.add_virtual_file("max.ps", vec![b' '; MAX_VIRTUAL_FILE_BYTES])
        .unwrap();
}

#[test]
fn the_number_of_files_is_bounded() {
    let mut it = interp();
    for i in 0..MAX_VIRTUAL_FILES {
        it.add_virtual_file(&format!("f/{i}.ps"), b"").unwrap();
    }
    assert_eq!(
        it.add_virtual_file("one/more.ps", b""),
        Err(PsError::Limitcheck)
    );
    // Replacing a registered name is not a new file.
    it.add_virtual_file("f/0.ps", b"1").unwrap();
    assert!(it.remove_virtual_file("f/1.ps"));
    it.add_virtual_file("one/more.ps", b"2").unwrap();
}

#[test]
fn a_host_operator_can_register_a_file_for_the_program_that_called_it() {
    let mut it = interp();
    it.register_operator("provide", |i| {
        i.add_virtual_file("made/later.ps", b"/later 99 def")
    })
    .unwrap();
    it.run_str("provide (made/later.ps) run later").unwrap();
    assert_eq!(it.pop_int().unwrap(), 99);
}

// ---- source positions (#1405) ----

#[test]
fn an_error_in_a_virtual_file_names_it_in_the_report_and_the_frames() {
    let mut it = interp();
    it.add_virtual_file(
        "components/foo.ps",
        b"/fine { 1 } def\n/boom {\n  1 0 div\n} def\n",
    )
    .unwrap();
    it.run_str("(components/foo.ps) run").unwrap();
    let err = it.run_str("boom").unwrap_err();
    let details = it.error_details(&err);
    assert_eq!(details.name, "undefinedresult");
    let frame = &details.backtrace[0];
    assert_eq!(frame.procedure.as_deref(), Some("boom"));
    assert_eq!(frame.file.as_deref(), Some("components/foo.ps"));
    assert_eq!(frame.line, Some(3));

    // An error at the file's own top level names it as the error's file.
    it.add_virtual_file("components/bad.ps", b"1\n2 0 idiv\n")
        .unwrap();
    let err = it.run_str("(components/bad.ps) run").unwrap_err();
    let details = it.error_details(&err);
    assert_eq!(details.file.as_deref(), Some("components/bad.ps"));
    assert_eq!(details.line, Some(2));
    assert!(it.error_report(&err).contains("components/bad.ps"));
}

#[test]
fn currentfilename_inside_a_virtual_file_is_its_name() {
    let mut it = interp();
    it.add_virtual_file("components/who.ps", b"currentfilename")
        .unwrap();
    it.run_str("(components/who.ps) run").unwrap();
    assert_eq!(text(&mut it), "components/who.ps");
}

// ---- limits ----

fn big_file(kib: usize) -> Vec<u8> {
    // A comment line repeated: a valid program that costs `kib` KiB to read.
    let mut v = Vec::with_capacity(kib * 1024);
    while v.len() < kib * 1024 {
        v.extend_from_slice(b"% padding padding padding padding padding padding padding\n");
    }
    v.truncate(kib * 1024);
    v
}

#[test]
fn serving_a_file_is_charged_against_the_step_limit() {
    let mut it = interp();
    it.add_virtual_file("big.ps", big_file(3000)).unwrap();
    it.set_limits(Limits::none().with_max_steps(1_000)).unwrap();
    assert_eq!(it.run_str("(big.ps) run"), Err(PsError::StepsExceeded));
    // A file that costs less than the cap loads.
    it.add_virtual_file("small.ps", big_file(10)).unwrap();
    it.run_str("(small.ps) run 1 pop").unwrap();
    // A program that loads it in a loop meets the cap.
    assert_eq!(
        it.run_str("100000 { (small.ps) run } repeat"),
        Err(PsError::StepsExceeded)
    );
}

#[test]
fn no_limits_means_no_charge() {
    let mut it = interp();
    it.add_virtual_file("big.ps", big_file(3000)).unwrap();
    it.run_str("1 20 { (big.ps) run } repeat").unwrap();
}

// ---- groups ----

#[test]
fn a_virtual_file_paints_into_an_open_group_like_any_program() {
    let canvas_corner = Rc::new(Cell::new([0u8; 4]));
    let mut it = interp();
    let seen = canvas_corner.clone();
    it.register_operator("peek", move |i| {
        seen.set(i.canvas_premultiplied_rgba8()[..4].try_into().unwrap());
        Ok(())
    })
    .unwrap();
    it.add_virtual_file("mark.ps", b"0 0 1 setrgbcolor 0 0 20 20 rectfill")
        .unwrap();
    it.run_str("<< >> begingroup (mark.ps) run peek endgroup")
        .unwrap();
    // The host saw the page below the layer while the group was open ...
    assert_eq!(canvas_corner.get(), [255, 255, 255, 255]);
    // ... and the group composited the file's marks at `endgroup`.
    assert_eq!(&it.canvas_premultiplied_rgba8()[..4], &[0, 0, 255, 255]);
}

// ---- Codex round 1: what registration costs and keeps ----

#[test]
fn a_name_over_the_limit_is_refused_so_names_cannot_hold_memory() {
    let mut it = interp();
    let long = "n".repeat(MAX_VIRTUAL_NAME_BYTES + 1);
    assert_eq!(it.add_virtual_file(&long, b""), Err(PsError::Limitcheck));
    // The longest name is kept and read back.
    let longest = "n".repeat(MAX_VIRTUAL_NAME_BYTES);
    it.add_virtual_file(&longest, b"7").unwrap();
    it.run_str(&format!("({longest}) run")).unwrap();
    assert_eq!(it.pop_int().unwrap(), 7);
}

#[test]
fn a_buffers_reserved_capacity_counts_against_the_bounds() {
    let mut it = interp();
    // Nothing in it, but it keeps more than a file may be.
    let hoard: Vec<u8> = Vec::with_capacity(MAX_VIRTUAL_FILE_BYTES + 1);
    assert_eq!(
        it.add_virtual_file("hoard", hoard),
        Err(PsError::Limitcheck)
    );
    // Eight empty buffers that each reserve 60 MiB fill the store (untouched
    // pages: no memory is used here); the ninth does not fit.
    for i in 0..8 {
        it.add_virtual_file(&format!("r{i}"), Vec::<u8>::with_capacity(60 << 20))
            .unwrap();
    }
    assert_eq!(
        it.add_virtual_file("r8", Vec::<u8>::with_capacity(60 << 20)),
        Err(PsError::Limitcheck)
    );
    // Replacing one with a tight buffer frees its reservation.
    it.add_virtual_file("r0", b"1").unwrap();
    it.add_virtual_file("r8", Vec::<u8>::with_capacity(60 << 20))
        .unwrap();
}

#[test]
fn a_borrowed_buffer_registered_by_a_host_operator_is_charged_as_host_work() {
    let blob = std::rc::Rc::new(vec![b' '; 1 << 20]);
    let mut it = interp();
    let b = blob.clone();
    it.register_operator("provide", move |i| i.add_virtual_file("blob.ps", &b[..]))
        .unwrap();
    it.set_limits(Limits::none().with_max_steps(5_000)).unwrap();
    // A MiB is about 1025 steps a call: a loop of them meets the cap.
    assert_eq!(
        it.run_str("100 { provide } repeat"),
        Err(PsError::StepsExceeded)
    );
    // A single registration is affordable.
    it.run_str("provide").unwrap();
}

// ---- checked beyond the review: nesting and unwinding ----

#[test]
fn a_virtual_file_that_runs_another_names_both_and_a_stopped_error_unwinds_cleanly() {
    let mut it = interp();
    it.add_virtual_file("components/outer.ps", b"(components/inner.ps) run\n")
        .unwrap();
    it.add_virtual_file("components/inner.ps", b"1\n1 0 idiv\n")
        .unwrap();
    let err = it.run_str("(components/outer.ps) run").unwrap_err();
    let report = it.error_report(&err);
    assert!(report.contains("components/inner.ps"), "{report}");
    assert!(report.contains("components/outer.ps"), "{report}");
    // Caught by `stopped`, the program goes on and a later run is whole.
    it.run_str("{ (components/inner.ps) run } stopped").unwrap();
    assert!(it.pop_bool().unwrap());
    it.add_virtual_file("ok.ps", b"42").unwrap();
    it.clear_operands();
    it.run_str("(ok.ps) run").unwrap();
    assert_eq!(it.pop_int().unwrap(), 42);
}
