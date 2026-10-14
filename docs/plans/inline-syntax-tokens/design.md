# Inline syntax tokens — design

## Overview

`parse_inline(text, definitions)` returns `InlineOutput { inlines, tokens,
diagnostics }`. `tokens` is a flat list of `InlineToken { span, node, role }`
that tiles `text`: in order, without gaps or overlaps, each a maximal,
non-empty run of bytes with one node and one role.

- `node` is the innermost inline whose span holds the bytes, as its index
  in `InlineOutput::nodes()`: the inlines in pre-order through
  `Inline::children()`, a link's text, an image's alt, and a directive's
  label included. The top-level inlines of a `parse_inline` tile its text,
  so every byte has a node.
- `role` is the part of that node the bytes are: `Text`, its content;
  `Syntax`, a character of its own syntax; or one of its written fields,
  `Destination`, `Title`, `Label`, `Target`, `Name`, `Attributes`. A field
  written empty (`[a](<>)`, `:d{}`) has no token.

A token names only its innermost node. The editor takes a container's mark
(strong, link) from the container's span, which `nodes()` gives it, and the
tokens refine the parts within each node; it maps (node kind, role) to a
mark and restates no rule of the syntax. The binding returns `nodes` too,
as the decoded objects in the same order, so JavaScript does not repeat the
walk.

## Roles by node

| Node | `Text` | `Syntax` | Fields |
|---|---|---|---|
| `Text`, `Html`, `SoftBreak` | all | | |
| `LineBreak` | its line ending | the spaces or `\` | |
| `Escape` | the escaped character | `\` | |
| `CharacterReference` | between `&` and `;` | `&`, `;` | |
| `Emphasis`, `Strong`, `Delete`, `Mark`, `InlineFootnote` | | the bytes its children do not cover | |
| `Code`, `Math` | between the delimiters and the padding | the delimiters (backtick runs, `$`, `$$`, `` $` ``, `` `$ ``); the padding | |
| `Link`, `Image` | | `!`, brackets, `(`, `)`, `<`, `>`, the title's delimiters, whitespace | `Destination`; `Title` |
| `LinkReference`, `ImageReference` | | `!`, brackets | `Label`, a full reference's second label |
| `Autolink` | | `<`, `>` | `Destination`: the rest |
| `FootnoteReference` | | `[^`, `]` | `Label` |
| `WikiLink` | | `!`, `[[`, `\|`, `]]` | `Target`; `Label`, after a pipe |
| `TextDirective` | | `:`, brackets, braces | `Name`; `Attributes`, all inside the braces |
| `Shortcode` | | the colons | `Name` |
| `Tag` | | `#` | `Name` |

The padding is what a code span or `$` math drops from its value: one space
or line ending at each end, when both ends hold one and the content is not
all spaces. `$$` and `` $`…`$ `` math drop none.

## Token pass

The pass runs inside `parse_inline`, after the inline parse and before the
`\r\n` mapping and the NUL rewrite, over the content the parse read. It
walks the tree in pre-order with an explicit stack (Bounded stack),
numbering nodes as it goes.

It rests on one invariant of the parse, which the plan makes a promise: a
parsed container's children tile its content, between its own opening and
closing syntax. So a container's own bytes are found by span arithmetic: a
link's `[` and `](`, a reference's `][` and `]`, a directive's brackets and
braces, a footnote's `^[` and `]`. Two parts need a scan, each run from the
last child's end or the node's start, so the pass reads each byte a bounded
number of times:

- a link's or image's tail, by the steps of `parse_link_resource` with
  `DirectLookups`, reworked to return where the destination and title lie as
  well as their values, so the parse and the pass read the tail with one
  function;
- a wiki link's separator, by `find_wikilink_separator`.

A code span's and `$` math's padding is decided by one predicate the parse
and the pass share, taken out of `normalize_code_span` and
`normalize_math_text`. A delimiter run's length is its leading run of
backticks or dollars.

Each token boundary is then mapped to `text` the way the inline spans are
(`\n` read from `\r\n` maps to both bytes), so the tokens still tile `text`.

Callers inside the crate that read only the inlines (the `with_source`
constructors, the serializer's read-back) call a crate-private entry that
skips the pass, so tokens cost only `parse_inline`'s callers.

The pass matches every `Inline` kind without a wildcard, as the NUL rewrite
does, and `Inline::children()` does too: a new kind does not compile until
both say what it holds.

## Read-back of parts

The check over the corpus reads each part in the content coordinates and
compares it with the field the parse stored:

- code and math: the `Text` bytes with the padding, normalized by the
  kind's rule, equal the value;
- a destination or title equals the stored value after decoding escapes and
  references; an angle autolink's or literal autolink's destination equals
  its `text`;
- a wiki link's target and label are its fields as written; a wiki link
  without a pipe has no `Label`;
- a directive's attribute run, parsed by `parse_attributes`, gives its
  attributes; a name equals the stored name.

## Decisions

- A flat token list beside the tree — because the editor's input is runs of
  text with marks, and the tree stays the semantic one the index and
  crates.io users read. Turned down: a span field per part on each node,
  which hand construction, equality, validation, the serializer, and the
  wire format would all carry for one consumer; the same reason decision
  0017 turns down spelling fields.
- Tokens computed after the parse from the node spans, with the parser's own
  tail and separator scanners — because recording them inside the parse
  would thread a sink through the delimiter stack, where a token's node is
  not known until the delimiters pair. Turned down: recording during the
  parse.
- A field on `InlineOutput` — because tokens describe the text the caller
  passed, which only `parse_inline` holds with its spans. Turned down: a
  method on the output, which would need the text passed again and could be
  given different text; a free function over (text, inlines), which a
  caller could feed inlines read from other text.
- `node` as a pre-order index, with `nodes()` giving that order — because it
  names a node without a path per token, and one function defines the walk
  for Rust and JavaScript. Turned down: a `NodePath` per token; an ancestor
  chain per token, which the spans of `nodes()` already give.
- A role names a part, not a style — the editor decides which parts it
  styles or hides.

## Test scope

- One case per node kind and per part, with exact tokens; an empty part;
  padding that is a line ending; a `\r\n` title; a NUL in a destination;
  nested containers through an alt and a directive label.
- Every leaf of the corpus and seeded inline text: tiling, nesting, children
  tiling their container, and parts that read back as their fields.
- The token wire encoding in Rust; non-BMP text through the binding in
  Node.
- `linear_growth.rs` and `pathological_inputs.rs` with the pass.

## Risks

- [A part and its field disagree] → the corpus check compares each part with
  its field by the field's own rule, in the coordinates the parse read.
- [Children stop tiling a container] → the corpus check states the
  invariant, and the pass is wrong only where it fails.
