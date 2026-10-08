# 0011: Bounded cost on untrusted input

Status: Accepted
Date: 2026-10-08
Supersedes: 0006

## Context

Applications parse files they did not write (editor indexes, sync agents), so
one crafted or accidental file must not stall them or abort the process; a
stack overflow aborts and cannot be caught.

A recursive-descent inline parser that finds each construct's closer by
scanning forward is quadratic in unclosed openers, cubic where a scan step
itself scans, and exponential where a nested label is re-parsed from every
enclosing level. CommonMark's inline algorithm avoids this for delimiters and
brackets by recording them as they are scanned and pairing them when a closer
arrives. Raw-text constructs (code spans, math, autolinks, raw HTML, wiki
links, directive labels and attributes) have no such pairing and still need to
find their end.

## Decision

Parsing, serializing, rendering, and validating take time linear in the input
size, and native stack use is bounded by fixed nesting limits rather than by
how deeply the input nests.

- **Delimiter stack.** Every emphasis-like mark (`*`, `_`, `~~`, `==`) is
  recorded as a run and paired by one delimiter stack, closers in source
  order, with per-key opener floors so no run is searched past twice; mark
  spans are found first among the mark runs. Brackets (`[`, `![`, `^[`) are
  openers on a bracket stack resolved when `]` arrives, and a formed link
  deactivates earlier `[` openers instead of re-parsing a label.
- **Memoized forward scans.** Questions of the form "where is the first closer
  at or after here", asked from many starts in one input, go through
  per-input structures in `src/memo.rs`: position tables, path memos, and
  bracket memos. Each walk is written once as a step function and runs
  unmemoized for one-off callers and memoized in the inline pass, so the two
  differ only in caching. State a pass shares across start positions (literal
  autolink and email runs) is cached in that pass's scan state.
- **Nesting limits.** Block containers nest at most 32 levels. Inline
  containers share 32 levels with the inline passes and open brackets around
  them, and `*` / `_` / `~~` emphasis nests at most 16 levels within one mark,
  bracket label, or directive label. A construct past a limit stays text,
  which bounds recursion in the parser and in everything that walks the tree.

`docs/specs/untrusted-input-cost.md` is the public contract. Each memo is
checked against a plain reference scan at every start of generated inputs
(`src/parse/scan_tests.rs`); `tests/pathological_inputs.rs` pins the time and
stack bounds, and `tests/linear_growth.rs` times combinations of syntax
fragments at `n` and `2n`.

## Considered options

- Forward scans kept linear by memo tables, a verdict memo, and a
  closer-search budget: the budget drops openers once spent and precedence
  follows branch order rather than CommonMark's algorithm.
- One work budget across all scans: realistic inputs with many unclosed
  brackets would exhaust it and silently lose constructs.
- The plain walk first, memo tables only after a budget: faster on ordinary
  paragraphs, but every lookup carries a second code path and a switch point.
- Capping how far a closer may be from its opener: changes meaning for
  legitimately long spans.
- Separate budgets of 32 for marks and brackets and 16 for emphasis within
  each: the deepest allowed tree overflows a 2 MiB stack when serialized in
  an unoptimized build.
- Leaving deep inputs to callers: not available in a `no_std` crate, and a
  caller cannot know the depth before parsing.

## Consequences

- Hand-built ASTs deeper than the limits are not covered: recursion over a
  caller's tree follows the caller's depth.
- Inputs that nest past the limits do not get exact CommonMark output.
- Marks that cross resolve in closing order.
