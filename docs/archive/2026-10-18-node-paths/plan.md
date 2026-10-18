# Node paths for diagnostics, and block children

## Why

An editor that builds a tree and is refused cannot tell which node a
validation diagnostic is about when that node has no span, so Settle retries
the write with each tight list made loose until fewer diagnostics come back.
And every caller that walks blocks writes its own match over the container
kinds, which silently skips a container kind added later.

- Scope: a `path` on every diagnostic validation reports, carried by the wasm
  binding; `Block::children()`.
- Breaking: `Diagnostic` gains a public field, and it is not
  `#[non_exhaustive]`, so a struct literal of it no longer compiles.

## What changes

### Diagnostics
- `public-api` › One diagnostic type: modified — `path: Option<NodePath>`
- `validation` › Validate reports invalid shapes: modified — each diagnostic
  carries the path of the node whose shape it rejects
- `wasm-binding` › Write from JavaScript: modified — a refusal's diagnostics
  carry their paths

### Tree walks
- `public-api` › Children of a node: added — `Inline::children()` and
  `Block::children()`
- `public-api` › Shortcode glyph: removed — merged into "Each written fact is
  stored once", which already holds the other values derived from a node's
  fields, to keep the spec within its budget
- `public-api` › Each written fact is stored once: modified — gains
  `Shortcode::glyph()`

### Outside the specs
- Settle (another repository), once this ships in a published release:
  `save()` writes loose exactly the tight lists a refusal's paths fall in,
  replacing its retry loop; the two `child_blocks` helpers in its core crate
  call `Block::children()`.

Specs:
- `public-api` (modified)
- `validation` (modified)
- `wasm-binding` (modified)

## Out of scope
- Settle's `inline_runs` — it reads each leaf's inlines, including an HTML
  container's, which `leaf_sources` (text, and no HTML container) does not
  replace; it stays in Settle.
- Paths on parser diagnostics — each already carries a span into the source
  it was read from, which addresses it.

## Design

### Which node a path names

A validation diagnostic's `path` is the `NodePath` of the innermost node a
path can name that the rejected shape belongs to:

- a shape of a block, list item, table row, or table cell: that node, a
  misplaced `BlankLine` included;
- a shape among sibling blocks (two adjacent lists with the same marker, a
  block that cannot follow a paragraph in a tight item): the later block;
- a shape in inline content: the leaf holding it, and for a directive label
  the directive, as `inline_source` addresses leaves.

A caller finds the enclosing list by walking up the path.

### Architecture

- `Diagnostic` gains `path: Option<NodePath>` after `span`. Parser
  diagnostics and `Diagnostic::new` / `Diagnostic::invalid` leave it `None`;
  the validator sets it. Serializer and HTML pre-validation already return
  the validator's diagnostics, so they carry paths with no change of their
  own.
- The validator keeps the current path while it descends and attaches it
  to each diagnostic it pushes.
- `Block::children(&self) -> &[Block]`: the blocks directly inside a block
  quote, an alert, a footnote definition, a container directive, or an HTML
  container holding blocks; an empty slice for every other block. A list's
  blocks are in its items (`ListItem::children`), and a table holds rows, as
  `NodePath` counts them.
- wasm: the diagnostic record on the wire gains its path; the JS
  `Diagnostic` gains `path: number[] | null`.

### Decisions
- The path rides on the diagnostic — because every place diagnostics flow
  (`SerializeError`, `HtmlError`, the wasm wire) then carries it unchanged.
  Turned down: a separate list of paths beside the diagnostics, which every
  carrier would have to keep in step.
- `Block::children()` returns block slices only — because callers walking
  blocks treat items and rows apart anyway (Settle's both do). Turned down:
  a node enum over blocks, items, rows, and cells, a new type for one
  walk shape.

## Tasks

Release: the change ships as `feat!`. The Settle change in What changes
follows in Settle once a published release contains this one.

### 1. Core crate
- [x] 1.1 `Diagnostic` holds `path`, and every validation diagnostic carries the path its shape belongs to; verified by `tests/validate_regressions.rs` cases naming `validation: Validate reports invalid shapes`: a hand-built tight item with two paragraphs (path of the second), two adjacent same-marker lists (path of the second), a table row of the wrong width, a `LineBreak` last in a nested paragraph (path of the paragraph), and a directive label (path of the directive).
  - Follows Design › Which node a path names.
- [x] 1.2 `to_markdown` refusals carry the same paths, and parser diagnostics none; verified by a `validate_regressions` case naming `public-api: One diagnostic type` with a span-less tree.
- [x] 1.3 `Block::children()` exists; verified by a `parse_block_regressions` test naming `public-api: Children of a node` over each container kind, a list, and a leaf.
- [x] 1.4 The test comment naming `public-api: Shortcode glyph` names `public-api: Each written fact is stored once`; verified by `grep`.

### 2. wasm binding
- [x] 2.1 The wire and the JS wrapper carry `path`, and `index.d.ts` declares it; verified by a `binding.test.mjs` case where `toMarkdown` refuses a hand-built tree and the diagnostic's `path` names the node.

### 3. Integration
- [x] 3.1 Every CI command in `CLAUDE.md` › Commands passes locally.

