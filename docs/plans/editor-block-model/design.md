# The block model of an editor, and source reused per node — design

## Overview

Batch 1 made the source the only truth at the grain of top-level blocks
(0013): a parsed `Document` keeps its source, and `to_markdown` copies each
unchanged top-level block. This plan carries the same rule down to every
node, and to each node's own syntax, and adds what a ProseMirror editor holds
that the AST cannot yet express: blank lines the user typed, a leaf's text as
the user typed it, and the number written on each ordered item.

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

The editor-facing shape this serves, in the Settle editor: nested lists in
the AST become flat `list` nodes in ProseMirror in Settle's adapter, a
`BlankLine` becomes an empty paragraph, and a leaf's inline source becomes
the text of its textblock.

## Architecture

```
parse(input) ──► Document { blocks…, source }        (unchanged entry point)
                    │
                    ├─ leaf_sources() ──► every leaf's inline source, one pass
                    ├─ inline_source(path)            (one leaf, as today)
                    │
editor / Rust caller edits the tree, keeping spans of nodes from the source;
an edited leaf may be rebuilt from text: Paragraph::with_source(text, defs)
                    │
                    ▼
to_markdown[_with] ──► base = parse(source) once, with its syntax map
                       match nodes to base by span, write per node
```

Dependencies are unchanged: the wasm crate depends on the core crate; the
core crate gains no dependency.

### AST changes

```rust
pub enum Block {
    // … existing variants …
    /// A blank line beyond the one that separates two blocks.
    BlankLine(BlankLine),
}
pub struct BlankLine { pub meta: NodeMeta }

pub struct List {
    pub meta: NodeMeta,
    pub ordered: bool,
    pub delimiter: ListDelimiter,
    pub tight: bool,
    pub children: Vec<ListItem>,   // `start` is gone: it is the first item's number
}
pub struct ListItem {
    pub meta: NodeMeta,
    pub checked: Option<bool>,
    /// The number an ordered item is written with; `None` writes the
    /// previous item's number plus one, or 1 for the first item.
    pub number: Option<u64>,
    pub children: Vec<Block>,
}
```

- `Paragraph`, `Heading`, `TableCell`, `LeafDirective`, and
  `ContainerDirective` gain a private inline source, set only by their
  `with_source` constructors (below). A parsed leaf holds none; its inline
  source is derived from the document's.
- In a table cell, `\|` reads as `Text("|")` where it was `Escape('|')`:
  the cell's inline source holds `|`, so `parse_inline` on it reproduces the
  cell, and the writer escapes every `|` in a cell by the existing cell rule.
  A code span in a cell already reads `\|` as `|`.

### Blank lines

The rule is meowdown's: K blank lines between two blocks are one separator
and K − 1 `BlankLine` nodes. One blank line is never a node, so a hand-built
`[Paragraph, Paragraph]` writes `a\n\nb` and reads back as two paragraphs
with nothing between them; counting every blank line as a node would make
canonical round trips unstable.

| Where | Nodes |
|---|---|
| Between two blocks in one container | K − 1 |
| Before the first block of the document | K (no separator precedes) |
| After frontmatter | K − 1 (the first blank line separates) |
| After the last block of the document, beyond its line ending | K |
| At the start or end of a block quote, alert, or container directive (`>` lines, or blank lines inside the fences) | K |
| Between two items of a list | K − 1, at the end of the earlier item's children |
| A document of only blank lines | one node per line |

- A `BlankLine` is written as one line holding the container prefix with its
  trailing whitespace removed (`>` in a block quote, nothing in a list item).
- Between items, one blank line still makes the list loose. `- a\n\n\n- b`
  is one loose list whose first item holds `[Paragraph, BlankLine]`, as
  CommonMark 0.31 reads two blank lines; meowdown splits it into two lists.
- A blank line inside an open leaf (fenced code, HTML block, math) is part of
  that leaf's value, as today.
- Validation: an empty `Paragraph` stays invalid. A `BlankLine` is invalid
  where no blank line can be written: first in a list item, first in a
  footnote definition, and in a tight list.

### Ordered item numbers

- Parse records each ordered item's number as written; bullet items record
  `None`.
- The writer writes a recorded number as it is, and `None` as the previous
  item's number plus one (1 for the first item).
- Validation keeps the overflow rule (at most nine digits) for every number,
  and rejects a number on a bullet item.
- Renumbering after an insertion is the caller's: an editor that inserts an
  item into `1. 2. 3.` gives the later items their new numbers, and each
  item whose number changed has only its marker rewritten. An editor that
  keeps `1. 1. 1.` gives the new item 1.

### Leaf sources

```rust
impl Document {
    /// The inline source of every leaf the document holds: paragraphs,
    /// headings, table cells, and leaf and container directive labels, in
    /// document order, read in one pass over the source.
    pub fn leaf_sources(&self) -> Vec<(NodePath, InlineSource)>;
}
impl Paragraph {
    /// A paragraph whose content is `text`, parsed as `parse_inline` parses
    /// it, which keeps `text` and is written as it. Its inlines' spans are
    /// offsets in `text`.
    pub fn with_source(text: &str, definitions: &Definitions) -> (Self, Vec<Diagnostic>);
}
// The same on Heading (with its depth and kind), TableCell, and the two
// directives (for their label).
```

- `leaf_sources` runs the block parser once over the whole source in the
  mode `inline_source` uses today, so loading a document is linear; today
  `inline_source` once per leaf costs 8.2 s for a 2000-item list (67 KB),
  since each call re-parses the list.
- `inline_source(path)` now also answers for table cells and directive
  labels. A path into a cell is the table, the row, and the cell; a path to a
  directive's label is the directive's path.
- A cell's text is its content between the pipes with `\|` read as `|`. A
  directive label's text is the content between its brackets.
- `with_source` keeps the text only while the leaf's inlines are the ones it
  parsed from: the writer compares them with a fresh `parse_inline` of the
  kept text, as it compares a document with its source, and a leaf whose
  inlines were edited after it was built is written by inline-grain reuse
  against that text (below).

### Writing

`to_markdown` and `to_markdown_with` keep their signatures.

**Base.** For a document with a source, the writer parses the source once
and builds, in the same pass, a syntax map: for each base node, the byte
ranges of its own syntax (below), the ranges where its children start and
end, and for each leaf its inline source. Nothing is stored between writes.

**Matching.** A node with a span is the successor of the base node of the
same kind at the same span, if the base has one and no earlier node in
document order has claimed it; ProseMirror copies attributes when it splits
a node, so two nodes can carry one span. A node with no successor in the
base is new.

**Own syntax** of each kind, the bytes that are neither children nor gaps
between them:

| Node | Own syntax |
|---|---|
| ATX heading | opening sequence and the whitespace after it; closing sequence and the whitespace around it |
| Setext heading | the underline line |
| List item | marker, task checkbox, and the whitespace after them |
| Block quote, alert | each line's `>` and the whitespace after it; an alert's `[!KIND]` line |
| Fenced code, math block, directive | opening line, closing line |
| Indented code | each line's indentation |
| Table | the delimiter row |
| Table row | pipes, cell padding, and the excess cells past the delimiter row's width |
| Link, image, reference | everything outside the text or alt: brackets, destination, title |
| Emphasis-like | delimiters |
| Text directive | name, brackets, attributes |

**Write rules**, applied at every node from the top:

1. *Unchanged node, unchanged context*: the node equals its base and so does
   every ancestor's own syntax, and it follows the same sibling it followed in
   the base. It is written as the source slice of its span; the slice holds
   the ancestors' prefixes on its continuation lines, which are what the
   writer would write there.
2. *Unchanged node, changed context*: it is written as its content lines,
   the source lines with the base ancestors' prefixes removed, under the
   current prefixes. Lazy continuation lines take the full prefix.
3. *Own fields unchanged, children changed*: its own syntax is copied from
   the source, and its children are written by these rules.
4. *Own fields changed, or new*: its own syntax is rendered canonically, and
   its children are written by these rules.

Leaves follow the same rules with inline nodes as children: an edited leaf is
written as its inline content, each unchanged inline copied from the leaf's
inline source and each changed one rendered, then its lines prefixed. A leaf
with a kept source and unchanged inlines is written as that source.

**Gaps.** Between two siblings written by rule 1 or 3 that are adjacent in
the base, the source between them is kept; every other boundary gets the
canonical separator, with `BlankLine` nodes written as their lines. Before
the first and after the last top-level block, the source's leading and
trailing bytes are kept when that block is reused there, as today.

**Read-back guards.** Batch 1's guards move from the top level to every
container: a reused node that a blank line and a paragraph after it would
change (an unclosed fence) is rendered when it comes before a node other than
its base successor; a reused node whose source opens with indentation is
rendered after a node other than its base predecessor where that
indentation would read differently; a reused list after a list with the same
marker is rendered. Rule 3 adds two: a reused setext underline is rendered
when the rendered content's last line would read as something else above it,
and a reused fence is rendered when the changed code holds a line that would
close it. Each guard asks the parser, as batch 1's do, and costs time linear
in the node it checks.

**Separator fixes** found by the study: a blank line inside a list item is
written without trailing whitespace (`- a\n\n  b` is written as itself, not
`- a\n  \n  b`).

### Leaf text that opens a block

A leaf's text is written as it stands. A line in it that would open a block
where it is written (a paragraph whose second line is `# b`) reads back as
that block; the writer does not escape it (0013: rendering only renders,
and its output may read back differently). The editor sees the new structure
when it reloads the file.

### WebAssembly binding

- `parse` returns the tree, the diagnostics, and every leaf's inline source
  (text and offset map) from `leaf_sources`, in one call.
- `toMarkdown` accepts, on a paragraph, heading, table cell, or directive
  label, `source: string` in place of its inline children; the binding builds
  the leaf with `with_source` and the document's definitions.
- The wire gains `blankLine`, `ListItem.number`, and drops `List.start`;
  `index.d.ts` follows.

## Flows

- **Load.** `parse(file)` → tree, diagnostics, every leaf's text → Settle's
  adapter builds ProseMirror nodes: list items flattened, `BlankLine` as an
  empty paragraph, each node keeping its span as an attribute.
- **Keystroke.** Unchanged: `parseInline(text, definitions)`.
- **Save.** The adapter builds the tree from ProseMirror, every node with the
  span it was loaded with, leaves with `source` → `toMarkdown` → text →
  `parse` the text for the next base.
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
- Blank lines: each row of the blank-line table, as `.ast` goldens and as
  canonical round trips.
- Identity: every unedited parse in the corpus writes its source, and every
  document rebuilt from `leaf_sources` with `with_source` leaves writes its
  source.
- `leaf_sources` against `inline_source` and against the parse, over the
  corpus and the generators: `parse_inline(text)` yields the leaf's inlines
  once mapped.
- meowdown's 101 graded round-trip cases and its blank-line cases, ported as
  inputs whose writes are checked byte for byte where the rules promise it.
- Cost: `leaf_sources`, `with_source`, and writing with edits at depth in
  `tests/linear_growth.rs`; deep nesting in `tests/pathological_inputs.rs`.
- JS: each changed `wasm-binding` requirement, a save with `source` leaves,
  and a list edit that rewrites one item's marker only.

## Later phases

None in this crate. Settle's adapter (flattening, empty paragraphs, spans as
attributes, renumbering) is Settle's own plan.

## Decisions

- Own syntax copied from the source, not spelling fields — because the
  writer reuses the source anyway, and copying keeps whitespace and lazy
  lines no field records. Turned down: meowdown's fields, which every spec,
  validator, serializer branch, wire field, and editor attribute would carry,
  and which a rewrite-everything writer needs but this one does not.
- Leaves keep the text they were built from — because the editor's text is
  what the user typed and what the file must hold; rendering parsed inlines
  changes `` `` b `` `` to `` `b` `` and `[a]( x )` to `[a](x)`. Turned
  down: `Text` holding raw Markdown, which makes the tree lie about its
  meaning; leaves holding only text with inlines parsed on demand, which
  changes every Rust reader of the AST to save one `parse_inline` per edited
  leaf.
- Reuse down to inline nodes — because Settle's rename rewrites links in
  notes the user has not opened, and must not move the rest of a paragraph.
  Turned down: stopping at leaves, which renders the whole paragraph around
  one changed link.
- `BlankLine` nodes, K − 1 between blocks — because a blank line the user
  types is an edit the tree must express, and one implicit separator keeps
  canonical round trips stable. Turned down: empty paragraphs (meowdown),
  which surface as `<p></p>` and noise to the index and keep no validation
  rule for real empty paragraphs; a blank-line count on each block, which
  every block and the document's end would carry; every blank line as a
  node, which makes `[Paragraph, Paragraph]` read back with a node between.
- A number on each ordered item, `List.start` removed — because the number
  is what the source writes and what an editor renumbers; keeping `start`
  would store the first number twice. Turned down: renumbering in the
  writer, which rewrites items the user did not touch, or leaves
  `1. 2. 2. 3.` when it reuses them.
- Nested lists in the AST — because tightness and the marker are facts of
  the list; a flat AST stores them on every item or only on the first, and
  the HTML renderer and index would regroup. Turned down: meowdown's flat
  `listItem` siblings.
- Matching by span, first claimant wins — because the editor then reports
  nothing beyond the tree, as in batch 1. Turned down: node identifiers,
  which the AST and the wire would carry for every node.
- One-pass `leaf_sources` — because loading calls for every leaf. Turned
  down: caching the base parse inside `Document`, which doubles memory for
  callers that only read.

## Risks

- [A reused own syntax and a rendered child read back differently] → the
  simulated edits at every depth; a failure becomes a guard that renders the
  own syntax.
- [The writer is no longer linear when every node consults the base] → the
  syntax map is built in the base parse; matching is one sorted lookup per
  node; linear-growth tests at depth.
- [Two nodes claim one span] → first claimant wins, tested with a split
  paragraph and a split list item.
- [Settle's index breaks on `BlankLine`, `ListItem.number`, and the cell
  `Text`] → listed as breaking in the release notes; Settle upgrades in its
  own change.

## Open questions

- Whether a gap with no blank line (`# h\npara`) can be kept beside a node
  rendered by rule 3 whose kind did not change; the simulated edits decide
  it, and it changes no requirement.
