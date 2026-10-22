# 0020: The source is the only truth, per node, with own syntax edited in place

Status: Accepted
Date: 2026-10-21
Supersedes: 0017

## Context

0017 made the source the only truth at the grain of every node: a node
that corresponds to its base is written as its source, one whose own fields
are unchanged keeps its own syntax around children written by the same
rules, and everything else is rendered, with one exception for a list item
whose number or checkbox changed. A table's alignments are its own fields,
so setting one column's alignment in an editor rendered the whole table and
rewrote every row the user did not touch, the way a renumbered item would
have rewritten its content before the item exception. The alignments live
in one line of their own, the delimiter row, which can change alone, as an
item's marker does. The rest of 0017 stands as it was decided.

## Decision

The source text is the only truth, at the grain of every node. Each spelling
decision is made once, where the information to make it exists:

- **A parsed document keeps its source, and writing reuses it per node.**
  `parse` stores its input in the `Document`; only `parse` can, and
  `without_source()` drops it. A node with a span is matched through its
  parent: a top-level block to the base top-level block at its span, any
  other node to the child at its span of the base node its parent matched,
  of the same kind. Matching claims nothing, so two nodes carrying one span
  both match it. Correspondence is computed bottom-up, once per node. A node
  that corresponds to its base is written as its source text; one whose own
  fields are unchanged keeps its own syntax from the source (markers,
  fences, delimiters, underlines, a table's delimiter row) around children
  written by the same rules, a list item whose number or checkbox changed
  keeping its content column where the new marker fits, and a table whose
  alignments changed writing them in its delimiter row's place, the
  colons of each changed column around that cell's dashes, below a header
  row written from the source, around rows written by the same rules; an
  edited leaf copies each inline that corresponds from its base leaf's
  inline source, with the text between two adjacent copied inlines;
  everything else is rendered. A copied line keeps its container prefixes, which stay valid
  because every container above it is matched and keeps its own.
- **Gaps are kept only where they still read the same.** The source between
  two siblings written from the source and adjacent there is kept where the
  boundary holds a blank line exactly when it did; one without a blank line
  is kept only beside a blank line, between two siblings that hold no
  blocks, or where a container's line on either side is the source's, since
  `> a` followed by `b`, with the quote's last block edited into a
  paragraph, would read `b` as its lazy continuation. A node with copied own
  syntax is not a rendered node here: its kind and own fields are the
  base's. Every other boundary is canonical, and a kept block that a
  following blank line and paragraph would extend, as an unclosed fence, is
  rendered where a new sibling follows it.
- **Placement is one rule at every level.** A block written from the
  source that no longer follows its predecessor there is rendered where
  what comes before it would take it in: indentation after a list or a
  footnote definition, code indentation, a list after a list with its
  marker. The same predicate serves the top level and every container.
- **One read-back check decides the rest.** A document written partly from
  its source is parsed once more; each top-level block written partly from
  the source that reads back differently is written canonically instead,
  and when the block count differs every such block is. Rules are fixed
  where the simulated edits show a fallback, so the check is a guard, not a
  mechanism; each kind refuses the own syntax it cannot copy, and three
  fragment probes (a leaf's text, a block before a new sibling, an item
  before a new item) decide before the check what it would otherwise
  undo.
- **Correspondence, not equality, decides what is unchanged.** A node
  corresponds to a base node when their spans and own fields are equal and
  their children correspond; a leaf built from text corresponds when that
  text equals the base leaf's inline source and its inlines are still the
  ones the text reads as.
- **A leaf keeps the text it was built from.** `with_source` builds a
  paragraph, heading, table cell, or directive label from text, as
  `parse_inline` reads it, and keeps the text; the writer writes the text
  while the inlines are unedited. `leaf_sources` gives every leaf's text in
  one pass.
- **The tree holds what a write needs and the source cannot carry.** A blank
  line beyond the one separating two blocks is a `BlankLine` node, and an
  ordered item records its number; `List` holds no start. No field records a
  spelling the source keeps for reused nodes: setext underlines, closing
  sequences, marker gaps, task-marker case, and thematic-break strings are
  copied, and rendered nodes take the standard spelling.
- **The parser records spellings, each fact once.** When one node kind has
  more than one spelling and a rendered node needs it, the parse records
  which one the source used: `Emphasis` and `Strong` their `*` or `_`
  delimiter, and an `Autolink` its form and its text as written. An escape or
  character reference that matters in the source is already an `Escape` or a
  `CharacterReference` node. A node stores no fact twice: `CodeInline` holds
  only its value.
- **Validation is the only gate.** `Document::validate` rejects the shapes
  that the node and its siblings show to have no spelling, each decided by
  the code that owns it; among them an empty paragraph, a `BlankLine` where
  no blank line can be written, and an item number the parser would not
  read. The serializer refuses nothing that validates. A shape that depends
  on neighbouring characters is not rejected; it renders and may read back
  differently.
- **Rendering only renders.** For each node it does not reuse, it writes
  text, escapes, and references as recorded, and a leaf's kept text as it
  stands; gives every rendered line its full prefix; writes a heading soft
  break as a space; encodes values by fixed rules; encodes each table cell
  once after writing it; and lets an overridden list marker yield to the
  next marker when it would repeat the previous sibling list's. It parses
  its output only for the read-back check of a document written partly
  from its source, and returns it even when it would read back
  differently.

For hand-built trees, the builder answers for literal text: punctuation that
must stay literal is built as `Escape` nodes.

## Considered options

- From 0017: rendering a table whose alignments changed whole, which
  rewrites every row for a change of one line.
- Rendering the delimiter row canonically where the alignments changed:
  a compact `|-|-|` row would become `| --- | --- |` for one column's
  colon.
- meowdown's spelling fields: every spec, validator, serializer branch, wire
  field, and editor attribute would carry them, for a writer that copies
  them from the source anyway.
- Matching with `==`: a leaf built from text never matches, so an editor
  save reuses nothing.
- From 0016: matching any base node at a span with the first claimant
  winning, which needs prefix stripping to move a node and a claim order to
  split one; a guard per construct in place of the read-back check, which
  grows with every construct and misses interactions; recording a prefix
  style per container, a spelling field.
- Rewriting a leaf built from text into document coordinates: needs the
  document's offset map at construction, and breaks for a leaf with no base.
- `Text` holding raw Markdown: the tree would lie about its meaning.
- Empty paragraphs for blank lines, as meowdown does: they render as
  `<p></p>`, reach the index as noise, and leave no rule for a real empty
  paragraph. Every blank line as a node: `[Paragraph, Paragraph]` would read
  back with a node between them.
- Renumbering ordered items in the writer: rewrites items nobody touched, or
  leaves `1. 2. 2. 3.` when it reuses them.
- Node identifiers instead of spans: the AST and the wire would carry one
  for every node.
- A flat list AST: tightness and the marker are facts of the list, and the
  HTML renderer and index would regroup.
- From 0013: a fixed escape table at render time; a read back with a search
  for another spelling; canonical output as the default goal; a copy of the
  original tree; a public `source` field; reuse in the editor's TypeScript.

## Consequences

- Lazy continuation lines, indentation widths, and a code span's fence
  length and padding survive wherever source is reused; rendered nodes get
  the standard rules. Blank-line counts survive everywhere a blank line can
  be written; the blank lines after an empty list item are not nodes.
- An unedited parse writes its source byte for byte by construction, so a
  round trip through the source proves nothing; canonical round-trip tests
  write documents without their source.
- `Document` and the leaves built from text have private fields, so hand
  construction goes through constructors.
- Renumbering after an insertion is the caller's.
- A leaf's text with a line that opens a block where it is written reads
  back as that block.
- One edited node costs a parse of the base, one more of the output, a
  fragment parse per edited leaf and per block a new sibling follows, and
  at most one more write, all linear.
- Copied text keeps its source line endings; an edited leaf's soft breaks
  take the options' ending, so an edited leaf in a CRLF source written with
  the default options mixes endings.
- A gap without a blank line beside an edited container becomes a blank
  line, and a loose list whose blank line went with an edit is written
  whole.
- A table cell empty in the source holds no text to keep the padding
  around, so text given to it is written with the canonical space on
  either side in place of the padding between its pipes, and a cell the
  source row lacks is written after the row with a pipe.
- The source read-back test in `tests/fixtures.rs` and the seeded round-trip
  test each list, with reasons, the documents that do not read back.
