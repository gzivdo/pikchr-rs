# Unsupported / not-yet-ported constructs

Per the porting spec, the Definition of Done is behavioral parity with `pikchr.y`
across the official `tests/` corpus. The port is incremental (milestones P0–P8);
anything deferred is tracked here with a reason. On completion this file should
be empty.

## Deferred to P5 (positioning & object references)

These parse, but currently raise a clear "not yet implemented (P5)" error or are
only partially handled:

- Object references that resolve other objects: `last`, `2nd`, `previous`,
  `Name.Sub`, `N-th [class]`, `... of/in ...` (`find_nth`).
- `from` / `to` on lines, `then heading`, `go ... heading`, `... until even
  with ...`, `same` / `same as`, `N-th vertex of`.
- Full `at`/`with` edge placement is only partially handled (center/basic).
- `chop` auto-trimming against target objects.

## Deferred to P6 (object & text fidelity)

- `arc` and curved `spline` rendering (currently straight polylines).
- Exact text metrics (per-character width table `awChar`) — text auto-fit and
  bounding boxes use an approximation, so geometry for text-heavy objects may
  differ from the C reference.
- Full text vertical layout (`above`/`below`/`center` slotting) and the
  `fill`/`stroke`/baseline cosmetic attributes on `<text>`.

## Deferred to P7

- `[ ... ]` sub-blocks beyond basic bounding-box init.
- `define` macros (text substitution at tokenize time) and `$1..$9` params.

_Reason: incremental milestone delivery (see `TZ_pikchr_rust_port.md` §5)._
