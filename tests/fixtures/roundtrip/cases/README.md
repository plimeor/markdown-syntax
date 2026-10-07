# Derived Round-Trip Case Corpus

This directory holds `markdown-syntax` round-trip-stability case files derived
from upstream test sources. The tests do not execute any upstream source
directly; the upstream license text is under
`../../conformance/THIRD-PARTY-LICENSES/`.

Cases are grouped by the upstream suite their input came from, not by
upstream tool. `origin:` in each case header is a count-bucket label
(`commonmark` or `gfm`); it is no longer a path segment.

## Executable Inputs

`commonmark/` and `gfm/` hold the executable derived corpus. They contain
Markdown source arguments extracted from recognized upstream parser-facing calls
and package-owned stability inputs. Executable cases declare
`role: upstream-input`.

Every case parses under the crate's one syntax.

The executable check uses the public `markdown-syntax` boundary:

- parse Markdown source into AST
- serialize AST back to Markdown source
- parse the serialized Markdown again
- compare the public AST projection

An input that reads back only with a lazy line, whitespace, or a blank line
the AST does not record is listed, with its reason, in `NOT_READING_BACK` in
`tests/fixtures.rs`; a listed input that reads back fails the check.

## Format

Each file starts with metadata:

```text
# markdown-syntax semantic input cases v2
origin: commonmark
commit: 1506572
source: upstream-tests/html_flow.rs
role: upstream-input
count: 151
```

The `source:` value is a historical provenance identifier. The vendored upstream
sources are not present in this tree, and `source:` is not cross-checked against
an on-disk file.

Each case is length-prefixed so the Markdown body can contain arbitrary
delimiter-like text:

```text
--- case 1 bytes 17
| a |
| - |
| b |
--- end
```

The `bytes` value is the UTF-8 byte length of the case body.
