# Validation
Type: api

## Purpose

Checking that a `Document`, usually a hand-built one, has a shape the serializer
and renderer can write faithfully. Owned by the validator.

## Requirements

### Requirement: Validate reports invalid shapes
`Document::validate` SHALL return one error-severity `InvalidDocument`
diagnostic per invalid shape it finds, and an empty list for a valid document.
Each diagnostic's `path` SHALL be the `NodePath`, in the document validated,
of the node the shape belongs to: the block, list item, table row, or table
cell; for a shape among sibling blocks, the later block; for a shape in
inline content, the leaf holding it, a directive label's being its
directive's.

#### Scenario: A block a tight item cannot hold
- **WHEN** a hand-built document holds a tight list whose first item holds two paragraphs, and `validate()` runs
- **THEN** it returns one diagnostic whose `path` is `[0, 0, 1]`, the second paragraph

### Requirement: Shapes that cannot be written
Validation SHALL reject exactly these shapes, each visible from the node, its
children, its sibling blocks and the last blocks they end in, how many
containers enclose it, or whether it sits in a table cell or a directive
label:

- Block sequences: two adjacent `List`s in the same container written with
  the same marker char; a list recording a delimiter of the other list kind is
  written with `-` when unordered and `.` when ordered. An indented
  `CodeBlock` after an indented `CodeBlock` or a `FootnoteDefinition` in the
  same container, blank lines between them or not; any block after an
  `HtmlBlock` whose first line opens one that an end marker closes (a
  comment, a raw tag such as `<script>`, a processing instruction, a
  declaration, or CDATA) and whose value holds no line that closes it.
- In a tight list item, which writes each block on the line after the one
  before, a block that the block before it takes as that line:
  - after a `Paragraph`, a block that cannot interrupt it, by the parser's
    own checks: a `Paragraph`, a `Definition`, an indented `CodeBlock`, a
    `Frontmatter`, an ordered `List` whose first item's number is not 1, a
    `List` whose first item has no blocks, or an `HtmlBlock` whose first
    line opens no HTML block or opens one that only a blank line ends (a
    lone tag such as `<span>`);
  - after a `Definition`, those blocks other than a `Paragraph` and a
    `Definition`;
  - after a `Table`, a block whose first line starts no block where no
    paragraph is open: a `Paragraph`, a `Definition`, a `Table`, or an
    `HtmlBlock` whose first line opens no HTML block;
  - after an `HtmlBlock` whose first line opens one that a blank line ends
    (a block-level tag such as `<div>`, or a lone tag), any block;
  - after a `BlockQuote` or an `Alert`, a `BlockQuote` or an `Alert`;
  - after a `BlockQuote`, an `Alert`, a `FootnoteDefinition`, or a `List`
    whose last line, through the last block of each and the last item of
    each list, is a `Paragraph`'s or a `Definition`'s, or is an `Alert`
    holding no blocks: the blocks after a `Table` and an indented
    `CodeBlock`, which that paragraph takes as a lazy line, except, where a
    `BlockQuote` or an `Alert` is passed on the way, a block written opening
    with a backtick or a tilde, which ends the quote's paragraph.
- `Paragraph`: no children other than empty `Text`.
- `Heading`: a depth outside 1–6.
- `Table`: no rows; a header row with no cells; an alignment count other than
  the header row's width; a row whose width differs from the header row's.
- `List`: no items; a loose list of one item holding at most one block.
- `ListItem`: a number beyond the parser's 9-digit marker limit; a number in
  a bullet list; a first line that, written on the marker's line after one
  space, would read otherwise, since an item's content cannot start below
  its marker (decision 0019): content opening with one to three spaces or a
  tab, or, with a `-` or `*` bullet and no checkbox, a first line of only
  that char, spaces, and tabs holding two or more of the char, which reads
  as one thematic break with the bullet (a thematic break of the bullet's
  char, a paragraph `--`, or a nested empty item two levels down).
- `BlankLine`: first or last in a footnote definition; first in a list item,
  or last in a list's last item; in a tight list.
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
  items, footnote definitions, HTML containers, and container directives).
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
- `Link`, `Image`, and `Definition`: an empty bare destination.
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
  `LineBreak` of trailing spaces, right after a `SoftBreak` or a `LineBreak`;
  two adjacent `Delete`s.
- `Emphasis`, `Strong`, `Delete`, and `Mark`: no children other than empty
  `Text`; content that, past empty `Text`, starts with a Unicode whitespace
  char, a `SoftBreak`, or a `LineBreak` of trailing spaces, or ends with a
  Unicode whitespace char, a `SoftBreak`, or a `LineBreak`; an
  `Emphasis` or a `Strong` whose only child is an `Emphasis` with the same
  delimiter, a `Delete` whose only child is a `Delete`, and a `Mark` whose
  only child is a `Mark`.
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
- `Tag`: a name that is not one Unicode letter followed by Unicode letters,
  Unicode numbers, `/`, `_`, or `-`.
- `CodeInline`: an empty value, or a value holding a line ending.
- `MathInline`: an empty value; a dollar fence of length 0; code-form math
  whose value holds a backtick followed by `$`; dollar math that, written
  alone behind its fence, the parser does not read back as the same value and
  fence: a fence of three or more, a `$` that closes it early, whitespace at
  an edge of single-`$` math, or a line ending.
- In a table cell, or in the label of a `LeafDirective` or a
  `ContainerDirective`, at any depth: a `SoftBreak` or a `LineBreak`.
- In a table cell, at any depth: a `CodeInline`, `MathInline`, or `Html`
  value, an `Autolink` text, a `LinkReference`, `ImageReference`, or
  `FootnoteReference` label, or a `WikiLink` target or label that holds a `|`
  right after an odd run of backslashes.

Any other shape is not checked, including one whose spelling depends on the
characters around it or that only the builder can know is meant literally
(punctuation meant literally is built as `Escape` nodes), and a block after a
`List` whose last line is not a paragraph's or a definition's, where the
block's first line, written four columns in or opening with spaces, reaches
the content column of the list's last item, which in a parsed document the
source's markers set: the builder answers
for it, and the serializer writes it as it is even when the output reads back
differently.

#### Scenario: A paragraph after a nested list in a tight item
- **WHEN** a hand-built document holds a tight list whose item holds a paragraph `a`, a tight list of one item holding a paragraph `b`, and a paragraph `c`, and `validate()` runs
- **THEN** it returns one diagnostic whose `path` is `[0, 0, 2]`, the paragraph `c`, which `"- a\n  - b\n  c\n"` would read as `b`'s lazy line
