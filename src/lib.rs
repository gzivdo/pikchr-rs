//! `pikchr-rs` — a pure-Rust port of the Pikchr (PIC-like) diagram language.
//!
//! Source of truth: the upstream Lemon grammar `pikchr.y` (D. R. Hipp, 0BSD).
//! No C code is used or linked. See `NOTICE`.
//!
//! Public entry point: [`pikchr`].
#![forbid(unsafe_code)]

use lalrpop_util::lalrpop_mod;

pub mod keywords;
pub mod lexer;
pub mod obj;
pub mod pik;
pub mod token;
pub mod value;

lalrpop_mod!(
    #[allow(clippy::all, dead_code, unused_imports)]
    pub grammar
);

mod error;

pub use error::PikchrError;

/// Flags controlling [`pikchr`] behavior, mirroring upstream `pikchr()` flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PikchrFlags {
    /// Emit error messages as text/plain instead of HTML (PIKCHR_PLAINTEXT_ERRORS).
    pub plaintext_errors: bool,
    /// Invert colors for dark mode (PIKCHR_DARK_MODE).
    pub dark_mode: bool,
}

/// Render Pikchr source text to an SVG string.
///
/// On success returns the SVG document. On a Pikchr-level error returns
/// [`PikchrError`] carrying the message and (1-based) location, matching the
/// upstream behavior of surfacing the error together with the source.
pub fn pikchr(input: &str, flags: PikchrFlags) -> Result<String, PikchrError> {
    use lalrpop_util::ParseError;

    let mut ctx = pik::Pik::new(input, flags.dark_mode);

    // Macro expansion ($1..$9 and `define`) happens up front at tokenize time,
    // mirroring upstream's `pik_tokenize`; the parser consumes the expanded
    // stream.
    let tokens = match lexer::tokenize(input) {
        Ok(t) => t,
        Err(e) => {
            let (line, col) = line_col(input, e.at);
            return Err(PikchrError::new(e.message, line, col));
        }
    };
    let parse = grammar::DocumentParser::new().parse(&mut ctx, tokens.into_iter().map(Ok));

    // A Pikchr-level (semantic) error recorded during parsing takes priority.
    if let Some(e) = ctx.err.take() {
        return Err(e);
    }
    match parse {
        Ok(()) => Ok(ctx.finish()),
        Err(ParseError::User { error }) => {
            let (line, col) = line_col(input, error.at);
            Err(PikchrError::new(error.message, line, col))
        }
        Err(ParseError::UnrecognizedEof { .. }) => {
            Err(PikchrError::new("unexpected end of input", 1, 1))
        }
        Err(ParseError::UnrecognizedToken { token, .. }) => {
            let (line, col) = line_col(input, token.0);
            Err(PikchrError::new("syntax error", line, col))
        }
        Err(ParseError::ExtraToken { token }) => {
            let (line, col) = line_col(input, token.0);
            Err(PikchrError::new("syntax error", line, col))
        }
        Err(ParseError::InvalidToken { location }) => {
            let (line, col) = line_col(input, location);
            Err(PikchrError::new("invalid token", line, col))
        }
    }
}

fn line_col(src: &str, byte: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, c) in src.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
