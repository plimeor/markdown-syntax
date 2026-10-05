# Public API — spec changes

## ADDED Requirements

### Requirement: Spans map stripped lines back to the source
A parsed node's span SHALL start at the source byte where its first character
was read and end after the source byte where its last character was read,
wherever the parser removes indentation, container markers, or table-cell
padding before reading a line, joins lines whose source line ending is `\r\n`,
or reads `\|` in a table cell as `|`; a space the parser produces by splitting a
tab SHALL map to that tab.

#### Scenario: Leading whitespace on a paragraph line
- **WHEN** `parse("  a *b*")` runs
- **THEN** the paragraph holds `Text("a ")` spanning bytes 2..4 and an `Emphasis` spanning bytes 4..7

#### Scenario: Block quote continuation line
- **WHEN** `parse("> a\n> b *c*")` runs
- **THEN** the block quote's paragraph spans bytes 2..11 and holds `Text("b ")` spanning bytes 6..8 and an `Emphasis` spanning bytes 8..11

#### Scenario: List item continuation line
- **WHEN** `parse("- a\n  b *c*")` runs
- **THEN** the item's paragraph spans bytes 2..11 and holds an `Emphasis` spanning bytes 8..11

#### Scenario: Later block inside a block quote
- **WHEN** `parse("> a\n>\n> b")` runs
- **THEN** the block quote's second paragraph spans bytes 8..9

#### Scenario: CRLF soft break
- **WHEN** `parse("a\r\nb")` runs
- **THEN** the paragraph holds a `SoftBreak` spanning bytes 1..3 and `Text("b")` spanning bytes 3..4

#### Scenario: Table cell content
- **WHEN** `parse("| a *b* |\n|-|")` runs
- **THEN** the header cell spans bytes 2..7 and holds `Text("a ")` spanning bytes 2..4 and an `Emphasis` spanning bytes 4..7

#### Scenario: Escaped pipe in a table cell
- **WHEN** `parse("| a\\|b |\n|-|")` runs
- **THEN** the header cell holds `Text("a|b")` spanning bytes 2..6

#### Scenario: Split tab
- **WHEN** `parse(">\t\tfoo")` runs
- **THEN** the block quote holds an indented code block with value `  foo` spanning bytes 1..6

### Requirement: Spans nest
Every parsed node's span SHALL lie on UTF-8 character boundaries within the
input and within the span of the node that contains it, and the spans of a
node's children SHALL be in source order and SHALL NOT overlap.

#### Scenario: Fixture corpus and generated inputs
- **WHEN** every fixture input and every seeded generated input is parsed in each dialect
- **THEN** every node, at every depth, satisfies these conditions

## MODIFIED Requirements

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

### Requirement: Emphasis-like spans cover their delimiters
The span of a parsed emphasis-like container (`Emphasis`, `Strong`,
`Underline`, `Delete`, `Insert`, `Mark`, `Spoiler`, `Subscript`, or
`Superscript`) SHALL run from the first character of the delimiters that open
it to the last character of the delimiters that close it, and SHALL lie within
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
