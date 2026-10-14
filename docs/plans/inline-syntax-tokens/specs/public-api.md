# Public API — spec changes

## ADDED Requirements

### Requirement: Inline syntax tokens
The `tokens` of `parse_inline(text, definitions)` SHALL tile `text`: in
order, without gaps or overlaps, each an `InlineToken { span, node, role }`
holding a maximal non-empty run of bytes with one `node` and one `role`.
The top-level inlines SHALL tile `text`, and a container's children its
content between its own syntax. `node` SHALL be the innermost inline whose
span holds the bytes, as its index in `InlineOutput::nodes()`, the inlines
in pre-order through `Inline::children()`. `role` SHALL be the part of that
node the bytes are: one of its written fields, `Destination` (a link's,
image's, or autolink's), `Title` (a link's or image's, inside its
delimiters), `Label` (a full reference's second label, a footnote
reference's label, or a wiki link's text after its pipe), `Target` (a wiki
link's), `Name` (a directive's, shortcode's, or tag's), or `Attributes` (a
directive's, inside its braces); `Text`, its content: a text node, raw
HTML, a soft break, a hard break's line ending, an escaped character, a
character reference between `&` and `;`, and code or math between its
delimiters, apart from the padding its value drops; or `Syntax`, every
other byte.

#### Scenario: Link with a title
- **WHEN** `parse_inline("[a](<u> \"t\")", &Definitions::new::<_, &str>([]))` runs
- **THEN** the tokens are `[` (node 0, `Syntax`), `a` (node 1, `Text`), `](<` (node 0, `Syntax`), `u` (node 0, `Destination`), `> "` (node 0, `Syntax`), `t` (node 0, `Title`), and `")` (node 0, `Syntax`)

#### Scenario: Wiki link with a label
- **WHEN** `parse_inline("[[T|L]]", &Definitions::new::<_, &str>([]))` runs
- **THEN** the tokens are `[[` (`Syntax`), `T` (`Target`), `|` (`Syntax`), `L` (`Label`), and `]]` (`Syntax`), all of node 0

## MODIFIED Requirements

### Requirement: Inline parse
`parse_inline(text, definitions)` SHALL read `text` as the inline content of
one leaf block, applying no block rule, and SHALL return an `InlineOutput {
inlines, tokens, diagnostics }`, a `#[non_exhaustive]` struct, for every
`&str` without panicking or returning an error. A reference link or image SHALL form only for a label
that `definitions` holds; `Definitions::of(&document)` SHALL hold the labels
of every link reference definition in `document`, and
`Definitions::new(labels)` the given labels. Spans SHALL be byte ranges into
`text`, nesting as "Source spans" states. U+0000 SHALL be read as U+FFFD as
`parse` reads it; a leading U+FEFF is content.

#### Scenario: Block syntax stays inline
- **WHEN** `parse_inline("# a\n===", &Definitions::new::<_, &str>([]))` runs
- **THEN** the inlines are `Text("# a")`, a `SoftBreak`, and `Text("===")`
