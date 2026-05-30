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

## Status

Under active, incremental construction. Milestones:

| Milestone | Scope | State |
|---|---|---|
| P0 | crate scaffold + lexer→LALRPOP pipeline + harness | ✅ done |
| P1 | full lexer (numbers/units, strings, names, comments, keywords) | ✅ done |
| P2 | grammar (LALRPOP) + object model; full grammar wired | ✅ done |
| P3 | expressions, variables, units, builtins, colors | ✅ done |
| P4 | basic objects + layout + SVG; **geometry matches C reference** | ✅ done |
| P5 | positioning & references (`at`/`from`/`to`/`then`/`chop`, `.n`/`.c`, `last`/`2nd`, `same`, sublists) | ✅ done |
| P6 | arc/spline curves + exact text metrics, vertical layout, `<text>` parity | ✅ done |
| P7 | containers `[ … ]`, `define` macros (`$1..$9`), `direction` | ✅ done |
| P8 | full official corpus parity, fuzzing, error parity | ✅ done |

**Parity:** every one of the 96 renderable diagrams in the official Pikchr
`tests/` suite renders **byte-for-byte identical** to the upstream C reference
(`tests/p8_official.rs`); 5 of the 6 deliberate error tests are rejected with a
matching diagnostic (see "Known differences"). Fuzz/robustness tests
(`tests/p8_fuzz.rs`) throw ~12k random, keyword-salad and corpus-mutation inputs
at `pikchr()` and confirm it never panics.

## Known differences

- `tests/test60` (a deliberate macro-redefinition *error* test) is accepted and
  rendered instead of raising the upstream "syntax error"; the divergence is in
  an obscure error path, not in any rendered output.

Output is validated against the upstream C Pikchr: the full official `tests/`
corpus byte-for-byte (`tests/p8_official.rs`), a 27-case geometry corpus
(`tests/p4_diff.rs`), and a 26-case full-SVG corpus
(`tests/p6_text.rs`). `box`, `circle`, `ellipse`, `line`, `arrow`, `move`,
`dot`, `diamond`, `cylinder`, `file`, `oval`, and `text` render; directions,
`then`-paths, auto-fit, rounded boxes, arrowheads, colors, dashes/dots,
thickness, named labels, `.edge` points, `last`/`Nth`/`Name.Sub` references,
`from`/`to`/`heading`/`until even with`, `chop`, `at`/`with` edge placement,
`same [as]`, and `[ … ]` sub-blocks are supported.

See [`UNSUPPORTED.md`](UNSUPPORTED.md) for language constructs not yet covered.

## License

MIT (see [`LICENSE`](LICENSE)). Original Pikchr is by D. R. Hipp under 0BSD; see
[`NOTICE`](NOTICE). Idea/originator: gzivdo.
