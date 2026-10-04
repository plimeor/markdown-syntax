# Inline delimiter stack and CommonMark input handling

## Why

The inline parser finds most closers by scanning forward and keeps that linear
only through memo tables, a closer-search budget that can drop openers, and a
link-label verdict memo; precedence between marks and links follows branch order
instead of the CommonMark algorithm. Separately, a leading BOM and NUL are
parsed as written, which departs from CommonMark and from cmark, comrak, and
micromark, and accounts for four of the seven conformance failures.

## What changes

- A leading U+FEFF is ignored and every U+0000 is read as U+FFFD, in structure
  recognition and in all node values; spans stay in original input coordinates.
- Underline `__`, `++`, `==`, `||`, `~` subscript, and `^` superscript join `*`,
  `_`, and `~~` on one delimiter stack. Marks that cross now resolve in closing
  order; inputs whose marks do not cross parse exactly as before.
- Links, images, and inline footnotes are resolved by a bracket stack when `]`
  arrives, as CommonMark specifies. Forming a link deactivates earlier `[`
  openers, which replaces re-parsing a label to see whether it holds a link.
- Without relaxed autolinks, a literal autolink is not recognized while an
  unclosed `[` precedes it.
- The `++` / `==` / underline closer-search budget, the link-label verdict memo,
  and the forward scans the stack replaces are removed.
- Decision 0006 is rewritten to describe the resulting design.
- Parse output changes for crossing marks, BOM input, and NUL input, so the
  change is released as SemVer-breaking.

Specs:
- `inline-syntax` (modified)
- `public-api` (modified)
- `untrusted-input-cost` (modified)

## Out of scope

- Text directive labels and attributes (`:name[label]{attrs}`) keep their
  forward scans — directives were excluded from the migration.
- Wikilinks and footnote references (`[^x]`) keep their forward scans — their
  content is raw text, not delimited Markdown.
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
- Each mark keeps today's opener and closer predicates and its length rule;
  only the pairing order changes — because that limits output changes to inputs
  whose marks cross. `*` and `_` keep the CommonMark rules including the rule of
  three; `__` pairs as `Underline` when underline is enabled; `~~` pairs by exact
  length (1 too with single-tilde strikethrough); `~` and `^` may open and close
  regardless of flanking, pair only with a non-empty span, and lose their open
  openers at a line break; `++`, `==`, and `||` pair by exact length 2. Turned
  down: byte-identical output for every input, which cannot hold once crossing
  marks resolve in closing order.
- Brackets follow CommonMark's look-for-link-or-image step: `[`, `![`, and `^[`
  push bracket entries; `]` resolves the nearest active one against what follows
  (`(…)` resource, full, collapsed, or shortcut reference), wraps the nodes
  between them, processes marks inside that range only, and after a link
  deactivates earlier `[` entries. Wikilinks and footnote references are tried
  at `[` before a bracket entry is pushed, as today. Directive labels keep the
  `label_ends` bracket memo. Turned down: keeping links on forward scans, since
  the label verdict memo, its near-limit caveat, and link recursion would stay.
- The flat inline node list that `process_emphasis` already uses gains the
  ability to wrap a resolved range into a container node — because the stack
  decides a construct only when its closer arrives. Turned down: re-parsing the
  range as a fresh inline pass, which brings back recursion and repeated work.
- Nesting limits keep their values and move to pairing time: brackets and the
  marks that used to recurse (`++`, `==`, `~`, `^`, `||`, underline) at 32 levels,
  `*` / `_` / `~~` at 16 — because the limits already shape output past them and
  keep tree walkers within a 2 MiB stack.
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
- Decision 0006 is rewritten in place rather than superseded — because it has
  not shipped and its intermediate design is not worth a separate record.

### Risks

- [Moving links regresses CommonMark conformance, where most link edge cases
  live] → links move last, in their own group, gated on no conformance
  regression from the previous group's result.
- [A mark behaves differently in an input where nothing crosses] → each group
  compares parse output against the previous group's end on the fixture corpus
  and seeded generated inputs, including a crossing-mark generator; every diff in
  an input without crossing marks is a defect.
- [A NUL classification site is missed] → tests cover each listed site and the
  two NUL conformance cases.
- [Linear time or the stack bound regresses] → `tests/pathological_inputs.rs`
  and the dialect growth sweep run at the end of every group.
- [comrak's bracket rule for literal autolinks differs from the spec change] →
  read comrak's source before implementing it and correct
  `specs/inline-syntax.md` first if it differs.

## Tasks

### 1. BOM and NUL
- [ ] 1.1 Add tests for every scenario of "Byte order mark and NUL" and the leading-BOM scenario of "Top-level spans tile the source", plus flanking (`*\u{0}*` delimiters) and URI-autolink cases with NUL; verified by the new tests failing on the current code.
- [ ] 1.2 Start parsing after one leading U+FEFF and let the span-tiling test helper accept that leading gap; verified by the BOM tests and `tests/parse_span_contract.rs` passing.
- [ ] 1.3 Read `\0` as U+FFFD at the flanking, link-destination, and URI-autolink classification sites, and replace it in every AST string in one end-of-parse pass guarded by a NUL check; verified by the NUL tests passing.
- [ ] 1.4 Run `cargo test` with and without `html`, and the conformance bench; verified by all tests passing, conformance at 2233/2236 (the BOM and NUL cases fixed, nothing else changed), and a parse-output comparison against the group's starting commit differing only on inputs containing U+FEFF or U+0000.

### 2. Emphasis-like marks on the delimiter stack
- [ ] 2.1 Dump AST, diagnostics, and `to_markdown` output for the fixture corpus and seeded generated inputs (adding a crossing-mark generator) at the end of group 1; verified by the baseline dump existing and covering every inline mark.
- [ ] 2.2 Add tests for every scenario of "Marks pair in closing order"; verified by the crossing scenarios failing on the current code and the non-crossing one passing.
- [ ] 2.3 Record underline `__`, `++`, `==`, and `||` runs on the delimiter stack with today's predicates and pair them in `process_emphasis`; remove `find_closing_delimiter`, `find_spoiler_close`, and the closer budget; verified by the 2.2 tests, `tests/maximal_default.rs`, and the inline regression tests passing.
- [ ] 2.4 Move `~` subscript and `^` superscript onto the stack with line-break invalidation and the non-empty rule, and remove `find_simple_inline_close` and any precedence scan the stack makes unnecessary; verified by the "Single-line subscript and superscript" and tilde/caret scenarios passing.
- [ ] 2.5 Enforce the 32-level limit for the migrated marks and the 16-level limit for `*` / `_` / `~~` at pairing time; verified by the "Deeply nested highlights" and "Deep emphasis" scenarios and the stack tests in `tests/pathological_inputs.rs` passing.
- [ ] 2.6 Remove the memo tables and scan-equivalence tests that served only the removed scans; verified by `cargo test` passing and `cargo clippy --all-targets` reporting no new warning kinds.
- [ ] 2.7 Compare parse output with the 2.1 baseline and read every changed `.ast` / `.canonical.md` golden; verified by every diff being an input with crossing marks, each changed golden checked for correct structure, conformance not below 2233, and `tests/pathological_inputs.rs` plus the growth sweep showing linear growth.

### 3. Brackets on the delimiter stack
- [ ] 3.1 Dump the group-2 end state as the new baseline and add tests for every scenario of "Marks inside a link stay inside it" and "Deeply nested images"; verified by the baseline existing and the tests running.
- [ ] 3.2 Let the inline node list wrap a resolved range into a container node; verified by unit tests wrapping ranges that hold text, marks, and nested containers.
- [ ] 3.3 Resolve links, images, and inline footnotes at `]` with the bracket stack, deactivating earlier `[` entries after a link and keeping the wikilink and footnote-reference checks at `[`; remove `label_contains_link`, `contains_link_inline`, the label verdict memo, and link recursion, keeping `label_ends` for directive labels; verified by the 3.1 tests, the CommonMark link and image conformance cases, and the inline regression tests passing.
- [ ] 3.4 Read comrak's rule for literal autolinks inside brackets, correct `specs/inline-syntax.md` if it differs, then implement it; verified by every scenario of "No literal autolinks inside brackets" and the four `autolink_*_in_brackets` conformance cases passing.
- [ ] 3.5 Enforce the 32-level bracket limit at push time; verified by the "Deeply nested images" scenario and the stack tests passing.
- [ ] 3.6 Compare parse output with the 3.1 baseline and read every changed golden; verified by every diff being an input with crossing marks or a bracketed literal URL in non-relaxed mode, each changed golden checked, and conformance at 2236/2236 or any remaining failure reported with its cause.
- [ ] 3.7 Rewrite decision 0006 to describe the delimiter-stack design, the remaining memoized forward scans, and the nesting limits, listing forward scans with memo tables and a closer budget among the considered options; verified by reading the record against the code.

### 4. Integration checks
- [ ] 4.1 `cargo fmt --check`, `cargo test` with and without `html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and `cargo +1.82 build` all pass.
- [ ] 4.2 `tests/pathological_inputs.rs`, the growth sweep across all dialects and operations, and the 2 MiB stack check pass in debug and release builds.
- [ ] 4.3 The conformance bench result and a benign-document benchmark against the pre-plan commit are recorded in the change description.
