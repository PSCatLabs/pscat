//! A write to stdout or stderr that the run's deadline can stop (#1457).
//!
//! A plain `write_all` to a pipe nobody drains blocks in the kernel with no
//! way out, so `--max-time` could not end a program stuck on a slow
//! consumer. Here a write to a pipe or a socket waits in `poll(2)` instead,
//! a few milliseconds at a time, and asks `look` (the deadline and the
//! host's interrupt flag) between waits.
//!
//! The file's `O_NONBLOCK` flag is never set: it belongs to the open file
//! description, which the parent shell and sibling processes share, and
//! flipping it under them makes their writes fail with `EAGAIN`. Instead:
//!
//! - A pipe (FIFO) is written at most `PIPE_BUF` bytes at a time, and only
//!   after `poll` reports it writable. A writable pipe has room for
//!   `PIPE_BUF` bytes (macOS: `PIPE_BUF` free; Linux: a free page slot), so
//!   that blocking write returns at once. Another process writing to the same
//!   pipe between the `poll` and the `write` can still fill it: then the
//!   write blocks as it always did.
//! - A socket is written with `send(MSG_DONTWAIT)` on Linux, non-blocking
//!   for that call only, and waits in `poll` on `EAGAIN`. macOS ignores the
//!   flag on a blocking socket, so there it is written like a pipe, in
//!   pieces small enough for a writable socket to take at once.
//! - Anything else (a regular file, a terminal, `/dev/null`) is written as
//!   before: a file does not block on a reader, and a terminal's `poll` is
//!   not dependable on macOS. See `notes/1457-stdout-write-deadline.md`.

use crate::PsError;
use std::sync::OnceLock;

/// How a descriptor is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FdKind {
    /// A plain blocking write: regular files, terminals, character devices.
    Blocking,
    /// A pipe or FIFO: `poll`, then at most `PIPE_BUF` bytes.
    Fifo,
    /// A socket: `send(MSG_DONTWAIT)` on Linux, else as a pipe in small pieces.
    Socket,
}

fn kind_of(fd: libc::c_int) -> FdKind {
    // SAFETY: `fstat` fills the zeroed struct or fails; nothing else is read.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } != 0 {
        return FdKind::Blocking;
    }
    match st.st_mode & libc::S_IFMT {
        libc::S_IFIFO => FdKind::Fifo,
        libc::S_IFSOCK => FdKind::Socket,
        _ => FdKind::Blocking,
    }
}

/// What fd 1 is, looked at once per process: the CLI never moves it, and an
/// `fstat` per write would cost the hot path a system call.
pub(crate) fn stdout_kind() -> FdKind {
    static KIND: OnceLock<FdKind> = OnceLock::new();
    *KIND.get_or_init(|| kind_of(1))
}

/// What fd 2 is, as [`stdout_kind`].
pub(crate) fn stderr_kind() -> FdKind {
    static KIND: OnceLock<FdKind> = OnceLock::new();
    *KIND.get_or_init(|| kind_of(2))
}

/// Why a timed write stopped.
#[derive(Debug)]
pub(crate) enum WriteStop {
    /// The write failed (`EPIPE` for a reader that went away).
    Io(std::io::Error),
    /// `look` said stop: the deadline passed or the host interrupted.
    Limit(PsError),
}

/// How long one `poll` waits before `look` is asked again. The time limit is
/// noticed this much late at most.
const POLL_MS: libc::c_int = 10;

/// Bytes written between looks while the reader keeps up, so a reader that
/// takes a few bytes at a time cannot keep the write from ever being asked.
const LOOK_EVERY: usize = 64 << 10;

/// Whether `send(MSG_DONTWAIT)` is non-blocking on a socket. Linux honours
/// the flag; macOS's `sosend` ignores it for a blocking socket (a test
/// caught `send` asleep in the kernel), so there a socket is written like a
/// pipe: `poll`, then a write no bigger than [`SOCKET_CHUNK`].
const SOCKET_DONTWAIT: bool = cfg!(any(target_os = "linux", target_os = "android"));

/// The most one write sends to a socket `poll` called writable, where
/// `MSG_DONTWAIT` is not honoured: below the default send low-water mark
/// (2048 bytes on macOS), which is the least room a writable socket has.
const SOCKET_CHUNK: usize = 512;

/// Write all of `bytes` to `fd`, asking `look` whenever the write would
/// wait and every [`LOOK_EVERY`] bytes. A stop leaves the bytes already
/// written written; the rest are not.
pub(crate) fn write_all(
    fd: libc::c_int,
    mut kind: FdKind,
    mut bytes: &[u8],
    look: &mut dyn FnMut() -> Result<(), PsError>,
) -> Result<(), WriteStop> {
    let mut since_look = 0usize;
    while !bytes.is_empty() {
        let poll_first = match kind {
            FdKind::Fifo => true,
            FdKind::Socket => !SOCKET_DONTWAIT,
            FdKind::Blocking => false,
        };
        if poll_first {
            match wait_writable(fd) {
                Ready::Yes => {}
                Ready::NotYet => {
                    look().map_err(WriteStop::Limit)?;
                    since_look = 0;
                    continue;
                }
                // Not a descriptor `poll` understands: write as before.
                Ready::Unknown => kind = FdKind::Blocking,
            }
        }
        let n = match kind {
            FdKind::Fifo => bytes.len().min(libc::PIPE_BUF),
            FdKind::Socket if !SOCKET_DONTWAIT => bytes.len().min(SOCKET_CHUNK),
            _ => bytes.len(),
        };
        let ptr = bytes.as_ptr().cast::<libc::c_void>();
        // SAFETY: `ptr` and `n` describe the live `bytes` slice.
        let r = unsafe {
            if kind == FdKind::Socket && SOCKET_DONTWAIT {
                libc::send(fd, ptr, n, SEND_FLAGS)
            } else {
                libc::write(fd, ptr, n)
            }
        };
        if r >= 0 {
            let r = r as usize;
            bytes = &bytes[r..];
            since_look += r;
            if since_look >= LOOK_EVERY && !bytes.is_empty() {
                look().map_err(WriteStop::Limit)?;
                since_look = 0;
            }
            continue;
        }
        let err = std::io::Error::last_os_error();
        match err.raw_os_error() {
            Some(libc::EINTR) => {}
            // A full socket, or a pipe someone else made non-blocking.
            Some(e) if e == libc::EAGAIN || e == libc::EWOULDBLOCK => {
                if wait_writable(fd) == Ready::Unknown {
                    kind = FdKind::Blocking;
                }
                look().map_err(WriteStop::Limit)?;
                since_look = 0;
            }
            // Detected as a socket but `send` does not apply: write as before.
            Some(libc::ENOTSOCK) if kind == FdKind::Socket => kind = FdKind::Blocking,
            _ => return Err(WriteStop::Io(err)),
        }
    }
    Ok(())
}

/// `send` without blocking, for this one call, and without `SIGPIPE`, so a
/// peer that went away is `EPIPE` as a plain `write` is under the Rust
/// runtime (which ignores `SIGPIPE`). Used only where [`SOCKET_DONTWAIT`].
#[cfg(any(target_os = "linux", target_os = "android"))]
const SEND_FLAGS: libc::c_int = libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL;
#[cfg(not(any(target_os = "linux", target_os = "android")))]
const SEND_FLAGS: libc::c_int = 0;

#[derive(PartialEq, Eq)]
enum Ready {
    Yes,
    NotYet,
    Unknown,
}

/// Wait up to [`POLL_MS`] for `fd` to take a write. A hung-up or failed
/// descriptor is "ready": the write that follows reports it (`EPIPE`), as
/// a plain write would.
fn wait_writable(fd: libc::c_int) -> Ready {
    let mut p = libc::pollfd {
        fd,
        events: libc::POLLOUT,
        revents: 0,
    };
    // SAFETY: one valid `pollfd`, count 1.
    let r = unsafe { libc::poll(&mut p, 1, POLL_MS) };
    if r < 0 {
        return match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EINTR) | Some(libc::EAGAIN) => Ready::NotYet,
            _ => Ready::Unknown,
        };
    }
    if r == 0 {
        return Ready::NotYet;
    }
    if p.revents & libc::POLLNVAL != 0 {
        Ready::Unknown
    } else {
        Ready::Yes
    }
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
        assert!(matches!(r, Err(WriteStop::Limit(PsError::TimeExceeded))));
        drop(writer);
        let mut held = Vec::new();
        reader.read_to_end(&mut held).expect("read");
        assert!(!held.is_empty() && held.len() < data.len());
        assert!(held.iter().all(|&b| b == 7));
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
            Err(WriteStop::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::BrokenPipe),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn kinds() {
        let (reader, _writer) = std::io::pipe().expect("pipe");
        assert_eq!(kind_of(reader.as_raw_fd()), FdKind::Fifo);
        let (a, _b) = std::os::unix::net::UnixStream::pair().expect("socket pair");
        assert_eq!(kind_of(a.as_raw_fd()), FdKind::Socket);
        assert_eq!(kind_of(-1), FdKind::Blocking);
    }
}
