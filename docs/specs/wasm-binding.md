# WebAssembly binding
Type: api

## Purpose

The JavaScript surface through which an editor parses Markdown, parses one
leaf block's inline content, reads a leaf's inline source, and writes an
edited tree back, all with the core crate's own code compiled to WebAssembly.
Owned by the `markdown-syntax-wasm` crate in `wasm/` and its JS wrapper.

## Requirements

### Requirement: Build
`markdown-syntax-wasm` SHALL build for `wasm32-unknown-unknown` with MSRV 1.82
and no dependency other than the core crate, SHALL NOT be published, and SHALL
leave the core crate's build surface, dependencies, and default-build behavior
unchanged. The JS wrapper SHALL need no npm package and SHALL run on Node 22
and in WKWebView.

#### Scenario: Core build unchanged
- **WHEN** `cargo build` runs at the repository root
- **THEN** only the core crate builds, and `cargo tree -p markdown-syntax-wasm` lists the core crate as its only dependency

### Requirement: Parse from JavaScript
`MarkdownSyntax.load(wasm)` SHALL return a binding whose `parse(source)`
returns a `ParsedDocument` holding `tree` and `diagnostics`, where `tree`
holds, for every node of the document the core crate's `parse(source)`
returns, a plain object with `type` and the node's fields, named as in Rust in
camelCase, and its children in order.

#### Scenario: Heading and paragraph
- **WHEN** `binding.parse("# T\n\na *b*")` runs
- **THEN** `tree.children` holds a `heading` with `depth: 1` and a `paragraph` whose children are a `text` with `value: "a "` and an `emphasis` with `delimiter: "asterisk"`

### Requirement: UTF-16 offsets
Every `span` the binding returns or accepts SHALL be `[from, to]` in UTF-16
code units of the JavaScript string it refers to, or `null` for a node
without a span.

#### Scenario: Text beyond the BMP
- **WHEN** `binding.parse("😀 *a*")` runs
- **THEN** the `emphasis` span is `[3, 6]`

### Requirement: Inline parse and inline source from JavaScript
`binding.parseInline(text, definitions)` SHALL return the inlines and
diagnostics of the core crate's `parse_inline`, and
`parsedDocument.inlineSource(path)` the text of the core crate's
`inline_source` with `toSource(offset)` and `spanToSource(span)` functions
that map as its `to_source` and `span_to_source` do, or `null` where it
returns `None`; `parsedDocument.definitions` SHALL hold the labels of the
document's definitions.

#### Scenario: Paragraph in a list item
- **WHEN** `inlineSource([0, 0, 0])` runs on the document parsed from `"- a\n  *b*"`
- **THEN** it returns the text `"a\n*b*"`, and `toSource(2)` returns 6

### Requirement: Write from JavaScript
`parsedDocument.toMarkdown(tree, options)` SHALL return the text the core
crate's `to_markdown_with` writes for `tree` given this document's source, or
SHALL return the validation diagnostics when it refuses the tree.

#### Scenario: Untouched tree
- **WHEN** `doc.toMarkdown(doc.tree)` runs
- **THEN** it returns the source `doc` was parsed from

### Requirement: Malformed trees
A tree passed to `toMarkdown` that names an unknown `type`, lacks a field
its type needs, or holds a child of the wrong kind SHALL make the call throw a
`TypeError` naming the first such node's path, and so SHALL a tree nested
deeper than the binding's limit, which lies above the nesting of any tree
`parse` returns; the call SHALL leave the `ParsedDocument` usable.

### Requirement: Released memory
`parsedDocument.free()` SHALL release the memory the document holds in
WebAssembly; any later call on it SHALL throw. Calls on other documents SHALL
be unaffected.
