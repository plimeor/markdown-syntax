# Keep item markers

## Why

After keep-lines-around-edits, an edit at the end of one leaf moves bytes
outside it in ten appends of this crate's corpus
(`NOT_KEPT_AFTER_APPENDING` in `tests/fixtures.rs`) and five simulated leaf
edits (`NOT_KEPT_AROUND_A_LEAF_EDIT`), and in one file of Settle's editor
round-trip gate (`extensions/gfm_task_list.md`, the gate's last failure).
Each is a list item written canonically although its own fields did not
change, because its marker is one the item writer does not copy:

- a tab after the marker or the checkbox (`-\ta`, `* [ ]\tText.`);
- a marker whose first child starts on a later line, with a checkbox
  (`* [x]\r\n  a`, `* [ ]\na`) or a checkbox split by a line ending
  (`* [\n] a`); and the line ending after a bare marker line, written
  canonically.

- Scope: an item whose own fields are equal keeps its marker as its source
  text up to its first child, tabs and line endings included, the line its
  first child starts on keeping that line's source prefix; the gap probes
  read such a marker as written.
- Not breaking: output changes only where it now keeps more source bytes.
- Risk: a tab's width depends on the column it stands at, which a canonical
  container prefix may change; a marker is copied only where no column
  turns its whitespace into indented code, and lines the writer indents
  take the widest content column.

## What changes

### Serialization
- `serialization` › Unchanged source is kept: modified — an item whose own
  fields are equal is written with its marker's source text up to its first
  child, tabs and line endings included, the line its first child starts on
  keeping that line's source prefix; the boundary after a list whose last
  item is empty is kept only where that item does not take the sibling's
  first line

Specs:
- `serialization` (modified)

## Out of scope
- An item whose number or checkbox changed and whose marker holds a tab, or
  a checkbox on a later line: rendered, as today (`changed_marker` edits
  the marker's first line).
- A marker whose whitespace before its content holds a tab and anything
  else (`* \t[x]`): at another column it could open indented code, so the
  item renders (the one append `NOT_KEPT_AFTER_APPENDING` still lists).

## Design
See `design.md`.

## Tasks

### 1. Markers
- [x] 1.1 Copy an unchanged item's marker with a lone tab before its
  content, lines the writer indents taking the widest content column;
  verified by `serialize_source.rs` `an_item_marker_with_a_tab_is_kept`
  (a bullet, a later paragraph, a tab after a checkbox, a quoted item, an
  ordered marker, a space and a tab rendered, a new paragraph indented in
  the item at both columns)
  - Design › Tabs
- [x] 1.2 Copy an unchanged item's marker that ends on a later line than it
  starts, with the source's line endings, the first child's line keeping
  its source prefix, and a lazy first line after a checkbox only where it
  reads as a paragraph's; verified by `serialize_source.rs`
  `an_item_marker_over_lines_is_kept` (CRLF, a lazy line, a split
  checkbox, one in a quote, a bare marker line, a lazy line edited into a
  heading)
  - Design › Markers over lines
- [x] 1.3 The gap probes read a lone tab after a marker as written, and an
  empty last item as its marker; verified by the moved expectation in
  `the_gap_after_an_edited_container_stays_where_nothing_takes_the_next_line`
  and the corpus checks of 2.1
  - Design › Gap probes

### 2. Checks
- [x] 2.1 `NOT_KEPT_AROUND_A_LEAF_EDIT` is removed, `NOT_KEPT_AFTER_APPENDING`
  lists only `* \t[x]`, and the full `cargo test --profile ci --features
  html` passes
- [x] 2.2 Update the spec from `specs/serialization.md`; verified by a read
  against the code
