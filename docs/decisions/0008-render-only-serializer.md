# 0008: A render-only serializer

Status: Accepted
Date: 2026-10-07

## Context

The serializer rendered a tree, parsed its own output back, and when the read
back differed from the tree it searched for another spelling: more escapes,
the other emphasis delimiter, a different list layout, character references,
and finally a form that escapes everything. That search existed because the
AST did not record which spelling the source used, so the serializer had to
predict the parser. Hand-built trees with no spelling surfaced as a
serializer error, `Unrepresentable`.

## Decision

Each spelling decision is made once, where the information to make it exists:

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
  spelling: a link in link text, emphasis-like content with whitespace at an
  edge, an autolink text that is not one autolink of its form, a character
  reference that is not one reference, an empty code span or one holding a
  line ending, empty inline math or code-form math holding its close,
  adjacent lists written with one marker, a table cell value no cell source
  spells, and a directive name outside the name rule. Each rule is decided by
  the code that owns it: the parser's own functions say whether a text is an
  autolink, and the serializer's says which marker a list is written with.
  The serializer refuses nothing that validates. A shape that depends on
  neighbouring characters is not rejected; it renders and may read back
  differently.
- **The serializer only renders.** It writes text, escapes, and references as
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
- Fall back to the source text when the read back fails: needs the input kept
  in `Document`, and with rendering only there is no failure to fall back
  from.
- A serializer error, or errors for hand-built nodes only: both keep the
  serializer responsible for detection.

## Consequences

- Lazy continuation lines, indentation widths, blank-line counts, and a code
  span's fence length and padding are not recorded; the serializer writes
  them by its standard rules.
- The source read-back test in `tests/fixtures.rs` and the seeded round-trip
  test each list, with reasons, the documents that do not read back, keyed
  by content, and fail when a listed one starts to or no longer exists.
- Round-trip comparison reads a heading's `SoftBreak` as a space.
