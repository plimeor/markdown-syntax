# Serialization

## Purpose

Turning a `Document`, parsed or hand-built, into canonical Markdown text. Owned
by `src/serialize.rs`.

## Requirements

### Requirement: Canonical output
`Document::to_markdown` SHALL emit canonical Markdown: one fixed spelling per
construct, chosen by `SerializeOptions`, independent of how the source spelled
it.

#### Scenario: Paragraph and heading
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `"# Title\n\nHello *world*.\n"`

### Requirement: Round-trip stability
For a parsed document, parsing the serialized Markdown SHALL yield the same AST
apart from spans, and serializing that reparsed document SHALL yield the same
text.

#### Scenario: Round-trip fixtures
- **WHEN** each fixture under `tests/fixtures/roundtrip/` is parsed, serialized, reparsed, and serialized again
- **THEN** the reparsed AST matches the first and the two serialized texts are identical

### Requirement: Serialize options
`SerializeOptions` SHALL control the line ending, the trailing newline, the
bullet marker, the ordered-list delimiter, and the code fence character, and
SHALL be constructed by mutating `SerializeOptions::default()`.

#### Scenario: CRLF without final newline
- **WHEN** `parse("# Title").document.to_markdown_with(&options)` runs with `line_ending = LineEnding::CrLf` and `final_newline = false`
- **THEN** it returns `"# Title"`

### Requirement: Invalid documents are rejected
Serialization SHALL validate the document first and return
`SerializeError::InvalidDocument` with the validation diagnostics when it is
invalid, and `SerializeError::UnsupportedNode` for a node kind it cannot write.

#### Scenario: Empty table
- **WHEN** a hand-built document holding a `Table` with no rows is serialized
- **THEN** `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`

### Requirement: Escaping keeps text literal
The serializer SHALL escape text so that reparsing the output yields the same
text rather than new constructs.

#### Scenario: Literal asterisks in text
- **WHEN** a hand-built paragraph holding `Text("*not emphasis*")` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same text and no `Emphasis`

### Requirement: No HTML filtering or style preservation
The serializer SHALL write raw HTML and MDX node values as they are, without
safety filtering, and SHALL NOT reproduce the source's original spelling from an
AST.

#### Scenario: Raw HTML passes through
- **WHEN** `parse("<script>alert(1)</script>").document.to_markdown()` runs
- **THEN** the output contains `<script>alert(1)</script>`
