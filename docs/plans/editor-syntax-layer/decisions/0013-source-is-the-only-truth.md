# 0013: The source is the only truth

Status: Accepted
Date: <landing date>
Supersedes: 0008

## Context

The serializer rendered a tree, parsed its own output back, and when the read
back differed from the tree it searched for another spelling: more escapes,
the other emphasis delimiter, a different list layout, character references,
and finally a form that escapes everything. That search existed because the
AST did not record which spelling the source used, so the serializer had to
predict the parser. Hand-built trees with no spelling surfaced as a
serializer error, `Unrepresentable`.

0008 made canonical output the serializer's goal: any tree written in one
standard spelling, with lazy continuation lines, indentation widths, and
blank-line counts normalized. Settle, the crate's user, never writes
canonical output. Its index only reads, and its editor writes files that
agents, git, and other editors also change, where a byte the user did not
touch must not move. Measured on 200 files, 40% of whole files and 95% of
top-level blocks re-serialize byte for byte; the rest of each file changes in
places nobody edited. The machinery that canonical output needs (two
read-back exception lists, normalization that cannot be undone) served a goal
no caller has.

## Decision

The source text is the only truth. A parsed document keeps the text it was
parsed from, and every write reuses it for whatever did not change; canonical
output is how a node with no source of its own is written. Each spelling
decision is made once, where the information to make it exists:

- **A parsed document keeps its source, and writing reuses it.** `parse`
  stores its input in the `Document`; only `parse` can, and
  `without_source()` drops it. `to_markdown` writes each node equal to the
  node at its span in a fresh parse of the source as that span's source text,
  keeps the source between reused neighbours and around the document, and
  renders everything else as below, with canonical separators beside it.
  Nothing but the source is kept: what changed is decided when writing. The
  grain is top-level blocks first and any node once the block model allows
  it; a kept gap is never written beside a rendered node, because `# a`
  followed by `b`, with the heading edited into a paragraph, would read back
  as one paragraph.
- **The parser records spellings, each fact once.** When one node kind has
  more than one spelling, the parse records which one the source used.
  `Emphasis` and `Strong` record their `*` or `_` delimiter, and an
  `Autolink` records its form (angle-bracket or literal) and its text as
  written; its destination is derived from that text. An escape or character
  reference that matters in the source is already an `Escape` or a
  `CharacterReference` node, which holds the reference as written and derives
  its value. A node stores no fact twice: `CodeInline` holds only its value,
  and the fence and padding it is written with are chosen when it is
  written.
- **Validation is the only gate.** `Document::validate` rejects, with
  `InvalidDocument`, the shapes that the node and its siblings show to have no
  spelling: a link in link text, an empty paragraph, emphasis-like content
  with whitespace at an edge, an autolink text that is not one autolink of its form, a character
  reference that is not one reference, an empty code span or one holding a
  line ending, empty inline math, code-form math holding its close, dollar
  math that does not read back with its value and fence, adjacent lists
  written with one marker, a loose list of one item holding at most one
  block, a table cell value no cell source
  spells, and a directive name outside the name rule; a span holding only a
  span whose delimiters join its own, adjacent strikethroughs, a line ending
  in text, a break after a break or on a one-line label or cell, a label or
  identifier the parser would not read back, a task item without its
  paragraph, a tight item's block that cannot interrupt the paragraph before
  it, a list among them whose first item is written below its bullet, an empty list or inline footnote, misplaced frontmatter or frontmatter
  holding its fence, an alert title or info string the parser would trim
  away, an empty bare destination, and indented code with a blank edge line.
  Each rule is decided by the code that owns it: the parser's own functions
  say whether a text is an autolink, a label is a label, inline math reads
  back, or a block interrupts a paragraph, and the serializer's say which
  marker a list is written with, whether a list's first item starts below its
  bullet, and whether a heading is written as setext.
  The serializer refuses nothing that validates. A shape that depends on
  neighbouring characters is not rejected; it renders and may read back
  differently.
- **Rendering only renders.** For each node it does not reuse, it writes text, escapes, and references as
  recorded; gives every container line its full prefix; writes a heading soft
  break as a space; encodes values (code, destinations, titles, attributes)
  by fixed rules; encodes each table cell once after writing it, giving a
  `|` after zero or an even number of backslashes one more backslash, so text
  in a cell is the one place text is not written exactly as recorded; and
  lets an overridden list marker yield to the next marker when it would
  repeat the previous sibling list's. It never parses its output, and returns
  it even when it would read back differently.

For hand-built trees, the builder answers for literal text: punctuation that
must stay literal is built as `Escape` nodes.

## Considered options

- A fixed escape table at render time, as remark does: escapes far more than
  needed (`x\_y\_`), and is one more copy of the parser's rules.
- Keep the read back for escapes only: read back plus search is the deferral
  this decision removes.
- Fall back to the source text when the read back fails: with rendering only
  there is no failure to fall back from; reuse is decided by what changed,
  not by what fails.
- Canonical output as the goal, with the source reused only by a second
  write verb: the default write keeps normalizing bytes nobody edited.
- Keep a copy of the original tree to find changes: doubles every parse's
  memory for callers that only read.
- A public `source` field: a caller could pair a tree with text it was not
  parsed from.
- Reuse in the editor's TypeScript: it would copy the parser's rules for
  when blocks merge.
- A serializer error, or errors for hand-built nodes only: both keep the
  serializer responsible for detection.

## Consequences

- Lazy continuation lines, indentation widths, blank-line counts, and a code
  span's fence length and padding survive wherever source is reused; rendered
  nodes get the standard rules until the AST records them.
- An unedited parse writes its source byte for byte by construction, so a
  round trip through the source proves nothing; canonical round-trip tests
  write documents without their source.
- `Document` has a private field: `Document { meta, children }` no longer
  compiles, and hand construction goes through `Document::new`.
- The source read-back test in `tests/fixtures.rs` and the seeded round-trip
  test each list, with reasons, the documents that do not read back, keyed
  by content, and fail when a listed one starts to or no longer exists.
- Round-trip comparison reads a heading's `SoftBreak` as a space.
