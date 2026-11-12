//! `--gpu-preview` under the interpreter's limits (issue #1065): the GPU
//! runs under a time or step limit, and the limits are looked at before
//! each dispatch, so a runaway kernel still stops with the limit's error.
//! Only in a build with the `gpu` feature; a machine with no GPU adapter
//! falls back to the CPU, which raises the same errors.
#![cfg(all(feature = "gpu", not(target_arch = "wasm32")))]

use pscat::{Interp, Limits, PsError};
use std::time::Duration;

/// A loop whose result is the pixel (so no shader compiler can drop it)
/// and whose exit is never taken: every pixel runs the GPU's loop budget.
const RUNAWAY: &str = "/pixel { add 0.001 mul \
     { dup dup mul 0.2 add sqrt 0.999 mul add 0.5 mul dup 10 gt { exit } if } loop \
     0.1 mul } bind def";

fn runaway(side: u32) -> String {
    format!("{RUNAWAY} {side} {side} [{side} 0 0 -{side} 0 {side}] /pixel load 1 kernelimage")
}

#[test]
fn a_runaway_gpu_kernel_stops_at_the_time_limit() {
    pscat::set_gpu_preview(true).expect("gpu build");
    let mut it = Interp::with_page(2000, 2000).expect("page");
    it.set_limits(Limits::none().with_max_time(Duration::from_millis(300)))
        .expect("limits");
    // 61 dispatches of the loop budget; the limit is looked at between
    // them. No wall-clock bound here (AGENTS.md: timing assertions measure
    // CPU time, and the GPU's time is not this thread's); without the
    // checks the 61 dispatches would take tens of seconds, not a fraction.
    let err = it.run_str(&runaway(2000)).unwrap_err();
    assert_eq!(err, PsError::TimeExceeded);
}

#[test]
fn a_gpu_kernel_is_charged_against_the_step_limit() {
    pscat::set_gpu_preview(true).expect("gpu build");
    let mut it = Interp::with_page(600, 600).expect("page");
    // Less than one dispatch's straight-line steps.
    it.set_limits(Limits::none().with_max_steps(100_000))
        .expect("limits");
    assert_eq!(
        it.run_str(&runaway(600)).unwrap_err(),
        PsError::StepsExceeded
    );
}

#[test]
fn a_gpu_kernel_runs_under_limits() {
    pscat::set_gpu_preview(true).expect("gpu build");
    let mut it = Interp::with_page(256, 256).expect("page");
    it.set_limits(
        Limits::none()
            .with_max_time(Duration::from_secs(30))
            .with_max_steps(1_000_000_000),
    )
    .expect("limits");
    it.run_str(
        "/pixel { 256 div exch 256 div 0.5 } bind def \
         256 256 [256 0 0 -256 0 256] /pixel load 3 kernelimage .kernelstats",
    )
    .expect("render");
    // The compiled path (the GPU, or the CPU on a machine without one)
    // finished the image.
    let st = it.operand_stack();
    assert_eq!(st[st.len() - 3].repr(), "1", "not compiled");
}

/// A grid row wider than one GPU dispatch would make a dispatch past the
/// bound the limits rely on (and past the device's workgroup count): the
/// CPU does that image instead, under the same limits (Codex review).
#[test]
fn a_row_wider_than_a_dispatch_runs_on_the_cpu() {
    pscat::set_gpu_preview(true).expect("gpu build");
    let mut it = Interp::with_page(64, 4).expect("page");
    it.set_limits(Limits::none().with_max_time(Duration::from_secs(30)))
        .expect("limits");
    // Past 65535 workgroups of 64 lanes, where one dispatch would panic.
    let w = 4_200_000;
    it.run_str(&format!(
        "/pixel {{ pop {w} div }} bind def \
         64 4 scale {w} 1 [{w} 0 0 -1 0 1] /pixel load 1 kernelimage .kernelstats"
    ))
    .expect("render");
    let st = it.operand_stack();
    assert_eq!(st[st.len() - 3].repr(), "1", "the compiled CPU path ran");
}
