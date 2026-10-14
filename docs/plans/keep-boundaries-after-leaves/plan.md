# Keep boundaries after leaves

## Why

Settle's editor saves a note by writing the parsed document with the
paragraphs it edited, and checks that an edit changes only the edited
leaf's bytes. A paragraph directly followed by a list or a block quote, a
common shape in notes, fails that check: editing the list writes a blank
line between them (`"aa\n- b\n"` → `"aa\n\n- bX\n"`). The writer keeps the
source between a leaf and a container only when the container's first line
is the source's, because an edited first line might no longer start the
container there. Whether a block starts on the line after a paragraph is
already one rule in the parser (`interrupts_paragraph`, which validation
uses too); asking it about the block as written keeps the gap wherever the
container still starts, and keeps the blank-line fallback where it does not.

- Scope: the source between a leaf and a container after it is kept
  whatever the container's first line became, while the container still
  starts there: after a paragraph, a definition, or a table, where it
  interrupts a paragraph; after any other leaf, always.
- Not breaking: output changes only where it now keeps more source bytes;
  the read-back check (`misread`) rewrites no more blocks than before.

## What changes

### Serialization
- `serialization` › Unchanged source is kept: modified — the boundary
  between a leaf and a container after it is kept while the container, as
  written, still starts there; the scenario covers a list after a paragraph
  and the fallback when the edited list no longer starts after it

Specs:
- `serialization` (modified)

## Out of scope
- Boundaries after a container, where the next block's first line can
  continue the container's last line; they stay as they are.
- Any change to what reads differently and falls back to canonical output.

## Tasks

### 1. Boundaries
- [x] 1.1 In `reuse::keeps_gap`, keep the gap before a container after a
  leaf whatever its first line became where it still starts there
  (`starts_after_leaf`, asking `interrupts_paragraph` after a paragraph, a
  definition, or a table); verified by `tests/serialize_source.rs`
  (`edited_items`, `a_container_after_a_leaf_keeps_the_gap_while_it_still_starts_there`,
  with no block rewritten by the read-back check) and the full
  `cargo test --profile ci --features html`
- [x] 1.2 Update the spec from `specs/serialization.md`; verified by a read
  against the code
