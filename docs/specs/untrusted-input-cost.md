# Untrusted input cost

## Purpose

Bounds on the time and native stack that parsing, serialization, rendering, and
validation may spend on any input, so that a crafted or accidental file cannot
stall or abort the host. Owned by the parser (`src/parse.rs`, `src/memo.rs`) and
the tree walkers that depend on its nesting limits.

## Requirements

### Requirement: Linear time
Parsing, `to_markdown`, `to_html`, and `validate` SHALL take time linear in the
input size, except MDX JSX tag matching, which SHALL take at most `n log n`.

#### Scenario: Unclosed openers
- **WHEN** an input of tens of thousands of unclosed openers of one construct (for example `[`, `++`, `<Tag`, or `{`) is parsed in any dialect
- **THEN** doubling the input at most roughly doubles the time

#### Scenario: Pathological suite
- **WHEN** `tests/pathological_inputs.rs` runs
- **THEN** every case finishes within its time limit

### Requirement: Bounded stack
The native stack these operations use SHALL NOT grow with how deeply the input
nests; the deepest input SHALL fit a 2 MiB thread stack, in unoptimized builds
too.

#### Scenario: Deep nesting on a small stack
- **WHEN** inputs nested thousands of levels deep (block quotes, lists, links, emphasis) are parsed, serialized, rendered, and validated on a 2 MiB thread in a debug build
- **THEN** none overflows the stack

### Requirement: Block nesting limit
Block containers (block quotes, list items, container directives, footnote
definitions, HTML containers, description details) SHALL nest at most 32 levels;
markers past the limit SHALL stay leaf-block text, usually paragraph text.

#### Scenario: 40 nested block quotes
- **WHEN** a line starting with 40 `>` markers is parsed
- **THEN** 32 levels of `BlockQuote` are produced and the remaining markers are paragraph text

### Requirement: Inline nesting limit
Inline containers — link and image labels, inline footnotes, directive labels,
emphasis and strong, strikethrough, and `++`, `==`, `~`, `^`, `||`, and
underline spans — SHALL nest at most 32 levels together; an opener past the
limit SHALL stay literal text.

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
