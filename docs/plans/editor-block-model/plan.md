# The block model of an editor

## Why

The tree cannot express what a ProseMirror editor holds: blank lines the user
typed, a paragraph's text as the user typed it, the number on each ordered
item. Loading every leaf's text costs one block re-parse per leaf, 8.2 s for
a 2000-item list.

- Scope: `BlankLine` nodes; ordered item numbers; leaves built from text that
  keep it; `leaf_sources` and inline sources for table cells and directive
  labels; the wasm binding for all of it.
- Decision 0013 is superseded by 0016.
- Breaking: `Block::BlankLine`, `ListItem::number`, `List::start` removed,
  `\|` in a table cell reads as text, and parsed documents with extra blank
  lines hold `BlankLine` nodes.
- Largest risk: a write that drops or moves a blank line; met by the
  blank-line goldens, the seeded round trip, and source read-back.

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
- `serialization` › Unchanged source is kept: modified — a node corresponds
  to its base when its leaves with their own source hold the base leaves'
  text, and such a leaf is written as its text
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
- Source reused below top-level blocks, down to inlines — the
  `per-node-reuse` plan, which builds on this one.
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
review (0014). Group 5 needs 4; group 6 needs 2–5; group 7 needs 6. Inner
loop: `cargo test --profile ci --features html --test <name>`.

### 1. Decision
- [x] 1.1 Write decision 0016 superseding 0013, `git mv` 0013 to
  `docs/decisions/superseded/`, and update links outside the archive;
  verified by `rg -n '0013' docs CLAUDE.md README.md` showing only
  superseded, archived, or updated references.

### 2. Blank lines
- [ ] 2.1 Add `BlankLine` and produce it by the blank-line table, rendering
  nothing for it in HTML; verified by new `.ast` goldens under
  `tests/fixtures/roundtrip/` for each row of design › Blank lines, and the
  conformance bench passing every case it passes today.
- [ ] 2.2 Write and validate `BlankLine`; verified by
  `tests/serialize_regressions.rs` and `tests/validate_regressions.rs`, and
  the seeded round trip generating blank-line runs.
- [ ] 2.3 Write a blank line in a list item without trailing whitespace;
  verified by a `serialize_regressions.rs` case for `- a\n\n  b`.

### 3. Ordered item numbers
- [ ] 3.1 Replace `List::start` with `ListItem::number`, parse, write, and
  validate it, and take the HTML `start` from the first item; verified by
  goldens, `serialize_regressions.rs` cases for `1. 1. 1.`, `3. 7.`, and an
  item without a number, and the conformance bench.

### 4. Leaf sources
- [ ] 4.1 Read `\|` in a cell as text; verified by the cell goldens and
  `parse_span_contract.rs`.
- [ ] 4.2 Add `Document::leaf_sources` and extend `inline_source` to cells and
  directive labels; verified by `tests/inline_source.rs` comparing
  `leaf_sources` with `inline_source` and with the parse for every corpus
  leaf.
- [ ] 4.3 Add `with_source` constructors and their kept text; verified by
  `tests/inline_source.rs` building each kind from text.
- [ ] 4.4 Add `leaf_sources` and `with_source` to the linear-growth suite;
  verified by `tests/linear_growth.rs`, with a 2000-item list.

### 5. Writing leaves with their source
- [ ] 5.1 Match a top-level block to its base by correspondence, and write a
  leaf with its own source as that source; verified by
  `tests/serialize_source.rs` rebuilding every corpus leaf with
  `with_source` from `leaf_sources` and writing the document's source, and
  by a case editing one inline of a leaf built from text.
  - Follows design › Correspondence.

### 6. Binding
- [ ] 6.1 Return leaf sources from `parse`, accept `source` leaves in
  `toMarkdown`, and carry `blankLine` and item numbers on the wire; verified
  by `cargo test -p markdown-syntax-wasm` and
  `node --test 'wasm/js/test/*.test.mjs'` naming each changed requirement,
  with a save that sends every leaf as `source` and returns the source.
- [ ] 6.2 Run `wasm/js/bench.mjs` with a load that reads every leaf source;
  verified by its output, observed and not stored, showing a 124 KB load
  under 16 ms.

### 7. Integration
- [ ] 7.1 Update `docs/overview.md`, `CLAUDE.md`, and `README.md` for the
  block model; verified by `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`
  and the README doc-test.
