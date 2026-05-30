//! P4 diff-test: geometry parity against the C reference.
//!
//! Each `tests/corpus/NN.pikchr` has a sibling `NN.geom` holding the
//! geometry primitives (viewBox + path/circle/ellipse/polygon coordinates)
//! extracted from the **C reference** Pikchr (the oracle is built once and is
//! not required in CI; see the porting spec §6). We render the same input
//! with this port, extract the same primitives, and require an exact match.
//! Text rendering (cosmetic attributes) is intentionally excluded here and
//! covered separately.

use std::fs;
use std::path::Path;

/// Extract the geometry-bearing fragments from an SVG, one per line, in order.
fn normalize(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in svg.lines() {
        let l = line.trim();
        if let Some(v) = extract(l, "viewBox=\"", "\"") {
            out.push(format!("viewBox=\"{v}\""));
        }
        if l.starts_with("<path ") {
            if let Some(d) = extract(l, "d=\"", "\"") {
                out.push(format!("<path d=\"{d}\""));
            }
        } else if l.starts_with("<circle ") {
            let cx = extract(l, "cx=\"", "\"").unwrap_or_default();
            let cy = extract(l, "cy=\"", "\"").unwrap_or_default();
            let r = extract(l, "r=\"", "\"").unwrap_or_default();
            out.push(format!("<circle cx=\"{cx}\" cy=\"{cy}\" r=\"{r}\""));
        } else if l.starts_with("<ellipse ") {
            let cx = extract(l, "cx=\"", "\"").unwrap_or_default();
            let cy = extract(l, "cy=\"", "\"").unwrap_or_default();
            let rx = extract(l, "rx=\"", "\"").unwrap_or_default();
            let ry = extract(l, "ry=\"", "\"").unwrap_or_default();
            out.push(format!(
                "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{rx}\" ry=\"{ry}\""
            ));
        } else if l.starts_with("<polygon ") {
            if let Some(pts) = extract(l, "points=\"", "\"") {
                out.push(format!("<polygon points=\"{pts}\""));
            }
        }
    }
    out
}

fn extract(s: &str, open: &str, close: &str) -> Option<String> {
    let start = s.find(open)? + open.len();
    let rest = &s[start..];
    let end = rest.find(close)?;
    Some(rest[..end].to_string())
}

#[test]
fn geometry_matches_c_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut cases = 0;
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .expect("corpus dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "pikchr").unwrap_or(false))
        .collect();
    entries.sort();

    for pikchr in entries {
        let geom = pikchr.with_extension("geom");
        let src = fs::read_to_string(&pikchr).unwrap();
        let golden = fs::read_to_string(&geom).unwrap();
        let golden: Vec<String> = golden.lines().map(|s| s.to_string()).collect();

        let svg = pikchr_rs::pikchr(&src, Default::default())
            .unwrap_or_else(|e| panic!("{}: render error: {e}", pikchr.display()));
        let got = normalize(&svg);

        assert_eq!(
            got,
            golden,
            "geometry mismatch for {}\ninput: {}",
            pikchr.display(),
            src.trim()
        );
        cases += 1;
    }
    assert!(cases >= 10, "expected a non-trivial corpus, got {cases}");
}
