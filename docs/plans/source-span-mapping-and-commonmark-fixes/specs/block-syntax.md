# Block syntax — spec changes

## MODIFIED Requirements

### Requirement: CommonMark blocks
The parser SHALL recognize CommonMark block constructs (ATX and setext headings,
thematic breaks, indented and fenced code blocks, block quotes, lists, HTML
blocks, link reference definitions, and paragraphs) as the CommonMark
specification defines them.

#### Scenario: Heading and paragraph
- **WHEN** `"# Title\n\nHello."` is parsed
- **THEN** the document holds a `Heading` of depth 1 followed by a `Paragraph`

#### Scenario: Lazy list marker ends a block quote
- **WHEN** `"> a\n- "` is parsed with the CommonMark preset
- **THEN** the document holds a `BlockQuote` with a paragraph `a`, followed by a `List` holding one empty item

#### Scenario: Lazy ordered item not starting at 1
- **WHEN** `"> > a\n2. b"` is parsed with the CommonMark preset
- **THEN** the document holds a `BlockQuote` followed by an ordered `List` starting at 2

#### Scenario: Final whitespace of a paragraph
- **WHEN** `"aaa     \nbbb     "` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("aaa")`, a `LineBreak`, and `Text("bbb")`

#### Scenario: Final whitespace of a setext heading
- **WHEN** `"Foo  \n-----"` is parsed with the CommonMark preset
- **THEN** the document holds a level-2 setext `Heading` holding `Text("Foo")`

#### Scenario: CommonMark oracle cases
- **WHEN** the block cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** the output matches the expected HTML
