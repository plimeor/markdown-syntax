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

- **The parser records spellings.** When one node kind has more than one
  spelling, the parse records which one the source used. `Emphasis` and
  `Strong` record their `*` or `_` delimiter and `Link` records whether it was
  written inline, as an angle-bracket autolink, or as a literal autolink. An
  escape or character reference that matters in the source is already an
  `Escape` or `CharacterReference` node.
- **Validation is the only gate.** `Document::validate` rejects, with
  `InvalidDocument`, the shapes that the node and its siblings show to have no
  spelling: a link in link text, emphasis-like content with whitespace at an
  edge, a recorded autolink form that does not fit its content, adjacent lists
  with one marker, and a directive name outside the name rule. A shape that
  depends on neighbouring characters is not rejected; it renders and may read
  back differently.
- **The serializer only renders.** It writes text, escapes, and references as
  recorded; gives every container line its full prefix; writes a heading soft
  break as a space; encodes values (code, destinations, titles, attributes)
  by fixed rules; and lets an overridden list marker yield to the next marker
  when it would repeat the previous sibling list's. It never parses its
  output, and returns it even when it would read back differently.

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

- Lazy continuation lines, indentation widths, and blank-line counts are not
  recorded; the serializer writes them by its standard rules.
- The seeded round-trip test lists, with reasons, the generated documents that
  do not read back, and fails when a listed one starts to.
- Round-trip comparison reads a heading's `SoftBreak` as a space.
