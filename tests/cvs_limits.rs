//! `cvs` of a large string against the execution limits (#1428, Codex round 1).
//! The text of a string is a copy of its bytes, so the copy is asked of
//! `--max-memory` before it is made and counted as work (a step per KiB, as
//! for a file read). The memory limit counts the whole process's heap, so
//! this is a test binary of its own with one test, as `tests/limits_memory.rs`.

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

#[test]
fn cvs_of_a_large_string_is_preflighted_and_charged() {
    assert!(pscat::memtrack::installed());

    // 16 MiB string, and its 16 MiB copy, under a 24 MiB cap: the copy is
    // refused before it is made. (No step limit set, so only memory speaks.)
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_memory(24 * MIB))
        .expect("the allocator is installed");
    assert_eq!(
        it.run_str("16777216 string dup cvs length pop"),
        Err(PsError::MemoryExceeded)
    );

    // The same copy with room for it succeeds.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_memory(100 * MIB))
        .expect("the allocator is installed");
    assert_eq!(it.run_str("16777216 string dup cvs length pop"), Ok(()));

    // Each 16 MiB copy costs 16384 steps: a hundred of them pass a 1000-step
    // cap, with the memory cap loose enough that only steps can stop it.
    let mut it = Interp::new();
    it.set_limits(
        Limits::none()
            .with_max_memory(100 * MIB)
            .with_max_steps(1000),
    )
    .expect("the allocator is installed");
    assert_eq!(
        it.run_str("/s 16777216 string def 100 { s dup cvs pop } repeat"),
        Err(PsError::StepsExceeded)
    );

    // `=` and `print` of a large string write it from its own storage (no
    // copy) and count it as work, a step per KiB.
    for show in ["=", "=only", "print"] {
        let mut it = Interp::new();
        it.set_limits(
            Limits::none()
                .with_max_memory(100 * MIB)
                .with_max_steps(1000),
        )
        .expect("the allocator is installed");
        it.capture_stdout();
        assert_eq!(
            it.run_str(&format!("/s 16777216 string def 100 {{ s {show} }} repeat")),
            Err(PsError::StepsExceeded),
            "{show}"
        );
    }

    // A `noaccess` string or buffer is refused before the copy is asked of
    // the memory cap or counted as work (#1362): `invalidaccess`, where an
    // unrefused call would be `MemoryExceeded` or run out of steps.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_memory(24 * MIB))
        .expect("the allocator is installed");
    it.capture_stdout();
    assert_eq!(
        it.run_str(
            "/s 16777216 string noaccess def \
             { s dup cvs } stopped { $error /errorname get = count = clear } if \
             { 7 s cvs } stopped { $error /errorname get = count = clear } if"
        ),
        Ok(())
    );
    assert_eq!(
        it.captured_stdout(),
        b"invalidaccess\n2\ninvalidaccess\n2\n"
    );
    let mut it = Interp::new();
    it.set_limits(
        Limits::none()
            .with_max_memory(100 * MIB)
            .with_max_steps(1000),
    )
    .expect("the allocator is installed");
    assert_eq!(
        it.run_str(
            "/s 16777216 string noaccess def 100 { { s dup cvs } stopped pop clear } repeat"
        ),
        Ok(())
    );

    // A string too long for its buffer is a rangecheck before any copy, and
    // `s s cvs` (one storage for both operands) still reads its own bytes.
    let mut it = Interp::new();
    it.set_limits(Limits::none().with_max_memory(24 * MIB))
        .expect("the allocator is installed");
    it.capture_stdout();
    assert_eq!(
        it.run_str(
            "{ 16777216 string 4 string cvs } stopped { $error /errorname get = } if \
             (a\\351b) dup dup cvs length ="
        ),
        Ok(())
    );
    assert_eq!(it.captured_stdout(), b"rangecheck\n3\n");
}
