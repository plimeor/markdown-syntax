# Public API — spec changes

## ADDED Requirements

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
The span of a parsed emphasis-like container (`Emphasis`, `Strong`,
`Underline`, `Delete`, `Insert`, `Mark`, `Spoiler`, `Subscript`, or
`Superscript`) in a block whose lines carry no leading whitespace after their container
markers SHALL
run from the first character of the delimiters that open it to the last
character of the delimiters that close it, and SHALL lie within the span of the
node that contains it.

#### Scenario: Strong inside emphasis
- **WHEN** `parse("***a***")` runs
- **THEN** the paragraph holds an `Emphasis` spanning bytes 0..7 that holds a `Strong` spanning bytes 1..6

#### Scenario: Emphasis inside strong
- **WHEN** `parse("x ***a* b**")` runs
- **THEN** the paragraph holds a `Strong` spanning bytes 2..11 that holds an `Emphasis` spanning bytes 4..7

#### Scenario: Leftover opening delimiter
- **WHEN** `parse("**a*")` runs
- **THEN** the paragraph holds `Text("*")` spanning bytes 0..1 and an `Emphasis` spanning bytes 1..4

## MODIFIED Requirements

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

## REMOVED Requirements

### Requirement: Input is parsed without preprocessing
**Reason**: Replaced by "Byte order mark and NUL", which follows CommonMark's
U+0000 rule and the leading-BOM handling of cmark, comrak, and micromark while
keeping spans in original input coordinates.
