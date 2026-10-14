# Provenance

This implementation was written clean room, in two separate steps.

1. **Specification.** The documents in `docs/spec/` were written from ITU-T Recommendation
   G.723.1 (05/2006) and the ITU-T reference software of its electronic attachment: the
   floating-point code of Annex B, with the fixed-point code for cross-checks, and the Annex
   C channel coding code. They describe the algorithms in prose and formulas and contain no
   code. `tools/extract_tables.py` copied the numeric constant tables of that software into
   `src/tables.rs`; it copies values and nothing else.
2. **Implementation.** The code in `src/` and `tests/` was written from the Recommendation
   text, the documents in `docs/spec/` and `src/tables.rs` only. Its author did not read the
   ITU-T reference software or any other implementation of G.723.1.

Questions that came up during the implementation were answered by amending the documents in
`docs/spec/`.

The ITU-T reference software and test vectors are not part of this repository. The
conformance tests download the electronic attachment from the ITU-T.
