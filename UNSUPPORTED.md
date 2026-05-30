# Unsupported / not-yet-ported constructs

Per the porting spec, the Definition of Done is behavioral parity with `pikchr.y`
across the official `tests/` corpus. While the port is incremental (milestones
P0–P8), anything deferred is tracked here with a reason. On completion this file
should be empty.

## Currently deferred (pre-P2)

- **Everything except the lexer→parser pipeline scaffold.** The crate is at the
  P0/P1 stage: `pikchr()` returns an "under construction" error. Object layout
  and SVG emission arrive in P4+.

_Reason: incremental milestone delivery (see `TZ_pikchr_rust_port.md` §5)._
