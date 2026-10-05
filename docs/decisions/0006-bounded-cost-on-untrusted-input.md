# 0006: Bounded cost on untrusted input

Status: Accepted
Date: 2026-10-04

## Context

Applications parse files they did not write (editor indexes, sync agents), so
one crafted or accidental file must not stall them or abort the process; a
stack overflow aborts and cannot be caught.

A recursive-descent inline parser that finds each construct's closer by
scanning forward is quadratic in unclosed openers, cubic where a scan step
itself scans, and exponential where a nested label is re-parsed from every
enclosing level. CommonMark's own inline algorithm avoids this for its
delimiter constructs: emphasis runs and brackets are recorded as they are
scanned and paired when a closer arrives, so no closer is searched for. The
constructs whose content is raw text (code spans, math, autolinks, raw HTML,
MDX, wikilinks, directive labels and attributes) have no such pairing and still
need to find their end.

Nesting limits are the boundary that keeps invalid states (unbounded recursion)
out of everything downstream: once the parsed tree is shallow, the serializer,
renderer, validator, and derived traits need no changes. Limits of 32 and 16 sit
far above hand-written Markdown, and the deepest tree they allow fits a 2 MiB
thread stack in an unoptimized build.

## Decision

Parsing, serializing, rendering, and validating take time linear in the input
size (`n log n` for MDX JSX tag matching), and native stack use bounded by fixed
nesting limits rather than by how deeply the input nests.

- **Delimiter stack.** Every emphasis-like mark (`*`, `_`, `~~`, `~`, `^`,
  `++`, `==`, `||`, underline `__`) is recorded as a run and paired by one
  delimiter stack, closers in source order, with per-key opener floors
  (`openers_bottom`) so no run is searched past twice. Mark spans are found
  first among the mark runs, so that properly nested emphasis inside a mark
  pairs as that mark's content reads on its own. Brackets (`[`, `![`, `^[`) are
  openers on a bracket stack resolved when `]` arrives; a formed link
  deactivates earlier `[` openers instead of re-parsing a label to see whether
  it holds a link. Both are linear by construction and need no budget.
- **Memoized forward scans.** The raw-content constructs still ask "where is
  the first closer at or after here" from many start positions in one input,
  as does text escaping. Those questions go through per-input memo structures
  in `src/memo.rs`: sorted position tables for position-intrinsic predicates,
  path memos for memoryless walks, and bracket memos for depth walks. Each walk
  is written once as a step function; one-off callers run it unmemoized
  (`path_walk`, `bracket_walk`) and the inline pass runs it memoized, so the
  two cannot disagree in logic, only in caching.
- **MDX JSX tag index.** A JSX element's closing tag is found by walking tag to
  tag and counting only same-name tags. Every tag in the input is indexed
  once, on the first JSX question: where it ends, and the next same-name tag
  along the walk after it, read from persistent per-name maps built from the
  last tag back (`memo::PersistentMap`). Matching then walks only same-name
  tags through a bracket memo. Block JSX and expression blocks run the same
  walks over their lines joined by `\n`, where a `\` ending a line escapes the
  next line's first byte as the line-by-line scan does.
- **Nesting limits.** Block containers nest at most 32 levels. Within inline
  content every container — emphasis, marks, brackets, directive labels —
  shares 32 levels with the inline passes and open brackets enclosing it, and
  `*` / `_` / `~~` emphasis also nests at most 16 levels within one mark,
  bracket label, or directive label. A
  bracket opener or directive past the limit stays text and a delimiter pair
  past it stays literal. These bound
  recursion in the parser and in everything that walks the parsed tree
  (`to_markdown`, `to_html`, `validate`, `Drop`).

`docs/specs/untrusted-input-cost.md` is the public contract for the limits and
their effect on output. Each memo is checked against a plain reference scan at
every start position of generated inputs (`src/parse/scan_tests.rs`,
`src/serialize/escape_scan_tests.rs`), and `tests/pathological_inputs.rs` pins
the time and stack bounds.

## Considered options

- Forward scans for every construct, kept linear by memo tables, a link-label
  verdict memo, and a closer-search budget for the start-dependent `++` / `==`
  / underline scan: keeps the recursive parser's shape, but the budget drops
  openers once spent, the verdict memo answers inconsistently near the nesting
  limit, and precedence between marks and links follows branch order rather
  than CommonMark's algorithm.
- A single work budget across all scans: simple, but realistic inputs with many
  unclosed brackets (logs, pasted code) would exhaust it and silently lose
  constructs.
- Answering each memoized question by the plain walk first and building memo
  tables only after a linear work budget is spent: ordinary paragraphs parse
  about 10–15% faster, 1–2 ms per 100 KB, but every lookup carries a second
  code path and a switch point that needs its own tests.
- Capping how far a closer may be from its opener: changes meaning for
  legitimately long spans.
- Separate budgets, 32 levels for marks and brackets and 16 for emphasis
  within each: simpler to count, but 16 emphasis levels inside each of 32 marks
  builds a tree that overflows a 2 MiB stack when serialized in an unoptimized
  build.
- Leaving deep inputs to callers (larger thread stacks): not available in a
  `no_std` crate, and a caller cannot know how deep an input nests before
  parsing it.

## Consequences

- Hand-built ASTs deeper than the limits are not covered: recursion over a tree
  the caller built follows the caller's depth.
- Inputs that nest past the limits do not get exact CommonMark output.
- Marks that cross resolve in closing order, so crossing extension marks parse
  differently from a parser that gives a mark its whole span first.
