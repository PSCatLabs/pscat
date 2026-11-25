//! The host integration API (`pscat::host`): operators backed by closures,
//! values across the operand stack, the current path as data, canvas
//! exchange, error details and the interrupt flag. Each test uses only the
//! supported surface, as an embedding application would.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pscat::host::{MAX_HOST_OPERATORS, PathSegment, RegisterError, Value};
use pscat::{Interp, Limits, Page, PsError};

mod common;

fn page(w: u32, h: u32) -> Interp {
    Interp::with_page(w, h).expect("valid page size")
}

#[test]
fn host_operator_is_an_operator_to_the_program() {
    let mut interp = Interp::new();
    interp
        .register_operator("twice", |i| {
            let v = i.pop_f64()?;
            i.push_real(v * 2.0);
            Ok(())
        })
        .unwrap();
    interp.capture_stdout();
    interp
        .run_str("/twice load type == /p { twice } bind def /p load 0 get type == 4 p ==")
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(interp.captured_stdout()),
        "operatortype\noperatortype\n8.0\n"
    );
}

#[test]
fn host_operator_shares_state_with_the_host() {
    let calls = Rc::new(Cell::new(0u32));
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut interp = Interp::new();
    let (c, l) = (calls.clone(), log.clone());
    interp
        .register_operator("note", move |i| {
            c.set(c.get() + 1);
            l.borrow_mut().push(i.pop_string()?);
            Ok(())
        })
        .unwrap();
    interp.run_str("(a) note 3 { (b) note } repeat").unwrap();
    assert_eq!(calls.get(), 4);
    assert_eq!(log.borrow().len(), 4);
    assert_eq!(log.borrow()[3], b"b");
}

#[test]
fn failed_host_operator_restores_operands_and_is_blamed() {
    let mut interp = Interp::new();
    interp
        .register_operator("needs2", |i| {
            i.pop_f64()?;
            i.pop_f64()?;
            Ok(())
        })
        .unwrap();
    let err = interp.run_str("1 (x) 2 needs2").unwrap_err();
    assert_eq!(err, PsError::Typecheck);
    let d = interp.error_details(&err);
    assert_eq!(d.command.as_deref(), Some("needs2"));
    // The 2 it popped before failing is back.
    assert_eq!(interp.operand_count(), 3);
    assert_eq!(interp.pop_f64().unwrap(), 2.0);
    // A program can catch it like any operator's error.
    interp.clear_operands();
    interp.run_str("{ needs2 } stopped").unwrap();
    assert!(interp.pop_bool().unwrap());
}

#[test]
fn registering_again_replaces_in_place_even_for_bound_procedures() {
    let mut interp = Interp::new();
    interp
        .register_operator("answer", |i| {
            i.push_int(1);
            Ok(())
        })
        .unwrap();
    interp.run_str("/p { answer } bind def").unwrap();
    interp
        .register_operator("answer", |i| {
            i.push_int(42);
            Ok(())
        })
        .unwrap();
    interp.run_str("p").unwrap();
    assert_eq!(interp.pop_int().unwrap(), 42);
}

#[test]
fn host_can_take_over_a_builtin() {
    let shown = Rc::new(Cell::new(0u32));
    let mut interp = page(10, 10);
    let s = shown.clone();
    interp
        .register_operator("showpage", move |_| {
            s.set(s.get() + 1);
            Ok(())
        })
        .unwrap();
    interp
        .run_str("0 0 10 10 rectfill showpage showpage")
        .unwrap();
    assert_eq!(shown.get(), 2);
    // The built-in never ran: no page was emitted, the canvas still holds the art.
    assert_eq!(interp.page_count(), 1);
    assert_eq!(&interp.canvas_premultiplied_rgba8()[..4], &[0, 0, 0, 255]);
}

#[test]
fn registration_limits_and_names() {
    let mut interp = Interp::new();
    for bad in ["", "two words", "a/b", "p(q", "x%"] {
        assert_eq!(
            interp.register_operator(bad, |_| Ok(())),
            Err(RegisterError::BadName(bad.to_string()))
        );
    }
    for n in 0..MAX_HOST_OPERATORS {
        interp
            .register_operator(&format!("hostop{n}"), move |i| {
                i.push_int(n as i64);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(
        interp.register_operator("onemore", |_| Ok(())),
        Err(RegisterError::TooMany)
    );
    // Replacing an existing one is still allowed at the limit.
    interp.register_operator("hostop7", |_| Ok(())).unwrap();
    // Each slot reaches its own closure.
    interp.run_str("hostop0 hostop100 hostop255").unwrap();
    assert_eq!(interp.pop_int().unwrap(), 255);
    assert_eq!(interp.pop_int().unwrap(), 100);
    assert_eq!(interp.pop_int().unwrap(), 0);
}

#[test]
fn two_interpreters_keep_their_own_operators() {
    let mut a = Interp::new();
    let mut b = Interp::new();
    a.register_operator("which", |i| {
        i.push_name("a");
        Ok(())
    })
    .unwrap();
    b.register_operator("which", |i| {
        i.push_name("b");
        Ok(())
    })
    .unwrap();
    a.run_str("which").unwrap();
    b.run_str("which").unwrap();
    assert_eq!(a.pop_name().unwrap(), "a");
    assert_eq!(b.pop_name().unwrap(), "b");
    let mut c = Interp::new();
    assert_eq!(
        c.run_str("which").unwrap_err(),
        PsError::Undefined("which".into())
    );
}

#[test]
fn values_cross_the_stack_both_ways() {
    let mut interp = Interp::new();
    interp
        .run_str("<< /n 3 /r 1.5 /b true /s (hi) /name /Bold /a [1 [2 3] null] /p { 1 add } >>")
        .unwrap();
    let v = interp.pop_value().unwrap();
    assert_eq!(v.get("n").and_then(Value::as_i64), Some(3));
    assert_eq!(v.get("n").and_then(Value::as_f64), Some(3.0));
    assert_eq!(v.get("r").and_then(Value::as_f64), Some(1.5));
    assert_eq!(v.get("b").and_then(Value::as_bool), Some(true));
    assert_eq!(v.get("s").and_then(Value::as_text), Some("hi"));
    assert!(matches!(v.get("name"), Some(Value::Name(n)) if n == "Bold"));
    let a = v.get("a").and_then(Value::as_array).unwrap();
    assert_eq!(a[1].as_numbers(), Some(vec![2.0, 3.0]));
    assert!(matches!(a[2], Value::Null));
    assert!(a[0].as_numbers().is_none());
    let Some(Value::Other(proc)) = v.get("p") else {
        panic!("a procedure has no plain-data form");
    };
    assert!(proc.is_procedure());
    assert_eq!(proc.type_name(), "arraytype");

    // Push the whole thing back and let the program use it, procedure included.
    interp.push_value(&v).unwrap();
    interp
        .run_str("dup /a get 1 get 1 get exch /p get exec")
        .unwrap();
    assert_eq!(interp.pop_int().unwrap(), 4);
    assert_eq!(interp.operand_count(), 0);
}

#[test]
fn typed_pops_check_types_without_consuming() {
    let mut interp = Interp::new();
    interp.push_int(5);
    assert_eq!(interp.pop_bool().unwrap_err(), PsError::Typecheck);
    assert_eq!(interp.pop_string().unwrap_err(), PsError::Typecheck);
    assert_eq!(interp.pop_name().unwrap_err(), PsError::Typecheck);
    assert_eq!(interp.pop_numbers().unwrap_err(), PsError::Typecheck);
    assert_eq!(interp.operand_count(), 1);
    interp.clear_operands();
    assert_eq!(interp.pop_bool().unwrap_err(), PsError::StackUnderflow);
    interp.run_str("[1 (two)]").unwrap();
    assert_eq!(interp.pop_numbers().unwrap_err(), PsError::Typecheck);
    assert_eq!(interp.operand_count(), 1);
    interp.clear_operands();
    interp.push_numbers(&[0.25, 4.0]);
    interp.push_string(b"bytes\xff");
    interp.push_name("round");
    interp.push_bool(false);
    assert!(!interp.pop_bool().unwrap());
    assert_eq!(interp.pop_name().unwrap(), "round");
    assert_eq!(interp.pop_string().unwrap(), b"bytes\xff");
    assert_eq!(interp.pop_numbers().unwrap(), vec![0.25, 4.0]);
}

#[test]
fn a_self_containing_array_stops_at_the_depth_limit() {
    let mut interp = Interp::new();
    interp.run_str("/a 1 array def a 0 a put a").unwrap();
    let mut v = interp.pop_value().unwrap();
    let mut depth = 0;
    while let Value::Array(items) = v {
        v = items.into_iter().next().unwrap();
        depth += 1;
    }
    assert_eq!(depth, pscat::host::MAX_VALUE_DEPTH);
    assert!(matches!(v, Value::Other(_)));
}

#[test]
fn lookup_and_define_value() {
    let mut interp = Interp::new();
    interp.define_value("gain", &Value::Real(0.75)).unwrap();
    interp
        .define_value(
            "config",
            &Value::Dict(vec![("Mode".into(), Value::Name("fast".into()))]),
        )
        .unwrap();
    interp
        .run_str("/out gain 2 mul def /mode config /Mode get def")
        .unwrap();
    assert_eq!(interp.lookup("out").and_then(|v| v.as_f64()), Some(1.5));
    assert!(matches!(interp.lookup("mode"), Some(Value::Name(n)) if n == "fast"));
    assert!(interp.lookup("add").is_some()); // an operator: Other
    assert!(interp.lookup("undefined-name").is_none());
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn current_path_is_in_user_space() {
    let mut interp = page(200, 200);
    interp
        .run_str("50 20 translate 30 rotate 2 3 scale 1 2 moveto 10 2 lineto 10 8 12 14 4 9 curveto closepath")
        .unwrap();
    let path = interp.current_path().unwrap();
    assert_eq!(path.len(), 4);
    let PathSegment::MoveTo(x, y) = path[0] else {
        panic!("{path:?}")
    };
    assert!(close(x, 1.0) && close(y, 2.0), "{path:?}");
    let PathSegment::CurveTo(x1, y1, x2, y2, x3, y3) = path[2] else {
        panic!("{path:?}")
    };
    for (got, want) in [
        (x1, 10.0),
        (y1, 8.0),
        (x2, 12.0),
        (y2, 14.0),
        (x3, 4.0),
        (y3, 9.0),
    ] {
        assert!(close(got, want), "{path:?}");
    }
    assert_eq!(path[3], PathSegment::Close);
    // No path: empty, even under a singular matrix.
    interp.run_str("newpath 0 0 scale").unwrap();
    assert!(interp.current_path().unwrap().is_empty());
}

#[test]
fn singular_ctm_with_a_path_is_undefinedresult() {
    let mut interp = Interp::new();
    interp.run_str("0 0 moveto 5 5 lineto 0 0 scale").unwrap();
    assert_eq!(interp.current_path().unwrap_err(), PsError::UndefinedResult);
}

#[test]
fn set_current_path_round_trips_and_paints() {
    let mut interp = page(40, 40);
    interp
        .run_str("5 5 translate 10 10 moveto 30 10 lineto 30 30 lineto closepath")
        .unwrap();
    let path = interp.current_path().unwrap();
    interp.run_str("newpath").unwrap();
    interp.set_current_path(&path).unwrap();
    assert_eq!(interp.current_path().unwrap().len(), path.len());
    interp.run_str("pathbbox").unwrap();
    let (ury, urx, lly, llx) = (
        interp.pop_f64().unwrap(),
        interp.pop_f64().unwrap(),
        interp.pop_f64().unwrap(),
        interp.pop_f64().unwrap(),
    );
    assert!(close(llx, 10.0) && close(lly, 10.0) && close(urx, 30.0) && close(ury, 30.0));
    // A subpath that does not start with a moveto is refused and leaves no path.
    let err = interp
        .set_current_path(&[PathSegment::LineTo(1.0, 1.0)])
        .unwrap_err();
    assert_eq!(err, PsError::NoCurrentPoint);
    assert!(interp.current_path().unwrap().is_empty());
}

/// A host operator that rewrites the current path: the pattern an
/// animation engine uses to draw a path on over time.
#[test]
fn host_operator_can_rewrite_the_path() {
    let mut interp = page(100, 10);
    interp
        .register_operator("firsthalf", |i| {
            let path = i.current_path()?;
            let (PathSegment::MoveTo(x0, y0), PathSegment::LineTo(x1, y1)) = (path[0], path[1])
            else {
                return Err(PsError::Rangecheck);
            };
            i.set_current_path(&[
                PathSegment::MoveTo(x0, y0),
                PathSegment::LineTo((x0 + x1) / 2.0, (y0 + y1) / 2.0),
            ])
        })
        .unwrap();
    interp
        .run_str("0 5 moveto 100 5 lineto firsthalf 10 setlinewidth stroke")
        .unwrap();
    let px = interp.canvas_premultiplied_rgba8();
    let at = |x: usize| px[(5 * 100 + x) * 4];
    assert_eq!(at(25), 0, "the first half is stroked");
    assert_eq!(at(75), 255, "the second half is not");
}

#[test]
fn swap_canvas_paints_a_layer_under_the_same_graphics_state() {
    let mut interp = page(20, 20);
    // A clip and a transform set before the swap still apply to the layer.
    interp
        .run_str("1 0 0 setrgbcolor 0 0 20 20 rectfill 0 0 10 20 rectclip 0 0 1 setrgbcolor")
        .unwrap();
    let background = interp
        .swap_canvas(Page::transparent(20, 20).unwrap())
        .unwrap();
    assert_eq!(&background.premultiplied_rgba8()[..4], &[255, 0, 0, 255]);
    interp.run_str("0 0 20 20 rectfill").unwrap();
    let layer = interp.swap_canvas(background).unwrap();
    let px = layer.premultiplied_rgba8();
    assert_eq!(&px[..4], &[0, 0, 255, 255], "inside the clip");
    assert_eq!(&px[15 * 4..15 * 4 + 4], &[0, 0, 0, 0], "outside the clip");
    // The original canvas is back, untouched by the layer's drawing.
    assert!(
        interp
            .canvas_premultiplied_rgba8()
            .chunks(4)
            .all(|p| p == [255, 0, 0, 255])
    );
}

#[test]
fn swap_canvas_refuses_another_size_and_keeps_scale() {
    let mut interp = Interp::with_page_scaled(10, 10, 2.0).unwrap();
    assert_eq!(interp.canvas_size(), (20, 20));
    let wrong = Page::transparent(10, 10).unwrap();
    let back = interp.swap_canvas(wrong).unwrap_err();
    assert_eq!((back.width(), back.height()), (10, 10));
    let mut data = vec![0u8; 20 * 20 * 4];
    data[..4].copy_from_slice(&[9, 9, 9, 255]);
    let prepared = Page::from_premultiplied_rgba8(20, 20, data).unwrap();
    interp.swap_canvas(prepared).unwrap();
    assert_eq!(&interp.canvas_premultiplied_rgba8()[..4], &[9, 9, 9, 255]);
    // Alpha fills composite on what was swapped in, not on a stale 16-bit copy.
    interp
        .run_str("1 setgray 0.5 setalpha 0 0 10 10 rectfill")
        .unwrap();
    let px = &interp.canvas_premultiplied_rgba8()[..4];
    assert!((130..=134).contains(&px[0]), "{px:?}");
}

#[test]
fn page_pixels_can_be_edited_in_place() {
    let mut page = Page::transparent(2, 1).unwrap();
    page.premultiplied_rgba8_mut()[..4].copy_from_slice(&[0, 128, 0, 128]);
    assert_eq!(page.rgba8()[..4], [0, 255, 0, 128]);
    assert!(Page::from_premultiplied_rgba8(0, 1, vec![]).is_none());
    assert!(Page::from_premultiplied_rgba8(9000, 1, vec![0; 36000]).is_none());
    assert!(Page::transparent(1, 9000).is_none());
}

#[test]
fn interrupt_stops_a_running_program_from_another_thread() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut interp = Interp::new();
    interp.set_interrupt(Some(stop.clone()));
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            stop.store(true, Ordering::Relaxed);
        })
    };
    // `stopped` cannot catch it.
    let err = interp.run_str("{ { } loop } stopped").unwrap_err();
    raiser.join().unwrap();
    assert_eq!(err, PsError::Interrupted);
    assert_eq!(err.name(), "interrupt");
    assert_eq!(interp.error_details(&err).name, "interrupt");
    // Lowering the flag makes the interpreter usable again.
    stop.store(false, Ordering::Relaxed);
    interp.run_str("3 4 add").unwrap();
    assert_eq!(interp.pop_int().unwrap(), 7);
    // Without a flag nothing changes.
    interp.set_interrupt(None);
    interp.run_str("1000 { } repeat").unwrap();
}

#[test]
fn error_details_fields() {
    let mut interp = Interp::new();
    let err = interp.run_str("\n\n  nosuchthing").unwrap_err();
    let d = interp.error_details(&err);
    assert_eq!(d.name, "undefined");
    assert_eq!(d.command.as_deref(), Some("nosuchthing"));
    assert_eq!((d.line, d.file.as_deref()), (Some(3), None));
    assert_eq!(d.limit, None);

    let err = interp.run_str("(unterminated").unwrap_err();
    let d = interp.error_details(&err);
    assert_eq!(d.name, "syntaxerror");
    assert!(d.detail.is_some());

    interp
        .set_limits(Limits::none().with_max_steps(500))
        .unwrap();
    let err = interp.run_str("{ } loop").unwrap_err();
    let d = interp.error_details(&err);
    assert_eq!(
        (d.name, d.limit),
        ("limitexceeded", Some(pscat::Limit::Steps))
    );
}

#[test]
fn backtrace_names_procedures_and_marks_the_element() {
    let mut interp = Interp::new();
    interp
        .run_str(
            "/inner { 1 2 add pop (text) 5 mul pop } def
             /middle { 3 { inner } repeat } def
             /outer { gsave middle grestore } def",
        )
        .unwrap();
    let err = interp.run_str("outer").unwrap_err();
    let d = interp.error_details(&err);
    assert_eq!(d.command.as_deref(), Some("mul"));
    let t = &d.backtrace;
    let names: Vec<Option<&str>> = t.iter().map(|f| f.procedure.as_deref()).collect();
    // Innermost first. `middle` and its `repeat` body are not there: each
    // had issued its last element, a call, and a procedure's frame is
    // dropped at that point (tail calls take no execution stack).
    assert_eq!(names, [Some("inner"), Some("outer")]);
    assert_eq!(t[0].element, 6);
    assert_eq!(t[0].excerpt, "... 2 add pop (text) 5 >>> mul <<< pop");
    assert_eq!(t[1].excerpt, "gsave >>> middle <<< grestore");

    // A loop body that goes on after the call is a frame with no name.
    interp.clear_operands();
    interp
        .run_str("/looper { 2 { inner 0 pop } repeat 0 pop } def")
        .unwrap();
    let err = interp.run_str("looper").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    let names: Vec<Option<&str>> = t.iter().map(|f| f.procedure.as_deref()).collect();
    assert_eq!(names, [Some("inner"), None, Some("looper")]);
    assert_eq!(t[1].excerpt, ">>> inner <<< 0 pop");
    assert_eq!(t[2].excerpt, "2 {inner 0 pop} >>> repeat <<< 0 pop");
    interp.clear_operands();

    // An error at the top level of the text has no procedure frames...
    let err = interp.run_str("1 (x) add").unwrap_err();
    assert!(interp.error_details(&err).backtrace.is_empty());
    // ...and a successful run leaves none behind for a later, unrelated report.
    let err = interp.run_str("outer").unwrap_err();
    assert_eq!(interp.error_details(&err).backtrace.len(), 2);
    interp.clear_operands();
    interp.run_str("1 1 add pop").unwrap();
    assert!(
        interp
            .error_details(&PsError::Typecheck)
            .backtrace
            .is_empty()
    );
}

#[test]
fn backtrace_is_bounded_and_survives_limits_and_caught_errors() {
    let mut interp = Interp::new();
    // Deep recursion: only the innermost frames are kept.
    interp
        .run_str("/down { dup 0 gt { 1 sub down 0 pop } { (x) 1 add pop } ifelse 0 pop } def")
        .unwrap();
    let err = interp.run_str("40 down").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t.len(), pscat::host::MAX_TRACE_FRAMES);
    assert!(t[0].excerpt.contains(">>> add <<<"), "{}", t[0].excerpt);
    assert_eq!(t[1].procedure.as_deref(), Some("down"));
    interp.clear_operands();

    // An error a program catches leaves no trace: nothing was uncaught.
    interp.run_str("{ (x) 1 add } stopped pop").unwrap();
    assert!(
        interp
            .error_details(&PsError::Typecheck)
            .backtrace
            .is_empty()
    );
    interp.clear_operands();

    // A limit error has one too: where the program was when the budget ran out.
    interp
        .run_str("/spin { { 1 pop } loop 0 pop } def")
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_steps(1_000))
        .unwrap();
    let err = interp.run_str("spin").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert!(
        t.iter().any(|f| f.procedure.as_deref() == Some("spin")),
        "{t:?}"
    );

    // Long values in an excerpt are shortened, so a frame stays one line.
    interp.set_limits(Limits::none()).unwrap();
    interp
        .run_str(
            "/big { (0123456789012345678901234567890123456789) [1 2 3 4 5 6 7 8 9] nosuch } def",
        )
        .unwrap();
    let err = interp.run_str("big").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert!(t[0].excerpt.len() < 140, "{}", t[0].excerpt);
    assert!(t[0].excerpt.ends_with(">>> nosuch <<<"));
}

/// The commonest place for an error is the last operator of a procedure,
/// whose frame is already popped when it runs. The trace still shows it.
#[test]
fn backtrace_includes_a_procedure_failing_at_its_last_element() {
    let mut interp = Interp::new();
    interp
        .run_str(
            "/label { 10 20 moveto show } def
             /page  { gsave 42 label grestore } def
             /fine  { 1 2 add pop } def",
        )
        .unwrap();
    let err = interp.run_str("page").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t.len(), 2, "{t:?}");
    assert_eq!(t[0].procedure.as_deref(), Some("label"));
    assert_eq!(t[0].excerpt, "10 20 moveto >>> show <<<");
    assert_eq!(t[1].excerpt, "gsave 42 >>> label <<< grestore");
    interp.clear_operands();

    // Called straight from program text.
    let err = interp.run_str("7 label").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].procedure.as_deref(), Some("label"));
    interp.clear_operands();

    // A procedure that returned is not blamed for a later error at the
    // same depth: in program text...
    let err = interp.run_str("fine (a) (b) add").unwrap_err();
    assert!(interp.error_details(&err).backtrace.is_empty());
    interp.clear_operands();
    // ...in its caller...
    interp
        .run_str("/caller { fine (a) (b) add pop } def")
        .unwrap();
    let err = interp.run_str("caller").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].excerpt, "fine (a) (b) >>> add <<< pop");
    interp.clear_operands();
    // ...or in a later iteration of a loop that called it.
    interp
        .run_str("/n 0 def /loopy { 3 { /n n 1 add def n 2 eq { (a) 1 add } { fine } ifelse } repeat } def")
        .unwrap();
    let err = interp.run_str("loopy").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert!(
        t.iter().all(|f| f.procedure.as_deref() != Some("fine")),
        "{t:?}"
    );
    assert!(t[0].excerpt.contains(">>> add <<<"), "{t:?}");
    interp.clear_operands();

    // A loop body failing at its last element, under a `for`.
    interp
        .run_str("/sweep { 0 1 3 { (s) add } for 0 pop } def")
        .unwrap();
    let err = interp.run_str("sweep").unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t[0].excerpt, "(s) >>> add <<<");
    assert_eq!(t[1].procedure.as_deref(), Some("sweep"));
}

/// The whole pattern together, as an animation engine uses it: set a
/// program up once, checkpoint the VM with `save`, then render any frame in
/// any order as a pure function of a host clock.
#[test]
fn frames_on_one_interpreter_are_time_addressable() {
    let clock = Rc::new(Cell::new(0.0_f64));
    let mut interp = page(64, 64);
    let c = clock.clone();
    interp
        .register_operator("now", move |i| {
            i.push_real(c.get());
            Ok(())
        })
        .unwrap();
    interp
        .run_str(
            "/count 0 def
             /scene { /count count 1 add def
                      32 32 translate now 90 mul rotate now 0.5 0.2 setrgbcolor
                      -20 -6 40 12 rectfill } bind def
             /checkpoint save def",
        )
        .unwrap();
    let mut frame = |t: f64| -> Vec<u8> {
        clock.set(t);
        interp
            .run_str("checkpoint restore /checkpoint save def erasepage initgraphics scene")
            .unwrap();
        interp.canvas_premultiplied_rgba8().to_vec()
    };
    let (a, b) = (frame(0.0), frame(0.5));
    assert_ne!(a, b);
    // Out of order, and after other frames ran: the same pixels.
    assert_eq!(frame(0.0), a);
    assert_eq!(frame(0.5), b);
    // `restore` rolled the scene's own mutation back each time.
    assert_eq!(interp.lookup("count").and_then(|v| v.as_i64()), Some(1));
}

// ---- PR #1410 review, round 1 -------------------------------------------

/// Finding 5: `Interrupted` from a host operator is as uncatchable as the
/// flag itself.
#[test]
fn interrupted_from_a_host_operator_is_uncatchable() {
    let mut interp = Interp::new();
    interp
        .register_operator("cancel", |_| Err(PsError::Interrupted))
        .unwrap();
    let err = interp.run_str("{ cancel } stopped pop 42").unwrap_err();
    assert_eq!(err, PsError::Interrupted);
    assert_eq!(interp.error_details(&err).name, "interrupt");
    assert_eq!(interp.operand_count(), 0, "42 was never pushed");
    // Under a step cap, the interrupt is reported as itself.
    interp
        .set_limits(Limits::none().with_max_steps(1_000_000))
        .unwrap();
    assert_eq!(
        interp.run_str("{ cancel } stopped").unwrap_err(),
        PsError::Interrupted
    );
}

/// Finding 4: a raised flag stops a run that is short enough to finish in
/// one slice, before anything executes; and a flag raised during the last
/// slice is reported, not lost.
#[test]
fn a_raised_flag_stops_even_a_short_run() {
    let stop = Arc::new(AtomicBool::new(true));
    let mut interp = Interp::new();
    interp.set_interrupt(Some(stop.clone()));
    assert_eq!(interp.run_str("1 2 add").unwrap_err(), PsError::Interrupted);
    assert_eq!(interp.operand_count(), 0, "nothing ran");
    stop.store(false, Ordering::Relaxed);
    let s = stop.clone();
    interp
        .register_operator("raise", move |_| {
            s.store(true, Ordering::Relaxed);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        interp.run_str("raise 1 2 add").unwrap_err(),
        PsError::Interrupted
    );
    stop.store(false, Ordering::Relaxed);
    interp.clear_operands();
    interp.run_str("1 2 add").unwrap();
    assert_eq!(interp.pop_int().unwrap(), 3);
}

/// Finding 6: `clear_operands` in a failing operator is rolled back with
/// the rest of its pops.
#[test]
fn clear_operands_in_a_failing_operator_is_rolled_back() {
    let mut interp = Interp::new();
    interp
        .register_operator("wipe", |i| {
            i.push_int(99);
            i.clear_operands();
            Err(PsError::Typecheck)
        })
        .unwrap();
    let err = interp.run_str("(keep) 1 2 wipe").unwrap_err();
    assert_eq!(err, PsError::Typecheck);
    assert_eq!(interp.operand_count(), 3);
    assert_eq!(interp.pop_int().unwrap(), 2);
    assert_eq!(interp.pop_int().unwrap(), 1);
    assert_eq!(interp.pop_string().unwrap(), b"keep");
}

/// Finding 6, and what an animation engine relies on: `clear_operands`
/// still empties the stack, inside an operator that succeeds and from the
/// host between frames, and each frame of a save/restore loop starts with
/// an empty stack and a rolled-back VM.
#[test]
fn clear_operands_still_empties_the_stack_between_frames() {
    let mut interp = Interp::new();
    interp
        .register_operator("reset", |i| {
            i.clear_operands();
            i.push_int(7);
            Ok(())
        })
        .unwrap();
    interp.run_str("1 2 3 reset").unwrap();
    assert_eq!(interp.operand_count(), 1);
    assert_eq!(interp.pop_int().unwrap(), 7);

    interp.run_str("/n 0 def /checkpoint save def").unwrap();
    for frame in 0..4 {
        assert_eq!(interp.operand_count(), 0, "frame {frame} starts empty");
        interp
            .run_str("checkpoint restore /checkpoint save def /n n 1 add def n 10 20 30")
            .unwrap();
        assert_eq!(interp.operand_count(), 4);
        // The VM was rolled back: `n` is 1 in every frame.
        assert_eq!(interp.lookup("n").and_then(|v| v.as_i64()), Some(1));
        if frame % 2 == 0 {
            interp.clear_operands();
        } else {
            interp.run_str("reset pop").unwrap();
        }
    }
}

/// Finding 8: a procedure that returned is not blamed for an error at the
/// top level of the text that happens to be its last operator.
#[test]
fn backtrace_does_not_blame_a_returned_procedure_with_the_same_last_operator() {
    let mut interp = Interp::new();
    interp.run_str("/fine { 1 pop } def").unwrap();
    let err = interp.run_str("fine pop").unwrap_err();
    assert_eq!(err, PsError::StackUnderflow);
    let t = interp.error_details(&err).backtrace;
    assert!(t.is_empty(), "{t:?}");
    // The procedure itself failing there is still in the trace.
    let err = interp
        .run_str("/bad { 1 0 pop pop pop } def bad")
        .unwrap_err();
    let t = interp.error_details(&err).backtrace;
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].procedure.as_deref(), Some("bad"));
}

/// Finding 2: a backtrace's names come from one bounded look at the
/// dictionary stack, so an uncaught error deep in a program with a large
/// dictionary costs no more than one with a small one.
#[test]
fn backtrace_cost_does_not_grow_with_dictionary_size() {
    use common::thread_cpu_time;
    // `d`'s branches are procedures with no name: the search for one
    // looks at every entry the dictionary stack has.
    const DOWN: &str = "/d { dup 0 gt { 1 sub d 0 pop } { (x) 1 add } ifelse 0 pop } def";
    fn timed(entries: usize) -> (std::time::Duration, std::time::Duration) {
        let mut interp = Interp::new();
        interp.run_str(DOWN).unwrap();
        let build = format!(
            "0 1 {} {{ dup 12 string cvs cvn exch def }} for",
            entries - 1
        );
        let t0 = thread_cpu_time();
        interp.run_str(&build).unwrap();
        let t_build = thread_cpu_time() - t0;
        let t0 = thread_cpu_time();
        for _ in 0..4 {
            let err = interp.run_str("20 d").unwrap_err();
            assert_eq!(err, PsError::Typecheck);
            assert!(!interp.error_details(&err).backtrace.is_empty());
            interp.clear_operands();
        }
        (t_build, thread_cpu_time() - t0)
    }
    let (t_build, t_errors) = (0..3)
        .map(|_| timed(200_000))
        .min_by_key(|(_, e)| *e)
        .unwrap();
    // Unbounded, the four errors copy the 200,000 entries sixteen times
    // each: several times the cost of defining them. Bounded, they are a
    // small fraction of it.
    assert!(
        t_errors < common::scale_budget(t_build / 4),
        "four errors took {t_errors:?}, defining the entries {t_build:?}"
    );
}

/// Finding 3: a long-running host operator charges its work with
/// `check_limits` and sees the step cap, the clock and the flag as it goes.
#[test]
fn a_long_host_operator_charges_its_work_and_sees_the_flag() {
    let mut interp = Interp::new();
    interp
        .register_operator("spin", |i| {
            let n = i.pop_int()?;
            for _ in 0..n {
                i.check_limits(1)?;
            }
            Ok(())
        })
        .unwrap();
    // No limits and no flag: it runs to the end.
    interp.run_str("100000 spin").unwrap();

    // The step cap: refused, uncatchable, and the operand given back.
    interp
        .set_limits(Limits::none().with_max_steps(10_000))
        .unwrap();
    let err = interp
        .run_str("{ 1000000000000 spin } stopped")
        .unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    assert_eq!(interp.error_details(&err).command.as_deref(), Some("spin"));
    assert_eq!(interp.pop_int().unwrap(), 1_000_000_000_000);
    // A short one fits, and the steps it charged are counted.
    interp.run_str("5000 spin").unwrap();
    assert!(interp.steps_used() >= 5000, "{}", interp.steps_used());

    // The clock.
    interp
        .set_limits(Limits::none().with_max_time(std::time::Duration::from_millis(50)))
        .unwrap();
    let err = interp.run_str("1000000000000 spin").unwrap_err();
    assert_eq!(err, PsError::TimeExceeded);
    interp.clear_operands();

    // The flag alone, raised from another thread while the operator runs.
    interp.set_limits(Limits::none()).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    interp.set_interrupt(Some(stop.clone()));
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            stop.store(true, Ordering::Relaxed);
        })
    };
    let err = interp
        .run_str("{ 1000000000000 spin } stopped")
        .unwrap_err();
    raiser.join().unwrap();
    assert_eq!(err, PsError::Interrupted);
    interp.clear_operands();

    // From the host between runs: only the flag is looked at.
    assert_eq!(interp.check_limits(0).unwrap_err(), PsError::Interrupted);
    stop.store(false, Ordering::Relaxed);
    interp
        .set_limits(Limits::none().with_max_steps(10))
        .unwrap();
    interp.check_limits(1_000_000).unwrap();
}

/// An operator that converts its operand to a `Value` and drops it.
fn with_extract(interp: &mut Interp) {
    interp
        .register_operator("extract", |i| {
            i.pop_value()?;
            Ok(())
        })
        .unwrap();
}

/// Finding 1: an array that holds itself twice is 2^32 nodes deep to the
/// depth limit. The copy is bounded by `MAX_VALUE_NODES`, not expanded.
#[test]
fn a_doubly_self_containing_array_is_refused_not_expanded() {
    let mut interp = Interp::new();
    with_extract(&mut interp);
    interp
        .run_str("/a 2 array def a 0 a put a 1 a put")
        .unwrap();
    // No limits: the node cap refuses it, and the failed operator's operand
    // comes back.
    let err = interp.run_str("a extract").unwrap_err();
    assert_eq!(err, PsError::Limitcheck);
    assert_eq!(
        interp.error_details(&err).command.as_deref(),
        Some("extract")
    );
    assert_eq!(interp.operand_count(), 1);
    // From the host, outside a run: the same refusal, and the operand stays.
    assert_eq!(interp.pop_value().unwrap_err(), PsError::Limitcheck);
    assert_eq!(interp.operand_count(), 1);
    interp.clear_operands();
    // `lookup` has no error to give: the value comes back whole, as opaque.
    assert!(matches!(interp.lookup("a"), Some(Value::Other(_))));
    // A wide but finite value is still copied in full.
    interp
        .run_str("/w 100000 array def 0 1 99999 { w exch dup put } for w")
        .unwrap();
    let w = interp.pop_value().unwrap();
    assert_eq!(w.as_array().map(<[Value]>::len), Some(100_000));
}

/// Finding 1: the copy's work counts against the step cap and looks at the
/// host's interrupt flag, like any other long operator.
#[test]
fn value_conversion_is_charged_to_the_limits() {
    let mut interp = Interp::new();
    with_extract(&mut interp);
    interp
        .run_str("/a 2 array def a 0 a put a 1 a put")
        .unwrap();
    interp
        .set_limits(Limits::none().with_max_steps(200))
        .unwrap();
    let err = interp.run_str("a extract").unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    // Uncatchable, like any limit.
    let err = interp.run_str("{ a extract } stopped pop 42").unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    interp.clear_operands();
    interp.set_limits(Limits::none()).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    interp.set_interrupt(Some(stop.clone()));
    let s = stop.clone();
    interp
        .register_operator("raise", move |_| {
            s.store(true, Ordering::Relaxed);
            Ok(())
        })
        .unwrap();
    let err = interp.run_str("a raise extract").unwrap_err();
    assert_eq!(err, PsError::Interrupted);
}

/// Finding 7: an opaque value belongs to the interpreter it came from. A
/// host operator (or a procedure `bind` filled with them, or a save
/// object) pushed into another interpreter would run that one's operator
/// slots under this one's names; it is refused, wherever it sits in the
/// value, and nothing is pushed or defined.
#[test]
fn an_opaque_value_is_refused_by_another_interpreter() {
    let mut a = Interp::new();
    let mut b = Interp::new();
    a.register_operator("foo", |i| {
        i.push_name("from_a_foo");
        Ok(())
    })
    .unwrap();
    b.register_operator("bar", |i| {
        i.push_name("from_b_bar");
        Ok(())
    })
    .unwrap();
    a.run_str("/p { foo } bind def /s save def").unwrap();
    let foo = a.lookup("foo").unwrap();
    let p = a.lookup("p").unwrap();
    let s = a.lookup("s").unwrap();
    for v in [&foo, &p, &s] {
        assert!(matches!(v, Value::Other(_)), "{v:?}");
        assert_eq!(b.push_value(v).unwrap_err(), PsError::InvalidAccess);
        assert_eq!(b.define_value("x", v).unwrap_err(), PsError::InvalidAccess);
    }
    let nested = Value::Array(vec![
        Value::Int(1),
        Value::Dict(vec![("f".into(), foo.clone())]),
    ]);
    assert_eq!(b.push_value(&nested).unwrap_err(), PsError::InvalidAccess);
    assert_eq!(b.operand_count(), 0);
    assert!(b.lookup("x").is_none());
    // Plain data crosses freely, and the interpreter it came from takes it back.
    b.push_value(&Value::Array(vec![Value::Int(1), "two".into()]))
        .unwrap();
    for v in [&foo, &p] {
        a.push_value(v).unwrap();
        a.run_str("exec").unwrap();
        assert_eq!(a.pop_name().unwrap(), "from_a_foo");
    }
    // An opaque value returned by a host operator into its own interpreter.
    a.register_operator("echo", |i| {
        let v = i.pop_value()?;
        i.push_value(&v)
    })
    .unwrap();
    a.run_str("/foo load echo exec").unwrap();
    assert_eq!(a.pop_name().unwrap(), "from_a_foo");
}

/// What an animation engine does: one interpreter per render thread, each
/// with its own host operators and its own interrupt flag. Raising one
/// thread's flag stops that thread's program only.
#[test]
fn interpreters_on_separate_threads_keep_their_operators_and_flags() {
    let flags: Vec<Arc<AtomicBool>> = (0..4).map(|_| Arc::new(AtomicBool::new(false))).collect();
    let (ready_tx, ready) = std::sync::mpsc::channel::<()>();
    let workers: Vec<_> = flags
        .iter()
        .enumerate()
        .map(|(k, flag)| {
            let flag = flag.clone();
            let ready_tx = ready_tx.clone();
            std::thread::spawn(move || {
                let mut interp = Interp::new();
                interp.set_interrupt(Some(flag));
                let id = k as i64;
                interp
                    .register_operator("worker", move |i| {
                        i.push_int(id);
                        Ok(())
                    })
                    .unwrap();
                interp
                    .register_operator("spin", |i| {
                        loop {
                            i.check_limits(1)?;
                        }
                    })
                    .unwrap();
                // Values round-trip within the thread's own interpreter.
                interp.run_str("/w /worker load def").unwrap();
                let w = interp.lookup("w").unwrap();
                interp.push_value(&w).unwrap();
                interp.run_str("exec worker add").unwrap();
                let sum = interp.pop_int().unwrap();
                // Thread 0 spins until the main thread raises its flag;
                // the others finish short runs.
                let result = if k == 0 {
                    // Whether the flag is seen before the run or during
                    // `spin`, the run is interrupted.
                    ready_tx.send(()).unwrap();
                    interp.run_str("{ spin } stopped")
                } else {
                    interp.run_str("1 2 add pop")
                };
                (sum, result)
            })
        })
        .collect();
    ready.recv().unwrap();
    flags[0].store(true, Ordering::Relaxed);
    for (k, w) in workers.into_iter().enumerate() {
        let (sum, result) = w.join().unwrap();
        assert_eq!(sum, 2 * k as i64, "thread {k} ran its own operator");
        if k == 0 {
            assert_eq!(result.unwrap_err(), PsError::Interrupted);
        } else {
            result.unwrap();
        }
    }
}
