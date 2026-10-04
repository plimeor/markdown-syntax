---
date: 2026-10-04
status: active
---

# Bounded Cost on Untrusted Input

## Decision

Parsing, serializing, rendering, and validating take time linear in the input
size (`n log n` for MDX JSX tag matching), and native stack use bounded by fixed
nesting limits rather than by how
deeply the input nests. Applications parse files they did not write (editor
indexes, sync agents), so one crafted or accidental file must not stall them or
abort the process; a stack overflow aborts and cannot be caught.

- **Memoized forward scans.** Inline parsing and text escaping ask "where is
  the first closer at or after here" from many start positions in one input.
  Those questions go through per-input memo structures in `src/memo.rs`:
  sorted position tables for position-intrinsic predicates, path memos for
  memoryless walks, and bracket memos for depth walks. Each walk is written once
  as a step function; one-off callers run it unmemoized (`path_walk`,
  `bracket_walk`) and the inline pass runs it memoized, so the two cannot
  disagree in logic, only in caching.
- **MDX JSX tag index.** A JSX element's closing tag is found by walking tag to
  tag and counting only same-name tags. Every tag in the input is indexed
  once, on the first JSX question: where it ends, and the next same-name tag
  along the walk after it, read from persistent per-name maps built from the
  last tag back (`memo::PersistentMap`). Matching then walks only same-name
  tags through a bracket memo. Block JSX and expression blocks run the same
  walks over their lines joined by `\n`, where a `\` ending a line escapes the
  next line's first byte as the line-by-line scan does.
- **Nesting limits.** Block containers nest at most 32 levels, recursively
  parsed inline content at most 32 levels, and `*` / `_` / `~~` emphasis at most
  16 levels within one inline span. Nesting past a limit stays literal text.
  These bound recursion in the parser and in everything that walks the parsed
  tree (`to_markdown`, `to_html`, `validate`, `Drop`).
- **Link-label verdict memo.** Whether a link label contains a link needs a full
  parse of the label, and nested labels are asked about from every enclosing
  level. The verdict is memoized per label for one block-level inline parse.
- **Closer-search budget.** The `++` / `==` / underline closer search depends on
  where it starts, so it is not memoized; it may scan 16 bytes per byte of its
  inline span plus 4 KiB, after which further openers in that span stay
  literal.

The README's "Bounded cost on untrusted input" section is the public contract
for the limits and their effect on output.

## Rationale

A recursive-descent inline parser that finds each construct's closer by
scanning forward is quadratic in unclosed openers, cubic where a scan step
itself scans, and exponential where a nested label is re-parsed from every
enclosing level. Memoizing the scans keeps the parser's structure, AST, and
output intact; each memo is checked against a plain reference scan at every
start position of generated inputs (`src/parse/scan_tests.rs`,
`src/serialize/escape_scan_tests.rs`), and `tests/pathological_inputs.rs` pins
the time and stack bounds.

Nesting limits are the boundary that keeps invalid states (unbounded recursion)
out of everything downstream: once the parsed tree is shallow, the serializer,
renderer, validator, and derived traits need no changes. Limits of 32 and 16 sit
far above hand-written Markdown, and the deepest tree they allow fits a 2 MiB
thread stack in an unoptimized build.

## Rejected Alternatives

- Rewriting link parsing as the CommonMark bracket-stack algorithm: linear by
  construction, but it changes parse results in edge cases the current
  conformance work settled, and every bracket construct (images, wikilinks,
  footnotes, directives, inline footnotes) shares the current approach.
- A single work budget across all scans: simple, but realistic inputs with many
  unclosed brackets (logs, pasted code) would exhaust it and silently lose
  constructs. The budget is kept only for the start-dependent closer search,
  where ordinary text never approaches it.
- Answering each question by the plain walk first and building memo tables
  only after a linear work budget is spent: ordinary paragraphs parse about
  10–15% faster and small MDX elements about 2–2.5× faster, 1–2 ms per 100 KB,
  but every lookup carries a second code path and a switch point that needs its
  own tests.
- Capping how far a closer may be from its opener: changes meaning for
  legitimately long spans.
- Keying the label-verdict memo by nesting depth too: exact past the limits,
  but multiplies the work by the depth limit, and only inputs that exceed the
  limits could tell the difference.
- Leaving deep inputs to callers (larger thread stacks): not available in a
  `no_std` crate, and a caller cannot know how deep an input nests before
  parsing it.

## Non-Goals

- Hand-built ASTs deeper than the limits: recursion over a tree the caller
  built follows the caller's depth.
- Exact CommonMark output for inputs that nest past the limits.
