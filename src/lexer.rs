//! The tokenizer ("scanner" in PLRM terms).
//!
//! Operates on bytes, not `char`s: PostScript is a byte-oriented language
//! (strings are byte arrays, and real-world files are not reliably UTF-8).
//!
//! One rule worth calling out because it shapes the whole number parser:
//! anything that *looks* like it might be a number but fails to parse as
//! one is a **name**, not an error. `1.2.3`, `16#zz`, and `-` are all
//! valid executable names per the PLRM.

use crate::error::PsError;
use std::rc::Rc;

use crate::file::{FileHandle, PsFile};
use crate::limits::DecodeBudget;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Token {
    Integer(i64),
    Real(f64),
    /// Contents of a `(...)` or `<...>` string, escapes already processed.
    String(Vec<u8>),
    /// An executable name — including the self-delimiting `[`, `]`, `<<`,
    /// `>>`, which the PLRM defines as ordinary names looked up in the
    /// dictionary stack.
    Name(String),
    /// `/name`
    LiteralName(String),
    /// `//name` — replaced by its current value at scan time.
    ImmediateName(String),
    LBrace,
    RBrace,
    /// A binary object sequence (token types 128 to 131, PLRM 3.14.4),
    /// header included. The scanner only frames the bytes; the interpreter
    /// decodes them (`binobj::decode`) because immediately evaluated names
    /// need the dictionary stack.
    BinarySeq(Vec<u8>),
    /// A homogeneous number array (token type 149, PLRM 3.14.2): the
    /// numbers, as `Integer` and `Real` tokens. It scans to a literal array.
    NumberArray(Vec<Token>),
}

/// The scanner reads through a shared [`FileHandle`], so a program can
/// interleave scanning with data reads from the same source
/// (`currentfile eexec`, inline images) and the position stays
/// consistent. Executable strings scan the same way but are not *files*
/// — `currentfile` skips them.
pub struct Lexer {
    file: FileHandle,
    is_file: bool,
    /// Set only for the top-level program source (`main_program`), not
    /// for `run`-loaded files, eexec streams, or executable strings —
    /// `line()` uses this to avoid attributing an error to the wrong
    /// source (issue #17: a `(lib/artkit.ps) run` file has its own line
    /// 1, and reporting it as a line of the submitted program would be
    /// actively misleading; `run_name` below is how a run file's lines
    /// are reported instead).
    is_main: bool,
    /// Byte offset the token most recently returned (or failed) by
    /// `next_token` started at, for `syntax::check_syntax` (#1096): after
    /// an unterminated string the file position is at EOF, which is no help
    /// to the person who has to find the opening `(`.
    token_start_pos: usize,
    /// The name a `run` file was loaded by (issue #1043), so an error in
    /// it can be reported as `Line: 5 (inner.ps)`. A run file has its
    /// own line numbers, which is why `is_main` alone leaves it out;
    /// with its name attached, its lines are no longer ambiguous.
    run_name: Option<Rc<str>>,
    /// eexec pushed systemdict for this source's duration; pop it when
    /// the source is exhausted or closed.
    pub(crate) pop_systemdict: bool,
    /// The line the token most recently returned by `next_token`
    /// started on, captured *before* consuming its first byte (a token
    /// terminated by whitespace consumes that delimiter as part of the
    /// same call — see `eat_token_delimiter` — so reading the file's
    /// live line position afterward would often be off by one).
    token_start_line: Option<usize>,
    /// Whether a token starting with 128 to 131 is a binary object
    /// sequence. The interpreter clears it while the object format is 0
    /// (`0 setobjectformat`), when those bytes are ordinary name characters
    /// (#733).
    pub(crate) binary_tokens: bool,
    /// The interpreter's limits, which a token's buffer asks before it grows
    /// (#1209). A filter holds them itself; a byte source (the main program,
    /// an executable string, a file read whole) does not, so the interpreter
    /// hands it this handle. `None` outside an interpreter.
    limits: Option<Rc<DecodeBudget>>,
}

const fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' | b'\0')
}

const fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

const fn is_regular(b: u8) -> bool {
    !is_whitespace(b) && !is_delimiter(b)
}

fn syntax(msg: &str) -> PsError {
    PsError::Syntax(msg.to_string())
}

impl Lexer {
    /// Scan a byte buffer that is *not* a file (an executable string).
    pub fn new(src: Vec<u8>) -> Self {
        Lexer {
            file: PsFile::from_bytes(src),
            is_file: false,
            is_main: false,
            token_start_pos: 0,
            run_name: None,
            pop_systemdict: false,
            token_start_line: None,
            binary_tokens: true,
            limits: None,
        }
    }

    /// Scan the top-level program source (what `begin_source` runs).
    pub fn main_program(src: Vec<u8>) -> Self {
        Lexer {
            file: PsFile::from_bytes(src),
            is_file: true,
            is_main: true,
            token_start_pos: 0,
            run_name: None,
            pop_systemdict: false,
            token_start_line: None,
            binary_tokens: true,
            limits: None,
        }
    }

    /// Scan an existing file object in place (`run`, `exec` on a file,
    /// eexec) — never the main program, even though it's a file.
    pub(crate) fn from_file(file: FileHandle) -> Self {
        Lexer {
            file,
            is_file: true,
            is_main: false,
            token_start_pos: 0,
            run_name: None,
            pop_systemdict: false,
            token_start_line: None,
            binary_tokens: true,
            limits: None,
        }
    }

    /// Scan a file loaded by `run` under `name` (issue #1043): its lines
    /// are reported, tagged with the name.
    pub(crate) fn run_file(file: FileHandle, name: Rc<str>) -> Self {
        Lexer {
            run_name: Some(name),
            ..Lexer::from_file(file)
        }
    }

    /// Have this scanner's token buffers ask `limits` before they grow.
    pub(crate) fn with_limits(mut self, limits: Rc<DecodeBudget>) -> Self {
        self.limits = Some(limits);
        self
    }

    /// The name of the `run` file this scanner reads, or `None` for the
    /// main program and every unnamed source.
    pub(crate) fn run_name(&self) -> Option<&Rc<str>> {
        self.run_name.as_ref()
    }

    /// The handle `currentfile` returns, if this scanner is a file.
    pub(crate) fn file(&self) -> Option<&FileHandle> {
        self.is_file.then_some(&self.file)
    }

    /// Bytes consumed so far — the `token` operator's remainder split.
    pub(crate) fn pos(&self) -> usize {
        self.file.borrow().pos()
    }

    /// The line the last-returned token started on, if this scanner is
    /// the main program or a named `run` file — `None` otherwise (eexec
    /// streams, executable strings, a file object `exec`ed directly),
    /// so callers don't misattribute an error to the wrong source.
    pub(crate) fn line(&self) -> Option<usize> {
        (self.is_main || self.run_name.is_some())
            .then_some(self.token_start_line)
            .flatten()
    }

    /// The byte offset the last-returned token (or the failed scan) started
    /// at, past any leading whitespace and comments.
    pub(crate) fn token_start(&self) -> usize {
        self.token_start_pos
    }

    fn peek(&mut self) -> Result<Option<u8>, PsError> {
        self.file.borrow_mut().peek_byte()
    }

    fn bump(&mut self) -> Result<Option<u8>, PsError> {
        self.file.borrow_mut().read_byte()
    }

    pub fn next_token(&mut self) -> Result<Option<Token>, PsError> {
        self.skip_insignificant()?;
        // Captured before dispatch, whether or not dispatch errors, so a
        // syntax error gets the line it actually happened on rather than
        // the previous good token's line.
        self.token_start_line = Some(self.file.borrow().line());
        self.token_start_pos = self.file.borrow().pos();
        self.dispatch_token()
    }

    /// Consume whitespace and `%` comments up to the next significant
    /// byte (or EOF). Split out from `next_token` so the token's start
    /// line can be captured exactly once, right after this returns.
    fn skip_insignificant(&mut self) -> Result<(), PsError> {
        loop {
            match self.peek()? {
                Some(b) if is_whitespace(b) => {
                    self.bump()?;
                }
                Some(b'%') => {
                    self.bump()?;
                    self.skip_comment()?;
                }
                _ => return Ok(()),
            }
        }
    }

    /// `next_token` with binary object sequences switched on or off first
    /// (off while the object format is 0, #733).
    pub(crate) fn next_token_in(&mut self, binary: bool) -> Result<Option<Token>, PsError> {
        self.binary_tokens = binary;
        self.next_token()
    }

    fn dispatch_token(&mut self) -> Result<Option<Token>, PsError> {
        let Some(b) = self.peek()? else {
            return Ok(None);
        };
        // A token that starts with 128 to 131 is a binary object sequence.
        // (Inside a name such a byte is still an ordinary character.)
        if self.binary_tokens && (128..=131).contains(&b) {
            return Ok(Some(Token::BinarySeq(self.read_binary_sequence()?)));
        }
        // So is a token of type 149, a homogeneous number array (#1192),
        // under the same switch.
        if self.binary_tokens && b == crate::numstring::HOMOGENEOUS_ARRAY {
            return Ok(Some(Token::NumberArray(self.read_number_array()?)));
        }
        if is_regular(b) {
            // Number-or-name words dispatch on their whole run; the
            // first byte must not be consumed before reading it.
            let word = self.read_regular_run()?;
            self.eat_token_delimiter()?;
            return Ok(Some(self.classify_word(word)?));
        }
        self.bump()?;
        match b {
            b'(' => Ok(Some(Token::String(self.read_string()?))),
            b')' => Err(syntax("')' with no matching '('")),
            b'<' => match self.peek()? {
                Some(b'<') => {
                    self.bump()?;
                    Ok(Some(Token::Name("<<".to_string())))
                }
                Some(b'~') => {
                    self.bump()?;
                    Ok(Some(Token::String(self.read_ascii85_string()?)))
                }
                _ => Ok(Some(Token::String(self.read_hex_string()?))),
            },
            b'>' => match self.peek()? {
                Some(b'>') => {
                    self.bump()?;
                    Ok(Some(Token::Name(">>".to_string())))
                }
                _ => Err(syntax("'>' outside a hex string")),
            },
            b'[' | b']' => Ok(Some(Token::Name((b as char).to_string()))),
            b'{' => Ok(Some(Token::LBrace)),
            b'}' => Ok(Some(Token::RBrace)),
            b'/' => {
                if self.peek()? == Some(b'/') {
                    self.bump()?;
                    let word = self.read_regular_run()?;
                    self.eat_token_delimiter()?;
                    Ok(Some(Token::ImmediateName(self.name_text(word)?)))
                } else {
                    let word = self.read_regular_run()?;
                    self.eat_token_delimiter()?;
                    Ok(Some(Token::LiteralName(self.name_text(word)?)))
                }
            }
            // is_regular handled above; whitespace/comments skipped above.
            _ => unreachable!("delimiter byte {b} not dispatched"),
        }
    }

    /// Read one whole binary object sequence, header included. Nothing
    /// after it is consumed (no token delimiter), and nothing is
    /// preallocated from the length the header claims: a short input that
    /// claims 4 GB is a truncated sequence, not an allocation.
    fn read_binary_sequence(&mut self) -> Result<Vec<u8>, PsError> {
        let truncated = || syntax("binary object sequence is truncated");
        let mut bytes: Vec<u8> = Vec::with_capacity(8);
        let mut want = 4;
        while bytes.len() < want {
            bytes.push(self.bump()?.ok_or_else(truncated)?);
            if bytes.len() == 4 && crate::binobj::header_is_long(&bytes) {
                want = 8;
            }
        }
        let (_, total, _) = crate::binobj::parse_header(&bytes)?;
        // The header can claim 4 GB; the up-front reserve is capped at
        // 1 MiB, and is put to the memory cap before it is taken.
        let want = total.saturating_sub(bytes.len()).min(1 << 20);
        self.room(&bytes, want)?;
        bytes.reserve(want);
        while bytes.len() < total {
            let b = self.bump()?.ok_or_else(truncated)?;
            self.put(&mut bytes, b)?;
        }
        Ok(bytes)
    }

    /// Read one homogeneous number array token (#1192): the 4-byte header
    /// and exactly the numbers it counts, nothing after (a stray byte or a
    /// delimiter stays in the input). Truncation is a `syntaxerror`, and
    /// nothing is allocated from the count before the bytes are there.
    fn read_number_array(&mut self) -> Result<Vec<Token>, PsError> {
        let truncated = || syntax("homogeneous number array is truncated");
        let mut bytes: Vec<u8> = Vec::with_capacity(4);
        for _ in 0..4 {
            bytes.push(self.bump()?.ok_or_else(truncated)?);
        }
        let total = crate::numstring::token_len(&bytes)?;
        while bytes.len() < total {
            bytes.push(self.bump()?.ok_or_else(truncated)?);
        }
        crate::numstring::token_numbers(bytes)
    }

    /// A token terminated *by* whitespace consumes that one whitespace
    /// character (CRLF counting as one), per the PLRM. Programs never
    /// notice, but anything reading data from a shared file position
    /// right after a token does — `N RD <binary>` in Type 1 fonts most
    /// of all.
    fn eat_token_delimiter(&mut self) -> Result<(), PsError> {
        if let Some(b) = self.peek()?
            && is_whitespace(b)
        {
            self.bump()?;
            if b == b'\r' && self.peek()? == Some(b'\n') {
                self.bump()?;
            }
        }
        Ok(())
    }

    fn skip_comment(&mut self) -> Result<(), PsError> {
        while let Some(b) = self.peek()? {
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.bump()?;
        }
        Ok(())
    }

    /// Push onto a token's buffer, asking the memory cap first when the
    /// push would grow it (#1209). A filter's decoder looks at the heap only
    /// every 64 KiB it decodes, and a buffer that doubles can take far more
    /// than that at once, so a token read from a decompression bomb would
    /// pass the cap by up to the buffer's size. One compare per byte.
    #[inline(always)]
    fn put(&self, out: &mut Vec<u8>, b: u8) -> Result<(), PsError> {
        if out.len() == out.capacity() {
            self.check_growth(out, 1)?;
        }
        out.push(b);
        Ok(())
    }

    /// [`Self::put`] for `extra` bytes at once.
    #[inline(always)]
    fn room(&self, out: &Vec<u8>, extra: usize) -> Result<(), PsError> {
        if out.capacity() - out.len() < extra {
            self.check_growth(out, extra)?;
        }
        Ok(())
    }

    /// The growth `Vec` makes to take `extra` more bytes, put to the cap as
    /// `PsFile::read_into` does. Only a source that decodes (a filter made
    /// by an interpreter) has limits to ask; a byte buffer is already in
    /// memory and bounds its own tokens.
    #[cold]
    fn check_growth(&self, out: &Vec<u8>, extra: usize) -> Result<(), PsError> {
        let cap = out.capacity();
        let grown = cap
            .saturating_mul(2)
            .max(out.len().saturating_add(extra))
            .max(8);
        self.ask(grown - cap)
    }

    /// `MemoryExceeded` if `bytes` more heap would pass the cap.
    fn ask(&self, bytes: usize) -> Result<(), PsError> {
        match &self.limits {
            Some(limits) => limits.check_alloc(bytes),
            None => self.file.borrow().check_alloc(bytes),
        }
    }

    /// A name's text, made from its buffer without a copy when it is UTF-8
    /// (a name is whatever a program wrote, so not always). Anything over
    /// 64 KiB is first put to the cap for what comes next: the interpreter
    /// interns every name, which copies it, and the buffer lives until
    /// then (#1209). Invalid UTF-8 is repaired into a copy at most three
    /// times its size.
    fn name_text(&self, word: Vec<u8>) -> Result<String, PsError> {
        if word.len() >= 64 << 10 {
            self.ask(word.len())?;
        }
        match String::from_utf8(word) {
            Ok(s) => Ok(s),
            Err(e) => {
                let bytes = e.into_bytes();
                self.ask(bytes.len().saturating_mul(3))?;
                Ok(String::from_utf8_lossy(&bytes).into_owned())
            }
        }
    }

    /// A word that is a number, or else a name.
    fn classify_word(&self, word: Vec<u8>) -> Result<Token, PsError> {
        let s = self.name_text(word)?;
        Ok(parse_number(&s).unwrap_or(Token::Name(s)))
    }

    fn read_regular_run(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        while let Some(b) = self.peek()? {
            if !is_regular(b) {
                break;
            }
            self.put(&mut out, b)?;
            self.bump()?;
        }
        Ok(out)
    }

    fn read_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        let mut depth = 1usize;
        loop {
            let Some(b) = self.bump()? else {
                return Err(syntax("unterminated string"));
            };
            match b {
                b'\\' => {
                    let Some(e) = self.bump()? else {
                        return Err(syntax("unterminated string"));
                    };
                    match e {
                        b'n' => self.put(&mut out, b'\n')?,
                        b'r' => self.put(&mut out, b'\r')?,
                        b't' => self.put(&mut out, b'\t')?,
                        b'b' => self.put(&mut out, 0x08)?,
                        b'f' => self.put(&mut out, 0x0c)?,
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.peek()? {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.bump()?;
                                    }
                                    _ => break,
                                }
                            }
                            // Per PLRM, high-order overflow in \ddd is ignored.
                            self.put(&mut out, (v & 0xff) as u8)?;
                        }
                        // \<newline> is a line continuation: no character.
                        b'\n' => {}
                        b'\r' => {
                            if self.peek()? == Some(b'\n') {
                                self.bump()?;
                            }
                        }
                        // Includes \\, \(, \): the escaped character itself.
                        other => self.put(&mut out, other)?,
                    }
                }
                b'(' => {
                    depth += 1;
                    self.put(&mut out, b)?;
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(out);
                    }
                    self.put(&mut out, b)?;
                }
                // The PLRM normalizes CR and CRLF inside strings to LF.
                b'\r' => {
                    if self.peek()? == Some(b'\n') {
                        self.bump()?;
                    }
                    self.put(&mut out, b'\n')?;
                }
                _ => self.put(&mut out, b)?,
            }
        }
    }

    /// `<~ ... ~>` ASCII85: 5 chars ('!'..'u') encode 4 bytes; 'z' is a
    /// shorthand for four zero bytes; a final partial group of n chars
    /// yields n-1 bytes.
    fn read_ascii85_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        let mut group = [0u8; 5];
        let mut n = 0usize;
        loop {
            let Some(b) = self.bump()? else {
                return Err(syntax("unterminated ASCII85 string"));
            };
            match b {
                b'~' => {
                    if self.bump()? != Some(b'>') {
                        return Err(syntax("'~' without '>' in ASCII85 string"));
                    }
                    break;
                }
                _ if is_whitespace(b) => {}
                b'z' if n == 0 => {
                    self.room(&out, 4)?;
                    out.extend_from_slice(&[0, 0, 0, 0]);
                }
                b'!'..=b'u' => {
                    group[n] = b - b'!';
                    n += 1;
                    if n == 5 {
                        let v = group.iter().fold(0u32, |acc, d| {
                            acc.wrapping_mul(85).wrapping_add(u32::from(*d))
                        });
                        self.room(&out, 4)?;
                        out.extend_from_slice(&v.to_be_bytes());
                        n = 0;
                    }
                }
                _ => return Err(syntax("invalid character in ASCII85 string")),
            }
        }
        if n == 1 {
            return Err(syntax("truncated ASCII85 group"));
        }
        if n > 1 {
            // Pad with 'u' (84) and keep n-1 output bytes.
            for slot in group.iter_mut().skip(n) {
                *slot = 84;
            }
            let v = group.iter().fold(0u32, |acc, d| {
                acc.wrapping_mul(85).wrapping_add(u32::from(*d))
            });
            self.room(&out, n - 1)?;
            out.extend_from_slice(&v.to_be_bytes()[..n - 1]);
        }
        Ok(out)
    }

    /// `<...>` hex. Pairs go straight into the result, never through a
    /// buffer of nibbles (a byte each, twice the result's size, #1209).
    fn read_hex_string(&mut self) -> Result<Vec<u8>, PsError> {
        let mut out = Vec::new();
        let mut high: Option<u8> = None;
        loop {
            let Some(b) = self.bump()? else {
                return Err(syntax("unterminated hex string"));
            };
            match b {
                b'>' => break,
                _ if is_whitespace(b) => {}
                _ => match (b as char).to_digit(16) {
                    Some(v) => match high.take() {
                        Some(h) => self.put(&mut out, (h << 4) | v as u8)?,
                        None => high = Some(v as u8),
                    },
                    None => return Err(syntax("invalid character in hex string")),
                },
            }
        }
        // An odd final nibble is padded with zero, per the PLRM.
        if let Some(h) = high {
            self.put(&mut out, h << 4)?;
        }
        Ok(out)
    }
}

/// Also used by `cvi`/`cvr`, which accept full scanner number syntax.
pub(crate) fn parse_number(s: &str) -> Option<Token> {
    // Radix numbers: base#digits, base 2..=36. Anything malformed falls
    // through to being a name.
    if let Some((base, digits)) = s.split_once('#') {
        let base: u32 = base.parse().ok()?;
        if !(2..=36).contains(&base)
            || digits.is_empty()
            || !digits.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return None;
        }
        return i64::from_str_radix(digits, base).ok().map(Token::Integer);
    }

    let bytes = s.as_bytes();
    if !bytes.iter().any(u8::is_ascii_digit) {
        return None;
    }
    if bytes
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-'))
    {
        if let Ok(i) = s.parse::<i64>() {
            return Some(Token::Integer(i));
        }
        // Integer literals too large for the integer type become reals —
        // that's the PLRM scanner's rule, not an invention.
        return s
            .parse::<f64>()
            .ok()
            .filter(|r| r.is_finite())
            .map(Token::Real);
    }
    // The charset pre-check keeps f64::from_str from accepting things
    // PostScript doesn't consider numbers ("inf", "NaN").
    if bytes
        .iter()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'))
    {
        return s
            .parse::<f64>()
            .ok()
            .filter(|r| r.is_finite())
            .map(Token::Real);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        let mut lx = Lexer::new(src.as_bytes().to_vec());
        let mut out = Vec::new();
        while let Some(t) = lx.next_token().expect("unexpected lex error") {
            out.push(t);
        }
        out
    }

    fn lex_err(src: &str) -> PsError {
        let mut lx = Lexer::new(src.as_bytes().to_vec());
        loop {
            match lx.next_token() {
                Ok(Some(_)) => {}
                Ok(None) => panic!("expected a lex error in {src:?}"),
                Err(e) => return e,
            }
        }
    }

    #[test]
    fn integers() {
        assert_eq!(
            lex("1 +2 -3 0"),
            vec![
                Token::Integer(1),
                Token::Integer(2),
                Token::Integer(-3),
                Token::Integer(0)
            ]
        );
    }

    #[test]
    fn reals() {
        assert_eq!(
            lex("1.5 -.5 6. 1e3 1E-3 2.5e2"),
            vec![
                Token::Real(1.5),
                Token::Real(-0.5),
                Token::Real(6.0),
                Token::Real(1000.0),
                Token::Real(0.001),
                Token::Real(250.0)
            ]
        );
    }

    #[test]
    fn huge_integer_literal_becomes_real() {
        assert_eq!(
            lex("9223372036854775808"),
            vec![Token::Real(9.223372036854776e18)]
        );
    }

    #[test]
    fn radix_numbers() {
        assert_eq!(
            lex("16#ff 8#777 2#1010 36#z"),
            vec![
                Token::Integer(255),
                Token::Integer(511),
                Token::Integer(10),
                Token::Integer(35)
            ]
        );
    }

    #[test]
    fn number_lookalikes_are_names() {
        for src in [
            "1.2.3", "1e", "16#zz", "16#", "1#0", "-", "+", "1-2", "inf", "NaN",
        ] {
            assert_eq!(
                lex(src),
                vec![Token::Name(src.to_string())],
                "input {src:?}"
            );
        }
    }

    #[test]
    fn literal_names() {
        assert_eq!(
            lex("/foo /"),
            vec![
                Token::LiteralName("foo".to_string()),
                Token::LiteralName(String::new())
            ]
        );
    }

    #[test]
    fn self_delimiting_tokens() {
        assert_eq!(
            lex("1[2]3"),
            vec![
                Token::Integer(1),
                Token::Name("[".to_string()),
                Token::Integer(2),
                Token::Name("]".to_string()),
                Token::Integer(3)
            ]
        );
        assert_eq!(
            lex("10{dup}"),
            vec![
                Token::Integer(10),
                Token::LBrace,
                Token::Name("dup".to_string()),
                Token::RBrace
            ]
        );
        assert_eq!(
            lex("<< >>"),
            vec![Token::Name("<<".to_string()), Token::Name(">>".to_string())]
        );
    }

    #[test]
    fn comments() {
        assert_eq!(
            lex("1 % junk ( { \n2"),
            vec![Token::Integer(1), Token::Integer(2)]
        );
        assert_eq!(lex("%!PS-Adobe-3.0\n42"), vec![Token::Integer(42)]);
    }

    #[test]
    fn strings() {
        assert_eq!(lex("(hello)"), vec![Token::String(b"hello".to_vec())]);
        assert_eq!(lex("(a(b)c)"), vec![Token::String(b"a(b)c".to_vec())]);
        assert_eq!(lex(r"(a\)b)"), vec![Token::String(b"a)b".to_vec())]);
        assert_eq!(lex(r"(a\nb)"), vec![Token::String(b"a\nb".to_vec())]);
        assert_eq!(lex(r"(\101\12\5)"), vec![Token::String(vec![65, 10, 5])]);
        assert_eq!(lex(r"(\q)"), vec![Token::String(b"q".to_vec())]);
        // Backslash-newline is a line continuation.
        assert_eq!(lex("(a\\\nb)"), vec![Token::String(b"ab".to_vec())]);
        // Raw CR and CRLF normalize to LF.
        assert_eq!(lex("(a\r\nb)"), vec![Token::String(b"a\nb".to_vec())]);
    }

    #[test]
    fn hex_strings() {
        assert_eq!(lex("<48656c6c6f>"), vec![Token::String(b"Hello".to_vec())]);
        assert_eq!(lex("<48 65>"), vec![Token::String(b"He".to_vec())]);
        assert_eq!(lex("<7>"), vec![Token::String(vec![0x70])]);
        // A pair may span whitespace, and an odd final nibble is padded.
        assert_eq!(lex("<4 8\n6>"), vec![Token::String(vec![0x48, 0x60])]);
        assert_eq!(lex("<>"), vec![Token::String(Vec::new())]);
        // Past the buffer's first capacities (8, 16, ...).
        let long = "ab".repeat(100);
        assert_eq!(
            lex(&format!("<{long}>")),
            vec![Token::String(vec![0xab; 100])]
        );
    }

    #[test]
    fn names_keep_their_text_and_numbers_stay_numbers() {
        assert_eq!(
            lex("abc /abc 12 1e3 16#ff 12abc"),
            vec![
                Token::Name("abc".into()),
                Token::LiteralName("abc".into()),
                Token::Integer(12),
                Token::Real(1000.0),
                Token::Integer(255),
                Token::Name("12abc".into()),
            ]
        );
        // Invalid UTF-8 in a name becomes the replacement character, in a
        // plain, a literal and an immediate name alike.
        let mut lexer = Lexer::new(b"a\xffb /a\xffb //a\xffb".to_vec());
        for want in [
            Token::Name("a\u{fffd}b".into()),
            Token::LiteralName("a\u{fffd}b".into()),
            Token::ImmediateName("a\u{fffd}b".into()),
        ] {
            assert_eq!(lexer.next_token().unwrap(), Some(want));
        }
        // A name past the size that is put to the cap (64 KiB) is the same.
        let long = "n".repeat(100_000);
        assert_eq!(lex(&long), vec![Token::Name(long.clone())]);
    }

    #[test]
    fn syntax_errors() {
        for src in ["(abc", ")", ">", "<zz>", "<12"] {
            assert!(matches!(lex_err(src), PsError::Syntax(_)), "input {src:?}");
        }
    }

    #[test]
    fn immediate_names() {
        assert_eq!(lex("//foo"), vec![Token::ImmediateName("foo".to_string())]);
        assert_eq!(
            lex("/a//b"),
            vec![
                Token::LiteralName("a".to_string()),
                Token::ImmediateName("b".to_string())
            ]
        );
    }

    #[test]
    fn ascii85_strings() {
        // "BOu!r" is the canonical encoding of "hell".
        assert_eq!(lex("<~BOu!r~>"), vec![Token::String(b"hell".to_vec())]);
        // Partial final group: 2 chars -> 1 byte.
        assert_eq!(lex("<~@/~>"), vec![Token::String(b"a".to_vec())]);
        // 'z' is four zero bytes; whitespace is ignored.
        assert_eq!(lex("<~ z ~>"), vec![Token::String(vec![0, 0, 0, 0])]);
        assert_eq!(lex("<~~>"), vec![Token::String(Vec::new())]);
        for src in ["<~BOu!r", "<~v~>", "<~!~>"] {
            assert!(matches!(lex_err(src), PsError::Syntax(_)), "input {src:?}");
        }
    }
}
