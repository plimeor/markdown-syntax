# Untrusted input cost — spec changes

## MODIFIED Requirements

### Requirement: Linear time
Parsing with `parse` and `parse_inline`, `to_markdown`, `to_html`, `validate`,
`Document::inline_source`, `Document::leaf_sources`, and the `with_source`
constructors SHALL take time linear in the input size: the input, the inline
text, or the document, its kept source, and the sources its leaves keep
together.
