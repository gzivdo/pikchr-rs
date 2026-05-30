//! P0 smoke test: confirms the lexer -> LALRPOP grammar pipeline is wired up.
//! Replaced by real parse/render tests in later milestones.

use pikchr_rs::grammar::DocumentParser;
use pikchr_rs::lexer::Lexer;

fn parse(src: &str) -> Vec<f64> {
    DocumentParser::new()
        .parse(Lexer::new(src))
        .expect("pipeline should parse a list of numbers")
}

#[test]
fn lexer_grammar_pipeline_parses_numbers() {
    assert_eq!(parse(""), Vec::<f64>::new());
    assert_eq!(parse("1"), vec![1.0]);
    assert_eq!(parse("1\n2\n3"), vec![1.0, 2.0, 3.0]);
    assert_eq!(parse("0.5\n42"), vec![0.5, 42.0]);
}

#[test]
fn public_api_is_callable() {
    // P0: pikchr() is a placeholder that reports it is under construction.
    let err = pikchr_rs::pikchr("box", Default::default()).unwrap_err();
    assert!(err.message.contains("construction"));
}
