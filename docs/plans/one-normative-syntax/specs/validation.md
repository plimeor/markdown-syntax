# Validation — spec changes

## MODIFIED Requirements

### Requirement: Shapes that cannot be written
Validation SHALL reject: a heading depth outside 1–6; a table with no rows or
no columns; empty inline math; an empty emphasis-like container (`Emphasis`,
`Strong`, `Delete`, or `Mark`), or one whose content starts or ends with a
space, a tab, a soft break, or a hard break; a `Link`, `LinkReference`, or
`WikiLink` inside the text of a `Link` or `LinkReference`; a `Link` recorded
as a literal or angle-bracket autolink whose content is not one `Text` that
such an autolink writes for its destination, or that has a title; two
adjacent lists in the same container written with the same marker and both
ordered or both unordered; an escape of a non-punctuation character; a
shortcode whose name is not in the crate's pinned gemoji table; a directive
whose name is not one or more runs of ASCII letters joined by single `-`
chars; inline code whose raw text holds a backtick run exactly as long as its
fence; an ordered list start beyond the parser's 9-digit marker limit; a hard
line break ending inline content other than link text, image alt text, an
inline footnote, or a text directive label, all of which close with a `]`;
and a definition whose identifier is empty or holds only spaces, tabs, and
line endings.

#### Scenario: Empty emphasis
- **WHEN** a paragraph holding an `Emphasis` with no children is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Space at an emphasis edge
- **WHEN** a paragraph holding an `Emphasis` around `Text("a ")` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Link inside a link
- **WHEN** a paragraph holding a `Link` whose children hold another `Link` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Autolink form that does not fit its content
- **WHEN** a `Link` to `http://a.b` recorded as a literal autolink, whose text is `x`, is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Adjacent lists with one marker
- **WHEN** a document holding two adjacent unordered `List`s that both use `-` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Directive name with a digit
- **WHEN** a paragraph holding a `TextDirective` named `h1` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Non-punctuation escape
- **WHEN** a paragraph holding an `Escape` of `a` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Unknown shortcode name
- **WHEN** a paragraph holding a `Shortcode` named `not_an_emoji_name` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Hard line break ending a paragraph
- **WHEN** a paragraph whose last inline is a `LineBreak` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Hard line break ending link text
- **WHEN** `parse("[a\\\n](u)").document.validate()` runs
- **THEN** it returns an empty list, and `to_markdown()` writes `[a\\\n](u)\n`

#### Scenario: No-break space in an angle-bracket autolink
- **WHEN** `parse("<http://a\u{a0}b>").document.validate()` runs
- **THEN** it returns an empty list, and the document holds a `Link` to `http://a\u{a0}b`

#### Scenario: Inline link whose text is an address
- **WHEN** `Link::new("mailto:a\u{a0}b@c.d", [Text::from("a\u{a0}b@c.d")])` is validated and serialized
- **THEN** validation returns an empty list, and `to_markdown()` writes it as `[a\u{a0}b@c.d](mailto:a\u{a0}b@c.d)`, which reads back as the same `Link`
