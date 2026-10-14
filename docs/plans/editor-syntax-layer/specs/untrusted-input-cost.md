# Untrusted input cost — spec changes

## MODIFIED Requirements

### Requirement: Linear time
Parsing with `parse` and `parse_inline`, `to_markdown`, `to_html`, `validate`,
and `Document::inline_source` SHALL take time linear in the input size: the
input, the inline text, or the document and its kept source together.

#### Scenario: Unclosed openers
- **WHEN** an input of tens of thousands of unclosed openers of one construct (for example `[`, `==`, `^[`, or `<!X`) is parsed
- **THEN** doubling the input at most roughly doubles the time

#### Scenario: Repeated fragment combinations
- **WHEN** any pair or triple of syntax fragments — link, image, footnote, wiki link, and directive openers and closers, literal autolinks, code, math, emphasis, strikethrough, and highlight delimiters, a backslash, a character reference, a pipe, a space, a line ending, and block quote and list markers — is repeated `n` and `2n` times, alone or after a paragraph's first word, and parsed and serialized
- **THEN** doubling the repetitions at most roughly doubles the time of each

#### Scenario: Email candidates before one `@`
- **WHEN** a long run of local-part bytes that each could start an email, such as `"-a"` or `"a+"` repeated, is followed by one `@` and a domain, and parsed
- **THEN** quadrupling the run at most roughly quadruples the time

#### Scenario: Literal autolinks in open labels
- **WHEN** thousands of link labels left open, each holding a literal autolink and followed by `(`, such as `"[ www.a]("` repeated, are parsed
- **THEN** quadrupling them at most roughly quadruples the time

#### Scenario: Diagnostics running to the end of a paragraph
- **WHEN** thousands of lines each holding a malformed text directive opener, such as `" x :a{"`, are parsed, at the top level or in a block quote
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Long run of tildes
- **WHEN** a paragraph holding tens of thousands of `~` between two words is parsed and serialized
- **THEN** quadrupling the run at most roughly quadruples the parse and the serialization time

#### Scenario: Deeply nested emphasis
- **WHEN** a paragraph holding 16 levels of nested emphasis and a mark around further nested emphasis is serialized
- **THEN** it finishes within 1 s in a debug build, and doubling the nesting depth from 4 to 8 to 16 at most roughly doubles the time

#### Scenario: Long footnote definition
- **WHEN** a footnote definition whose paragraph takes thousands of lazy lines, `"[^1]: b\n"` followed by `"c\n"` repeated, is parsed
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Long nested containers
- **WHEN** inputs of thousands of lines inside nested block quotes and list items are parsed and serialized
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Pathological suite
- **WHEN** `tests/pathological_inputs.rs` runs
- **THEN** every case finishes within its time limit

#### Scenario: Inline parse and source reuse
- **WHEN** the repeated fragment combinations are parsed with `parse_inline`, and documents parsed from them are edited in one top-level block and written with their source
- **THEN** doubling the repetitions at most roughly doubles the time of each
