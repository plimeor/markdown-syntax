# WebAssembly binding — spec changes

## MODIFIED Requirements

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
