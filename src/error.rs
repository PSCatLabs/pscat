//! The error type: [`PsError`].

use thiserror::Error;

/// A PostScript error, named after the PLRM's standard error names.
///
/// A program can catch any of these itself with `stopped`; one that reaches
/// the top level comes back from [`Interp::run_str`](crate::Interp::run_str)
/// and friends. `Display` prints the PLRM name, plus the detail for
/// [`Undefined`](Self::Undefined) and [`Syntax`](Self::Syntax);
/// [`Interp::error_report`](crate::Interp::error_report) gives the full
/// Ghostscript-style report.
///
/// New variants may be added as coverage of the PLRM grows, so the enum is
/// `#[non_exhaustive]`: match on [`name`](Self::name) or keep a `_` arm.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum PsError {
    /// An operator needed more operands than the stack held (`stackunderflow`).
    #[error("stackunderflow")]
    StackUnderflow,
    /// Procedure calls nested too deeply, usually runaway recursion (`execstackoverflow`).
    #[error("execstackoverflow")]
    ExecStackOverflow,
    /// An operand had the wrong type (`typecheck`).
    #[error("typecheck")]
    Typecheck,
    /// An operand was out of range (`rangecheck`).
    #[error("rangecheck")]
    Rangecheck,
    /// A name was not found in any dictionary on the dictionary stack (`undefined`); holds the name.
    #[error("undefined in {0}")]
    Undefined(String),
    /// A numeric result could not be represented, such as division by zero (`undefinedresult`).
    #[error("undefinedresult")]
    UndefinedResult,
    /// `counttomark`, `]` or `>>` found no mark on the stack (`unmatchedmark`).
    #[error("unmatchedmark")]
    UnmatchedMark,
    /// A path operator needed a current point and there was none (`nocurrentpoint`).
    #[error("nocurrentpoint")]
    NoCurrentPoint,
    /// `exit` was used outside a loop (`invalidexit`).
    #[error("invalidexit")]
    InvalidExit,
    /// `end` would have popped a permanent dictionary (`dictstackunderflow`).
    #[error("dictstackunderflow")]
    DictStackUnderflow,
    /// A font dictionary was malformed or a font operation was invalid (`invalidfont`).
    #[error("invalidfont")]
    InvalidFont,
    /// A file was opened with a mode that is not allowed (`invalidfileaccess`).
    #[error("invalidfileaccess")]
    InvalidFileAccess,
    /// A file named by `run` or `file` was not found (`undefinedfilename`).
    #[error("undefinedfilename")]
    UndefinedFilename,
    /// The scanner could not read the program text (`syntaxerror`); holds a detail message.
    #[error("syntaxerror: {0}")]
    Syntax(String),
    /// An implementation limit was exceeded (`limitcheck`).
    #[error("limitcheck")]
    Limitcheck,
    /// An input/output operation failed (`ioerror`).
    #[error("ioerror")]
    Io,
    /// `restore` was given a save object it cannot restore to (`invalidrestore`).
    #[error("invalidrestore")]
    InvalidRestore,
    /// `findresource` found no resource with that name (`undefinedresource`).
    #[error("undefinedresource")]
    UndefinedResource,
    /// The program passed one of its [`Limits`](crate::Limits): too many
    /// steps, too long, or too much memory (`limitexceeded`). Unlike every
    /// error above, a program **cannot catch this one**: `stopped` does not
    /// see it, so a runaway program cannot loop around the limit. It is also
    /// never written to `$error`.
    #[error("limitexceeded: {0}")]
    LimitExceeded(crate::Limit),
}

impl PsError {
    /// The bare PLRM error name, without any payload detail — what a real
    /// interpreter would report as the error's name.
    pub fn name(&self) -> &'static str {
        match self {
            PsError::StackUnderflow => "stackunderflow",
            PsError::ExecStackOverflow => "execstackoverflow",
            PsError::Typecheck => "typecheck",
            PsError::Rangecheck => "rangecheck",
            PsError::Undefined(_) => "undefined",
            PsError::UndefinedResult => "undefinedresult",
            PsError::UnmatchedMark => "unmatchedmark",
            PsError::NoCurrentPoint => "nocurrentpoint",
            PsError::InvalidExit => "invalidexit",
            PsError::DictStackUnderflow => "dictstackunderflow",
            PsError::InvalidFont => "invalidfont",
            PsError::InvalidFileAccess => "invalidfileaccess",
            PsError::UndefinedFilename => "undefinedfilename",
            PsError::Syntax(_) => "syntaxerror",
            PsError::Limitcheck => "limitcheck",
            PsError::Io => "ioerror",
            PsError::InvalidRestore => "invalidrestore",
            PsError::UndefinedResource => "undefinedresource",
            PsError::LimitExceeded(_) => "limitexceeded",
        }
    }

    /// The inverse of [`PsError::name`], for rebuilding the error a
    /// `stopped` recorded in `$error` (issue #142).
    ///
    /// `$error` is the authority rather than any Rust-side cache,
    /// because it is a VM dict: a `restore` rolls it back like anything
    /// else, and can leave it naming an *earlier* error than the one
    /// last caught. `command` supplies `undefined`'s payload, which
    /// `$error` keeps in its own `/command` entry. `syntaxerror`'s
    /// detail has nowhere to live in `$error`, so it comes back empty —
    /// callers that still hold the original should prefer it.
    #[doc(hidden)]
    pub fn from_name(name: &str, command: Option<String>) -> Option<PsError> {
        Some(match name {
            "stackunderflow" => PsError::StackUnderflow,
            "execstackoverflow" => PsError::ExecStackOverflow,
            "typecheck" => PsError::Typecheck,
            "rangecheck" => PsError::Rangecheck,
            "undefined" => PsError::Undefined(command.unwrap_or_default()),
            "undefinedresult" => PsError::UndefinedResult,
            "unmatchedmark" => PsError::UnmatchedMark,
            "nocurrentpoint" => PsError::NoCurrentPoint,
            "invalidexit" => PsError::InvalidExit,
            "dictstackunderflow" => PsError::DictStackUnderflow,
            "invalidfont" => PsError::InvalidFont,
            "invalidfileaccess" => PsError::InvalidFileAccess,
            "undefinedfilename" => PsError::UndefinedFilename,
            "syntaxerror" => PsError::Syntax(String::new()),
            "limitcheck" => PsError::Limitcheck,
            "ioerror" => PsError::Io,
            "invalidrestore" => PsError::InvalidRestore,
            "undefinedresource" => PsError::UndefinedResource,
            // Not here: `limitexceeded` is never caught, so `$error` never
            // holds it and there is nothing to rebuild.
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PsError;

    /// A sample of every variant, written as an exhaustive `match` so
    /// that adding a `PsError` variant fails to *compile* until it is
    /// listed here. A hand-written array would silently omit it, and
    /// `from_name`'s `_ => None` would then make a top-level `stop` go
    /// quiet for that error kind — the exact regression the round-trip
    /// test below exists to prevent (review of PR #138).
    fn sample_of(e: &PsError) -> PsError {
        match e {
            PsError::StackUnderflow => PsError::StackUnderflow,
            PsError::ExecStackOverflow => PsError::ExecStackOverflow,
            PsError::Typecheck => PsError::Typecheck,
            PsError::Rangecheck => PsError::Rangecheck,
            PsError::Undefined(n) => PsError::Undefined(n.clone()),
            PsError::UndefinedResult => PsError::UndefinedResult,
            PsError::UnmatchedMark => PsError::UnmatchedMark,
            PsError::NoCurrentPoint => PsError::NoCurrentPoint,
            PsError::InvalidExit => PsError::InvalidExit,
            PsError::DictStackUnderflow => PsError::DictStackUnderflow,
            PsError::InvalidFont => PsError::InvalidFont,
            PsError::InvalidFileAccess => PsError::InvalidFileAccess,
            PsError::UndefinedFilename => PsError::UndefinedFilename,
            PsError::Syntax(d) => PsError::Syntax(d.clone()),
            PsError::Limitcheck => PsError::Limitcheck,
            PsError::Io => PsError::Io,
            PsError::InvalidRestore => PsError::InvalidRestore,
            PsError::UndefinedResource => PsError::UndefinedResource,
            PsError::LimitExceeded(l) => PsError::LimitExceeded(*l),
        }
    }

    /// Every variant must survive the round trip, or a `stop` re-raise
    /// would silently drop an error kind (issue #142).
    #[test]
    fn every_error_name_rebuilds_into_its_own_variant() {
        for e in [
            PsError::StackUnderflow,
            PsError::ExecStackOverflow,
            PsError::Typecheck,
            PsError::Rangecheck,
            PsError::Undefined("thing".to_string()),
            PsError::UndefinedResult,
            PsError::UnmatchedMark,
            PsError::NoCurrentPoint,
            PsError::InvalidExit,
            PsError::DictStackUnderflow,
            PsError::InvalidFont,
            PsError::InvalidFileAccess,
            PsError::UndefinedFilename,
            PsError::Syntax(String::new()),
            PsError::Limitcheck,
            PsError::Io,
            PsError::InvalidRestore,
            PsError::UndefinedResource,
        ] {
            // Routing through the exhaustive match is what ties this
            // list to the enum: a new variant stops it compiling.
            let e = sample_of(&e);
            let command = match &e {
                PsError::Undefined(n) => Some(n.clone()),
                _ => None,
            };
            assert_eq!(
                PsError::from_name(e.name(), command).as_ref(),
                Some(&e),
                "{} did not round-trip through its own name",
                e.name()
            );
        }
        assert_eq!(PsError::from_name("notanerror", None), None);
    }

    /// A limit error (issue #604) is not a catchable error: `stopped` never
    /// records it, so `$error` never names it and there is nothing to
    /// rebuild or re-raise.
    #[test]
    fn a_limit_error_has_a_name_but_no_errordict_handler() {
        let e = PsError::LimitExceeded(crate::Limit::Memory);
        assert_eq!(e.name(), "limitexceeded");
        assert_eq!(e.to_string(), "limitexceeded: memory");
        assert_eq!(PsError::from_name("limitexceeded", None), None);
        assert!(sample_of(&e) == e);
        let mut errordict = crate::object::Dict::new();
        crate::ops::misc::install_errordict(&mut errordict);
        assert!(errordict.get("limitexceeded").is_none());
    }

    /// `errordict` has a handler for every error the interpreter can
    /// raise (issue #372), or `errordict /name get exec` would be an
    /// `undefined` error again for that name. Same list as above, so a
    /// new variant without a handler fails here.
    #[test]
    fn every_error_name_has_an_errordict_handler() {
        let mut errordict = crate::object::Dict::new();
        crate::ops::misc::install_errordict(&mut errordict);
        for e in [
            PsError::StackUnderflow,
            PsError::ExecStackOverflow,
            PsError::Typecheck,
            PsError::Rangecheck,
            PsError::Undefined("thing".to_string()),
            PsError::UndefinedResult,
            PsError::UnmatchedMark,
            PsError::NoCurrentPoint,
            PsError::InvalidExit,
            PsError::DictStackUnderflow,
            PsError::InvalidFont,
            PsError::InvalidFileAccess,
            PsError::UndefinedFilename,
            PsError::Syntax(String::new()),
            PsError::Limitcheck,
            PsError::Io,
            PsError::InvalidRestore,
            PsError::UndefinedResource,
        ] {
            let e = sample_of(&e);
            assert!(
                errordict.get(e.name()).is_some(),
                "errordict has no handler for {}",
                e.name()
            );
        }
    }
}
