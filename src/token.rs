//! Token types produced by the lexer and consumed by the LALRPOP grammar.
//!
//! Mirrors the `T_*` token codes, the `PikWord` keyword table, and the
//! `PToken` payload (text/eCode/eEdge) from `pikchr.y`. Every token carries a
//! [`Tok`] with its source span and text, matching how the upstream semantic
//! actions reach into `PToken.z/n/eCode/eEdge`.

/// Corner / edge codes (`CP_*` in pikchr.y).
pub mod cp {
    pub const N: u8 = 1;
    pub const NE: u8 = 2;
    pub const E: u8 = 3;
    pub const SE: u8 = 4;
    pub const S: u8 = 5;
    pub const SW: u8 = 6;
    pub const W: u8 = 7;
    pub const NW: u8 = 8;
    pub const C: u8 = 9; // .center or .c
    pub const END: u8 = 10; // .end
    pub const START: u8 = 11; // .start
}

/// Builtin function codes (`FN_*` in pikchr.y).
pub mod fnc {
    pub const ABS: i32 = 0;
    pub const COS: i32 = 1;
    pub const INT: i32 = 2;
    pub const MAX: i32 = 3;
    pub const MIN: i32 = 4;
    pub const SIN: i32 = 5;
    pub const SQRT: i32 = 6;
}

/// Movement directions (`DIR_*` in pikchr.y).
pub mod dir {
    pub const RIGHT: i32 = 0;
    pub const DOWN: i32 = 1;
    pub const LEFT: i32 = 2;
    pub const UP: i32 = 3;
}

/// Which operator an `ASSIGN` token represents (its `eCode`). For `=` the
/// upstream code stores `T_ASSIGN`; the compound forms store the arithmetic
/// op so that `x += y` desugars to `x = x + y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,   // =
    Plus,  // +=
    Minus, // -=
    Star,  // *=
    Slash, // /=
}

/// A lexed token together with its source span and (where relevant) text and
/// auxiliary codes. Equivalent to `PToken`.
#[derive(Debug, Clone, PartialEq)]
pub struct Tok {
    /// The exact source text of the token (for STRING this still includes the
    /// surrounding quotes, matching `PToken.z/n`).
    pub text: String,
    pub start: usize,
    pub end: usize,
    /// Auxiliary code (`eCode`): function id, direction, NTH ordinal, etc.
    pub e_code: i32,
    /// Corner/edge code (`eEdge`): a `cp::*` value, or 0.
    pub e_edge: u8,
}

impl Tok {
    pub fn new(text: &str, start: usize, end: usize) -> Self {
        Tok {
            text: text.to_string(),
            start,
            end,
            e_code: 0,
            e_edge: 0,
        }
    }
}

/// Keyword token kinds (the `T_*` codes that come from the keyword table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    Above,
    Aligned,
    And,
    As,
    Assert,
    At,
    Behind,
    Below,
    Between,
    Big,
    Bold,
    Bottom,
    Ccw,
    Center,
    Chop,
    Close,
    Color,
    Cw,
    Dashed,
    Define,
    Diameter,
    Dist,
    Dotted,
    Down,
    Edgept,
    End,
    Even,
    Fill,
    Fit,
    From,
    Func1,
    Func2,
    Go,
    Heading,
    Height,
    In,
    Invis,
    Italic,
    Last,
    Left,
    Ljust,
    Mono,
    Of,
    Print,
    Radius,
    Right,
    Rjust,
    Same,
    Small,
    Solid,
    Start,
    The,
    Then,
    Thick,
    Thickness,
    Thin,
    This,
    To,
    Top,
    Until,
    Up,
    Vertex,
    Way,
    Width,
    With,
    X,
    Y,
}

/// The token stream alphabet consumed by the grammar.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A keyword (carries direction/edge/func code in the [`Tok`]).
    Kw(Kw, Tok),
    /// Numeric literal, already converted to inches (units folded in).
    Num(f64, Tok),
    /// String literal; `Tok.text` includes the surrounding quotes.
    Str(Tok),
    /// Lowercase identifier that is neither a keyword nor a class name.
    Id(Tok),
    /// Uppercase-initial name (a place label, possibly a color name).
    Placename(Tok),
    /// A drawing class name (box, circle, line, ...).
    Classname(Tok),
    /// Ordinal selector ("2nd", "first", ...); ordinal in `Tok.e_code`.
    Nth(Tok),
    /// `{ ... }` macro/code block; `Tok.text` includes the braces.
    Codeblock(Tok),
    /// Statement separator (newline or `;`).
    Eol(Tok),
    /// Assignment operator; which op in the [`AssignOp`].
    Assign(AssignOp, Tok),

    DotE(Tok),  // ".<edge>"
    DotU(Tok),  // ".<Uppercase>" sublist member
    DotL(Tok),  // ".<lowercase property>"
    DotXy(Tok), // ".x" / ".y"

    Plus(Tok),
    Minus(Tok),
    Star(Tok),
    Slash(Tok),
    Percent(Tok),
    Lp(Tok),
    Rp(Tok),
    Lb(Tok),
    Rb(Tok),
    Comma(Tok),
    Colon(Tok),
    Eq(Tok),
    Lt(Tok),
    Gt(Tok),
    Larrow(Tok),
    Rarrow(Tok),
    Lrarrow(Tok),
}

impl Token {
    /// The source span of this token.
    pub fn span(&self) -> (usize, usize) {
        let t = self.tok();
        (t.start, t.end)
    }

    /// Borrow the [`Tok`] payload regardless of variant.
    pub fn tok(&self) -> &Tok {
        use Token::*;
        match self {
            Kw(_, t) | Num(_, t) | Str(t) | Id(t) | Placename(t) | Classname(t) | Nth(t)
            | Codeblock(t) | Eol(t) | Assign(_, t) | DotE(t) | DotU(t) | DotL(t) | DotXy(t)
            | Plus(t) | Minus(t) | Star(t) | Slash(t) | Percent(t) | Lp(t) | Rp(t) | Lb(t)
            | Rb(t) | Comma(t) | Colon(t) | Eq(t) | Lt(t) | Gt(t) | Larrow(t) | Rarrow(t)
            | Lrarrow(t) => t,
        }
    }
}
