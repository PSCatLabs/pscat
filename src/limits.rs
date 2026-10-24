//! Execution limits for running a program you do not trust: [`Limits`].
//!
//! A [`Limits`] caps how much work and memory one program may use. Give it
//! to an interpreter with [`Interp::set_limits`](crate::Interp::set_limits);
//! when a program passes a cap, the run stops with
//! [`PsError::StepsExceeded`](crate::PsError::StepsExceeded) (or `TimeExceeded`,
//! `MemoryExceeded`) and the page it
//! had drawn so far is still there to read. Nothing is limited by default:
//! `Limits::default()` changes nothing, so existing programs and renders are
//! unaffected.
//!
//! ```
//! use pscat::{Interp, Limits, PsError};
//!
//! let mut interp = Interp::new();
//! interp.set_limits(Limits::none().with_max_steps(10_000))?;
//! // A runaway loop stops with a structured error instead of hanging.
//! let err = interp.run_str("{ } loop").unwrap_err();
//! assert_eq!(err, PsError::StepsExceeded);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! The limits apply to one program, from [`Interp::begin_source`] (or
//! [`run_str`](crate::Interp::run_str)) to its end; the step count and the
//! clock start again for the next program run on the same interpreter. A
//! program cannot catch a limit error with `stopped`.
//!
//! What a limit can and cannot stop is spelled out in `SECURITY.md`: steps
//! are counted between objects, so a single long-running operator (one
//! enormous fill, a whole-file read) finishes before the time or step limit
//! is looked at.
//!
//! [`Interp::begin_source`]: crate::Interp::begin_source

use std::time::Duration;

use thiserror::Error;

/// Which limit a program passed: [`PsError::limit`](crate::PsError::limit)
/// returns it for a limit error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Limit {
    /// More than [`Limits::max_steps`] interpreter steps.
    Steps,
    /// Ran longer than [`Limits::max_time`].
    Time,
    /// Grew the heap by more than [`Limits::max_memory`] bytes.
    Memory,
}

impl Limit {
    /// The limit's short name: `steps`, `time` or `memory`. It is what the
    /// CLI's error report and the MCP server's `limit` field carry.
    pub fn name(self) -> &'static str {
        match self {
            Limit::Steps => "steps",
            Limit::Time => "time",
            Limit::Memory => "memory",
        }
    }
}

impl std::fmt::Display for Limit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Caps on one program's work and memory. `None` means no cap.
///
/// Build one from [`Limits::none`] with the `with_*` methods, or take
/// [`Limits::untrusted`], the preset the MCP server uses. The struct is
/// `#[non_exhaustive]`, so a new kind of limit is not a breaking change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Limits {
    /// The most interpreter steps (objects executed, including every pass
    /// round a loop body) a program may run.
    pub max_steps: Option<u64>,
    /// The longest wall-clock time a program may run. Not enforced in the
    /// WebAssembly build, which has no clock.
    pub max_time: Option<Duration>,
    /// The most heap, in bytes, the process may grow by while the program
    /// runs. Needs [`TrackingAllocator`](crate::TrackingAllocator) installed
    /// as the global allocator: [`Interp::set_limits`](crate::Interp::set_limits)
    /// returns [`LimitsError::AllocatorNotInstalled`] without it.
    pub max_memory: Option<usize>,
}

impl Limits {
    /// No limits at all. Same as `Limits::default()`.
    pub const fn none() -> Self {
        Limits {
            max_steps: None,
            max_time: None,
            max_memory: None,
        }
    }

    /// The preset for programs from strangers, which the MCP server applies
    /// by default: [`UNTRUSTED_MAX_STEPS`] steps, [`UNTRUSTED_MAX_TIME`] and
    /// [`UNTRUSTED_MAX_MEMORY`] bytes. Chosen from the heaviest gallery
    /// pieces with about ten times of headroom (`SECURITY.md`).
    pub const fn untrusted() -> Self {
        Limits {
            max_steps: Some(UNTRUSTED_MAX_STEPS),
            max_time: Some(UNTRUSTED_MAX_TIME),
            max_memory: Some(UNTRUSTED_MAX_MEMORY),
        }
    }

    /// Cap the step count.
    pub const fn with_max_steps(mut self, steps: u64) -> Self {
        self.max_steps = Some(steps);
        self
    }

    /// Cap the wall-clock time.
    pub const fn with_max_time(mut self, time: Duration) -> Self {
        self.max_time = Some(time);
        self
    }

    /// Cap the heap growth, in bytes.
    pub const fn with_max_memory(mut self, bytes: usize) -> Self {
        self.max_memory = Some(bytes);
        self
    }

    /// True when no limit is set (the interpreter's fast path).
    pub const fn is_none(&self) -> bool {
        self.max_steps.is_none() && self.max_time.is_none() && self.max_memory.is_none()
    }
}

/// Steps in [`Limits::untrusted`]: 25 billion, about ten times the heaviest
/// gallery piece (2.3 billion). At the interpreter's speed (about 60 million
/// steps a second) that is minutes, so with the default time limit the clock
/// is what stops a runaway first; the step cap is the bound that does not
/// depend on how fast the machine is, and the one that binds once the time
/// limit is raised.
pub const UNTRUSTED_MAX_STEPS: u64 = 25_000_000_000;
/// Wall-clock time in [`Limits::untrusted`].
pub const UNTRUSTED_MAX_TIME: Duration = Duration::from_secs(45);
/// Heap growth in [`Limits::untrusted`]: 1 GiB.
pub const UNTRUSTED_MAX_MEMORY: usize = 1 << 30;

/// Why [`Interp::set_limits`](crate::Interp::set_limits) refused a set of
/// limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum LimitsError {
    /// `max_memory` was set but [`TrackingAllocator`](crate::TrackingAllocator)
    /// is not the global allocator, so heap use cannot be measured. A limit
    /// that cannot be enforced is refused rather than silently ignored.
    #[error("a memory limit needs pscat::TrackingAllocator installed as the #[global_allocator]")]
    AllocatorNotInstalled,
}
