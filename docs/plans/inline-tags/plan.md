# Tags in the syntax

## Why

Settle reads `#tag` from note bodies with its own regex over a flattened copy
of the inline tree, so its editor, which styles text from this crate's
parse, cannot see the same tags its index stores. Making tags part of the one
syntax gives the editor and the index one reading.

- Scope: an `Inline::Tag` node, recognized by `parse` and `parse_inline`,
  written back as `#name`, rendered as its text by the `html` feature, and
  carried by the wasm binding.
- The rule is Settle's (`docs/specs/tags.md` in plimeor/settle); the crate
  takes it over, and Settle's index reads the nodes.
- Breaking: text that held a tag now holds a `Tag` node, and decision 0012's
  list of constructs gains tags (superseded by 0015).

## What changes

### Parsing
- `inline-syntax` › Tags: added
- `public-api` › One syntax: modified — the syntax includes tags

### Writing and validation
- `validation` › Shapes that cannot be written: modified — a tag whose name
  breaks the tag rule has no spelling

### HTML
- `html-rendering` › Tags render as their text: added

### Outside the specs
- Decision 0015 "Parse entry points and one normative syntax" supersedes
  0012, restating it with tags in the syntax.
- The wasm binding's tree and `index.d.ts` gain the `tag` node.
- Settle (another repository): `crates/core/src/note/tags.rs` reads `Tag`
  nodes instead of matching its regex.

Specs:
- `inline-syntax` (modified)
- `public-api` (modified)
- `validation` (modified)
- `html-rendering` (modified)

## Out of scope
- The editor block model, blank lines, and per-node source reuse — the
  second plan of batch 2, `editor-block-model`.
- meowdown's tag rule (a tag may start with a digit) — Settle's rule is the
  requirement; the two differ only where Settle's spec decided.

## Design

### Rule

A tag is `#` followed by one letter and then any number of letters, digits,
`/`, `_`, or `-` (Unicode `L` and `N` categories), where the character before
the `#` is whitespace or the start of the leaf's inline content. It is read
over the leaf's content flattened as Settle flattens it today:

- text, soft breaks, and hard breaks (as a line ending) are read as written;
- the delimiters of `Emphasis`, `Strong`, `Delete`, and `Mark` are
  transparent: their content joins the text around them, so `*#a*` holds a
  tag and `**a**#b` does not;
- the content of a link, image, inline footnote, or text directive is read,
  with an opaque character at each of its edges, so `[see #rust](u)` holds a
  tag and `[#rust](u)` does not;
- every other inline (code, math, autolinks, wiki links, raw HTML, escapes,
  character references, shortcodes, footnote references) is one opaque
  character: nothing inside it is read, and a `#` right after it opens no
  tag.

Code blocks, HTML blocks, and frontmatter hold no inline content, so they
hold no tags.

### Parsing

Tags are found in a pass after delimiter pairing, because whether `*` is a
delimiter or text decides whether it is transparent. The pass walks each
inline list once, carrying the last flattened character across nested
emphasis-like content, and splits a `Text` node where a tag starts and ends.
It runs inside the inline parse, before spans are translated to the source,
so a tag's span is the `#` and its name in the source, and `parse_inline`
finds the same tags `parse` does. It is linear in the leaf's content.

### Node

```rust
pub struct Tag {
    pub meta: NodeMeta,
    /// The name as written, without the `#`; `/` separates levels, which the
    /// caller splits.
    pub name: String,
}
```

`Tag::new(name)` builds one. The serializer writes `#` and the name.
Validation rejects a name that breaks the rule above, since it would read
back as text. A `Text` that would form a tag where it is written is the
builder's to escape with `Escape('#')` (0013: a shape that depends on its
neighbours renders and may read back differently).

### HTML

The renderer writes a tag as its text, `#` and the name escaped as text, so
the conformance oracles, which have no tags, see the same output.

### Decisions

- Settle's rule over meowdown's — because Settle is the crate's user and its
  `tags` spec was decided there. Turned down: meowdown's rule, which also
  reads a tag that starts with a digit, such as `#1a`.
- A pass after delimiter pairing — because transparency of `*` depends on
  pairing. Turned down: recognizing `#` in the inline scanner, which would
  need to know the pairing before it is made.
- Tags render as text in HTML — because the crate offers no tag URL scheme.
  Turned down: a `<span>` or link, which would add a class name or URL rule
  the crate cannot choose for its users.

### Risks

- [The flattening drifts from Settle's, so the index loses or gains tags on
  upgrade] → Settle's tag tests are ported as this crate's tests, and
  Settle's own tests run against the new nodes in its upgrade.

## Tasks

Inner loop: `cargo test --profile ci --features html --test <name>`.

### 1. Decision
- [ ] 1.1 Write decision 0015 superseding 0012 with tags added to the syntax,
  `git mv` 0012 to `docs/decisions/superseded/`, and update links outside the
  archive; verified by `rg -n '0012' docs CLAUDE.md README.md` showing only
  superseded, archived, or updated references.

### 2. Parse
- [ ] 2.1 Add `Tag` to the AST with `Tag::new`, `From<Tag> for Inline`, and
  the prelude export; verified by `cargo test --no-run`.
- [ ] 2.2 Add the tag pass to the inline parse; verified by
  `tests/parse_inline_regressions.rs` cases naming `Tags`, ported from
  Settle's `tags.rs` tests and meowdown's recognized, rejected, and
  truncation lists where Settle's rule agrees.
- [ ] 2.3 Regenerate the `.ast` goldens that hold tags and read each moved
  golden in review; verified by `cargo test --profile ci --features html
  --test fixtures`.
- [ ] 2.4 Add tag-dense inputs to the linear-growth and pathological suites;
  verified by `tests/linear_growth.rs` and `tests/pathological_inputs.rs`.

### 3. Write, validate, render
- [ ] 3.1 Write tags as `#name`, validate their names, and escape `#` in the
  seeded round-trip generator's text where it would form a tag; verified by
  `tests/serialize_regressions.rs`, `tests/validate_regressions.rs`, and
  `tests/serialize_roundtrip_fuzz.rs`.
- [ ] 3.2 Render tags as text in HTML; verified by `tests/html_regressions.rs`
  naming `Tags render as their text`, and the conformance bench unchanged.

### 4. Binding
- [ ] 4.1 Carry `tag` through both wire directions and `index.d.ts`;
  verified by `cargo test -p markdown-syntax-wasm` and
  `node --test 'wasm/js/test/*.test.mjs'`.

### 5. Integration
- [ ] 5.1 Run Settle's tag tests against a build of this branch with
  `tags.rs` reading `Tag` nodes; verified by `cargo test -p settle-core tags`
  in a Settle checkout pointing at this branch, observed and not stored.
