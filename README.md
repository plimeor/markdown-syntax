# markdown-syntax

`markdown-syntax` is a `no_std + alloc` Rust crate for Markdown syntax:

- Markdown source to Markdown AST.
- Markdown AST to canonical Markdown source.
- With the opt-in `html` feature, Markdown AST to safe-by-default HTML.
- Raw HTML is represented as Markdown syntax nodes; the opt-in HTML renderer
  escapes or omits it by default unless explicitly configured otherwise.
- Pluggable sanitization policies, editor models, DOM semantics, and product
  behavior are outside the default crate surface.

## Design

The public API is AST-first. Parser event streams and block operations are
private implementation details, not v1 compatibility surfaces.

The AST is an owned enum tree using `alloc` types. Source locations are optional
half-open byte spans into the original input. Human-readable line and column
positions are derived with `LineIndex`.

The parser is tolerant by default:

```rust
use markdown_syntax::parse;

let output = parse("# Title\n\nHello *world*.");
let markdown = markdown_syntax::to_markdown(&output.document)?;
# Ok::<(), markdown_syntax::SerializeError>(())
```

`parse_strict_with_options` promotes configured extension diagnostics to a hard
error. Parser diagnostics, AST validation diagnostics, and serializer errors are
separate domains.

When built with `--features html`, the crate also exposes `to_html`,
`to_html_with_options`, `HtmlOptions`, `HtmlError`, `SafeRawHtmlForm`, and
`TasklistAttrOrder`. The default HTML options validate the AST first, escape raw
HTML, and filter dangerous link/image protocols.

## Syntax Scope

The first implementation is a vertical slice, not a full CommonMark claim. It
defines the public AST and implements core block/inline parsing, canonical
serialization, directives, raw HTML syntax nodes, tables, strict inline math,
frontmatter, GFM alerts, wikilinks, underline, subscript, superscript, spoiler,
description lists, and raw MDX syntax nodes where enabled.

Unsupported nodes and validated-invalid AST shapes fail serialization with
structured errors. Validation is intentionally conservative and does not claim to
prove every semantic invariant of a hand-written AST.
The serializer does not perform HTML safety filtering and does not preserve the
original byte-for-byte authoring style from a bare AST.
The HTML renderer evaluates no MDX and performs no syntax highlighting or DOM
post-processing.

## Fixture Corpus

The test corpus is split by role: `tests/fixtures/roundtrip/` holds
package-owned round-trip snapshots (Markdown source → AST → Markdown source
stability), and `tests/fixtures/conformance/` holds fixtures derived from the
CommonMark specification and other reference Markdown implementations, used to
measure AST correctness against expected HTML. Derived fixtures are exercised
through this crate's public stability boundary; with `--features html`, the
HTML conformance bench also exercises the public renderer against those
expected-HTML oracles. Attribution for the vendored conformance suites lives in
`tests/fixtures/conformance/THIRD-PARTY-LICENSES/`.

This package is currently marked `publish = false`; the fixture corpus is kept
in Cargo package snapshots for local test and audit coverage. Publishing needs
an explicit package boundary and fixture-corpus license/provenance review first.

## no_std Boundary

The crate root uses `#![no_std]` and `extern crate alloc`. Default features are
empty. The opt-in `html` feature also stays `no_std + alloc`. The package has
no runtime dependencies.

## Rust Version

The minimum supported Rust version is 1.82.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
