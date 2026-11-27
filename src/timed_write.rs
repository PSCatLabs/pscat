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
//! (its flush would block), so it keeps its own buffer, [`BUFFER`], with
//! std's rules exactly ([`line_write`]): process-wide like std's, and handed
//! over to std's own buffer ([`hand_over`]) before anything else writes to
//! stdout, so the two never reorder.

use crate::PsError;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

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
                Room::Free(free) => rest.len().min(free),
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
    /// This many bytes are free: a write of no more returns at once.
    Free(usize),
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
        free if free > 0 => Room::Free(usize::try_from(free).unwrap_or(0)),
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

/// Whether `fd` takes `n` bytes now without blocking (`n` no more than
/// std's stdout buffer, which every writable pipe and stream socket has).
fn has_room(fd: libc::c_int, kind: FdKind, n: usize) -> bool {
    if kind == FdKind::Fifo {
        match fifo_room(fd) {
            Room::Free(free) => return free >= n,
            Room::Grows => return true,
            Room::Full => return poll_out(fd, 0) == Ready::Hangup,
            Room::Unknown => {}
        }
    }
    poll_out(fd, 0) != Ready::NotYet
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
/// through std's. Process-wide like std's; never more than [`CAPACITY`].
static BUFFER: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// [`BUFFER`] is not empty: the one load the plain path pays to know it has
/// nothing to hand over.
static BUFFERED: AtomicBool = AtomicBool::new(false);

fn buffer() -> std::sync::MutexGuard<'static, Vec<u8>> {
    // A panic mid-write poisons it; the bytes in it are still the output.
    BUFFER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the timed path holds stdout bytes not yet sent.
pub(crate) fn buffered() -> bool {
    BUFFERED.load(Ordering::Relaxed)
}

/// Write `bytes` to stdout (fd 1, of `kind`) as std's `LineWriter` would,
/// then flush when `flush` is set: the same bytes leave at the same call,
/// and an error is this call's. The caller holds std's stdout lock.
pub(crate) fn line_write(
    kind: FdKind,
    bytes: &[u8],
    flush: bool,
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), WriteStop> {
    let mut buf = buffer();
    let mut out = LineOut {
        buf: &mut buf,
        kind,
        look,
    };
    let mut r = out.write_all(bytes);
    if r.is_ok() && flush {
        r = out.flush_buf();
    }
    BUFFERED.store(!buf.is_empty(), Ordering::Relaxed);
    r
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

/// Move what [`BUFFER`] holds into std's own stdout buffer, so whatever
/// writes to stdout next (the CLI's own output, a REPL prompt, the exit
/// flush) comes after it, as it would if it had been in std's buffer all
/// along. No I/O as long as std's buffer is empty (it always is, during a
/// run): a partial line under [`CAPACITY`] bytes, written in two pieces so
/// std never takes its write-through path for a buffer-sized piece.
pub(crate) fn hand_over() -> std::io::Result<()> {
    use std::io::Write;
    if !buffered() {
        return Ok(());
    }
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let bytes = std::mem::take(&mut *buffer());
    BUFFERED.store(false, Ordering::Relaxed);
    if bytes.is_empty() {
        return Ok(());
    }
    if bytes.contains(&b'\n') {
        // Lines left behind by a failed flush: std would retry them at its
        // next flush; this is it.
        return lock.write_all(&bytes);
    }
    let (head, last) = bytes.split_at(bytes.len() - 1);
    lock.write_all(head)?;
    lock.write_all(last)
}

/// At the end of a run under a time limit, with bytes left in the buffer:
/// make sure the exit flush that will send them cannot block. Wait (asking
/// `look`) until stdout has room for them; if `look` stops the wait, drop
/// them and return its error. With room they stay where they are, as
/// without a limit.
pub(crate) fn ensure_room_at_end(
    kind: FdKind,
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), PsError> {
    let n = buffer().len();
    if n == 0 {
        return Ok(());
    }
    loop {
        if has_room(1, kind, n) {
            return Ok(());
        }
        if let Err(e) = look() {
            buffer().clear();
            BUFFERED.store(false, Ordering::Relaxed);
            return Err(e);
        }
        let _ = wait_writable(1);
    }
}

/// Write what [`BUFFER`] holds without waiting, for a process about to exit
/// from another thread (the CLI's parent-gone watch): what the descriptor
/// takes now is sent, the rest dropped. Never blocks on a lock: a thread
/// stuck in a write holds them.
pub(crate) fn drain_now() {
    let Ok(mut buf) = BUFFER.try_lock() else {
        return;
    };
    let mut first = true;
    let _ = write_all(1, classify(1), &buf, &mut || {
        if std::mem::take(&mut first) {
            Ok(())
        } else {
            Err(PsError::Interrupted)
        }
    });
    buf.clear();
    BUFFERED.store(false, Ordering::Relaxed);
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
