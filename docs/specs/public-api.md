# Public API

## Purpose

The Rust surface through which callers parse Markdown, read diagnostics and
source positions, build ASTs by hand, and ask a `Document` for Markdown, HTML,
or validation results. Owned by `src/lib.rs`, `src/diagnostic.rs`,
`src/span.rs`, and `src/ast.rs`.

## Requirements

### Requirement: Build surface
The crate SHALL build as `no_std + alloc` with an empty default feature set, zero
runtime dependencies, and MSRV 1.82; the `html` feature SHALL be additive and
keep the same constraints.

#### Scenario: Default build
- **WHEN** the crate is built with default features
- **THEN** it compiles for `wasm32-unknown-unknown` and with Rust 1.82, pulls in no runtime dependency, and exports no HTML renderer

#### Scenario: HTML feature
- **WHEN** the crate is built with `--features html`
- **THEN** `Document::to_html` and `Document::to_html_with` become available and the default-build parse, AST, and serializer behavior is unchanged

### Requirement: Infallible parse
`parse(input)` SHALL return a `ParseOutput { document, diagnostics }` for every
`&str` input without panicking or returning an error; parse problems SHALL be
reported as diagnostics.

#### Scenario: Clean input
- **WHEN** `parse("# Title\n\nHello *world*.")` runs
- **THEN** `diagnostics` is empty and the document holds a heading and a paragraph

#### Scenario: Problem input
- **WHEN** `parse(":::note\nunclosed container")` runs
- **THEN** a document is returned and `diagnostics` contains an error-severity `UnclosedDirectiveContainer`

#### Scenario: Multi-byte whitespace before an email-like run
- **WHEN** `"\u{a0}e+@"` is parsed
- **THEN** the call returns a document

#### Scenario: Generated whitespace and autolink pieces
- **WHEN** every seeded generated input built from Unicode whitespace chars and literal-autolink pieces (`www.`, `://`, `@`, `.`, `+`, `_`, letters, CJK punctuation, `[[`) is parsed
- **THEN** no call panics

### Requirement: Output verbs on Document
A `Document` SHALL offer `to_markdown()`, `to_markdown_with(&SerializeOptions)`,
and `validate()`, and with the `html` feature `to_html()` and
`to_html_with(&HtmlOptions)`.

#### Scenario: Round trip through the document
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `Ok("# Title\n\nHello *world*.\n")`

### Requirement: One diagnostic type
Parser diagnostics, AST validation, and serializer and HTML pre-validation SHALL
all report a single `Diagnostic { severity, code, span: Option<Span>, message }`.

#### Scenario: Parser diagnostic has a span
- **WHEN** a parse reports a diagnostic
- **THEN** its `span` is `Some` and lies within the input

#### Scenario: Validation diagnostic on a hand-built node
- **WHEN** `validate()` reports a problem on a node without a span
- **THEN** the diagnostic's `span` is `None` and its code is `InvalidDocument`

### Requirement: Source spans
Every parsed node SHALL carry an absolute, half-open UTF-8 byte range into the
original input; hand-built nodes SHALL carry `None`; `LineIndex` SHALL convert a
span to 1-based line and column positions.

#### Scenario: First block position
- **WHEN** `parse("# Title\n\nHello.")` runs and the first block's span is passed to `LineIndex::new(source).span(span)`
- **THEN** the start position is line 1, column 1

#### Scenario: Hand-built node
- **WHEN** a `Heading` is built with `Heading::new(1, [Text::from("Title")])`
- **THEN** its `span()` is `None`

#### Scenario: Empty table cell
- **WHEN** `parse("| a | |\n|-|-|")` runs
- **THEN** the second header cell's span is the empty range at byte 6, just before the pipe that closes it

#### Scenario: Missing table cell
- **WHEN** `parse("| a | b |\n|-|-|\n| c")` runs
- **THEN** the body row's second cell has no children and its span is the empty range at the end of the row, byte 19

### Requirement: Top-level spans tile the source
The spans of a parsed document's top-level blocks SHALL be in source order,
non-overlapping, on UTF-8 character boundaries, within the input, and separated
only by whitespace, apart from one leading U+FEFF before the first block.

#### Scenario: Multi-block input with CRLF
- **WHEN** an input of several blocks with CRLF line endings is parsed
- **THEN** slicing the input with each top-level span yields the block's source and every gap between spans is whitespace

#### Scenario: Unclosed fence
- **WHEN** an input ending in an unclosed code fence is parsed
- **THEN** the fence's span ends within the input and the tiling holds

#### Scenario: Leading BOM
- **WHEN** `"\u{feff}# title\n\nbody"` is parsed
- **THEN** the first top-level span starts at byte 3 and the tiling holds with the BOM as the leading gap

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Text`, `Paragraph`, `Heading`, `Link`, `Code`, and `List`, all defaulting `meta`
to no span; `markdown_syntax::prelude::*` SHALL import this surface.

#### Scenario: Build and serialize
- **WHEN** a `Document` is built from `Heading::new(1, [Text::from("Title")]).into()` and `Paragraph::new([Text::from("hello")]).into()`
- **THEN** `to_markdown()` returns `"# Title\n\nhello\n"`

### Requirement: Byte order mark and NUL
The parser SHALL ignore one leading U+FEFF and SHALL treat every U+0000 as
U+FFFD, both when recognizing structure and in every node value, while all spans
stay in the coordinates of the original input.

#### Scenario: Leading BOM
- **WHEN** `parse("\u{feff}# title")` runs
- **THEN** the document holds a `Heading` with text `title` whose span starts at byte 3

#### Scenario: BOM inside a line
- **WHEN** `parse("# hea\u{feff}ding")` runs
- **THEN** the heading text is `hea\u{feff}ding`

#### Scenario: NUL in text
- **WHEN** `parse("a\u{0}b")` runs
- **THEN** the paragraph text is `a\u{FFFD}b` and its span covers bytes 0..3

#### Scenario: NUL in a link destination
- **WHEN** `parse("[a](\u{0})")` runs
- **THEN** the paragraph holds a `Link` whose destination is `\u{FFFD}`

#### Scenario: NUL in an image destination
- **WHEN** `parse("![](\\#\u{0})")` runs
- **THEN** the paragraph holds an `Image` whose destination is `#\u{FFFD}`

### Requirement: Emphasis-like spans cover their delimiters
The span of a parsed emphasis-like container (`Emphasis`, `Strong`, `Delete`,
or `Mark`) SHALL run from the first character of the delimiters that open it
to the last character of the delimiters that close it, and SHALL lie within
the span of the node that contains it.

#### Scenario: Strong inside emphasis
- **WHEN** `parse("***a***")` runs
- **THEN** the paragraph holds an `Emphasis` spanning bytes 0..7 that holds a `Strong` spanning bytes 1..6

#### Scenario: Emphasis inside strong
- **WHEN** `parse("x ***a* b**")` runs
- **THEN** the paragraph holds a `Strong` spanning bytes 2..11 that holds an `Emphasis` spanning bytes 4..7

#### Scenario: Leftover opening delimiter
- **WHEN** `parse("**a*")` runs
- **THEN** the paragraph holds `Text("*")` spanning bytes 0..1 and an `Emphasis` spanning bytes 1..4

#### Scenario: Emphasis on a block quote continuation line
- **WHEN** `parse("> a\n> *b*")` runs
- **THEN** the block quote's paragraph holds an `Emphasis` spanning bytes 6..9

### Requirement: Spans map stripped lines back to the source
A parsed node's span SHALL end after the source byte where its last character
was read, and SHALL start at the source byte where its first character was
read, or for a block, where its first line starts after the markers and
indentation of the containers around it; a task item's checkbox and the space
or tab after it count as part of the item's marker. This SHALL hold wherever the
parser removes indentation, container markers, or table-cell padding before
reading a line, or joins lines whose source line ending is `\r\n`; a space
the parser produces by splitting a tab SHALL map to that tab.

#### Scenario: Leading whitespace on a paragraph line
- **WHEN** `parse("  a *b*")` runs
- **THEN** the paragraph holds `Text("a ")` spanning bytes 2..4 and an `Emphasis` spanning bytes 4..7

#### Scenario: Block quote continuation line
- **WHEN** `parse("> a\n> b *c*")` runs
- **THEN** the block quote's paragraph spans bytes 2..11 and holds `Text("b ")` spanning bytes 6..8 and an `Emphasis` spanning bytes 8..11

#### Scenario: List item continuation line
- **WHEN** `parse("- a\n  b *c*")` runs
- **THEN** the item's paragraph spans bytes 2..11 and holds an `Emphasis` spanning bytes 8..11

#### Scenario: Task item text
- **WHEN** `parse("- [ ] task")` runs
- **THEN** the item's paragraph and its `Text("task")` both span bytes 6..10

#### Scenario: Task item with inline markup
- **WHEN** `parse("- [x] done *x*")` runs
- **THEN** the item's paragraph spans bytes 6..14 and holds `Text("done ")` spanning bytes 6..11 and an `Emphasis` spanning bytes 11..14

#### Scenario: Ordered task item
- **WHEN** `parse("1. [ ] step")` runs
- **THEN** the item's paragraph and its `Text("step")` both span bytes 7..11

#### Scenario: Later block inside a block quote
- **WHEN** `parse("> a\n>\n> b")` runs
- **THEN** the block quote's second paragraph spans bytes 8..9

#### Scenario: Nested list item after multi-byte text
- **WHEN** `parse("- 项目\n  - 嵌套 [[library/工作/买菜]]\n")` runs
- **THEN** the nested item's `WikiLink` spans bytes 20..45

#### Scenario: Nested block quote
- **WHEN** `parse("> 外层\n> > 内层 [[A]]\n")` runs
- **THEN** the inner block quote's `WikiLink` spans bytes 20..25

#### Scenario: Alert body
- **WHEN** `parse("> [!NOTE]\n> 见 [[A]]\n")` runs
- **THEN** the alert's `WikiLink` spans bytes 16..21

#### Scenario: Footnote definition continuation line
- **WHEN** `parse("正文[^1]\n\n[^1]: 见 [[A]]\n    续 [[B]]\n")` runs
- **THEN** the footnote definition's second `WikiLink` spans bytes 36..41

#### Scenario: Inside an HTML container
- **WHEN** `parse("<details>\n<summary>更多</summary>\n\n- 项目\n  - [[A]]\n\n</details>\n")` runs
- **THEN** the nested item's `WikiLink` spans bytes 50..55

#### Scenario: Inside a container directive
- **WHEN** `parse(":::note\n- 项目\n  - [[A]]\n:::\n")` runs
- **THEN** the nested item's `WikiLink` spans bytes 21..26

#### Scenario: Last child of a container directive
- **WHEN** `parse(":::note\n```\nx\n```\n:::\n")` runs
- **THEN** the directive's fenced `CodeBlock` spans bytes 8..18, ending after its closing fence's line ending as the same block does at the top level

#### Scenario: Tab-indented nested list
- **WHEN** `parse("- 项目\n\t- 嵌套 [[A]]\n")` runs
- **THEN** the nested item's `WikiLink` spans bytes 19..24

#### Scenario: CRLF nested list
- **WHEN** `parse("- 项目\r\n  - 嵌套 [[A]]\r\n")` runs
- **THEN** the nested item's `WikiLink` spans bytes 21..26

#### Scenario: CRLF soft break
- **WHEN** `parse("a\r\nb")` runs
- **THEN** the paragraph holds a `SoftBreak` spanning bytes 1..3 and `Text("b")` spanning bytes 3..4

#### Scenario: Table cell content
- **WHEN** `parse("| a *b* |\n|-|")` runs
- **THEN** the header cell spans bytes 2..7 and holds `Text("a ")` spanning bytes 2..4 and an `Emphasis` spanning bytes 4..7

#### Scenario: Escaped pipe in a table cell
- **WHEN** `parse("| a\\|b |\n|-|")` runs
- **THEN** the header cell spans bytes 2..6 and holds `Text("a")` spanning bytes 2..3, `Escape('|')` spanning bytes 3..5, and `Text("b")` spanning bytes 5..6

#### Scenario: Escaped pipe opening a table cell
- **WHEN** `parse("| \\|a |\n|-|")` runs
- **THEN** the header cell holds `Escape('|')` spanning bytes 2..4 and `Text("a")` spanning bytes 4..5

#### Scenario: Split tab
- **WHEN** `parse(">\t\tfoo")` runs
- **THEN** the block quote holds an indented code block with value `"  foo\n"` spanning bytes 1..6

#### Scenario: Container span regression cases
- **WHEN** `inline_spans_address_source_inside_containers` in `tests/parse_span_contract.rs` parses each of its 31 cases (lists, task lists, block quotes, alerts, tables, footnote definitions, HTML containers, container directives, frontmatter, CRLF, and tabs)
- **THEN** every `WikiLink`, `Link`, `Image`, and `#`-holding `Text` it collects spans exactly the literal it occupies in the input

### Requirement: Spans nest
Every parsed node's span SHALL lie on UTF-8 character boundaries within the
input and within the span of the node that contains it, and the spans of a
node's children SHALL be in source order and SHALL NOT overlap.

#### Scenario: Fixture corpus and generated inputs
- **WHEN** every fixture input and every seeded generated input is parsed
- **THEN** every node, at every depth, satisfies these conditions

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

#### Scenario: Character reference value
- **WHEN** `value()` is called on the `CharacterReference` parsed from `"&amp;"`
- **THEN** it returns `Some("&")`

#### Scenario: Hand-built reference that does not decode
- **WHEN** `value()` is called on a hand-built `CharacterReference` holding `"&amp"`
- **THEN** it returns `None`

#### Scenario: Literal autolink destination
- **WHEN** `destination()` is called on the `Autolink` parsed from `"www.a.b"`
- **THEN** it returns `Some("http://www.a.b")`

#### Scenario: Text the parser would trim
- **WHEN** `destination()` is called on a hand-built literal `Autolink` holding `"http://a.b."`
- **THEN** it returns `None`

### Requirement: Shortcode glyph
`Shortcode::glyph()` SHALL return the emoji that the crate's pinned gemoji
table gives the shortcode's name, and `None` for a name the table does not
hold.

#### Scenario: Known name
- **WHEN** `glyph()` is called on the `Shortcode` parsed from `":tada:"`
- **THEN** it returns `Some("🎉")`

#### Scenario: Hand-built unknown name
- **WHEN** `glyph()` is called on a hand-built `Shortcode` named `not_an_emoji_name`
- **THEN** it returns `None`

### Requirement: One syntax
`parse(input)` SHALL be the only way to parse, and SHALL recognize one fixed
syntax with no configuration: CommonMark with raw HTML and indented code,
`<details>` HTML containers, GFM tables, task list items, literal autolinks,
and `~~` strikethrough, footnotes and inline footnotes, GitHub alerts,
frontmatter, shortcodes, `==` highlight, wiki links, math, and directives.
Subscript, superscript, insert, spoiler, underline, description lists,
scheme-less and non-HTTP literal autolinks, and MDX SHALL NOT be recognized.

#### Scenario: Double underscore is strong
- **WHEN** `parse("a __b__ c")` runs
- **THEN** `__b__` is `Strong`

#### Scenario: Removed marks stay text
- **WHEN** `parse("H~2~O and x^2^ ++a++ ||b||")` runs
- **THEN** the paragraph holds only text

#### Scenario: GFM strikethrough
- **WHEN** `parse("~~done~~")` runs
- **THEN** the paragraph holds a `Delete`

### Requirement: Recorded syntax forms
Where one node kind has more than one Markdown spelling, a parsed node SHALL
record the spelling its source used: `Emphasis` and `Strong` record whether
their delimiter is `*` or `_`, and `Autolink` records whether it was written
in angle brackets `<destination>` or as a literal autolink. A `Link` is an
inline link `[text](destination)`. A node built with a constructor SHALL take
the default spelling: `*`.

#### Scenario: Underscore delimiters
- **WHEN** `parse("_a_ __b__")` runs
- **THEN** the `Emphasis` and the `Strong` both record the `_` delimiter

#### Scenario: Link forms
- **WHEN** `parse("www.a.b <http://c.d> [e](f)")` runs
- **THEN** the paragraph holds a literal `Autolink`, an angle-bracket `Autolink`, and a `Link`, in that order
