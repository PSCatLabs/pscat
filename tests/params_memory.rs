//! The parameter operators under the memory limit (#1270). A string
//! parameter is copied when it is set and again each time it is read, so a
//! program holding a big string could ask for megabytes in one step: the copy
//! is refused against `--max-memory` before it is made. It counts the whole
//! process's heap, so this binary has exactly one test and nothing else
//! allocates alongside it (the shape of `user_objects_memory.rs`).

use pscat::{Interp, Limits, PsError};

#[global_allocator]
static ALLOC: pscat::TrackingAllocator = pscat::TrackingAllocator;

const MIB: usize = 1 << 20;

fn read_back(it: &mut Interp, src: &str) -> String {
    it.set_limits(Limits::none()).expect("no limit");
    it.capture_stdout();
    it.run_str(src).expect("the interpreter still runs");
    String::from_utf8_lossy(it.captured_stdout())
        .trim()
        .to_string()
}

#[test]
fn a_string_parameter_is_copied_only_with_room_for_the_copy() {
    assert!(pscat::memtrack::installed());
    let mut it = Interp::new();
    it.run_str("/big 8388608 string def").expect("a big string");

    // Setting: the copy would take 8 MiB, the limit leaves 2.
    it.set_limits(Limits::none().with_max_memory(2 * MIB))
        .expect("the allocator is installed");
    for set in [
        "<< /JobName big >> setuserparams",
        "<< /PrinterName big >> setsystemparams",
        "<< /LicenseID big >> setsystemparams",
        "<< /CurInputDevice big >> setsystemparams",
    ] {
        assert_eq!(it.run_str(set), Err(PsError::MemoryExceeded), "{set}");
        // The operand the refusal gave back is the dictionary: the stack is
        // as it was before the operator.
        assert_eq!(
            read_back(&mut it, "count = clear"),
            "1",
            "{set}: the dictionary operand is back"
        );
        it.set_limits(Limits::none().with_max_memory(2 * MIB))
            .expect("the allocator is installed");
    }
    assert_eq!(
        read_back(&mut it, "currentuserparams /JobName get length ="),
        "0",
        "a refused set left JobName alone"
    );

    // With room for the copy it is made.
    it.set_limits(Limits::none().with_max_memory(32 * MIB))
        .expect("the allocator is installed");
    it.run_str("<< /JobName big >> setuserparams")
        .expect("room for the copy");

    // Reading: the string comes back as a fresh copy, refused the same way.
    it.set_limits(Limits::none().with_max_memory(2 * MIB))
        .expect("the allocator is installed");
    assert_eq!(it.run_str("currentuserparams"), Err(PsError::MemoryExceeded));
    assert_eq!(read_back(&mut it, "count ="), "0", "nothing was pushed");
    it.set_limits(Limits::none().with_max_memory(32 * MIB))
        .expect("the allocator is installed");
    it.run_str("currentuserparams /JobName get length =")
        .expect("room for the copy");
}
