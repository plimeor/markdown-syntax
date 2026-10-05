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
- A lazy line that opens a list ends the block quote it would otherwise
  continue, including an empty item and an ordered item not starting at 1.
- A paragraph and a setext heading drop the final whitespace of their content.
  A level-two setext heading whose text ends in `|` is written with that pipe
  escaped, so its `---` underline is not read as a table delimiter row.
- Lazy lines continue a paragraph in list items and block quotes as cmark,
  commonmark.js, and micromark read them: after an item that started blank,
  after a single-line block, across nested containers, and at the quote level
  the paragraph sits in. A blank line ends a block quote however far it is
  indented, and a blank line between two items loosens the list.
- A complete HTML tag (a type-7 HTML block start) on a lazy line ends a list
  item, as it already ends a block quote.
- Only a line that opens an ATX heading or a code fence interrupts a
  paragraph: `#)` and a backtick run whose info string holds a backtick
  continue it.
- A line without a `>` after an unclosed fence, math block, or HTML block in a
  block quote ends the quote rather than continuing that block, and a fence
  that closes leaves the quote's next paragraph open to lazy lines again.
- A code or math block that the input ends ends its last line with the value's
  first line ending, `\n` when it has none.
- The serializer writes these so that they reparse to the same tree: an empty
  fenced code block, spaces and tabs at the ends of an info string, text right
  after a literal autolink, a paragraph that opens with a soft break, an HTML
  block value, a text line that would open an HTML block or a leaf directive,
  and an indented code block ending in `\r` or `\r\n`. The CRLF line-ending
  option leaves a value's `\r\n` as it is.
- A `_` run preceded or followed by Unicode punctuation opens and closes as
  one beside ASCII punctuation does; an escaped backslash before a line ending
  is text, not a hard break; a bare link destination ends at a space inside
  parentheses; a line with a malformed directive opener does not end a
  paragraph; and a container's last content line ends in `\n` like the others.
- The serializer escapes a run of `*`, `_`, or `$` in text whole or not at
  all, reads the text's neighbours as the reparse sees them (delimiters the
  later inlines write, tabs, and chars written as references), writes `_`
  emphasis only where `_` can open and close, indents a continuation line
  inside an inline that would start a block, writes a break that opens a line
  or a delimited span after a `&#x20;`, writes a space in a bare destination
  as a reference, starts a list item whose first block opens with whitespace
  on the line after its marker, and lengthens a code fence only past lines
  that would close it.
- A complete-tag line right after a definition continues the paragraph the
  definition was read from, a table header row indented four columns or more
  starts no table, and a directive attribute without a valid name is
  dropped, so every parsed document serializes.
- The serializer also writes these so that they reparse to the same tree:
  text that would open a text directive, shortcode, raw HTML, or math with
  an inline after it; a pipe that raw HTML, an autolink, or math writes in a
  table cell, escaped with the cell; a list before a block indented one to
  three columns, whose markers are indented past it; a paragraph after a
  block quote in a tight item; a list item whose first line would make the
  marker's line a thematic break; a code fence that a content line would
  close, indented when that keeps the fence's length; raw HTML that opens a
  paragraph or setext heading after a definition; a line that would open
  description details; frontmatter ending in an empty line; and a literal `~`
  beside an emphasis run, which stays literal unless it could pair.
- `src/parse/nul.rs` is named `nul_replacement.rs`, since `nul` is a reserved
  Windows filename that `cargo package` warns about.

Specs:
- `public-api` (modified)
- `inline-syntax` (modified)
- `serialization` (modified)
- `block-syntax` (modified)

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
- A list marker on a lazy line ends the block quote whatever its content or
  start number — because cmark, commonmark.js, markdown-it, and micromark all
  read `> a\n- ` and `> a\n2. b` as a quote followed by a list: the empty-item
  and start-number restrictions apply only when the paragraph is in the
  container the list would open in. Turned down: applying those restrictions to
  lazy lines too, which the spec's "would otherwise count as paragraph
  continuation text" could be read to ask but no reference implementation does.
- Final whitespace is dropped from the derived inline input, with its map, after
  the last line is read — because the inline parser then never sees it, and
  hard line breaks on earlier lines keep their trailing spaces. Turned down:
  trimming the last `Text` node after the inline parse, which leaves the spaces
  to interact with the inline scan first.
- The serializer escapes a `|` that ends a level-two setext heading only when a
  text node supplies it — because a pipe ending a literal autolink belongs to
  its URL, and escaping it there changes the autolink.
- A complete HTML tag on a lazy line ends the container, in list items as in
  block quotes — because cmark-gfm (GitHub) and micromark start a type-7 HTML
  block there, and the micromark case `html_flow` 147 pins it. Turned down:
  upstream cmark's and commonmark.js's reading, which keeps the tag in the
  paragraph, against the GitHub rendering.
- Whether a paragraph is open is tracked with the number of nested block quotes
  it sits in — because a line that reaches that level continues the paragraph
  by the paragraph-interruption rules, while a line that stops short of it can
  only continue it as a lazy line. Turned down: a yes/no flag, which reads
  `> > a\n> 1. ` as continuation instead of a list in the outer quote.
- The GH-19 rule that a fence-like lazy line ends a block quote's paragraph is
  kept for block quotes and not extended to list items — because extending it
  moved 22 generated inputs away from cmark, commonmark.js, and micromark.
- After a shortcut reference, the serializer escapes the character that would
  re-read its brackets, because the AST must round-trip.
  - Turned down: writing the reference in collapsed form, which changes the
    reparsed `ReferenceKind`.

### Risks
- [Round-trip classes the serializer cannot reach without the parse's
  context] → Generated inputs still fail to round-trip in four classes: a
  `Strong` or `Emphasis` nested against a run of the same char (`__**)**&__`),
  which needs `__` that the `Underline` dialect would read otherwise; a
  relaxed literal autolink that ends at a span's closing delimiter
  (`*://*&mp;`); a literal `~` whose emphasis run also touches other marks
  (`a*~ **&*`); and backticks or brackets inside a wiki link or footnote
  label (`[^`]``). Each needs the dialect or a reparse check in the
  serializer, which this plan does not add.

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

### 5. Lazy list markers and final whitespace
- [x] 5.1 Add tests for "Lazy list marker ends a block quote", "Lazy ordered item not starting at 1", "Final whitespace of a paragraph", and "Final whitespace of a setext heading", and for `> - a\n- `; verified by each failing on the group-4 code.
- [x] 5.2 Let any list marker end a lazy line in `lazy_line_starts_block`; verified by the lazy-list tests and by a comparison with commonmark.js on 30,000 generated block-structure inputs in which no input that matched before stops matching.
- [x] 5.3 Trim the final whitespace of paragraph and setext heading content before the inline parse, regenerating and reading the moved goldens; verified by the final-whitespace tests and `tests/fixtures.rs` passing.
- [x] 5.4 Escape a text `|` that ends a level-two setext heading; verified by `a_pipe_ending_a_level_two_setext_heading_does_not_start_a_table` (failing before) and a parse-output comparison with the group-4 end in which no input newly fails to round-trip and every tree diff comes from 5.2 or 5.3.

### 6. Container laziness
- [x] 6.1 Add tests for "Lazy line in an item that started blank", "Blank line indented four columns ends a block quote", "Blank line between an empty item and the next", and "Complete HTML tag on a lazy line ends a list item", and for lazy lines after a thematic break, through nested containers, and short of the paragraph's quote level; verified by each failing on the group-5 code, apart from the quote-level guard.
- [x] 6.2 End a block quote at any blank line; track an item's open paragraph afresh after a blank line or a single-line block; track the open paragraph's quote level; carry a lazy flag into nested lists; loosen a list at a blank line between items; and read a list item's lazy lines with the block quote's rule, GH-19 aside; verified by a comparison with cmark/commonmark.js and micromark, where they agree, on 43,805 generated block-structure inputs without tabs or fences, with no mismatch left and no input that matched before stopping matching, and by a parse-output comparison with the group-5 end in which no input newly fails to round-trip.

### 7. Integration checks
- [x] 7.1 `cargo fmt --check`, `cargo test` with and without `html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and `cargo +1.82 build` all pass.
- [x] 7.2 `tests/pathological_inputs.rs`, the growth sweep, and the 2 MiB stack check pass in debug and release builds.
- [x] 7.3 The conformance bench result is measured and reported with the change, along with a benign-document benchmark against the starting commit.

### 8. Paragraph interruption, verbatim blocks in block quotes, and round-trips
- [x] 8.1 Add tests for "ATX-like line that is no heading", "Backtick run with a backtick in its info", "Unclosed fence in a block quote", and "Last line ending of indented code", and for each serializer case under "Escaping keeps text literal" that this group adds, with guards that a closed fence and a fence inside a quoted list item leave lazy lines as they were; verified by each failing on the group-6 code, apart from the two guards.
- [x] 8.2 Read `#` and fence lines in `likely_block_start` with the ATX and fence opener rules; track a block quote's open fence, math block, and HTML block in `content_line_state`, which list items share; end a code or math block's last line with the value's first line ending; regenerate and read the `commonmark_code_spans`, `commonmark_blockquotes`, and `commonmark_tabs` goldens; verified by the comparison with cmark/commonmark.js and micromark on the 95,474 inputs where they agree (792 mismatches before, 751 after, none newly mismatching; of the rest, 744 involve tabs, 6 the GH-19 rule, and 1 a blank line inside an unclosed fence of a nested item) and by conformance staying at 2233 of 2236.
- [x] 8.3 Write the serializer cases above; verified by the round-trip fuzz over 30,000 generated inputs dropping from 812 failures at the group-6 end to 372, and by `tests/fixtures.rs` passing.

### 9. Inline flanking, delimiter runs, and further round-trips
- [x] 9.1 Add tests for "Underscore after Unicode punctuation", "Escaped backslash before a line ending", "Space inside a bare destination's parentheses", "Container content ending in a carriage return", "Malformed directive line inside a paragraph", and each serializer scenario this group adds; verified by each failing on the group-8 code.
- [x] 9.2 Read the `_` rules' punctuation as Unicode punctuation; take a hard break only after an unescaped backslash; end a bare destination at any space; require a valid opener for a directive line to interrupt a paragraph; end a container's last line with `\n`; verified by the tests, by conformance staying at 2233 of 2236, and by the comparison with cmark/commonmark.js and micromark on the 95,474 inputs where they agree with no input newly mismatching.
- [x] 9.3 Write the serializer cases above, regenerating the `commonmark_attention` canonical output, whose `\__foo\__bar` becomes `\_\_foo\_\_bar`; verified by the round-trip fuzz over 30,000 generated inputs dropping from 372 failures at the group-8 end to none.

### 10. Extension constructs, cells, list edges, and definitions
- [x] 10.1 Add tests for "Complete tag after a definition", "Indented table header row", and "Directive attribute without a valid name", and for each serializer scenario this group adds; verified by each failing on the group-9 code.
- [x] 10.2 Skip type-7 HTML blocks right after a definition; reject table header rows indented four columns or more; drop directive attributes whose names do not validate; and skip HTML blocks of types 1–5 when looking for an unclosed fence in a closed container; verified by the tests, by conformance staying at 2233 of 2236, and by the comparison with cmark/commonmark.js and micromark on the 95,474 inputs where they agree with no input newly mismatching.
- [x] 10.3 Write the serializer cases above, regenerating the `commonmark_character_escapes` and `math_edges` canonical outputs (a final `~` stays literal; a `$` that a later line's `$` could close is escaped) and updating two serializer regression tests whose expected output changed for the better (a thematic-break item keeps its marker; dollar math in a table cell keeps its kind); verified by the round-trip fuzz over 200,000 generated inputs on four seeds dropping from 125–168 failures to 20–27, all in the classes listed under Risks.
