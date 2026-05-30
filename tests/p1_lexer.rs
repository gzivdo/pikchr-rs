//! P1 lexer tests: token classification per pikchr.y's `pik_token_length`.

use pikchr_rs::lexer::{atof, Lexer};
use pikchr_rs::token::{AssignOp, Kw, Token};

/// Lex into a vector of tokens (panicking on lex error), for assertions.
fn lex(src: &str) -> Vec<Token> {
    Lexer::new(src)
        .map(|r| r.expect("lex error").1)
        .collect()
}

fn texts(src: &str) -> Vec<String> {
    lex(src).iter().map(|t| t.tok().text.clone()).collect()
}

#[test]
fn skips_whitespace_and_comments() {
    assert!(lex("   \t  ").is_empty());
    assert!(lex("# a line comment").is_empty());
    assert!(lex("// slash comment").is_empty());
    assert!(lex("/* block\n comment */").is_empty());
    // Backslash-newline is whitespace (line continuation).
    assert!(lex("\\\n").is_empty());
}

#[test]
fn newline_and_semicolon_are_eol() {
    let t = lex("\n;\n");
    assert_eq!(t.len(), 3);
    assert!(t.iter().all(|x| matches!(x, Token::Eol(_))));
}

#[test]
fn numbers_with_units_convert_to_inches() {
    assert_eq!(atof("1"), 1.0);
    assert_eq!(atof("2.54cm"), 1.0);
    assert_eq!(atof("25.4mm"), 1.0);
    assert_eq!(atof("96px"), 1.0);
    assert_eq!(atof("72pt"), 1.0);
    assert_eq!(atof("6pc"), 1.0);
    assert_eq!(atof("3in"), 3.0); // inches unchanged
    assert_eq!(atof("0x10"), 16.0); // hex
    assert_eq!(atof("1.5e2"), 150.0);
    match &lex("2cm")[0] {
        Token::Num(v, t) => {
            assert!((v - 2.0 / 2.54).abs() < 1e-12);
            assert_eq!(t.text, "2cm");
        }
        other => panic!("expected Num, got {other:?}"),
    }
}

#[test]
fn ordinal_suffix_is_nth() {
    assert!(matches!(lex("2nd")[0], Token::Nth(_)));
    assert!(matches!(lex("3rd")[0], Token::Nth(_)));
    assert!(matches!(lex("1st")[0], Token::Nth(_)));
    assert!(matches!(lex("4th")[0], Token::Nth(_)));
    assert!(matches!(lex("first")[0], Token::Nth(_)));
}

#[test]
fn strings_keep_quotes_and_handle_escapes() {
    match &lex(r#""hello""#)[0] {
        Token::Str(t) => assert_eq!(t.text, r#""hello""#),
        other => panic!("expected Str, got {other:?}"),
    }
    match &lex(r#""a\"b""#)[0] {
        Token::Str(t) => assert_eq!(t.text, r#""a\"b""#),
        other => panic!("expected Str, got {other:?}"),
    }
}

#[test]
fn classes_keywords_ids_placenames() {
    assert!(matches!(lex("box")[0], Token::Classname(_)));
    assert!(matches!(lex("circle")[0], Token::Classname(_)));
    assert!(matches!(lex("at")[0], Token::Kw(Kw::At, _)));
    assert!(matches!(lex("box2")[0], Token::Id(_))); // not a class (len differs)
    assert!(matches!(lex("Foo")[0], Token::Placename(_)));
    assert!(matches!(lex("myvar")[0], Token::Id(_)));
}

#[test]
fn edge_keywords_carry_corner_codes() {
    use pikchr_rs::token::cp;
    match &lex("ne")[0] {
        Token::Kw(Kw::Edgept, t) => assert_eq!(t.e_edge, cp::NE),
        other => panic!("expected Edgept, got {other:?}"),
    }
    match &lex("left")[0] {
        Token::Kw(Kw::Left, t) => {
            assert_eq!(t.e_edge, cp::W);
            assert_eq!(t.e_code, pikchr_rs::token::dir::LEFT);
        }
        other => panic!("expected Left, got {other:?}"),
    }
}

#[test]
fn dot_classification_consumes_only_the_dot() {
    // ".n" => DotE (len 1) then Edgept("n")
    let t = lex(".n");
    assert!(matches!(t[0], Token::DotE(_)));
    assert_eq!(t[0].tok().text, ".");
    assert!(matches!(t[1], Token::Kw(Kw::Edgept, _)));

    // ".x" => DotXy then X
    let t = lex(".x");
    assert!(matches!(t[0], Token::DotXy(_)));
    assert!(matches!(t[1], Token::Kw(Kw::X, _)));

    // ".width" => DotL then Width
    let t = lex(".width");
    assert!(matches!(t[0], Token::DotL(_)));
    assert!(matches!(t[1], Token::Kw(Kw::Width, _)));

    // ".Foo" => DotU then Placename
    let t = lex(".Foo");
    assert!(matches!(t[0], Token::DotU(_)));
    assert!(matches!(t[1], Token::Placename(_)));

    // ".5" => Number
    assert!(matches!(lex(".5")[0], Token::Num(_, _)));
}

#[test]
fn operators_and_arrows() {
    assert!(matches!(lex("->")[0], Token::Rarrow(_)));
    assert!(matches!(lex("<-")[0], Token::Larrow(_)));
    assert!(matches!(lex("<->")[0], Token::Lrarrow(_)));
    assert!(matches!(lex("==")[0], Token::Eq(_)));
    assert!(matches!(lex("=")[0], Token::Assign(AssignOp::Set, _)));
    assert!(matches!(lex("+=")[0], Token::Assign(AssignOp::Plus, _)));
    assert!(matches!(lex("/=")[0], Token::Assign(AssignOp::Slash, _)));
    // Unicode arrows
    assert!(matches!(lex("\u{2190}")[0], Token::Larrow(_)));
    assert!(matches!(lex("\u{2192}")[0], Token::Rarrow(_)));
    assert!(matches!(lex("\u{2194}")[0], Token::Lrarrow(_)));
}

#[test]
fn codeblock_braces_balance() {
    match &lex("{ box; {circle} }")[0] {
        Token::Codeblock(t) => assert_eq!(t.text, "{ box; {circle} }"),
        other => panic!("expected Codeblock, got {other:?}"),
    }
}

#[test]
fn a_small_full_line() {
    // "box \"hi\" wid 2cm" => Classname, Str, Width, Num
    let t = texts("box \"hi\" wid 2cm");
    assert_eq!(t, vec!["box", "\"hi\"", "wid", "2cm"]);
    let toks = lex("box \"hi\" wid 2cm");
    assert!(matches!(toks[0], Token::Classname(_)));
    assert!(matches!(toks[1], Token::Str(_)));
    assert!(matches!(toks[2], Token::Kw(Kw::Width, _)));
    assert!(matches!(toks[3], Token::Num(_, _)));
}
