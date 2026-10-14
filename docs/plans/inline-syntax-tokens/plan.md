# Inline syntax tokens

## Why

An editor that keeps a leaf's Markdown as its text (meowdown's model, which
Settle's editor follows) styles that text from the parse: which bytes are
syntax characters and which are a link's destination, its title, a wiki
link's target, or code. The inline tree gives each node one span and stores
destinations, titles, targets, and code as decoded strings, so the editor
would have to restate the syntax's rules in JavaScript to find those bytes,
and the two readings would drift.

- Scope: `parse_inline` also returns tokens that tile its text, each naming
  the innermost node that holds it and the part of that node it is, and the
  inlines in the order tokens name them; the wasm binding carries both with
  UTF-16 spans.
- Breaking: `InlineOutput` gains public fields and becomes
  `#[non_exhaustive]`. The analysis behind this plan
  (`editor-marking-gap.md` in the project files, §4) expected an additive
  change; a field is the shape that keeps tokens bound to the text they
  describe (Decisions), and the break is taken once.

## What changes

### Parsing
- `public-api` › Inline parse: modified — returns `tokens` and is
  `#[non_exhaustive]`; the spans clause cites "Source spans", the
  requirement that states nesting
- `public-api` › Inline syntax tokens: added

### WebAssembly binding
- `wasm-binding` › Inline parse and inline source from JavaScript: modified
  — `parseInline` returns the tokens, with UTF-16 spans, and the nodes in
  the order the tokens name them

### Outside the specs
- `index.d.ts` gains the token type.
- `Inline::children()` lists every kind, so a new kind cannot fall through.

Specs:
- `public-api` (modified)
- `wasm-binding` (modified)

## Out of scope
- Tokens for block syntax (`#`, `>`, `- `, fences) — the editor keeps block
  markers out of its text; they come back only if Settle decides to show
  them as text.
- Tokens on `parse`'s document, its leaf sources, or `InlineSource` — the
  editor reads a leaf's tokens by calling `parse_inline` on its text, which
  `leaf_sources` gives.
- Span fields on AST nodes for each part — turned down in Decisions.
- The editor's mark vocabulary, units, and show/hide rules — Settle's.

## Design
See `design.md`.

## Tasks

### 1. Tokens
- [ ] 1.1 Add `InlineToken { span, node, role }`, `TokenRole`, and
  `InlineOutput::nodes()`, exported from the crate root and the prelude;
  make `InlineOutput` `#[non_exhaustive]` and `Inline::children()`
  exhaustive; verified by `cargo doc` and a test that names them through
  `prelude`
- [ ] 1.2 Compute the tokens in one pass over the inline tree and the
  content the parse read, before the `\r\n` mapping and the NUL rewrite,
  and return them from `parse_inline`; internal callers that read only the
  inlines take a crate-private entry without the pass; verified by
  `tests/inline_tokens.rs`: one case per node kind and per part, an empty
  part, padding that is a line ending, a `\r\n` title, a NUL in a
  destination, and nested containers through an image's alt and a
  directive's label
  - Design › Token pass
- [ ] 1.3 Check the tokens of every corpus leaf and of seeded inline text:
  they tile the text in order; each lies within its node's span and
  outside that node's children; a container's children tile its content;
  and each part reads back as its field (Design › Read-back of parts);
  verified by `tests/inline_tokens.rs`
- [ ] 1.4 Keep the pass linear and stack-bounded: `parse_inline` in
  `tests/linear_growth.rs` and the deep inline cases of
  `tests/pathological_inputs.rs` include it; verified by those suites

### 2. WebAssembly binding
- [ ] 2.1 Carry the tokens on the wire and return them from `parseInline`
  as `{ span, node, role }` with UTF-16 spans and the role in camelCase,
  with `nodes` holding the decoded inline objects in pre-order; verified by
  a unit test of the token encoding in `wasm/src/wire.rs` and a Node test
  that marks a link's destination and title in text holding a non-BMP
  character
- [ ] 2.2 Add the token type and `nodes` to `index.d.ts`; verified by the
  Node test suite and a read of the declaration
