# Validation

## Purpose

Checking that a `Document`, usually a hand-built one, has a shape the serializer
and renderer can write faithfully. Owned by `src/validate.rs`.

## Requirements

### Requirement: Validate reports invalid shapes
`Document::validate` SHALL return one error-severity `InvalidDocument`
diagnostic per invalid node it finds, and an empty list for a valid document.

#### Scenario: Parsed document
- **WHEN** `parse("# Title\n\nHello *world*.").document.validate()` runs
- **THEN** it returns an empty list

#### Scenario: Empty table
- **WHEN** a document holding a `Table` with no rows is validated
- **THEN** the result holds an `InvalidDocument` error

### Requirement: Shapes that cannot be written
Validation SHALL reject: a heading depth outside 1–6; a table with no rows or
no columns; empty inline math;
an empty emphasis-like container; an escape of a non-punctuation character; an
autolink containing whitespace or angle brackets; inline code whose raw text
holds a backtick run exactly as long as its fence; an ordered list start beyond
the parser's 9-digit marker limit; a hard line break ending inline content
other than link text, image alt text, an inline footnote, or a text directive
label, all of which close with a `]`; and a definition with an empty or blank
identifier.

#### Scenario: Empty emphasis
- **WHEN** a paragraph holding an `Emphasis` with no children is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Non-punctuation escape
- **WHEN** a paragraph holding an `Escape` of `a` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Hard line break ending a paragraph
- **WHEN** a paragraph whose last inline is a `LineBreak` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Hard line break ending link text
- **WHEN** `parse("[a\\\n](u)").document.validate()` runs
- **THEN** it returns an empty list, and `to_markdown()` writes `[a\\\n](u)\n`

### Requirement: Conservative scope
Validation SHALL check only the shapes listed by this spec and SHALL NOT be
relied on to prove every semantic invariant of a hand-built AST.

#### Scenario: Unlisted oddity
- **WHEN** a hand-built document has a shape this spec does not list as invalid
- **THEN** `validate()` may return an empty list
