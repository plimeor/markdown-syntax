# Serialization
Type: api

## Purpose

Turning a `Document`, parsed or hand-built, into Markdown text: the source of
what a parsed document left unchanged, and canonical Markdown for the rest.
Owned by the serializer (the `serialize` module).

## Requirements

### Requirement: Canonical output
`Document::to_markdown` SHALL emit canonical Markdown for every node it does
not take from the source as "Unchanged source is kept" states: for each
construct, the spelling the AST records for it, such as a list marker, an
emphasis delimiter, or an ordered item's number, or else one fixed spelling,
independent of source details the AST does not record. An ordered item
without a number SHALL be written with the previous item's number plus one,
or 1 when it is the first. A `BlankLine` SHALL be written as one line holding
the container prefix without its trailing whitespace, and so SHALL any blank
line the serializer writes inside a container, except a blank line that
opens a block quote, which SHALL be written `> `. An empty heading, list
item, or block quote SHALL be written as its marker followed by one space:
`# `, `- `, `1. `, `> ` (decision 0019). A fenced code block's info string
SHALL follow its opening fence directly, after one space only where it
opens with the fence's char.

#### Scenario: Empty blocks
- **WHEN** a hand-built document holding a `Heading` of depth 1 with no children, a list of one item with no blocks, or a `BlockQuote` holding a `BlankLine` and a paragraph `a` is serialized
- **THEN** `to_markdown()` returns `"# \n"`, `"- \n"`, or `"> \n> a\n"`, each of which reads back as the document

### Requirement: Round-trip stability
For a parsed document without its source, parsing the Markdown that
`to_markdown` writes SHALL yield a document equal to the first apart from
spans, reading each `Escape` as a `Text` holding its char, each
`CharacterReference` as a `Text` holding its value, and each `SoftBreak`
inside a heading, which the serializer writes as one space, as a `Text`
holding a space, and merging adjacent `Text` nodes; and serializing that
reparsed document SHALL yield the same text byte for byte, unless the source
holds a detail the AST does not record (decision 0017), such as a lazy
continuation line or the width of an indentation.

### Requirement: Serialize options
`SerializeOptions` SHALL control the line ending and the trailing newline. The
`bullet`, `ordered_delimiter`, and `fence_marker` options SHALL keep the
marker each node records when `None`, the default, and SHALL write every
unordered list, ordered list, or fenced code block with the marker they hold
when `Some`, whichever marker that is; a fenced block whose info string holds a
backtick SHALL take tildes regardless. A replaced list marker SHALL yield where
the list before it in the same container is written with the same marker;
which marker a yielding list takes is unspecified.

### Requirement: Invalid documents are rejected
Serialization SHALL validate the document first and return
`SerializeError::InvalidDocument` with the validation diagnostics when it is
invalid. Validation is the only check that refuses a document: every valid
document is written.

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

### Requirement: Values are encoded by rule
The serializer SHALL write a value that a construct holds as raw text, such as
a code block or a code span, so that the construct reads back with the same
value, writing a value's own line endings as they are; the fence and padding a
code span is written with are unspecified.

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
source wherever it is written, except that a heading, a table cell, or
a directive label whose source holds a line ending is written from its
inlines. The writer SHALL write:

- a node that corresponds to its match, under containers that all keep their
  source prefixes, as its span's source text;
- any other matched node with the source text of its own syntax (markers,
  fences, delimiters, the table's delimiter row, and everything
  else written once per node that is not a child), when its own fields are
  equal, and its children by these rules; a list item whose number or
  checkbox changed renders them in its marker's place, keeping its content
  column where they fit;
- every other node as canonical output, with its children by these rules.

A line written where the base node has a source line keeps that line's
source prefix (container markers and indentation) and the line ending before
it: a table's delimiter row, and a blank line matched to the base's whole,
unless it opens its container, whose own syntax opens it;
an item's marker, for an item whose own fields are equal and whose
whitespace before its content is spaces or one tab, as its source text up
to its first child, tabs and line endings included, the line its
first child starts on keeping that line's source prefix where it reads as
that line does there, after a checkbox as a paragraph's; a later line of an
edited leaf's text, a table's later row, or the line an alert's or a
directive's first child starts on after the container's own, where it
reads as that line does there: it is that line's text, it parses alone as a
paragraph, or the source line stands where no line opens a block, as one
indented four columns past its container does. Every other line SHALL take
the canonical prefix.

The writer SHALL keep the source between two siblings written from the
source that are adjacent in the source, where the boundary holds a
separating blank line exactly when it did there, and where the boundary
holds a blank line, has a `BlankLine` on one side, stands between two
siblings that hold no blocks, has the source's lines on both sides, or
follows a last leaf, through the last child of each container before it,
that as written does not take the sibling's first line: after a paragraph,
a definition, or a table at the same level, where the sibling interrupts a
paragraph (a table: ends a table); after a paragraph or a definition inside
containers, or an alert without blocks, where that line as a lazy line ends
those containers; after an empty last item, where that line stands outside
it; after a directive's last leaf never, unless the source's lines are on
both sides; after any other last leaf always; and, after a list whose last
item is not the base's last item written from the source, only where the
source's lines are on both sides. Which other boundaries without a blank
line are kept is unspecified. It SHALL keep the source before the
document's first block and after its last when that block is written from
the source there; every other boundary SHALL be canonical. A `---` thematic
break that opens the document in the source and in the output SHALL be kept
from the source unless a later line of the output closes it as frontmatter.
A node SHALL be written as canonical output even so where its source would
read differently in its place; which placements make a node's source read
differently is unspecified, except that a top-level block written partly
from the source SHALL be written as canonical output where the parsed output
holds a different block in its place. Options SHALL apply only to the text
the writer produces, line endings included, and a list written with its own
syntax from the source SHALL write a new item with the marker it records;
text and line endings copied from the source SHALL be kept as they are.
Parsing the output SHALL yield a document equal to the written one as
"Round-trip stability" compares them wherever parsing the canonical output
of the same document does.

#### Scenario: Edited items
- **WHEN** the paragraph of the second item of the document parsed from `"-   a\n-   b\n\n    c\n-   d\n"`, and the paragraph of the item of the one parsed from `"aa\n- b\n"`, are each replaced by a hand-built paragraph `Text("x")`, the item of `"aa\n- b\n"` is separately left with no children, and each document is written
- **THEN** they return `"-   a\n-   x\n\n    c\n-   d\n"`, `"aa\n- x\n"`, and `"aa\n\n- \n"`: an item keeps its marker, its gap, and its second paragraph at four columns, the other items and the boundary after the paragraph are kept, and the empty item, which cannot interrupt the paragraph, is written after a blank line with the space that opens it

#### Scenario: One edited link
- **WHEN** the target of the `WikiLink` in the document parsed from ``"see  [[A]] and `` b ``"`` is changed to `B` and the document is written
- **THEN** it returns ``"see  [[B]] and `` b ``"``

#### Scenario: Lines around an edited leaf
- **WHEN** the paragraph of the quote parsed from `"> a\nb\n"` is given the text `"a\nbX"`, the paragraph in the quote parsed from `"> - a\n- b\n"` the text `"aX"`, and each document is written
- **THEN** they return `"> a\nbX\n"` and `"> - aX\n- b\n"`: the lazy line stays lazy, and the list after the quote, which a lazy line's list marker starts, stays on the next line
