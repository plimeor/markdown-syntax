# Public API — spec changes

## MODIFIED Requirements

### Requirement: One syntax
`parse(input)`, for a document, and `parse_inline(text, definitions)`, for the
inline content of one leaf block, SHALL be the only ways to parse, and both
SHALL recognize one fixed syntax with no configuration: CommonMark with raw HTML and indented code,
`<details>` HTML containers, GFM tables, task list items, literal autolinks,
and `~~` strikethrough, footnotes and inline footnotes, GitHub alerts,
frontmatter, shortcodes, `==` highlight, wiki links, math, directives, and
`#` tags.
Subscript, superscript, insert, spoiler, underline, description lists,
literal autolinks with a scheme other than `http`, `https`, `mailto`, and
`xmpp`, and MDX SHALL NOT be recognized.

#### Scenario: Removed marks stay text
- **WHEN** `parse("H~2~O and x^2^ ++a++ ||b||")` runs
- **THEN** the paragraph holds only text
