# Public API
Type: api

## Purpose

The Rust surface through which callers parse Markdown, read diagnostics and
source positions, build ASTs by hand, and ask a `Document` for Markdown, HTML,
or validation results. Owned by the crate root and its `ast` module.

## Requirements

### Requirement: Build surface
The crate SHALL build as `no_std + alloc` with an empty default feature set, zero
runtime dependencies, and MSRV 1.82; the `html` feature SHALL be additive and
keep the same constraints.

### Requirement: Infallible parse
`parse(input)` SHALL return a `ParseOutput { document, diagnostics }` for every
`&str` input without panicking or returning an error; parse problems SHALL be
reported as diagnostics.

### Requirement: Output verbs on Document
A `Document` SHALL offer `to_markdown()`, `to_markdown_with(&SerializeOptions)`,
`inline_source(&NodePath)`, `source()`, `without_source()`, and `validate()`,
and with the `html` feature `to_html()` and `to_html_with(&HtmlOptions)`.

#### Scenario: Round trip through the document
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `Ok("# Title\n\nHello *world*.")`, the source unchanged

### Requirement: Serialize option types
`SerializeOptions` SHALL be `#[non_exhaustive]` and hold `line_ending:
LineEnding`, `final_newline: bool`, `bullet: Option<BulletMarker>`,
`ordered_delimiter: Option<OrderedDelimiter>`, and `fence_marker:
Option<FenceMarker>`, each marker option `None` by default. `BulletMarker`
SHALL hold only `Dash`, `Asterisk`, and `Plus`, and `OrderedDelimiter` only
`Period` and `Paren`. Both SHALL be exported from the crate root and the
prelude.

### Requirement: One diagnostic type
Parser diagnostics, AST validation, and serializer and HTML pre-validation SHALL
all report a single `Diagnostic { severity, code, span: Option<Span>, message }`.

### Requirement: Source spans
Every parsed node SHALL carry an absolute, half-open UTF-8 byte range into the
original input; hand-built nodes SHALL carry `None`; `LineIndex` SHALL convert a
span to 1-based line and column positions. A parsed node's span SHALL end
after the source byte where its last character was read, and SHALL start at
the source byte where its first character was read, or for a block, where its
first line starts after the markers and indentation of the containers around
it; a task item's checkbox and the space or tab after it count as part of the
item's marker. This SHALL hold wherever the parser removes indentation,
container markers, or table-cell padding before reading a line, or joins lines
whose source line ending is `\r\n`; a space the parser produces by splitting a
tab SHALL map to that tab. The span of a parsed emphasis-like container
(`Emphasis`, `Strong`, `Delete`, or `Mark`) SHALL run from the first character
of the delimiters that open it to the last character of the delimiters that
close it. Every parsed node's span SHALL lie on UTF-8 character boundaries
within the input and within the span of the node that contains it, and the
spans of a node's children SHALL be in source order and SHALL NOT overlap.

#### Scenario: Task item text
- **WHEN** `parse("- [ ] task")` runs
- **THEN** the item's paragraph and its `Text("task")` both span bytes 6..10

#### Scenario: Block quote continuation line
- **WHEN** `parse("> a\n> b *c*")` runs
- **THEN** the block quote's paragraph spans bytes 2..11 and holds `Text("b ")` spanning bytes 6..8 and an `Emphasis` spanning bytes 8..11

#### Scenario: Strong inside emphasis
- **WHEN** `parse("***a***")` runs
- **THEN** the paragraph holds an `Emphasis` spanning bytes 0..7 that holds a `Strong` spanning bytes 1..6

### Requirement: Top-level spans tile the source
The spans of a parsed document's top-level blocks SHALL be in source order,
non-overlapping, on UTF-8 character boundaries, within the input, and separated
only by whitespace, apart from one leading U+FEFF before the first block.

#### Scenario: Leading BOM
- **WHEN** `"\u{feff}# title\n\nbody"` is parsed
- **THEN** the first top-level span starts at byte 3 and the tiling holds with the BOM as the leading gap

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Document`, `Text`, `Paragraph`, `Heading`, `Link`, `Autolink`, `CodeInline`,
`CharacterReference`, `Tag`, and `List`, all defaulting `meta` to no span; `markdown_syntax::prelude::*` SHALL import this surface.

### Requirement: Byte order mark and NUL
The parser SHALL ignore one leading U+FEFF and SHALL treat every U+0000 as
U+FFFD, both when recognizing structure and in every node value, while all spans
stay in the coordinates of the original input.

#### Scenario: NUL in text
- **WHEN** `parse("a\u{0}b")` runs
- **THEN** the paragraph text is `a\u{FFFD}b` and its span covers bytes 0..3

### Requirement: Each written fact is stored once
A node SHALL hold what its source writes once and derive the rest from it.
`CharacterReference` SHALL hold only the reference as written, and
`CharacterReference::value()` SHALL return the character it decodes to, or
`None` when the reference is not exactly one character reference.
`CodeInline` SHALL hold only its value, as the parser normalizes it.
`Autolink` SHALL hold its form and its text as written, and
`Autolink::destination()` SHALL return the link target that text writes: an
angle-bracket URI itself and an angle-bracket email after `mailto:`; a literal
`www.` domain after `http://`, a literal email without a scheme after
`mailto:`, and any other literal autolink itself; or `None` when the text is
not exactly one autolink of its form.
A `Link`, an `Image`, and a `Definition` SHALL hold a title, when they have
one, as one `Title` holding its value and the quotes it is written in, so a
title without quotes or quotes without a title cannot be built.

#### Scenario: Text the parser would trim
- **WHEN** `destination()` is called on a hand-built literal `Autolink` holding `"http://a.b."`
- **THEN** it returns `None`

### Requirement: Shortcode glyph
`Shortcode::glyph()` SHALL return the emoji that the crate's pinned gemoji
table gives the shortcode's name, and `None` for a name the table does not
hold.

### Requirement: Wiki link decoding
`WikiLink::decoded_target()` and `WikiLink::decoded_label()` SHALL return the
target and the label with each backslash escape of an ASCII punctuation
character replaced by that character and each valid character reference
replaced by the text it names, the decoding CommonMark applies to a link
destination. A backslash before any other character SHALL stay as written.

#### Scenario: Escapes and references
- **WHEN** `decoded_target()` and `decoded_label()` are called on the `WikiLink` parsed from `"[[a\\|b &amp; c|x &#65; \\y]]"`
- **THEN** they return `a|b & c` and `x A \y`

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

### Requirement: Recorded syntax forms
Where one node kind has more than one Markdown spelling, a parsed node SHALL
record the spelling its source used: `Emphasis` and `Strong` record whether
their delimiter is `*` or `_`, and `Autolink` records whether it was written
in angle brackets `<destination>` or as a literal autolink. A `Link` is an
inline link `[text](destination)`. A node built with a constructor SHALL take
the default spelling: `*`.

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

### Requirement: Parsed documents keep their source
The document `parse(input)` returns SHALL hold `input` as its source, which
`source()` SHALL return; a document built with `Document::new` or converted
from blocks SHALL hold none, `without_source()` SHALL return the same document
without one, and no other way to give a document a source SHALL exist.

#### Scenario: Hand-built document
- **WHEN** `Document::new(vec![Paragraph::new([Text::from("a")]).into()]).source()` runs
- **THEN** it returns `None`
