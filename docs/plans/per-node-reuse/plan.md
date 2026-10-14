# Source reused per node

## Why

Writing a parsed document reuses its source only for unchanged top-level
blocks, so editing one item of a long list rewrites the whole list, and
renaming one link in a paragraph rewrites the paragraph, moving bytes nobody
edited in files that agents, git, and other editors also change.

- Scope: source reused per node and per node's own syntax, down to inlines,
  in parsed leaves and in leaves built from text.
- Builds on the `editor-block-model` plan: correspondence, `BlankLine`, item
  numbers, and leaves with their own source.
- Largest risk: a node written from the source beside a rendered one reading
  back differently; met by simulated edits at every depth.

## What changes

- `serialization` › Unchanged source is kept: modified — per node, per own
  syntax, down to inlines and leaves with their own source

Specs:
- `serialization` (modified)

## Out of scope
- Spelling fields for setext underlines, closing sequences, marker gaps,
  task-marker case, and thematic-break strings — the own syntax is copied
  (`editor-block-model` design › Decisions).

## Design
See `design.md`.

## Tasks

Group 2 needs 1; group 3 needs 2. Inner loop:
`cargo test --profile ci --features html --test <name>`.

### 1. The base and its syntax map
- [ ] 1.1 Build the syntax map from the base parse; verified by a unit test
  that every base node's one-time syntax, children, and gaps tile its span.
  - Follows design › Base and own syntax.
- [ ] 1.2 Match nodes to the base by span with first-claimant wins; verified
  by `tests/serialize_source.rs` cases for a split paragraph and a split
  list item.

### 2. Writing
- [ ] 2.1 Write blocks by the four rules and the gap rule, with batch 1's
  guards moved to every container; verified by `tests/serialize_source.rs`
  naming `Unchanged source is kept`.
- [ ] 2.2 Write inlines by the same rules inside an edited leaf, parsed or
  built from text; verified by a case that changes one `WikiLink` in a
  paragraph and keeps every other byte.
- [ ] 2.3 Generalize the simulated edits to every node at every depth;
  verified by `tests/fixtures.rs` and `tests/serialize_roundtrip_fuzz.rs`.
  - Follows design › Test scope.
- [ ] 2.4 Port meowdown's graded round-trip and blank-line cases; verified by
  `tests/serialize_source.rs`.
- [ ] 2.5 Add writing with edits at depth to the linear-growth and
  pathological suites; verified by `tests/linear_growth.rs` and
  `tests/pathological_inputs.rs`.

### 3. Integration
- [ ] 3.1 Check a JS save that edits one list item's marker only and keeps
  every other byte, and update `docs/overview.md` and `CLAUDE.md` for
  per-node reuse; verified by `node --test 'wasm/js/test/*.test.mjs'` and
  `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.
