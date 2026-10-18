# An item whose first line reads as a break with its bullet

## Why

The serializer wrote a bullet item's content on its marker's line even when
the two together read as one thematic break: three nested empty items as
`- - - `, and an item holding a paragraph `--` as `- --`. Both were written
with no diagnostic and read back as a thematic break. Settle's random key
test found them.

- Scope: the canonical writer and the source writer's edited items; the
  validation rule that already depends on where the serializer starts an
  item's content.

## What changes

### Validation
- `validation` › Shapes that cannot be written: modified — a list that the
  serializer starts on a line after its bullet now includes one whose first
  line reads as a thematic break with a `-` or `*` bullet, such as a
  paragraph `--` or a nested empty item two levels down

### Outside the specs
- The serializer starts such an item's content on the line after its
  bullet (`-\n  --`), as it already did for a thematic break of the
  bullet's char; parsed documents such as `-\n  --` and `*\n  **` now read
  back exactly.

Specs:
- `validation` (modified)

## Out of scope
- A paragraph whose text reads as syntax wherever it is written (`___`,
  `-`, `* *`) — text the builder answers for.
- Validating against the caller's `SerializeOptions.bullet` — validation
  judges default options, as before this change.

## Design

`starts_below_bullet` decides from the item's first written line: padding,
or, after a `-` or `*` bullet, a line of only that char, spaces, and tabs
with two or more of the char. `write_item` applies it to the written
content; validation's `first_item_starts_below_its_bullet` applies it to
`item_content_first_line`, which follows the first-child chain of nested
lists and writes at most one leaf, so validation and the writer agree and
validation stays linear. The source writer renders an edited item
canonically when its kept bullet and new first line read as a break. The
first line ends at `\n` or `\r`.

### Decisions
- Move the content below the bullet — because that is the writer's existing
  answer for a thematic break of the bullet's char. Turned down: refusing
  the shape, which `-\n  --` writes faithfully.

## Tasks

### 1. Writer
- [x] 1.1 The canonical writer starts such items below the bullet; verified by `serialize_regressions` › `an_item_whose_first_line_reads_as_a_break_with_its_bullet_starts_below_it`.
- [x] 1.2 An edited item written from the source does the same; verified by `serialize_source` › `an_edited_item_that_would_read_as_a_break_starts_below_its_bullet`.
- [x] 1.3 A tight item refuses such a list after a paragraph; verified by `validate_regressions` › `a_block_after_a_paragraph_in_a_tight_item_interrupts_it`.

### 2. Integration
- [x] 2.1 Every CI command in `CLAUDE.md` › Commands passes locally.
