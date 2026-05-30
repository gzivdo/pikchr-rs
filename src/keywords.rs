//! Keyword classification, mirroring the `pik_keywords` table in pikchr.y.

use crate::token::{cp, dir, fnc, Kw};

/// Result of looking up a word in the keyword table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KwHit {
    /// A keyword token with its auxiliary `e_code` and corner `e_edge`.
    Kw(Kw, i32, u8),
    /// "first" — an ordinal selector (T_NTH).
    Nth,
    /// "pikchr_date" — replaced by an ISO-date string literal (T_ISODATE).
    Isodate,
}

/// Class names recognized as `T_CLASSNAME` (the entries of `aClass` that can
/// be produced from source text).
pub fn is_class_name(word: &str) -> bool {
    matches!(
        word,
        "arc" | "arrow"
            | "box"
            | "circle"
            | "cylinder"
            | "diamond"
            | "dot"
            | "ellipse"
            | "file"
            | "line"
            | "move"
            | "oval"
            | "spline"
            | "text"
    )
}

/// Look up a lowercase word in the keyword table.
pub fn lookup(word: &str) -> Option<KwHit> {
    use Kw::*;
    let hit = |k: Kw| KwHit::Kw(k, 0, 0);
    Some(match word {
        "above" => hit(Above),
        "abs" => KwHit::Kw(Func1, fnc::ABS, 0),
        "aligned" => hit(Aligned),
        "and" => hit(And),
        "as" => hit(As),
        "assert" => hit(Assert),
        "at" => hit(At),
        "behind" => hit(Behind),
        "below" => hit(Below),
        "between" => hit(Between),
        "big" => hit(Big),
        "bold" => hit(Bold),
        "bot" => KwHit::Kw(Edgept, 0, cp::S),
        "bottom" => KwHit::Kw(Bottom, 0, cp::S),
        "c" => KwHit::Kw(Edgept, 0, cp::C),
        "ccw" => hit(Ccw),
        "center" => KwHit::Kw(Center, 0, cp::C),
        "chop" => hit(Chop),
        "close" => hit(Close),
        "color" => hit(Color),
        "cos" => KwHit::Kw(Func1, fnc::COS, 0),
        "cw" => hit(Cw),
        "dashed" => hit(Dashed),
        "define" => hit(Define),
        "diameter" => hit(Diameter),
        "dist" => hit(Dist),
        "dotted" => hit(Dotted),
        "down" => KwHit::Kw(Down, dir::DOWN, 0),
        "e" => KwHit::Kw(Edgept, 0, cp::E),
        "east" => KwHit::Kw(Edgept, 0, cp::E),
        "end" => KwHit::Kw(End, 0, cp::END),
        "even" => hit(Even),
        "fill" => hit(Fill),
        "first" => KwHit::Nth,
        "fit" => hit(Fit),
        "from" => hit(From),
        "go" => hit(Go),
        "heading" => hit(Heading),
        "height" => hit(Height),
        "ht" => hit(Height),
        "in" => hit(In),
        "int" => KwHit::Kw(Func1, fnc::INT, 0),
        "invis" => hit(Invis),
        "invisible" => hit(Invis),
        "italic" => hit(Italic),
        "last" => hit(Last),
        "left" => KwHit::Kw(Left, dir::LEFT, cp::W),
        "ljust" => hit(Ljust),
        "max" => KwHit::Kw(Func2, fnc::MAX, 0),
        "min" => KwHit::Kw(Func2, fnc::MIN, 0),
        "mono" => hit(Mono),
        "monospace" => hit(Mono),
        "n" => KwHit::Kw(Edgept, 0, cp::N),
        "ne" => KwHit::Kw(Edgept, 0, cp::NE),
        "north" => KwHit::Kw(Edgept, 0, cp::N),
        "nw" => KwHit::Kw(Edgept, 0, cp::NW),
        "of" => hit(Of),
        "pikchr_date" => KwHit::Isodate,
        "previous" => hit(Last),
        "print" => hit(Print),
        "rad" => hit(Radius),
        "radius" => hit(Radius),
        "right" => KwHit::Kw(Right, dir::RIGHT, cp::E),
        "rjust" => hit(Rjust),
        "s" => KwHit::Kw(Edgept, 0, cp::S),
        "same" => hit(Same),
        "se" => KwHit::Kw(Edgept, 0, cp::SE),
        "sin" => KwHit::Kw(Func1, fnc::SIN, 0),
        "small" => hit(Small),
        "solid" => hit(Solid),
        "south" => KwHit::Kw(Edgept, 0, cp::S),
        "sqrt" => KwHit::Kw(Func1, fnc::SQRT, 0),
        "start" => KwHit::Kw(Start, 0, cp::START),
        "sw" => KwHit::Kw(Edgept, 0, cp::SW),
        "t" => KwHit::Kw(Top, 0, cp::N),
        "the" => hit(The),
        "then" => hit(Then),
        "thick" => hit(Thick),
        "thickness" => hit(Thickness),
        "thin" => hit(Thin),
        "this" => hit(This),
        "to" => hit(To),
        "top" => KwHit::Kw(Top, 0, cp::N),
        "until" => hit(Until),
        "up" => KwHit::Kw(Up, dir::UP, 0),
        "vertex" => hit(Vertex),
        "w" => KwHit::Kw(Edgept, 0, cp::W),
        "way" => hit(Way),
        "west" => KwHit::Kw(Edgept, 0, cp::W),
        "wid" => hit(Width),
        "width" => hit(Width),
        "with" => hit(With),
        "x" => hit(X),
        "y" => hit(Y),
        _ => return None,
    })
}

/// Returns true if a keyword hit denotes a 2-D place value (used to decide
/// whether a `.word` is a DOT_E edge access). Mirrors the upstream condition
/// `eEdge>0 || eType in {EDGEPT, START, END}` — note `top`, `bottom`, `left`,
/// `right`, `center` etc. all carry a non-zero corner code.
pub fn is_edge_like(hit: &KwHit) -> bool {
    match hit {
        KwHit::Kw(k, _, edge) => {
            *edge > 0 || matches!(k, Kw::Edgept | Kw::Start | Kw::End)
        }
        _ => false,
    }
}

/// Returns true if a keyword hit is `x` or `y` (used to decide DOT_XY).
pub fn is_xy(hit: &KwHit) -> bool {
    matches!(hit, KwHit::Kw(Kw::X, _, _) | KwHit::Kw(Kw::Y, _, _))
}
