//! P6 full-SVG parity: text metrics, vertical layout, `<text>` attributes,
//! arc and spline curves. Goldens are the **C reference** output with only the
//! CLI-injected `class="pikchr"` and `data-pikchr-date="..."` removed. Unlike
//! `p4_diff` (geometry-only), this asserts the entire SVG matches byte-for-byte.

use std::fs;
use std::path::Path;

#[test]
fn full_svg_matches_c_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus_full");
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("corpus_full dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "pikchr").unwrap_or(false))
        .collect();
    entries.sort();

    let mut cases = 0;
    for pikchr in entries {
        let svg_path = pikchr.with_extension("svg");
        let src = fs::read_to_string(&pikchr).unwrap();
        let golden = fs::read_to_string(&svg_path).unwrap();
        let got = pikchr_rs::pikchr(&src, Default::default())
            .unwrap_or_else(|e| panic!("{}: render error: {e}", pikchr.display()));
        // The reference CLI appends an extra trailing newline; ignore trailing
        // whitespace only.
        assert_eq!(
            got.trim_end(),
            golden.trim_end(),
            "full SVG mismatch for {}\ninput: {}",
            pikchr.display(),
            src.trim()
        );
        cases += 1;
    }
    assert!(cases >= 12, "expected a non-trivial corpus, got {cases}");
}
