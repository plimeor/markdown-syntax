# WebAssembly binding
Type: api

## Purpose

The JavaScript surface through which an editor parses Markdown, parses one
leaf block's inline content, reads a leaf's inline source, and writes an
edited tree back, all with the core crate's own code compiled to WebAssembly.
Owned by the `markdown-syntax-wasm` crate and its JS wrapper.

## Requirements

### Requirement: Build
`markdown-syntax-wasm` SHALL build for `wasm32-unknown-unknown` with MSRV 1.82
and no dependency other than the core crate, SHALL NOT be published, and SHALL
leave the core crate's build surface, dependencies, and default-build behavior
unchanged. The JS wrapper SHALL need no npm package and SHALL run on Node 22
and in WKWebView.

### Requirement: Parse from JavaScript
`MarkdownSyntax.load(wasm)` SHALL return a binding whose `parse(source)`
returns a `ParsedDocument` holding `tree`, `diagnostics`, and `leafSources`,
where `tree` holds, for every node of the document the core crate's
`parse(source)` returns, a plain object with `type` and the node's fields,
named as in Rust in camelCase, and its children in order, and `leafSources`
holds, for every entry of the core crate's `leaf_sources`, its path and its
text with `toSource(offset)` and `spanToSource(span)` functions.

#### Scenario: Heading and paragraph
- **WHEN** `binding.parse("# T\n\na *b*")` runs
- **THEN** `tree.children` holds a `heading` with `depth: 1` and a `paragraph` whose children are a `text` with `value: "a "` and an `emphasis` with `delimiter: "asterisk"`

### Requirement: What references and shortcodes stand for
Every `characterReference` in a tree or inline list the binding returns
SHALL hold `value`, the text the core crate's `CharacterReference::value`
returns for it, and every `shortcode` SHALL hold `glyph`, the text of
`Shortcode::glyph`, each `null` where that returns `None`. `toMarkdown`
SHALL ignore both, writing the node from its `reference` or `name`.

#### Scenario: Reference and shortcode
- **WHEN** `binding.parse("&amp; :smile:\n")` runs
- **THEN** its paragraph holds a `characterReference` with `reference: "&amp;"` and `value: "&"`, and a `shortcode` with `name: "smile"` and `glyph: "😄"`

#### Scenario: Ignored on the way back
- **WHEN** `toMarkdown` gets a paragraph built in JavaScript holding `{type: "characterReference", span: null, reference: "&lt;", value: "x"}`
- **THEN** it writes `&lt;`

### Requirement: UTF-16 offsets
Every `span` the binding returns or accepts SHALL be `[from, to]` in UTF-16
code units of the JavaScript string it refers to, or `null` for a node
without a span.

### Requirement: Inline parse and inline source from JavaScript
`binding.parseInline(text, definitions)` SHALL return the inlines, tokens,
and diagnostics of the core crate's `parse_inline`, each token as `{ span,
node, role }` with `role` named as in Rust in camelCase, and `nodes`, the
inline objects in the order of its `nodes()`, which a token's `node`
indexes; and `parsedDocument.inlineSource(path)` the text of the core
crate's `inline_source` with `toSource(offset)` and `spanToSource(span)`
functions that map as its `to_source` and `span_to_source` do, or `null`
where it returns `None`; `parsedDocument.definitions` SHALL hold the labels
of the document's definitions.

### Requirement: Write from JavaScript
`parsedDocument.toMarkdown(tree, options)` SHALL return the text the core
crate's `to_markdown_with` writes for `tree` given this document's source, or
SHALL return the validation diagnostics when it refuses the tree, each with
its `path` as an array of indices. A paragraph, heading, table cell, or
directive in `tree` MAY carry `source: string` in place of its inline
content (for a directive, its label's); the binding SHALL build it with the
core crate's `with_source` and the definitions of `tree`.

#### Scenario: Untouched tree
- **WHEN** `doc.toMarkdown(doc.tree)` runs
- **THEN** it returns the source `doc` was parsed from

#### Scenario: Leaf given as text
- **WHEN** `doc` is parsed from `"> a\n\nb\n"` and `toMarkdown` gets its tree with the quote's paragraph replaced by `{type: "paragraph", source: "x `` y ``", span: null}`
- **THEN** it returns `"> x `` y ``\n\nb\n"`

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
