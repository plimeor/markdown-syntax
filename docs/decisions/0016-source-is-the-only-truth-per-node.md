# 0016: The source is the only truth, per node

Status: Accepted
Date: 2026-10-14
Supersedes: 0013

## Context

0013 made the source the only truth: a parsed `Document` keeps its source,
and writing reuses it for whatever did not change, at the grain of top-level
blocks first. Settle's editor is a ProseMirror editor that holds what the
tree could not express: blank lines the user typed, a leaf's text as the user
typed it, the number written on each ordered item. It sends every leaf back
as text, which a tree compared with `==` never matches, since a leaf built
from text carries spans in that text. And a top-level grain rewrites a whole
list when one item changes, and a whole paragraph when a rename changes one
link in a note nobody opened.

meowdown, the editor this one is modeled on, records each spelling in a field
(`setextUnderline`, `closingHashes`, `markerGap`, `taskMarker`, the
thematic-break string), because it rewrites the whole file on every save.

## Decision

The source text is the only truth, at the grain of every node. Each spelling
decision is made once, where the information to make it exists:

- **A parsed document keeps its source, and writing reuses it per node.**
  `parse` stores its input in the `Document`; only `parse` can, and
  `without_source()` drops it. A node with a span is matched to the base node
  of its kind at that span in a fresh parse of the source, the first claimant
  winning. A node that corresponds to its base and stands where it stood is
  written as its source text; one whose own fields are unchanged keeps its
  own syntax from the source (markers, fences, delimiters, underlines) around
  children written by the same rules; everything else is rendered. Line
  prefixes are kept only inside kept source. A kept gap is never written
  beside a rendered node, because `# a` followed by `b`, with the heading
  edited into a paragraph, would read back as one paragraph. The grain
  reaches every node as the `per-node-reuse` plan lands; until then it is
  top-level blocks.
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
  next marker when it would repeat the previous sibling list's. It never
  parses its output, and returns it even when it would read back
  differently.

For hand-built trees, the builder answers for literal text: punctuation that
must stay literal is built as `Escape` nodes.

## Considered options

- meowdown's spelling fields: every spec, validator, serializer branch, wire
  field, and editor attribute would carry them, for a writer that copies
  them from the source anyway.
- Matching with `==`: a leaf built from text never matches, so an editor
  save reuses nothing.
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
  the standard rules. Blank-line counts survive everywhere.
- An unedited parse writes its source byte for byte by construction, so a
  round trip through the source proves nothing; canonical round-trip tests
  write documents without their source.
- `Document` and the leaves built from text have private fields, so hand
  construction goes through constructors.
- Renumbering after an insertion is the caller's.
- A leaf's text with a line that opens a block where it is written reads
  back as that block.
- The source read-back test in `tests/fixtures.rs` and the seeded round-trip
  test each list, with reasons, the documents that do not read back.
