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
| P6 | remaining objects & attributes (arc/spline curves, text metrics, justification) | ⬜ |
| P7 | containers `[ … ]`, `define` macros, `direction` | ⬜ |
| P8 | full parity vs C reference, fuzzing, error parity | ⬜ |

Geometry is validated byte-for-byte against the upstream C Pikchr on a 27-case
corpus covering object chains *and* positioning/references (see
`tests/p4_diff.rs`). `box`, `circle`, `ellipse`, `line`, `arrow`, `move`,
`dot`, `diamond`, `cylinder`, `file`, `oval`, and `text` render; directions,
`then`-paths, auto-fit, rounded boxes, arrowheads, colors, dashes/dots,
thickness, named labels, `.edge` points, `last`/`Nth`/`Name.Sub` references,
`from`/`to`/`heading`/`until even with`, `chop`, `at`/`with` edge placement,
`same [as]`, and `[ … ]` sub-blocks are supported.

See [`UNSUPPORTED.md`](UNSUPPORTED.md) for language constructs not yet covered.

## License

MIT (see [`LICENSE`](LICENSE)). Original Pikchr is by D. R. Hipp under 0BSD; see
[`NOTICE`](NOTICE). Idea/originator: gzivdo.
