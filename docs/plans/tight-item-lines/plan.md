# Blocks the block before them takes

## Why

A tight item writes each block on the line after the one before, and
validation checks only a block right after a paragraph. A block after a
nested list, a quote, or a footnote that ends in a paragraph is read back as
that paragraph's lazy line, and a block after a table, a `<div>`-style HTML
block, a definition, or a quote is read back inside it: `to_markdown`
returns Markdown that reads as another tree, and Settle cannot tell that it
should write the list loose.

- Reported by Settle: a tight item holding a paragraph `a`, a tight list of
  an item `b`, and a paragraph `c` is written `"- a\n  - b\n  c\n"`, where
  `c` reads back as `b`'s lazy line.
- Scope: every pair of adjacent blocks in a tight item that the line join
  merges, and indented code after indented code or a footnote definition in
  any container, which no source spells either. Validation refuses them; the
  writer is unchanged.

## What changes

### Validation
- `validation` › Shapes that cannot be written: modified — in a tight list
  item, a block is refused after any block that takes the line written
  after it, not only after a paragraph; in any container, an indented code
  block is refused after an indented code block or a footnote definition

### Outside the specs
- Settle (another repository), once this ships in a published release: bump
  `markdown-syntax`; `save()` already writes loose the tight list a refusal's
  path falls in, so a refused item is saved loose with no Settle code change.

Specs:
- `validation` (modified)

## Out of scope
- A block after a list whose first line reaches the list's last item's
  content column (indented code, or an HTML block opening with spaces), in a
  tight item or across a blank line — the source's marker width sets that
  column, so parsed documents hold such pairs (`" -    one\n\n     two"`),
  and refusing them would refuse documents the parser produces; it needs a
  writer change, in its own plan.
- A paragraph whose text reads as syntax where it is written, such as `(t)`
  after a definition without a title — text the builder answers for, by the
  rule closing `validation` › Shapes that cannot be written.
- Writing such an item loose instead of refusing it — the serializer only
  renders (`serialization` › Rendering only), and `tight` is part of the
  tree a read-back compares.

## Design

### The rule

For each pair of adjacent blocks `left`, `right` in a container (blank
lines skipped, as for adjacent lists), `right` is refused when `left` takes
its first line. Which lines a block takes is read from its kind and its last
descendants, by the parser's own checks. In any container:

- an indented code block after an indented code block (which it continues)
  or a footnote definition (whose content is indented four columns).

In a tight item, which joins blocks by one line ending:

| `left` | `right` refused when |
|---|---|
| `Paragraph` | it does not interrupt a paragraph as written (the current rule) |
| `Definition` | the same, except a paragraph, a definition, and a setext heading, which read the same after a definition's lines |
| `Table` | its first line starts no block where no paragraph is open: a paragraph, a definition, a table, a setext heading, an HTML block opening none; the line is read as a row |
| `HtmlBlock` that a blank line ends (block-level or lone tag) | always |
| `BlockQuote` or `Alert` | `right` is a `BlockQuote` or `Alert`: the `>` lines continue it |
| a container whose last line, through the last child of each container and the last item of each list, is a paragraph's or a definition's, or is an alert without blocks | the table's set, and indented code, which cannot start while the paragraph is open: the line is the paragraph's lazy line; except after a quote or alert on the way, a block opening with a backtick or a tilde, which ends the quote's paragraph (GH-19) |

The parser checks block starts on a line after a nested container against
the matched container (the outer item), not the open paragraph, so an
ordered list from 2, an empty item, and a lone-tag HTML block start there:
the lazy and table rows use that check, not "interrupts a paragraph".

Every other left block ends where its own syntax or line does. The descent
stops at a last `BlankLine` (a closed paragraph), at a nested empty item
(the line is written short of its content column), and at fenced code,
headings, directives, and HTML containers.

The diagnostic's path is `right`'s, as for every shape among siblings, so
Settle's `save()` writes loose the innermost tight list around it, the
item's own list, which then writes the pair with a blank line between.

### Architecture

`validate_blocks` takes whether its container is a tight item and checks
each pair through `validate_taken_block`, which replaces
`validate_tight_item`. The tight table lives in `takes_next_line`, with
`starts_outside_paragraph` for the matched-container start check and
`ends_in_open_paragraph` for the descent. The descent follows only last
children, and a block reached on one left block's descent is a last child
and so is never another left block, so the walks stay linear in the tree.

Parser helpers give the HTML kinds (`html_block_opens`,
`html_block_ends_at_blank_line`); the serializer gives
`written_opens_fence_like`, which writes the right block once.

### Decisions
- Refuse in validation — because validation is the only check that refuses
  (`serialization` › Invalid documents are rejected) and Settle already
  writes a refused tight list loose. Turned down: writing the item loose,
  which changes `tight` silently.
- A rule by kind, not a probe parse of the two blocks as written — because
  validation judges shapes, and a probe writes and parses each pair at every
  nesting level (quadratic in depth on untrusted trees, decision 0011).
  Turned down: the source writer's probe (`starts_after_leaf`), which reads
  the source's markers and needs the written first line.
- Refuse only shapes no source spells — because validation runs before the
  source writer too, and refusing a parsed document would leave an editor
  unable to save a note it opened. Turned down: refusing indented code after
  a list, which wide source markers spell.

### Test scope

Hand-built tight items for each row, each refused tight and read back when
written loose; the blocks that start their own line after each row; and
parsed tight items holding those shapes (an ordered list from 2, a lone tag,
an empty item, indented code after a table, a fence-like paragraph after a
quote, a quote ending in a blank line). The fixtures and generated read-back
tests keep checking that parsed documents still write.

## Tasks

Release: the change ships as `fix` (a hand-built tree written wrongly is
now refused). The Settle bump follows once a published release holds it.

### 1. Validation
- [x] 1.1 Each pair of the rule is refused and the nearby shapes accepted; verified by `tests/validate_regressions.rs` › `written_shapes` › `a_block_in_a_tight_item_is_not_taken_by_the_block_before_it` and `indented_code_is_not_taken_by_the_block_before_it`, naming `validation: Shapes that cannot be written`.
  - Follows Design › The rule.
- [x] 1.2 The reported tree is refused with the path of its last paragraph; verified by `node_paths` › `a_paragraph_after_a_nested_list_in_a_tight_item_names_the_paragraph`, the spec change's scenario.

### 2. Integration
- [x] 2.1 Every CI command in `CLAUDE.md` › Commands passes locally.
