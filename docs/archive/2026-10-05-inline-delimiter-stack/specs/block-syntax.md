# Block syntax — spec changes

## ADDED Requirements

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
