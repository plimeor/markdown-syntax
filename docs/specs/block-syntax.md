# Block syntax
Type: api

## Purpose

Recognition of a document's block structure: which lines form which blocks and
how containers nest. Owned by the block parser.

## Requirements

### Requirement: CommonMark blocks
The parser SHALL recognize CommonMark block constructs as the CommonMark
specification 0.31.2 defines them. Where commonmark.js renders a case
otherwise, and for nested containers, the output SHALL match commonmark.js:
blank lines inside a fence in a list item hold no whitespace past the item's
indentation, and a fence a container ends right after its opening line is
empty.

### Requirement: GFM blocks
The parser SHALL recognize GFM tables and task list items as the GFM
specification 0.29-gfm defines them, following cmark-gfm where that
specification leaves a case open, and GitHub alerts, whose content SHALL take
lazy continuation lines as a block quote's does.

### Requirement: Extension blocks
The parser SHALL recognize footnote definitions, math blocks, frontmatter
delimited by `---` or `+++` at the start of the document, and
Markdown-compatible HTML containers (`details` / `summary`). A blank line
inside a math block SHALL NOT make the list around it loose, and an unclosed
math block that the input or its container ends SHALL keep its trailing blank
lines in its value.

#### Scenario: HTML container
- **WHEN** `"<details>\n<summary>Open</summary>\n\nbody\n\n</details>\n"` is parsed with `parse`
- **THEN** the document holds exactly one `HtmlContainer`

### Requirement: Block directives
The parser SHALL recognize `::name[label]{attrs}` leaf directives standing
alone on their line, followed by nothing but spaces and tabs, and `:::name`
container directives, whose container closes at a fence of at least the
opening fence's length. A directive name SHALL be one or more runs of ASCII
letters joined by single `-` chars. A closing fence SHALL close the innermost
open container directive it can close, and SHALL do so before any block
inside that directive reads the line. A container directive's content lines
are read as the document's lines are: a line that looks like a directive
opener inside a fenced code block or HTML block is that block's content, and
one inside a list item opens a directive in that item. A line with a
malformed opener, or with text after a leaf directive, SHALL NOT end a
paragraph and SHALL be read as paragraph text; a malformed opener that starts
a block SHALL be reported as an error-severity `InvalidDirectiveName`
diagnostic. A container directive left unclosed SHALL hold the remaining
content and be reported as an error-severity `UnclosedDirectiveContainer`
diagnostic. An attribute without a valid name SHALL be dropped and reported
as a warning-severity diagnostic.

#### Scenario: Directive opener inside fenced code
- **WHEN** ``":::t\n```\n:::e\n```\n:::"`` is parsed
- **THEN** the document holds one `ContainerDirective` named `t` holding a `CodeBlock` whose value is `":::e\n"`

### Requirement: Footnote definition content
A footnote definition's content SHALL start after the spaces and tabs that
follow its `]:` and keep the trailing spaces of its first line, which may
make a hard break; a paragraph open in it SHALL take lazy continuation lines,
also when the definition sits in a block quote.

### Requirement: One pass over open blocks
Block quotes, list items, container directives, footnote definitions, and
HTML containers SHALL all be containers: a line that does not continue a
container closes it and every block inside it, and a line continues a
paragraph lazily only when the innermost open block is a paragraph.

### Requirement: Block extensions indent at most three columns
A footnote definition, a leaf directive, a container directive opener, and a
math block fence SHALL start only when indented at most three columns; a line
indented four or more columns is indented code, or a paragraph continuation
line inside a paragraph.

### Requirement: Blank lines are nodes
The parser SHALL represent blank lines beyond the one that separates two
blocks as `Block::BlankLine` nodes, one per line: K blank lines between two
blocks of one container, or between two items of a list, SHALL yield K − 1
nodes (between items, at the end of the earlier item's children); blank lines
before a document's first block, at the start or end of a block quote, alert,
HTML container, or container directive, and after a document's last line
ending SHALL yield one node each, and after frontmatter K − 1. Blank lines
after the last block of a list item or footnote definition SHALL belong to
the container holding the list or definition. Blank lines between an empty
list item and the next item SHALL yield no node, since no blank line can be
written in an empty item. A blank line inside fenced code,
an HTML block, or a math block SHALL stay part of its value.

#### Scenario: Extra blank lines between paragraphs
- **WHEN** `parse("a\n\n\n\nb")` runs
- **THEN** the document holds a `Paragraph`, two `BlankLine`s, and a `Paragraph`

#### Scenario: Two blank lines between items
- **WHEN** `parse("- a\n\n\n- b")` runs
- **THEN** the document holds one loose list whose first item holds a `Paragraph` and a `BlankLine`
