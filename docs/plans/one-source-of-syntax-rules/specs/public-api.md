# Public API — spec changes

## ADDED Requirements

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

## MODIFIED Requirements

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
- **WHEN** `"\u{a0}e+@"` is parsed with `parse` and with the GFM preset
- **THEN** each call returns a document

#### Scenario: Generated whitespace and autolink pieces
- **WHEN** every seeded generated input built from Unicode whitespace chars and literal-autolink pieces (`www.`, `://`, `@`, `.`, `+`, `_`, letters, CJK punctuation, `[[`) is parsed in each dialect
- **THEN** no call panics

### Requirement: Spans map stripped lines back to the source
A parsed node's span SHALL end after the source byte where its last character
was read, and SHALL start at the source byte where its first character was
read, or for a block, where its first line starts after the markers and
indentation of the containers around it; a task item's checkbox and the space
after it count as part of the item's marker. This SHALL hold wherever the
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
