//! Tokenizer for the Pikchr language — a faithful port of `pik_token_length`
//! (and the relevant parts of `pik_tokenize`) from pikchr.y.
//!
//! Produces LALRPOP-compatible `(start, Token, end)` triples. Whitespace and
//! comments are dropped; `$1..$9` macro parameters are dropped at top level
//! (they only matter inside `define` bodies, handled in a later milestone).

use crate::error::LexError;
use crate::keywords::{self, KwHit};
use crate::token::{AssignOp, Kw, Tok, Token};

pub type Spanned = Result<(usize, Token, usize), LexError>;

/// Convert a NUMBER token's text to inches (port of `pik_atof`).
pub fn atof(text: &str) -> f64 {
    let b = text.as_bytes();
    let n = b.len();
    if n >= 3 && b[0] == b'0' && (b[1] == b'x' || b[1] == b'X') {
        return i64::from_str_radix(&text[2..], 16).unwrap_or(0) as f64;
    }
    // Parse the leading floating-point prefix, like C's strtod.
    let (val, consumed) = strtod_prefix(text);
    // Unit suffix applies only when it is exactly the final two characters.
    if consumed == n.wrapping_sub(2) && n >= 2 {
        let c1 = b[consumed];
        let c2 = b[consumed + 1];
        return match (c1, c2) {
            (b'c', b'm') => val / 2.54,
            (b'm', b'm') => val / 25.4,
            (b'p', b'x') => val / 96.0,
            (b'p', b't') => val / 72.0,
            (b'p', b'c') => val / 6.0,
            _ => val, // "in" and anything else: inches / unchanged
        };
    }
    val
}

/// Parse a leading C-`strtod`-style float, returning (value, bytes_consumed).
fn strtod_prefix(s: &str) -> (f64, usize) {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    // exponent
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            j += 1;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let val = s[..i].parse::<f64>().unwrap_or(0.0);
    (val, i)
}

/// The classification of a raw token, mirroring `PToken.eType`.
#[derive(Debug, Clone, PartialEq)]
enum Raw {
    Whitespace,
    Eol,
    Error,
    Str,
    Codeblock,
    Parameter,
    Assign(AssignOp),
    Slash,
    Plus,
    Star,
    Percent,
    Lp,
    Rp,
    Lb,
    Rb,
    Comma,
    Colon,
    Gt,
    Eq,
    Minus,
    Lt,
    Rarrow,
    Larrow,
    Lrarrow,
    DotE,
    DotU,
    DotL,
    DotXy,
    Number,
    Nth,
    Classname,
    Id,
    Placename,
    Kw(Kw, i32, u8),
    Isodate,
}

#[inline]
fn is_lower(c: u8) -> bool {
    c.is_ascii_lowercase()
}
#[inline]
fn is_upper(c: u8) -> bool {
    c.is_ascii_uppercase()
}
#[inline]
fn is_alnum(c: u8) -> bool {
    c.is_ascii_alphanumeric()
}
#[inline]
fn id_char(c: u8) -> bool {
    is_alnum(c) || c == b'_'
}

/// Return the length (in bytes) of the next token starting at `z[0]`, plus its
/// classification. Faithful port of `pik_token_length`.
fn token_length(z: &[u8], allow_codeblock: bool) -> (usize, Raw) {
    let get = |i: usize| -> u8 { *z.get(i).unwrap_or(&0) };
    match z[0] {
        b'\\' => {
            let mut i = 1;
            while matches!(get(i), b'\r' | b' ' | b'\t') {
                i += 1;
            }
            if get(i) == b'\n' {
                (i + 1, Raw::Whitespace)
            } else {
                (1, Raw::Error)
            }
        }
        b';' | b'\n' => (1, Raw::Eol),
        b'"' => {
            let mut i = 1;
            loop {
                let c = get(i);
                if c == 0 {
                    return (i, Raw::Error);
                }
                if c == b'\\' {
                    if get(i + 1) == 0 {
                        return (i, Raw::Error);
                    }
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    return (i + 1, Raw::Str);
                }
                i += 1;
            }
        }
        b' ' | b'\t' | b'\x0c' | b'\r' => {
            let mut i = 1;
            while matches!(get(i), b' ' | b'\t' | b'\r' | b'\x0c') {
                i += 1;
            }
            (i, Raw::Whitespace)
        }
        b'#' => {
            let mut i = 1;
            while get(i) != 0 && get(i) != b'\n' {
                i += 1;
            }
            (i, Raw::Whitespace)
        }
        b'/' => {
            if get(1) == b'*' {
                let mut i = 2;
                while get(i) != 0 && !(get(i) == b'*' && get(i + 1) == b'/') {
                    i += 1;
                }
                if get(i) == b'*' {
                    (i + 2, Raw::Whitespace)
                } else {
                    (i, Raw::Error)
                }
            } else if get(1) == b'/' {
                let mut i = 2;
                while get(i) != 0 && get(i) != b'\n' {
                    i += 1;
                }
                (i, Raw::Whitespace)
            } else if get(1) == b'=' {
                (2, Raw::Assign(AssignOp::Slash))
            } else {
                (1, Raw::Slash)
            }
        }
        b'+' => {
            if get(1) == b'=' {
                (2, Raw::Assign(AssignOp::Plus))
            } else {
                (1, Raw::Plus)
            }
        }
        b'*' => {
            if get(1) == b'=' {
                (2, Raw::Assign(AssignOp::Star))
            } else {
                (1, Raw::Star)
            }
        }
        b'%' => (1, Raw::Percent),
        b'(' => (1, Raw::Lp),
        b')' => (1, Raw::Rp),
        b'[' => (1, Raw::Lb),
        b']' => (1, Raw::Rb),
        b',' => (1, Raw::Comma),
        b':' => (1, Raw::Colon),
        b'>' => (1, Raw::Gt),
        b'=' => {
            if get(1) == b'=' {
                (2, Raw::Eq)
            } else {
                (1, Raw::Assign(AssignOp::Set))
            }
        }
        b'-' => {
            if get(1) == b'>' {
                (2, Raw::Rarrow)
            } else if get(1) == b'=' {
                (2, Raw::Assign(AssignOp::Minus))
            } else {
                (1, Raw::Minus)
            }
        }
        b'<' => {
            if get(1) == b'-' {
                if get(2) == b'>' {
                    (3, Raw::Lrarrow)
                } else {
                    (2, Raw::Larrow)
                }
            } else {
                (1, Raw::Lt)
            }
        }
        0xe2 => {
            // Unicode arrows: ← → ↔
            if get(1) == 0x86 {
                match get(2) {
                    0x90 => return (3, Raw::Larrow),
                    0x92 => return (3, Raw::Rarrow),
                    0x94 => return (3, Raw::Lrarrow),
                    _ => {}
                }
            }
            (1, Raw::Error)
        }
        b'{' => {
            if !allow_codeblock {
                return (1, Raw::Error);
            }
            let mut i = 1;
            let mut depth = 1;
            while get(i) != 0 && depth > 0 {
                let (len, _) = token_length(&z[i..], false);
                if len == 1 {
                    if get(i) == b'{' {
                        depth += 1;
                    }
                    if get(i) == b'}' {
                        depth -= 1;
                    }
                }
                i += len;
            }
            if depth != 0 {
                (1, Raw::Error)
            } else {
                (i, Raw::Codeblock)
            }
        }
        b'&' => {
            const ENTITIES: &[(&[u8], Raw)] = &[
                (b"&rarr;", Raw::Rarrow),
                (b"&rightarrow;", Raw::Rarrow),
                (b"&larr;", Raw::Larrow),
                (b"&leftarrow;", Raw::Larrow),
                (b"&leftrightarrow;", Raw::Lrarrow),
            ];
            for (ent, raw) in ENTITIES {
                if z.len() >= ent.len() && &z[..ent.len()] == *ent {
                    return (ent.len(), raw.clone());
                }
            }
            (1, Raw::Error)
        }
        b'.' => {
            let c1 = get(1);
            if is_lower(c1) {
                // Read the following lowercase word just to classify the dot;
                // the dot token itself is 1 byte (the word is re-lexed).
                let mut i = 2;
                while is_lower(get(i)) {
                    i += 1;
                }
                let word = std::str::from_utf8(&z[1..i]).unwrap_or("");
                match keywords::lookup(word) {
                    Some(h) if keywords::is_edge_like(&h) => (1, Raw::DotE),
                    Some(h) if keywords::is_xy(&h) => (1, Raw::DotXy),
                    _ => (1, Raw::DotL),
                }
            } else if c1.is_ascii_digit() {
                number(z)
            } else if is_upper(c1) {
                (1, Raw::DotU)
            } else {
                (1, Raw::Error)
            }
        }
        c if c.is_ascii_digit() => number(z),
        c if is_lower(c) => {
            let mut i = 1;
            while id_char(get(i)) {
                i += 1;
            }
            let word = std::str::from_utf8(&z[..i]).unwrap_or("");
            match keywords::lookup(word) {
                Some(KwHit::Kw(k, code, edge)) => (i, Raw::Kw(k, code, edge)),
                Some(KwHit::Nth) => (i, Raw::Nth),
                Some(KwHit::Isodate) => (i, Raw::Isodate),
                None => {
                    if keywords::is_class_name(word) {
                        (i, Raw::Classname)
                    } else {
                        (i, Raw::Id)
                    }
                }
            }
        }
        c if is_upper(c) => {
            let mut i = 1;
            while id_char(get(i)) {
                i += 1;
            }
            (i, Raw::Placename)
        }
        b'$' if (b'1'..=b'9').contains(&get(1)) && !get(2).is_ascii_digit() => {
            (2, Raw::Parameter)
        }
        b'_' | b'$' | b'@' => {
            let mut i = 1;
            while id_char(get(i)) {
                i += 1;
            }
            (i, Raw::Id)
        }
        _ => (1, Raw::Error),
    }
}

/// Parse a NUMBER (or NTH) token starting at `z[0]`, which is a digit or `.`.
/// Faithful port of the numeric branch of `pik_token_length`.
fn number(z: &[u8]) -> (usize, Raw) {
    let get = |i: usize| -> u8 { *z.get(i).unwrap_or(&0) };
    let c0 = z[0];
    let mut i;
    let mut is_int = true;
    let mut n_digit;
    if c0 != b'.' {
        n_digit = 1;
        i = 1;
        while get(i).is_ascii_digit() {
            n_digit += 1;
            i += 1;
        }
        if i == 1 && (get(i) == b'x' || get(i) == b'X') {
            i = 2;
            while get(i) != 0 && get(i).is_ascii_hexdigit() {
                i += 1;
            }
            return (i, Raw::Number);
        }
    } else {
        is_int = false;
        n_digit = 0;
        i = 0;
    }
    if get(i) == b'.' {
        is_int = false;
        i += 1;
        while get(i).is_ascii_digit() {
            n_digit += 1;
            i += 1;
        }
    }
    if n_digit == 0 {
        return (i, Raw::Error);
    }
    if get(i) == b'e' || get(i) == b'E' {
        let i_before = i;
        i += 1;
        let mut c2 = get(i);
        if c2 == b'+' || c2 == b'-' {
            i += 1;
            c2 = get(i);
        }
        if !c2.is_ascii_digit() {
            i = i_before;
        } else {
            i += 1;
            is_int = false;
            while get(i).is_ascii_digit() {
                i += 1;
            }
        }
    }
    let c = get(i);
    let c2 = if c != 0 { get(i + 1) } else { 0 };
    if is_int
        && matches!(
            (c, c2),
            (b't', b'h') | (b'r', b'd') | (b'n', b'd') | (b's', b't')
        )
    {
        return (i + 2, Raw::Nth);
    }
    if matches!(
        (c, c2),
        (b'i', b'n') | (b'c', b'm') | (b'm', b'm') | (b'p', b't') | (b'p', b'x') | (b'p', b'c')
    ) {
        i += 2;
    }
    (i, Raw::Number)
}

/// Streaming lexer over a Pikchr source string.
pub struct Lexer<'input> {
    input: &'input str,
    bytes: Vec<u8>, // NUL-terminated copy so lookahead past the end reads 0
    pos: usize,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self {
        let mut bytes = input.as_bytes().to_vec();
        bytes.push(0); // sentinel; mirrors C's reliance on the NUL terminator
        Lexer {
            input,
            bytes,
            pos: 0,
        }
    }

    fn slice(&self, start: usize, end: usize) -> &str {
        // end is within the original input (the sentinel is never included in
        // a token span), so this is valid UTF-8 boundary-wise for our tokens.
        &self.input[start..end]
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Spanned;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // Stop at the sentinel NUL.
            if self.pos >= self.input.len() {
                return None;
            }
            let start = self.pos;
            let (len, raw) = token_length(&self.bytes[start..], true);
            let end = (start + len).min(self.input.len());
            self.pos = start + len;

            let text = self.slice(start, end);
            match raw {
                Raw::Whitespace | Raw::Parameter => continue,
                Raw::Error => {
                    return Some(Err(LexError {
                        message: format!("unrecognized token {text:?}"),
                        at: start,
                    }))
                }
                _ => {
                    if let Some(token) = raw_to_token(raw, text, start, end) {
                        return Some(Ok((start, token, end)));
                    }
                    continue;
                }
            }
        }
    }
}

/// Convert a (non-whitespace, non-error, non-parameter) raw classification to
/// a [`Token`]. Returns `None` only for raws that produce no token.
fn raw_to_token(raw: Raw, text: &str, start: usize, end: usize) -> Option<Token> {
    let mut tok = Tok::new(text, start, end);
    Some(match raw {
        Raw::Eol => Token::Eol(tok),
        Raw::Str => Token::Str(tok),
        Raw::Codeblock => Token::Codeblock(tok),
        Raw::Assign(op) => Token::Assign(op, tok),
        Raw::Slash => Token::Slash(tok),
        Raw::Plus => Token::Plus(tok),
        Raw::Star => Token::Star(tok),
        Raw::Percent => Token::Percent(tok),
        Raw::Lp => Token::Lp(tok),
        Raw::Rp => Token::Rp(tok),
        Raw::Lb => Token::Lb(tok),
        Raw::Rb => Token::Rb(tok),
        Raw::Comma => Token::Comma(tok),
        Raw::Colon => Token::Colon(tok),
        Raw::Gt => Token::Gt(tok),
        Raw::Eq => Token::Eq(tok),
        Raw::Minus => Token::Minus(tok),
        Raw::Lt => Token::Lt(tok),
        Raw::Rarrow => Token::Rarrow(tok),
        Raw::Larrow => Token::Larrow(tok),
        Raw::Lrarrow => Token::Lrarrow(tok),
        Raw::DotE => Token::DotE(tok),
        Raw::DotU => Token::DotU(tok),
        Raw::DotL => Token::DotL(tok),
        Raw::DotXy => Token::DotXy(tok),
        Raw::Number => Token::Num(atof(text), tok),
        Raw::Nth => Token::Nth(tok),
        Raw::Classname => Token::Classname(tok),
        Raw::Id => Token::Id(tok),
        Raw::Placename => Token::Placename(tok),
        Raw::Kw(k, code, edge) => {
            tok.e_code = code;
            tok.e_edge = edge;
            Token::Kw(k, tok)
        }
        // ISO date: upstream substitutes a "YYYY-MM-DD..." string literal.
        Raw::Isodate => {
            tok.text = "\"\"".to_string();
            Token::Str(tok)
        }
        Raw::Whitespace | Raw::Parameter | Raw::Error => return None,
    })
}

/// A `define`d macro: name plus the byte range of its body in the source.
struct Macro {
    name: String,
    body: usize,
    body_len: usize,
    in_use: bool,
}

const MAX_MACRO_DEPTH: usize = 10;

#[derive(PartialEq)]
enum DefState {
    None,
    Name,
    Body,
}

/// Tokenize with `define`-macro expansion and `$1..$9` parameter substitution,
/// mirroring `pik_tokenize` / `pik_parse_macro_args`. Because macro bodies and
/// call arguments are substrings of the source, the resulting token spans stay
/// valid offsets into the original input.
pub fn tokenize(input: &str) -> Result<Vec<(usize, Token, usize)>, LexError> {
    let mut bytes = input.as_bytes().to_vec();
    bytes.push(0);
    let mut t = Tokenizer {
        input,
        bytes,
        macros: Vec::new(),
        out: Vec::new(),
        err: None,
        def: DefState::None,
        pending_name: None,
    };
    let n = input.len();
    t.run(0, n, &EMPTY_ARGS, 0);
    match t.err {
        Some(e) => Err(e),
        None => Ok(t.out),
    }
}

type Args = [Option<(usize, usize)>; 9];
const EMPTY_ARGS: Args = [None; 9];

struct Tokenizer<'a> {
    input: &'a str,
    bytes: Vec<u8>,
    macros: Vec<Macro>,
    out: Vec<(usize, Token, usize)>,
    err: Option<LexError>,
    def: DefState,
    pending_name: Option<(usize, usize)>,
}

impl<'a> Tokenizer<'a> {
    fn find_macro(&self, name: &str) -> Option<usize> {
        self.macros.iter().position(|m| m.name == name)
    }

    /// Tokenize the byte range `[base, base+n)` with the given `$n` arguments.
    fn run(&mut self, base: usize, n: usize, args: &Args, depth: usize) {
        let mut i = 0usize;
        while i < n && self.bytes[base + i] != 0 && self.err.is_none() {
            let abs = base + i;
            let (sz, raw) = token_length(&self.bytes[abs..], true);
            match raw {
                Raw::Whitespace => {
                    i += sz;
                    continue;
                }
                Raw::Error => {
                    self.err = Some(LexError {
                        message: "unrecognized token".to_string(),
                        at: abs,
                    });
                    break;
                }
                _ => {}
            }
            if sz + i > n {
                self.err = Some(LexError {
                    message: "syntax error".to_string(),
                    at: abs,
                });
                break;
            }
            // $n parameter: substitute the corresponding argument's text.
            if raw == Raw::Parameter {
                let idx = (self.bytes[abs + 1] - b'1') as usize;
                if let Some((ps, pl)) = args[idx] {
                    if pl > 0 {
                        if depth >= MAX_MACRO_DEPTH {
                            self.err = Some(LexError {
                                message: "macros nested too deep".to_string(),
                                at: abs,
                            });
                            break;
                        }
                        self.run(ps, pl, &EMPTY_ARGS, depth + 1);
                    }
                }
                i += sz;
                continue;
            }
            // Macro invocation (an ID naming a macro, unless it is the name in
            // a `define` we are currently reading).
            if raw == Raw::Id && self.def != DefState::Name {
                let name = &self.input[abs..abs + sz];
                if let Some(mi) = self.find_macro(name) {
                    if self.macros[mi].in_use {
                        self.err = Some(LexError {
                            message: "recursive macro definition".to_string(),
                            at: abs,
                        });
                        break;
                    }
                    if depth >= MAX_MACRO_DEPTH {
                        self.err = Some(LexError {
                            message: "macros nested too deep".to_string(),
                            at: abs,
                        });
                        break;
                    }
                    let (consumed, call_args) =
                        self.parse_macro_args(abs + sz, n - (i + sz), args);
                    if self.err.is_some() {
                        break;
                    }
                    let (body, body_len) = (self.macros[mi].body, self.macros[mi].body_len);
                    self.macros[mi].in_use = true;
                    self.run(body, body_len, &call_args, depth + 1);
                    self.macros[mi].in_use = false;
                    i += sz + consumed;
                    continue;
                }
            }
            // Ordinary token: emit it, advancing the `define` state machine.
            let text = &self.input[abs..abs + sz];
            if let Some(token) = raw_to_token(raw.clone(), text, abs, abs + sz) {
                self.advance_def(&token, abs, sz);
                self.out.push((abs, token, abs + sz));
            }
            i += sz;
        }
    }

    /// Track `DEFINE ID CODEBLOCK` to register macros during tokenization.
    fn advance_def(&mut self, token: &Token, abs: usize, sz: usize) {
        match (&self.def, token) {
            (_, Token::Kw(crate::token::Kw::Define, _)) => {
                self.def = DefState::Name;
            }
            (DefState::Name, Token::Id(_)) => {
                self.pending_name = Some((abs, sz));
                self.def = DefState::Body;
            }
            (DefState::Body, Token::Codeblock(_)) => {
                // Body is the code block without its surrounding braces.
                if let Some((ns, nl)) = self.pending_name {
                    let name = self.input[ns..ns + nl].to_string();
                    let body = abs + 1;
                    let body_len = sz.saturating_sub(2);
                    match self.find_macro(&name) {
                        Some(mi) => {
                            self.macros[mi].body = body;
                            self.macros[mi].body_len = body_len;
                            self.macros[mi].in_use = false;
                        }
                        None => self.macros.push(Macro {
                            name,
                            body,
                            body_len,
                            in_use: false,
                        }),
                    }
                }
                self.def = DefState::None;
                self.pending_name = None;
            }
            _ => {
                self.def = DefState::None;
                self.pending_name = None;
            }
        }
    }

    /// Port of `pik_parse_macro_args`: parse `(a, b, ...)` after a macro name.
    /// Returns (bytes consumed incl. parens, the up-to-9 argument ranges).
    fn parse_macro_args(&mut self, abs: usize, navail: usize, outer: &Args) -> (usize, Args) {
        let mut args: Args = EMPTY_ARGS;
        let z = abs; // absolute offset of '('
        if navail == 0 || self.bytes[z] != b'(' {
            return (0, args);
        }
        // Raw (start, end) byte ranges per argument, before $n resolution.
        let mut ranges: [(usize, usize); 9] = [(0, 0); 9];
        let mut n_arg = 0usize;
        let mut i_start = 1usize;
        let mut depth = 0i32;
        let mut i = 1usize;
        while i < navail && self.bytes[z + i] != b')' {
            let (sz, _) = token_length(&self.bytes[z + i..], false);
            if sz != 1 {
                i += sz;
                continue;
            }
            let c = self.bytes[z + i];
            if c == b',' && depth <= 0 {
                ranges[n_arg] = (z + i_start, z + i);
                if n_arg == 8 {
                    self.err = Some(LexError {
                        message: "too many macro arguments - max 9".to_string(),
                        at: z,
                    });
                    return (0, args);
                }
                n_arg += 1;
                i_start = i + 1;
                depth = 0;
            } else if c == b'(' || c == b'{' || c == b'[' {
                depth += 1;
            } else if c == b')' || c == b'}' || c == b']' {
                depth -= 1;
            }
            i += sz;
        }
        if i < navail && self.bytes[z + i] == b')' {
            ranges[n_arg] = (z + i_start, z + i);
            for j in 0..=n_arg {
                let (mut s, mut e) = ranges[j];
                while s < e && (self.bytes[s] as char).is_whitespace() {
                    s += 1;
                }
                while e > s && (self.bytes[e - 1] as char).is_whitespace() {
                    e -= 1;
                }
                // A bare $n argument forwards the outer context's argument.
                if e - s == 2 && self.bytes[s] == b'$' && (b'1'..=b'9').contains(&self.bytes[s + 1]) {
                    let oi = (self.bytes[s + 1] - b'1') as usize;
                    args[j] = outer[oi];
                } else if e > s {
                    args[j] = Some((s, e - s));
                } else {
                    args[j] = None;
                }
            }
            return (i + 1, args);
        }
        self.err = Some(LexError {
            message: "unterminated macro argument list".to_string(),
            at: z,
        });
        (0, args)
    }
}
