# Validation — spec changes

## MODIFIED Requirements

### Requirement: Shapes that cannot be written
Validation SHALL reject: a heading depth outside 1–6; a table with no rows or
no columns; empty inline math;
an empty emphasis-like container; an escape of a non-punctuation character; an
angle-bracket autolink whose destination holds a space, an ASCII control
char, or an angle bracket; inline code whose raw text holds a backtick run
exactly as long as its fence; an ordered list start beyond the parser's
9-digit marker limit; a hard line break ending inline content other than link
text, image alt text, an inline footnote, or a text directive label, all of
which close with a `]`; and a definition whose identifier is empty or holds
only spaces, tabs, and line endings.

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

#### Scenario: No-break space in an angle-bracket autolink
- **WHEN** `parse("<http://a\u{a0}b>").document.validate()` runs
- **THEN** it returns an empty list, and the document holds an `Autolink` to `http://a\u{a0}b`
