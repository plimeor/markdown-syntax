# Block syntax

## Purpose

Recognition of a document's block structure: which lines form which blocks and
how containers nest. Owned by the block parser in `src/parse/blocks.rs`.

## Requirements

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

#### Scenario: Lazy line in an item that started blank
- **WHEN** `"- \n  a\nb"` is parsed with the CommonMark preset
- **THEN** the list's one item holds a paragraph `a`, a soft break, and `b`

#### Scenario: Blank line indented four columns ends a block quote
- **WHEN** `"> a\n    \n> b"` is parsed with the CommonMark preset
- **THEN** the document holds two `BlockQuote`s

#### Scenario: Blank line between an empty item and the next
- **WHEN** `"* \n\n  * b c"` is parsed with the CommonMark preset
- **THEN** the document holds one loose `List` of two items

#### Scenario: Complete HTML tag on a lazy line ends a list item
- **WHEN** `"- a\n<a>"` is parsed with the CommonMark preset
- **THEN** the document holds a `List` followed by an `HtmlBlock`

#### Scenario: ATX-like line that is no heading
- **WHEN** `"a\n#)"` is parsed with the CommonMark preset
- **THEN** the document holds one `Paragraph` holding `Text("a")`, a `SoftBreak`, and `Text("#)")`

#### Scenario: Backtick run with a backtick in its info
- **WHEN** ````"a\n``` `` ```"```` is parsed with the CommonMark preset
- **THEN** the document holds one `Paragraph` whose second line is a code span

#### Scenario: Unclosed fence in a block quote
- **WHEN** ``"> ```\n> x\na"`` is parsed with the CommonMark preset
- **THEN** the document holds a `BlockQuote` whose fenced `CodeBlock` holds `"x\n"`, followed by a `Paragraph` holding `Text("a")`

#### Scenario: Last line ending of indented code
- **WHEN** `"\ta\r\tb"` is parsed with the CommonMark preset
- **THEN** the document holds an indented `CodeBlock` whose value is `"a\rb\r"`

#### Scenario: Container content ending in a carriage return
- **WHEN** ``"- ```\n  ~\r"`` is parsed with the CommonMark preset
- **THEN** the list item's fenced `CodeBlock` holds `"~\n"`

#### Scenario: Complete tag after a definition
- **WHEN** `"[o]: u\n<a>"` is parsed with the CommonMark preset
- **THEN** the document holds a `Definition` followed by a `Paragraph` holding an `Html` inline

#### Scenario: Indented table header row
- **WHEN** `"a\n    |b\n----"` is parsed with `parse`
- **THEN** the document holds one setext `Heading`

#### Scenario: Tab after a top-level block quote marker
- **WHEN** `"> \tcode"` is parsed with the CommonMark preset
- **THEN** the document holds a `BlockQuote` holding a `Paragraph`, since the tab spans columns 2 to 4

#### Scenario: Quoted paragraph opening with backticks
- **WHEN** `"> ``a\nb"` is parsed with the CommonMark preset
- **THEN** the document holds one `BlockQuote` whose paragraph ends with `Text("b")`

#### Scenario: Blank line inside a nested item's open fence
- **WHEN** ``"2. a\n   1. ```\n\n2. b"`` is parsed with the CommonMark preset
- **THEN** the document holds one tight `List`

#### Scenario: Tab inside nested containers
- **WHEN** `"* - \tb c"` is parsed with the CommonMark preset
- **THEN** the inner list item holds an indented `CodeBlock`, since the tab spans columns 4 to 8

#### Scenario: Lazy list marker inside a quoted item
- **WHEN** `"> - a\n> 2.\nz"` is parsed with the CommonMark preset
- **THEN** the document holds a `BlockQuote` holding two lists, followed by a `Paragraph` holding `Text("z")`

#### Scenario: Setext-like line short of a quoted item
- **WHEN** `"> 1. a\n> ===\nb"` is parsed with the CommonMark preset
- **THEN** the item holds one `Paragraph` holding `Text("a")`, `Text("===")`, and `Text("b")` with soft breaks between

#### Scenario: Sibling item ending a nested fence
- **WHEN** ``"- - ```\n  - a\n\n- b"`` is parsed with the CommonMark preset
- **THEN** the outer `List` is loose

#### Scenario: Lazy fence-like line in an item
- **WHEN** ``"1.   a\n    ```\n\nb"`` is parsed with the CommonMark preset
- **THEN** the document holds a `List` followed by a `Paragraph` holding `Text("b")`

#### Scenario: CommonMark oracle cases
- **WHEN** the block cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** the output matches the expected HTML

### Requirement: GFM blocks
When the corresponding constructs are enabled, the parser SHALL recognize GFM
tables, task list items, and alerts.

#### Scenario: Task list item
- **WHEN** `"- [x] done"` is parsed with the GFM preset
- **THEN** the list item is a task item marked checked

#### Scenario: Table
- **WHEN** `"| a | b |\n| - | - |\n| 1 | 2 |"` is parsed with the GFM preset
- **THEN** the document holds a `Table` with a header row and one body row

### Requirement: Extension blocks
When the corresponding constructs are enabled, the parser SHALL recognize
footnote definitions, math blocks, frontmatter delimited by `---` or `+++` at the
start of the document, description lists, and Markdown-compatible HTML
containers (`details` / `summary`).

#### Scenario: HTML container in the default dialect
- **WHEN** `"<details>\n<summary>Open</summary>\n\nbody\n\n</details>\n"` is parsed with `parse`
- **THEN** the document holds exactly one `HtmlContainer`

#### Scenario: Frontmatter
- **WHEN** `"---\ntitle: x\n---\n\nbody"` is parsed with `parse`
- **THEN** the first block is `Frontmatter` and the second is a `Paragraph`

### Requirement: Block directives
When directives are enabled, the parser SHALL recognize `::name[label]{attrs}`
leaf directives and `:::name` container directives, whose container closes at a
fence of at least the opening fence's length, and a line with a malformed
opener SHALL NOT end a paragraph.

#### Scenario: Container directive
- **WHEN** `":::note\nbody\n:::"` is parsed with `parse`
- **THEN** the document holds a `ContainerDirective` named `note` whose children hold a paragraph `body`

#### Scenario: Unclosed container
- **WHEN** `":::note\nunclosed container"` is parsed
- **THEN** a `ContainerDirective` holds the remaining content and an error-severity `UnclosedDirectiveContainer` diagnostic is reported

#### Scenario: Invalid name
- **WHEN** a leaf directive opener has a malformed name
- **THEN** an error-severity `InvalidDirectiveName` diagnostic is reported

#### Scenario: Directive attribute without a valid name
- **WHEN** `":b{<} :c{a <=1 d}"` is parsed with `parse` and serialized
- **THEN** `to_markdown()` returns `":b :c{a d}\n"`

#### Scenario: Malformed directive line inside a paragraph
- **WHEN** `"a\n::1bad"` or `"a\n:::"` is parsed with `parse`
- **THEN** the document holds one `Paragraph` holding both lines

### Requirement: Directives are not MDX
The parser SHALL treat `:name`, `::name`, and `:::name` as the directive family
under every option set and SHALL never parse them as MDX.

#### Scenario: Directive in MDX mode
- **WHEN** `":::note\nbody\n:::"` is parsed with the MDX preset and directives enabled
- **THEN** the result is a `ContainerDirective`, not an MDX node

### Requirement: MDX flow constructs
In MDX mode the parser SHALL recognize ESM blocks, flow expressions (`{…}`
standing alone on their lines), and flow JSX elements, keeping each element's
source text as the node value.

#### Scenario: Flow JSX
- **WHEN** `"<Note kind=\"x\">Para {props.v} *em*</Note>"` is parsed with the MDX preset
- **THEN** the document holds one `MdxJsx` whose value is that line verbatim

### Requirement: Spoilers in table rows
With spoilers enabled, a table row SHALL keep inside one cell every pipe that a
spoiler in that cell holds, pairing `||` runs outside code spans first-closer
style: a run opens with its last two bars and pairs with the next run, which
closes with its first two; the pipes between the two stay in the cell, and the
bars of a run that opens no spoiler delimit. Runs are read as the cell's text
has them, with each escaped pipe unescaped; an escaped pipe never delimits, and
a pair that uses one SHALL NOT hold a pipe that would otherwise delimit. A code
span counts only when it closes before a pipe that would split the row inside
it. The pairing does not consider links, emphasis, inline math, raw HTML, or
autolinks, so a spoiler one of them keeps from forming leaves its bars and the
pipes it held as text in the cell.

#### Scenario: Spoiler holding a pipe
- **WHEN** `"| x | y |\n|---|---|\n| ||a | b|| | c |"` is parsed with `parse`
- **THEN** the body row has two cells, the first holding a `Spoiler` containing `a | b`

#### Scenario: Bars inside a code span
- **WHEN** `"| w | x | y | z |\n|-|-|-|-|\n| ||a `||` | b |"` is parsed with `parse`
- **THEN** the body row's cells are empty, empty, `a ` followed by code `||`, and `b`

#### Scenario: Escaped pipes stay in their own cells
- **WHEN** `"| x | y |\n|---|---|\n| a \\|\\| b | c \\|\\| d |"` is parsed with `parse`
- **THEN** the body row's cells are `a || b` and `c || d`

#### Scenario: Escaped backtick
- **WHEN** `"| x | y |\n|---|---|\n| \\`||a\\` | b|| |"` is parsed with `parse`
- **THEN** the body row's first cell holds `` ` `` followed by a `Spoiler` containing `` a` | b ``

#### Scenario: Empty cell between bars
- **WHEN** `"| x | y | z |\n|---|---|---|\n|a||b|"` is parsed with `parse`
- **THEN** the body row's cells are `a`, empty, and `b`

### Requirement: Spaces and tabs are block whitespace
The parser SHALL read only spaces and tabs as whitespace in block structure: a
blank line holds only spaces and tabs; indentation, the space after a block
marker, the trailing whitespace a thematic break, setext underline, closing
fence, ATX closing sequence, HTML block start line, definition, or table row
allows, the indentation before flow MDX JSX, and the final whitespace of a
paragraph are spaces and tabs. Any
other whitespace char, such as a no-break space or a form feed, is content.

#### Scenario: Other whitespace after a thematic break
- **WHEN** `"***\u{a0}"` is parsed with the CommonMark preset
- **THEN** the document holds a `Paragraph`, not a `ThematicBreak`

#### Scenario: Line holding only a no-break space
- **WHEN** `"a\n\u{a0}\nb"` is parsed with the CommonMark preset
- **THEN** the document holds one `Paragraph`

#### Scenario: No-break space before MDX JSX
- **WHEN** `"\u{a0} <p/>"` is parsed with the MDX preset
- **THEN** the document holds a `Paragraph`, not a flow `MdxJsx` block

#### Scenario: Form feed ending a paragraph
- **WHEN** `"a\u{c}"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("a\u{c}")`

### Requirement: Fenced code inside a container directive
A fenced code block inside a container directive SHALL hold its lines as code:
a line in it that looks like a directive opener opens no nested directive,
while a closing fence of the directive still closes it, and a fence that a
nested directive leaves open ends with that directive.

#### Scenario: Directive opener inside fenced code
- **WHEN** `":::t\n```\n:::e\n```\n:::"` is parsed
- **THEN** the document holds one `ContainerDirective` named `t` holding a `CodeBlock` whose value is `":::e\n"`

#### Scenario: Fence left open in a nested directive
- **WHEN** ``":::outer\n:::inner\n```\n:::\n:::inner2\nx\n:::\n:::\nafter"`` is parsed
- **THEN** the `ContainerDirective` named `outer` holds the directives `inner` and `inner2`, a `Paragraph` holding `Text("after")` follows it, and no diagnostic is reported

### Requirement: Footnote definition content
A footnote definition's content SHALL start after the spaces and tabs that
follow its `]:` and keep the trailing spaces of its first line, which may
make a hard break.

#### Scenario: Hard break on a footnote definition's first line
- **WHEN** `"[^1]: a  \nb"` is parsed
- **THEN** the definition's paragraph holds `Text("a")`, a `LineBreak`, and `Text("b")`

### Requirement: List after a definition
A list SHALL start on the line right after a definition only when its first
item could interrupt a paragraph: a bullet or an ordered item starting at 1,
with content. Otherwise the line continues the paragraph the definition was
read from.

#### Scenario: Ordered item not starting at 1 after a definition
- **WHEN** `"[foo]: /url\n2) a"` is parsed with the CommonMark preset
- **THEN** the document holds the `Definition` and a `Paragraph`

### Requirement: GFM table start
A GFM table SHALL start only where the line after its header row is a
delimiter row that is neither a lazy continuation line nor a setext
underline; such a line keeps its other reading.

#### Scenario: Delimiter row without pipes
- **WHEN** `"| --- |\n-- "` is parsed with the GFM preset
- **THEN** the document holds a level-2 setext `Heading`, not a `Table`

#### Scenario: Setext underline below a header row
- **WHEN** `"a\n|b\n---"` is parsed with the GFM preset
- **THEN** the document holds one level-2 setext `Heading` holding both lines

#### Scenario: Lazy delimiter row
- **WHEN** `"1. ---(\n:-:"` is parsed with the GFM preset
- **THEN** the list item holds a `Paragraph`, not a `Table`

#### Scenario: Table header row that looks like an empty list item
- **WHEN** `"a\n+\n|-"` is parsed with the GFM preset
- **THEN** the document holds a `Paragraph` and a `Table` whose header cell holds `+`
