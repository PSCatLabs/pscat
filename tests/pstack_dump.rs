//! Bounds on the `--pstack-on-error` operand-stack dump (issue #703).
//! These pin the bounded dump's text (`==`, `pstack` and `stack` share the
//! printer, with their own limits: `tests/operator_output_bounds.rs`).

use pscat::name;
use pscat::object::{DumpLimits, Object, STACK_DUMP_LIMITS, Value, stack_repr_bounded};

fn big_limits(max_bytes: usize) -> DumpLimits {
    DumpLimits {
        max_bytes,
        ..STACK_DUMP_LIMITS
    }
}

#[test]
fn under_the_limits_the_dump_equals_repr() {
    let stack = vec![
        Object::int(1),
        Object::string(b"a\n".to_vec()),
        Object::array(vec![
            Object::int(2),
            Object::real(2.5),
            Object::name("x"),
            Object::array(vec![]),
        ]),
        Object::procedure(vec![Object::exec(Value::Name(name::intern("add")))]),
        Object::bool(true),
        Object::lit(Value::Null),
    ];
    let expected: Vec<String> = stack.iter().map(Object::repr).collect();
    assert_eq!(
        stack_repr_bounded(&stack, &STACK_DUMP_LIMITS),
        expected.join(" ")
    );
    assert_eq!(stack_repr_bounded(&[], &STACK_DUMP_LIMITS), "");
}

#[test]
fn elements_beyond_the_count_limit_are_elided_with_a_marker() {
    let lim = DumpLimits {
        max_elements: 3,
        ..STACK_DUMP_LIMITS
    };
    let arr = Object::array((1..=5).map(Object::int).collect());
    assert_eq!(stack_repr_bounded(&[arr], &lim), "[1 2 3 ... truncated]");
}

#[test]
fn nesting_beyond_the_depth_limit_is_elided() {
    let lim = DumpLimits {
        max_depth: 2,
        ..STACK_DUMP_LIMITS
    };
    let deep = Object::array(vec![Object::array(vec![Object::array(vec![Object::int(
        1,
    )])])]);
    assert_eq!(stack_repr_bounded(&[deep], &lim), "[[... truncated]]");
}

#[test]
fn long_strings_are_cut_before_escaping() {
    let lim = DumpLimits {
        max_string: 4,
        ..STACK_DUMP_LIMITS
    };
    let s = Object::string(b"abcdefgh".to_vec());
    assert_eq!(stack_repr_bounded(&[s], &lim), "(abcd ... truncated)");
}

#[test]
fn a_cut_stack_keeps_the_top_and_marks_the_elided_bottom() {
    let stack: Vec<Object> = (1..=5).map(Object::int).collect();
    // Each entry costs its text plus one separator: "5" and "4" fit in 4 bytes.
    assert_eq!(
        stack_repr_bounded(&stack, &big_limits(4)),
        "... truncated 4 5"
    );
}

#[test]
fn shared_structure_is_bounded_without_expanding_it() {
    // 2^40 leaves if expanded; the dump must stop early.
    let mut a = Object::array(vec![Object::int(0)]);
    for _ in 0..40 {
        a = Object::array(vec![a.clone(), a.clone()]);
    }
    let text = stack_repr_bounded(&[a], &STACK_DUMP_LIMITS);
    assert!(text.len() < 16 * 1024, "dump is {} bytes", text.len());
    assert!(text.contains("... truncated"));
}

#[test]
fn huge_arrays_strings_and_stacks_are_bounded() {
    let arr = Object::array((0..1_000_000).map(Object::int).collect());
    let text = stack_repr_bounded(&[arr], &STACK_DUMP_LIMITS);
    assert!(
        text.starts_with("[0 1 2 "),
        "{}",
        &text[..text.len().min(40)]
    );
    assert!(text.len() < 16 * 1024, "array dump is {} bytes", text.len());

    let s = Object::string(vec![b'a'; 1_000_000]);
    let text = stack_repr_bounded(&[s], &STACK_DUMP_LIMITS);
    assert!(text.len() < 1024, "string dump is {} bytes", text.len());

    let stack: Vec<Object> = (0..1_000_000).map(Object::int).collect();
    let text = stack_repr_bounded(&stack, &STACK_DUMP_LIMITS);
    assert!(text.starts_with("... truncated "));
    assert!(text.ends_with(" 999999"), "the top survives: {text}");
    assert!(text.len() < 16 * 1024, "stack dump is {} bytes", text.len());
}
