# The block model of an editor — design

## Overview

Batch 1 made the source the only truth at the grain of top-level blocks
(0013): a parsed `Document` keeps its source, and `to_markdown` copies each
unchanged top-level block. This plan adds what a ProseMirror editor holds
that the AST cannot yet express: blank lines the user typed, a leaf's text as
the user typed it, and the number written on each ordered item. The
`per-node-reuse` plan then carries source reuse down to every node.

Largest risk: a write that drops or moves a blank line the user typed, or
reads back with one more or one fewer. Met by a golden per row of the
blank-line table, the seeded round trip generating blank-line runs, and the
source read-back corpus.

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
to_markdown[_with] ──► base = parse(source) once
                       top-level blocks matched to the base by correspondence
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
  `with_source` constructors (below), with a read accessor. A parsed leaf
  holds none; its inline source is derived from the document's.
- In a table cell, `\|` reads as `Text("|")` where it was `Escape('|')`:
  the cell's inline source holds `|`, so `parse_inline` on it reproduces the
  cell, and the writer escapes every `|` in a cell by the existing cell rule.
  A code span in a cell already reads `\|` as `|`.
- Every reader of the AST in the crate takes the new shape: the HTML
  renderer renders a `BlankLine` as nothing and takes `<ol start>` from the
  first item's number; the comparison, source-map, and NUL-replacement passes
  gain a `BlankLine` arm.

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
| At the start or end of a block quote, alert, HTML container, or container directive (`>` lines, or blank lines inside the tags or fences) | K |
| Between two items of a list | K − 1, at the end of the earlier item's children |
| After the last block of a list item or footnote definition | to the container that holds the list or definition |
| A document of only blank lines | one node per line |

- A `BlankLine`'s span is its line, with its line ending.
- A `BlankLine` is written as one line holding the container prefix with its
  trailing whitespace removed (`>` in a block quote, nothing in a list item).
- Between items, one blank line still makes the list loose. `- a\n\n\n- b`
  is one loose list whose first item holds `[Paragraph, BlankLine]`, as
  CommonMark 0.31 reads two blank lines; meowdown splits it into two lists.
- A blank line inside an open leaf (fenced code, HTML block, math) is part of
  that leaf's value, as today.
- Validation: an empty `Paragraph` stays invalid. A `BlankLine` is invalid
  where no blank line can be written: first in a list item, last in a list's
  last item, first or last in a footnote definition (the parser gives those
  lines to the parent), and in a tight list.
- A blank line inside a list item is written without trailing whitespace
  (`- a\n\n  b` is written as itself, not `- a\n  \n  b`).

### Ordered item numbers

- Parse records each ordered item's number as written; bullet items record
  `None`.
- The writer writes a recorded number as it is, and `None` as the previous
  item's number plus one (1 for the first item).
- Validation keeps the overflow rule (at most nine digits) for every number,
  and rejects a number on a bullet item.
- Renumbering after an insertion is the caller's: an editor that inserts an
  item into `1. 2. 3.` gives the later items their new numbers. An editor
  that keeps `1. 1. 1.` gives the new item 1.

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
- A leaf's kept text counts only while its inlines are the ones it parsed
  from: the writer compares them with a fresh `parse_inline` of the kept
  text, as it compares a document with its source. A leaf whose inlines were
  edited after it was built is written from its inlines.

### Correspondence

Matching a node to its base by `==` fails for a leaf built from text: its
inlines' spans are offsets in its text, not in the document, and it holds a
kept source the base leaf does not. The editor sends every leaf as text, so
under `==` no block holding a leaf would ever be reused. The writer therefore
matches by one relation, **correspondence**, defined recursively:

- A node corresponds to a base node of the same kind when their spans are
  equal, their own fields are equal, and their children correspond pairwise.
- A leaf with a kept source, whose inlines are the ones it parsed from,
  corresponds to a base leaf when its kept text equals the base leaf's inline
  source text; its inlines are not compared further.
- Any other leaf corresponds when its inlines are equal, spans included.

At the top-level grain this plan writes, a block that corresponds to the base
block at its span is written as that span's source text, as `==` decides
today. A block that does not is written canonically, and a leaf with a kept
source inside it is written as that source, its lines under the prefixes the
block writes. The whole-document shortcut (the tree equals the base, so the
source is the output) uses the same relation.

### Leaf text that opens a block

A leaf's text is written as it stands. A line in it that would open a block
where it is written (a paragraph whose second line is `# b`) reads back as
that block; the writer does not escape it (0016: rendering only renders,
and its output may read back differently). The editor sees the new structure
when it reloads the file.

### WebAssembly binding

- `parse` returns the tree, the diagnostics, and every leaf's inline source
  (text and offset map) from `leaf_sources`, in one call.
- `toMarkdown` accepts, on a paragraph, heading, table cell, or directive
  label, `source: string` in place of its inline children; the binding builds
  the leaf with `with_source` and the document's definitions, and gives it
  the node's span.
- The wire gains `blankLine`, `ListItem.number`, and drops `List.start`;
  `index.d.ts` follows.

## Flows

- **Load.** `parse(file)` → tree, diagnostics, every leaf's text → Settle's
  adapter builds ProseMirror nodes: list items flattened, `BlankLine` as an
  empty paragraph, each node keeping its span as an attribute.
- **Keystroke.** Unchanged: `parseInline(text, definitions)`.
- **Save.** The adapter builds the tree from ProseMirror, every node with the
  span it was loaded with, leaves with `source` → `toMarkdown` → text →
  `parse` the text for the next base. Unedited top-level blocks are written
  as their source; an edited one is rendered with its leaves as their text.

## Test scope

- Blank lines: each row of the blank-line table, as `.ast` goldens and as
  canonical round trips; the seeded round trip generates blank-line runs.
- Identity: every unedited parse in the corpus writes its source, and every
  document rebuilt from `leaf_sources` with `with_source` leaves writes its
  source.
- `leaf_sources` against `inline_source` and against the parse, over the
  corpus: `parse_inline(text)` yields the leaf's inlines once mapped.
- HTML: the conformance bench passes every case it passes today.
- Cost: `leaf_sources` and `with_source` in `tests/linear_growth.rs`.
- JS: each changed `wasm-binding` requirement, and a save with every leaf
  sent as `source` that returns the source.

## Later phases

The `per-node-reuse` plan: source reused per node and per node's own syntax,
down to inlines. Settle's adapter (flattening, empty paragraphs, spans as
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
- Correspondence instead of `==` — because a leaf built from text carries
  spans in its own text and a kept source; comparing its text with the base
  leaf's is the comparison `==` makes for a parsed leaf. Turned down:
  rewriting a `with_source` leaf's spans into document coordinates, which
  needs the document's offset map at construction and breaks for a leaf with
  no base.
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
- One-pass `leaf_sources` — because loading calls for every leaf. Turned
  down: caching the base parse inside `Document`, which doubles memory for
  callers that only read.
- This plan and `per-node-reuse` as two plans — because this one delivers
  the editor's load and its text-preserving save on its own, and the
  per-node writer is the larger risk. Turned down: one plan, which holds the
  model changes until the writer is done.

## Risks

- [A blank line moves or doubles on write] → a golden per table row, the
  seeded round trip, and read-back of the corpus.
- [Settle's index breaks on `BlankLine`, `ListItem.number`, and the cell
  `Text`] → listed as breaking in the release notes; Settle upgrades in its
  own change.
- [A leaf built from text reads back differently, as a paragraph whose line
  opens a heading] → accepted (Leaf text that opens a block); the editor
  reloads.
