# Block syntax — spec changes

## ADDED Requirements

### Requirement: Blank lines are nodes
The parser SHALL represent blank lines beyond the one that separates two
blocks as `Block::BlankLine` nodes, one per line: K blank lines between two
blocks of one container, or between two items of a list, SHALL yield K − 1
nodes (between items, at the end of the earlier item's children); blank lines
before a document's first block, at the start or end of a block quote, alert,
or container directive, and after a document's last line ending SHALL yield
one node each, and after frontmatter K − 1. A blank line inside fenced code,
an HTML block, or a math block SHALL stay part of its value.

#### Scenario: Extra blank lines between paragraphs
- **WHEN** `parse("a\n\n\n\nb")` runs
- **THEN** the document holds a `Paragraph`, two `BlankLine`s, and a `Paragraph`

#### Scenario: Two blank lines between items
- **WHEN** `parse("- a\n\n\n- b")` runs
- **THEN** the document holds one loose list whose first item holds a `Paragraph` and a `BlankLine`
