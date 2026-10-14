# Public API — spec changes

## ADDED Requirements

### Requirement: Leaf sources
For a document with a source, `Document::leaf_sources()` SHALL return, in
document order, the path and the inline source of every leaf the document
holds (paragraphs, headings, table cells, and the labels of leaf and
container directives), each equal to what `inline_source` returns for that
path, in time linear in the source.

### Requirement: Leaves built from their source
`Paragraph::with_source`, `Heading::with_source`, `TableCell::with_source`,
and the `with_source` constructors of `LeafDirective` and
`ContainerDirective`, for their label, SHALL build the leaf whose inline
content `parse_inline(text, definitions)` reads from `text`, return its
diagnostics, and keep `text` as the leaf's source, which `source()` (for a
directive, `label_source()`) returns; the spans of its inlines SHALL be byte
ranges into `text`. No other way to give a leaf a source SHALL exist, and a
parsed or `new` leaf SHALL keep none.

## MODIFIED Requirements

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Document`, `Text`, `Paragraph`, `Heading`, `Link`, `Autolink`, `CodeInline`,
`CharacterReference`, `Tag`, `List`, `ListItem`, `TableCell`, `LeafDirective`,
`ContainerDirective`, and `BlankLine`, all defaulting `meta` to no span, a
list item's number to `None`, and a directive's label and attributes to none;
`markdown_syntax::prelude::*` SHALL import this surface.

### Requirement: Recorded syntax forms
Where one node kind has more than one Markdown spelling, a parsed node SHALL
record the spelling its source used: `Emphasis` and `Strong` record whether
their delimiter is `*` or `_`, `Autolink` records whether it was written
in angle brackets `<destination>` or as a literal autolink, and an item of an
ordered list records the number it was written with, which for the first item
is the list's start. A `Link` is an inline link `[text](destination)`. A node
built with a constructor SHALL take the default spelling: `*`, and no number.

#### Scenario: Repeated numbers
- **WHEN** `parse("1. a\n1. b")` runs
- **THEN** both items record the number 1

### Requirement: Inline source of a leaf block
For a document with a source, `document.inline_source(path)` SHALL return, for
the paragraph, heading, table cell, or leaf or container directive label at
`path`, an `InlineSource` holding the inline content the parser read for it
and a map from offsets in that text to offsets in `source`, such that
`parse_inline(text, &Definitions::of(&document))` yields the node's inlines
once their spans are mapped to the source. A cell's path is its table's, its
row's index, and its own; a directive label's path is its directive's. A
cell's text SHALL read each `\|` as `|`. It SHALL return `None` for a document
without a source, and when `path` names no such leaf, a directive written
without a label, or a node without a span.

#### Scenario: Paragraph in a block quote
- **WHEN** `inline_source(&NodePath::from([0, 0]))` runs on the document parsed from `source = "> a *b*\n> c"`
- **THEN** the text is `"a *b*\nc"`, and offset 6 maps to source byte 10
