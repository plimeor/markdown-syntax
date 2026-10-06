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
  deleted. Every parse case listed in #11 matches the reference.
- A task item's checkbox is part of the item's marker, so its paragraph and
  first inline start after it (#7).
- After a split tab, the later tabs keep their columns. A blank line inside a
  fence, math block, or HTML block of types 1–5 loosens no list. An unclosed
  fence keeps its trailing blank lines. The table-start check runs only when
  tables are on.
- Footnote definitions, leaf and container directive openers, math fences,
  and description-details markers start only when indented at most three
  columns. A leaf directive must stand alone on its line; `::a b` is
  paragraph text. A directive attribute without a valid name is still
  dropped, and now reported as a warning (`DiagnosticCode::InvalidDirectiveAttribute`).
  A footnote definition takes lazy lines. An alert's paragraph takes lazy
  lines as a quote's does. Description details parse in linear time.
- The serializer decides escapes and the `*`/`_` delimiter of each emphasis
  and strong by parsing its own rendering and reading which positions the
  parser took as syntax. Its copies of parser rules are deleted:
  `shortcode_can_form`, `text_directive_can_start`, `tilde_run_can_pair`, the
  flanking checks, the literal-autolink scan emulation, the HTML-block,
  math-block, and alert-marker checks, the per-node delimiter choice,
  `serialize_reading_back`, `RunStyle`, `AutolinkEdges`, `RenderMemo`, and the
  escape memos with `src/serialize/escape_scan_tests.rs`. Canonical style
  stays as it is, minus the over-escaping #11 lists.
- `SerializeOptions` gains `syntax: SyntaxOptions`, the dialect the output is
  read back under, which defaults to the maximal dialect. It replaces the
  default → GFM → MDX cascade.
- `SerializeError::Unrepresentable` (with `DiagnosticCode::Unrepresentable`)
  is returned when no Markdown the serializer can write reads back as the
  same tree.
- Literal autolinks, angle-bracket autolinks, and links whose text equals
  their URL are all `Link` nodes. `Inline::Autolink` and `AutolinkKind` are
  removed. A `Link` is written `<url>` when that reads back, and
  `[text](url)` otherwise.
- A literal autolink ends at Unicode whitespace, `<`, a non-ASCII Unicode
  punctuation or symbol char, or (with wikilinks on) `[[`, under every preset
  (#10). Every boundary check reads Unicode whitespace on char boundaries,
  which removes the `prefix_ends_with_gfm_email` panic.
- `Escape` and `CharacterReference` nodes are always produced (#8).
  `ParseOptions::preserve_character_escapes` and
  `ParseOptions::preserve_character_references` are removed. A cell's `\|` is
  an `Escape('|')`. The serializer writes both nodes as recorded.
- `WikiLink` gains `embed: bool`: `![[x]]` is an embed, while `\![[x]]` is an
  `Escape('!')` followed by a plain wiki link. The HTML renderer adds
  `data-wikilink-embed="true"`.
- A shortcode needs both a name in a pinned gemoji table and no letter or
  digit directly outside either colon (#9). `Shortcode::glyph()` looks the
  name up. Validation rejects unknown names. The HTML renderer uses
  `glyph()` and drops its 13-name table.
- A `_` run gets no strikethrough bonus beside a `~`.
- The test gaps #11 lists are closed. Seeded round-trip generators (inline,
  block-oriented, emphasis-heavy) join the suite with their seeds recorded.
- The release is SemVer-breaking. `CHANGELOG.md` gets a migration note
  covering the AST, options, errors, and canonical output changes.

Specs:
- `block-syntax` (modified)
- `inline-syntax` (modified)
- `public-api` (modified)
- `serialization` (modified)
- `validation` (modified)
- `html-rendering` (modified)
- `untrusted-input-cost` (modified)

## Out of scope

- The settled divergences that #11 lists stay as they are: the GH-19 rule,
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
- **Container directive closing fence.** A container directive's closing
  fence is checked at the directive's own level, before the blocks inside
  it. This is because "Fenced code inside a container directive" already
  commits to a closing fence that closes the directive from inside a fence.
  The opener-like line `:::e` is not a closing fence, so inside an HTML block
  or a list item's fence it is content.
  - Turned down: letting the inner blocks see the line first, which lets an
    unclosed inner fence swallow the directive's end.
- **Task checkbox.** The checkbox is consumed when the item's first
  paragraph opens, as part of the item's prefix. This is because GFM defines
  it at the start of the first paragraph, and the stack then starts the
  paragraph's line after it.
- **Parser-sourced escaping.** The serializer runs in four steps:
  1. It renders with text raw. Backticks are escaped, and `Escape` and
     `CharacterReference` nodes are written as recorded.
  2. It parses the rendering under `SerializeOptions::syntax` with a
     crate-private syntax trace. The trace records which bytes from text
     nodes the parser consumed as syntax: paired delimiter runs, construct
     openers and closers, and block markers at line starts.
  3. It escapes those positions, a delimiter run as a whole.
  4. It parses again to verify.

  It allows at most three escape rounds. After that, the affected block
  falls back to escaping every escapable text char (references for chars a
  backslash cannot escape). A block that still does not read back yields
  `Unrepresentable`. This is because the number of parses then stays
  constant, so serialization stays linear without a render cache.
  - Turned down: keeping local predicates. Each fuzz seed found a new
    combination they missed.
  - Turned down: unbounded rounds to a fixed point, which can be quadratic.
- **Delimiter choice.** The choice between `*` and `_` keeps today's
  canonical default, `*` emphasis and `**` strong. A run that does not read
  back switches to `_` where the trace's flanking roles for a `_` run at that
  position allow it, and runs that abut are switched together. This is
  because both delimiters have the same length, so a switch rewrites bytes in
  place and never re-renders children. That removes the render cycle behind
  the exponential case.
  - Turned down: rendering children once per choice, which is the cycle
    `RenderMemo` hides.
- **Syntax trace.** The trace is an optional parameter that the public
  `parse` never builds, so parsing costs the same.
- **Read-back comparison.** Trees are compared with spans dropped and
  adjacent `Text` nodes merged. Reference labels get stand-in definitions,
  as today. This is because hand-built paragraphs split their text freely.
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
- **Literal-autolink boundaries.** These boundaries use the crate's existing
  Unicode punctuation set (CommonMark's `P` and `S` categories) outside
  ASCII, and `char::is_whitespace` with char-boundary arithmetic. This is
  because the crate keeps one definition of punctuation, and the inline
  parser already reads Unicode whitespace. Letters stay in the URL, so IRI
  paths such as `/wiki/中文` keep working.
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
- **Shortcode boundary.** The boundary uses `char::is_alphanumeric`, so CJK
  text next to a colon blocks it as letters do.
- **Cell `\|`.** Cell splitting leaves `\|` in the cell's inline input, so the
  inline parser reads it as an ordinary escape. Only inside a code span in a
  cell is `\|` still rewritten to `|`, as GFM requires.
- **Decision 0006.** Decision 0006's bounds still hold. The text-escaping
  memos it mentions are removed with the predicates they served, and linear
  time now comes from the constant number of parses. Whether a superseding
  record is needed is put through the decision gate at archive.

### Risks

- [The block rewrite moves many goldens] → Each changed `.ast` /
  `.canonical.md` is regenerated and read for structure (decisions/0004).
  Conformance numbers are observed before and after. Group 3 lands only with
  `cargo test` green.
- [A parsed document comes back `Unrepresentable`] → The seeded generators
  treat it as a failure unless the case is listed in the test with its
  reason.
- [The escape rounds or the fallback over-escape common text] → The
  over-escaping scenarios and the round-trip fixtures pin canonical bytes.
  Every changed `.canonical.md` is read.
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

### 1. AST and option surface
- [ ] 1.1 Fold `Inline::Autolink`/`AutolinkKind` into `Link` across the parser, `validate.rs`, `html/`, `source_map.rs`, and `nul_replacement.rs`. The serializer writes autolink-form links as `<url>` or `[text](url)`. Verified by the inline-syntax Literal autolinks "Bare URL" and Angle-bracket autolink URI scenarios, serialization "Links written as autolinks", and regenerated, read goldens.
- [ ] 1.2 Always produce `Escape` and `CharacterReference`, including `Escape('|')` for a cell's `\|`. Remove both `preserve_*` fields and the tests that set them, and write both nodes as recorded. Verified by inline-syntax "Escapes and character references are nodes", serialization Canonical output "Escape the author wrote" and "Character reference the author wrote", public-api "Escaped pipe in a table cell", and the modified inline-syntax CommonMark inlines and Hard line breaks from spaces scenarios and block-syntax Spoilers in table rows scenarios.
- [ ] 1.3 Add `WikiLink.embed`: parse `![[…]]`, serialize it, and render `data-wikilink-embed`. Verified by inline-syntax "Wiki embeds", serialization "Wiki embed" and "Bang before a wiki link", and html-rendering "Wiki embeds are marked".
- [ ] 1.4 Add `SerializeOptions::syntax`, used by the current read-back in place of the preset cascade. Round-trip fixtures pass their profile's options. Verified by `cargo test --test fixtures` and serialization "Content that reads back only under its preset".

### 2. Inline boundaries
- [ ] 2.1 Literal-autolink boundary: Unicode whitespace on char boundaries, the non-ASCII punctuation/symbol stop, and the `[[` stop. Add a seeded no-panic generator over Unicode whitespace and autolink pieces. Verified by the inline-syntax Literal autolinks scenarios and the public-api Infallible parse scenarios.
- [ ] 2.2 Add `tools/gemoji/` and a generated `src/gemoji.rs` at a pinned tag with gemoji's MIT notice, plus `Shortcode::glyph()`. The parser applies the name and boundary rule, validation rejects unknown names, and the HTML renderer uses `glyph()`. Verified by the inline-syntax shortcode scenarios, public-api "Shortcode glyph", validation "Unknown shortcode name", and html-rendering "Shortcodes render their glyph". Release wasm size is recorded before and after.
- [ ] 2.3 Drop the strikethrough bonus for `_` runs. Verified by inline-syntax "Underscores around a tilde" and serialization "Tilde beside an attention run".

### 3. Block parser on an open-block stack
- [ ] 3.1 Line loop with the open-block stack for block quotes, list items, and paragraphs with lazy continuation, spans through `source_map.rs`. Add `nested_containers_match_the_reference` with every parse case listed in #11 and its expected HTML. Verified by the block-syntax "One pass over open blocks" scenarios and the CommonMark blocks scenarios.
- [ ] 3.2 Leaf blocks on the stack: fences, indented code, HTML blocks, math blocks, setext and ATX headings, thematic breaks, tables, and definitions. Verified by the CommonMark oracle cases, the GFM table start scenarios, and "Delimiter-row-like line with tables off".
- [ ] 3.3 Extension containers on the stack: container directives (closing fence at their level), footnote definitions with lazy lines, HTML containers, description details, and alerts. Also: leaf directives alone on their line, the three-column indent limit, and the `InvalidDirectiveAttribute` warning. Verified by the block-syntax Block directives, Footnote definition content, GFM blocks, and Block extensions indent scenarios.
- [ ] 3.4 Tabs after a split tab, blank lines inside open leaf blocks, and trailing blank lines of unclosed fences. Verified by "Tabs after a split tab" and "Blank lines inside open leaf blocks".
- [ ] 3.5 Task checkbox as part of the item marker. Verified by the public-api task item span scenarios.
- [ ] 3.6 Delete `content_line_state`, `OpenParagraph`/`OpenParagraphIn`, `OpenBlock`, `lazy_flags`, `continues_verbatim`, and the lazy `\` insertion, and add the description-details and nested-container growth cases to `tests/pathological_inputs.rs`. Verified by `grep` finding none of these names, the untrusted-input-cost "Long description details" and "Long nested containers" scenarios, and `cargo test`.

### 4. Parser-sourced serializer
- [ ] 4.1 Add the crate-private syntax trace: bytes consumed as syntax, plus flanking roles for a `*` or `_` run at a position. Verified by unit tests comparing the trace with the parsed tree over the generated inputs from `src/test_support.rs`.
- [ ] 4.2 Escape pipeline: render, trace, escape, verify, with at most three rounds, the escape-all fallback, and `SerializeError::Unrepresentable`. Delete the predicates, `serialize_reading_back`, `RunStyle`, `AutolinkEdges`, `RenderMemo`, the escape memos, and `src/serialize/escape_scan_tests.rs`. Verified by the serialization "Syntax rules come from the parser", "Escaping keeps text literal", "Invalid documents are rejected", and Serialize options scenarios.
- [ ] 4.3 Delimiter choice from trace roles, with abutting runs switched together. Verified by serialization "Abutting attention runs" (both scenarios) and untrusted-input-cost "Deeply nested emphasis".
- [ ] 4.4 Add `tests/serialize_roundtrip_fuzz.rs` with inline, block-oriented, and emphasis-heavy generators and their seeds recorded, run in each dialect. Verified by serialization "Seeded round-trip generators" passing.

### 5. Test gaps from #11
- [ ] 5.1 `spans_nest_in_the_fixture_corpus` reads cases through `read_derived_cases` and parses each under its profile. Verified by the test running every case under the case's own options.
- [ ] 5.2 Growth tests scale nesting depth for the nested-emphasis and nested-quote cases. Bounds match the scenarios: 50 ms, and roughly 2x or 4x with the tolerance stated in the test. Verified by each new case failing against `f987d66` in a scratch worktree and passing here.
- [ ] 5.3 Replace `"==\\=a=="` and the CommonMark/default `"*a***b**"` runs with inputs that fail at `f987d66`, and name the seed of every generated-input test. Verified the same way as 5.2.

### 6. Docs and release notes
- [ ] 6.1 Add a `CHANGELOG.md` migration note covering `Autolink` → `Link`, the always-on `Escape`/`CharacterReference`, the removed `preserve_*` fields, `WikiLink.embed`, the gemoji shortcodes and `Shortcode::glyph()`, `SerializeOptions::syntax`, `SerializeError::Unrepresentable`, the new diagnostic codes, and the canonical output changes. Update README examples. Verified by the README doc-test in `cargo test` and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.

### 7. Integration checks
- [ ] 7.1 `cargo fmt --check`, `cargo build`, `cargo test`, `cargo test --features html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and a build with Rust 1.82 all pass.
- [ ] 7.2 `cargo test --features html --test html_conformance -- --nocapture` numbers are observed before and after the change and reported in the PR, not stored.
- [ ] 7.3 Every parse case in #11 matches the reference or is one of the divergences listed under Out of scope. Every round-trip case in #11 reads back. The seeded generators are clean.
- [ ] 7.4 `tests/pathological_inputs.rs` and the 2 MiB stack test pass in a debug build.
