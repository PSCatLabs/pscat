//! File objects and decode filters.
//!
//! A `PsFile` is a byte source with a *shared position*: the scanner,
//! `currentfile`, `read`/`readstring`/`readhexstring`, and filters all
//! pull from the same cursor. That sharing is the whole point — it is
//! what makes `currentfile eexec` (embedded Type 1 fonts), inline image
//! data, and `currentfile ... readhexstring` prologs work: the program
//! reads data from the middle of its own source, and the interpreter
//! resumes scanning exactly where the data ended.
//!
//! Filters are files layered on files. Decoding is **on demand**, one
//! pull at a time, so a filter consumes only as many source bytes as
//! its consumer actually needed — the property eexec's
//! `currentfile closefile` convention depends on (the plaintext zeros
//! after the encrypted region must remain unread until the outer
//! scanner gets there).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use crate::error::PsError;

pub type FileHandle = Rc<RefCell<PsFile>>;

pub struct PsFile {
    kind: Kind,
    closed: bool,
    /// Bytes handed to consumers — the `token` operator's remainder
    /// arithmetic and diagnostics.
    consumed: usize,
    /// 1-indexed line of the next byte to be read. Source-line
    /// attribution for error reports (issue #17). Counts CR, LF, and
    /// CRLF as one line ending each (PostScript accepts all three) —
    /// `last_was_cr` is what keeps a CRLF pair from counting twice.
    line: usize,
    last_was_cr: bool,
}

enum Kind {
    Bytes {
        data: Vec<u8>,
        pos: usize,
    },
    Filter {
        source: FileHandle,
        dec: Decoder,
        out: VecDeque<u8>,
        eod: bool,
    },
    /// A file opened for writing (issue #740). Reading one is an
    /// `ioerror`. `None` once closed (a disk file's handle is dropped).
    Output(Option<Sink>),
}

/// Where an output file's bytes go. The write operators dispatch on it:
/// `%stdout` goes through the interpreter (`Interp::write_stdout`), so a
/// captured stdout (the browser build, tests) sees file writes in order
/// with `print`.
pub enum Sink {
    Stdout,
    Stderr,
    /// A disk file, buffered ([`DiskSink`]); the interpreter flushes every
    /// open one at `showpage`, `quit`, `flush` and before a read, and the
    /// CLI flushes them all on every exit path (issue #762).
    Disk(DiskSink),
}

/// Bytes a [`DiskSink`] holds before it writes them out.
const DISK_BUFFER: usize = 64 * 1024;

/// A buffered disk file (issue #762). Not `BufWriter`, because
/// `resetfile` must be able to throw the pending bytes away, and
/// `fileposition` must know the real position, buffer included.
///
/// A failed write leaves a sticky error, as C's `ferror` does: the next
/// write or flush (and so `closefile`) reports `ioerror`, so a deferred
/// failure is not lost when the flush came from `showpage` or `quit`,
/// which cannot raise it. `resetfile` clears it. Dropping a sink flushes
/// it, so a file the program never closed (or dropped unreferenced) still
/// reaches the disk.
pub struct DiskSink {
    file: std::fs::File,
    buf: Vec<u8>,
    failed: bool,
    /// The opening interpreter's note: set when this sink is dropped with
    /// bytes it could not write, for `Interp::flush_outputs` to report.
    lost: Rc<Cell<bool>>,
}

impl DiskSink {
    pub fn new(file: std::fs::File, lost: Rc<Cell<bool>>) -> Self {
        DiskSink {
            file,
            buf: Vec::new(),
            failed: false,
            lost,
        }
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), PsError> {
        use std::io::Write;
        if self.failed {
            return Err(PsError::Io);
        }
        if self.buf.len() + bytes.len() <= DISK_BUFFER {
            self.buf.extend_from_slice(bytes);
            return Ok(());
        }
        self.flush()?;
        if bytes.len() >= DISK_BUFFER {
            return self.file.write_all(bytes).map_err(|_| {
                self.failed = true;
                PsError::Io
            });
        }
        self.buf.extend_from_slice(bytes);
        Ok(())
    }

    /// Write out what is buffered. The buffer is emptied even on failure.
    pub fn flush(&mut self) -> Result<(), PsError> {
        use std::io::Write;
        if self.failed {
            self.buf.clear();
            return Err(PsError::Io);
        }
        if self.buf.is_empty() {
            return Ok(());
        }
        let wrote = self.file.write_all(&self.buf);
        self.buf.clear();
        wrote.map_err(|_| {
            self.failed = true;
            PsError::Io
        })
    }

    /// `resetfile`: drop the pending bytes and the sticky error.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.failed = false;
    }

    /// The position the next write lands at (after a flush).
    fn position(&mut self) -> Result<u64, PsError> {
        use std::io::Seek;
        self.flush()?;
        self.file.stream_position().map_err(|_| PsError::Io)
    }

    /// Seek to `pos`, which must lie within the file: `0..=length`, so a
    /// seek never leaves a hole (a sparse file would cost no counted
    /// bytes). A file opened for append still writes at its end.
    fn seek(&mut self, pos: u64) -> Result<(), PsError> {
        use std::io::{Seek, SeekFrom};
        self.flush()?;
        let len = self.file.metadata().map_err(|_| PsError::Io)?.len();
        if pos > len {
            return Err(PsError::Rangecheck);
        }
        self.file
            .seek(SeekFrom::Start(pos))
            .map(|_| ())
            .map_err(|_| PsError::Io)
    }
}

impl Drop for DiskSink {
    fn drop(&mut self) {
        // Nobody can be told now, so remember it: the interpreter that
        // opened the file reports it at its final `flush_outputs`.
        if self.flush().is_err() {
            self.lost.set(true);
        }
    }
}

impl PsFile {
    pub fn from_bytes(data: Vec<u8>) -> FileHandle {
        Rc::new(RefCell::new(PsFile {
            kind: Kind::Bytes { data, pos: 0 },
            closed: false,
            consumed: 0,
            line: 1,
            last_was_cr: false,
        }))
    }

    /// A file for writing to `sink`.
    pub fn output(sink: Sink) -> FileHandle {
        Rc::new(RefCell::new(PsFile {
            kind: Kind::Output(Some(sink)),
            closed: false,
            consumed: 0,
            line: 1,
            last_was_cr: false,
        }))
    }

    /// An open output file's sink; `None` for an input file or a closed one.
    pub fn sink_mut(&mut self) -> Option<&mut Sink> {
        match &mut self.kind {
            Kind::Output(sink) if !self.closed => sink.as_mut(),
            _ => None,
        }
    }

    /// Whether this is an output file (open or closed).
    pub fn is_output(&self) -> bool {
        matches!(self.kind, Kind::Output(_))
    }

    /// The PLRM's "invalid file object" — what `currentfile` returns
    /// when no file is being executed.
    pub fn closed_file() -> FileHandle {
        let f = Self::from_bytes(Vec::new());
        f.borrow_mut().closed = true;
        f
    }

    pub fn filtered(source: FileHandle, dec: Decoder) -> FileHandle {
        Rc::new(RefCell::new(PsFile {
            kind: Kind::Filter {
                source,
                dec,
                out: VecDeque::new(),
                eod: false,
            },
            closed: false,
            consumed: 0,
            line: 1,
            last_was_cr: false,
        }))
    }

    pub fn read_byte(&mut self) -> Result<Option<u8>, PsError> {
        if self.closed {
            return Ok(None);
        }
        let b = match &mut self.kind {
            Kind::Bytes { data, pos } => {
                let b = data.get(*pos).copied();
                if b.is_some() {
                    *pos += 1;
                }
                b
            }
            Kind::Filter {
                source,
                dec,
                out,
                eod,
            } => {
                Self::ensure_output(source, dec, out, eod)?;
                out.pop_front()
            }
            Kind::Output(_) => return Err(PsError::Io),
        };
        if let Some(byte) = b {
            self.consumed += 1;
            match byte {
                b'\r' => {
                    self.line += 1;
                    self.last_was_cr = true;
                }
                b'\n' => {
                    // A lone LF ends a line; the LF half of a CRLF pair
                    // doesn't (the CR just before it already counted).
                    if !self.last_was_cr {
                        self.line += 1;
                    }
                    self.last_was_cr = false;
                }
                _ => self.last_was_cr = false,
            }
        }
        Ok(b)
    }

    /// Append up to `max` bytes to `buf` and return how many were added
    /// (fewer only at end of data). `ioerror` for a closed file or an output
    /// file. The position, byte count and line count move exactly as that
    /// many [`Self::read_byte`] calls would move them (issue #773).
    pub fn read_into(&mut self, buf: &mut Vec<u8>, max: usize) -> Result<usize, PsError> {
        if self.closed {
            return Err(PsError::Io);
        }
        let start = buf.len();
        if self.is_filter() {
            for _ in 0..max {
                match self.read_byte()? {
                    Some(b) => buf.push(b),
                    None => break,
                }
            }
            return Ok(buf.len() - start);
        }
        match &mut self.kind {
            Kind::Bytes { data, pos } => {
                let take = max.min(data.len() - *pos);
                buf.extend_from_slice(&data[*pos..*pos + take]);
                *pos += take;
            }
            _ => return Err(PsError::Io),
        }
        // The bookkeeping `read_byte` does, over the bytes just taken.
        self.consumed += buf.len() - start;
        for &byte in &buf[start..] {
            match byte {
                b'\r' => {
                    self.line += 1;
                    self.last_was_cr = true;
                }
                b'\n' => {
                    if !self.last_was_cr {
                        self.line += 1;
                    }
                    self.last_was_cr = false;
                }
                _ => self.last_was_cr = false,
            }
        }
        Ok(buf.len() - start)
    }

    pub fn peek_byte(&mut self) -> Result<Option<u8>, PsError> {
        if self.closed {
            return Ok(None);
        }
        Ok(match &mut self.kind {
            Kind::Bytes { data, pos } => data.get(*pos).copied(),
            Kind::Filter {
                source,
                dec,
                out,
                eod,
            } => {
                Self::ensure_output(source, dec, out, eod)?;
                out.front().copied()
            }
            Kind::Output(_) => return Err(PsError::Io),
        })
    }

    fn ensure_output(
        source: &FileHandle,
        dec: &mut Decoder,
        out: &mut VecDeque<u8>,
        eod: &mut bool,
    ) -> Result<(), PsError> {
        while out.is_empty() && !*eod {
            dec.pull(source, out, eod)?;
        }
        Ok(())
    }

    pub fn close(&mut self) {
        self.closed = true;
        // Drop a disk file's handle now; `%stdout`/`%stderr` only mark
        // this file object closed, never the process's streams.
        if let Kind::Output(sink) = &mut self.kind {
            *sink = None;
        }
    }

    pub fn is_open(&self) -> bool {
        !self.closed
    }

    /// `fileposition`: the byte offset of the next read or write.
    /// `ioerror` for a file that cannot be positioned (a filter, `%stdout`,
    /// `%stderr`) and for a closed one.
    pub fn file_position(&mut self) -> Result<u64, PsError> {
        if self.closed {
            return Err(PsError::Io);
        }
        match &mut self.kind {
            Kind::Bytes { pos, .. } => Ok(*pos as u64),
            Kind::Output(Some(Sink::Disk(d))) => d.position(),
            Kind::Filter { .. } | Kind::Output(_) => Err(PsError::Io),
        }
    }

    /// `setfileposition`: move the next read or write to `to`.
    /// `rangecheck` for a position outside `0..=length`; `ioerror` as for
    /// [`Self::file_position`]. The line count follows the new position.
    pub fn set_file_position(&mut self, to: i64) -> Result<(), PsError> {
        if self.closed {
            return Err(PsError::Io);
        }
        match &mut self.kind {
            Kind::Bytes { data, pos } => {
                let to = usize::try_from(to)
                    .ok()
                    .filter(|t| *t <= data.len())
                    .ok_or(PsError::Rangecheck)?;
                *pos = to;
                self.consumed = to;
                // Recount lines up to here: CR, LF and CRLF end one each.
                let mut line = 1;
                let mut prev_cr = false;
                for &b in &data[..to] {
                    match b {
                        b'\r' => line += 1,
                        b'\n' if !prev_cr => line += 1,
                        _ => {}
                    }
                    prev_cr = b == b'\r';
                }
                self.line = line;
                self.last_was_cr = prev_cr;
                Ok(())
            }
            Kind::Output(Some(Sink::Disk(d))) => {
                d.seek(u64::try_from(to).map_err(|_| PsError::Rangecheck)?)
            }
            Kind::Filter { .. } | Kind::Output(_) => Err(PsError::Io),
        }
    }

    /// `resetfile`: an output file throws away what it has buffered and
    /// forgets a write error; an input file holds no buffer of its own
    /// (its bytes are in memory), so nothing changes. A closed file is not
    /// an error, as in Ghostscript.
    pub fn reset(&mut self) {
        if self.closed {
            return;
        }
        if let Kind::Output(Some(Sink::Disk(d))) = &mut self.kind {
            d.reset();
        }
    }

    /// Flush a disk output file's buffer (no-op for anything else).
    pub fn flush_disk(&mut self) -> Result<(), PsError> {
        match self.sink_mut() {
            Some(Sink::Disk(d)) => d.flush(),
            _ => Ok(()),
        }
    }

    /// `bytesavailable`: None means unknown (filters), per the PLRM's -1.
    pub fn bytes_available(&self) -> Option<usize> {
        if self.closed {
            return Some(0);
        }
        match &self.kind {
            Kind::Bytes { data, pos } => Some(data.len() - pos),
            Kind::Filter { .. } | Kind::Output(_) => None,
        }
    }

    pub fn pos(&self) -> usize {
        self.consumed
    }

    /// 1-indexed line of the next unread byte.
    pub fn line(&self) -> usize {
        self.line
    }

    /// Whether this file is a decode filter (as opposed to raw bytes).
    /// `image` drains filter sources to their EOD marker on completion
    /// — that's how the scanner gets past an inline image's trailing
    /// `>` or `~>` (pinned against gs); raw files are left exact.
    pub fn is_filter(&self) -> bool {
        matches!(self.kind, Kind::Filter { .. })
    }

    /// A filter's underlying source, for draining chains: each layer
    /// must reach its own EOD marker so the bottom raw file ends up
    /// positioned right after all the encoded data.
    pub fn filter_source(&self) -> Option<FileHandle> {
        match &self.kind {
            Kind::Filter { source, .. } => Some(source.clone()),
            Kind::Bytes { .. } | Kind::Output(_) => None,
        }
    }

    pub fn same_object(a: &FileHandle, b: &FileHandle) -> bool {
        Rc::ptr_eq(a, b)
    }
}

/// Streaming decoders. Each `pull` produces at least one output byte or
/// sets `eod` (end of data — the filter's own marker, or source
/// exhaustion, which the PLRM treats as premature-but-terminal).
pub enum Decoder {
    AsciiHex {
        hi: Option<u8>,
    },
    Ascii85 {
        group: [u8; 5],
        n: usize,
    },
    RunLength,
    /// The eexec / Type 1 charstring cipher. Sniffs hexadecimal vs
    /// binary form from the first four bytes, per the Type 1 spec, and
    /// discards `skip` leading plaintext bytes.
    Eexec {
        r: u16,
        skip: usize,
        mode: EexecMode,
    },
    /// zlib (RFC 1950) — PostScript FlateDecode. Fed one byte at a
    /// time so the source stops exactly at the end of the compressed
    /// stream (deflate marks its own final block).
    Flate(Box<flate2::Decompress>),
    /// PostScript LZWDecode: variable 9–12 bit codes, MSB-first,
    /// EarlyChange=1.
    Lzw(Box<LzwState>),
    /// DCTDecode (JPEG, via zune-jpeg). The one decoder that buffers:
    /// JPEG needs whole-stream context (Huffman tables, progressive
    /// scans), so the first pull reads one complete JPEG — marker-aware,
    /// stopping exactly at the EOI — and decodes it all at once. The
    /// flag records that the image has been consumed.
    Dct {
        done: bool,
    },
}

pub struct LzwState {
    /// Bit reservoir, MSB-aligned.
    acc: u32,
    nbits: u32,
    code_width: u32,
    /// Dictionary entries beyond the 256 literals + 2 control codes.
    table: Vec<Vec<u8>>,
    prev: Option<Vec<u8>>,
}

impl Default for LzwState {
    fn default() -> Self {
        LzwState {
            acc: 0,
            nbits: 0,
            code_width: 9,
            table: Vec::new(),
            prev: None,
        }
    }
}

pub enum EexecMode {
    Unknown,
    Binary,
    /// Buffered pending high nibble of a hex pair.
    Hex(Option<u8>),
}

impl Decoder {
    pub fn ascii_hex() -> Self {
        Decoder::AsciiHex { hi: None }
    }

    pub fn ascii85() -> Self {
        Decoder::Ascii85 {
            group: [0; 5],
            n: 0,
        }
    }

    pub fn eexec() -> Self {
        Decoder::Eexec {
            r: 55665,
            skip: 4,
            mode: EexecMode::Unknown,
        }
    }

    pub fn flate() -> Self {
        Decoder::Flate(Box::new(flate2::Decompress::new(true)))
    }

    pub fn lzw() -> Self {
        Decoder::Lzw(Box::default())
    }

    pub fn dct() -> Self {
        Decoder::Dct { done: false }
    }

    fn pull(
        &mut self,
        source: &FileHandle,
        out: &mut VecDeque<u8>,
        eod: &mut bool,
    ) -> Result<(), PsError> {
        let next = || source.borrow_mut().read_byte();
        match self {
            Decoder::AsciiHex { hi } => loop {
                let Some(b) = next()? else {
                    if let Some(h) = hi.take() {
                        out.push_back(h << 4);
                    }
                    *eod = true;
                    return Ok(());
                };
                match b {
                    b'>' => {
                        // An odd final digit is padded with zero.
                        if let Some(h) = hi.take() {
                            out.push_back(h << 4);
                        }
                        *eod = true;
                        return Ok(());
                    }
                    _ if is_ps_whitespace(b) => {}
                    _ => match (b as char).to_digit(16) {
                        Some(v) => match hi.take() {
                            Some(h) => {
                                out.push_back((h << 4) | v as u8);
                                return Ok(());
                            }
                            None => *hi = Some(v as u8),
                        },
                        None => return Err(PsError::Io),
                    },
                }
            },
            Decoder::Ascii85 { group, n } => loop {
                let Some(b) = next()? else {
                    *eod = true;
                    return flush_a85(group, *n, out);
                };
                match b {
                    b'~' => {
                        // The '>' of the '~>' EOD; tolerate EOF after '~'.
                        let _ = next()?;
                        *eod = true;
                        return flush_a85(group, *n, out);
                    }
                    _ if is_ps_whitespace(b) => {}
                    b'z' if *n == 0 => {
                        out.extend([0, 0, 0, 0]);
                        return Ok(());
                    }
                    b'!'..=b'u' => {
                        group[*n] = b - b'!';
                        *n += 1;
                        if *n == 5 {
                            let v = group.iter().fold(0u32, |acc, d| {
                                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
                            });
                            out.extend(v.to_be_bytes());
                            *n = 0;
                            return Ok(());
                        }
                    }
                    _ => return Err(PsError::Io),
                }
            },
            Decoder::RunLength => {
                let Some(len) = next()? else {
                    *eod = true;
                    return Ok(());
                };
                match len {
                    128 => *eod = true,
                    0..=127 => {
                        for _ in 0..=len {
                            let Some(b) = next()? else {
                                *eod = true;
                                return Ok(());
                            };
                            out.push_back(b);
                        }
                    }
                    _ => {
                        let Some(b) = next()? else {
                            *eod = true;
                            return Ok(());
                        };
                        for _ in 0..257 - usize::from(len) {
                            out.push_back(b);
                        }
                    }
                }
                Ok(())
            }
            Decoder::Flate(z) => {
                let mut outbuf = [0u8; 512];
                loop {
                    let Some(b) = next()? else {
                        *eod = true;
                        return Ok(());
                    };
                    let before = z.total_out();
                    let status = z
                        .decompress(&[b], &mut outbuf, flate2::FlushDecompress::None)
                        .map_err(|_| PsError::Io)?;
                    let produced = (z.total_out() - before) as usize;
                    out.extend(&outbuf[..produced]);
                    if matches!(status, flate2::Status::StreamEnd) {
                        *eod = true;
                        return Ok(());
                    }
                    if produced > 0 {
                        return Ok(());
                    }
                }
            }
            Decoder::Lzw(st) => loop {
                // Refill the bit reservoir to one code's worth.
                while st.nbits < st.code_width {
                    let Some(b) = next()? else {
                        *eod = true;
                        return Ok(());
                    };
                    st.acc = (st.acc << 8) | u32::from(b);
                    st.nbits += 8;
                }
                st.nbits -= st.code_width;
                let code = (st.acc >> st.nbits) & ((1 << st.code_width) - 1);
                match code {
                    256 => {
                        st.table.clear();
                        st.code_width = 9;
                        st.prev = None;
                    }
                    257 => {
                        *eod = true;
                        return Ok(());
                    }
                    _ => {
                        let entry: Vec<u8> = if code < 256 {
                            vec![code as u8]
                        } else if let Some(e) = st.table.get(code as usize - 258) {
                            e.clone()
                        } else if let Some(prev) = &st.prev {
                            // The classic KwKwK case: code == next slot.
                            let mut e = prev.clone();
                            e.push(prev[0]);
                            e
                        } else {
                            return Err(PsError::Io);
                        };
                        if let Some(prev) = st.prev.take() {
                            let mut new = prev;
                            new.push(entry[0]);
                            st.table.push(new);
                        }
                        // EarlyChange=1: widen one code early.
                        let filled = st.table.len() + 258;
                        if filled + 1 >= (1 << st.code_width) && st.code_width < 12 {
                            st.code_width += 1;
                        }
                        st.prev = Some(entry.clone());
                        out.extend(entry);
                        return Ok(());
                    }
                }
            },
            Decoder::Dct { done } => {
                use zune_jpeg::zune_core::{colorspace::ColorSpace, options::DecoderOptions};
                if *done {
                    *eod = true;
                    return Ok(());
                }
                *done = true;
                let data = buffer_jpeg(source)?;
                let mut dec = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(data));
                dec.decode_headers().map_err(|_| PsError::Io)?;
                // One output byte per PostScript sample: grayscale JPEGs
                // yield 1 component, CMYK/YCCK 4; everything else (the
                // usual YCbCr) converts to RGB triples.
                let want = match dec.input_colorspace() {
                    Some(ColorSpace::Luma) => ColorSpace::Luma,
                    Some(ColorSpace::CMYK | ColorSpace::YCCK) => ColorSpace::CMYK,
                    _ => ColorSpace::RGB,
                };
                dec.set_options(DecoderOptions::default().jpeg_set_out_colorspace(want));
                out.extend(dec.decode().map_err(|_| PsError::Io)?);
                *eod = true;
                Ok(())
            }
            Decoder::Eexec { r, skip, mode } => {
                if matches!(mode, EexecMode::Unknown) {
                    // Skip leading whitespace, then sniff the first four
                    // bytes: all-hex means the hexadecimal form.
                    let mut first4 = [0u8; 4];
                    let mut i = 0;
                    while i < 4 {
                        let Some(b) = next()? else {
                            *eod = true;
                            return Ok(());
                        };
                        if i == 0 && is_ps_whitespace(b) {
                            continue;
                        }
                        first4[i] = b;
                        i += 1;
                    }
                    let hex = first4.iter().all(u8::is_ascii_hexdigit);
                    *mode = if hex {
                        EexecMode::Hex(None)
                    } else {
                        EexecMode::Binary
                    };
                    for b in first4 {
                        eexec_byte(b, r, skip, mode, out)?;
                    }
                    return Ok(());
                }
                loop {
                    let Some(b) = next()? else {
                        *eod = true;
                        return Ok(());
                    };
                    let before = out.len();
                    eexec_byte(b, r, skip, mode, out)?;
                    if out.len() > before {
                        return Ok(());
                    }
                }
            }
        }
    }
}

/// Read one complete JPEG from `source`, consuming exactly through its
/// EOI marker so bytes after the image stay unread (the shared-cursor
/// contract every other decoder keeps). Marker-aware: segment lengths
/// are honored (an EXIF thumbnail's embedded EOI is skipped, not
/// mistaken for the end) and entropy-coded data is scanned respecting
/// FF00 byte stuffing and RST0–7 restart markers. Premature EOF just
/// ends the buffer — zune-jpeg reports the damage.
fn buffer_jpeg(source: &FileHandle) -> Result<Vec<u8>, PsError> {
    let mut buf = Vec::new();
    macro_rules! byte {
        // The inner block scopes the RefCell borrow — two byte!()s can
        // share a statement.
        () => {
            match {
                let b = source.borrow_mut().read_byte();
                b
            }? {
                Some(b) => {
                    buf.push(b);
                    b
                }
                None => return Ok(buf),
            }
        };
    }
    loop {
        // Find the next marker: skip to an FF (entropy data or fill
        // bytes), take the code, and treat FF00 stuffing and restart
        // markers as data.
        let marker = loop {
            if byte!() != 0xFF {
                continue;
            }
            let mut code = byte!();
            while code == 0xFF {
                code = byte!();
            }
            match code {
                0x00 | 0xD0..=0xD7 => {}
                _ => break code,
            }
        };
        match marker {
            0xD9 => return Ok(buf), // EOI
            0xD8 | 0x01 => {}       // SOI, TEM: no payload
            _ => {
                // Every other marker carries a 16-bit length that
                // counts itself; SOS's header is followed by entropy
                // data, which the scan above walks through.
                let len = usize::from(byte!()) << 8 | usize::from(byte!());
                for _ in 2..len {
                    byte!();
                }
            }
        }
    }
}

/// One input byte of an eexec stream (already past mode sniffing):
/// assemble hex pairs if applicable, decrypt, and emit unless still in
/// the 4-byte plaintext lead-in.
fn eexec_byte(
    b: u8,
    r: &mut u16,
    skip: &mut usize,
    mode: &mut EexecMode,
    out: &mut VecDeque<u8>,
) -> Result<(), PsError> {
    let cipher = match mode {
        EexecMode::Binary => b,
        EexecMode::Hex(hi) => {
            if is_ps_whitespace(b) {
                return Ok(());
            }
            let v = (b as char).to_digit(16).ok_or(PsError::Io)? as u8;
            match hi.take() {
                Some(h) => (h << 4) | v,
                None => {
                    *hi = Some(v);
                    return Ok(());
                }
            }
        }
        EexecMode::Unknown => unreachable!("sniffed before decrypting"),
    };
    let plain = decrypt_byte(cipher, r);
    if *skip > 0 {
        *skip -= 1;
    } else {
        out.push_back(plain);
    }
    Ok(())
}

/// The shared eexec/charstring cipher step (Type 1 spec, C1/C2).
pub fn decrypt_byte(cipher: u8, r: &mut u16) -> u8 {
    let plain = cipher ^ (*r >> 8) as u8;
    *r = (u16::from(cipher).wrapping_add(*r))
        .wrapping_mul(52845)
        .wrapping_add(22719);
    plain
}

/// Charstring decryption: whole-buffer form, dropping `len_iv` leading
/// plaintext bytes (Private dict /lenIV, default 4).
pub fn decrypt_charstring(data: &[u8], len_iv: usize) -> Vec<u8> {
    let mut r: u16 = 4330;
    let mut out = Vec::with_capacity(data.len().saturating_sub(len_iv));
    for (i, &c) in data.iter().enumerate() {
        let p = decrypt_byte(c, &mut r);
        if i >= len_iv {
            out.push(p);
        }
    }
    out
}

fn flush_a85(group: &mut [u8; 5], n: usize, out: &mut VecDeque<u8>) -> Result<(), PsError> {
    match n {
        0 => Ok(()),
        1 => Err(PsError::Io),
        _ => {
            for slot in group.iter_mut().skip(n) {
                *slot = 84;
            }
            let v = group.iter().fold(0u32, |acc, d| {
                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
            });
            out.extend(v.to_be_bytes().into_iter().take(n - 1));
            Ok(())
        }
    }
}

fn is_ps_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

/// Bytes per read when a whole file is read under execution limits
/// ([`read_path_limited`]): the limits are looked at once per chunk.
#[cfg(not(target_arch = "wasm32"))]
const READ_CHUNK: usize = 64 * 1024;

/// How often the limits are looked at while a read waits for a FIFO, a
/// terminal or a socket to produce bytes.
#[cfg(not(target_arch = "wasm32"))]
const READ_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// The interpreter's look at its limits during a whole-file read
/// (`Interp::poll_read`): the whole KiB read since the last look (0 while
/// waiting; see [`read_path_limited`]) and the heap the buffer is about to
/// take (0 for none).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type ReadPoll<'a> = &'a mut dyn FnMut(usize, usize) -> Result<(), PsError>;

/// Make room for `chunk` more bytes in `data`, telling `poll` first how much
/// more heap that takes (the live count already holds the old capacity): `Vec::reserve` grows to `max(2 * capacity, len + chunk)`.
#[cfg(not(target_arch = "wasm32"))]
fn make_room(data: &mut Vec<u8>, chunk: usize, poll: ReadPoll<'_>) -> Result<(), PsError> {
    if data.len() + chunk > data.capacity() {
        let grown = (2 * data.capacity()).max(data.len() + chunk);
        poll(0, grown - data.capacity())?;
        data.reserve(chunk);
    }
    Ok(())
}

/// Read the whole of `path` the way `std::fs::read` does (`Ok(None)` when it
/// cannot be opened or read), but look at the execution limits as the bytes
/// arrive instead of after the last one (issue #702). An `Err` from `poll`
/// stops the read and is returned.
///
/// - A regular file is read on the calling thread, `READ_CHUNK` bytes at a
///   time, into a buffer sized from its length when the limits allow it.
/// - Anything else (`/dev/zero`, a FIFO, a terminal, a socket) may never end
///   or never produce a byte, and the open itself can block, so it is read on
///   a helper thread that hands over chunks through a small channel while
///   this thread waits `READ_POLL` at a time and looks at the limits between
///   waits. When a limit stops the read, the helper is left behind: it exits
///   as soon as its next send fails, or stays blocked in the kernel until the
///   process ends.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_path_limited(
    path: &std::path::Path,
    poll: ReadPoll<'_>,
) -> Result<Option<Vec<u8>>, PsError> {
    use std::io::Read;
    use std::sync::mpsc::{RecvTimeoutError, sync_channel};

    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(None);
    };
    // Steps are charged per whole KiB read, however the source splits its
    // reads: a pipe that returns one byte at a time costs the same as one
    // that returns 64 KiB (the remainder under a KiB is never charged).
    let mut carry = 0usize;
    let mut kib = |n: usize| {
        carry += n;
        let k = carry / 1024;
        carry %= 1024;
        k
    };
    let mut data: Vec<u8> = Vec::new();
    if meta.is_file() {
        let size = usize::try_from(meta.len()).unwrap_or(usize::MAX);
        poll(0, size)?;
        // The whole file's room in one go: no doubling to be refused halfway
        // for a file that passed the check above. A size that cannot be had
        // reads as unreadable, as `std::fs::read` fails on it.
        if data.try_reserve_exact(size.saturating_add(1)).is_err() {
            return Ok(None);
        }
        let Ok(mut f) = std::fs::File::open(path) else {
            return Ok(None);
        };
        loop {
            // Fill the room the size gave (no growth for a file that is what
            // `metadata` said); make more only once it is full.
            if data.len() == data.capacity() {
                make_room(&mut data, READ_CHUNK, poll)?;
            }
            let room = (data.capacity() - data.len()).min(READ_CHUNK);
            match f.by_ref().take(room as u64).read_to_end(&mut data) {
                Ok(0) => return Ok(Some(data)),
                Ok(n) => poll(kib(n), 0)?,
                Err(_) => return Ok(None),
            }
        }
    }
    let (tx, rx) = sync_channel::<std::io::Result<Vec<u8>>>(2);
    let owned = path.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("pscat-read".into())
        .spawn(move || {
            let mut f = match std::fs::File::open(&owned) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            let mut buf = vec![0u8; READ_CHUNK];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => {
                        if tx.send(Ok(buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                }
            }
        });
    if spawned.is_err() {
        return Ok(None);
    }
    loop {
        match rx.recv_timeout(READ_POLL) {
            Ok(Ok(chunk)) => {
                make_room(&mut data, chunk.len(), poll)?;
                data.extend_from_slice(&chunk);
                poll(kib(chunk.len()), 0)?;
            }
            Ok(Err(_)) => return Ok(None),
            Err(RecvTimeoutError::Timeout) => poll(0, 0)?,
            Err(RecvTimeoutError::Disconnected) => return Ok(Some(data)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(f: &FileHandle) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some(b) = f.borrow_mut().read_byte().expect("read") {
            out.push(b);
        }
        out
    }

    #[test]
    fn a_dropped_file_that_could_not_write_leaves_a_note() {
        let path = std::env::temp_dir().join(format!("pscat-lost-{}", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        // A read-only handle: buffering succeeds, the flush at drop cannot.
        let lost = Rc::new(Cell::new(false));
        let mut sink = DiskSink::new(std::fs::File::open(&path).unwrap(), lost.clone());
        sink.write(b"data").unwrap();
        assert!(!lost.get());
        drop(sink);
        assert!(lost.get());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn setting_the_position_recounts_lines_and_the_consumed_count() {
        let f = PsFile::from_bytes(b"a\r\nb\rc\nd".to_vec());
        let at = |pos: i64| {
            f.borrow_mut().set_file_position(pos).unwrap();
            (f.borrow().line(), f.borrow().pos())
        };
        // After the CR of a CRLF pair the LF half must not count again.
        assert_eq!(at(2), (2, 2));
        assert_eq!(at(3), (2, 3));
        f.borrow_mut().read_byte().unwrap();
        assert_eq!(f.borrow().line(), 2);
        assert_eq!(at(4), (2, 4));
        assert_eq!(at(5), (3, 5));
        assert_eq!(at(0), (1, 0));
        assert_eq!(at(7), (4, 7));
        // Reading on from a CR whose LF follows counts one line ending.
        f.borrow_mut().set_file_position(2).unwrap();
        assert_eq!(drain(&f), b"\nb\rc\nd");
        assert_eq!(f.borrow().line(), 4);
        assert_eq!(
            f.borrow_mut().set_file_position(9),
            Err(PsError::Rangecheck)
        );
        assert_eq!(
            f.borrow_mut().set_file_position(-1),
            Err(PsError::Rangecheck)
        );
    }

    #[test]
    fn ascii_hex_decodes_and_stops_at_eod() {
        let src = PsFile::from_bytes(b"48 65 6c6C6f>tail".to_vec());
        let f = PsFile::filtered(src.clone(), Decoder::ascii_hex());
        assert_eq!(drain(&f), b"Hello");
        // The filter consumed exactly through '>' — 'tail' remains.
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b't'));
    }

    #[test]
    fn ascii85_matches_the_lexer() {
        let src = PsFile::from_bytes(b"BOu!r~>rest".to_vec());
        let f = PsFile::filtered(src.clone(), Decoder::ascii85());
        assert_eq!(drain(&f), b"hell");
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b'r'));
    }

    #[test]
    fn run_length_roundtrip() {
        // literal "ab", then 'c' x 4, then EOD.
        let src = PsFile::from_bytes(vec![1, b'a', b'b', 253, b'c', 128, b'X']);
        let f = PsFile::filtered(src.clone(), Decoder::RunLength);
        assert_eq!(drain(&f), b"abcccc");
        assert_eq!(src.borrow_mut().read_byte().expect("read"), Some(b'X'));
    }

    #[test]
    fn eexec_binary_roundtrip() {
        // Encrypt "42 =" with the eexec cipher (4-byte lead), decrypt back.
        let mut r: u16 = 55665;
        let mut enc = Vec::new();
        for &p in b"\0\0\0\0" {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        for &p in b"42 =" {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        let f = PsFile::filtered(PsFile::from_bytes(enc), Decoder::eexec());
        assert_eq!(drain(&f), b"42 =");
    }

    #[test]
    fn charstring_decrypt_roundtrip() {
        let mut r: u16 = 4330;
        let mut enc = Vec::new();
        for &p in &[0u8, 0, 0, 0, 13, 14] {
            let c = p ^ (r >> 8) as u8;
            r = (u16::from(c).wrapping_add(r))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            enc.push(c);
        }
        assert_eq!(decrypt_charstring(&enc, 4), vec![13, 14]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn limited_reads_charge_whole_kib_and_return_every_byte() {
        let path = std::env::temp_dir().join(format!("pscat-file-kib-{}.bin", std::process::id()));
        let body: Vec<u8> = (0..(3 * READ_CHUNK + 1500))
            .map(|i| (i % 251) as u8)
            .collect();
        std::fs::write(&path, &body).unwrap();
        let mut kib_total = 0usize;
        let got = read_path_limited(&path, &mut |k, _| {
            kib_total += k;
            Ok(())
        });
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, Ok(Some(body.clone())));
        assert_eq!(kib_total, body.len() / 1024, "whole KiB only");
        // A limit from the poll stops the read and is what comes back.
        let path = std::env::temp_dir().join(format!("pscat-file-kib2-{}.bin", std::process::id()));
        std::fs::write(&path, &body).unwrap();
        let got = read_path_limited(&path, &mut |_, _| Err(PsError::TimeExceeded));
        let _ = std::fs::remove_file(&path);
        assert_eq!(got, Err(PsError::TimeExceeded));
        // Not there: unreadable, not an error.
        assert_eq!(
            read_path_limited(std::path::Path::new("/no/such/pscat/file"), &mut |_, _| Ok(
                ()
            )),
            Ok(None)
        );
    }
}
