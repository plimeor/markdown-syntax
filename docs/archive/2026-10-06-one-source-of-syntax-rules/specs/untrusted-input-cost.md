# Untrusted input cost — spec changes

## MODIFIED Requirements

### Requirement: Linear time
Parsing, `to_markdown`, `to_html`, and `validate` SHALL take time linear in the
input size, except MDX JSX tag matching, which SHALL take at most `n log n`.

#### Scenario: Unclosed openers
- **WHEN** an input of tens of thousands of unclosed openers of one construct (for example `[`, `++`, `<Tag`, or `{`) is parsed in any dialect
- **THEN** doubling the input at most roughly doubles the time

#### Scenario: Diagnostics running to the end of a paragraph
- **WHEN** thousands of lines each holding a malformed text directive opener, such as `" x :a{"`, are parsed, at the top level or in a block quote
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Long run of tildes
- **WHEN** a paragraph holding tens of thousands of `~` between two words is parsed and serialized
- **THEN** quadrupling the run at most roughly quadruples the serialization time

#### Scenario: Deeply nested emphasis
- **WHEN** a paragraph holding 16 levels of nested emphasis, a mark around further nested emphasis, and a tail that abuts the runs is serialized
- **THEN** it finishes within 1 s in a debug build, and doubling the nesting depth from 4 to 8 to 16 at most roughly doubles the time

#### Scenario: Long description details
- **WHEN** a description list whose details hold thousands of continuation lines, `"a\n: b\n"` followed by `"c\n"` repeated, is parsed
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Long nested containers
- **WHEN** inputs of thousands of lines inside nested block quotes and list items are parsed and serialized
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Pathological suite
- **WHEN** `tests/pathological_inputs.rs` runs
- **THEN** every case finishes within its time limit
