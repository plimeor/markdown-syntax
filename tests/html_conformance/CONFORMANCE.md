# AST → HTML Conformance Bench

A measurement harness that answers the question the rest of the suite cannot:
*how correct is the parser?* The derived corpus the rest of the suite uses only
asserts round-trip **stability** (parse → serialize → reparse yields the same
AST). A stably-wrong parse passes. This bench instead measures **correctness**
against CommonMark/GFM expected-HTML oracles using the crate's opt-in public HTML
renderer.

The bench **owns its conformance test data**. Each `(markdown input → expected
HTML, options, label)` case lives in this project's own byte-counted fixture
files under `tests/fixtures/conformance/<category>/<source>.cases`, one
file per source name (`commonmark`, `gfm_table`, `autolink`, …). The two
subdirectories (`commonmark/`, `gfm/`) are the suite **category** — a
spec-layer grouping label, not a rendering switch. The fixtures were
snapshotted from upstream CommonMark/GFM oracle suites (provenance in
`tests/fixtures/conformance/THIRD-PARTY-LICENSES/`); the snapshot keeps only the
cases the bench actually runs.

The bench renders **one** convention: every case is rendered with the public
`markdown_syntax::to_html_with_options` API, with math always in the GFM form
(`data-math-style` wrappers). The suite category is retained for oracle
conventions whose expected HTML legitimately differs by spec layer: safe-mode
raw HTML form, task-list checkbox attribute order, and the GFM/cmark-gfm
link-scheme denylist for unknown URI schemes.

## How it works

```
conformance/<category>/*.cases ──extractor (reader)──▶ (input, expected_html, category, options, label)
                                              │
   input ──parse_with_options(crate)──▶ AST ──to_html_with_options(crate)──▶ html
                                              │
              normalize_html(html) == normalize_html(expected_html) ?
```

- **extractor.rs** is a simple reader for the suite fixtures. Each case is a
  byte-counted block (the byte counts let multi-line input/expected containing
  any delimiter-like text round-trip exactly):

  ```text
  --- case <i> options <tok,tok|-> label-bytes <L> input-bytes <I> expected-bytes <E>
  <L bytes of label>
  --- input
  <I bytes of markdown input>
  --- expected
  <E bytes of expected HTML>
  --- end
  ```

  The reader yields `(input, expected_html, category, option_tokens, label,
  source)` for every case; the suite category is derived from the file's
  subdirectory. Because the fixtures hold only runnable cases, the reader has no
  parsing of upstream `.rs`, no format grammar, and no skip-routing flags — that
  complexity was retired with the vendored oracles.
- **src/html/** is the opt-in public renderer over the crate's AST, covering all
  19 `Block` + 30 `Inline` arms. It renders a single convention (math always
  GFM); the category-divergent oracle conventions noted above are selected from
  the `HtmlOptions` the runner builds per tuple.
- **normalizer.rs** is a faithful Rust port of the CommonMark spec test harness'
  `normalize.py` (block-tag-aware whitespace collapse, attribute lowercase+sort,
  `href`/`src` URL canonicalization, entity decode with `<>&"`→entities, `<pre>`
  preserved byte-exact). Both sides pass through the **same** normalizer, so the
  comparison ignores only differences the CommonMark project itself deems
  insignificant and never masks a structural defect (verified by anti-masking
  self-tests: idempotency + deliberate should-fail fixtures).
- **runner.rs** maps each case's option tokens → parse `SyntaxOptions` +
  public `HtmlOptions` (one unified plan; the former per-suite token
  vocabularies are token-disjoint so both clause blocks apply unconditionally),
  then runs parse→render→normalize→compare. Since the fixtures already exclude
  cases the bench can't fairly run, the runner has no skip path: every case is
  run and compared.
- **report.rs** prints the per-suite / per-file breakdown and dumps every
  failure to `target/html_conformance_failures.txt` for inspection.

## Run it

```sh
cargo test --features html --test html_conformance -- --nocapture
```

`corpus_counts_match` asserts the snapshot-integrity anchor (the
`commonmark/commonmark.cases` source carries exactly **652** cases — the snapshot
of the upstream CommonMark spec corpus). `html_conformance_report` prints the
summary and writes the failure dump. Neither asserts a pass threshold: this is a
measurement, not a pass/fail gate.

## Result (2026-06-20)

The suite holds **2265** runnable cases. (An earlier revision carried a
second math oracle pair — `commonmark/math_flow.cases` +
`commonmark/math_text.cases`, 65 cases — that rendered math in a second
`class="language-math …"` convention. When the bench collapsed to a
single GFM rendering convention, those two fixtures were dropped; flow-math
blocks keep round-trip coverage via the `extensions/math_edges` fixture and GFM
math conformance stays via `gfm/math.cases`. The remaining commonmark-suite
math defects therefore no longer appear in this headline.)

| scope | ran | passed | conformance |
|---|---|---|---|
| **Headline (all)** | 2265 | 2265 | **100.00%** |
| commonmark suite | 1990 | 1990 | **100.00%** |
| └ CommonMark spec (`commonmark.cases`) | 652 | 652 | **100.00%** |
| gfm suite | 275 | 275 | **100.00%** |

There are **0 residual failures** and **0 parse errors**. The latest run wrote an
empty failure dump to `target/html_conformance_failures.txt`.

### Snapshot-drop accounting (what the suite intentionally omits)

These upstream cases were dropped when the suite was snapshotted — the bench
cannot fairly run them, so they were never part of the headline denominator.
They are recorded here, not in the fixtures.

| reason | count | meaning |
|---|---|---|
| mdx-needs-swc | 175 | MDX cases needing an external SWC parser hook the crate does not embed |
| parser-unsupported-construct | 174 | upstream test disables a core construct or enables a GFM-only feature the crate intentionally cannot represent (parser is correct; case is unrunnable) |
| divergent-form | 50 | CommonMark `user-content-` footnote shape (renderer implements only the GFM shape) |
| gfm-closure-options | 37 | GFM `html_opts_i(.., |opts| {..})` options set by a Rust closure, not portable |

## What the number means (and doesn't)

- **100% CommonMark** = all 652 official spec examples parse to an AST that
  renders to the spec's exact HTML (after the standard normalization).
- It is **not** a claim of feature-completeness. The bench measures the
  implemented surface against real oracles; unsupported or intentionally omitted
  surfaces remain outside the denominator.
- Cases the crate cannot represent (disabled core constructs, MDX-SWC,
  GFM-only render modes) are **excluded and counted**, never silently passed.

## Regenerating the design / triage

The bench was authored from a design contract (node→HTML rules, oracle extraction
grammar, normalization spec) and a triage pass that classified + adversarially
verified every failure. To re-derive after large parser changes, re-run the
bench and re-triage the new `target/html_conformance_failures.txt`.
