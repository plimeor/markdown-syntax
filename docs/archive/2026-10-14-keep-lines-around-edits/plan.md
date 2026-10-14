# Keep lines around edits

## Why

Settle's editor checks that an edit at the end of one leaf changes only that
leaf's bytes. Over the corpus of `tests/fixtures/roundtrip/` and Settle's
notes, 14 files fail it, every failure in this writer moving bytes the edit
did not touch (`editor-roundtrip-failures.txt` in the project files). The
same check run over this crate's corpus, one appended char per paragraph,
heading, and table cell, moves bytes in about 330 of some 3,000 appends:

- an edited leaf's later lines lose their source indentation, laziness, or
  line ending (`[wrapped](/url\n  "title")`, `> a\nb`, `[^1]: a\nb`,
  `a\r\nb`), and so do a setext underline, a table's later rows, and an
  alert's first paragraph on a lazy line;
- a blank line at a container's edge loses its own prefix (`> ` → `>`);
- a container whose last line was edited, or the sibling after it whose
  first line was, gets a blank line between them (`- a\n* b`, `> a\n---`,
  `> - a\n- b`, `> ```\na`);
- a `---` break opening the document is rewritten as `- - -` with a blank
  line whenever anything else changed.

- Scope: each of those kept from the source where it still reads the same,
  checked by an append test over the corpus; every other case keeps today's
  output.
- Not breaking: output changes only where it now keeps more source bytes.
- Risk: a kept line reading differently in its new neighbourhood; each rule
  asks the question that decides it, mostly by parsing a one- or two-line
  probe, and the read-back check stays the backstop.

## What changes

### Serialization
- `serialization` › Unchanged source is kept: modified — own syntax copied
  from the source includes each later line's prefix and line ending of an
  edited leaf where that line reads the same there, a table's later rows'
  prefixes, a setext underline's line, and a matched blank line's line; the
  boundary after or before a container is kept where the last leaf before
  it, as written, does not take the line after it; a dash break opening the
  document stays while no later line of the output closes it as frontmatter

Specs:
- `serialization` (modified)

## Out of scope
- An item whose marker is followed by a tab, or whose checkbox is split
  from its marker by a line ending: own syntax of items, still rendered
  (the ten appends `NOT_KEPT_AFTER_APPENDING` lists).
- Lines the edit added beyond the base leaf's: they take the canonical
  prefix, as today.

## Design
See `design.md`.

## Tasks

### 1. Lines
- [x] 1.1 Keep the source prefix and line ending of each later line of an
  edited leaf that the base leaf has, where it reads as that line does
  there; verified by `serialize_source.rs`
  `later_lines_of_an_edited_leaf_keep_their_source_prefix` (indentation, a
  lazy line in a quote and a footnote, CRLF, an added line, a lazy line
  that would open a list, a line where no block opens)
  - Design › Leaf lines
- [x] 1.2 Keep a setext underline's line, a table's later rows' prefixes, a
  marker line's trailing whitespace, a matched blank line's line, and an
  alert's first line from the source; verified by
  `own_lines_of_an_edited_block_keep_their_source`
  - Design › Own lines

### 2. Boundaries
- [x] 2.1 Keep the gap after or before a container where the last leaf
  before it, as written, does not take the line after it; verified by
  `the_gap_after_an_edited_container_stays_where_nothing_takes_the_next_line`
  (two lists, a quote before a break, a quoted list before a list, each
  edited on either side, a closed fence, lazy lines a quote's paragraph does
  and does not take, an empty alert, an open list) and the moved
  expectation in `a_container_after_a_leaf_keeps_the_gap_while_it_still_starts_there`
  - Design › After a container
- [x] 2.2 Keep a dash break opening the document while the output does not
  open with frontmatter; verified by
  `a_break_opening_the_document_stays_while_no_line_closes_it`
  - Design › The opening break

### 3. Checks
- [x] 3.1 `appending_to_a_leaf_changes_only_that_leaf` in `tests/fixtures.rs`
  passes with only the out-of-scope appends listed; the stale entries of
  `NOT_KEPT_AROUND_A_LEAF_EDIT` and the dash-break rule of
  `tests/support/edits.rs` follow the change; the full
  `cargo test --profile ci --features html` passes
- [x] 3.2 Update the spec from `specs/serialization.md`; verified by a read
  against the code
