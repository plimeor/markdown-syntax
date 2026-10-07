# Validation — spec changes

## MODIFIED Requirements

### Requirement: Shapes that cannot be written
Validation SHALL reject exactly these shapes, each visible from the node, its
children, its sibling blocks, how many containers enclose it, or whether it
sits in a table cell or a directive label:

- Block sequences: two adjacent `List`s in the same container written with
  the same marker char; a list recording a delimiter of the other list kind is
  written with `-` when unordered and `.` when ordered.
- In a tight list item, a block right after a `Paragraph` that cannot
  interrupt it, by the parser's own checks: a `Paragraph`, a `Definition`, an
  indented `CodeBlock`, a `Frontmatter`, a `Heading` written as a setext
  heading, an ordered `List` whose start is not 1, a `List` whose first item
  has no blocks or that the serializer starts on a line after its bullet (a
  thematic break of the bullet's char, or content opening with one to three
  spaces or a tab), or an
  `HtmlBlock` whose first line opens no HTML block or opens one that only a
  blank line ends (a lone tag such as `<span>`).
- `Paragraph`: no children other than empty `Text`, which writes nothing.
- `Heading`: a depth outside 1–6.
- `Table`: no rows; a header row with no cells; an alignment count other than
  the header row's width; a row whose width differs from the header row's.
- `List`: no items; a loose list of one item holding at most one block,
  which no source spells, since blank lines between items or between two
  blocks of an item make a list loose; an ordered list whose start is beyond
  the parser's 9-digit marker limit.
- `ListItem`: a task item (`checked` set) whose first block after its leading
  `Definition`s is not a `Paragraph` with children.
- `Alert`: a title that is empty or starts or ends with a space or a tab.
- `CodeBlock`: an info string that is `Some("")`; an indented block whose
  value is empty or whose first or last line is blank (only spaces and
  tabs), where one line ending at the value's end closes its last line.
- `Frontmatter`: inside any container; a value holding a line that is its own
  fence (`---` for YAML, `+++` for TOML, spaces and tabs after it aside).
- `Definition`: an identifier that is empty or holds only spaces, tabs, and
  line endings; a label that is `^` followed by a footnote label, unless the
  definition directly follows another `Definition`, is the first block of a
  container, or sits 32 or more containers deep (block quotes, alerts, list
  items, footnote definitions, HTML containers, and container directives),
  where the parser reads such a line as a definition.
- `Definition`, and a `Full` `LinkReference` or `ImageReference`: a label
  that `[label]` does not read back as: one holding an unescaped `[` or `]`,
  ending in a backslash that escapes its `]`, holding a blank line between
  its first and last line, longer than 999 chars, or blank.
- `Definition`, `LinkReference`, and `ImageReference`: an identifier other
  than the parser's normalization of the label.
- `FootnoteDefinition`, `FootnoteReference`, `LinkReference`, and
  `ImageReference`: an empty identifier.
- `FootnoteDefinition` and `FootnoteReference`: an identifier other than the
  parser's normalization of the label; a label that is not a footnote label
  (empty, holding a space, a tab, a line ending, or an unescaped `[` or `]`,
  or longer than 999 chars) or that ends in a backslash escaping its `]`.
- `Link`, `Image`, and `Definition`: an empty bare destination, which is
  written `<>` and reads back as an angle-bracket one.
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
- Inline content: a `Text` holding a line ending; a `SoftBreak`, or a
  `LineBreak` of trailing spaces, right after a `SoftBreak` or a `LineBreak`,
  which leaves an empty line; two adjacent `Delete`s, written `~~a~~~~b~~`.
- `Emphasis`, `Strong`, `Delete`, and `Mark`: no children other than empty
  `Text`; content that, past empty `Text`, starts with a Unicode whitespace
  char, a `SoftBreak`, or a `LineBreak` of trailing spaces, or ends with a
  Unicode whitespace char, a `SoftBreak`, or a `LineBreak`; an
  `Emphasis` or a `Strong` whose only child is an `Emphasis` with the same
  delimiter (written `**a**` or `***a***`), a `Delete` whose only child is a
  `Delete`, and a `Mark` whose only child is a `Mark`.
- `InlineFootnote`: no children.
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
  whose value holds a backtick followed by `$`, which would close it early;
  dollar math that, written alone behind its fence, the parser does not read
  back as the same value and fence: a fence of three or more, a `$` that
  closes it early, whitespace at an edge of single-`$` math, or a line
  ending, which single-`$` math reads as a space.
- In a table cell, or in the label of a `LeafDirective` or a
  `ContainerDirective`, at any depth: a `SoftBreak` or a `LineBreak`.
- In a table cell, at any depth: a `CodeInline`, `MathInline`, or `Html`
  value, an `Autolink` text, a `LinkReference`, `ImageReference`, or
  `FootnoteReference` label, or a `WikiLink` target or label that holds a `|`
  right after an odd run of backslashes. A row splits at a `|` after no
  backslash or an even run of them, and the cell reads an odd run before a
  `|` with one backslash less, so no cell source gives such a value.

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

#### Scenario: Code span holding an escaped pipe in a table cell
- **WHEN** a table cell holding a `CodeInline` whose value is `a\|b` is validated and serialized
- **THEN** the result holds an `InvalidDocument` error, and `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`; the same code span in a paragraph is valid and is written `` `a\|b` ``

#### Scenario: Adjacent lists with one marker
- **WHEN** a document holding two adjacent unordered `List`s that both use `-` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Adjacent lists written with one marker
- **WHEN** a document holding an unordered `List` recording `Period` next to an unordered `List` recording `Dash` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Span holding only a span with its delimiter
- **WHEN** a paragraph holding an `Emphasis` whose only child is an `Emphasis` with the same delimiter, or a `Delete` holding only a `Delete`, is validated
- **THEN** the result holds an `InvalidDocument` error; `parse("****a****")`, a `Strong` holding only a `Strong`, validates

#### Scenario: Adjacent strikethroughs
- **WHEN** a paragraph holding two adjacent `Delete`s is validated
- **THEN** the result holds an `InvalidDocument` error; two adjacent `Mark`s, and `parse("**:* ~**a**")`, two adjacent `Emphasis` spans, validate

#### Scenario: Reference label outside the label grammar
- **WHEN** a `Full` `LinkReference` or a `Definition` whose label is `a]b`, or whose identifier is `Foo` for the label `Foo`, is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Definition label that opens a footnote definition
- **WHEN** a document holding a `Definition` labeled `^a` as its first block is validated
- **THEN** the result holds an `InvalidDocument` error; `parse("[^]: x")` and `parse("[a]: /u\n    [^a]: x")` validate

#### Scenario: Footnote label with a space
- **WHEN** a paragraph holding a `FootnoteReference` labeled `a b` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Task item without a paragraph
- **WHEN** a list holding a task item with no blocks, or one whose first block is a `CodeBlock`, is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Line endings and breaks
- **WHEN** a paragraph holding `Text("a\nb")`, or two adjacent `SoftBreak`s, is validated
- **THEN** the result holds an `InvalidDocument` error; `parse("a\n\\\n\\\nb")`, a backslash break after a break, validates

#### Scenario: Break in a one-line label
- **WHEN** a table cell, or a leaf or container directive label, holding a `SoftBreak` is validated
- **THEN** the result holds an `InvalidDocument` error; a text directive label holding one `SoftBreak` validates

#### Scenario: Block after a paragraph in a tight item
- **WHEN** a tight list item holding a `Paragraph` followed by a `Paragraph`, a `Definition`, an indented `CodeBlock`, a setext `Heading`, an ordered `List` starting at 2, or the `HtmlBlock` `<span>x</span>` is validated
- **THEN** the result holds an `InvalidDocument` error; the same item in a loose list, and `parse("- a\n  | h |\n  | - |")`, validate

#### Scenario: Empty list
- **WHEN** a document holding a `List` with no items is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Frontmatter out of place
- **WHEN** a block quote holding a `Frontmatter`, or a YAML `Frontmatter` whose value is `a: 1\n---\nb: 2`, is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Empty inline footnote
- **WHEN** a paragraph holding an `InlineFootnote` with no children is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Alert title with spaces around it
- **WHEN** an `Alert` whose title is `Some("")` or `Some(" t")` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Empty info string
- **WHEN** a fenced `CodeBlock` whose info is `Some("")` is validated
- **THEN** the result holds an `InvalidDocument` error

#### Scenario: Empty bare destination
- **WHEN** a `Link` with an empty `Bare` destination is validated
- **THEN** the result holds an `InvalidDocument` error; an empty `Angle` or `Omitted` destination validates

#### Scenario: Indented code with a blank edge line
- **WHEN** an indented `CodeBlock` whose value is `""`, `"\na\n"`, or `"a\n\n"` is validated
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

### Requirement: Validate reports invalid shapes
`Document::validate` SHALL return one error-severity `InvalidDocument`
diagnostic per invalid shape it finds, and an empty list for a valid document.

#### Scenario: Parsed document
- **WHEN** `parse("# Title\n\nHello *world*.").document.validate()` runs
- **THEN** it returns an empty list

#### Scenario: Empty table
- **WHEN** a document holding a `Table` with no rows is validated
- **THEN** the result holds an `InvalidDocument` error

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
- A `Frontmatter` at the top level that is not the document's first block.
- An `Alert` title that holds a line ending.
- Two adjacent `Emphasis` or `Strong` spans with one delimiter char: whether
  their joined run closes depends on the characters around it, and
  `**:* ~**a**` parses to two such `Emphasis` spans.
- A collapsed or shortcut reference whose text, as written, does not
  normalize to its identifier: a code span's fence and padding are not
  recorded, so ```[`` a ``]``` is written `` [`a`] ``.
- A `List` after a `Paragraph` in a tight item whose first item a `bullet`
  override makes start on the line after its marker: `- a\n  * ---` written
  with `bullet = Some(BulletMarker::Dash)` is `- a\n  -\n    ---`, which reads
  back as a setext heading.
- A `WikiLink` target that holds an unescaped `|`, or a target or label that
  holds an unescaped `[` or `]` or a line ending.

#### Scenario: Unlisted oddity
- **WHEN** a hand-built document has a shape this spec does not list as invalid
- **THEN** `validate()` may return an empty list

#### Scenario: Frontmatter after a paragraph
- **WHEN** a hand-built document holding a `Paragraph` followed by a `Frontmatter` is validated
- **THEN** it returns an empty list
