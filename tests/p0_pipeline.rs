//! Pipeline smoke test: lexer -> LALRPOP grammar -> layout -> SVG.

#[test]
fn renders_a_box() {
    let svg = pikchr_rs::pikchr("box", Default::default()).expect("should render");
    assert!(svg.starts_with("<svg"), "got: {svg}");
    assert!(svg.contains("<path"));
    assert!(svg.ends_with("</svg>\n"));
}

#[test]
fn empty_input_is_ok() {
    let svg = pikchr_rs::pikchr("", Default::default()).expect("empty should render");
    assert!(svg.contains("<svg"));
}
