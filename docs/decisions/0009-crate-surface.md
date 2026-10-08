# 0009: Crate surface

Status: Accepted
Date: 2026-10-08
Supersedes: 0001, 0002, 0005

## Context

The crate's core job is Markdown source to AST and AST to canonical Markdown.
Keeping that surface dependency-free and feature-minimal makes default builds
predictable for embedded, wasm, and library consumers that only need syntax
structure. The crate is published to crates.io, so the build surface is also a
promise to downstream users.

HTML output is observable behavior with a security edge (raw HTML and
dangerous protocols). Keeping one public renderer, the one the conformance
bench exercises, avoids drift between a test renderer and a shipped one.

Callers meet the crate through a small surface: the common path should cost
one concept, the configured path should read as intent, and no redundant or
internal item should greet them.

The test fixture corpus vendors third-party CommonMark, GFM, comrak, and
markdown-rs cases; redistributing it would need a license and provenance
review of every bundled case.

## Decision

The default build is the syntax-core surface:

- empty default feature set;
- zero runtime dependencies;
- `#![no_std]` with `alloc`;
- MSRV 1.82.

AST-to-HTML rendering lives in the crate behind the non-default `html`
feature; the default build exports no renderer. The renderer is
safe-by-default: it validates the AST first, escapes or omits raw HTML, and
blanks dangerous link, image, and wiki-link protocols unless configured
otherwise. It does not build a DOM, highlight syntax, or offer pluggable
sanitization. Parser, AST, and serializer behavior do not change merely to
support HTML output.

The public API around `parse`:

- **Output on `Document`.** After `parse`, the caller asks the document:
  `to_markdown()`, `to_markdown_with(&SerializeOptions)`, `to_html()` and
  `to_html_with(&HtmlOptions)` behind the `html` feature, and
  `validate() -> Vec<Diagnostic>`. There are no free output functions and no
  `*_with_options` names.
- **One diagnostic type.** `Diagnostic` serves parse, validate, serialize, and
  render; its `span` is an `Option<Span>`, and `DiagnosticCode::InvalidDocument`
  marks a tree that has no spelling. `SerializeError` and `HtmlError` carry
  diagnostics.
- **AST ergonomics.** `Block` and `Inline` give `meta()` and `span()`, and
  `Inline::children()` reads the inline content of any inline node, image alt
  included; block children stay match-based. Nodes are built with
  `From<&str>` and `From<String>` for `Text`, `From<node>` for `Block` and
  `Inline`, and `new(..)` on the common nodes, each defaulting `meta`.
  `NodeMeta` and the one-struct-per-variant enums stay, so every node is a
  nameable type.
- **Exports.** Render internals are crate-private; the crate root re-exports an
  explicit list beside the `ast::*` glob, and `prelude` is the recommended
  one-line import. `ParseOutput` is not generic.

The published package ships only the library source, `Cargo.toml`, the
README, the changelog, and the licenses (the `include` list in `Cargo.toml`).
The vendored fixture corpus stays in the repository as dev-only material.

The public contract for this surface lives in `docs/specs/public-api.md`,
`docs/specs/html-rendering.md`, `Cargo.toml`, and the crate root.

## Considered options

- Default-on optional capabilities, or a default-on renderer: they silently
  widen the build surface, and the renderer brings the HTML/XSS-emitting
  surface to every parse/serialize consumer.
- Runtime dependencies in the syntax core: they weaken the zero-dependency
  and `no_std + alloc` boundary.
- A sibling `markdown-html` crate, or a test-only renderer beside a public one:
  the bench would depend on a downstream crate or keep a second renderer, and
  the two drift.
- Publishing the fixture corpus: it would redistribute third-party cases
  without a per-case license and provenance review.
- Output functions beside the document, or `*_with_options` names: the
  document is the value in hand on output, and the suffix regrows the
  surface.
- A builder module for every node, or a typed `Block::children()`: too much
  surface for a secondary use, and block children differ in kind.

## Consequences

- Opt-in features remain allowed.
- Tests run from the repository, not from the published package.
- No sanitization plugins, custom allowlists, syntax highlighting, or table of
  contents generation.
