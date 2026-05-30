# Unsupported / not-yet-ported constructs

Per the porting spec, the Definition of Done is behavioral parity with `pikchr.y`
across the official `tests/` corpus. The port is incremental (milestones P0–P8);
anything deferred is tracked here with a reason. On completion this file should
be empty.

## Deferred to P8 (stabilization)

- Running the **full official `tests/` corpus** under the diff harness (the
  current corpora are curated subsets, all byte-for-byte vs the C reference).
- Fuzzing (`cargo-fuzz`) for panic-freedom on arbitrary/hostile input.
- Error-message text parity with the C reference (we surface a message +
  line/column; the exact wording is not yet matched everywhere).

_Reason: incremental milestone delivery (see `TZ_pikchr_rust_port.md` §5)._
