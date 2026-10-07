# CommonMark Example Inputs

This directory contains `markdown-syntax` case files derived from the
generated CommonMark official example suite (the standard CommonMark spec
examples).

`official-stable-inputs.cases` is the executable package-owned stability subset
derived from the generated suite. Those examples are inputs of the source
read-back test in `tests/fixtures.rs`, which checks them through the public
Markdown syntax boundary:

- parse CommonMark source into AST
- serialize AST back to Markdown source
- parse the serialized Markdown again
- compare the two trees, and the Markdown the second serializes to with the
  first

The copied upstream Rust source is not present in this tree. The full
CommonMark expected-HTML suite is checked by the AST-to-HTML conformance bench
under `../../conformance/`. License text for the upstream source snapshots is
under `../../conformance/THIRD-PARTY-LICENSES/`.

The file format is the same length-prefixed `.cases` format used by
`../cases/`: each case header is `--- case N bytes B`, preceded by a comment
line naming the spec section the example comes from.
