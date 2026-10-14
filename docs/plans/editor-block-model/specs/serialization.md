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
For a document with a source, `to_markdown` and `to_markdown_with` SHALL write
each top-level block that carries a span equal to that of a top-level block
of the document `parse` returns for the source, and corresponds to that block,
as that span's source text; SHALL keep the source between two such blocks
that are adjacent in the source, and the source before the first block and
after the last when that block is one of them; and SHALL write every other
block as canonical output, with a blank line on each side. A node corresponds
to a base node of its kind when their spans and their own fields are equal
and their children correspond pairwise; a leaf with its own source
(`with_source`) whose inlines are the ones it was built from corresponds when
that source equals the base leaf's inline source, and is written as that
source wherever it is written, except that an ATX heading, a table cell, or
a directive label whose source holds a line ending is written from its
inlines. A block SHALL be written as canonical output
even so where its source would read differently in its place; which
placements make a block's source read differently is unspecified. Options
SHALL apply only to the text written as canonical output. Parsing the output
SHALL yield a document equal to the written one as "Round-trip stability"
compares them wherever parsing the canonical output of the same document
does.

#### Scenario: One edited block
- **WHEN** the second block of the document parsed from `"|a|b|\n|-|-|\n\n*  x\n\n\n# T #"` is replaced by a hand-built paragraph `Text("y")` and the document is written
- **THEN** it returns `"|a|b|\n|-|-|\n\ny\n\n\n# T #"`: the table, the blank line, the heading, and the missing final newline are kept, and the gap before the new paragraph is canonical

#### Scenario: Leaves given as text
- **WHEN** every leaf of the document parsed from `"- a  *b*\n\n  c\n"` is replaced by the leaf `with_source` builds from its `leaf_sources` text, with its span, and the document is written
- **THEN** it returns the source
