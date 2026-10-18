# CJK-friendly emphasis

## Why

CommonMark's flanking rules read `这是**重点。**然后` as text: a closing `**`
after CJK punctuation and before a CJK character is not right-flanking, and
Chinese and Japanese put no space there. Settle's notes are written in
Chinese, so bold that ends in `。` or `」` silently fails to form in the
editor and the index.

- Scope: the flanking of every emphasis-like delimiter run (`*`, `_`, `~~`,
  `==`) follows the markdown-cjk-friendly amendments; the syntax stays one
  fixed syntax with no option.
- Risk: a document that parsed one way can parse another way. The amendments
  leave every CommonMark example unchanged (upstream checks this), and source
  bytes never move on write, so the change shows only in the tree.

## What changes

### Inline syntax
- `inline-syntax` › CJK-friendly emphasis flanking: added
- `inline-syntax` › CommonMark inlines: modified — names the flanking
  exception

### Decisions
- `0018` supersedes `0015`: the one normative syntax reads emphasis with the
  CJK-friendly amendments.

### Outside the specs
- Settle (another repository), once this ships in a published release:
  depend on the new version, and rebuild the vendored wasm binding from it.

Specs:
- `inline-syntax` (modified)

## Design

### Reference

The amendments are
<https://github.com/tats-u/markdown-cjk-friendly/blob/5b17173/specification.md>,
with concrete ranges in its `ranges.md` (Unicode 18). The reference
implementation is `micromark-extension-cjk-friendly` 2.0.2 with
`micromark-extension-cjk-friendly-gfm-strikethrough`; its
`classifyPrecedingCharacter` settles readings the text leaves loose.

### Character classes

A new crate-private module holds sorted inclusive ranges, copied from
`ranges.md` at that commit, and three predicates: a CJK character, a
non-emoji general-use variation selector (U+FE00–U+FE0E), and an ideographic
variation selector (U+E0100–U+E01EF). Unicode punctuation stays the existing
table.

Each side of a run is classified once into whitespace (or a bounded side, or
none), CJK, punctuation, CJK punctuation, ideographic variation selector, or
other:

- After the run: the next char alone.
- Before the run: the previous char; when it is a non-emoji general-use
  variation selector, the char before it decides, unless that is whitespace
  or absent, where the selector reads as other (unspecified upstream). `‘`
  `’` `“` `”` followed by U+FE01 read as CJK punctuation, as the reference
  does.

### Flanking

With `punct` the Unicode punctuation set and `cjk` the CJK set:

- left = !after.space && (!after.punct || before.space || before.punct ||
  after.cjk || before.cjk || before.ivs)
- right = !before.space && (!before.punct || after.space || after.punct ||
  before.cjk || after.cjk)

These are the amendments' definitions in the reduced form of the upstream
implementers' tips. `delimiter_flanking_within` computes them for every
caller: `*`, `_`, `~~`, and `==` runs. The `_` open rule's "preceded by
punctuation" reads the previous base char, so a punctuation char followed by a
variation selector counts; its close rule is CommonMark's. The GFM rule that
lets a `*` run beside `~` open or close stays.

### Serializer

The serializer writes a span's delimiters around its content and relies on
read-back tests; nothing in it reads flanking. Read-back can only improve for
CJK text, and the seeded generator's `NOT_READING_BACK` list is checked as it
stands.

### Test scope

- `tests/fixtures/syntax_decisions/cjk_friendly_emphasis.cases`: the inputs of
  upstream `testcases/*.md` (`strong.md`, `korean.md`, `pseudo-emoji.md`,
  `non-bmp.md`, `underscore-cjk-punct.md`, `gfm-strikethrough.md`,
  `gfm-non-cjk.md`), with HTML verified against micromark with GFM and the
  two cjk-friendly extensions, where this crate's renderer agrees elsewhere.
- The conformance bench: no oracle case moves (CommonMark examples are
  unchanged by the amendments); a GFM case that moves leaves the bench by the
  conformance rule.
- Unit tests of the classes at their range edges and of the variation
  selector look-back.

### Decisions
- Always on, no option — because 0015 fixes one syntax with no configuration
  and Settle's notes need it. Turned down: an opt-in parse option.
- Every emphasis-like mark, `~~` and `==` included — because they share one
  flanking function and the reference applies the amendments to
  strikethrough too. Turned down: `*` and `_` only, which would make `~~重点。~~然后`
  fail where `**重点。**然后` forms.
- The ranges copied from the spec's `ranges.md` — because the CJK set excludes
  default-emoji-presentation chars, which needs emoji data the crate's
  generated tables do not carry; the spec publishes the set itself. Turned
  down: generating from the UCD tables the crate already uses (15.1.0).
- Follow the reference for the ambiguous quotes with U+FE01 — the spec text
  defines them but its flanking rule does not name them.

### Risks
- [A non-CJK document pairs differently because a run becomes both-flanking]
  → the amendments only widen flanking beside CJK chars and variation
  selectors; the conformance bench and the goldens show any change, each read
  in review.

## Tasks

### 1. Classes and flanking
- [x] 1.1 Add the CJK and variation-selector ranges and predicates; verified by
  unit tests at the range edges.
  - The module doc names `ranges.md` at 5b17173 and its generator
    (`node --run print-ranges`), so a Unicode update knows where to look.
- [x] 1.2 Classify both sides of a run and compute flanking and the `_` open
  rule by the amendments; verified by `cjk_friendly_emphasis.cases` matching
  micromark's HTML and by unchanged conformance numbers and goldens.
  - The flanking comment cites the spec's definitions and the reduction from
    the upstream implementers' tips.
  - `tests/syntax_decisions.rs`'s header admits cases taken from an
    amendment's own test inputs, labelled with their upstream file.

### 2. Docs
- [x] 2.1 Merge the spec change and write decision 0018 superseding 0015;
  verified by the requirement named in the new tests and `0015` moved to
  `superseded/`.

### 3. Integration
- [x] 3.1 Run CI's commands (fmt, build, tests with `--profile ci --features
  html`, docs, the wasm job); verified by each passing.
