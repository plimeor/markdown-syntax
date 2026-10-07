# Validation

## Purpose

Checking that a `Document`, usually a hand-built one, has a shape the serializer
and renderer can write faithfully. Owned by `src/validate.rs`.

## Requirements

### Requirement: Validate reports invalid shapes
`Document::validate` SHALL return one error-severity `InvalidDocument`
diagnostic per invalid shape it finds, and an empty list for a valid document.

#### Scenario: Parsed document
- **WHEN** `parse("# Title\n\nHello *world*.").document.validate()` runs
- **THEN** it returns an empty list

#### Scenario: Empty table
- **WHEN** a document holding a `Table` with no rows is validated
- **THEN** the result holds an `InvalidDocument` error

### Requirement: Shapes that cannot be written
Validation SHALL reject exactly these shapes, each visible from the node, its
children, or its sibling blocks:

- Block sequences: two adjacent `List`s that are both ordered or both
  unordered and record the same delimiter.
- `Heading`: a depth outside 1–6.
- `Table`: no rows; a header row with no cells; an alignment count other than
  the header row's width; a row whose width differs from the header row's.
- `List`: an ordered list whose start is beyond the parser's 9-digit marker
  limit.
- `Definition`: an identifier that is empty or holds only spaces, tabs, and
  line endings.
- `FootnoteDefinition`, `FootnoteReference`, `LinkReference`, and
  `ImageReference`: an empty identifier.
- `HtmlContainer`: an empty opening or closing tag name, opening and closing
  tag names that differ, or an empty opening or closing tag source.
- `LeafDirective`, `ContainerDirective`, and `TextDirective`: a name that is
  not one or more runs of ASCII letters joined by single `-` chars; an
  attribute name that does not start with an ASCII letter, `_`, or `-`, or
  holds a char other than ASCII letters, digits, `_`, `-`, and `:`.
- Inline content of a paragraph, a heading, a table cell, an HTML container,
  a leaf or container directive label, or an emphasis-like container: a
  `LineBreak` as its last inline. Link text, image alt text, an inline
  footnote, and a text directive label close with a `]` and may end with one.
- `Emphasis`, `Strong`, `Delete`, and `Mark`: no children, or content that
  starts or ends with a space, a tab, a `SoftBreak`, or a `LineBreak`.
- The text of a `Link` or `LinkReference`: a `Link`, `Autolink`,
  `LinkReference`, or `WikiLink` at any depth.
- `Autolink`: a text that is not exactly one autolink of its form, so that
  `Autolink::destination()` returns `None`.
- `Escape`: a value that is not an ASCII punctuation char.
- `CharacterReference`: a reference that is not exactly one character
  reference.
- `Shortcode`: a name not in the crate's pinned gemoji table.
- `WikiLink`: an empty target.
- `CodeInline`: an empty value, or a value holding a line ending.
- `MathInline`: an empty value; a dollar fence of length 0; code-form math
  whose value holds a backtick followed by `$`, which would close it early.

#### Scenario: Empty inline math
- **WHEN** a paragraph holding a `MathInline` with an empty value, in the dollar or the code form, is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Code-form math holding its close
- **WHEN** a paragraph holding a code-form `MathInline` whose value is ``a`$b`` is validated and serialized
- **THEN** the result holds an `InvalidDocument` error, and `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`

#### Scenario: Empty emphasis
- **WHEN** a paragraph holding an `Emphasis` with no children is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Space at an emphasis edge
- **WHEN** a paragraph holding an `Emphasis` around `Text("a ")` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Link inside a link
- **WHEN** a paragraph holding a `Link` whose children hold another `Link` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Autolink text that is not one autolink
- **WHEN** a paragraph holding a literal `Autolink` whose text is `HTTP://a.b` or `http://a.b.` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Character reference that does not decode
- **WHEN** a paragraph holding a `CharacterReference` whose reference is `&amp` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Code span holding a line ending
- **WHEN** a paragraph holding `CodeInline::new("a\nb")` is validated
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
- **THEN** it returns an empty list, and the document holds an `Autolink` to `http://a\u{a0}b`

#### Scenario: Inline link whose text is an address
- **WHEN** `Link::new("mailto:a\u{a0}b@c.d", [Text::from("a\u{a0}b@c.d")])` is validated and serialized
- **THEN** validation returns an empty list, and `to_markdown()` writes it as `[a\u{a0}b@c.d](mailto:a\u{a0}b@c.d)`, which reads back as the same `Link`

### Requirement: Conservative scope
Validation SHALL check only the shapes listed by this spec and SHALL NOT be
relied on to prove every semantic invariant of a hand-built AST. A shape whose
spelling depends on the characters around it, or that only the builder can
know is meant literally, is not checked: the builder answers for it, and the
serializer writes it as it is even when the output reads back differently.
Validation does not check, among others:

- `Text` whose value reads as syntax, such as `*a*` or `# a` at a line start;
  punctuation meant literally is built as `Escape` nodes.
- A `LineBreak` inside an ATX heading.
- A `Frontmatter` that is not the document's first block.
- An `Alert` title that holds a line ending.
- A `WikiLink` target that holds an unescaped `|`, or a target or label that
  holds an unescaped `[` or `]` or a line ending.

#### Scenario: Unlisted oddity
- **WHEN** a hand-built document has a shape this spec does not list as invalid
- **THEN** `validate()` may return an empty list

#### Scenario: Frontmatter after a paragraph
- **WHEN** a hand-built document holding a `Paragraph` followed by a `Frontmatter` is validated
- **THEN** it returns an empty list
