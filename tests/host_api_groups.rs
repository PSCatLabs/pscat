//! The host API (#1403) with transparency groups (#1406): while a group is
//! open the raster a program paints on is the group's layer, but the page a
//! host reads, exchanges or resets is the canvas below it. A stop (the
//! interrupt flag, a limit) closes the group, and the interpreter is whole
//! for the next run.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pscat::host::Value;
use pscat::{Interp, Limits, Page, PsError};

const SIZE: u32 = 20;
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

/// A red page, then a group whose left half is painted blue, then `after`.
fn red_then_blue_group(after: &str) -> String {
    format!(
        "1 0 0 setrgbcolor 0 0 20 20 rectfill
         << >> begingroup 0 0 1 setrgbcolor 0 0 10 20 rectfill {after}"
    )
}

fn page() -> Interp {
    Interp::with_page(SIZE, SIZE).expect("valid page size")
}

/// The pixel at column `x`, row `y` of premultiplied RGBA bytes.
fn at(rgba: &[u8], x: usize, y: usize) -> [u8; 4] {
    let i = (y * SIZE as usize + x) * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn left_right(rgba: &[u8]) -> ([u8; 4], [u8; 4]) {
    (at(rgba, 2, 10), at(rgba, 17, 10))
}

/// A program paused in a group's loop, as a host driving it slice by slice
/// finds it.
fn paused_in_a_group(interp: &mut Interp) {
    interp.begin_source(
        red_then_blue_group("/go false def { go { exit } if } loop endgroup").as_bytes(),
    );
    assert!(
        interp.step_n(400).expect("slice"),
        "the loop is still running"
    );
}

#[test]
fn a_host_reading_between_slices_sees_the_page_not_the_layer() {
    let mut interp = page();
    paused_in_a_group(&mut interp);

    // The accessors of #1410 were already page readers: the canvas is the page.
    assert_eq!(interp.canvas_size(), (SIZE, SIZE));
    let canvas = interp.canvas_premultiplied_rgba8().to_vec();
    assert_eq!(left_right(&canvas), (RED, RED));

    // So are the page copies (they read the layer before this change, whose
    // right half is transparent and left half blue).
    let pages = interp.pages();
    assert_eq!(pages.len(), 1);
    assert_eq!(left_right(&pages[0].rgba8()), (RED, RED));
    assert_eq!(interp.page_count(), 1);
    let first = interp.page(0).expect("the page in progress");
    assert_eq!(left_right(&first.rgba8()), (RED, RED));

    // The program goes on and the group composites onto the same page.
    interp.define_value("go", &Value::Bool(true)).unwrap();
    assert!(!interp.step_n(400).expect("slice"));
    let done = interp.canvas_premultiplied_rgba8().to_vec();
    assert_eq!(left_right(&done), (BLUE, RED));
    assert_eq!(left_right(&interp.pages()[0].rgba8()), (BLUE, RED));
}

#[test]
fn into_last_page_with_a_group_open_gives_the_page_below_it() {
    let mut interp = page();
    paused_in_a_group(&mut interp);
    let last = interp.into_last_page();
    assert_eq!(left_right(&last.rgba8()), (RED, RED));
}

#[test]
fn an_interrupt_with_a_group_open_drops_the_layer_and_leaves_a_whole_page() {
    let stop = Arc::new(AtomicBool::new(false));
    let mut interp = page();
    interp.set_interrupt(Some(stop.clone()));
    let flag = stop.clone();
    interp
        .register_operator("trip", move |_| {
            flag.store(true, Ordering::Relaxed);
            Ok(())
        })
        .unwrap();
    let err = interp
        .run_str(&red_then_blue_group("trip { } loop endgroup"))
        .unwrap_err();
    assert_eq!(err, PsError::Interrupted);

    // The group was open when the host stopped the program: like a time
    // limit, the stop drops its layer. The page is the one below it, whole.
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));
    assert_eq!(left_right(&interp.pages()[0].rgba8()), (RED, RED));

    // A later run on the same interpreter works, and groups work in it.
    stop.store(false, Ordering::Relaxed);
    interp
        .run_str(
            "0 1 0 setrgbcolor 0 0 10 20 rectfill
             << >> begingroup 0 0 1 setrgbcolor 10 0 10 20 rectfill endgroup",
        )
        .unwrap();
    assert_eq!(
        left_right(interp.canvas_premultiplied_rgba8()),
        (GREEN, BLUE)
    );
}

#[test]
fn an_interrupt_raised_by_a_host_operator_in_a_group_does_the_same() {
    let mut interp = page();
    interp
        .register_operator("bail", |i| {
            i.check_limits(0)?;
            Err(PsError::Interrupted)
        })
        .unwrap();
    let err = interp
        .run_str(&red_then_blue_group("{ bail } stopped pop endgroup"))
        .unwrap_err();
    assert_eq!(err, PsError::Interrupted);
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));
    interp
        .run_str("0 1 0 setrgbcolor 0 0 20 20 rectfill")
        .unwrap();
    assert_eq!(
        left_right(interp.canvas_premultiplied_rgba8()),
        (GREEN, GREEN)
    );
}

#[test]
fn a_step_limit_with_a_group_open_composites_what_was_painted() {
    let mut interp = page();
    interp
        .set_limits(Limits::none().with_max_steps(3_000))
        .unwrap();
    let err = interp
        .run_str(&red_then_blue_group("{ } loop endgroup"))
        .unwrap_err();
    assert_eq!(err, PsError::StepsExceeded);
    // The group closed with the program: its marks are on the page.
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (BLUE, RED));

    interp.set_limits(Limits::none()).unwrap();
    interp
        .run_str(
            "0 1 0 setrgbcolor 0 0 20 20 rectfill
             << >> begingroup 0 0 1 setrgbcolor 0 0 10 20 rectfill endgroup",
        )
        .unwrap();
    assert_eq!(
        left_right(interp.canvas_premultiplied_rgba8()),
        (BLUE, GREEN)
    );
}

#[test]
fn a_time_limit_with_a_group_open_drops_the_layer() {
    let mut interp = page();
    interp
        .set_limits(Limits::none().with_max_time(std::time::Duration::from_millis(50)))
        .unwrap();
    let err = interp
        .run_str(&red_then_blue_group("{ } loop endgroup"))
        .unwrap_err();
    assert_eq!(err, PsError::TimeExceeded);
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));

    interp.set_limits(Limits::none()).unwrap();
    interp
        .run_str("<< >> begingroup 0 0 1 setrgbcolor 0 0 10 20 rectfill endgroup")
        .unwrap();
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (BLUE, RED));
}

#[test]
fn a_canvas_swap_with_a_group_open_swaps_the_page_and_keeps_the_composite() {
    let mut interp = page();
    let taken: Rc<RefCell<Option<Page>>> = Rc::new(RefCell::new(None));
    let wrong_size_refused = Rc::new(Cell::new(false));
    let (t, w) = (taken.clone(), wrong_size_refused.clone());
    interp
        .register_operator("swap", move |i| {
            w.set(i.swap_canvas(Page::transparent(3, 3).unwrap()).is_err());
            let fresh = Page::transparent(SIZE, SIZE).unwrap();
            *t.borrow_mut() = Some(i.swap_canvas(fresh).expect("same size"));
            Ok(())
        })
        .unwrap();
    interp
        .run_str(&red_then_blue_group(
            "swap 0 1 0 setrgbcolor 10 0 10 10 rectfill endgroup",
        ))
        .unwrap();
    assert!(wrong_size_refused.get());

    // What came back is the page the group was drawn over, not its layer.
    let old = taken.borrow_mut().take().expect("swapped");
    assert_eq!(left_right(&old.rgba8()), (RED, RED));
    // The group composited onto the page the host handed over.
    let canvas = interp.canvas_premultiplied_rgba8();
    assert_eq!(at(canvas, 2, 10), BLUE);
    assert_eq!(at(canvas, 17, 5), CLEAR);
    assert_eq!(at(canvas, 17, 15), GREEN);
    // One page, no leftover layer: the interpreter paints on as usual.
    interp
        .run_str("1 0 0 setrgbcolor 0 0 20 20 rectfill")
        .unwrap();
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));
}

#[test]
fn a_canvas_swap_inside_a_pattern_is_refused_not_a_panic() {
    let mut interp = page();
    let refused = Rc::new(Cell::new(0u32));
    let r = refused.clone();
    interp
        .register_operator("swap", move |i| {
            if i.swap_canvas(Page::transparent(SIZE, SIZE).unwrap())
                .is_err()
            {
                r.set(r.get() + 1);
            }
            Ok(())
        })
        .unwrap();
    interp
        .run_str(
            "<< /PatternType 1 /PaintType 1 /TilingType 1 /BBox [0 0 4 4]
                /XStep 4 /YStep 4 /PaintProc { swap 1 0 0 setrgbcolor 0 0 4 4 rectfill } >>
             matrix makepattern setpattern 0 0 20 20 rectfill",
        )
        .unwrap();
    assert!(
        refused.get() > 0,
        "the PaintProc ran and its swap was refused"
    );
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));
}

#[test]
fn a_host_reset_with_a_group_open_clears_the_page_and_keeps_the_composite() {
    let mut interp = page();
    interp
        .register_operator("reset", |i| {
            i.set_transparent_background();
            Ok(())
        })
        .unwrap();
    interp
        .run_str(&red_then_blue_group("reset endgroup"))
        .unwrap();
    // The page below was reset to transparent; the group's marks, painted
    // before the reset, were not lost with it.
    assert_eq!(
        left_right(interp.canvas_premultiplied_rgba8()),
        (BLUE, CLEAR)
    );
    interp
        .run_str("1 0 0 setrgbcolor 0 0 20 20 rectfill")
        .unwrap();
    assert_eq!(left_right(interp.canvas_premultiplied_rgba8()), (RED, RED));
}
