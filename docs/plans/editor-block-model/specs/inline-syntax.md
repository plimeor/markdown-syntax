# Inline syntax — spec changes

## MODIFIED Requirements

### Requirement: Escapes and character references are nodes
The parser SHALL produce an `Escape` for every backslash escape and a
`CharacterReference` for every entity or numeric character reference in
inline content, and SHALL NOT fold either into a neighbouring `Text`; in a
table cell, a `\|` SHALL read as `|`, which is `Text` outside raw-text
constructs. Inside a code span, autolink, raw HTML, or another construct whose
content is raw text, a backslash or `&` stays part of that raw text, except
that in a table cell a `\|` inside such a construct (a code span, inline math,
raw HTML, an autolink, a wikilink, or a directive's attributes) is read as
`|` in its value.

#### Scenario: Escaped pipe inside raw HTML in a table cell
- **WHEN** `"| <a b=\"x\\|y\"> |\n|-|"` is parsed
- **THEN** the header cell holds one `Html` inline whose value is `<a b="x|y">`
