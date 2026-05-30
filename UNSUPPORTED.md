# Known differences from upstream Pikchr

The port targets behavioral parity with `pikchr.y`. Every renderable diagram in
the official `tests/` corpus (96 of them) produces SVG **byte-for-byte identical**
to the C reference, and `pikchr()` never panics (fuzz-tested). The only known
divergence:

- **`tests/test60`** — a deliberate test of *error messages inside macros* that
  redefines a macro three times. Upstream raises a "syntax error"; this port
  accepts the input and renders it. The difference is confined to an obscure
  macro-redefinition error path and does not affect any successfully rendered
  diagram.

Cosmetic, non-geometric encoding differences from the C output (e.g. attribute
ordering) are allowed by design (see `TZ_pikchr_rust_port.md` §2), but in
practice none remain on the tested corpus.
