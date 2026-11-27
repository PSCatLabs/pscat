//! A write to stdout or stderr that the run's deadline can stop (#1457).
//!
//! A plain `write_all` to a pipe nobody drains blocks in the kernel with no
//! way out, so `--max-time` could not end a program stuck on a slow
//! consumer. Here a write to a pipe or a stream socket is never bigger than
//! the room the descriptor has, and while there is none it waits in `poll(2)`
//! (polling every few milliseconds) and asks `look` (the deadline and the
//! host's interrupt flag) between waits.
//!
//! The file's `O_NONBLOCK` flag is never set: it belongs to the open file
//! description, which the parent shell and sibling processes share, and
//! flipping it under them makes their writes fail with `EAGAIN`. Instead:
//!
//! - An anonymous pipe is written no more than it has room for. On Linux
//!   with `pwritev2(RWF_NOWAIT)`, non-blocking for that call only (or, on a
//!   kernel without it for pipes, `PIPE_BUF` bytes after `poll` reports a
//!   free page slot); on macOS the free space `fstat` reports (`st_blksize - st_size`), which
//!   also lets a full pipe grow from 16 to 64 KiB as a plain write would.
//! - A stream socket is written with `send(MSG_DONTWAIT)` on Linux, and on
//!   macOS (which ignores that flag on a blocking socket) like a pipe, 512
//!   bytes after `poll` reports it writable (it then has 2048 free).
//! - Anything else is written as before, blocking: a regular file, a
//!   terminal, `/dev/null`, a datagram or seqpacket socket (splitting a write
//!   would change its message boundaries), and on macOS a named FIFO
//!   (socket-backed there: `fstat` does not describe its buffer and `poll`
//!   calls it writable with one byte free).
//!
//! The guarantee is for one writer: another process writing into the same
//! pipe between the room check and the `write` can fill it, and that write
//! then blocks as it always did. See `notes/1457-stdout-write-deadline.md`.
//!
//! **Stdout's line buffer.** std's stdout is a `LineWriter` of 1024 bytes,
//! and where it flushes decides how stdout and stderr interleave and in which
//! operator a write error surfaces. The timed path cannot write through it
//! (its flush would block), so it keeps its own buffer, [`LINE`], with
//! std's rules exactly ([`line_write`]), owned by one interpreter at a time,
//! and sent ([`send_line`]) wherever the CLI flushes its stdout, so the two
//! never reorder.

use crate::PsError;
use std::sync::Mutex;
#[cfg(target_os = "linux")]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};

/// How a descriptor is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FdKind {
    /// A plain blocking write.
    Blocking,
    /// An anonymous pipe (any FIFO on Linux): no more than it has room for.
    Fifo,
    /// A stream socket: `send(MSG_DONTWAIT)` on Linux, else as a pipe in
    /// small pieces.
    Socket,
}

/// What `fd` is, for the timed path.
pub(crate) fn classify(fd: libc::c_int) -> FdKind {
    // SAFETY: `fstat` fills the zeroed struct or fails; nothing else is read.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return FdKind::Blocking;
    }
    match st.st_mode & libc::S_IFMT {
        // A macOS named FIFO lives on a file system (`st_dev` set, even once
        // unlinked); an anonymous pipe has `st_dev` 0.
        libc::S_IFIFO if cfg!(target_os = "macos") && st.st_dev != 0 => FdKind::Blocking,
        libc::S_IFIFO => FdKind::Fifo,
        libc::S_IFSOCK if socket_type(fd) == Some(libc::SOCK_STREAM) => FdKind::Socket,
        _ => FdKind::Blocking,
    }
}

fn socket_type(fd: libc::c_int) -> Option<libc::c_int> {
    let mut ty: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: `ty` and `len` describe one writable `c_int`.
    let r = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut ty as *mut libc::c_int).cast(),
            &mut len,
        )
    };
    (r == 0).then_some(ty)
}

/// Why a timed write stopped, and how much of it was written first.
#[derive(Debug)]
pub(crate) struct Stopped {
    pub(crate) written: usize,
    pub(crate) why: WriteStop,
}

#[derive(Debug)]
pub(crate) enum WriteStop {
    /// The write failed (`EPIPE` for a reader that went away).
    Io(std::io::Error),
    /// `look` said stop: the deadline passed or the host interrupted.
    Limit(PsError),
}

/// The polling interval while a write waits for room: `look` is asked about
/// this often, so a stop is noticed about this late (plus scheduling).
const POLL_MS: libc::c_int = 10;

/// Bytes written between looks while the reader keeps up, so a reader that
/// takes a few bytes at a time cannot keep the write from ever being asked.
const LOOK_EVERY: usize = 64 << 10;

/// Whether `send(MSG_DONTWAIT)` is non-blocking on a socket. Linux honours
/// the flag; macOS's `sosend` ignores it for a blocking socket (a test
/// caught `send` asleep in the kernel), so there a socket is written like a
/// pipe: `poll`, then a write no bigger than [`SOCKET_CHUNK`].
const SOCKET_DONTWAIT: bool = cfg!(any(target_os = "linux", target_os = "android"));

/// The most one write sends to a stream socket `poll` called writable, where
/// `MSG_DONTWAIT` is not honoured: below the 2048-byte send low-water mark
/// a writable macOS socket has free.
const SOCKET_CHUNK: usize = 512;

/// Write all of `bytes` to `fd`, asking `look` first, whenever the write
/// would wait, after an interrupted system call and every [`LOOK_EVERY`]
/// bytes. A stop leaves the bytes already written written ([`Stopped`] says
/// how many); the rest are not.
pub(crate) fn write_all(
    fd: libc::c_int,
    mut kind: FdKind,
    bytes: &[u8],
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), Stopped> {
    let mut done = 0usize;
    let stop = |done: usize, why: WriteStop| Stopped { written: done, why };
    if bytes.is_empty() {
        return Ok(());
    }
    look().map_err(|e| stop(0, WriteStop::Limit(e)))?;
    let mut since_look = 0usize;
    while done < bytes.len() {
        let rest = &bytes[done..];
        #[cfg(target_os = "linux")]
        if kind == FdKind::Fifo && !NOWAIT_UNSUPPORTED.load(Ordering::Relaxed) {
            let iov = libc::iovec {
                iov_base: rest.as_ptr().cast_mut().cast::<libc::c_void>(),
                iov_len: rest.len(),
            };
            // SAFETY: one `iovec` describing the live `rest` slice; `-1` is
            // the current position, the only offset a pipe takes.
            let r = unsafe { libc::pwritev2(fd, &iov, 1, -1, libc::RWF_NOWAIT) };
            if r > 0 {
                let r = r as usize;
                done += r;
                since_look += r;
                if since_look >= LOOK_EVERY {
                    look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                    since_look = 0;
                }
                continue;
            }
            if r == 0 {
                return Err(stop(
                    done,
                    WriteStop::Io(std::io::ErrorKind::WriteZero.into()),
                ));
            }
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                // No room for this write (a write of up to `PIPE_BUF` bytes
                // goes in whole or not at all): wait for the reader.
                Some(e) if e == libc::EAGAIN || e == libc::EINTR => {
                    if e == libc::EAGAIN {
                        let _ = wait_writable(fd);
                    }
                    look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                    since_look = 0;
                }
                // A kernel without `RWF_NOWAIT` for pipes: `poll` from now on.
                Some(e) if e == libc::EOPNOTSUPP || e == libc::EINVAL || e == libc::ENOSYS => {
                    NOWAIT_UNSUPPORTED.store(true, Ordering::Relaxed);
                }
                _ => return Err(stop(done, WriteStop::Io(err))),
            }
            continue;
        }
        let n = match kind {
            FdKind::Fifo => match fifo_room(fd) {
                Room::Free(free, _) => rest.len().min(free),
                Room::Grows => rest.len().min(libc::PIPE_BUF),
                Room::Unknown | Room::Full => match wait_writable(fd) {
                    // Linux: a writable pipe has a free page slot.
                    Ready::Yes if fifo_room(fd) == Room::Unknown => rest.len().min(libc::PIPE_BUF),
                    // A reader that went away (or an error): the write that
                    // follows reports it, as a plain write would.
                    Ready::Hangup => rest.len().min(libc::PIPE_BUF),
                    // macOS: read the room again; asked here too, so `poll`
                    // and `fstat` disagreeing cannot spin past the deadline.
                    Ready::Yes | Ready::NotYet => {
                        look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                        since_look = 0;
                        continue;
                    }
                    // Not a descriptor `poll` understands: write as before.
                    Ready::Unknown => {
                        kind = FdKind::Blocking;
                        rest.len()
                    }
                },
            },
            FdKind::Socket if !SOCKET_DONTWAIT => match wait_writable(fd) {
                Ready::Yes | Ready::Hangup => rest.len().min(SOCKET_CHUNK),
                Ready::NotYet => {
                    look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                    since_look = 0;
                    continue;
                }
                Ready::Unknown => {
                    kind = FdKind::Blocking;
                    rest.len()
                }
            },
            _ => rest.len(),
        };
        let ptr = rest.as_ptr().cast::<libc::c_void>();
        // SAFETY: `ptr` and `n` describe the live `rest` slice (n <= len).
        let r = unsafe {
            if kind == FdKind::Socket && SOCKET_DONTWAIT {
                libc::send(fd, ptr, n, SEND_FLAGS)
            } else {
                libc::write(fd, ptr, n)
            }
        };
        if r > 0 {
            let r = r as usize;
            done += r;
            since_look += r;
            if since_look >= LOOK_EVERY {
                look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                since_look = 0;
            }
            continue;
        }
        if r == 0 {
            // As std's `write_all`: no progress on a non-empty buffer is an
            // error, not a reason to try again.
            return Err(stop(
                done,
                WriteStop::Io(std::io::ErrorKind::WriteZero.into()),
            ));
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINTR) => {
                look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                since_look = 0;
            }
            // A full socket, or a pipe someone else made non-blocking.
            Some(e) if e == libc::EAGAIN || e == libc::EWOULDBLOCK => {
                if wait_writable(fd) == Ready::Unknown {
                    kind = FdKind::Blocking;
                }
                look().map_err(|e| stop(done, WriteStop::Limit(e)))?;
                since_look = 0;
            }
            _ => return Err(stop(done, WriteStop::Io(err))),
        }
    }
    Ok(())
}

/// Linux pipes take `pwritev2(RWF_NOWAIT)`: non-blocking for that one call,
/// with exactly a plain write's room. `poll` alone undercounts it: it calls a
/// pipe full once its 16 page slots are in use, while a small write still
/// fits in the last page (an undrained pipe held 4 KiB less than without a
/// limit). Set once a kernel refuses the flag; `poll` then.
#[cfg(target_os = "linux")]
static NOWAIT_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

/// `send` without blocking, for this one call, and without `SIGPIPE`, so a
/// peer that went away is `EPIPE` as a plain `write` is under the Rust
/// runtime (which ignores `SIGPIPE`). Used only where [`SOCKET_DONTWAIT`].
#[cfg(any(target_os = "linux", target_os = "android"))]
const SEND_FLAGS: libc::c_int = libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const SEND_FLAGS: libc::c_int = 0;

/// How much a pipe takes without blocking, where the system says.
// Only macOS says; elsewhere every pipe is `Unknown`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(PartialEq, Eq, Debug)]
enum Room {
    /// This many bytes are free: a write of no more returns at once. The
    /// flag: the pipe can still grow, so a bigger write does not block
    /// either.
    Free(usize, bool),
    /// Full, but smaller than the most a pipe grows to: a small write makes
    /// the kernel grow it, as a plain write would (macOS starts a pipe at
    /// 16 KiB and grows it to 64 KiB on a write that does not fit; `poll`
    /// calls a full 16 KiB pipe not writable, so without this an undrained
    /// pipe would hold a quarter of what it did). The write blocks only if
    /// the kernel cannot grow the pipe (out of pipe memory).
    Grows,
    /// Full at its largest: wait for the reader.
    Full,
    /// The system does not say: trust `poll`.
    Unknown,
}

/// The largest a macOS pipe grows to (`BIG_PIPE_SIZE` in XNU's
/// `sys_pipe.c`).
#[cfg(target_os = "macos")]
const MACOS_BIG_PIPE: i64 = 64 << 10;

/// A macOS pipe's room: `fstat` on either end gives the buffer's size as
/// `st_blksize` and the bytes in it as `st_size`.
#[cfg(target_os = "macos")]
fn fifo_room(fd: libc::c_int) -> Room {
    // SAFETY: `fstat` fills the zeroed struct or fails; nothing else is read.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return Room::Unknown;
    }
    let (size, held) = (i64::from(st.st_blksize), st.st_size);
    if size <= 0 || held < 0 {
        return Room::Unknown;
    }
    match size - held {
        free if free > 0 => Room::Free(usize::try_from(free).unwrap_or(0), size < MACOS_BIG_PIPE),
        _ if size < MACOS_BIG_PIPE => Room::Grows,
        _ => Room::Full,
    }
}

/// Elsewhere `poll` is the measure (Linux: a writable pipe has a free page
/// slot, so a write of `PIPE_BUF` returns at once).
#[cfg(not(target_os = "macos"))]
fn fifo_room(_fd: libc::c_int) -> Room {
    Room::Unknown
}

#[derive(PartialEq, Eq)]
enum Ready {
    Yes,
    /// Hung up or failed: a write would return at once with the error.
    Hangup,
    NotYet,
    Unknown,
}

/// Wait up to `ms` for `fd` to take a write.
fn poll_out(fd: libc::c_int, ms: libc::c_int) -> Ready {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: one valid `pollfd`, count 1.
    let r = unsafe { libc::poll(&mut p, 1, ms) };
    if r < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EINTR) | Some(libc::EAGAIN) => Ready::NotYet,
            _ => Ready::Unknown,
        };
    }
    if r == 0 {
        Ready::NotYet
    } else if p.revents & libc::POLLNVAL != 0 {
        Ready::Unknown
    } else if p.revents & (libc::POLLERR | libc::POLLHUP) != 0 {
        Ready::Hangup
    } else {
        Ready::Yes
    }
}

fn wait_writable(fd: libc::c_int) -> Ready {
    poll_out(fd, POLL_MS)
}

/// Whether a plain write of `n` bytes (no more than std's stdout buffer)
/// would return without waiting, for the end-of-run check. "No" only where
/// the system says the write would block; where it cannot say exactly, the
/// answer leans to "yes", which leaves the bytes to the exit flush as
/// without a limit.
///
/// - macOS pipe: `fstat`'s free room, exact, or below 64 KiB (a write that
///   does not fit grows it).
/// - Linux pipe: writable by `poll` (a free page slot), or the free bytes
///   (`F_GETPIPE_SZ` less `FIONREAD`) hold `n`: `poll` alone calls a pipe
///   full while its last page still takes a small write.
/// - Linux stream socket: writable by `poll`, or the queued bytes
///   (`SIOCOUTQ`) are under the send buffer (`SO_SNDBUF`), where a send
///   still goes in: `poll` calls a Unix socket writable only while at most a
///   quarter of the buffer is used.
/// - macOS stream socket: nothing on the writing end says how much its peer
///   takes (`POLLOUT` needs 2048 bytes free; a 1024-byte send fits in less),
///   so always "yes": the exit flush may then wait on a full socket, as
///   without a limit.
fn has_room(fd: libc::c_int, kind: FdKind, n: usize) -> bool {
    if poll_out(fd, 0) != Ready::NotYet {
        // Writable, hung up (the write fails at once) or not pollable.
        return match fifo_room(fd) {
            Room::Free(free, grows) if kind == FdKind::Fifo => grows || free >= n,
            _ => true,
        };
    }
    match kind {
        FdKind::Fifo => match fifo_room(fd) {
            Room::Free(free, grows) => grows || free >= n,
            Room::Grows => true,
            Room::Full => false,
            Room::Unknown => linux_pipe_free(fd).is_none_or(|free| free >= n),
        },
        FdKind::Socket => linux_socket_has_room(fd).unwrap_or(true),
        FdKind::Blocking => true,
    }
}

/// A Linux pipe's free bytes: its size less what it holds.
#[cfg(target_os = "linux")]
fn linux_pipe_free(fd: libc::c_int) -> Option<usize> {
    // SAFETY: `fcntl` with no argument; `ioctl` writes one `c_int`.
    let size = unsafe { libc::fcntl(fd, libc::F_GETPIPE_SZ) };
    let mut held: libc::c_int = 0;
    let r = unsafe { libc::ioctl(fd, libc::FIONREAD, &mut held) };
    if size < 0 || r != 0 || held < 0 {
        return None;
    }
    Some((size as usize).saturating_sub(held as usize))
}

#[cfg(not(target_os = "linux"))]
fn linux_pipe_free(_fd: libc::c_int) -> Option<usize> {
    None
}

/// Whether a Linux stream socket's send queue is under its send buffer.
/// `None` elsewhere, or when the system does not say.
#[cfg(target_os = "linux")]
fn linux_socket_has_room(fd: libc::c_int) -> Option<bool> {
    let mut queued: libc::c_int = 0;
    // SAFETY: `ioctl` writes one `c_int`; `getsockopt` one `c_int` and its
    // length.
    let r = unsafe { libc::ioctl(fd, libc::TIOCOUTQ, &mut queued) };
    let mut sndbuf: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let s = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_SNDBUF,
            (&mut sndbuf as *mut libc::c_int).cast(),
            &mut len,
        )
    };
    (r == 0 && s == 0).then_some(queued < sndbuf)
}

#[cfg(not(target_os = "linux"))]
fn linux_socket_has_room(_fd: libc::c_int) -> Option<bool> {
    None
}

// ---------------------------------------------------------------------
// Stdout's line buffer
// ---------------------------------------------------------------------

/// The capacity of std's stdout buffer (`LineWriter::new`: 1024 bytes).
/// The flush points below follow from it, pinned against the plain path by
/// `tests/stdout_deadline.rs` (1023, 1024, 1025 bytes; 1023 + 1; 1023 + 1 +
/// 1; 1000 + 100).
const CAPACITY: usize = 1024;

/// Program stdout written on the timed path and not yet sent: std's
/// `LineWriter` buffer, kept here because the timed path cannot write
/// through std's (its flush would block). Never more than [`CAPACITY`].
///
/// One interpreter owns it at a time (`owner`, an `Interp`'s `host_id`; 0
/// for none): the first to write while it is empty claims it, and it is
/// free again once empty. Another interpreter's writes meanwhile go straight
/// to the descriptor ([`LineWrite::NotOwner`]), and its `Drop` never touches
/// it, so one interpreter cannot send another's partial line early, drop it,
/// or wait on it.
struct Line {
    buf: Vec<u8>,
    owner: u64,
}

static LINE: Mutex<Line> = Mutex::new(Line {
    buf: Vec::new(),
    owner: 0,
});

/// The owner of [`LINE`], 0 for none: what a reader that must not wait
/// on the lock (`Drop`, the plain path) looks at first. Written only with
/// the lock held.
static OWNER: AtomicU64 = AtomicU64::new(0);

/// Whether interpreter `id` holds bytes in the line buffer.
pub(crate) fn owned_by(id: u64) -> bool {
    OWNER.load(Ordering::Relaxed) == id
}

/// Take `mutex` without waiting past `look`: try, and between tries ask
/// `look` (a thread stuck in a write may hold it).
fn lock_within<'a, T>(
    mutex: &'a Mutex<T>,
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<std::sync::MutexGuard<'a, T>, PsError> {
    loop {
        match mutex.try_lock() {
            Ok(guard) => return Ok(guard),
            // A panic mid-write poisons it; what it holds is still the output.
            Err(std::sync::TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {
                look()?;
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
    }
}

fn lock_line() -> std::sync::MutexGuard<'static, Line> {
    LINE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Keep [`OWNER`] in step after `line` changed: the buffer is free once
/// empty.
fn settle(line: &mut Line) {
    if line.buf.is_empty() {
        line.owner = 0;
    }
    OWNER.store(line.owner, Ordering::Relaxed);
}

/// Serialises the timed stderr writes of several threads, as std's stderr
/// lock does for its own, but taken within the deadline.
static STDERR: Mutex<()> = Mutex::new(());

/// A timed write to stderr (unbuffered, as std's is).
pub(crate) fn stderr_write(
    kind: FdKind,
    bytes: &[u8],
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), WriteStop> {
    let _guard = lock_within(&STDERR, look).map_err(WriteStop::Limit)?;
    write_all(2, kind, bytes, look).map_err(|s| s.why)
}

/// What [`line_write`] did.
pub(crate) enum LineWrite {
    /// Written as std's `LineWriter` would have.
    Done,
    /// Another interpreter owns the buffer: the caller writes its bytes
    /// itself, straight through.
    NotOwner,
}

/// Write `bytes` to stdout (fd 1, of `kind`) for interpreter `id` as std's
/// `LineWriter` would, then flush when `flush` is set: the same bytes leave
/// at the same call, and an error is this call's.
pub(crate) fn line_write(
    id: u64,
    kind: FdKind,
    bytes: &[u8],
    flush: bool,
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<LineWrite, WriteStop> {
    let mut line = lock_within(&LINE, look).map_err(WriteStop::Limit)?;
    if line.owner != 0 && line.owner != id {
        return Ok(LineWrite::NotOwner);
    }
    line.owner = id;
    let mut out = LineOut {
        buf: &mut line.buf,
        kind,
        look,
    };
    let mut r = out.write_all(bytes);
    if r.is_ok() && flush {
        r = out.flush_buf();
    }
    settle(&mut line);
    r.map(|()| LineWrite::Done)
}

/// A write of `bytes` straight to stdout, for an interpreter that does not
/// own the line buffer.
pub(crate) fn direct_write(
    kind: FdKind,
    bytes: &[u8],
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), WriteStop> {
    write_all(1, kind, bytes, look).map_err(|s| s.why)
}

struct LineOut<'a, 'b> {
    buf: &'a mut Vec<u8>,
    kind: FdKind,
    look: &'b mut dyn FnMut() -> Result<(), PsError>,
}

impl LineOut<'_, '_> {
    /// Straight to the descriptor (std's `inner.write_all`): a stop loses
    /// the bytes not written, as there.
    fn direct(&mut self, bytes: &[u8]) -> Result<(), WriteStop> {
        write_all(1, self.kind, bytes, self.look).map_err(|s| s.why)
    }

    /// `BufWriter::flush_buf`: what was written leaves the buffer, the rest
    /// stays for a later flush.
    fn flush_buf(&mut self) -> Result<(), WriteStop> {
        if self.buf.is_empty() {
            return Ok(());
        }
        match write_all(1, self.kind, self.buf, self.look) {
            Ok(()) => {
                self.buf.clear();
                Ok(())
            }
            Err(s) => {
                self.buf.drain(..s.written);
                Err(s.why)
            }
        }
    }

    /// `BufWriter::write_all`.
    fn buffered_write_all(&mut self, bytes: &[u8]) -> Result<(), WriteStop> {
        let spare = CAPACITY.saturating_sub(self.buf.len());
        if bytes.len() < spare {
            self.buf.extend_from_slice(bytes);
            return Ok(());
        }
        if bytes.len() > spare {
            self.flush_buf()?;
        }
        if bytes.len() >= CAPACITY {
            self.direct(bytes)
        } else {
            self.buf.extend_from_slice(bytes);
            Ok(())
        }
    }

    /// `LineWriterShim::write_all`.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteStop> {
        match bytes.iter().rposition(|&b| b == b'\n') {
            None => {
                if self.buf.last() == Some(&b'\n') {
                    self.flush_buf()?;
                }
                self.buffered_write_all(bytes)
            }
            Some(i) => {
                let (lines, tail) = bytes.split_at(i + 1);
                if self.buf.is_empty() {
                    self.direct(lines)?;
                } else {
                    self.buffered_write_all(lines)?;
                    self.flush_buf()?;
                }
                self.buffered_write_all(tail)
            }
        }
    }
}

/// Send what the line buffer holds now, with no deadline: what std's flush
/// does where the CLI flushes its stdout (its own output, the REPL prompt,
/// the end of `main`), and where the owner is dropped. A write that fails
/// keeps the bytes not written in the buffer, as std's `flush_buf` does, so
/// the next flush fails again, and returns the error: a reader that went
/// away still shows at the CLI's last flush (Codex round 2, R1).
///
/// `only` limits it to one owner (`Drop`, the plain path); `None` sends
/// whatever is there (the CLI, which runs one interpreter).
pub(crate) fn send_line(only: Option<u64>) -> std::io::Result<()> {
    if only.is_some_and(|id| !owned_by(id)) {
        return Ok(());
    }
    let mut line = lock_line();
    if only.is_some_and(|id| line.owner != id) || line.buf.is_empty() {
        return Ok(());
    }
    let kind = classify(1);
    let mut never = || Ok(());
    let r = write_all(1, kind, &line.buf, &mut never);
    let r = match r {
        Ok(()) => {
            line.buf.clear();
            Ok(())
        }
        Err(s) => {
            line.buf.drain(..s.written);
            Err(match s.why {
                WriteStop::Io(e) => e,
                WriteStop::Limit(_) => std::io::ErrorKind::Interrupted.into(),
            })
        }
    };
    settle(&mut line);
    r
}

/// Give up `id`'s ownership of the line buffer (its `Drop`, after a send
/// that failed): the bytes stay, for the CLI's last flush to report, and the
/// next interpreter to write claims them with the buffer.
pub(crate) fn release(id: u64) {
    if !owned_by(id) {
        return;
    }
    let mut line = lock_line();
    if line.owner == id {
        line.owner = 0;
        OWNER.store(0, Ordering::Relaxed);
    }
}

/// At the end of a run under a time limit, with bytes of `id`'s in the line
/// buffer: make sure the flush that will send them (the CLI's, or the
/// interpreter's `Drop`) cannot block. Wait (asking `look`, holding the
/// buffer so no other interpreter changes it meanwhile) until stdout has room
/// for them; if `look` stops the wait, drop them and return its error. With
/// room they stay where they are, as without a limit. Not this interpreter's
/// buffer: nothing to do.
pub(crate) fn ensure_room_at_end(
    id: u64,
    kind: FdKind,
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), PsError> {
    if !owned_by(id) {
        return Ok(());
    }
    let mut line = lock_within(&LINE, look)?;
    if line.owner != id || line.buf.is_empty() {
        return Ok(());
    }
    let n = line.buf.len();
    loop {
        if has_room(1, kind, n) {
            return Ok(());
        }
        if let Err(e) = look() {
            line.buf.clear();
            settle(&mut line);
            return Err(e);
        }
        let _ = wait_writable(1);
    }
}

/// Write what the line buffer holds for a process about to exit from another
/// thread (the CLI's parent-gone watch), then drop it. No lock is waited on
/// (the main thread may be stuck in a write holding it), and the write waits
/// for room at most once, one polling interval: what stdout takes by then is
/// sent, the rest dropped.
pub(crate) fn drain_now() {
    let mut line = match LINE.try_lock() {
        Ok(line) => line,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return,
    };
    let mut first = true;
    let kind = classify(1);
    // Only a pipe or a socket ever filled it; a descriptor now of another
    // kind (moved since) is not written, as a plain write there may block.
    if kind == FdKind::Blocking {
        line.buf.clear();
        settle(&mut line);
        return;
    }
    let _ = write_all(1, kind, &line.buf, &mut || {
        if std::mem::take(&mut first) {
            Ok(())
        } else {
            Err(PsError::Interrupted)
        }
    });
    line.buf.clear();
    settle(&mut line);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::fd::AsRawFd;

    /// A full pipe stops the write at the first look that says so, with the
    /// pipe's worth written and nothing past it.
    #[test]
    fn a_full_pipe_stops_at_the_look() {
        let (mut reader, writer) = std::io::pipe().expect("pipe");
        let data = vec![7u8; 4 << 20];
        let mut looks = 0;
        let r = write_all(writer.as_raw_fd(), FdKind::Fifo, &data, &mut || {
            looks += 1;
            if looks >= 3 {
                Err(PsError::TimeExceeded)
            } else {
                Ok(())
            }
        });
        let Err(Stopped {
            written,
            why: WriteStop::Limit(PsError::TimeExceeded),
        }) = r
        else {
            panic!("{r:?}");
        };
        drop(writer);
        let mut held = Vec::new();
        reader.read_to_end(&mut held).expect("read");
        assert_eq!(held.len(), written);
        assert!(!held.is_empty() && held.len() < data.len());
        assert!(held.iter().all(|&b| b == 7));
    }

    /// A deadline already past stops the write before its first byte.
    #[test]
    fn a_past_deadline_writes_nothing() {
        let (mut reader, writer) = std::io::pipe().expect("pipe");
        let r = write_all(writer.as_raw_fd(), FdKind::Fifo, b"abc", &mut || {
            Err(PsError::TimeExceeded)
        });
        assert!(matches!(r, Err(Stopped { written: 0, .. })));
        drop(writer);
        let mut held = Vec::new();
        reader.read_to_end(&mut held).expect("read");
        assert!(held.is_empty());
    }

    /// A drained pipe gets every byte, in order.
    #[test]
    fn a_drained_pipe_gets_everything() {
        let (mut reader, writer) = std::io::pipe().expect("pipe");
        let data: Vec<u8> = (0..(1u32 << 20)).map(|i| (i % 251) as u8).collect();
        let t = std::thread::spawn(move || {
            let mut all = Vec::new();
            reader.read_to_end(&mut all).expect("read");
            all
        });
        let r = write_all(writer.as_raw_fd(), FdKind::Fifo, &data, &mut || Ok(()));
        assert!(r.is_ok());
        drop(writer);
        assert_eq!(t.join().expect("reader"), data);
    }

    /// A closed reader is `EPIPE`, as for a plain write.
    #[test]
    fn a_closed_pipe_is_epipe() {
        let (reader, writer) = std::io::pipe().expect("pipe");
        drop(reader);
        let r = write_all(writer.as_raw_fd(), FdKind::Fifo, b"x", &mut || Ok(()));
        match r {
            Err(Stopped {
                why: WriteStop::Io(e),
                ..
            }) => assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe),
            other => panic!("{other:?}"),
        }
    }

    fn set_nonblocking(fd: libc::c_int) {
        // SAFETY: flag change on a descriptor this test owns alone.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }

    /// Whether a plain write of `n` bytes would wait: a non-blocking one is
    /// refused (`EAGAIN`) or takes only part of it (a blocking one would then
    /// wait for the rest).
    fn would_block(fd: libc::c_int, n: usize) -> bool {
        let data = vec![b'#'; n];
        // SAFETY: a write of a live buffer.
        let r = unsafe { libc::write(fd, data.as_ptr().cast(), n) };
        r < 0 || (r as usize) < n
    }

    /// Fill `fd` (non-blocking, owned by the test) in steps; whenever
    /// `has_room` says no, a write of that size must really be refused.
    fn room_is_never_denied_wrongly(fd: libc::c_int, kind: FdKind) {
        set_nonblocking(fd);
        let chunk = vec![b'#'; 700];
        for _ in 0..400 {
            for n in [1, 100, 1024] {
                if !has_room(fd, kind, n) {
                    assert!(
                        would_block(fd, n),
                        "{kind:?}: no room said for {n} bytes that fit"
                    );
                }
            }
            // SAFETY: a write of a live buffer.
            if unsafe { libc::write(fd, chunk.as_ptr().cast(), chunk.len()) } < 0 {
                break;
            }
        }
    }

    #[test]
    fn the_end_of_run_check_never_denies_room_that_is_there() {
        let (_reader, writer) = std::io::pipe().expect("pipe");
        room_is_never_denied_wrongly(writer.as_raw_fd(), FdKind::Fifo);
        let (_a, b) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        room_is_never_denied_wrongly(b.as_raw_fd(), FdKind::Socket);
        // The Linux state `poll` miscounts: 15 full pages and part of a 16th.
        let (_reader, writer) = std::io::pipe().expect("pipe");
        let fd = writer.as_raw_fd();
        set_nonblocking(fd);
        let page = vec![b'#'; 4096];
        for _ in 0..15 {
            // SAFETY: a write of a live buffer.
            assert_eq!(unsafe { libc::write(fd, page.as_ptr().cast(), 4096) }, 4096);
        }
        // SAFETY: as above.
        assert_eq!(unsafe { libc::write(fd, page.as_ptr().cast(), 3000) }, 3000);
        assert!(has_room(fd, FdKind::Fifo, 1024), "a 1024-byte write fits");
        assert!(!would_block(fd, 1024));
    }

    #[test]
    fn kinds() {
        let (reader, _writer) = std::io::pipe().expect("pipe");
        assert_eq!(classify(reader.as_raw_fd()), FdKind::Fifo);
        let (a, _b) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        assert_eq!(classify(a.as_raw_fd()), FdKind::Socket);
        let (d, _e) = std::os::unix::net::UnixDatagram::pair().expect("datagram pair");
        assert_eq!(classify(d.as_raw_fd()), FdKind::Blocking);
        assert_eq!(classify(-1), FdKind::Blocking);
    }
}
