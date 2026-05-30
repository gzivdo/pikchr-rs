# pikchr-rs

A **pure-Rust** port of [Pikchr](https://pikchr.org/) — the PIC-inspired diagram
language that turns a compact text description into an SVG drawing. No C, no FFI,
no build-time C toolchain.

```rust
use pikchr_rs::{pikchr, PikchrFlags};

let svg = pikchr("box \"Hello\"; arrow; circle \"World\"", PikchrFlags::default())?;
```

## Why a port instead of FFI?

The existing `pikchr` crate wraps the upstream `pikchr.c` via FFI. This project
re-implements Pikchr in safe Rust (`#![forbid(unsafe_code)]`) so it can be
statically linked with no C toolchain and embedded uniformly alongside other
pure-Rust modules.

The **source of truth** is the upstream Lemon grammar
[`pikchr.y`](https://github.com/drhsqlite/pikchr/blob/master/pikchr.y), not the
generated `pikchr.c`. The ~6% grammar rules are ported to
[LALRPOP](https://github.com/lalrpop/lalrpop); the ~94% C semantics (geometry,
layout, SVG emission) are hand-ported to Rust.

## Features

All Pikchr objects render — `box`, `circle`, `ellipse`, `line`, `arrow`,
`move`, `dot`, `diamond`, `cylinder`, `file`, `oval`, `text` — together with
directions, `then`-paths, arc/spline curves, auto-fit sizing, rounded boxes,
arrowheads, colors, dashes/dots, thickness, text with full positioning and
justification, named labels, `.edge` points, `last`/`Nth`/`Name.Sub`
references, `from`/`to`/`heading`/`until even with`, `chop`, `at`/`with` edge
placement, `same [as]`, `[ … ]` sub-blocks, and `define` macros with `$1..$9`
parameters.

## Validation

Output is checked **byte-for-byte** against upstream Pikchr: every one of the 96
renderable diagrams in the official Pikchr `tests/` suite is identical
(`tests/p8_official.rs`), alongside curated geometry and full-SVG corpora
(`tests/p4_diff.rs`, `tests/p6_text.rs`). Robustness tests (`tests/p8_fuzz.rs`)
throw ~12k random, keyword-salad and corpus-mutation inputs at `pikchr()` and
confirm it never panics. 5 of the 6 deliberate error tests are rejected with a
matching diagnostic.

## Known differences

- `tests/test60` (a deliberate macro-redefinition *error* test) is accepted and
  rendered instead of raising the upstream "syntax error"; the divergence is in
  an obscure error path, not in any rendered output.

## License

MIT (see [`LICENSE`](LICENSE)). Original Pikchr is by D. R. Hipp under 0BSD; see
[`NOTICE`](NOTICE). Idea/originator: gzivdo.
