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

#### Scenario: Nested emphasis that does not read back
- **WHEN** a paragraph holding 16 levels of nested emphasis, a mark around further nested emphasis, and a tail whose plain rendering does not read back is serialized
- **THEN** it finishes in milliseconds, since nesting does not multiply the renders a delimiter choice makes

#### Scenario: Pathological suite
- **WHEN** `tests/pathological_inputs.rs` runs
- **THEN** every case finishes within its time limit
