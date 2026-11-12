//! `pscat::syntax::check_syntax` (#1096) against the loop pscat-mcp wrote on
//! the hidden `pscat::lexer` before this API existed. A host that swaps one
//! for the other must see the same ok flag, token count and message.

use pscat::lexer::{Lexer, Token};
use pscat::syntax::{SyntaxErrorKind, check_syntax};

/// pscat-mcp's `syntax_scan`, verbatim in behaviour: (ok, tokens, message).
fn reference(source: &[u8]) -> (bool, usize, Option<String>) {
    let mut lexer = Lexer::new(source.to_vec());
    let (mut tokens, mut depth) = (0usize, 0usize);
    loop {
        match lexer.next_token() {
            Ok(Some(Token::LBrace)) => {
                tokens += 1;
                depth += 1;
            }
            Ok(Some(Token::RBrace)) => {
                tokens += 1;
                if depth == 0 {
                    return (
                        false,
                        tokens,
                        Some(format!("unmatched '}}' (token {tokens})")),
                    );
                }
                depth -= 1;
            }
            Ok(Some(_)) => tokens += 1,
            Ok(None) => {
                let error = (depth > 0).then(|| {
                    format!("unterminated procedure: {depth} '{{' without a matching '}}'")
                });
                return (error.is_none(), tokens, error);
            }
            Err(e) => return (false, tokens, Some(e.to_string())),
        }
    }
}

#[test]
fn matches_the_hidden_lexer_loop_on_a_corpus() {
    let corpus: &[&[u8]] = &[
        b"",
        b"   % nothing\n",
        b"1 2 add",
        b"/f { 1 { 2 } if } def f",
        b"(a (nested) string) pop",
        b"<48 65 6c6c6f> pop <~87cURD]j7BEbo80~> pop",
        b"/caf\xE9 1",
        b"(unterminated",
        b"<4865",
        b"<~87cURD",
        b"1 2 )",
        b"1 >",
        b"<zz>",
        b"}",
        b"1 2 } 3",
        b"{ 1",
        b"{ { { 1 }",
        b"{ 1 } }",
        b"/s (}{) def { (}) pop",
    ];
    for src in corpus {
        let (ok, tokens, message) = reference(src);
        match check_syntax(src) {
            Ok(summary) => {
                assert!(ok, "{src:?}: reference rejected, check accepted");
                assert_eq!(summary.tokens, tokens, "{src:?}");
            }
            Err(e) => {
                assert!(!ok, "{src:?}: reference accepted, check rejected");
                assert_eq!(e.tokens, tokens, "{src:?}");
                assert_eq!(Some(e.message.clone()), message, "{src:?}");
                assert!(e.line >= 1, "{src:?}");
                assert!(e.offset <= src.len(), "{src:?}");
            }
        }
    }
}

#[test]
fn error_kinds_and_positions() {
    let e = check_syntax(b"(abc").unwrap_err();
    assert_eq!(e.kind, SyntaxErrorKind::Lexical);
    assert_eq!((e.offset, e.line), (0, 1));
    let e = check_syntax(b"\n\n  }").unwrap_err();
    assert_eq!(e.kind, SyntaxErrorKind::UnmatchedCloseBrace);
    assert_eq!((e.offset, e.line), (4, 3));
    let e = check_syntax(b"1\n{ 2").unwrap_err();
    assert_eq!(e.kind, SyntaxErrorKind::UnterminatedProcedure);
    assert_eq!((e.offset, e.line), (2, 2));
}

#[test]
fn a_long_program_is_checked_without_running_it() {
    // Would never finish if it ran: the check only scans.
    let src = b"{ 1 } loop { 2 } loop";
    assert_eq!(check_syntax(src).unwrap().tokens, 8);
}
