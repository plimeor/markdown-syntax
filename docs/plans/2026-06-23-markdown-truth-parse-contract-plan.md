---
date: 2026-06-23
status: completed
---

# Markdown-Truth Parse Contract - Implementation Plan

Source requirement: `requirements/2026-06-23-markdown-truth-parse-contract.md`.
This plan implements the narrow read-side contract for a Markdown-as-truth host:
parsed top-level spans must stay usable as slices into the original input.

## Clarification Status

Clear enough to execute.

The user-approved lean boundary is:

- A leading UTF-8 BOM (`U+FEFF`) is abnormal Markdown input and is parsed as
  ordinary source content, not stripped.
- An embedded NUL (`U+0000`) is abnormal Markdown input and is parsed as ordinary
  source content, not replaced with `U+FFFD`.
- The implementation does not add an offset mapper, fallback raw block, public
  API, diagnostic, feature flag, or runtime dependency.

## Background / Problem

A downstream Markdown-as-truth host treats the raw `.md` bytes on disk as durable
truth and uses this crate only to derive a read-side block projection. The host
slices each top-level block out of the original source with `Block::span()`.

Current parsing preprocesses the input before collecting source positions:

- `parse_checked` strips one leading `U+FEFF`.
- `parse_checked` replaces each `U+0000` with `U+FFFD`.

Those transformations make CommonMark preprocessing more faithful, but they
break the simpler invariant needed by the host: parsed spans must be absolute
ranges in the original input passed to `parse()`.

## Objective

`parse()` and `SyntaxOptions::parse()` produce top-level block spans that are
safe, ordered, non-overlapping, and expressed in the original input coordinate
space, while preserving the existing default build constraints: `no_std +
alloc`, zero runtime dependencies, empty default features, and MSRV 1.82.

## Scope

- `src/parse.rs`, specifically the input preprocessing in `parse_checked`.
- A focused parser span-contract regression test under `tests/`.
- Existing parser/serializer/html tests as regression evidence.

Authorized behavior boundary:

- Abnormal `U+FEFF` and `U+0000` input is literal source content. The AST may
  contain those scalar values when they appear in source text.
- Normal Markdown behavior is expected to stay unchanged.

## Non-Goals

- No byte-oriented API such as `parse_bytes`.
- No source offset mapper.
- No parser-normalized shadow string.
- No whole-document fallback block.
- No new diagnostics for BOM or NUL.
- No serializer, HTML renderer, style-preserving serializer, single-block
  serializer, identity, history, diff, merge, CRDT, or public API freeze work.
- No broad conformance work beyond the regression checks named here.

## Required Context

Read these before editing:

- `requirements/2026-06-23-markdown-truth-parse-contract.md`.
- `docs/index.md`.
- `src/parse.rs:97-157` for parse entry points and current preprocessing.
- `src/parse.rs:318-370` for `collect_lines` and source offsets.
- `src/ast.rs:25-80` for `Document`, `Block`, and `NodeMeta`.
- `src/span.rs` for `Span` semantics.
- `README.md` source-position section, if public docs need a wording update.

## Planning Iteration

Design Gate result: keep the lean literal-source design.

Two real options were considered:

- **Literal-source parse boundary (chosen).** Delete the preprocessing and parse
  exactly the `&str` passed by the caller. Future maintainers have one invariant:
  every parser span is in original input coordinates. Complexity is lower because
  there is no hidden normalized buffer, no offset mapper, and no fallback mode.
  This avoids APOSD change amplification: span construction sites do not need to
  learn a second coordinate system.
- **Normalized parse buffer plus offset mapper (rejected).** Preserve CommonMark
  preprocessing semantics while translating all spans back to original input
  coordinates. This makes unusual BOM/NUL input more spec-like, but pushes
  hidden mapping knowledge across parser block and inline construction. That
  raises cognitive load and unknown-unknown risk for future span work.

A BOM-only special case was also rejected. It would preserve heading recognition
after a leading BOM, but only by retaining a special coordinate rule for an
abnormal input class the user explicitly does not need optimized.

No `code-lean:` source comment is planned: this is not a hidden ceiling in code.
The simplification is the explicit parse contract recorded in this plan and
covered by tests.

## Proposed Approach

Own the invariant at the parser entry boundary.

`parse_checked` should validate options and then parse the original `input`
directly. Remove the `Cow` import and the preprocessing block that strips
`U+FEFF` and replaces `U+0000`.

After that, `collect_definitions(input, options)` and
`parse_blocks(input, 0, true, options, ...)` already run in the correct source
coordinate space. `Document.meta.span` remains `Span::new(0, input.len())`.

Add one focused public-boundary test helper that inspects only the parsed
document and original source:

- every top-level block has `Some(span)`;
- `span.start <= span.end <= source.len()`;
- `span.start` and `span.end` are UTF-8 char boundaries;
- spans are in source order and do not overlap;
- bytes between top-level spans are Markdown trivia, meaning whitespace only.

The helper should be exercised on a small set of cases that represent the risk:
normal multi-block Markdown, CRLF, leading BOM, embedded NUL, and an unterminated
block construct such as an unclosed fence.

## Work Sequence

### Slice 1 - Add the span-contract regression test

Purpose: lock the desired public behavior before changing parser preprocessing.

Touchpoints:

- New test file, for example `tests/parse_span_contract.rs`.

Forward evidence:

- The new test fails on the current code for at least leading BOM or embedded
  NUL because spans are not expressed against the original input.

Regression evidence:

- The helper is public-boundary oriented and does not assert private parser
  internals.

### Slice 2 - Remove input preprocessing

Purpose: make parser coordinates match the original source by construction.

Touchpoints:

- `src/parse.rs`: remove `Cow` from the `alloc` imports.
- `src/parse.rs`: remove leading BOM stripping and NUL replacement in
  `parse_checked`.

Forward evidence:

- The new span-contract test passes.

Regression evidence:

- `cargo test` passes.

### Slice 3 - Update stale comments only if they become false

Purpose: avoid leaving comments that claim parser-normalized NUL behavior.

Touchpoints:

- `src/html/escape.rs` only if its parser-normalization comment becomes
  inaccurate after Slice 2.
- Public docs only if they already claim BOM/NUL preprocessing.

Forward evidence:

- Comments describe the stable target behavior and do not introduce new contract
  promises.

Regression evidence:

- `cargo fmt --check` and `cargo test` still pass.

## Acceptance, Regression Evidence, And Verification

Acceptance:

- For the focused test cases, every parsed top-level block has a populated,
  valid, in-bounds span.
- Top-level block spans are ordered and non-overlapping.
- Any source between top-level block spans is whitespace trivia.
- Leading `U+FEFF` and embedded `U+0000` parse without panic and without span
  coordinate drift.
- No public API, feature flag, dependency, MSRV, or default feature changes are
  introduced.

Verification commands:

- `cargo fmt --check`
- `cargo test`
- `cargo build`

Optional broader checks if execution time is acceptable:

- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`
- `cargo build --target wasm32-unknown-unknown`
- `cargo test --features html`

Regression surface:

- Normal parser behavior for CommonMark/GFM fixtures.
- Serializer and HTML renderer behavior for normal Markdown.
- Source-position examples in the README doctest.

## Risks And Rabbit Holes

- **BOM heading behavior may change.** `"\u{feff}# title"` can parse as a
  paragraph instead of a heading. This is accepted under the abnormal-input
  boundary. Do not reintroduce BOM-specific offset handling unless a real caller
  needs heading recognition after BOM while also preserving original spans.
- **NUL may reach AST values.** Literal `U+0000` can now appear in text values.
  This is accepted for content preservation. Do not add output-layer
  sanitization in this task.
- **Over-broad span tests can become a parser conformance suite.** Keep the test
  matrix small and risk-focused. Existing fixtures and `cargo test` cover normal
  Markdown breadth.
- **Whitespace trivia classification can overreach.** The helper should only
  classify gaps between top-level block spans, not nested spans. Nested blocks
  are outside this contract.

## Checkpoints

- After Slice 1, confirm the new test demonstrates the current preprocessing
  problem.
- After Slice 2, report any existing tests that changed because literal BOM/NUL
  behavior reached observable AST, serializer, or HTML output.

## Stop Condition

Stop when the focused span-contract test and `cargo test` pass, formatting is
clean, and the only behavior difference is literal handling of abnormal
`U+FEFF` / `U+0000` input. Do not continue into serializer policy, HTML
sanitization, byte-input parsing, offset mapping, or broader Markdown
conformance work.

## Pause Conditions

Pause before expanding scope if:

- preserving CommonMark BOM/NUL preprocessing semantics becomes a requirement;
- a normal Markdown fixture fails for a reason unrelated to BOM or NUL;
- implementation requires a public API, dependency, feature flag, MSRV change,
  or non-default feature behavior change.

## Progress Report Format

For execution, report only:

- slice completed;
- command(s) run and observed result;
- any changed behavior outside abnormal `U+FEFF` / `U+0000` input.
