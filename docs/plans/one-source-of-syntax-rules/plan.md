# One source of syntax rules

## Why

The open issues plimeor/markdown-syntax#7–#11 share one cause: parts of the
crate keep their own partial model of Markdown instead of asking the one that
decides. Block containers re-predict the state of the blocks nested in them,
the serializer copies the parser's rules to choose escapes and delimiters,
literal autolinks keep a spelling whose extent depends on what follows, and
the parser drops what the author marked literal (escapes, references) or
meant (embeds, emoji names). Patching each defect where it surfaced kept
producing new ones of the same kind (#11), and `parse` can panic.

## What changes

- Block structure is read in one pass with one stack of open blocks, as
  CommonMark's algorithm does. Block quotes, list items, container
  directives, footnote definitions, HTML containers, description details,
  and alerts are containers on the stack. The re-prediction state
  (`content_line_state`, `OpenParagraph`/`OpenParagraphIn`, `OpenBlock`,
  `lazy_flags`, `continues_verbatim`, and the lazy-line `\` insertion) is
  deleted. Every parse case listed in #11 matches commonmark.js.
- A task item's checkbox is part of the item's marker, so its paragraph and
  first inline start after it (#7). A container directive's last child ends
  after its line ending, as the same block does elsewhere.
- After a split tab, the later tabs keep their columns. A blank line inside a
  fence, math block, or HTML block of types 1–5 loosens no list. An unclosed
  fence keeps its trailing blank lines. The table-start check runs only when
  tables are on.
- Footnote definitions, leaf and container directive openers, math fences,
  and description-details markers start only when indented at most three
  columns.
- Directive fixes:
  - A leaf directive must stand alone on its line; `::a b` is paragraph text.
  - A closing fence closes the innermost container directive it can close.
  - A directive attribute without a valid name is still dropped, and is now
    reported as a warning (`DiagnosticCode::InvalidDirectiveAttribute`).
- A footnote definition takes lazy lines, and so does an alert's paragraph,
  as a quote's does. Description details parse in linear time.
- The serializer reads three things from a parse of its own rendering instead
  of from its copies of the parser's rules:
  - which text chars to escape;
  - the `*`/`_` delimiter of each emphasis and strong;
  - when a block needs a layout other than the default one.

  These copies are deleted: `shortcode_can_form`, `text_directive_can_start`,
  `tilde_run_can_pair`, the flanking checks, the literal-autolink scan
  emulation, the HTML-block, math-block, and alert-marker checks, the
  per-node delimiter choice, `serialize_reading_back`, `RunStyle`,
  `AutolinkEdges`, `RenderMemo`, and the escape memos with
  `src/serialize/escape_scan_tests.rs`.
- Canonical output changes wherever today's escapes came from those copies:
  - an unpaired delimiter stays raw (`x_y_`, not `x_y\_`);
  - a pipe ending a setext heading stays raw (`a |`, not `a \|`);
  - brackets that form no reference stay raw (`[x]` when no definition of
    `x` exists);
  - a delimiter run the parse uses is escaped whole (`\=\=a\=\=`);
  - emphasis and strong are written with `*` unless that does not read back
    (`***(a b)_.***\*#`, where `_**(a b)\_.**_\*#` was written);
  - the over-escaping #11 lists goes away.
- Serialization compares a reparsed tree with the written one apart from
  spans, reading `Escape` and `CharacterReference` as text and merging
  adjacent `Text`. It writes a node that does not read back with
  character references at its edges. It returns
  `SerializeError::Unrepresentable` (with `DiagnosticCode::Unrepresentable`)
  when no Markdown it can write reads back.
- `SerializeOptions` gains `syntax: SyntaxOptions`, the dialect the output is
  read back under. It defaults to the maximal dialect and replaces the
  default → GFM → MDX cascade.
- Literal autolinks, angle-bracket autolinks, and links whose text equals
  their URL are all `Link` nodes. `Inline::Autolink` and `AutolinkKind` are
  removed. Such a `Link` is written `<url>` when that reads back, and
  `[text](url)` otherwise.
- A literal or relaxed-scheme autolink ends at Unicode whitespace, `<`, a
  non-ASCII Unicode punctuation or symbol char, or, with wikilinks on, `[[`.
  This holds under every preset (#10). Every boundary check reads Unicode
  whitespace on char boundaries, which removes the
  `prefix_ends_with_gfm_email` panic.
- `Escape` and `CharacterReference` nodes are always produced (#8), and
  `ParseOptions::preserve_character_escapes` and
  `ParseOptions::preserve_character_references` are removed:
  - A cell's `\|` is an `Escape('|')` in text and `|` inside a raw-text
    construct.
  - The serializer writes both nodes as recorded.
  - An escape the serializer adds reads back as an `Escape` node.
- `WikiLink` gains `embed: bool`: `![[x]]` is an embed, while `\![[x]]` is an
  `Escape('!')` followed by a plain wiki link. The HTML renderer adds
  `data-wikilink-embed="true"`.
- A shortcode needs a name in a pinned gemoji table and no letter or digit as
  the source char directly outside either colon (#9). `Shortcode::glyph()`
  looks the name up, validation rejects unknown names, and the HTML renderer
  uses `glyph()` and drops its 13-name table.
- A `_` run gets no strikethrough bonus beside a `~`.
- The test gaps #11 lists are closed. Seeded round-trip generators (inline,
  block-oriented, emphasis-heavy) join the suite with their seeds recorded.
- The release is SemVer-breaking. `CHANGELOG.md` gets a migration note
  covering the changes to the AST, options, errors, and canonical output.

Specs:
- `block-syntax` (modified)
- `inline-syntax` (modified)
- `public-api` (modified)
- `serialization` (modified)
- `validation` (modified)
- `html-rendering` (modified)
- `untrusted-input-cost` (modified)

## Out of scope

- The settled divergences #11 lists stay as they are: the GH-19 rule,
  link-text autolink demotion, HTML lazy lines following GitHub, the
  cmark-gfm emphasis/autolink order, and the relaxed scheme in the GFM
  preset. The new literal-autolink boundary also applies under the GFM
  preset, as one more registered divergence from cmark-gfm. All of these
  were confirmed in the interview.
- A 0.3.x patch release for the `"\u{a0}e+@"` panic. The fix lands with the
  rewrite and ships in the next breaking release.
- Widening the directive attribute-name rule. Names stay
  `[A-Za-z_-][A-Za-z0-9_:-]*`; a dropped attribute is reported instead.
- Telling image embeds from note embeds in the HTML renderer. The renderer
  marks the embed and leaves the target to the caller.

## Design

### Decisions

- **Block parser.** The block parser follows commonmark.js's block phase.
  For each line it walks the open containers, calling each one's
  continuation check on what the outer ones left. It then tries block starts
  on the rest, continues a paragraph lazily only when the tip is a
  paragraph, and closes what did not continue. Lines keep their source
  column, and spans keep coming from `source_map.rs`. Containers no longer
  join their lines into a string to reparse, so inline input is the only
  derived string left.
  - Turned down: keeping recursive per-container reparsing and adding the
    missing state to the prediction. Two rounds of patches showed that the
    prediction always lags the real parse.
- **Reference for block cases.** When commonmark.js and micromark disagree,
  commonmark.js is the reference, and the test notes micromark's output. This
  is because commonmark.js is maintained with the CommonMark spec, and in the
  30 cases #11 lists the two disagree only on blank lines inside an unclosed
  fence, where micromark adds a line.
  - Turned down: requiring both to agree, which leaves those cases without a
    reference.
- **Container directive closing fence.** A closing fence closes the innermost
  open container directive it can close. The check runs before any block
  inside that directive reads the line, so a fence the directive holds ends
  with it, while a nested directive keeps its own closing fence. An
  opener-like line such as `:::e` is not a closing fence, so inside an HTML
  block or a fenced code block it is content.
  - Turned down: checking each directive at its own level from the outside
    in, which closes the outer directive at the inner one's fence.
- **Task checkbox.** The checkbox is consumed when the item's first
  paragraph opens, as part of the item's prefix. This is because GFM defines
  it at the start of the first paragraph, and the stack then starts the
  paragraph's line after it.
- **Tree comparison.** One crate-private comparison serves round-trip
  stability, the serializer's read-back, and `Unrepresentable`. It drops
  spans, reads `Escape` and `CharacterReference` as text, and merges adjacent
  `Text`. This is because the parser cannot tell an author's escape from one
  the serializer adds, so an escaped text char always reads back as an
  `Escape`. Serializing the reparsed tree is still byte-identical, because
  `Escape` nodes are written as recorded.
  - Turned down: comparing exact trees, which makes every escaped text char a
    mismatch.
  - Turned down: marking escapes by origin, which the source cannot express.
- **Escape rule.** A text char is escaped only when the parse of the
  rendering reads it, written raw, as part of a construct, a delimiter run
  counting whole. A backtick is always escaped. This keeps prose such as
  `x^2`, `~5`, and `a*b` raw, and keeps the serializer free of knowledge of
  how many chars break a run.
  - Turned down: escaping every run with an opening or closing role, which
    converges in one round but escapes unpaired `^`, `~`, and `*` throughout
    prose.
  - Turned down: one backslash per run, which needs the run-breaking rule
    the serializer is meant to drop.
- **Escape pipeline.** The serializer runs in these steps:
  1. It renders with text raw. Backticks are escaped, and `Escape` and
     `CharacterReference` nodes are written as recorded.
  2. It parses the rendering under `SerializeOptions::syntax` with a
     crate-private syntax trace, appending a stand-in definition for each
     reference label the document uses without defining, as today's
     read-back does per paragraph. The trace records the bytes from text nodes
     that the parse read as syntax, the flanking roles of each delimiter
     run, and the container and block each rendered line landed in.
  3. It escapes those bytes. An ASCII punctuation char takes a backslash,
     unless the trace still reads that backslash form as syntax, as with a
     backtick that closes a label's code span. Such a char, and any other
     escaped char, is written as a character reference.
  4. It repeats steps 2 and 3, up to three rounds in all. One round can
     unblock a pairing that an escaped run used to stop.
  5. It verifies with the tree comparison. For each node that does not read
     back, it writes the text chars touching that node's delimiters, inside
     and outside, as character references, then verifies again.
  6. A block that still does not read back has every ASCII punctuation char
     of its text escaped. If it then still fails, the serializer returns
     `Unrepresentable`.

  The number of parses stays constant, so serialization stays linear without
  a render cache. Parsed documents reach step 5 only in hand-built shapes,
  because a source's own references now survive as `CharacterReference`
  nodes.
  - Turned down: keeping local predicates. Each fuzz seed found a new
    combination they missed.
  - Turned down: unbounded rounds to a fixed point, which can be quadratic.
- **Shortcode boundary.** The check reads the source char next to each colon,
  as CommonMark flanking does. A character reference before the colon
  therefore lets a hand-built letter-then-shortcode be written
  (`&#97;:smile:`). The check uses `char::is_alphanumeric`, so CJK text next
  to a colon blocks a shortcode as letters do.
- **Delimiter choice.** Emphasis and strong are written with `*` and `**`.
  A run that the parse does not read where it was written switches between
  `*` and `_` where the parser's flanking for a `_` run at that position
  allows it. In a group of abutting runs, one run switches per round, the
  last one written that was not read where it was written first, and no
  set of choices is tried twice; then the run the tree comparison blames
  switches alone. A block that still does not read back retries the switches
  with its text raw, since a text char can share a delimiter run that the
  parser leaves literal (`***b_*_b_*`). Switching every abutting run together
  keeps merged runs merged, so it cannot split `**_em_**`. Both delimiters
  have the same length, so a switch never changes the text around it, and
  the rounds stay bounded. That removes the render cycle behind the
  exponential case. Today's serializer picks `_` in some places where `*`
  reads back, so that output changes.
  - Turned down: rendering children once per choice, which is the cycle
    `RenderMemo` hides.
- **Block layout from the trace.** The serializer first writes the default
  layout. It then parses the whole output and compares it with the document
  apart from what the serializer chooses (list markers, code fences, heading
  forms). Where a written line landed in a container or block other than its
  own, the serializer applies one of its layout alternatives to a node on the
  way to that difference and then verifies, withdrawing an alternative that
  leaves the difference where it was:
  - an empty quote line that ends a quote's last paragraph;
  - an empty first quote line;
  - an item's first block moved to the line after its marker;
  - a paragraph continuation line indented;
  - a nested list's markers indented past the block after it;
  - a list marker other than the next list's, so adjacent lists stay apart.

  Examples are a paragraph read as a lazy line of a quote and an HTML block
  read into a nested item. The trace decides when an alternative applies, so
  the serializer predicts nothing. Value encodings that the serializer
  derives from a node's own content stay serializer rules: fence length,
  info-string edges, and code-span fences.
  - Turned down: keeping the predicates that chose these separators ahead of
    time, which are the copies this plan removes.
- **Syntax trace.** The trace is an optional parameter that the public
  `parse` never builds, so parsing costs the same.
- **Dialect.** `SerializeOptions::syntax` defaults to
  `SyntaxOptions::default()`. This is because the serializer cannot know
  which dialect will read its output, and the reader is the one who should
  say.
  - Turned down: the default → GFM → MDX cascade, which runs several
    reparses and still guesses.
- **Links from autolinks.** A literal or angle-bracket autolink becomes a
  `Link` with a `Bare` destination, no title, and one `Text` child. This is
  the remark and comrak model, agreed in #11's review. It removes the
  serializer's need to reproduce a bare URL's extent.
- **Literal-autolink boundaries.** The boundaries use the crate's existing
  Unicode punctuation set (CommonMark's `P` and `S` categories) outside
  ASCII, and `char::is_whitespace` with char-boundary arithmetic, for GFM
  and relaxed-scheme autolinks alike. This is because the crate keeps one
  definition of punctuation, and the inline parser already reads Unicode
  whitespace. Letters stay in the URL, so IRI paths such as `/wiki/中文` keep
  working.
  - Turned down: a fixed list of full-width CJK punctuation.
  - Turned down: block whitespace (space and tab), which would let a
    no-break space join a URL.
- **Gemoji table.**
  - A standalone package, `tools/gemoji/` (`publish = false`, outside the
    published `include`, allowed its own dependencies), reads github/gemoji's
    `db.json` at a pinned tag. It writes `src/gemoji.rs`: a sorted
    `&[(&str, &str)]` of every name and alias with its emoji, headed by
    gemoji's version and its MIT notice.
  - Lookup is a binary search, so each candidate costs constant time.
  - Updating the table is an explicit regeneration.
  - Turned down: a Python script, which adds a second language to the repo.
  - Turned down: a hand-kept table, which drifts.
- **Cell `\|`.** A table cell's inline input keeps `\|`. In text, the inline
  parser reads it as an ordinary escape. Inside a raw-text construct (code
  span, inline math, raw HTML, autolink, wikilink, directive attributes, or
  MDX), it reads `|`, as GFM's cell-level unescape gives those values today.
  - Turned down: rewriting `\|` only inside code spans, which would put `\|`
    into math and raw HTML values in cells.
- **Decision 0006.** Decision 0006's bounds still hold. The text-escaping
  memos it mentions are removed with the predicates they served, and linear
  time now comes from the constant number of parses. Whether a superseding
  record is needed is put through the decision gate at archive.

### Risks

- [The block rewrite moves many goldens] → Each changed `.ast` /
  `.canonical.md` is regenerated and read for structure (decisions/0004).
  Conformance numbers are observed before and after.
- [The current serializer's predictions break round-trip fixtures against
  the new block parser] → Group 2 lands with `cargo test` green. If a
  fixture fails only because of a prediction that group 3 deletes, groups 2
  and 3 land together.
- [The canonical-output change touches many `.canonical.md` goldens] → The
  changes follow the escape rule above. Each changed golden is read, and the
  change is listed in `CHANGELOG.md`.
- [A parsed document comes back `Unrepresentable`] → The seeded generators
  treat it as a failure unless the case is listed in the test with its
  reason.
- [The gemoji table grows the default and wasm build] → Release wasm size is
  measured before and after and reported in the PR. The table is static
  string slices with no per-entry allocation.
- [Escape and reference nodes break consumers that read `Text` values] →
  The `CHANGELOG.md` migration note and the SemVer-breaking release cover
  it.
- [The syntax trace slows ordinary parsing] → The trace is off in `parse`.
  The pathological suite and growth tests run with it off and on.

### Open questions

- Which gemoji tag to pin. The latest release at implementation time is
  used. This does not change the requirements.

## Tasks

### 1. Comparison and AST surface
- [x] 1.1 Add the crate-private tree comparison and use it in the serializer's read-back and the test helpers' round-trip checks. Verified by the serialization "Tree comparison" scenarios, with `cargo test` otherwise unchanged.
- [x] 1.2 Always produce `Escape` and `CharacterReference`, with the cell `\|` rule. Remove both `preserve_*` fields and the tests that set them, and write both nodes as recorded. Verified by:
  - inline-syntax "Escapes and character references are nodes", and its modified CommonMark inlines and Hard line breaks from spaces scenarios;
  - serialization Canonical output "Escape the author wrote" and "Character reference the author wrote";
  - public-api "Escaped pipe in a table cell";
  - block-syntax Spoilers in table rows.
- [x] 1.3 Add `WikiLink.embed`: parse `![[…]]`, serialize it, and render `data-wikilink-embed`. Verified by inline-syntax "Wiki embeds", serialization "Wiki embed" and "Bang before a wiki link", and html-rendering "Wiki embeds are marked".
- [x] 1.4 Add `SerializeOptions::syntax`, used by the current read-back in place of the preset cascade. Round-trip fixtures pass their profile's options. Verified by `cargo test --test fixtures` and serialization "Content that reads back only under its preset".
- [x] 1.5 Fold `Inline::Autolink`/`AutolinkKind` into `Link` across the parser, `validate.rs`, `html/`, `source_map.rs`, and `nul_replacement.rs`. The current serializer writes these through its `Link` path until 3.5. Verified by the inline-syntax Literal autolinks "Bare URL" and Angle-bracket autolink URI scenarios, and by regenerated, read goldens.

### 2. Block parser on an open-block stack
- [x] 2.1 Line loop with the open-block stack for block quotes, list items, and paragraphs with lazy continuation, spans through `source_map.rs`. Add `nested_containers_match_the_reference` with every parse case listed in #11 and commonmark.js's HTML for it, noting where micromark differs. Verified by the block-syntax "One pass over open blocks" and CommonMark blocks scenarios.
- [x] 2.2 Leaf blocks on the stack: fences, indented code, HTML blocks, math blocks, setext and ATX headings, thematic breaks, tables, and definitions. Verified by the CommonMark oracle cases, the GFM table start scenarios, and "Delimiter-row-like line with tables off".
- [x] 2.3 Extension containers on the stack:
  - container directives, with innermost-first closing fences and the last child ending after its line ending;
  - footnote definitions with lazy lines;
  - HTML containers, description details, and alerts;
  - leaf directives alone on their line, the three-column indent limit, and the `InvalidDirectiveAttribute` warning.

  Verified by the block-syntax Block directives, Footnote definition content, GFM blocks, and Block extensions indent scenarios, and public-api "Last child of a container directive".
- [x] 2.4 Tabs after a split tab, blank lines inside open leaf blocks, and trailing blank lines of unclosed fences. Verified by "Tabs after a split tab" and "Blank lines inside open leaf blocks".
- [x] 2.5 Task checkbox as part of the item marker. Verified by the public-api task item span scenarios.
- [x] 2.6 Delete `content_line_state`, `OpenParagraph`/`OpenParagraphIn`, `OpenBlock`, `lazy_flags`, `continues_verbatim`, and the lazy `\` insertion. Add the description-details and nested-container growth cases to `tests/pathological_inputs.rs`. Verified by `grep` finding none of these names, by the untrusted-input-cost "Long description details" and "Long nested containers" scenarios, and by `cargo test`.

### 3. Parser-sourced serializer
- [x] 3.1 Add the crate-private syntax trace: bytes read as syntax, the emphasis and strong runs read where they were written, the cells a table drops, and the block a written line landed in; the flanking a `_` run would have comes from the parser's own flanking function. Verified by unit tests comparing the trace with the parsed tree over the generated inputs from `src/test_support.rs`.
- [x] 3.2 Escape pipeline: escape rule, escape forms, three rounds, edge encoding, fallback, and `SerializeError::Unrepresentable`. Delete the predicates, `serialize_reading_back`, `RunStyle`, `AutolinkEdges`, `RenderMemo`, the escape memos, and `src/serialize/escape_scan_tests.rs`. Verified by the serialization "Syntax rules come from the parser", "Escape forms", "Escaping keeps text literal", "Invalid documents are rejected", and Serialize options scenarios.
- [x] 3.3 Delimiter choice from the parser's flanking, switching abutting runs one at a time. Verified by serialization "Abutting attention runs" (both scenarios), "Text delimiter after a closing run", and untrusted-input-cost "Deeply nested emphasis".
- [x] 3.4 Block layout from the trace: the layout alternatives in Design, applied where a written line lands in the wrong container or block. Verified by:
  - serialization "Nested list before an indented block" and "Paragraph after an empty quote line in an item";
  - the retained layout scenarios "Thematic break opening a list item", "Whitespace that opens a list item's first block", "Math opening a definition's paragraph", and "Alert title and empty container directive";
  - the round-trip fixtures.
- [x] 3.5 Write autolink-form links as `<url>` when that reads back. Verified by serialization "Links written as autolinks" and validation "Link text that no angle-bracket autolink can write".
- [x] 3.6 Add `tests/serialize_roundtrip_fuzz.rs` with inline, block-oriented, and emphasis-heavy generators and their seeds recorded, run in each dialect. Verified by serialization "Seeded round-trip generators" passing.

### 4. Inline boundaries
- [x] 4.1 Literal and relaxed-scheme autolink boundary: Unicode whitespace on char boundaries, the non-ASCII punctuation/symbol stop, and the `[[` stop. Add a seeded no-panic generator over Unicode whitespace and autolink pieces. Verified by the inline-syntax Literal autolinks scenarios and the public-api Infallible parse scenarios.
- [x] 4.2 Add `tools/gemoji/` and a generated `src/gemoji.rs` at a pinned tag with gemoji's MIT notice, plus `Shortcode::glyph()`. The parser applies the name rule and the source-char boundary, validation rejects unknown names, and the HTML renderer uses `glyph()`. Verified by:
  - the inline-syntax shortcode scenarios;
  - public-api "Shortcode glyph";
  - validation "Unknown shortcode name";
  - serialization "Letter before a shortcode";
  - html-rendering "Shortcodes render their glyph".

  Release wasm size is recorded before and after.
- [x] 4.3 Drop the strikethrough bonus for `_` runs. Verified by inline-syntax "Underscores around a tilde" and serialization "Tilde beside an attention run".

### 5. Test gaps from #11
- [ ] 5.1 `spans_nest_in_the_fixture_corpus` reads cases through `read_derived_cases` and parses each under its profile. Verified by the test running every case under the case's own options.
- [ ] 5.2 Growth tests scale nesting depth for the nested-emphasis and nested-quote cases. Bounds match the scenarios: 1 s in a debug build, and roughly 2x or 4x with the tolerance stated in the test. Verified by each new case failing against `f987d66` in a scratch worktree and passing here.
- [ ] 5.3 Replace `"==\\=a=="` and the CommonMark/default `"*a***b**"` runs with inputs that fail at `f987d66`, and name the seed of every generated-input test. Verified the same way as 5.2.

### 6. Docs and release notes
- [ ] 6.1 Add a `CHANGELOG.md` migration note covering:
  - `Autolink` → `Link`, the always-on `Escape`/`CharacterReference`, the removed `preserve_*` fields, and `WikiLink.embed`;
  - the gemoji shortcodes and `Shortcode::glyph()`;
  - `SerializeOptions::syntax`, `SerializeError::Unrepresentable`, and the new diagnostic codes;
  - the canonical output changes.

  Update the README examples. Verified by the README doc-test in `cargo test` and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.

### 7. Integration checks
- [ ] 7.1 These all pass: `cargo fmt --check`, `cargo build`, `cargo test`, `cargo test --features html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and a build with Rust 1.82.
- [ ] 7.2 `cargo test --features html --test html_conformance -- --nocapture` numbers are observed before and after the change and reported in the PR, not stored.
- [ ] 7.3 Every parse case in #11 matches commonmark.js or is one of the divergences listed under Out of scope. Every round-trip case in #11 reads back. The seeded generators are clean.
- [ ] 7.4 `tests/pathological_inputs.rs` and the 2 MiB stack test pass in a debug build.
