# The block model of an editor, and source reused per node

## Why

Writing a parsed document reuses its source only for unchanged top-level
blocks, so editing one item of a long list rewrites the whole list, and the
tree cannot express what a ProseMirror editor holds: blank lines the user
typed, a paragraph's text as the user typed it, the number on each ordered
item. Loading every leaf's text costs one block re-parse per leaf, 8.2 s for
a 2000-item list.

- Scope: source reused per node and per node's own syntax, down to inlines;
  `BlankLine` nodes; ordered item numbers; leaves built from text that keep
  it; `leaf_sources` and inline sources for table cells and directive labels;
  the wasm binding for all of it.
- Decision 0013 is superseded by 0016.
- Breaking: `Block::BlankLine`, `ListItem::number`, `List::start` removed,
  `\|` in a table cell reads as text, and parsed documents with extra blank
  lines hold `BlankLine` nodes.
- Largest risk: a node written from the source beside a rendered one reading
  back differently; met by simulated edits at every depth.
- Depends on the `inline-tags` plan only through decision numbering.

## What changes

### Parsing and the AST
- `block-syntax` › Blank lines are nodes: added
- `public-api` › Leaf sources: added
- `public-api` › Leaves built from their source: added
- `public-api` › Recorded syntax forms: modified — an ordered item records its
  number, which replaces `List::start`
- `public-api` › Inline source of a leaf block: modified — answers for table
  cells and directive labels
- `public-api` › Hand construction: modified — `ListItem::new` and
  `BlankLine::new`
- `inline-syntax` › Escapes and character references are nodes: modified —
  `\|` in a table cell reads as text
- `untrusted-input-cost` › Linear time: modified — covers `leaf_sources` and
  the `with_source` constructors

### Writing
- `serialization` › Unchanged source is kept: modified — per node, per own
  syntax, down to inlines and leaves with their own source
- `serialization` › Canonical output: modified — numbers as recorded, blank
  lines as their prefix without trailing whitespace
- `serialization` › Round-trip stability: modified — a blank-line count is
  now recorded, so it leaves the list of unrecorded details
- `serialization` › Table cells encoded by one rule: modified — its scenario
  reads the pipe back as text
- `validation` › Shapes that cannot be written: modified — `BlankLine` where
  no blank line can be written, item numbers; an empty paragraph stays
  invalid

### WebAssembly binding
- `wasm-binding` › Parse from JavaScript: modified — returns every leaf's
  inline source
- `wasm-binding` › Write from JavaScript: modified — leaves may carry
  `source`

### Outside the specs
- Decision 0016 "The source is the only truth" supersedes 0013: reuse per
  node and per own syntax, leaves that keep their text, `BlankLine`, and
  fields only for values a tree without a source must carry.
- `docs/overview.md`'s Current focus moves to the Settle editor's adapter.
- Settle (another repository): the editor adapter (flat lists, empty
  paragraphs, spans as attributes, renumbering) and the index's upgrade.

Specs:
- `public-api` (modified)
- `block-syntax` (modified)
- `inline-syntax` (modified)
- `serialization` (modified)
- `validation` (modified)
- `untrusted-input-cost` (modified)
- `wasm-binding` (modified)

## Out of scope
- `#tag` — the `inline-tags` plan.
- Spelling fields for setext underlines, closing sequences, marker gaps,
  task-marker case, and thematic-break strings — the source keeps them
  (design › Decisions).
- A flat list AST — Settle's adapter flattens (design › Decisions).
- An incremental block parse entry point, and a round-trip grading API like
  meowdown's `checkRoundTrip` — the editor builds block structure with input
  rules and parses pastes with `parse`; an unedited document writes its
  source by construction.
- Publishing `markdown-syntax-wasm` or an npm package — unchanged from
  batch 1.

## Design
See `design.md`.

## Tasks

Groups 2–4 change the AST and each regenerates the goldens it moves, read in
review (0014). Group 5 needs 2–4; group 6 needs 5; group 7 needs 6. Inner
loop: `cargo test --profile ci --features html --test <name>`.

### 1. Decision
- [ ] 1.1 Write decision 0016 superseding 0013, `git mv` 0013 to
  `docs/decisions/superseded/`, and update links outside the archive;
  verified by `rg -n '0013' docs CLAUDE.md README.md` showing only
  superseded, archived, or updated references.

### 2. Blank lines
- [ ] 2.1 Add `BlankLine` and produce it by the blank-line table; verified by
  new `.ast` goldens under `tests/fixtures/roundtrip/` for each row of
  design › Blank lines.
- [ ] 2.2 Write and validate `BlankLine`; verified by
  `tests/serialize_regressions.rs` and `tests/validate_regressions.rs`, and
  the seeded round trip generating blank-line runs.
- [ ] 2.3 Write a blank line in a list item without trailing whitespace;
  verified by a `serialize_regressions.rs` case for `- a\n\n  b`.

### 3. Ordered item numbers
- [ ] 3.1 Replace `List::start` with `ListItem::number`, parse, write, and
  validate it; verified by goldens and `serialize_regressions.rs` cases for
  `1. 1. 1.`, `3. 7.`, and an item without a number.

### 4. Leaf sources
- [ ] 4.1 Read `\|` in a cell as text; verified by the cell goldens and
  `parse_span_contract.rs`.
- [ ] 4.2 Add `Document::leaf_sources` and extend `inline_source` to cells and
  directive labels; verified by `tests/inline_source.rs` and the
  reproduction check of batch 1 extended to every leaf kind.
- [ ] 4.3 Add `with_source` constructors and their kept text; verified by
  `tests/inline_source.rs` rebuilding every corpus leaf from its text and
  writing the source back.
- [ ] 4.4 Add `leaf_sources` and `with_source` to the linear-growth suite;
  verified by `tests/linear_growth.rs`, with a 2000-item list.

### 5. Writing per node
- [ ] 5.1 Build the syntax map in the base parse; verified by a unit test
  that every base node's own-syntax ranges, children, and gaps tile its span.
  - Follows design › Writing.
- [ ] 5.2 Match nodes to the base by span with first-claimant wins; verified
  by `tests/serialize_source.rs` cases for a split paragraph and a split
  list item.
- [ ] 5.3 Write blocks by the four rules and the gap rule, with batch 1's
  guards moved to every container; verified by `tests/serialize_source.rs`
  naming `Unchanged source is kept`.
- [ ] 5.4 Write inlines by the same rules inside an edited leaf; verified by a
  case that changes one `WikiLink` in a paragraph and keeps every other byte.
- [ ] 5.5 Generalize the simulated edits to every node at every depth;
  verified by `tests/fixtures.rs` and `tests/serialize_roundtrip_fuzz.rs`.
  - Follows design › Test scope.
- [ ] 5.6 Port meowdown's graded round-trip and blank-line cases; verified by
  `tests/serialize_source.rs`.
- [ ] 5.7 Add writing with edits at depth to the linear-growth and
  pathological suites; verified by `tests/linear_growth.rs` and
  `tests/pathological_inputs.rs`.

### 6. Binding
- [ ] 6.1 Return leaf sources from `parse`, accept `source` leaves in
  `toMarkdown`, and carry `blankLine` and item numbers on the wire; verified
  by `cargo test -p markdown-syntax-wasm` and
  `node --test 'wasm/js/test/*.test.mjs'` naming each changed requirement.
- [ ] 6.2 Run `wasm/js/bench.mjs` with a load that reads every leaf source;
  verified by its output, observed and not stored, showing a 124 KB load
  under 16 ms.

### 7. Integration
- [ ] 7.1 Update `docs/overview.md`, `CLAUDE.md`, and `README.md` for the
  block model and per-node reuse; verified by
  `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` and the README doc-test.
