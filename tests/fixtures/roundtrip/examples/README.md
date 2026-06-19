# CommonMark Example Inputs

This directory contains `markdown-syntax` case files derived from the
generated CommonMark official example suite (the standard CommonMark spec
examples).

`official-inputs.cases` materializes the full set of 652 official Markdown
inputs from the copied generated suite. It is retained as audit corpus. The
upstream generated source also contains expected HTML renderings, but
`markdown-syntax` does not implement or target HTML output, so those HTML
expectations are not materialized or asserted here.

`official-stable-inputs.cases` is the executable package-owned stability subset.
Those examples are checked through the public Markdown syntax boundary:

- parse CommonMark source into AST
- serialize AST back to Markdown source
- parse the serialized Markdown again
- compare the public AST projection

The copied upstream source remains under
`../oracles/upstream-tests/commonmark.rs`, and its license text under
`../oracles/THIRD-PARTY-LICENSES/`.

The file format is the same length-prefixed `.cases` format used by
`../cases/`; executable subset cases include `profile commonmark` in
their case headers.
