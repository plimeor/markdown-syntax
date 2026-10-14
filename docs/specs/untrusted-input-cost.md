# Untrusted input cost
Type: api

## Purpose

Bounds on the time and native stack that parsing, serialization, rendering, and
validation may spend on any input, so that a crafted or accidental file cannot
stall or abort the host. Owned by the parser and the tree walkers that depend
on its nesting limits.

## Requirements

### Requirement: Linear time
Parsing with `parse` and `parse_inline`, `to_markdown`, `to_html`, `validate`,
and `Document::inline_source` SHALL take time linear in the input size: the
input, the inline text, or the document and its kept source together.

### Requirement: Bounded stack
The native stack these operations use SHALL NOT grow with how deeply the input
nests; the deepest input SHALL fit a 2 MiB thread stack, in unoptimized builds
too; a hand-built tree nested deeper than a parse produces is outside this
bound.

### Requirement: Block nesting limit
Block containers (block quotes, list items, container directives, footnote
definitions, HTML containers) SHALL nest at most 32 levels; markers past the
limit SHALL stay leaf-block text, usually paragraph text.

### Requirement: Inline nesting limit
Inline containers — link and image labels, inline footnotes, directive labels,
emphasis and strong, strikethrough, and `==` spans — SHALL nest at most 32
levels together, and `*` and `_` emphasis and strong, and `~~` strikethrough,
at most 16 levels within one inline span; an opener or a delimiter pair past
a limit SHALL stay literal text.
