# Unsupported / not-yet-ported constructs

Per the porting spec, the Definition of Done is behavioral parity with `pikchr.y`
across the official `tests/` corpus. The port is incremental (milestones P0–P8);
anything deferred is tracked here with a reason. On completion this file should
be empty.

## Deferred to P7

- `define` macros (text substitution at tokenize time) and `$1..$9` parameters.
  `add_macro` is currently a no-op and macro invocations are not expanded.
- `[ ... ]` sub-blocks work for layout/bbox and references, but some advanced
  container coordinate interactions are still being hardened.

## Deferred to P8

- Running the full official `tests/` corpus under the diff harness.
- Fuzzing (`cargo-fuzz`) for panic-freedom.
- Error-message text parity with the C reference.

_Reason: incremental milestone delivery (see `TZ_pikchr_rust_port.md` §5)._
