# Block syntax

## Purpose

Recognition of a document's block structure: which lines form which blocks and
how containers nest. Owned by the block parser in `src/parse.rs`.

## Requirements

### Requirement: CommonMark blocks
The parser SHALL recognize CommonMark block constructs (ATX and setext headings,
thematic breaks, indented and fenced code blocks, block quotes, lists, HTML
blocks, link reference definitions, and paragraphs) as the CommonMark
specification defines them.

#### Scenario: Heading and paragraph
- **WHEN** `"# Title\n\nHello."` is parsed
- **THEN** the document holds a `Heading` of depth 1 followed by a `Paragraph`

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
fence of at least the opening fence's length.

#### Scenario: Container directive
- **WHEN** `":::note\nbody\n:::"` is parsed with `parse`
- **THEN** the document holds a `ContainerDirective` named `note` whose children hold a paragraph `body`

#### Scenario: Unclosed container
- **WHEN** `":::note\nunclosed container"` is parsed
- **THEN** a `ContainerDirective` holds the remaining content and an error-severity `UnclosedDirectiveContainer` diagnostic is reported

#### Scenario: Invalid name
- **WHEN** a leaf directive opener has a malformed name
- **THEN** an error-severity `InvalidDirectiveName` diagnostic is reported

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
