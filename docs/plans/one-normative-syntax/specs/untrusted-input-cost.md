# Untrusted input cost — spec changes

## MODIFIED Requirements

### Requirement: Linear time
Parsing, `to_markdown`, `to_html`, and `validate` SHALL take time linear in the
input size.

#### Scenario: Unclosed openers
- **WHEN** an input of tens of thousands of unclosed openers of one construct (for example `[`, `==`, `^[`, or `<!X`) is parsed
- **THEN** doubling the input at most roughly doubles the time

#### Scenario: Diagnostics running to the end of a paragraph
- **WHEN** thousands of lines each holding a malformed text directive opener, such as `" x :a{"`, are parsed, at the top level or in a block quote
- **THEN** quadrupling the lines at most roughly quadruples the time

#### Scenario: Long run of tildes
- **WHEN** a paragraph holding tens of thousands of `~` between two words is parsed and serialized
- **THEN** quadrupling the run at most roughly quadruples the serialization time

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

### Requirement: Block nesting limit
Block containers (block quotes, list items, container directives, footnote
definitions, HTML containers) SHALL nest at most 32 levels; markers past the
limit SHALL stay leaf-block text, usually paragraph text.

#### Scenario: 40 nested block quotes
- **WHEN** a line starting with 40 `>` markers is parsed
- **THEN** 32 levels of `BlockQuote` are produced and the remaining markers are paragraph text

### Requirement: Inline nesting limit
Inline containers — link and image labels, inline footnotes, directive labels,
emphasis and strong, strikethrough, and `==` spans — SHALL nest at most 32
levels together; an opener past the limit SHALL stay literal text.

#### Scenario: Deeply nested highlights
- **WHEN** 40 nested `==` spans are parsed
- **THEN** at most 32 levels of `Mark` are produced and the delimiters past the limit stay literal text

#### Scenario: Emphasis inside highlights
- **WHEN** 32 nested `==` spans each holding 16 nested `*` emphasis spans are parsed
- **THEN** the inline content nests at most 32 levels

#### Scenario: Deeply nested images
- **WHEN** 40 images nested in each other's alt text are parsed
- **THEN** at most 32 levels of `Image` are produced and the deeper brackets stay literal text
