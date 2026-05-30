//! P8 robustness: `pikchr()` must never panic, on any input — it should always
//! return either an SVG string or a clean `PikchrError`. This is a lightweight,
//! dependency-free, deterministic stand-in for `cargo-fuzz`: it throws random
//! byte soup, keyword salad, and mutations of the official corpus at the entry
//! point and asserts panic-freedom.

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Tiny deterministic PRNG (xorshift64*).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn must_not_panic(input: &str) {
    let res = catch_unwind(AssertUnwindSafe(|| {
        let _ = pikchr_rs::pikchr(input, Default::default());
    }));
    assert!(
        res.is_ok(),
        "pikchr() panicked on input ({} bytes): {:?}",
        input.len(),
        input
    );
}

#[test]
fn random_byte_soup_never_panics() {
    let mut rng = Rng(0x9E3779B97F4A7C15);
    // A printable-ASCII alphabet biased toward Pikchr-significant characters.
    let alpha: &[u8] = b"boxcircleardownuptlinearwfromto[](){}.,;\"$+-*/<>=%0123456789 \n\t#\\&:";
    for _ in 0..4000 {
        let len = rng.below(60);
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            s.push(alpha[rng.below(alpha.len())] as char);
        }
        must_not_panic(&s);
    }
}

#[test]
fn keyword_salad_never_panics() {
    let words = [
        "box", "circle", "arrow", "line", "spline", "arc", "from", "to", "then", "at", "with",
        ".n", ".c", ".end", "last", "2nd", "vertex", "of", "chop", "same", "[", "]", "define",
        "foo", "{", "}", "$1", "heading", "until", "even", "fit", "\"x\"", "->", "<-", "1cm",
        "(", ")", ",", "+", "*", "wid", "ht", "color", "red", "dashed", "0x10",
    ];
    let mut rng = Rng(0xDEADBEEFCAFEF00D);
    for _ in 0..4000 {
        let n = 1 + rng.below(12);
        let mut parts = Vec::new();
        for _ in 0..n {
            parts.push(words[rng.below(words.len())]);
        }
        must_not_panic(&parts.join(" "));
    }
}

#[test]
fn corpus_mutations_never_panic() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/official");
    let mut rng = Rng(0x12345678ABCDEF01);
    let srcs: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "pikchr").unwrap_or(false))
                .filter_map(|p| std::fs::read_to_string(p).ok())
                .collect()
        })
        .unwrap_or_default();
    for src in &srcs {
        must_not_panic(src);
        let bytes = src.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        // Truncations.
        for _ in 0..20 {
            let cut = rng.below(bytes.len());
            if let Ok(s) = std::str::from_utf8(&bytes[..cut]) {
                must_not_panic(s);
            }
        }
        // Single-byte ASCII substitutions.
        for _ in 0..40 {
            let mut b = bytes.to_vec();
            let pos = rng.below(b.len());
            b[pos] = (0x20 + rng.below(95)) as u8;
            if let Ok(s) = std::str::from_utf8(&b) {
                must_not_panic(s);
            }
        }
    }
}
