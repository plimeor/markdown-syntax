# AST → HTML Conformance Bench

A **test-only** measurement harness that answers the question the rest of the
suite cannot: *how correct is the parser?* The derived corpus the rest of the
suite uses only asserts round-trip **stability** (parse → serialize → reparse
yields the same AST). A stably-wrong parse passes. This bench instead measures
**correctness** against CommonMark/GFM expected-HTML oracles.

The bench **owns its conformance test data**. Each `(markdown input → expected
HTML, options, label)` case lives in this project's own byte-counted fixture
files under `tests/fixtures/conformance/<category>/<source>.cases`, one
file per source name (`commonmark`, `gfm_table`, `autolink`, …). The two
subdirectories (`commonmark/`, `gfm/`) are the suite **category** — a
spec-layer grouping label, not a rendering switch. The fixtures were
snapshotted from upstream CommonMark/GFM oracle suites (provenance in
`tests/fixtures/conformance/THIRD-PARTY-LICENSES/`); the snapshot keeps only the
cases the bench actually runs.

The bench renders **one** convention: every case is rendered the same way, with
math always in the GFM form (`data-math-style` wrappers). The only behaviour
keyed on the suite category is the two oracle conventions whose expected HTML
legitimately differs by spec layer — safe-mode raw HTML (the commonmark suite
text-escapes it, the gfm suite emits the `<!-- raw HTML omitted -->`
placeholder) and the task-list checkbox attribute order.

> The crate still ships **no** HTML renderer. The renderer here lives entirely
> under `tests/html_conformance/renderer/` and exists only to turn an AST into
> something comparable with the oracles. It is not part of the public surface.

## How it works

```
conformance/<category>/*.cases ──extractor (reader)──▶ (input, expected_html, category, options, label)
                                              │
   input ──parse_with_options(crate)──▶ AST ──renderer──▶ html
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
- **renderer/** is a faithful CommonMark/GFM reference renderer over the crate's
  AST, covering all 19 `Block` + 30 `Inline` arms. It renders a single
  convention (math always GFM); the only category-divergent behaviour is the two
  oracle conventions noted above (safe-mode raw HTML form, task-list checkbox
  attribute order), selected from the `RenderConfig` the runner builds per tuple.
- **normalizer.rs** is a faithful Rust port of the CommonMark spec test harness'
  `normalize.py` (block-tag-aware whitespace collapse, attribute lowercase+sort,
  `href`/`src` URL canonicalization, entity decode with `<>&"`→entities, `<pre>`
  preserved byte-exact). Both sides pass through the **same** normalizer, so the
  comparison ignores only differences the CommonMark project itself deems
  insignificant and never masks a structural defect (verified by anti-masking
  self-tests: idempotency + deliberate should-fail fixtures).
- **runner.rs** maps each case's option tokens → parse `SyntaxOptions` +
  `RenderConfig` (one unified plan; the former per-suite token vocabularies are
  token-disjoint so both clause blocks apply unconditionally), then runs
  parse→render→normalize→compare. Since the fixtures already exclude cases the
  bench can't fairly run, the runner has no skip path: every case is run and
  compared.
- **report.rs** prints the per-suite / per-file breakdown and dumps every
  failure to `target/html_conformance_failures.txt` for inspection.

## Run it

```sh
cargo test --no-default-features --test html_conformance -- --nocapture
```

`corpus_counts_match` asserts the snapshot-integrity anchor (the
`commonmark/commonmark.cases` source carries exactly **652** cases — the snapshot
of the upstream CommonMark spec corpus). `html_conformance_report` prints the
summary and writes the failure dump. Neither asserts a pass threshold: this is a
measurement, not a pass/fail gate.

## Result (2026-06-20)

The suite holds **2260** runnable cases. (An earlier revision carried a
second math oracle pair — `commonmark/math_flow.cases` +
`commonmark/math_text.cases`, 65 cases — that rendered math in a second
`class="language-math …"` convention. When the bench collapsed to a
single GFM rendering convention, those two fixtures were dropped; flow-math
blocks keep round-trip coverage via the `extensions/math_edges` fixture and GFM
math conformance stays via `gfm/math.cases`. The remaining commonmark-suite
math defects therefore no longer appear in this headline.)

| scope | ran | passed | conformance |
|---|---|---|---|
| **Headline (all)** | 2260 | 2216 | **98.05%** |
| commonmark suite | 1990 | 1956 | **98.29%** |
| └ CommonMark spec (`commonmark.cases`) | 652 | 645 | **98.93%** |
| gfm suite | 270 | 260 | 96.30% |

The **44 residual failures** are the real parser defects this bench exists to
surface (which the round-trip corpus never caught) plus a handful of
test-harness renderer/oracle long-tail items. The 2026-06-20 batches closed 19:
the unmatched-backtick code-span sub-run retry (```` ```foo`` ````), the `ẞ`↔`SS`
reference-label casefold, the `[foo](not a link)` shortcut-reference fallback,
the `> x\n``\n` lazy fence interruption, the `allow_dangerous_protocol`
image-src bypass (test-only renderer), and the `relaxed_autolinks` feature —
cmark-gfm bare-`scheme://` auto-linkification (`smb://`, `irc://`, `rdar://`,
`we://`, scheme-less `://-`, balanced bracket/curly URL extents) with a new
`Constructs.relaxed_autolinks` flag (on in `gfm()`). The bench renderer's
link-href scheme policy is now category-keyed (GFM/cmark-gfm denylist vs
CommonMark/micromark allowlist) since the two suites' oracles genuinely disagree
on unknown schemes. The verified parser-defect inventory below predates the
math-fixture drop, so its math row counts the now-removed commonmark-suite math
cases; the live failure list always regenerates to `target/html_conformance_failures.txt`.

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

## Verified parser-defect inventory

All in `src/parse.rs` unless noted. Counts are failing oracle cases.

| area | cases | core defect(s) |
|---|---|---|
| **math** | 55 | `parse_math_block` is not a fenced-code analogue (no meta/info string, no `>=`-length or EOF close, no indentation handling); `parse_math_inline` is not a code-span analogue (no run-length match, no padding strip, wrongly applies escapes); `MathInline` lacks inline/display + code/dollar discriminants. The math subsystem is effectively a placeholder. |
| **commonmark-core** | 41 | a spread of CommonMark §-level defects surfaced by the 652-example spec set (all real_parser_defect, no renderer/oracle artifacts) |
| **autolink** | 39 | GFM literal autolink applied to URL text **inside** a link → nested `<a><a>`; trailing-punctuation trimming; www/scheme synthesis edge cases |
| **links-refs-images** | 24 | `parse_image` lacks a shortcut-reference branch (`![label]`); `normalize_label` wrongly **unescapes** the label before matching (CommonMark matches the raw label) — companion `serialize.rs` fix required |
| **lists-tasks** | 19 | blank-line list-item continuation keys off the marker column instead of `content_indent`; under-indented continuation handling |
| **tables** | 13 | GFM table parsing (empty/merged cells, delimiter edges) |
| **gfm-ext** | 13 | link-destination scanner treats `\ ` as escaped space; HTML block type-1 close condition; misc extension parsing |
| **code** | 11 | fenced/indented/math code drop a blank **first** content line (`push_line` can't distinguish "no content yet" from "leading blank line") |
| **blocks-breaks** | 8 | `parse_block_quote` lazy-continuation / indented-line classification (setext underline + block interruption inside quotes) |
| **emphasis-strike** | 3 | strikethrough flanking edge |

**High severity (5):** `parse_math_block` (meta string + EOF close); `parse_image`
shortcut-reference branch (§6.4 ex. 573/587/588/589/591); `parse_list` continuation
indent thresholds; `parse_block_quote` lazy-continuation classification;
`normalize_label` raw-label matching (parser + serializer must drop unescaping together).

Full per-failure detail (root cause, spec rule, minimal repro, fix location) is in
the triage record; the live failure list regenerates to
`target/html_conformance_failures.txt` on every run.

## What the number means (and doesn't)

- **98.5% CommonMark** = of the 652 official spec examples, 642 parse to an AST
  that renders to the spec's exact HTML (after the standard normalization). The
  10 misses are concrete, enumerated parser defects — not unknowns.
- It is **not** a claim of feature-completeness. The bench measures the
  implemented surface against real oracles; closing the remaining defects (the
  math subsystem especially) is the work to raise it toward 100%.
- Cases the crate cannot represent (disabled core constructs, MDX-SWC,
  GFM-only render modes) are **excluded and counted**, never silently passed.

## Regenerating the design / triage

The bench was authored from a design contract (node→HTML rules, oracle extraction
grammar, normalization spec) and a triage pass that classified + adversarially
verified every failure. To re-derive after large parser changes, re-run the
bench and re-triage the new `target/html_conformance_failures.txt`.
