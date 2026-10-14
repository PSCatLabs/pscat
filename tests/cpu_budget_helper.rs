//! The CPU-time budget helper itself (issue #329): the clock must not
//! count time the thread spent waiting, and the budget check must be
//! able to fail.

mod common;

use common::{cpu_time_of, within_cpu_budget};
use std::time::Duration;

/// The whole point of the switch: sleeping costs wall time, not CPU time.
#[test]
fn sleeping_is_wall_time_not_cpu_time() {
    let ((), cpu) = cpu_time_of(|| std::thread::sleep(Duration::from_millis(300)));
    assert!(
        cpu < Duration::from_millis(100),
        "300 ms asleep read as {cpu:?} of CPU"
    );
}

/// Real work is measured: a fixed amount of arithmetic reads as more than
/// nothing (a loose lower bound, so a fast machine cannot fail it).
#[test]
fn computing_is_cpu_time() {
    let (sum, cpu) = cpu_time_of(|| {
        (0..200_000_000u64).fold(0u64, |a, i| std::hint::black_box(a.wrapping_add(i * i)))
    });
    assert_ne!(sum, 1);
    assert!(
        cpu > Duration::from_millis(1),
        "200M iterations read as {cpu:?}"
    );
}

/// Under budget passes and returns the closure's value.
#[test]
fn an_idle_closure_is_within_budget() {
    let v = within_cpu_budget("idle", Duration::from_millis(500), 3, || {
        std::thread::sleep(Duration::from_millis(50));
        7
    });
    assert_eq!(v, 7);
}

/// The negative check: a budget no work can meet must fail, after
/// exhausting every try.
#[test]
#[should_panic(expected = "over the 0ns budget")]
fn an_impossible_budget_fails() {
    within_cpu_budget("spin", Duration::ZERO, 2, || {
        std::hint::black_box((0..1_000_000u64).sum::<u64>())
    });
}

/// A slow first try is forgiven when a later one is fast (best-of-N).
#[test]
fn a_slow_outlier_is_forgiven() {
    let mut calls = 0;
    within_cpu_budget("outlier", Duration::from_millis(100), 3, || {
        calls += 1;
        if calls == 1 {
            // burn well past the budget in CPU, not sleep
            let (_, _) = cpu_time_of(|| {
                let t = common::thread_cpu_time();
                while common::thread_cpu_time() - t < Duration::from_millis(250) {
                    std::hint::spin_loop();
                }
            });
        }
    });
    assert_eq!(calls, 2);
}
