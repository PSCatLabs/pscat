//! A syntax check that runs nothing: [`check_syntax`].
//!
//! Tools that accept PostScript from somewhere else (an editor, a service, an
//! agent) often want to say "this will not even scan" before they hand the
//! program to an [`Interp`](crate::Interp). [`check_syntax`] tokenizes a
//! program with the same scanner the interpreter uses, counts the tokens, and
//! reports the first thing wrong with where it is, without executing a single
//! operator, so it cannot draw, read a file or run forever.
//!
//! It finds what the scanner can see:
//!
//! - an unterminated `(string)`, `<hex string>` or `<~ASCII85~>`;
//! - a stray `)` or a `>` outside a hex string;
//! - an invalid character in a hex or ASCII85 string;
//! - a `}` with no `{` before it, and a `{` that is never closed (the scanner
//!   reports `{` and `}` as separate tokens and leaves balancing to the
//!   interpreter, so this function balances them).
//!
//! It does not find a misspelt operator, a stack underflow or a type error:
//! those exist only when the program runs.
//!
//! # Limits of a check that runs nothing
//!
//! A program can read its own text as data: `currentfile eexec`, an inline
//! `image` whose samples follow the operator, `currentfile readhexstring`. The
//! scanner cannot know where such data starts, so it tokenizes those bytes as
//! if they were program text and may report a syntax error inside them that
//! the real run would never hit. Treat a failure as "the scanner cannot read
//! this as written", which for ordinary programs is a mistake and for programs
//! with embedded data is a prompt to look closer. The check scans the way a
//! fresh interpreter does: once the scanner reads binary object sequences
//! (token types 128 to 131, #733), one counts as a single token and a
//! truncated one is reported as a [`Lexical`](SyntaxErrorKind::Lexical)
//! error. A program that switches that scanning off with `0 setobjectformat`
//! is not modelled, since nothing runs.
//!
//! # Example
//!
//! ```
//! use pscat::syntax::{check_syntax, SyntaxErrorKind};
//!
//! let ok = check_syntax(b"/box { 0 0 10 10 rectfill } def box").unwrap();
//! assert_eq!(ok.tokens, 10);
//!
//! // The missing `}` is reported where the procedure opened.
//! let err = check_syntax(b"1 2 add\n/f { 3 4 mul\n").unwrap_err();
//! assert_eq!(err.kind, SyntaxErrorKind::UnterminatedProcedure);
//! assert_eq!((err.line, err.offset), (2, 11));
//! assert_eq!(err.tokens, 8);
//! ```

use std::fmt;

use crate::error::PsError;
use crate::lexer::{Lexer, Token};

/// What [`check_syntax`] found in a program that scans cleanly.
///
/// New fields may be added in a minor release, so the struct is
/// `#[non_exhaustive]`: read the fields, do not construct one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyntaxSummary {
    /// How many tokens the program holds. Numbers, names, strings, `{` and `}`
    /// each count as one; comments and whitespace do not.
    pub tokens: usize,
}

/// Which kind of problem a [`SyntaxError`] is.
///
/// More kinds may be added in a minor release, so the enum is
/// `#[non_exhaustive]`: keep a `_` arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SyntaxErrorKind {
    /// The scanner rejected the text: an unterminated string, a stray `)`,
    /// an invalid character in a hex string, and so on. The
    /// [`message`](SyntaxError::message) says which.
    Lexical,
    /// A `}` with no open `{`.
    UnmatchedCloseBrace,
    /// One or more `{` still open at the end of the program.
    UnterminatedProcedure,
}

/// The first syntax error in a program, from [`check_syntax`].
///
/// New fields may be added in a minor release, so the struct is
/// `#[non_exhaustive]`: read the fields, do not construct one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SyntaxError {
    /// The class of problem; see [`SyntaxErrorKind`].
    pub kind: SyntaxErrorKind,
    /// A one-line description for a person, for example
    /// `syntaxerror: unterminated string` or
    /// `unmatched '}' (token 4)`. The wording is for reading and can change
    /// in any release; branch on [`kind`](Self::kind) instead.
    pub message: String,
    /// Byte offset (from 0) where the offending token starts: the `(` of an
    /// unterminated string, the stray `}`, or, for an unterminated procedure,
    /// the outermost `{` that was never closed.
    pub offset: usize,
    /// The 1-based line that `offset` is on. A CR, an LF or a CR LF pair each
    /// end one line, as in PostScript.
    pub line: usize,
    /// How many tokens were read before the error (the offending token is
    /// not counted unless it was a stray `}`, which is a token that scanned).
    pub tokens: usize,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (line {}, byte {})",
            self.message, self.line, self.offset
        )
    }
}

impl std::error::Error for SyntaxError {}

/// Tokenize `source` without executing it, and check that its `{`/`}` balance.
///
/// Returns the token count for a program that scans, or the first
/// [`SyntaxError`] with its position. `source` is raw bytes, not a `str`,
/// because PostScript is a byte language and real programs are not always
/// UTF-8. An empty program is fine and holds 0 tokens. See the
/// [module docs](self) for what this can and cannot find.
///
/// Cost is linear in the size of `source`; nothing is executed, nothing is
/// read from disk and no interpreter is created.
pub fn check_syntax(source: &[u8]) -> Result<SyntaxSummary, SyntaxError> {
    // `main_program` rather than `Lexer::new`: only the main program's
    // scanner reports source lines.
    let mut lexer = Lexer::main_program(source.to_vec());
    let mut tokens = 0usize;
    let mut depth = 0usize;
    // Where the outermost open `{` sits: the useful place to point at when
    // the program ends with braces still open.
    let mut outer_open = (0usize, 1usize);
    loop {
        match lexer.next_token() {
            Ok(Some(token)) => {
                tokens += 1;
                // Only braces matter here. A catch-all arm keeps this correct
                // as the scanner grows new token kinds (binary object
                // sequences, #733): their bytes can hold a 0x7B or 0x7D that
                // is not a brace.
                match token {
                    Token::LBrace => {
                        if depth == 0 {
                            outer_open = (lexer.token_start(), line_of(&lexer));
                        }
                        depth += 1;
                    }
                    Token::RBrace => {
                        if depth == 0 {
                            return Err(SyntaxError {
                                kind: SyntaxErrorKind::UnmatchedCloseBrace,
                                message: format!("unmatched '}}' (token {tokens})"),
                                offset: lexer.token_start(),
                                line: line_of(&lexer),
                                tokens,
                            });
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            Ok(None) => {
                if depth == 0 {
                    return Ok(SyntaxSummary { tokens });
                }
                return Err(SyntaxError {
                    kind: SyntaxErrorKind::UnterminatedProcedure,
                    message: format!(
                        "unterminated procedure: {depth} '{{' without a matching '}}'"
                    ),
                    offset: outer_open.0,
                    line: outer_open.1,
                    tokens,
                });
            }
            Err(e) => return Err(lexical(&lexer, e, tokens)),
        }
    }
}

/// The 1-based line the scanner's last token started on.
fn line_of(lexer: &Lexer) -> usize {
    // `main_program` always has one after a token or a failed scan; 1 is the
    // answer for a scan that failed before any token.
    lexer.line().unwrap_or(1)
}

fn lexical(lexer: &Lexer, e: PsError, tokens: usize) -> SyntaxError {
    SyntaxError {
        kind: SyntaxErrorKind::Lexical,
        message: e.to_string(),
        offset: lexer.token_start(),
        line: line_of(lexer),
        tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(src: &str) -> SyntaxError {
        check_syntax(src.as_bytes()).unwrap_err()
    }

    #[test]
    fn counts_tokens_and_ignores_comments() {
        assert_eq!(check_syntax(b"").unwrap().tokens, 0);
        assert_eq!(check_syntax(b"% only a comment\n  \n").unwrap().tokens, 0);
        // braces, names, numbers and strings each count once
        let s = check_syntax(b"1 2.5 add (hi) <6869> /n { dup } def % tail").unwrap();
        assert_eq!(s.tokens, 10);
    }

    #[test]
    fn accepts_nested_procedures_and_delimiter_strings() {
        let src = b"/f { { 1 } { (}) pop (a\\)b) pop } ifelse } def << /k [ 1 2 ] >> pop";
        assert!(check_syntax(src).is_ok());
    }

    #[test]
    fn braces_inside_strings_and_comments_do_not_count() {
        assert!(check_syntax(b"(}) pop % {\n(\\() pop").is_ok());
    }

    #[test]
    fn unterminated_string_points_at_the_opening_paren() {
        let e = err("1 2 add\n(never closed\nmore");
        assert_eq!(e.kind, SyntaxErrorKind::Lexical);
        assert!(e.message.contains("unterminated string"), "{}", e.message);
        assert_eq!((e.offset, e.line, e.tokens), (8, 2, 3));
    }

    #[test]
    fn unterminated_hex_string() {
        let e = err("<6869");
        assert_eq!(e.kind, SyntaxErrorKind::Lexical);
        assert!(e.message.contains("unterminated hex string"));
        assert_eq!((e.offset, e.line, e.tokens), (0, 1, 0));
    }

    #[test]
    fn stray_close_paren_and_angle() {
        let e = err("1 )");
        assert_eq!(
            (e.kind, e.offset, e.tokens),
            (SyntaxErrorKind::Lexical, 2, 1)
        );
        assert!(e.message.contains("')'"));
        let e = err("1\n 2 >");
        assert_eq!((e.offset, e.line), (5, 2));
    }

    #[test]
    fn unmatched_close_brace() {
        let e = err("1 2\n} 3");
        assert_eq!(e.kind, SyntaxErrorKind::UnmatchedCloseBrace);
        assert_eq!(e.message, "unmatched '}' (token 3)");
        assert_eq!((e.offset, e.line, e.tokens), (4, 2, 3));
    }

    #[test]
    fn unterminated_procedure_points_at_the_outermost_open_brace() {
        let e = err("{ 1\n  { 2 }\n  { 3");
        assert_eq!(e.kind, SyntaxErrorKind::UnterminatedProcedure);
        assert_eq!(
            e.message,
            "unterminated procedure: 2 '{' without a matching '}'"
        );
        assert_eq!((e.offset, e.line), (0, 1));
        // after the outer procedure closed and a new one opened
        let e = err("{ 1 }\n/g {");
        assert_eq!((e.offset, e.line), (9, 2));
    }

    #[test]
    fn lines_follow_postscript_line_endings() {
        // CR, LF and CR LF each end one line.
        let e = err("a\rb\r\nc\n(open");
        assert_eq!((e.line, e.offset), (4, 7));
    }

    #[test]
    fn non_utf8_bytes_are_fine() {
        // 0xE9 is an ordinary name character (not 128 to 131, which start a
        // binary object sequence once those are scanned).
        assert_eq!(check_syntax(b"/caf\xE9 1").unwrap().tokens, 2);
    }

    #[test]
    fn error_displays_with_position() {
        let e = err("}");
        assert_eq!(e.to_string(), "unmatched '}' (token 1) (line 1, byte 0)");
    }

    #[test]
    fn agrees_with_the_interpreter_on_what_scans() {
        // Whatever the check accepts, a real run must not reject for syntax.
        for src in [
            "1 2 add pop",
            "/f { 1 } def f pop",
            "(a(b)c) pop",
            "<41> pop",
        ] {
            assert!(check_syntax(src.as_bytes()).is_ok(), "{src}");
            assert!(crate::Interp::new().run_str(src).is_ok(), "{src}");
        }
        for src in ["(open", "}", "{ 1", "1 ) pop"] {
            let rejected = check_syntax(src.as_bytes()).is_err();
            let run = crate::Interp::new().run_str(src);
            assert!(rejected, "{src}");
            assert!(matches!(run, Err(PsError::Syntax(_))), "{src}: {run:?}");
        }
    }
}
