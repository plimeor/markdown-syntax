# Source span mapping and CommonMark fixes

## Why

Spans are absolute only up to the first stripped byte. Container content (block
quotes, list items, and the other containers) is joined into one string and
re-read from a single base offset, and paragraph, heading, and table-cell inline
input is joined the same way. So every line after a stripped prefix, a CRLF, or
a cell pipe is shifted: block and inline spans slice the wrong text, and code
blocks inside containers get spans past the end of the input. Issue
plimeor/markdown-syntax#6 measures this on 31 container cases, 26 of which fail
on 0.3.0. Editors that rewrite source in place by span then panic when a span
lands inside a multi-byte character, or silently replace the wrong bytes.
Separately, three
CommonMark and round-trip defects predate the delimiter-stack change: the rule
of three uses the remaining run length, an image with an invalid `(…)` does not
fall back to a reference, and the serializer writes text that reparses as a code
span, an inline link, or a link reference definition.

## What changes

- Every parsed node's span maps to the source bytes it was read from, on every
  line of every container, across CRLF line endings, inside table cells, and
  across split tabs. This resolves issue plimeor/markdown-syntax#6, whose
  31-case regression test joins the suite.
- Table cells carry spans; today they carry none.
- Every span lies within its parent's span, in source order among its siblings,
  and this holds for the whole tree.
- The leading-whitespace qualifier on "Emphasis-like spans cover their
  delimiters" is removed.
- The rule of three and its opener floor use each delimiter run's original
  length, as CommonMark specifies.
- An image whose `(…)` is not a valid resource falls back to the full,
  collapsed, and shortcut reference forms, as a link does.
- The serializer writes every backtick in text as `` \` ``. This changes
  canonical output for text that holds a backtick it left bare before.
- The serializer escapes text after a shortcut link or image reference so that
  reparsing keeps the reference: a `(` directly after it, and a `:` directly
  after one that starts a paragraph.

Specs:
- `public-api` (modified)
- `inline-syntax` (modified)
- `serialization` (modified)

## Out of scope

- The final line of a paragraph keeps its trailing whitespace (`a  \n` gives
  `Text("a  ")`), which CommonMark strips. This is block-level whitespace
  handling that touches hard breaks and spans, so it goes to its own task.
- Renaming `src/parse/nul.rs`, a reserved Windows filename that `cargo package`
  warns about. It is a packaging fix with no behavior change and lands on its own
  before the crate is published.

## Design

### Decisions

- One source-map type owns the translation from a derived string to the
  original input. Each derived string (container content, the inline input of a
  paragraph, heading, table cell, directive label, description term, or HTML
  container) is built together with its map. A `Line` takes its positions
  through the map when it is created, and an inline parse translates its spans
  once, after the pass. This is because the span construction sites in the
  parser (about 96 `base_offset +` expressions) then stay as they are.
  - Turned down: translating at each construction site, for the reason the BOM
    and NUL change turned down a second coordinate system.
  - Turned down: a dense per-byte offset table, which costs a machine word per
    content byte at each of up to 32 container levels.
  - Turned down: mapping only leading whitespace, the scope first proposed. It
    leaves container continuation lines, CRLF, table cells, and out-of-bounds
    code-block spans wrong.
- A map is always in original-input coordinates. A container composes its
  parent's map as it copies the parent's lines. This is because a lookup then
  costs the same at any depth.
  - Turned down: chaining a child map to its parent's, which makes each lookup
    walk the nesting.
- A map is a sequence of segments, each pairing a content range with the source
  range it came from.
  - Verbatim segments map byte for byte.
  - A segment whose content replaces source maps every content byte to the
    whole source range it replaces. This covers the spaces split from a tab, the
    `\n` that joins lines ending in `\r\n`, and the `|` read from `\|`.
  - A start position at a segment boundary maps through the segment that begins
    there. An end position maps through the segment that ends there.
  - This is because these rules give each scenario its span directly: a soft
    break covers its whole line ending, and a node ending before a line break
    ends on its own line.
- Inline spans are computed in inline-input coordinates and translated by one
  walk over the finished subtree.
  - In preorder, starts never decrease, so a forward cursor maps them.
  - Each end is found by searching forward from its start's segment.
  - The walk is linear because inline nesting is capped at 32 levels.
  - Turned down: binary search per position, which is `n log n` against the
    linear-time requirement.
- A table cell's span covers its trimmed content, the text its inline parse
  reads.
  - An empty cell gets the empty range just before the pipe that closes it.
  - A cell missing from a short row gets the empty range at the row's end.
  - This is because every parsed node must carry a span, and children then start
    where their cell starts.
  - Turned down: leaving cells without spans, which breaks "Source spans".
  - Turned down: including padding and pipes, which makes adjacent cells share
    a delimiter byte.
- "Spans nest" is checked over the whole tree for the fixture corpus and
  generated inputs, because a missed construction site at any depth then fails a
  test instead of waiting for a scenario.
- Each delimiter run keeps its original length beside its remaining length.
  The rule of three and the `openers_bottom` key both use the original length,
  because cmark and commonmark.js do both. The opener floor is sound only when
  keyed on what the predicate reads, so the key uses the original length for `*`
  and `_`, whose predicate is the rule of three, and the remaining length for
  the other marks, whose predicates read it.
  - Turned down: changing only the predicate, which leaves the floor keyed on a
    length the predicate no longer reads.
  - During planning, this change reduced mismatches against commonmark.js on
    20,476 generated `*`/text inputs from 304 to 0, with `cargo test` and
    conformance unchanged.
- The image-only early return in `match_link_target` is deleted, so `![` and `[`
  resolve what follows `]` with one sequence. The experiment during planning
  left tests and conformance unchanged.
  - Turned down: an image-specific fallback path.
- Every backtick in text is escaped, and the code-span prediction
  `text_code_span_can_start` is removed. A backslash stops a backtick from
  opening a code span but not from closing one, so a prediction must reason
  about escaped backticks as closers. Dropping the prediction removes the
  serializer's copy of the code-span rules. During planning, 5 fixture inputs
  changed canonical output.
  - Turned down: predicting per run, which keeps output byte-stable but keeps a
    mirrored copy of parser rules.
- Inside an `_`-delimited emphasis, the serializer escapes a `_` in text that
  can close, as text inside `*`-delimited emphasis already encodes its `*`,
  because the reparse would close the emphasis there. Turned down: escaping
  every `*` / `_` that can close, or every character of an escaped run, which
  moved canonical output for about 1,000 corpus inputs while the defect needs a
  delimiter outside the text node.
- After a shortcut reference, the serializer escapes the character that would
  re-read its brackets, because the AST must round-trip.
  - Turned down: writing the reference in collapsed form, which changes the
    reparsed `ReferenceKind`.

### Risks

- [A derived string built without its map leaves a shifted span] → The "Spans
  nest" check runs over the corpus and over generated inputs that mix block
  quotes, list items, tables, CRLF, and tabs. Each container kind has a
  scenario or test.
- [Spaces split from a tab are assumed to occur only at a line's front, so a
  nested container that strips more could meet them elsewhere] → The generated
  inputs mix tabs with nested container markers. A counterexample changes the
  segment representation, not the requirements.
- [Map lookups break linear time] → A growth check over long, nested block
  quotes and list items and over long tables joins `tests/pathological_inputs.rs`
  and the growth sweep.
- [The rule-of-three change moves emphasis output beyond the targeted inputs] →
  Parse output is compared with the plan's starting commit. Every diff must
  involve a `*`, `_`, or underline `__` run that pairs more than once.
- [Downstream users rely on bare backticks in canonical output] → The commit
  states the output change, and the "Literal backticks are always escaped"
  scenarios pin it.

## Tasks

### 1. Rule of three
- [x] 1.1 Add tests for "Rule of three counts whole delimiter runs" and for `***a*a*a`; verified by both failing on the current code.
- [x] 1.2 Keep each delimiter run's original length, and use it in `emphasis_delimiters_match` and in `openers_bottom_key` for `*` / `_`; verified by:
  - the 1.1 tests and `cargo test` passing
  - conformance not below 2233/2236
  - a parse-output comparison with the starting commit, over the fixture corpus and seeded generated inputs, differing only in the class listed under Risks
- [x] 1.3 Escape a `_` in text that can close when the text sits inside an `_`-delimited emphasis, because the new parse produces that shape for inputs that round-tripped before; verified by `an_underscore_that_can_close_stays_inside_underscore_emphasis` (failing before) and the parse-output comparison showing no corpus input whose canonical output moves

### 2. Image fallback and text after shortcut references
- [x] 2.1 Add tests for "Image whose resource is invalid" and the three shortcut-reference scenarios of "Escaping keeps text literal"; verified by each failing on the current code.
- [x] 2.2 Delete the image-only early return in `match_link_target` and correct its doc comment; verified by the image test passing and conformance unchanged.
- [x] 2.3 Escape a `(` directly after a shortcut `LinkReference` or `ImageReference`, and a `:` directly after one that starts a paragraph; verified by the shortcut-reference tests and the round-trip fixtures passing.

### 3. Backticks
- [x] 3.1 Add tests for both scenarios of "Literal backticks are always escaped"; verified by the first failing on the current code.
- [x] 3.2 Escape every backtick in text, and remove `text_code_span_can_start` and any scan state only it used; `ordinary_punctuation_text_does_not_reparse_as_character_escapes` drops the backtick from its sample of punctuation left unescaped; verified by:
  - the 3.1 tests and `cargo test` passing
  - every moved `.canonical.md` golden regenerated and read for correctness
  - every canonical diff from the starting commit being a backtick in text

### 4. Source spans
- [x] 4.1 Add tests:
  - every scenario of "Spans map stripped lines back to the source", the new "Source spans" scenarios, and "Emphasis on a block quote continuation line"
  - issue plimeor/markdown-syntax#6's regression test, as `inline_spans_address_source_inside_containers` in `tests/parse_span_contract.rs`, with its 31 cases unchanged
  - a "Spans nest" check over the fixture corpus and over seeded generated inputs mixing containers, tables, CRLF, and tabs

  Verified by the changed-behavior tests failing on the current code, the issue's test failing 26 of 31 cases.
- [x] 4.2 Add the source-map type and build every container's content with its map, composed in original coordinates. The containers are block quotes and alerts, list items, footnote definitions, container directives, description details, and HTML containers. `Line` positions come from the map. A container whose first content line is empty keeps that line, which the old string join dropped; this realigns lazy-line flags, so `> \n> a\n- ` parses as `> a\n- ` does. Verified by:
  - the "Later block inside a block quote" and "Split tab" tests passing
  - `tests/parse_span_contract.rs` passing
  - the nest check finding no block span outside its parent
- [x] 4.3 Build every inline input with its map: paragraphs, ATX and setext headings, table cells, directive labels, description terms, and HTML containers. Translate spans in one walk per block-level inline parse. Verified by the inline scenarios, all 31 cases of `inline_spans_address_source_inside_containers`, and the nest check passing at every depth.
- [x] 4.4 Give table cells their spans, as described under Decisions; verified by the table-cell scenarios passing.
- [x] 4.5 Extend `inline_container_spans_cover_their_delimiters_and_content` to inputs with leading whitespace, block quotes, list items, tables, and CRLF; verified by it passing.
- [x] 4.6 Add a growth check for long, nested block quotes and list items and for long tables to `tests/pathological_inputs.rs`; verified by linear growth in debug and release builds.

### 5. Integration checks
- [ ] 5.1 `cargo fmt --check`, `cargo test` with and without `html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and `cargo +1.82 build` all pass.
- [ ] 5.2 `tests/pathological_inputs.rs`, the growth sweep, and the 2 MiB stack check pass in debug and release builds.
- [ ] 5.3 The conformance bench result is measured and reported with the change, along with a benign-document benchmark against the starting commit.
