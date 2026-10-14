# Inline syntax tokens — design

## Overview

`parse_inline(text, definitions)` returns `InlineOutput { inlines, tokens,
diagnostics }`. `tokens` is a flat list of `InlineToken { span, node, role }`
that tiles `text`: in order, without gaps or overlaps, each a maximal run of
bytes with one node and one role.

- `node` is the innermost inline whose span holds the bytes, as its index in
  a pre-order walk of `inlines` through `Inline::children()` (a link's text
  and an image's alt included), or `None` for bytes outside every top-level
  inline.
- `role` is the part of that node the bytes are: `Text`, its content;
  `Syntax`, a character of its own syntax; or one of its written fields,
  `Destination`, `Title`, `Label`, `Target`, `Name`, `Attributes`.

The editor's mapping is then a table from (node kind, role) to a mark; it
restates no rule of the syntax.

## Roles by node

| Node | `Text` | `Syntax` | Fields |
|---|---|---|---|
| `Text`, `Html` | all | | |
| `SoftBreak` | all | | |
| `LineBreak` | its line ending | the spaces or `\` | |
| `Escape` | the escaped character | `\` | |
| `CharacterReference` | between `&` and `;` | `&`, `;` | |
| `Emphasis`, `Strong`, `Delete`, `Mark`, `InlineFootnote` | | every byte its children do not cover | |
| `Code`, `Math` | between the padding | backtick or dollar runs, the padding the value drops | |
| `Link`, `Image` | | `!`, brackets, `(`, `)`, `<`, `>`, quotes, whitespace | `Destination`; `Title` without its quotes |
| `LinkReference`, `ImageReference` | | `!`, brackets | `Label`, a full reference's second label |
| `Autolink` | | `<`, `>` | `Destination`: the rest |
| `FootnoteReference` | | `[^`, `]` | `Label` |
| `WikiLink` | | `!`, `[[`, `\|`, `]]` | `Target`; `Label`, after the pipe |
| `TextDirective` | | `:`, brackets, braces | `Name`; `Attributes`, inside the braces |
| `Shortcode` | | the colons | `Name` |
| `Tag` | | `#` | `Name` |

A container's children carry their own tokens; the container's tokens are
the bytes of its span its children do not cover.

## Token pass

The pass runs after the inline parse, over the tree and the text the parse
read. It walks the tree in pre-order with an explicit stack (Bounded stack),
numbering nodes as it goes, and emits a node's tokens between its children.
Each part is located inside the node's own span by the scanner the parser
used to read it: `parse_link_destination` and `parse_link_title` for a link's
tail, `find_wikilink_separator` for a wiki link, the directive opener and
attribute scanners for a directive, the backtick and dollar runs for code
and math. So the token pass decides nothing about where a construct ends;
the parse did, and the scanners only split what it read.

`parse_inline` reads `\r\n` as `\n`. The pass runs over that content, in its
coordinates, and each token boundary is then mapped to `text` the way the
inline spans are, so a `\r\n` inside a token stays in it and the tokens
still tile `text`.

Cost: every byte is scanned a bounded number of times, once by the parse and
once by the pass, so `parse_inline` stays linear (Linear time).

## Decisions

- A flat token list beside the tree — because the editor's input is runs of
  text with marks, and the tree stays the semantic one the index and
  crates.io users read. Turned down: a span field per part on each node,
  which hand construction, equality, validation, the serializer, and the
  wire format would all carry for one consumer; the same reason decision
  0017 turns down spelling fields.
- Tokens computed after the parse from the node spans, with the parser's own
  scanners — because recording them inside the parse would thread a sink
  through the delimiter stack, where a token's node is not known until the
  delimiters pair. Turned down: recording during the parse.
- A field on `InlineOutput` — because tokens describe the text the caller
  passed, which only `parse_inline` holds with its spans. Turned down: a
  method on the output, which would need the text passed again and could be
  given different text; a free function over (text, inlines), which a
  caller could feed inlines read from other text.
- `node` as a pre-order index — because it names a node without allocating
  a path per token, and JavaScript walks the same order. Turned down: a
  `NodePath` per token.
- A role names a part, not a style — the editor decides which parts it
  styles or hides.

## Test scope

- One case per node kind and per part, with exact tokens.
- Every leaf of the corpus and seeded inline text: tiling, nesting, and
  parts that read back as their fields.
- `\r\n` and NUL in the text; non-BMP text through the wasm binding.
- `linear_growth.rs` and `pathological_inputs.rs` with the pass.

## Risks

- [A part's bytes and its decoded field disagree] → the corpus check compares
  each part with its field wherever the field holds no escape or reference.
- [The pass reads past a node's span] → each scanner runs on the node's own
  slice, so it cannot.
