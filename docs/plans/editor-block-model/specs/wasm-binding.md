# WebAssembly binding — spec changes

## MODIFIED Requirements

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

### Requirement: Write from JavaScript
`parsedDocument.toMarkdown(tree, options)` SHALL return the text the core
crate's `to_markdown_with` writes for `tree` given this document's source, or
SHALL return the validation diagnostics when it refuses the tree. A
paragraph, heading, table cell, or directive in `tree` MAY carry `source: string`
in place of its inline content (for a directive, its label's); the binding
SHALL build it with the core crate's `with_source` and the definitions of
`tree`.

#### Scenario: Untouched tree
- **WHEN** `doc.toMarkdown(doc.tree)` runs
- **THEN** it returns the source `doc` was parsed from

#### Scenario: Leaf given as text
- **WHEN** `doc` is parsed from `"> a\n\nb\n"` and `toMarkdown` gets its tree with the quote's paragraph replaced by `{type: "paragraph", source: "x `` y ``", span: null}`
- **THEN** it returns `"> x `` y ``\n\nb\n"`
