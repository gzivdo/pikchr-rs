//! Error types for pikchr-rs.

use thiserror::Error;

/// A Pikchr-level error: a human-readable message plus a 1-based source
/// location (line/column), mirroring upstream's "message + source echo"
/// behavior.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message} (line {line}, col {col})")]
pub struct PikchrError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl PikchrError {
    pub fn new(message: impl Into<String>, line: usize, col: usize) -> Self {
        PikchrError {
            message: message.into(),
            line,
            col,
        }
    }
}

/// Lexer-level error reported through the LALRPOP token stream.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("lex error at byte {at}: {message}")]
pub struct LexError {
    pub message: String,
    pub at: usize,
}
