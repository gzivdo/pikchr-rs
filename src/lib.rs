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
pub mod token;

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
pub fn pikchr(input: &str, _flags: PikchrFlags) -> Result<String, PikchrError> {
    // P0 placeholder: the pipeline (lexer -> LALRPOP grammar -> layout -> SVG)
    // is wired up incrementally across milestones P1..P8.
    let tokens = lexer::Lexer::new(input);
    let _ = tokens; // lexer is exercised by unit tests until the grammar consumes it
    Err(PikchrError::new(
        "pikchr-rs is under construction (P0 scaffold)",
        1,
        1,
    ))
}
