# serialization — spec changes

## MODIFIED Requirements

### Requirement: Unchanged source is kept
For a document with a source, `to_markdown` and `to_markdown_with` SHALL
match each node that carries a span to the node of its kind at that span in
the document `parse` returns for the source: a top-level block among the
top-level blocks, and any other node among the children of the node its
parent matched. A node corresponds to its match when their own fields are
equal and their children correspond pairwise; a leaf with its own source
(`with_source`) whose inlines are the ones `parse_inline` reads from that
source with the definitions of the document being written corresponds when
that source equals the base leaf's inline source, and is written as that
source wherever it is written, except that an ATX heading, a table cell, or
a directive label whose source holds a line ending is written from its
inlines. The writer SHALL write:

- a node that corresponds to its match, under containers that all keep their
  source prefixes, as its span's source text;
- any other matched node with the source text of its own syntax (markers,
  fences, delimiters, underlines, the table's delimiter row, and everything
  else written once per node that is not a child), when its own fields are
  equal, and its children by these rules; a list item whose number or
  checkbox changed renders them in its marker's place, keeping its content
  column where they fit;
- every other node as canonical output, with its children by these rules.

The writer SHALL keep the source between two siblings written from the
source that are adjacent in the source, where the boundary holds a
separating blank line exactly when it did there, and where the boundary
holds a blank line, has a `BlankLine` on one side, stands between two
siblings that hold no blocks, stands between one that holds no blocks and a
container after it that, as written, starts on the line after it (after a
paragraph, a definition, or a table, where it interrupts a paragraph), or
has the source's lines on both sides; which other boundaries without a
blank line are kept is unspecified. It SHALL keep
the source before the document's first block and after its last when that
block is written from the source there; every other boundary SHALL be
canonical. A node SHALL be written as canonical output even so where its
source would read differently in its place; which placements make a node's
source read differently is unspecified, except that a top-level block
written partly from the source SHALL be written as canonical output where
the parsed output holds a different block in its place. Options SHALL apply
only to the text the writer produces, line endings included, and a list
written with its own syntax from the source SHALL write a new item with the
marker it records; text and line endings copied from the source SHALL be
kept as they are. Parsing the output SHALL yield a document equal to the
written one as "Round-trip stability" compares them wherever parsing the
canonical output of the same document does.

#### Scenario: Edited items
- **WHEN** the paragraph of the second item of the document parsed from `"-   a\n-   b\n\n    c\n-   d\n"`, and the paragraph of the item of the one parsed from `"aa\n- b\n"`, are each replaced by a hand-built paragraph `Text("x")`, the item of `"aa\n- b\n"` is separately left with no children, and each document is written
- **THEN** they return `"-   a\n-   x\n\n    c\n-   d\n"`, `"aa\n- x\n"`, and `"aa\n\n-\n"`: an item keeps its marker, its gap, and its second paragraph at four columns, the other items and the boundary after the paragraph are kept, and the empty item, which would underline the paragraph, is written canonically

#### Scenario: One edited link
- **WHEN** the target of the `WikiLink` in the document parsed from ``"see  [[A]] and `` b ``"`` is changed to `B` and the document is written
- **THEN** it returns ``"see  [[B]] and `` b ``"``
