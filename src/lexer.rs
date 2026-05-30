//! Tokenizer for the Pikchr language.
//!
//! P0: a minimal lexer that recognizes numbers and end-of-line separators,
//! producing LALRPOP-compatible `(start, Token, end)` triples. The full
//! Pikchr tokenizer (numbers+units, strings, names, operators, comments,
//! line continuations, keyword/edge classification) lands in P1.

use crate::error::LexError;
use crate::token::Token;

pub type Spanned = Result<(usize, Token, usize), LexError>;

/// Streaming lexer over a Pikchr source string.
pub struct Lexer<'input> {
    input: &'input str,
    bytes: &'input [u8],
    pos: usize,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self {
        Lexer {
            input,
            bytes: input.as_bytes(),
            pos: 0,
        }
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Spanned;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.bytes;
        // Skip plain spaces/tabs (not newlines, which are EOL tokens).
        while self.pos < b.len() && (b[self.pos] == b' ' || b[self.pos] == b'\t') {
            self.pos += 1;
        }
        if self.pos >= b.len() {
            return None;
        }
        let start = self.pos;
        let c = b[self.pos];

        if c == b'\n' {
            self.pos += 1;
            return Some(Ok((start, Token::Eol, self.pos)));
        }

        if c.is_ascii_digit() || c == b'.' {
            while self.pos < b.len()
                && (b[self.pos].is_ascii_digit() || b[self.pos] == b'.')
            {
                self.pos += 1;
            }
            let text = &self.input[start..self.pos];
            return Some(match text.parse::<f64>() {
                Ok(n) => Ok((start, Token::Number(n), self.pos)),
                Err(_) => Err(LexError {
                    message: format!("invalid number {text:?}"),
                    at: start,
                }),
            });
        }

        // Unknown byte in P0: report and stop.
        self.pos = b.len();
        Some(Err(LexError {
            message: format!("unexpected character {:?}", c as char),
            at: start,
        }))
    }
}
