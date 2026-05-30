//! Token types produced by the lexer and consumed by the LALRPOP grammar.
//!
//! P0: a minimal placeholder set, just enough to validate the lexer ->
//! LALRPOP build pipeline. The full Pikchr terminal set (mirroring the
//! `PToken` types in `pikchr.y`) is introduced in P1/P2.

/// A lexed token together with the byte span it covers in the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub tok: Token,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A numeric literal (with units already folded into inches; see P3).
    Number(f64),
    /// End-of-line statement separator.
    Eol,
}
