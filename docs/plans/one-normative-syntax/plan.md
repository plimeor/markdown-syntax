# One normative syntax

## Why

The crate parses four dialects, and its 34 `Constructs` flags can be combined
2^34 ways. Their union contradicts itself (`~x~`, `__x__`, raw HTML against
MDX), and the maximal default misreads ordinary prose such as `~/.bashrc`,
`^12.0.0`, and `[[Prototype]]` (plimeor/markdown-syntax#14). The serializer
has to read its own output back under every configuration, and it searches
for alternative spellings when that read-back fails. That search exists only
because the AST does not record what the source said. This plan fixes one
syntax and moves each spelling decision to where it can be made once: the
parser records it, validation rejects shapes that have no spelling, and the
serializer only renders.

## What changes

- `parse(input)` is the only entry point, and it reads one fixed syntax:
  CommonMark with raw HTML and indented code, `<details>` containers, GFM
  tables, task items, literal autolinks, and `~~` strikethrough, footnotes
  and inline footnotes, alerts, frontmatter, shortcodes, `==` highlight, wiki
  links (title after the pipe), math, and directives.
- The following are removed:
  - Configuration: `SyntaxOptions` and its presets, `Constructs`,
    `Construct`, `ParseOptions`, `WikiLinkOrder`, `SyntaxConfigError`,
    `parse_strict`, `ParseStrictError`, and `SerializeOptions::syntax`.
  - Constructs: subscript, superscript, insert, spoiler, underline,
    description lists, single-tilde strikethrough, relaxed (non-HTTP and
    scheme-less) literal autolinks, and MDX ESM, JSX, and expressions. Their
    AST nodes go too: `Subscript`, `Superscript`, `Insert`, `Spoiler`,
    `Underline`, `DescriptionList` and its item and details nodes, and the
    `MdxEsm`, `MdxExpression`, `MdxJsx`, `MdxExpressionInline`, and
    `MdxJsxInline` nodes.
  - Single-valued fields: `Delete.marker` with `DeleteMarker`, and
    `WikiLink.label_order` with `WikiLinkLabelOrder`.
  - Codes and errors: `DiagnosticCode::StrictParse`,
    `DiagnosticCode::InvalidMdx`, `DiagnosticCode::Unrepresentable`, and
    `SerializeError::Unrepresentable`.
- Precedence changes:
  - Literal autolinks take the strict GFM extents. `mailto:` and `xmpp:`
    stay. A bare URL with any other scheme stays text.
  - A directive name is one or more runs of ASCII letters joined by single
    `-`. A text directive forms only when its name is followed by `[`, `{`,
    a space, a tab, or a line ending, or ends the inline content. So
    `:noreply@x.com` is an email link and `:www.x.com` is text.
  - A whole text directive is followed by whitespace or the end of the
    content, or by ASCII punctuation when it has a non-empty label or
    non-blank attribute braces. So `:badge[ok].` is a directive, and `:e{}x`, `:e{}.`, and
    `:e[a]b` are text.
  - A wiki link's content holds no unescaped `[` or `]`, so `[[[foo]]]` is
    `[`, a wiki link, and `]`. A wiki link still wins over a defined
    reference label.
- New AST fields record which spelling a node used:
  - `Emphasis` and `Strong` record a `*` or `_` delimiter.
  - `Link` records whether it was written inline, as an angle-bracket
    autolink, or as a literal autolink.
  - Parsing fills these fields from the source. `Link::new` records an
    inline link, and both new types default to `*` and an inline link.
  - Any other node kind that the inventory in task 6.3 finds with more than
    one spelling also records it.
- Validation rejects these shapes with `InvalidDocument`:
  - a link inside link text;
  - an emphasis-like node whose content starts or ends with whitespace;
  - a recorded autolink form that does not fit its content;
  - two adjacent lists with one marker;
  - a directive name outside the new rule.
- The serializer renders and nothing else:
  - It writes text, escapes, and character references as recorded. Every
    line of a container gets its full prefix. A heading soft break is
    written as a space. Values are encoded by fixed rules.
  - It never parses its output. When the output would read back differently
    it still returns it.
  - Removed with it: the read-back, the parser's syntax trace, the escape
    rounds, delimiter switching, the layout alternatives, and every fallback
    encoding.
- Tests:
  - Every test input stays, and each assertion now states the result under
    the one syntax. Tests of the configuration API, and tests of deleted
    internal functions, are deleted.
  - The round-trip corpus drops its profiles.
  - The six conformance files that test only removed constructs are
    deleted.
  - The conformance bench parses every case with `parse`. It keeps, in code,
    the list of cases that differ by design, each with its reason.
  - The seeded round-trip test lists the generated documents that do not
    read back, each with its reason.
- The fixture inventory (task 6.3) adds these fixed rules:
  - A dash thematic break that opens the document or follows a paragraph
    line is written `- - -`.
  - A list item whose content is a thematic break of its bullet's char, or
    begins with a space or a tab, starts that content on the line after the
    bullet.
  - Tree comparison compares a code span by its value.
  - A wiki link's target and label keep their source as written, escapes
    and character references included, and the HTML renderer decodes them.
  - A literal autolink trims a trailing `;` after a hex character reference
    alone, as after a decimal one.
- The conformance bench lists, apart from the by-design deviations, the
  known defects: cases that fail on `main` as well.
- Decision 0007 records the one syntax and supersedes 0005. Decision 0008
  records the render-only serializer.
- `CHANGELOG.md` `[Unreleased]` is rewritten as one migration note for this
  change and the unreleased #15 changes. The `SerializeOptions::syntax` entry
  is dropped. `README.md`, `CLAUDE.md`, `AGENTS.md`, and `docs/overview.md`
  stop describing presets and MDX.

Specs:
- `public-api` (modified)
- `inline-syntax` (modified)
- `block-syntax` (modified)
- `serialization` (modified)
- `validation` (modified)
- `html-rendering` (modified)
- `untrusted-input-cost` (modified)

## Out of scope

- The serializer style options `bullet`, `ordered_delimiter`, and
  `fence_marker` stay, and `HtmlOptions` is unchanged. Only the renderer arms
  for removed nodes go (#14).
- Decision 0006 is not superseded, though its text still names MDX and the
  escape memos. The owner kept it as written.
- The literal-autolink `\<punct>` guard stays a recorded divergence from
  cmark-gfm, as in the archived one-source-of-syntax-rules plan.
- Phoenix HEEx snippets in prose get no special handling. `:let={f}` is a
  text directive by the directive syntax.
- No helper builds literal text with escapes for hand-built trees. The
  builder writes `Escape` nodes itself.
- Lazy continuation lines, indentation widths, and blank-line counts are not
  recorded. The serializer writes them by its standard rules.
- No source-text fallback in the serializer. That was dropped when the
  serializer became render-only.
- Fixing the type-fest readme serialization. #15 already fixed it and pinned
  it in `tests/parse_block_regressions.rs`.

## Design

### Decisions

- **The syntax is hard-coded.** Each flag read becomes its constant branch:
  a true read keeps the branch and drops the check, and a false read deletes
  the code. No crate-private `Constructs` value is kept. The parser never
  builds a modified options value: sub-parses of labels, cells, and headings
  share one reference, and context lives in other state. So a fixed syntax
  expresses every case.
  - Turned down: a private constant `Constructs`. It keeps dead branches
    alive, for a configuration that no longer exists.
- **Strict GFM literal autolinks.** Turning off `relaxed_autolinks` also
  turns off the extents it gave the kept forms: balanced `[]`/`{}`, links on
  an invalid host, and the short-`www.` rule. The kept forms then match
  cmark-gfm and markdown-rs.
  - Turned down: keeping the relaxed extents for `http(s)://` and `www.`.
    They match only comrak's relaxed mode.
- **Directive names.**
  - The name rule `[A-Za-z]+(-[A-Za-z]+)*` covers all three directive forms,
    so validation and the serializer share one rule. A text directive also
    needs `[`, `{`, whitespace, or the end of its content after its name.
  - Turned down: blocking a bare text directive only before `@` or
    `.letter`. That is narrower, and it keeps `(:note)` and `:h1`.
  - Turned down: requiring a label or attributes. That breaks bare `:name`.
- **Wiki links.**
  - A wiki link's content excludes unescaped brackets. This is the Obsidian
    note-name rule, and it splits `[[[foo]]]` predictably.
  - Turned down: no wiki link in a longer `[` run, which would leave the
    whole of `[[[foo]]]` as text.
  - Turned down: letting a defined reference label beat a wiki link. A
    reading would then depend on definitions elsewhere in the document.
- **The serializer only renders. The parser records spellings.**
  - The rule: when one node kind has more than one spelling, the parse
    records which one the source used. With that, rendering a parsed tree
    reads back without the serializer predicting or searching anything. An
    escape that matters in the source is already an `Escape` node, and is
    written as recorded.
  - For hand-built trees, the builder answers for literal text, and
    validation rejects the shapes that have no spelling.
  - Turned down: a fixed escape table at render time, as remark does. It
    escapes far more than needed (`x\_y\_`), and it is one more copy of the
    parser's rules.
  - Turned down: keeping the read-back only for escapes. Read-back plus
    search is the deferral this plan removes.
  - Turned down: a source-text fallback when the read-back fails. It needs
    the input kept in `Document`, and with rendering only there is no
    failure to fall back from.
- **Validation is the only gate.**
  - A shape is rejected when the node and its siblings show it, without
    knowing the neighbours' text: link in link, edge whitespace, a
    mismatched autolink form, adjacent same-marker lists.
  - A shape that depends on the surrounding characters is not rejected,
    such as `Text("a")` before `Emphasis(Text("(b"))`. It renders and may
    read back differently. Catching it would copy the parser's flanking
    rules into validation.
  - Turned down: a serializer error (`Unrepresentable`), and errors for
    hand-built nodes only. Both keep the serializer responsible for
    detection.
- **A replaced list marker yields by a sibling rule.** When a `bullet` or
  `ordered_delimiter` override would give a list the same marker as the list
  before it, that list takes the next marker in `-`, `*`, `+` (or `.`, `)`).
  This replaces the read-back that chose the marker.
- **Heading soft breaks become spaces.** This is decided in #14. Round-trip
  comparison reads a heading's `SoftBreak` as a space, so a parsed setext
  heading on two lines round-trips as one line.
- **Tests keep their inputs.**
  - Every fixture, regression, pathological, and generated input stays, and
    is asserted under the one syntax, with assertions rewritten to the real
    result (e.g. `H~2~O` is text). Goldens that move are regenerated and
    read one by one (decision 0004).
  - A test of a deleted internal function goes with the function.
  - Turned down: deleting tests whose subject was a removed construct. Their
    inputs now pin that the syntax reads as text.
- **Exception lists live in code and are checked.** These are not ledgers
  under decision 0003, because a test or the bench derives and checks every
  entry.
  - The conformance bench reports a listed case that starts passing apart
    from an unlisted case that starts deviating. The bench is still not a
    gate.
  - The seeded round-trip test fails on an unlisted document that does not
    read back, and also on a listed document that does, so the list cannot
    go stale.
- **Release.** This lands in the unreleased version that already holds #15
  (`feat!`), and `[Unreleased]` describes the net change.

### Risks

- [A round-trip fixture reads back only with a spelling the AST does not
  record] → Record it at parse time, per the rule above. If the spelling is
  whitespace or a lazy line, which this plan writes by standard rules, stop
  and take the case to the user before changing a requirement.
- [Rendering raw text changes many `.canonical.md` goldens] → Each changed
  golden is regenerated and read. The CHANGELOG lists the kinds of change:
  fewer escapes, recorded `_` delimiters, literal links kept literal, and
  heading soft breaks.
- [Downstream code that builds `Text` holding Markdown punctuation now gets
  that punctuation read as syntax] → The CHANGELOG migration note says to
  build literal punctuation from `Escape` nodes, and lists the shapes
  validation now rejects.
- [The directive name rule turns existing directives into text: `:h1[x]`,
  `::my_note`] → The CHANGELOG names the rule. A leaf or container opener
  with such a name reports `InvalidDirectiveName`.
- [Conformance results fall under the one syntax] → Every by-design
  deviation is listed with its reason. Numbers are observed before and
  after and reported in the PR, not stored.
- [Removing the parser's syntax trace or the serializer's memos breaks a
  linear-time bound] → `tests/pathological_inputs.rs` and the growth tests
  run after each serializer task.

## Tasks

### 1. Decision records
- [x] 1.1 Write decision 0007, one normative syntax: `Accepted`, superseding 0005. Change only 0005's status line to `Superseded by 0007`. Verified by reading both files.
- [x] 1.2 Write decision 0008, a render-only serializer with spellings fixed at parse and validation as the gate: `Accepted`. Verified by reading the file.

### 2. One syntax in the parser
- [x] 2.1 Hard-code the syntax: replace every flag read in `src/parse.rs` and `src/parse/blocks.rs` with its constant branch, and drop the options parameter from the parser's functions. Delete `src/options.rs`, `SyntaxOptions::parse`, `parse_strict`, `ParseStrictError`, and their crate-root and prelude exports. Port every test caller to `parse`:
  - `tests/support/fixtures.rs`: drop `profile_options`, `assert_required_profiles`, the per-profile MANIFEST check, and the `profile` field in `.cases` headers;
  - the preset loops in `parse_inline_regressions.rs`, `serialize_regressions.rs`, `parse_span_contract.rs`, `serialize_roundtrip_fuzz.rs`, and `src/serialize/read_back.rs`;
  - `tests/html_conformance/runner.rs` `plan()`, which keeps only the render tokens.

  Regenerate and read each golden that moves. Verified by `cargo build`, by `grep` finding no `SyntaxOptions`, `Constructs`, `Construct`, `ParseOptions`, `parse_strict`, or `WikiLinkOrder` in `src/` and `tests/`, and by `cargo test` passing.
- [x] 2.2 Delete the code of the removed constructs from the parser, block parser, AST, HTML renderer, `validate.rs`, `compare.rs`, `source_map.rs`, `nul_replacement.rs`, and `src/test_support.rs`, along with their scan mirrors and the tests of those internals in `src/parse/scan_tests.rs`. Rewrite each assertion that named a removed node to the result under the one syntax. Verified by:
  - inline-syntax "Strikethrough takes two tildes", "Inline footnotes", "Double underscore is strong", "Extension marks", and "Marks pair in closing order";
  - block-syntax "Extension blocks" and "Table rows split at unescaped pipes";
  - public-api "One syntax";
  - `cargo test`.
- [x] 2.3 Remove `DiagnosticCode::StrictParse`, `DiagnosticCode::InvalidMdx`, `Delete.marker`/`DeleteMarker`, and `WikiLink.label_order`/`WikiLinkLabelOrder`. Verified by `cargo test` and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.

### 3. Precedence rules
- [x] 3.1 Strict GFM literal autolinks: delete the relaxed path and the extents it gave the kept forms, and keep `mailto:` and `xmpp:`. Verified by inline-syntax "Literal autolinks", including "Other schemes stay text" and "Prefixed email forms".
- [x] 3.2 Directive names: apply `[A-Za-z]+(-[A-Za-z]+)*` to all three forms in the parser, `validate.rs`, and the serializer, and add the text directive's following-char rule. Verified by inline-syntax "Shortcodes and text directives share the colon" and block-syntax "Block directives". This includes the gfm_autolink_literal oracle cases 54 and 55.
- [x] 3.3 Wiki links: no unescaped `[` or `]` in the content. Verified by inline-syntax "Wikilinks": "Extra brackets around a wikilink" and "Defined label".

### 4. Recorded spellings
- [x] 4.1 Add a `*`/`_` delimiter field to `Emphasis` and `Strong`, and a form field to `Link` (inline, angle-bracket autolink, literal autolink). The parser fills them; `Link::new` records an inline link, and both types' `Default` is `*` and inline. Verified by public-api "Recorded syntax forms".

### 5. Validation gate
- [x] 5.1 Reject the new shapes in `validate.rs`: link inside link text; emphasis-like content with edge whitespace; an autolink form that does not fit its content; adjacent same-marker lists; directive names outside the rule. Verified by the validation "Shapes that cannot be written" scenarios and serialization "Link inside a link".

### 6. Render-only serializer
- [x] 6.1 Delete the read-back:
  - `src/serialize/read_back.rs`, `src/serialize/layout.rs`, the parser's syntax trace, the escape rounds, delimiter switching, edge character references, the all-escapes fallback, and the settle passes;
  - `SerializeOptions::syntax`, `SerializeError::Unrepresentable`, and `DiagnosticCode::Unrepresentable`.

  `compare.rs` stays for round-trip checks and reads a heading's `SoftBreak` as a space. Verified by `grep` finding no `read_back`, `Unrepresentable`, or trace parameter in `src/`, and by serialization "Rendering only" and "Tree comparison".
- [x] 6.2 Render by rule:
  - text, escapes, and references as recorded;
  - recorded delimiters and link forms;
  - full container prefixes, and heading soft breaks as spaces;
  - the value encodings, and the list-marker yield rule.

  Verified by the serialization scenarios "Text is written as recorded", "Container lines take their full prefix", "Heading soft breaks", "Values are encoded by rule", "Links written in their recorded form", "Canonical output", and "Serialize options".
- [x] 6.3 Inventory: run every fixture under `tests/fixtures/roundtrip/` through parse, render, and reparse. Settle each failure with a recorded spelling in the parser (task 4.1's rule), a fixed render rule, or a validation rule, never a search, and list each addition in this plan's What changes. Verified by `cargo test --test fixtures` and serialization "Round-trip fixtures".

  Inputs that read back only with a lazy line, whitespace, or a blank line
  the AST does not record are accepted as not reading back (Risks, first
  entry): the fixtures `gfm_table_containers` and `gfm_table_edges`, the
  derived cases listed in `tests/support/fixtures.rs`, and the inputs that
  `tests/serialize_regressions.rs` and `tests/serialize_roundtrip_fuzz.rs`
  list, each with its reason.
- [x] 6.4 Seeded round trip: `tests/serialize_roundtrip_fuzz.rs` runs one syntax and lists each generated document that does not read back, with its reason. Listed documents must serialize without panicking, and a listed document that reads back fails the test. Keep every parsed input of the removed serialization scenarios in `tests/serialize_regressions.rs`, each asserting a round trip or a listed reason. Verified by serialization "Seeded round-trip generators" and `cargo test`.

### 7. Conformance bench and cost tests
- [x] 7.1 Delete `gfm/description_lists`, `spoiler`, `subscript`, `supersubscript`, `insert`, and `underline`, and add the deviation list in `tests/html_conformance/`: file, case, and reason for each case that differs by design. The report prints, separately, the listed cases that now pass and the unlisted cases that deviate. Verified by inline-syntax and block-syntax "CommonMark oracle cases", and by `cargo test --features html --test html_conformance -- --nocapture` showing no unlisted deviation.
- [x] 7.2 `tests/pathological_inputs.rs`:
  - keep every input under `parse`;
  - add the long footnote definition case;
  - drop the MDX `n log n` allowance;
  - update `inline_depth` for the removed nodes.

  Verified by untrusted-input-cost "Linear time", "Block nesting limit", and "Inline nesting limit", and by the suite passing in a debug build.

### 8. Docs and release notes
- [x] 8.1 Rewrite `CHANGELOG.md` `[Unreleased]` as one migration note, folding in #15's unreleased changes and dropping the `SerializeOptions::syntax` entry. Cover:
  - the removed API, constructs, nodes, fields, codes, and errors;
  - the precedence changes;
  - the recorded spellings;
  - text written as recorded, and building literal punctuation with `Escape`;
  - the new validation rejections;
  - the canonical output changes.

  Verified by reading it against the code.
- [x] 8.2 Update `README.md` (examples, dialect table, scope list), `CLAUDE.md` and `AGENTS.md` (the identity line no longer mentions MDX), `docs/overview.md` (What this is, Current focus, Next), and the Purpose lines of `docs/specs/public-api.md` and `docs/specs/inline-syntax.md`, which name `src/options.rs`, choosing a dialect, and the MDX forms. Verified by the README doc-test in `cargo test` and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`.

### 9. Integration checks
- [x] 9.1 These all pass: `cargo fmt --check`, `cargo build`, `cargo test`, `cargo test --features html`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, `cargo build --target wasm32-unknown-unknown`, and a build with Rust 1.82.
- [ ] 9.2 Conformance numbers observed on `main` and on the branch, reported in the PR and not stored; every deviation is listed with its reason.
- [x] 9.3 `tests/pathological_inputs.rs` and the 2 MiB stack test pass in a debug build.

### 10. Design review follow-ups
The design review of this branch found the issues below; each task carries the owner's decided fix. Where a task and an earlier task disagree (4.1's `Link` form, 5.1's autolink rule), the later task wins.
- [x] 10.1 Linear time: memoize the link-resource tail scan behind literal autolinks in unclosed link text; add `tests/linear_growth.rs`, which times pairs and triples of syntax fragments at n and 2n; fix every quadratic shape it finds; recalibrate `tests/pathological_inputs.rs` for the `ci` profile. Verified by `cargo test --profile ci --features html --test linear_growth --test pathological_inputs`.
- [x] 10.2 Validation defers to the owner of each rule: an `Autolink` is valid when the parser's own destination function accepts its text, and validation and the serializer share `written_marker` for adjacent lists. Verified by validation "Autolink text that is not one autolink" and "Adjacent lists written with one marker".
- [x] 10.3 Validation is the only gate: code-form math holding its close and empty inline math are validation errors, `SerializeError::UnsupportedNode` is removed, and the validation spec lists exactly the checks the code makes. Verified by `tests/validate_regressions.rs`.
- [x] 10.4 Conformance exception lists are keyed by case content, and `exception_lists_are_current` fails on a stale or duplicate entry. Verified by `cargo test --profile ci --features html --test html_conformance`.
- [x] 10.5 Tests organized by flow: Markdown → AST (`.ast`), AST → Markdown (exact strings), Markdown → AST → Markdown (`.canonical.md` and `CANONICAL_INPUTS`), source read-back (`sources_read_back`) with one content-keyed exception list, and tree read-back in the seeded fuzz test only. Verified by `cargo test --profile ci --features html --test fixtures --test serialize_regressions`.
- [x] 10.6 Table cells: the serializer encodes `|` in one pass over each written cell, following the parser's cell split, and validation rejects a cell value no cell source can spell. Verified by serialization "Table cells encoded by one rule".
- [x] 10.7 A directive opener reports why it was refused (`Refused::{Name, Unclosed, Follow}`); diagnostics are emitted by reason. Verified by the existing directive tests and a differential parse of about 575k inputs.
- [x] 10.8 Strikethrough and autolink residue of removed configuration is deleted (`strike`, `DelimRoles`, `trim_from`). Verified by the same differential parse.
- [x] 10.9 One crate-private decoder for escapes and character references; `WikiLink::decoded_target()` and `WikiLink::decoded_label()`. Verified by `tests/parse_inline_regressions.rs`.
- [x] 10.10 Docs agree with the code: literal autolink extents, constructors, fixture READMEs, and this plan's serialization spec copy. Verified by reading `docs/specs/inline-syntax.md`, `docs/specs/public-api.md`, and the fixture READMEs.
- [x] 10.11 CI runs every test once under `[profile.ci]` (release with debug assertions and overflow checks); the wasm32 and MSRV jobs are removed. Verified by reading `.github/workflows/ci.yml` and running its steps.
- [x] 10.12 `SerializeOptions` list-marker and fence fields are `Option`s: `None` keeps the recorded spelling, `Some` replaces it. Verified by serialization "Serialize options".
- [x] 10.13 Each fact is stored once: `Autolink { form, text }` with `destination()`, `CodeInline { value }`, and `CharacterReference { reference }` with `value()`. Verified by public-api and validation scenarios and unchanged conformance numbers.
