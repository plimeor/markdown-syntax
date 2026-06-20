# markdown-syntax

[![crates.io](https://img.shields.io/crates/v/markdown-syntax.svg)](https://crates.io/crates/markdown-syntax)
[![docs.rs](https://docs.rs/markdown-syntax/badge.svg)](https://docs.rs/markdown-syntax)
[![CI](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml/badge.svg)](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/markdown-syntax.svg)](#license)

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

The parser is tolerant by default. `parse` recognizes the maximal non-MDX
dialect (GFM plus footnotes, math, frontmatter, wikilinks, directives, and the
extra inline marks), and the verbs live on the values you hold: configure
`SyntaxOptions`, then ask the resulting `Document`.

```rust
use markdown_syntax::parse;

let output = parse("# Title\n\nHello *world*.");
let markdown = output.document.to_markdown()?;
# Ok::<(), markdown_syntax::SerializeError>(())
```

To pin a specific dialect, build a preset and call `.parse`; to tune one, chain
the `Construct` builder:

```rust
use markdown_syntax::{SyntaxOptions, Construct};

let gfm = SyntaxOptions::gfm().parse("~~done~~ https://example.com");
let no_math = SyntaxOptions::default().disable(Construct::Math).parse("price $5");
# let _ = (gfm, no_math);
```

`SyntaxOptions::{commonmark, gfm, mdx}` are the named presets; `parse(input)` is
sugar for `SyntaxOptions::default().parse(input)`. `SyntaxOptions::parse` is
infallible — a contradictory hand-built config surfaces as a diagnostic — while
`SyntaxOptions::parse_strict` promotes any error-severity diagnostic to a hard
error, and `SyntaxOptions::validate` checks a config up front. Parser
diagnostics, AST validation, and serializer errors are separate domains that
share one `Diagnostic` type.

`Document` owns the output verbs: `to_markdown` / `to_markdown_with`,
`validate`, and — under `--features html` — `to_html` / `to_html_with`. The
default HTML options validate the AST first, escape raw HTML, and filter
dangerous link/image protocols; the `html` feature also exposes `HtmlOptions`,
`HtmlError`, `SafeRawHtmlForm`, and `TasklistAttrOrder`. `use
markdown_syntax::prelude::*` imports the common surface in one line.

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
