# Source reused per node

## Why

Writing a parsed document reuses its source only for unchanged top-level
blocks, so editing one item of a long list rewrites the whole list, and
renaming one link in a paragraph rewrites the paragraph, moving bytes nobody
edited in files that agents, git, and other editors also change.

- Scope: source reused per node and per node's own syntax, down to inlines
  in parsed leaves; a leaf built from text stays written whole as its text.
- Builds on the `editor-block-model` plan: correspondence, `BlankLine`, item
  numbers, and leaves with their own source.
- Largest risk: a node written from the source beside a rendered one reading
  back differently; met by one read-back check of the output with a fallback
  per top-level block, and by simulated edits at every depth.

## What changes

- `serialization` › Unchanged source is kept: modified — per node, per own
  syntax, down to inlines and leaves with their own source
- `serialization` › Rendering only: modified — the output is parsed once to
  check what is written partly from the source

Specs:
- `serialization` (modified)

## Out of scope
- meowdown's graded round-trip cases: neither this repo nor the project files
  hold them; the read-back corpus and the seeded generators cover the same
  shapes, and their expected outputs are meowdown's canonical form anyway.
- Spelling fields for setext underlines, closing sequences, marker gaps,
  task-marker case, and thematic-break strings — the own syntax is copied
  (`editor-block-model` design › Decisions).

## Design
See `design.md`.

## Tasks

Group 2 needs 1; group 3 needs 2. Inner loop:
`cargo test --profile ci --features html --test <name>`.

### 1. Matching and correspondence
- [ ] 1.1 Match each node through its parent and compute correspondence
  bottom-up with an exhaustive own-fields comparison, returning
  `Kept(span)` or lines marked content or full; verified by
  `tests/serialize_source.rs` cases for a split paragraph, a moved item, and
  an unedited document written as its source.
  - Follows design › Matching, Bottom-up correspondence, Line model.

### 2. Writing
- [ ] 2.1 Write blocks with copied own syntax, kept gaps, content columns
  from copied markers, and renumbered or ticked items keeping their column;
  verified by `tests/serialize_source.rs` naming `Unchanged source is kept`
  (One edited item).
- [ ] 2.2 Write an edited parsed leaf inline by inline from its inline
  source; verified by the One edited link scenario and a case that changes
  one emphasis's text inside a link.
- [ ] 2.3 Add the read-back check with its fallback per top-level block and
  to batch 1's grain; verified by a case whose copied underline would read
  differently under its rendered content.
- [ ] 2.4 Apply options only to rendered text, with new items of a list
  written from the source taking its recorded marker; verified by
  `tests/serialize_source.rs` with `bullet: Some(..)`.
- [ ] 2.5 Generalize the simulated edits to every node at every depth,
  failing on any fallback they trigger where the canonical output reads
  back; verified by
  `tests/fixtures.rs` and `tests/serialize_roundtrip_fuzz.rs`.
  - Follows design › Test scope.
- [ ] 2.6 Add writing with edits at depth to the linear-growth and
  pathological suites; verified by `tests/linear_growth.rs` and
  `tests/pathological_inputs.rs`.

### 3. Integration
- [ ] 3.1 Check a JS save that ticks one task item and keeps every other
  byte; update `docs/overview.md` and `CLAUDE.md` for per-node reuse; and
  record decision 0017, superseding 0016, with matching through the parent
  in place of first claimant wins, a node with copied own syntax as not
  rendered beside a kept gap, and the read-back check in place of "it never
  parses its output"; verified by
  `node --test 'wasm/js/test/*.test.mjs'` and
  `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.
