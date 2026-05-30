//! P8 full-corpus parity: every renderable diagram from the **official Pikchr
//! `tests/` suite** must produce SVG byte-for-byte identical to the C reference
//! (modulo the CLI-injected `class="pikchr"` / `data-pikchr-date`, already
//! stripped from the goldens, and a trailing newline).
//!
//! The 6 upstream tests that are deliberate error cases are not included here;
//! error behavior is covered separately.

use std::fs;
use std::path::Path;

#[test]
fn official_corpus_matches_c_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/official");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("tests/official dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "pikchr").unwrap_or(false))
        .collect();
    entries.sort();

    let mut cases = 0;
    let mut failures = Vec::new();
    for pikchr in &entries {
        let svg_path = pikchr.with_extension("svg");
        let src = fs::read_to_string(pikchr).unwrap();
        let golden = fs::read_to_string(&svg_path).unwrap();
        match pikchr_rs::pikchr(&src, Default::default()) {
            Ok(got) => {
                if got.trim_end() != golden.trim_end() {
                    failures.push(format!("{}: SVG mismatch", pikchr.display()));
                }
            }
            Err(e) => failures.push(format!("{}: render error: {e}", pikchr.display())),
        }
        cases += 1;
    }
    assert!(cases >= 90, "expected the full official corpus, got {cases}");
    assert!(
        failures.is_empty(),
        "{} of {cases} official tests diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
