# Public API — spec changes

## ADDED Requirements

### Requirement: Inline parse
`parse_inline(text, definitions)` SHALL read `text` as the inline content of
one leaf block, applying no block rule, and SHALL return an `InlineOutput {
inlines, diagnostics }` for every `&str` without panicking or returning an
error. A reference link or image SHALL form only for a label that
`definitions` holds; `Definitions::of(&document)` SHALL hold the labels of
every link reference definition in `document`, and `Definitions::new(labels)`
the given labels. Spans SHALL be byte ranges into `text`, nesting as "Spans
nest" states. U+0000 SHALL be read as U+FFFD as `parse` reads it; a leading
U+FEFF is content.

#### Scenario: Block syntax stays inline
- **WHEN** `parse_inline("# a\n===", &Definitions::new::<_, &str>([]))` runs
- **THEN** the inlines are `Text("# a")`, a `SoftBreak`, and `Text("===")`

#### Scenario: Definitions from the document
- **WHEN** `parse_inline("[x][r]", &Definitions::of(&doc))` runs, where `doc` was parsed from `"[r]: /u"`
- **THEN** the inlines hold one `LinkReference` with identifier `r`, spanning bytes 0..6

### Requirement: Inline source of a leaf block
For a document with a source, `document.inline_source(path)` SHALL return, for the paragraph or heading at `path`, an `InlineSource`
holding the inline content the parser read for it and a map from offsets in
that text to offsets in `source`, such that `parse_inline(text,
&Definitions::of(&document))` yields the node's inlines once their spans are
mapped to the source. It SHALL return `None` for a document without a source,
and when `path` names no paragraph or heading or a node without a span; for
table cells and directive labels it is unspecified.

#### Scenario: Paragraph in a block quote
- **WHEN** `inline_source(&NodePath::from([0, 0]))` runs on the document parsed from `source = "> a *b*\n> c"`
- **THEN** the text is `"a *b*\nc"`, and offset 6 maps to source byte 10

#### Scenario: ATX heading
- **WHEN** `inline_source(&NodePath::from([0]))` runs on the document parsed from `source = "## T *x* ##"`
- **THEN** the text is `"T *x*"`, and offset 0 maps to source byte 3

### Requirement: Parsed documents keep their source
The document `parse(input)` returns SHALL hold `input` as its source, which
`source()` SHALL return; a document built with `Document::new` or converted
from blocks SHALL hold none, `without_source()` SHALL return the same document
without one, and no other way to give a document a source SHALL exist.

#### Scenario: Hand-built document
- **WHEN** `Document::new(vec![Paragraph::new([Text::from("a")]).into()]).source()` runs
- **THEN** it returns `None`

## MODIFIED Requirements

### Requirement: Output verbs on Document
A `Document` SHALL offer `to_markdown()`, `to_markdown_with(&SerializeOptions)`,
`inline_source(&NodePath)`, `source()`, `without_source()`, and `validate()`,
and with the `html` feature `to_html()` and `to_html_with(&HtmlOptions)`.

#### Scenario: Round trip through the document
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `Ok("# Title\n\nHello *world*.")`, the source unchanged

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Document`, `Text`, `Paragraph`, `Heading`, `Link`, `Autolink`, `CodeInline`,
`CharacterReference`, and `List`, all defaulting `meta` to no span; `markdown_syntax::prelude::*` SHALL import this surface.

#### Scenario: Build and serialize
- **WHEN** a `Document` is built with `Document::new` from `Heading::new(1, [Text::from("Title")]).into()` and `Paragraph::new([Text::from("hello")]).into()`
- **THEN** `to_markdown()` returns `"# Title\n\nhello\n"`

### Requirement: One syntax
`parse(input)`, for a document, and `parse_inline(text, definitions)`, for the
inline content of one leaf block, SHALL be the only ways to parse, and both
SHALL recognize one fixed syntax with no configuration: CommonMark with raw HTML and indented code,
`<details>` HTML containers, GFM tables, task list items, literal autolinks,
and `~~` strikethrough, footnotes and inline footnotes, GitHub alerts,
frontmatter, shortcodes, `==` highlight, wiki links, math, and directives.
Subscript, superscript, insert, spoiler, underline, description lists,
literal autolinks with a scheme other than `http`, `https`, `mailto`, and
`xmpp`, and MDX SHALL NOT be recognized.

#### Scenario: Double underscore is strong
- **WHEN** `parse("a __b__ c")` runs
- **THEN** `__b__` is `Strong`

#### Scenario: Removed marks stay text
- **WHEN** `parse("H~2~O and x^2^ ++a++ ||b||")` runs
- **THEN** the paragraph holds only text

#### Scenario: GFM strikethrough
- **WHEN** `parse("~~done~~")` runs
- **THEN** the paragraph holds a `Delete`
