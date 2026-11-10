# Benchmark corpus

`gen.mjs` writes the corpus into `out/` (not checked in): the two documents
vendored here, this repository's docs, and seeded synthetic documents.

| File | Source | License |
|---|---|---|
| `spec.md` | The CommonMark spec, version 0.31.2, by John MacFarlane | CC-BY-SA 4.0, as its front matter states |
| `rust-releases.md` | `RELEASES.md` of rust-lang/rust, through version 1.82.0 | MIT OR Apache-2.0 |

Both are inputs to measure against only; nothing in the crate is built from
them, and the published crate does not include `tools/`.
