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

Largest risk: a node written from the source beside a rendered sibling or
child that reads back differently (a reused setext underline under rendered
content, a reused fence around changed code). It is met by one read-back
check of the whole output, with a fallback to the top-level grain, and by
generalizing batch 1's simulated edits to every node at every depth.

## Architecture

```
to_markdown[_with] ──► base = parse(source) once
                       write each node bottom-up against its base node
                         ├─ Kept(span)      the node is its source slice
                         └─ Lines           own syntax copied or rendered,
                                            children as they were written
                       join the top level as today (batch 1 guards)
                       read-back check ──► fallback per top-level block
```

`to_markdown` and `to_markdown_with` keep their signatures. Nothing is stored
between writes. The canonical writer (`write_canonical` and the `write_*`
functions under it) is unchanged and stays the only renderer; reuse is a
layer above it that decides, per node, between the source and the renderer.

### Matching

A node is matched to a base node only through its parent: a top-level block
to the base top-level block at its span, as today; any other node to the
child of its parent's base node at its span (a list item to an item of the
base list, a cell to the cell at its row and column). The base node must be
of the same kind. A node with no match is new. Matching claims nothing:
two nodes carrying one span (a split paragraph) both match it, and the
read-back check decides what their copies may keep.

Matching through the parent is narrower than "any base node at its span":
a node moved into another container is new there and is rendered. That
costs bytes only on a move, and keeps every copied line's container
prefixes valid (see Line model).

### Bottom-up correspondence

Each node is written after its children, and returns either `Kept(span)` or
`Lines`. A node returns `Kept` when it is matched, its own fields equal its
base node's, and its children returned `Kept` for exactly its base node's
children in order; a leaf returns `Kept` when it corresponds to its base
leaf as batch 1 defines it (kept text included). So correspondence is
computed once per node, never by comparing subtrees top-down, and `Kept`
needs no text until a parent that is not kept takes the slice. Own fields
are compared by one exhaustive function over node kinds that ignores
children, the counterpart of `compare.rs`.

### Line model

A node that is not kept returns its text as lines, each marked:

- **content** — the line as its parent's content: every enclosing container
  adds its prefix to it, as the canonical writer does now;
- **full** — the line already holds every enclosing container's prefix, as
  copied from the source, and no container adds one.

A kept child's slice gives its first line as content (a span starts after
its containers' markers) and every later line as full. A kept gap between
two siblings gives full lines, and the part of the last line before the next
sibling makes that sibling's first line full too. Rendered text gives
content lines.

Full lines are valid only where every enclosing container writes the same
prefixes as in the source. Matching through the parent makes every
container above a written-from-source node matched; the one way a matched
container changes its prefix is a list item whose marker no longer fits its
content column, and such an item is rendered whole (Own syntax). So a full
line is never rewritten, and each line is written once.

### Own syntax

A matched node that is not kept copies its own syntax from the source and
writes its children by these rules. Own syntax is the bytes of its span that
no child covers, taken per kind:

| Node | Own syntax copied |
|---|---|
| ATX heading | opening sequence and the whitespace after it; closing sequence and the whitespace around it |
| Setext heading | the underline line |
| Block quote | the `>` and the space after it before its first block, on its first line |
| List item | marker, task checkbox, and the whitespace after them on its first line |
| Alert | the `[!KIND]` line |
| Fenced code, math block, container directive | opening line; closing line |
| HTML container | the source before its first block and after its last |
| Footnote definition | the `[^label]:` and the whitespace after it, or, before blocks on a later line, the lines up to them |
| Table | the delimiter row; each row that corresponds, as its line |

A line of own syntax that is not the node's first line (an underline, a
closing fence, a delimiter row) is copied from its first character that is
not a space, a tab, or `>`, and written as a content line: those characters
are the containers' prefixes, which no own-syntax line opens with.

A node whose own fields changed renders its own syntax, except that a list
item whose number or checkbox changed keeps its marker's gap: it renders the
marker and checkbox and pads them to the source's content column when they
fit in it, so a renumbered or ticked item keeps its children's columns; an
item whose marker does not fit, or whose marker line holds a tab, is
rendered whole. A
code, math, or HTML block whose value changed renders as a whole: its value
is an own field.

Rendered lines in a list item take its content column, which is the copied
marker's width when the marker is copied, so `-   x` keeps its continuation
lines at four columns.

### Leaves

A matched leaf that is not kept is written as inline content: each inline
that corresponds to the base inline at its span is copied from the base
leaf's inline source, each matched inline whose own fields are equal copies
its own syntax around children written by the same rules (emphasis
delimiters; a link's brackets, destination, and title), and every other one
is rendered. The text between two adjacent inlines that are copied and
adjacent in the base is kept; nothing is written between others, as the
canonical writer writes none. The inline source is prefix-free, so a leaf's
lines are content lines and take the current prefixes.

A leaf built from text whose inlines were then edited has lost that text in
the writer's pre-pass and is rendered, as in batch 1.

### Gaps

Between two siblings that are matched, written from the source (kept or
with copied own syntax), and adjacent in the base, the source between them
is kept, where the boundary still does or does not hold a separating blank
line as in the base. A gap without a blank line stays only where it still
reads the same: between two siblings that hold no blocks, beside a
`BlankLine`, or where the line before it and the line after it are the
source's, since a leaf's edited last line could read the next sibling's
first line as its lazy continuation inside a container. Every other boundary
gets the canonical separator, with `BlankLine` nodes written as their lines.
A node with copied own syntax is not a rendered node in decision 0016's
sense ("a kept gap is never written beside a rendered node"): its kind and
own fields are the base's. A loose list whose kept gaps no longer hold a
blank line is rendered, as it would read back tight.

### Top level

The top level keeps batch 1's writer. A top-level block that returns
`Lines` is written as those lines where a rendered block would be written,
with canonical separators around it.

### Placement

A block written from the source that no longer follows its predecessor
there is rendered where what is written before it would take it in:
indentation after a list or a footnote definition, which blank lines do not
end; four columns of indentation on a block other than indented code; a
list after a list with its marker. One predicate states this for every
sequence of siblings, the top level and every container alike; the top
level adds the `---` break that would open frontmatter. A kept block that a
blank line and a paragraph would extend, as an unclosed fence, is rendered
before a sibling other than its successor.

### Read-back check

When a top-level block returned `Lines`, the writer parses the output once
and compares its top-level blocks pairwise with the document's as "Round-trip
stability" compares them (`normalized_blocks`). Each block that returned
`Lines` and differs is written canonically instead: what it reads back as is
then the canonical writer's, which the spec already allows to differ. When
the top-level counts differ, the document is written at batch 1's grain.
The pure rendering path, a document without a source or with nothing
reused, still never parses its output. The cost is one parse of the output
and at most one more write, each linear.

### Options

Options apply only to rendered text. A list written with copied own syntax
writes its new items with the list's recorded marker, not an overriding
`bullet` or `ordered_delimiter`, so that one list is not split into two; the
override applies to lists written whole.

## Flows

- **Save (editor).** The adapter sends every node with its span and every
  leaf as `source`; one edited item in a 2000-item list rewrites only that
  item's leaf, and a renumbered or ticked item only its number or checkbox.
- **Rust edit (Settle rename).** Parse a note, change a `WikiLink`'s target,
  write: the paragraph copies every other inline and the gaps between them,
  and every other block is kept.

## Test scope

- Simulated edits, generalizing batch 1's over the read-back corpus and the
  seeded generators: for every node at every depth, change one own field,
  change one child, delete it, and insert a new sibling; check that the
  written text reads back to the edited tree, and that a node the rules keep
  is kept byte for byte. A block the check rewrote canonically, where its
  canonical output reads back as the edited tree, is reported by the test,
  so that a rule is fixed, not left to the fallback.
- The same edits on a document whose leaves are all rebuilt from text.
- Cases for a multi-line item under a wider marker, a list under a marker
  option, a renumbered and a ticked item with nested blocks, and edits at
  the deepest nesting the parser allows.
- Cost: writing with edits at depth in `tests/linear_growth.rs`; deep
  nesting in `tests/pathological_inputs.rs`.

## Decisions

- Reuse down to inline nodes — because Settle's rename rewrites links in
  notes the user has not opened, and must not move the rest of a paragraph.
  Turned down: stopping at leaves, which renders the whole paragraph around
  one changed link.
- Matching through the parent — because a copied line holds its containers'
  prefixes, which are valid only under the same containers. Turned down:
  matching any base node at the span with first claimant wins, which needs
  prefix stripping to move a node and a claim order to split one.
- Correspondence bottom-up, returning `Kept(span)` — because a top-down
  `==` at every node compares each subtree once per ancestor. Turned down:
  a separate syntax map, which would restate what spans and correspondence
  already give.
- One read-back check of the output — because guards per construct (an
  unclosed fence, an underline under rendered content, a fence closed by
  changed code) grow with every construct and miss interactions. Turned
  down: a guard list, batch 1's approach, kept only for placement, where
  one predicate serves every level.
- Prefixes kept only inside kept source — because a changed child breaks
  the line-to-line mapping a copied prefix would need. Turned down:
  recording a prefix style per container, which is a spelling field.

## Risks

- [Reuse that reads back differently] → the read-back check falls back per
  top-level block; the simulated edits report every fallback.
- [The writer is no longer linear] → correspondence once per node, matching
  by binary search within the parent, one parse for the check;
  linear-growth tests at depth.
- [The check hides a weak rule] → the simulated edits fail on any fallback
  they trigger, so rules are fixed rather than masked.
