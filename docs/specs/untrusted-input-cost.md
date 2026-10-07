# Untrusted input cost

## Purpose

Bounds on the time and native stack that parsing, serialization, rendering, and
validation may spend on any input, so that a crafted or accidental file cannot
stall or abort the host. Owned by the parser (`src/parse.rs`, `src/memo.rs`) and
the tree walkers that depend on its nesting limits.

## Requirements

### Requirement: Linear time
Parsing, `to_markdown`, `to_html`, and `validate` SHALL take time linear in the
input size.

#### Scenario: Unclosed openers
- **WHEN** an input of tens of thousands of unclosed openers of one construct (for example `[`, `==`, `^[`, or `<!X`) is parsed
- **THEN** doubling the input at most roughly doubles the time

#### Scenario: Repeated fragment combinations
- **WHEN** any pair or triple of syntax fragments — link, image, footnote, wiki link, and directive openers and closers, literal autolinks, code, math, emphasis, strikethrough, and highlight delimiters, a backslash, a character reference, a pipe, a space, a line ending, and block quote and list markers — is repeated `n` and `2n` times, alone or after a paragraph's first word, and parsed and serialized
- **THEN** doubling the repetitions at most roughly doubles the time of each

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

### Requirement: Bounded stack
The native stack these operations use SHALL NOT grow with how deeply the input
nests; the deepest input SHALL fit a 2 MiB thread stack, in unoptimized builds
too.

#### Scenario: Deep nesting on a small stack
- **WHEN** inputs nested tens of thousands of levels deep (block quotes, lists, links, emphasis) are parsed, serialized, rendered, and validated on a 2 MiB thread, in a debug or an optimized build
- **THEN** none overflows the stack

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

### Requirement: Emphasis nesting limit
`*` and `_` emphasis and strong, and `~~` strikethrough, SHALL nest at most 16
levels within one inline span; a delimiter pair past the limit SHALL stay
literal text.

#### Scenario: Deep emphasis
- **WHEN** 20 nested `*` emphasis spans are parsed
- **THEN** at most 16 levels of emphasis are produced and the outer delimiters stay text
