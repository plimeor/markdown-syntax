# Source reused per node — design

## Overview

The `editor-block-model` plan writes a parsed document from its source at
the grain of top-level blocks, with leaves built from text written as their
text. This plan carries the same rule down to every node, and to each node's
own syntax.

The rule, stated once: **a node with a counterpart in the source is written
from the source wherever it and its context are unchanged, and only what
changed is rendered.** It replaces meowdown's approach of one field per
spelling (`setextUnderline`, `closingHashes`, `markerGap`, `taskMarker`, the
thematic-break string), which meowdown needs because it rewrites the whole
file on every save. Copying a node's own syntax from the source keeps those
spellings and also the ones no field captures: tabs, padding, trailing
spaces, lazy continuation lines.

Largest risks: a node written from the source beside a rendered sibling or
child that reads back differently (a reused setext underline under rendered
content, a reused fence around changed code), and the writer's cost. Both
are met by generalizing batch 1's simulated-edit test to every node at every
depth, and by the linear-growth suite.

## Architecture

```
to_markdown[_with] ──► base = parse(source) once, with its syntax map
                       match nodes to the base by span, write per node
```

`to_markdown` and `to_markdown_with` keep their signatures. Nothing is stored
between writes.

### Base and own syntax

For a document with a source, the writer parses the source once and builds
a syntax map from the base tree: for each base node, the byte ranges of its
one-time syntax, the ranges its children cover, and the gaps between them;
for each leaf, its inline source, taken from `leaf_sources`. The ranges come
from the spans the parse already records; where a node's one-time syntax is
not the complement of its children (a list item's marker and the indentation
of its continuation lines), the map takes it from the first line.

**One-time syntax** of each kind, the bytes written once per node:

| Node | One-time syntax |
|---|---|
| ATX heading | opening sequence and the whitespace after it; closing sequence and the whitespace around it |
| Setext heading | the underline line |
| List item | marker, task checkbox, and the whitespace after them on its first line |
| Alert | the `[!KIND]` line |
| Fenced code, math block, directive, HTML container | opening line, closing line |
| Table | the delimiter row |
| Table row | its leading and trailing pipe and the excess cells past the delimiter row's width |
| Link, image, reference | everything outside the text or alt: brackets, destination, title |
| Emphasis-like | delimiters |
| Text directive | name, brackets, attributes |

**Line prefixes** (a block quote's `>`, a list item's continuation
indentation, an indented code block's indentation, a table cell's padding)
are not one-time syntax: once a child changes, the lines no longer map one to
one. A prefix is kept where it lies inside kept source, which is inside the
slice of a child written from the source (its continuation lines) or inside a
kept gap (the line a reused child starts on). Every line the writer renders
takes the canonical prefix.

### Matching

A node with a span is the successor of the base node of the same kind at the
same span, if the base has one and no earlier node in document order has
claimed it; ProseMirror copies attributes when it splits a node, so two nodes
can carry one span. A node with no successor in the base is new. Inline
nodes in a parsed leaf match the base leaf's inlines by their document spans;
inline nodes in a leaf built from text match the inlines `parse_inline`
reads from that leaf's kept text, by their spans in it.

"Unchanged" below is correspondence, as the `editor-block-model` design
defines it, not `==`.

### Write rules

Applied at every node from the top:

1. *Unchanged node, unchanged context*: the node corresponds to its base, so
   does every ancestor's one-time syntax, and it follows the same sibling it
   followed in the base. It is written as the source slice of its span; the
   slice holds the ancestors' prefixes on its continuation lines, which are
   what the writer would write there.
2. *Unchanged node, changed context*: it is written as its content lines, the
   source lines with the base ancestors' prefixes removed, under the current
   prefixes. Lazy continuation lines take the full prefix.
3. *Own fields unchanged, children changed*: its one-time syntax is copied
   from the source, and its children are written by these rules.
4. *Own fields changed, or new*: its own syntax is rendered canonically, and
   its children are written by these rules.

Leaves follow the same rules with inline nodes as children: an edited leaf is
written as its inline content, each unchanged inline copied from the leaf's
inline source and each changed one rendered, then its lines prefixed. A leaf
with a kept source and unchanged inlines is written as that source.

### Gaps

Between two siblings written by rule 1 or 3 that are adjacent in the base,
the source between them is kept; every other boundary gets the canonical
separator, with `BlankLine` nodes written as their lines. Before the first
and after the last top-level block, the source's leading and trailing bytes
are kept when that block is reused there, as today.

### Read-back guards

Batch 1's guards move from the top level to every container: a reused node
that a blank line and a paragraph after it would change (an unclosed fence)
is rendered when it comes before a node other than its base successor; a
reused node whose source opens with indentation is rendered after a node
other than its base predecessor where that indentation would read
differently; a reused list after a list with the same marker is rendered.
Rule 3 adds two: a reused setext underline is rendered when the rendered
content's last line would read as something else above it, and a reused
fence is rendered when the changed code holds a line that would close it.
Each guard asks the parser, as batch 1's do, and costs time linear in the
node it checks.

## Flows

- **Save (editor).** The adapter sends every node with its span and every
  leaf as `source`; one edited item in a 2000-item list rewrites only that
  item's text, and a renumbered item only its marker.
- **Rust edit (Settle rename).** Parse a note, change a `WikiLink`'s target,
  write: the paragraph is written by rule 3 with every other inline copied,
  and every other block by rule 1.

## Test scope

- Simulated edits, generalizing batch 1's over the read-back corpus and the
  seeded generators: for every node at every depth, change one own field,
  change one child, delete it, move it to a sibling container, and insert a
  new sibling; check that the bytes outside the edited node are unchanged
  where the rules say they are, and that the written text reads back to the
  edited tree. A failure is fixed in a write rule or a guard, never listed
  away.
- The same edits on a document whose leaves are all rebuilt from text.
- meowdown's graded round-trip cases and its blank-line cases, ported as
  inputs whose writes are checked byte for byte where the rules promise it.
- Cost: writing with edits at depth in `tests/linear_growth.rs`; deep
  nesting in `tests/pathological_inputs.rs`.

## Decisions

- Reuse down to inline nodes — because Settle's rename rewrites links in
  notes the user has not opened, and must not move the rest of a paragraph.
  Turned down: stopping at leaves, which renders the whole paragraph around
  one changed link.
- Line prefixes kept only inside kept source — because a changed child
  breaks the line-to-line mapping a copied prefix would need. Turned down:
  recording a prefix style per container, which is a spelling field.
- Matching by span, first claimant wins — because the editor then reports
  nothing beyond the tree, as in batch 1. Turned down: node identifiers,
  which the AST and the wire would carry for every node.

## Risks

- [A reused own syntax and a rendered child read back differently] → the
  simulated edits at every depth; a failure becomes a guard that renders the
  own syntax.
- [The writer is no longer linear when every node consults the base] → the
  syntax map is built once; matching is one sorted lookup per node;
  linear-growth tests at depth.
- [Two nodes claim one span] → first claimant wins, tested with a split
  paragraph and a split list item.

## Open questions

- Whether a gap with no blank line (`# h\npara`) can be kept beside a node
  rendered by rule 3 whose kind did not change; the simulated edits decide
  it, and it changes no requirement.
