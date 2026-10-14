# Serialization — spec changes

## MODIFIED Requirements

### Requirement: Rendering only
`Document::to_markdown` SHALL produce the text of each node it does not
write from the source from that node, its kept text, and the options alone,
by fixed rules; it SHALL parse its output only to check the nodes it writes
partly from the source (see "Unchanged source is kept"), and SHALL NOT parse
it for a document without a source; when the output would read back as a
different tree, it SHALL still return it. It SHALL write each `Text` value as
it is, each `Escape` as a backslash and its char, each `CharacterReference`
as its reference, and raw HTML node values as they are, without safety
filtering, and SHALL add no escape of its own, apart from the backslashes a
table cell's encoding adds before pipes.

#### Scenario: Text that reads as syntax
- **WHEN** a hand-built paragraph holding `Text("==a==")` is serialized
- **THEN** `to_markdown()` returns `"==a==\n"`, which reads back as a `Mark`

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
siblings that hold no blocks, or has the source's lines on both sides; which
other boundaries without a blank line are kept is unspecified. It SHALL keep
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
kept as they are. Parsing the output SHALL yield a
document equal to the written one as "Round-trip stability" compares them
wherever parsing the canonical output of the same document does.

#### Scenario: One edited block
- **WHEN** the second block of the document parsed from `"|a|b|\n|-|-|\n\n*  x\n\n\n# T #"` is replaced by a hand-built paragraph `Text("y")` and the document is written
- **THEN** it returns `"|a|b|\n|-|-|\n\ny\n\n\n# T #"`: the table, the blank line, the heading, and the missing final newline are kept, and the gap before the new paragraph is canonical

#### Scenario: Leaves given as text
- **WHEN** every leaf of the document parsed from `"- a  *b*\n\n  c\n"` is replaced by the leaf `with_source` builds from its `leaf_sources` text, with its span, and the document is written
- **THEN** it returns the source

#### Scenario: One edited item
- **WHEN** the paragraph of the second item of the document parsed from `"-   a\n-   b\n\n    c\n-   d\n"` is replaced by a hand-built paragraph `Text("x")` and the document is written
- **THEN** it returns `"-   a\n-   x\n\n    c\n-   d\n"`: the item keeps its marker, its gap, and its second paragraph at four columns, and the other items are kept

#### Scenario: One edited link
- **WHEN** the target of the `WikiLink` in the document parsed from ``"see  [[A]] and `` b ``"`` is changed to `B` and the document is written
- **THEN** it returns ``"see  [[B]] and `` b ``"``
