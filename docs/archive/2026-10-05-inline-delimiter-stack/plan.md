# Inline delimiter stack and CommonMark input handling

## Why

The inline parser finds most closers by scanning forward and keeps that linear
only through memo tables, a closer-search budget that can drop openers, and a
link-label verdict memo; precedence between marks and links follows branch order
instead of the CommonMark algorithm. Separately, a leading BOM and NUL are
parsed as written, which departs from CommonMark and from cmark, comrak, and
micromark, and accounts for four of the seven conformance failures. The same
surfaces carry four correctness defects: emphasis and strong spans miss their
own delimiters, `[foo][bar` drops CommonMark's shortcut fallback, validation
rejects link text ending in a hard line break, and the table-row splitter
predicts spoilers with rules of its own.

## What changes

- A leading U+FEFF is ignored and every U+0000 is read as U+FFFD, in structure
  recognition and in all node values; spans stay in original input coordinates.
- Underline `__`, `++`, `==`, `||`, `~` subscript, and `^` superscript join `*`,
  `_`, and `~~` on one delimiter stack. Marks that cross now resolve in closing
  order; inputs whose marks do not cross parse as before, apart from the classes
  listed under Risks.
- Every inline container, emphasis included, shares the 32-level nesting limit,
  so the deepest inline tree fits a 2 MiB stack whatever the input.
- Links, images, inline footnotes, and footnote references are resolved by a
  bracket stack when `]` arrives, as CommonMark specifies. Forming a link
  deactivates earlier `[` openers, which replaces re-parsing a label to see
  whether it holds a link.
- The `++` / `==` / underline closer-search budget, the link-label verdict memo,
  and the forward scans the stack replaces are removed.
- Emphasis and strong spans cover exactly their own delimiters and what those
  enclose, as every other emphasis-like span already does.
- A link or image label followed by a `[` that opens no reference label is a
  shortcut reference, as CommonMark specifies.
- Link text, image alt text, inline footnotes, and text directive labels may end
  with a hard line break: validation accepts it and serialization writes it.
- One table-row scanner decides which pipes delimit cells, for the parser and
  the serializer alike; it pairs spoilers as the inline parser pairs them in a
  cell, reads runs as the cell's unescaped text has them, and keeps code spans
  within one cell.
- The `^[` rule is stated as the lexical decision it is, with what happens when
  a crossing mark drops the superscript that claimed the caret.
- Decision 0006 is rewritten to describe the resulting design.
- Parse output changes for crossing marks, BOM input, and NUL input, so the
  change is released as SemVer-breaking.

Specs:
- `block-syntax` (modified)
- `inline-syntax` (modified)
- `public-api` (modified)
- `untrusted-input-cost` (modified)
- `validation` (modified)

## Out of scope

- Text directive labels and attributes (`:name[label]{attrs}`) keep their
  forward scans — directives were excluded from the migration.
- Wikilinks keep their forward scan — their content is raw text, not delimited
  Markdown.
- Code spans, inline math, autolinks, raw HTML, MDX JSX and expressions, and
  shortcodes keep their forward scans and memo tables — they are atomic
  constructs, not delimiters, in CommonMark and in this design.

## Design

### Decisions

- One delimiter stack pairs every emphasis-like mark, processing closers in
  source order with per-kind opener floors (`openers_bottom`) — because that is
  CommonMark's pairing algorithm, it is linear by construction, and it needs no
  closer budget. Turned down: migrating only `++` / `==` first, because links
  would keep their own scan, memo, and recursion and the parser would carry
  three inline styles instead of two.
- Each mark keeps today's opener and closer predicates and its length rule,
  and marks that do not cross nest as they do today; only the pairing order of
  crossing marks changes — because that limits output changes to inputs whose
  marks cross. `*` and `_` keep the CommonMark rules including the rule of
  three; `~~` pairs by exact length (1 too with single-tilde strikethrough); `~`
  and `^` open and close regardless of flanking, pair with the first closer on
  their line, and never form an empty span; `++` and `==` open with a run's last
  two characters and close with its first two (and again with each further
  aligned pair of a long run); `||` pairs like `~` but two characters at a time.
  Underline follows CommonMark `_` runs, a pair of two forming `Underline`
  instead of `Strong`. Turned down: byte-identical output for every input,
  which cannot hold once crossing marks resolve in closing order.
- Mark spans are found first, by pairing the mark runs (`++`, `==`, `||`, `~`,
  `^`, and with underline the `__` runs) among themselves; an emphasis run
  touching the inner side of a span's delimiter then flanks as the edge of the
  span's content, and a run that can both open and close does not close across
  an enclosing span — because inside a mark today's parser reads the content on
  its own, and these two rules keep properly nested marks parsing the same.
  Turned down: pairing every mark before any emphasis (today's precedence),
  which would resolve every crossing toward the mark instead of in closing
  order.
- Code spans, inline math, raw HTML, autolinks, and links keep forming in the
  main scan before marks pair, so a mark delimiter inside one is part of its
  content — because CommonMark gives these constructs precedence over emphasis.
- A `^` that a superscript on its line is waiting for closes it even when a
  `[` follows, as the forward scan let an earlier `^` claim it — because
  `^b^[link](u)` must stay a superscript and a link.
- Brackets follow CommonMark's look-for-link-or-image step: `[`, `![`, and `^[`
  push openers; `]` resolves the innermost one against what follows (`(…)`
  resource, full, collapsed, or shortcut reference; a non-empty inline
  footnote; a plain `[^label]` footnote reference), and a link deactivates
  every earlier `[`. A failed `![` leaves its `!` as text and retries as a `[`;
  a failed or unclosed `^[` wakes its `^` as a superscript closer, and a failed
  one retries as a `[`; a failed `![` whose `[` starts a wikilink running
  through its `]` becomes that wikilink. Wikilinks are tried at `[` before an opener is pushed, as today, and
  directive labels keep the `label_ends` bracket memo. Turned down: keeping
  links on forward scans, since the label verdict memo, its near-limit caveat,
  and link recursion would stay.
- When `]` forms a construct, the nodes and runs after its opener are taken off
  the pass's lists, their marks are paired as one label, and the result becomes
  the construct's content; link text then reads its autolinks as text — because
  the stack decides a construct only when its closer arrives, and link text
  holds no links. Turned down: re-parsing the label as a fresh inline pass,
  which brings back recursion and repeated work.
- Literal autolinks keep GitHub's behavior inside brackets: a bare URL inside an
  unclosed bracket is an autolink, and only link text demotes it — because the
  micromark suite, which follows GitHub, expects that in five conformance cases.
  Turned down: comrak's rule of no bare-URL autolinks while any bracket is open
  without relaxed autolinks, which three comrak cases expect; the two suites
  conflict on `[https://…]` and no rule satisfies both.
- Nesting limits move to pairing time and every inline container shares the
  32 levels: enclosing inline passes, open brackets, and each emphasis, mark,
  formed bracket, and directive level count, while `*` / `_` / `~~` also keep
  their 16-level cap within one mark, bracket label, or directive label. A `[` past the limit stays text and
  closes with the next `]` as text, a directive past it stays text, and a
  formed bracket or directive counts its content's depth toward the spans
  around it — because a tree whose depth is bounded only per mark (16 emphasis
  inside each of 32 marks) overflows a 2 MiB stack when serialized in an
  unoptimized build. Turned down: separate 32 / 16 budgets as before, which
  allow that tree.
- BOM: parsing starts after one leading U+FEFF, with spans computed in the
  original input — because cmark, comrak, and micromark skip it and an offset of
  3 needs no second coordinate system. NUL: the character classifications that
  differ between U+0000 and U+FFFD (Unicode punctuation for flanking, ASCII
  control in link destinations and URI autolinks) read `\0` as U+FFFD, and a
  single pass at the end of parsing, run only when the input contains `\0`,
  replaces it in every AST string — because CommonMark requires the replacement
  and spans must stay in original coordinates for hosts that slice the source.
  Turned down: a normalized input buffer with an offset mapper, because every
  span construction site would have to translate between two coordinate
  systems.
- Every emphasis-like span is cut from the text nodes of its consumed
  delimiters: the opener's last characters through the closer's first —
  because those nodes carry exact source positions, while counting run lengths
  from the run's start put a strong pair's span outside the emphasis around it.
- A `]` whose next `[` opens no reference label (unclosed, over-long, or holding
  a bracket) falls back to the shortcut form — because CommonMark resolves the
  reference forms in turn and only a present, valid second label suppresses the
  shortcut.
- Validation rejects a final hard line break only where the closing syntax
  cannot follow a line break: block-level inline content ends with its line, and
  emphasis-like closers do not close after one; a `]` does — because the parser
  produces exactly that shape for `[a\` followed by a newline and `](u)`, and
  it serializes back unchanged.
- Table rows are split by one scanner shared with the block-quote row check and
  the serializer's cell check: unescaped single pipes delimit, `||` runs outside
  code spans pair first-closer style with the next run, a code span (escaped
  backticks read as the inline parser reads them) counts only if it closes
  before a pipe that would split the row inside it, and runs are read in the
  cell's unescaped text, where an escaped pipe joins the bars beside it but a
  pair using one never holds a pipe that would delimit — because the inline
  parse of each cell is the ground truth, three diverging copies of the
  prediction disagreed with it and with each other, and an escaped pipe must
  never merge two cells. Turned down: never splitting at `||` when spoilers are
  on, which needs no prediction but merges `|a||b|` empty cells; forcing the
  predicted spoilers when a cell is parsed, which agrees by construction but
  makes cells resolve crossing marks unlike paragraphs; and leaving escaped
  pipes out of runs, as the old prediction did, which mispredicts `||\|||`,
  read as one five-bar run in the cell. This is a compromise: a code span
  holding a splitting pipe while a spoiler crosses it has no split the inline
  parse agrees with, and a spoiler that a link, crossing mark, inline math, raw
  HTML, or an autolink breaks in the cell leaves its bars and the pipes it held
  as text. Rows whose cells all hold code spans and spoilers parse and
  serialize about 12% slower than before; plain tables are unchanged.
- The `^` of `^[` is decided when the scan reaches it: a superscript opened
  earlier on the line, not directly before it, and still unclosed claims it — because the stack pairs
  marks only after brackets resolve and the mark-span pass reads runs after the
  caret, so whether that superscript survives is not known at the `^`. When a
  crossing mark later drops it, the caret stays text. Turned down: re-deciding
  the bracket once pairing is done, which re-parses labels.
- Decision 0006 is rewritten in place rather than superseded — because it has
  not shipped and its intermediate design is not worth a separate record.

### Risks

- [Moving links regresses CommonMark conformance, where most link edge cases
  live] → links move last, in their own group, gated on no conformance
  regression from the previous group's result.
- [A mark behaves differently in an input where nothing crosses] → each group
  compares parse output with the previous group's end on the fixture corpus and
  seeded generated inputs, including a crossing-mark generator and a generator
  of properly nested marks whose intended tree is known. An input that parsed to
  its intended tree before and no longer does is a defect; every other diff must
  fall in one of these classes: marks that cross or whose delimiters could pair
  either way, an atomic construct or link taking precedence over a mark, an
  underline run of other than two `_`, nesting past the limits (inner levels
  now form and outer delimiters stay literal), or the span of an unpaired mark
  run before spaces dropped at a line break (now excluding them, as `*` runs
  already did).
- [A NUL classification site is missed] → tests cover each listed site and the
  two NUL conformance cases.
- [Linear time or the stack bound regresses] → `tests/pathological_inputs.rs`
  and the dialect growth sweep run at the end of every group.
- [A bracket change departs from CommonMark where the conformance bench is
  silent] → the CommonMark-dialect inputs whose output changes are compared
  with comrak's rendering, and every mismatch is traced to its cause.
- [The table-row scanner disagrees with the inline parse of a cell, or the
  serializer accepts a cell the parser splits] → generated rows check that
  every predicted spoiler forms in its cell and no other does, and the
  serializer calls the parser's scanner instead of keeping its own.

## Tasks

### 1. BOM and NUL
- [x] 1.1 Add tests for every scenario of "Byte order mark and NUL" and the leading-BOM scenario of "Top-level spans tile the source", plus flanking (`*\u{0}*` delimiters), URI-autolink, and literal-autolink-host cases with NUL; verified by the tests of changed behavior failing on the current code.
- [x] 1.2 Start parsing after one leading U+FEFF and let the span-tiling test helper accept that leading gap; verified by the BOM tests and `tests/parse_span_contract.rs` passing.
- [x] 1.3 Read `\0` as U+FFFD at the flanking, link-destination, and URI-autolink classification sites, and replace it in every AST string in one end-of-parse pass guarded by a NUL check; verified by the NUL tests passing.
- [x] 1.4 Run `cargo test` with and without `html`, and the conformance bench; verified by all tests passing, conformance at 2233/2236 (the BOM and NUL cases fixed, nothing else changed), and a parse-output comparison against the group's starting commit differing only on inputs containing U+FEFF or U+0000.

### 2. Emphasis-like marks on the delimiter stack
- [x] 2.1 Dump AST, diagnostics, and `to_markdown` output for the fixture corpus and seeded generated inputs (adding a crossing-mark generator) at the end of group 1; verified by the baseline dump existing and covering every inline mark.
- [x] 2.2 Add tests for every scenario of "Marks pair in closing order" and "Atomic constructs bind tighter than marks"; verified by the scenarios whose output changes failing on the group-1 code and the others passing on both.
- [x] 2.3 Record underline `__`, `++`, `==`, and `||` runs on the delimiter stack with today's predicates and pair them in `process_emphasis`; remove `find_closing_delimiter` and the closer budget (table-cell splitting keeps `find_spoiler_close`); verified by the 2.2 tests, `tests/maximal_default.rs`, and the inline regression tests passing.
- [x] 2.4 Move `~` subscript and `^` superscript onto the stack with line-break invalidation and the non-empty rule, and remove `find_simple_inline_close` and any precedence scan the stack makes unnecessary; verified by the "Single-line subscript and superscript" and tilde/caret scenarios passing.
- [x] 2.5 Enforce the shared 32-level limit for every inline container and the 16-level limit for `*` / `_` / `~~` at pairing time; verified by the "Deeply nested highlights", "Emphasis inside highlights", and "Deep emphasis" scenarios and the stack tests in `tests/pathological_inputs.rs` passing.
- [x] 2.6 Remove the memo tables and scan-equivalence tests that served only the removed scans; verified by `cargo test` passing and `cargo clippy --all-targets` reporting no new warning kinds.
- [x] 2.7 Compare parse output with the 2.1 baseline and read every changed `.ast` / `.canonical.md` golden; verified by no properly nested input regressing from its intended tree, every other diff falling in a class listed under Risks, each changed golden checked for correct structure, conformance not below 2233, and `tests/pathological_inputs.rs` plus the growth sweep showing linear growth.

### 3. Brackets on the delimiter stack
- [x] 3.1 Dump the group-2 end state as the new baseline and add tests for every scenario of "Marks inside a link stay inside it" and "Deeply nested images"; verified by the baseline existing and the tests running.
- [x] 3.2 Take a resolved bracket's nodes and runs off the pass's lists and pair its marks as one label; verified by tests of links and images whose content holds text, marks, and nested links or images (`tests/delimiter_stack.rs`).
- [x] 3.3 Resolve links, images, inline footnotes, and footnote references at `]` with the bracket stack, deactivating earlier `[` openers after a link and keeping the wikilink check at `[`; remove `label_contains_link`, the label verdict memo, the inline-footnote and unescaped-bracket scans, and link recursion, keeping `label_ends` for directive labels and `contains_link_inline` for links inside directive labels; verified by the 3.1 tests, the CommonMark link and image conformance cases, and the inline regression tests passing.
- [x] 3.4 Compare comrak's rule for literal autolinks inside brackets with the micromark (GitHub) conformance cases and keep the behavior that more cases expect; verified by the conformance bench passing the micromark bracket cases, with the three conflicting comrak cases reported.
- [x] 3.5 Enforce the 32-level bracket limit at push time, closing a refused `[` with the next `]` as text, and count directive labels and formed brackets toward the limit; verified by the "Deeply nested images" scenario, the directive-nesting stack tests, and the image span test passing.
- [x] 3.6 Compare parse output with the 3.1 baseline and read every changed golden; verified by no properly nested input regressing from its intended tree, every CommonMark-dialect diff matching comrak or traced to a cause outside this plan, each changed golden checked, and conformance at 2236/2236 or any remaining failure reported with its cause.
- [x] 3.7 Rewrite decision 0006 to describe the delimiter-stack design, the remaining memoized forward scans, and the nesting limits, listing forward scans with memo tables and a closer budget among the considered options; verified by reading the record against the code.

### 4. Leftover correctness fixes
- [x] 4.1 Cut every emphasis-like span from its consumed delimiters' text nodes; verified by `inline_container_spans_cover_their_delimiters_and_content` passing (and failing on the group-3 code) and a parse-output comparison with the group-3 end in which only `Emphasis` and `Strong` spans move.
- [x] 4.2 Fall back to a shortcut reference when the `[` after a label opens no reference label; verified by `an_unclosed_reference_label_leaves_a_shortcut_reference` passing (and failing on the group-3 code) and conformance unchanged.
- [x] 4.3 Let link text, alt text, inline footnotes, and text directive labels end with a hard line break in validation; verified by `hard_line_break_may_end_bracketed_content` parsing, validating, serializing, and reparsing each form, and an emphasis ending in one staying invalid.
- [x] 4.4 Replace `split_table_row`'s spoiler prediction, `contains_unescaped_pipe`, and the serializer's `table_cell_has_unescaped_pipe` with one row scanner, removing `find_spoiler_close` and `find_table_cell_spoiler_close`; verified by `table_spoilers_pair_as_the_inline_parser_pairs_them` (failing on the group-3 code; covering held pipes, bars in and after code spans, escaped pipes, escaped backticks, and rows that start with a spoiler), `table_row_spoilers_form_where_the_row_scan_predicts` over generated rows with and without code spans, the serializer round-trip tests passing, and a table-row growth check staying linear.
- [x] 4.5 State the `^[` caret rule lexically and pin the crossing case; verified by `a_superscript_dropped_by_a_crossing_mark_leaves_its_caret_as_text` passing.
- [x] 4.6 Compare parse output, validation, and serialization with the group-3 end on the fixture corpus, the CommonMark examples, and seeded generated inputs; verified by every tree diff coming from 4.2 or 4.4, every span-only diff being an `Emphasis` or `Strong` span, and no input newly failing to serialize or to round-trip stably.

### 5. Integration checks
- [x] 5.1 `cargo fmt --check`, `cargo test` with and without `html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and `cargo +1.82 build` all pass.
- [x] 5.2 `tests/pathological_inputs.rs`, the growth sweep across all dialects and operations, and the 2 MiB stack check pass in debug and release builds.
- [x] 5.3 The conformance bench result and a benign-document benchmark against the pre-plan commit are measured and reported with the change.
