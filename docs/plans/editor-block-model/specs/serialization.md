# Serialization — spec changes

## MODIFIED Requirements

### Requirement: Canonical output
`Document::to_markdown` SHALL emit canonical Markdown for every node it does
not take from the source as "Unchanged source is kept" states: for each
construct, the spelling the AST records for it, such as a list marker, an
emphasis delimiter, or an ordered item's number, or else one fixed spelling,
independent of source details the AST does not record. An ordered item
without a number SHALL be written with the previous item's number plus one,
or 1 when it is the first. A `BlankLine` SHALL be written as one line holding
the container prefix without its trailing whitespace, and so SHALL any blank
line the serializer writes inside a container.

### Requirement: Round-trip stability
For a parsed document without its source, parsing the Markdown that
`to_markdown` writes SHALL yield a document equal to the first apart from
spans, reading each `Escape` as a `Text` holding its char, each
`CharacterReference` as a `Text` holding its value, and each `SoftBreak`
inside a heading, which the serializer writes as one space, as a `Text`
holding a space, and merging adjacent `Text` nodes; and serializing that
reparsed document SHALL yield the same text byte for byte, unless the source
holds a detail the AST does not record (decision 0016), such as a lazy
continuation line or the width of an indentation.

### Requirement: Table cells encoded by one rule
The serializer SHALL write a table cell's inline content as it writes any
inline content and then encode it as cell source by one rule: a `\` is added
before each `|` that no backslash or an even run of backslashes precedes, and
nothing else changes. The row splits at none of the cell's pipes, and the cell
reads each added `\|` as the `|` written, so its inline content reads as the
content written. The rule covers every inline alike: a `Text` holding `|` is
written with `\|`.

#### Scenario: Pipe in hand-built text
- **WHEN** a hand-built table whose body cell holds `Text("a|b")` under a header cell holding `Text("h")` is serialized
- **THEN** `to_markdown()` returns `"| h |\n| --- |\n| a\\|b |\n"`, which reads back with the cell holding `Text("a|b")`

### Requirement: Unchanged source is kept
For a document with a source, `to_markdown` and `to_markdown_with` SHALL treat
a node that carries a span as the successor of the node of the same kind at
that span in the document `parse` returns for the source, when one exists and
no node earlier in document order has claimed it, and SHALL write:

- a successor equal to its base node, spans at every depth included, whose
  ancestors all keep their own syntax and which follows the sibling it
  followed in the base, as that span's source text;
- any other successor equal to its base node as its source lines without the
  base's container prefixes, under the prefixes it is written with;
- a successor whose own fields equal its base node's, with the source text of
  its own syntax (markers, fences, delimiters, underlines, table pipes and
  padding, and everything else that is not a child or a gap between
  children), and its children by these rules;
- every other node as canonical output, with its children by these rules.

A leaf with a source of its own (`with_source`) SHALL be written likewise
against that source, its inlines being the children. The writer SHALL keep
the source between two siblings written from the source that are adjacent in
the base, and the source before the document's first block and after its last
when that block is written from the source there; every other boundary SHALL
be canonical. A node SHALL be written as canonical output even so where its
source would read differently in its place; which placements make a node's
source read differently is unspecified. Options SHALL apply only to the text
written as canonical output. Parsing the output SHALL yield a document equal
to the written one as "Round-trip stability" compares them wherever parsing
the canonical output of the same document does.

#### Scenario: One edited item
- **WHEN** the paragraph of the second item of the document parsed from `"-   a\n-   b\n-   c\n"` is replaced by a hand-built paragraph `Text("x")` and the document is written
- **THEN** it returns `"-   a\n-   x\n-   c\n"`: the item keeps its marker and gap, and the other items are kept

#### Scenario: One edited link
- **WHEN** the target of the `WikiLink` in the document parsed from `"see  [[A]] and `` b ``"` is changed to `B` and the document is written
- **THEN** it returns `"see  [[B]] and `` b ``"`
