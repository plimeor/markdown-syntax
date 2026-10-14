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
item's marker. A `BlankLine`'s span SHALL be its line with its line ending,
from the line's start, or from where its container starts when that container
opens on the same line. This SHALL hold wherever the parser removes indentation,
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
`CharacterReference`, `Tag`, `List`, `ListItem`, `TableCell`, `LeafDirective`,
`ContainerDirective`, and `BlankLine`, all defaulting `meta` to no span, a
list item's number to `None`, and a directive's label and attributes to none;
`markdown_syntax::prelude::*` SHALL import this surface.

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
their delimiter is `*` or `_`, `Autolink` records whether it was written
in angle brackets `<destination>` or as a literal autolink, and an item of an
ordered list records the number it was written with, which for the first item
is the list's start. A `Link` is an inline link `[text](destination)`. A node
built with a constructor SHALL take the default spelling: `*`, and no number.

#### Scenario: Repeated numbers
- **WHEN** `parse("1. a\n1. b")` runs
- **THEN** both items record the number 1

### Requirement: Inline parse
`parse_inline(text, definitions)` SHALL read `text` as the inline content of
one leaf block, applying no block rule, and SHALL return an `InlineOutput {
inlines, tokens, diagnostics }`, a `#[non_exhaustive]` struct, for every
`&str` without panicking or returning an error. A reference link or image
SHALL form only for a label that `definitions` holds;
`Definitions::of(&document)` SHALL hold the labels of every link reference
definition in `document`, and `Definitions::new(labels)` the given labels.
Spans SHALL be byte ranges into `text`, nesting as "Source spans" states.
U+0000 SHALL be read as U+FFFD as `parse` reads it; a leading U+FEFF is
content.

#### Scenario: Block syntax stays inline
- **WHEN** `parse_inline("# a\n===", &Definitions::new::<_, &str>([]))` runs
- **THEN** the inlines are `Text("# a")`, a `SoftBreak`, and `Text("===")`

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

### Requirement: Inline source of a leaf block
For a document with a source, `document.inline_source(path)` SHALL return, for
the paragraph, heading, table cell, or leaf or container directive label at
`path`, an `InlineSource` holding the inline content the parser read for it
and a map from offsets in that text to offsets in `source`, such that
`parse_inline(text, &Definitions::of(&document))` yields the node's inlines
once their spans are mapped to the source. A cell's path is its table's, its
row's index, and its own; a directive label's path is its directive's. A
cell's text SHALL read each `\|` as `|`. It SHALL return `None` for a document
without a source, and when `path` names no such leaf, a directive written
without a label, or a node without a span.

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

### Requirement: Leaf sources
For a document with a source, `Document::leaf_sources()` SHALL return, in
document order, the path and the inline source of every leaf the document
holds (paragraphs, headings, table cells, and the labels of leaf and
container directives), each equal to what `inline_source` returns for that
path, in time linear in the source.

### Requirement: Leaves built from their source
`Paragraph::with_source`, `Heading::with_source`, `TableCell::with_source`,
and the `with_source` constructors of `LeafDirective` and
`ContainerDirective`, for their label, SHALL build the leaf whose inline
content `parse_inline(text, definitions)` reads from `text`, return its
diagnostics, and keep `text` as the leaf's source, which `source()` (for a
directive, `label_source()`) returns; the spans of its inlines SHALL be byte
ranges into `text`. No other way to give a leaf a source SHALL exist, and a
parsed or `new` leaf SHALL keep none. `Document::reread_kept_sources` SHALL
build the inlines of every leaf that keeps a source again from it, with the
document's definitions.

#### Scenario: Leaf built before its definition
- **WHEN** a document holding `[a]: /u` gets a paragraph built with `Paragraph::with_source("[a] [l]( x )", &Definitions::default())`, and `reread_kept_sources` runs
- **THEN** the paragraph opens with a reference to `a`, and the document writes it as `[a] [l]( x )`
