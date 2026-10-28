# Native baseline: markdown-syntax against pulldown-cmark, comrak, markdown-rs

Measured 2026-10-27 with `tools/bench` (`src/main.rs` documents its usage).
Raw data: `native-baseline.json` in this directory, the per-case best of three
full runs (`--merge`).

## Environment

- CPU: Apple M4 (10 cores: 4 performance, 6 efficiency), macOS (Darwin 27.0.0).
- rustc 1.95.0 (59807616e 2026-04-14).
- markdown-syntax 0.8.0 (path dependency, `html` feature), pulldown-cmark
  0.13.5, comrak 0.59.0 (`default-features = false`), markdown (markdown-rs)
  1.0.0.
- One `[profile.release]` for the whole graph: `opt-level = 3`,
  `lto = "thin"`, `codegen-units = 1`.
- The machine was not idle: another agent ran Node/wasm benchmarks and a
  profiler at the same time (load average 2.3–5.2 during the runs). The bench
  thread asks for the `USER_INTERACTIVE` QoS class so macOS keeps it on a
  performance core, and runs at normal priority: lowering its priority with
  `nice` would let the scheduler move it to an efficiency core, which changes
  timings far more than the background load does.

## Method

- Corpus: `tools/bench/corpus/out/*.md` from `node tools/bench/corpus/gen.mjs`
  — three real documents (CommonMark spec 205 KB, Rust release notes 753 KB,
  this repository's docs 697 KB), eleven ~100 KB synthetic files with one
  construct each (`syn-cjk` is 275 KB), and the `mixed-1k … mixed-1m` scaling
  series.
- Per case: warm up for 0.1 s, choose a batch so one sample takes ≥ 200 µs,
  collect samples for 1 s (≥ 10 samples), and report the median and the
  minimum of time per call. Each call includes dropping what it built.
  Three full runs were made; the baseline keeps, per case, the run with the
  lowest median (its `run spread` column is the largest ÷ smallest median of
  the three).
- Allocations: a counting `#[global_allocator]`, switched off while timing
  and on for one extra call per case. It records `alloc`/`realloc` calls,
  bytes requested (a `realloc` counts its new size), and the peak of live
  heap bytes during the call. Peak heap stands in for peak RSS, which cannot
  be attributed to one operation inside a process that runs them all.
- Throughput (MB/s) is input bytes ÷ median time, with MB = 10⁶ bytes.
- Every run first checks, per corpus, that our unedited write-back equals the
  source, that HTML and canonical writes succeed, and that markdown-rs
  accepts the input; all 18 corpora pass.

### Operations

| key | what one call does |
|---|---|
| `ours.parse` | `parse(src)` → `Document` + diagnostics |
| `ours.editor_parse` | `parse(src)` then `document.leaf_sources()` — the Rust work of the wasm binding's `parse`, without its wire encoding |
| `ours.leaf_sources` | `leaf_sources()` on a parsed document |
| `ours.validate` | `validate()` on a parsed document |
| `ours.definitions` | `Definitions::of(&document)` |
| `ours.writeback` | `to_markdown()` on a parsed, unedited document (with its source) |
| `ours.canonical` | `to_markdown()` on the same tree after `without_source()` (done once, outside the timing) |
| `ours.render` | `to_html()` on a parsed document |
| `ours.html` | `parse(src).document.to_html()` |
| `ours.inline_leaves` | `parse_inline(text, &Definitions::of(doc))` over every leaf's inline source (not measured on `syn-code`, which has no leaves) |
| `ours.inline_largest` | `parse_inline` on the longest leaf only; MB/s over that leaf's text |
| `pulldown.events` | iterate every event of `Parser::new_ext` — **streaming, builds no tree** |
| `pulldown.html` | `push_html` over the parser |
| `comrak.parse` | `parse_document` into a fresh arena |
| `comrak.render` | `format_html` of an already parsed tree |
| `comrak.commonmark` | `format_commonmark` of an already parsed tree |
| `comrak.html` | `markdown_to_html` |
| `mdrs.mdast` | `markdown::to_mdast` |
| `mdrs.html` | `markdown::to_html_with_options` |

### What each parser recognizes

Each peer runs GFM plus whichever of this crate's extensions it offers, so
that a peer does not win by skipping constructs it could parse:

- **pulldown-cmark**: tables, footnotes, strikethrough, task lists, YAML
  metadata blocks, math, GFM (alerts), wiki links. It has no literal
  autolinks (`www.`/bare URLs stay text), and its HTML passes raw HTML through
  unsanitized.
- **comrak**: strikethrough, tables, literal autolinks, task lists, footnotes
  and inline footnotes, `---` front matter, alerts, `$` math, wiki links.
  Raw HTML is omitted in HTML output (its safe default).
- **markdown-rs**: `ParseOptions::gfm()` (literal autolinks, footnotes,
  strikethrough, tables, task lists) plus front matter and math;
  `CompileOptions::gfm()` (safe by default, tag filter). No wiki links,
  alerts, or directives (its directives are MDX only, left off).
- **ours**: the one fixed syntax, always all on — the above plus
  directives, `#` tags, shortcodes, `==` highlight, alerts, wiki links; every
  node carries a source span, blank lines are nodes, and the document keeps
  its source. HTML is safe by default (raw HTML escaped).

So on `syn-directives` and on the tags, shortcodes, and wiki links that the
generator scatters through `mixed-*` and the synthetic files, peers parse
less structure than we do (they see text). Setext headings go the other way:
peers recognize them and we do not; the corpus holds few.

## Gap analysis

**Summary.** Parsing to a tree is 2.4× slower than comrak (geometric mean
over the real and synthetic files; 1.2×–3.6× by file) and 5.8× slower than
pulldown-cmark's tree-less event stream. Markdown → HTML is 3.0× slower than
comrak and 6.8× slower than pulldown, because our render step alone is 5.9×
slower than comrak's. Canonical Markdown writing is 4× *faster* than
comrak's CommonMark formatter. We are 2.7–57× faster than markdown-rs
everywhere (4.3–57× for parsing alone). Nothing of ours grows super-linearly on the scaling series.

1. **Parse: 2.4× comrak, 5.8× pulldown.** Worst files: `syn-paragraphs`
   3.59×, `syn-quotes` 3.56×, `syn-cjk` 3.41×, `syn-html` 3.17×; the real
   documents sit at 2.1–2.8×. Closest: `syn-tables` 1.19× and `syn-emphasis`
   1.49×, where comrak itself is slow. On plain prose (`syn-paragraphs`) we
   parse 57 MB/s against comrak's 207 MB/s and pulldown's 435 MB/s.
2. **The inline phase is most of the parse.** `inline_leaves` (re-parsing
   every leaf's text with `parse_inline`) takes 1.71 ms of
   `syn-paragraphs`' 1.74 ms parse and 1.09 ms of `real-spec`'s 1.89 ms,
   while `leaf_sources` — which runs the block phase again without inlines —
   takes 0.17 ms and 0.71 ms. `inline_leaves` runs at 58–81 MB/s of input on
   the paragraph, quote, directive, and mixed files; comrak parses those same
   files, blocks and inlines, at 118–207 MB/s. This split is inferred from separate
   operations (`parse_inline` standalone, not the inline pass inside
   `parse`), not from a profile.
3. **Allocation volume tracks the gap.** Per parse we make 1.2–4.0× comrak's
   allocation calls (2–4× on every file but `syn-code`) and request
   1.2–7.5× its bytes (3–6× on most): `real-spec` 24.5 k calls and 10.1 MB
   requested for 205 KB of input (≈ 49 bytes per input byte) against
   comrak's 10.3 k and 1.7 MB; `syn-paragraphs` 6.6 MB against 1.4 MB. Peak
   live heap is less lopsided: 0.8–3.5× comrak's (`real-spec` 3.9 MB vs
   1.3 MB, `syn-cjk` 2.3 MB vs 2.6 MB). pulldown makes tens to a few
   thousand allocations per document.
4. **The editor's parse runs the block phase twice.** `editor_parse`
   (`parse` + `leaf_sources`, what the wasm binding's `parse` does before
   encoding) costs 1.30× `parse` (geomean; up to 1.76× on `syn-code`, 1.51×
   on `syn-tables`), because `leaf_sources` re-collects lines and re-reads
   the document's blocks to recover the inline text that `parse` already
   built and dropped.
5. **Unedited write-back costs more than a parse.** `writeback` is 1.21× our
   `parse` (geomean; 1.48× on `real-rust-releases`, 1.52× on `syn-lists`):
   `Document::write` validates, collects definitions, and parses the source
   again to compare each block with it, even when nothing changed. Its
   absolute speed equals comrak's `format_commonmark` (1.01×), which does
   not reuse the source at all.
6. **HTML render is the largest relative gap: 5.9× comrak** (render-only,
   both on a parsed tree), worst on `syn-references` 21×,
   `real-rust-releases` 12.7×, `syn-lists` 8.2×, `syn-html` 7.2×. comrak's
   render makes 15–350 allocations per document (1.3 k on `real-spec`) by
   writing into one growing buffer; ours makes about one or more per node
   (14 k on `real-spec`, 278 k on `real-rust-releases`), consistent with the
   renderer returning a fresh `String` per block and inline and joining them.
   Its definition map also clones every definition and normalizes each
   reference label into a new `String` on lookup. `to_html` runs `validate`
   first, which costs 0–29 % of a parse (4.3 ms on `real-rust-releases`).
7. **Canonical write is fast**: 0.24× comrak's formatter time (geomean),
   140–980 MB/s, with few allocations per output.
8. **Scaling is linear.** Our ns per input byte over `mixed-1k → 1m` rises
   by 1.04–1.19× from 10 KB to 1 MB, less than the peers'
   (pulldown 1.30×, comrak 1.45×), which points at cache effects rather than
   algorithmic growth; `mixed-1k` costs no more per byte than `mixed-10k`,
   so there is no large fixed per-call overhead. markdown-rs is the only
   super-linear one (2.54×).
9. **Absolute numbers that matter for the editor**, natively: parsing a
   ~1.3 KB document takes 21 µs (26 µs with leaf sources); ~100 KB takes
   1.75 ms (2.2 ms with leaf sources); 1 MB takes 19 ms (24 ms). The wasm
   build adds its own overhead on top; that is measured separately.

### Noise

Within a run, median ÷ min had a median of 1.04 and a 90th percentile of
1.10. Across the three runs, the same case's median moved by a median of
4.9 % and up to 29 % (`mixed-1m/ours.leaf_sources`); run 3 was 2–5 % faster
across every operation than runs 1 and 2 as the background load fell. Ratios
between implementations on the same file are steadier than absolute times,
but they are not interleaved measurements: a ratio can carry up to ~10 % of
noise. Treat differences under ~10 % between two runs as noise unless they
repeat.

### Not measured

- Peak RSS: replaced by peak live heap per operation (see Method).
- Incremental editing (re-parse after a keystroke, write-back after an
  edit): no API for it exists to time beyond `parse_inline` on one leaf
  (`inline_largest`) and the full-document operations above.
- The wasm binding and JS wrapper: measured separately by the Node/wasm
  benchmark.
- Interleaved or core-pinned measurement: macOS offers QoS hints, not
  pinning, and the machine was shared.

## Rerun

From `tools/bench/`:

```text
cargo run --release -- --compare native-baseline        # full run, compared with this baseline
cargo run --release -- --filter ours.parse --compare native-baseline
cargo run --release -- --save run1 && cargo run --release -- --save run2 \
  && cargo run --release -- --merge run1,run2 --save after --report after.md
```

## Tables

- CPU: Apple M4
- rustc 1.95.0 (59807616e 2026-04-14)
- markdown-syntax 0.8.0, pulldown-cmark 0.13.5, comrak 0.59.0, markdown 1.0.0
- sample time per case: 1 s; run(s) started 2026-10-27T15:31:39Z, 2026-10-27T15:40:43Z, 2026-10-27T15:47:18Z

Cells are `median time (MB/s)`. A ratio is our median time over the fastest peer's for the same work: above 1 is slower.

### Parse to a tree

pulldown-cmark builds no tree: `pulldown.events` iterates every event of its streaming parser, a lower bound for any tree builder.

| corpus | ours.parse | comrak.parse | mdrs.mdast | pulldown.events (stream) | ours ÷ fastest tree | ours ÷ pulldown |
|---|---:|---:|---:|---:|---:|---:|
| real-spec | 1.89 ms (108) | 896.7 µs (229) | 24.16 ms (8) | 334.2 µs (614) | 2.11× | 5.66× |
| real-rust-releases | 14.78 ms (51) | 6.00 ms (126) | 165.44 ms (5) | 3.56 ms (212) | 2.46× | 4.15× |
| real-docs | 11.37 ms (61) | 4.07 ms (171) | 107.65 ms (6) | 1.63 ms (427) | 2.79× | 6.97× |
| syn-cjk | 2.07 ms (133) | 605.7 µs (454) | 12.27 ms (22) | 310.9 µs (884) | 3.41× | 6.65× |
| syn-code | 289.3 µs (347) | 136.5 µs (736) | 4.39 ms (23) | 48.4 µs (2074) | 2.12× | 5.97× |
| syn-directives | 1.80 ms (56) | 644.2 µs (156) | 8.31 ms (12) | 251.6 µs (398) | 2.80× | 7.16× |
| syn-emphasis | 5.57 ms (18) | 3.74 ms (27) | 34.74 ms (3) | 2.05 ms (49) | 1.49× | 2.72× |
| syn-headings | 1.48 ms (68) | 651.7 µs (153) | 34.28 ms (3) | 242.2 µs (413) | 2.27× | 6.11× |
| syn-html | 1.31 ms (76) | 414.2 µs (242) | 5.91 ms (17) | 192.1 µs (522) | 3.17× | 6.83× |
| syn-lists | 1.71 ms (59) | 714.3 µs (142) | 8.32 ms (12) | 262.9 µs (385) | 2.39× | 6.50× |
| syn-paragraphs | 1.74 ms (57) | 484.0 µs (207) | 6.96 ms (14) | 229.8 µs (435) | 3.59× | 7.57× |
| syn-quotes | 1.79 ms (56) | 502.2 µs (201) | 7.53 ms (13) | 241.3 µs (417) | 3.56× | 7.42× |
| syn-references | 2.13 ms (47) | 1.04 ms (96) | 13.38 ms (7) | 465.1 µs (216) | 2.05× | 4.59× |
| syn-tables | 2.06 ms (49) | 1.72 ms (58) | 117.28 ms (1) | 376.6 µs (266) | 1.19× | 5.46× |
| mixed-1k | 20.5 µs (66) | 8.4 µs (160) | 88.7 µs (15) | 3.0 µs (445) | 2.44× | 6.79× |
| mixed-10k | 195.3 µs (61) | 69.6 µs (172) | 825.0 µs (14) | 26.7 µs (448) | 2.81× | 7.32× |
| mixed-100k | 1.75 ms (58) | 748.8 µs (136) | 9.13 ms (11) | 264.7 µs (383) | 2.33× | 6.59× |
| mixed-1m | 18.77 ms (54) | 8.64 ms (118) | 179.55 ms (6) | 2.97 ms (345) | 2.17× | 6.33× |
| **geomean** (real + syn) | | | | | **2.42×** | **5.80×** |

### Markdown to HTML (parse + render)

| corpus | ours.html | pulldown.html | comrak.html | mdrs.html | ours ÷ fastest | ours ÷ comrak |
|---|---:|---:|---:|---:|---:|---:|
| real-spec | 2.77 ms (74) | 472.1 µs (434) | 1.20 ms (171) | 24.51 ms (8) | 5.87× | 2.31× |
| real-rust-releases | 24.63 ms (31) | 4.12 ms (183) | 7.43 ms (101) | 169.92 ms (4) | 5.98× | 3.31× |
| real-docs | 17.88 ms (39) | 2.14 ms (326) | 5.20 ms (134) | 106.59 ms (7) | 8.37× | 3.43× |
| syn-cjk | 2.88 ms (95) | 444.5 µs (618) | 813.5 µs (338) | 12.36 ms (22) | 6.49× | 3.54× |
| syn-code | 473.0 µs (212) | 82.4 µs (1220) | 163.4 µs (615) | 4.59 ms (22) | 5.74× | 2.89× |
| syn-directives | 2.64 ms (38) | 329.8 µs (304) | 810.5 µs (124) | 7.89 ms (13) | 8.01× | 3.26× |
| syn-emphasis | 8.54 ms (12) | 2.17 ms (46) | 4.87 ms (21) | 33.12 ms (3) | 3.94× | 1.75× |
| syn-headings | 2.26 ms (44) | 331.3 µs (302) | 856.7 µs (117) | 34.61 ms (3) | 6.81× | 2.64× |
| syn-html | 1.94 ms (52) | 233.9 µs (429) | 507.4 µs (198) | 5.88 ms (17) | 8.30× | 3.83× |
| syn-lists | 2.88 ms (35) | 329.6 µs (307) | 869.2 µs (116) | 8.06 ms (13) | 8.73× | 3.31× |
| syn-paragraphs | 2.45 ms (41) | 300.4 µs (333) | 627.1 µs (159) | 6.94 ms (14) | 8.15× | 3.90× |
| syn-quotes | 2.60 ms (39) | 320.6 µs (314) | 653.2 µs (154) | 7.44 ms (14) | 8.10× | 3.97× |
| syn-references | 3.16 ms (32) | 544.2 µs (184) | 1.10 ms (91) | 13.43 ms (7) | 5.81× | 2.88× |
| syn-tables | 3.71 ms (27) | 491.4 µs (204) | 2.10 ms (48) | 116.66 ms (1) | 7.55× | 1.77× |
| mixed-1k | 32.4 µs (41) | 3.9 µs (344) | 10.7 µs (125) | 88.5 µs (15) | 8.30× | 3.02× |
| mixed-10k | 305.8 µs (39) | 33.1 µs (361) | 83.1 µs (144) | 811.0 µs (15) | 9.24× | 3.68× |
| mixed-100k | 2.81 ms (36) | 340.5 µs (298) | 920.3 µs (110) | 9.02 ms (11) | 8.26× | 3.06× |
| mixed-1m | 30.09 ms (34) | 3.82 ms (268) | 10.31 ms (99) | 178.01 ms (6) | 7.87× | 2.92× |
| **geomean** (real + syn) | | | | | **6.84×** | **2.97×** |

### HTML from a parsed tree (render only)

| corpus | ours.render | comrak.render | ours ÷ comrak |
|---|---:|---:|---:|
| real-spec | 757.4 µs (271) | 258.5 µs (793) | 2.93× |
| real-rust-releases | 10.58 ms (71) | 832.7 µs (905) | 12.71× |
| real-docs | 6.39 ms (109) | 974.5 µs (715) | 6.56× |
| syn-cjk | 787.8 µs (349) | 200.4 µs (1372) | 3.93× |
| syn-code | 184.5 µs (545) | 27.0 µs (3716) | 6.82× |
| syn-directives | 800.0 µs (125) | 148.1 µs (676) | 5.40× |
| syn-emphasis | 2.56 ms (39) | 949.0 µs (105) | 2.70× |
| syn-headings | 765.3 µs (131) | 196.8 µs (508) | 3.89× |
| syn-html | 596.6 µs (168) | 82.8 µs (1210) | 7.20× |
| syn-lists | 1.10 ms (92) | 134.7 µs (752) | 8.20× |
| syn-paragraphs | 665.2 µs (150) | 124.3 µs (805) | 5.35× |
| syn-quotes | 728.4 µs (138) | 132.4 µs (761) | 5.50× |
| syn-references | 984.3 µs (102) | 46.8 µs (2143) | 21.04× |
| syn-tables | 1.57 ms (64) | 343.9 µs (291) | 4.58× |
| mixed-1k | 11.8 µs (114) | 2.0 µs (681) | 5.97× |
| mixed-10k | 106.3 µs (112) | 12.7 µs (944) | 8.40× |
| mixed-100k | 1.01 ms (100) | 142.8 µs (711) | 7.09× |
| mixed-1m | 10.91 ms (94) | 1.58 ms (646) | 6.89× |
| **geomean** (real + syn) | | | **5.90×** |

### Markdown from a parsed tree

`ours.writeback` writes a parsed, unedited document with its source (the editor's save path); `ours.canonical` writes the same tree `without_source()`; `comrak.commonmark` formats its tree as CommonMark, which never reuses the source.

| corpus | ours.writeback | ours.canonical | comrak.commonmark | writeback ÷ comrak | canonical ÷ comrak |
|---|---:|---:|---:|---:|---:|
| real-spec | 2.23 ms (92) | 431.5 µs (475) | 3.09 ms (66) | 0.72× | 0.14× |
| real-rust-releases | 21.91 ms (34) | 8.33 ms (90) | 12.14 ms (62) | 1.81× | 0.69× |
| real-docs | 15.14 ms (46) | 4.03 ms (173) | 13.81 ms (50) | 1.10× | 0.29× |
| syn-cjk | 2.27 ms (121) | 404.5 µs (680) | 2.15 ms (128) | 1.06× | 0.19× |
| syn-code | 314.9 µs (319) | 102.8 µs (977) | 1.79 ms (56) | 0.18× | 0.06× |
| syn-directives | 2.00 ms (50) | 389.9 µs (257) | 2.06 ms (49) | 0.97× | 0.19× |
| syn-emphasis | 6.49 ms (15) | 664.4 µs (151) | 2.83 ms (35) | 2.30× | 0.24× |
| syn-headings | 1.73 ms (58) | 525.9 µs (190) | 1.93 ms (52) | 0.90× | 0.27× |
| syn-html | 1.51 ms (67) | 272.7 µs (368) | 1.90 ms (53) | 0.79× | 0.14× |
| syn-lists | 2.60 ms (39) | 849.2 µs (119) | 2.04 ms (50) | 1.27× | 0.42× |
| syn-paragraphs | 1.91 ms (52) | 281.0 µs (356) | 2.03 ms (49) | 0.94× | 0.14× |
| syn-quotes | 1.98 ms (51) | 463.8 µs (217) | 2.00 ms (50) | 0.99× | 0.23× |
| syn-references | 2.82 ms (36) | 829.0 µs (121) | 1.43 ms (70) | 1.98× | 0.58× |
| syn-tables | 2.57 ms (39) | 910.3 µs (110) | 2.19 ms (46) | 1.17× | 0.42× |
| mixed-1k | 27.9 µs (48) | 8.6 µs (157) | 28.0 µs (48) | 1.00× | 0.31× |
| mixed-10k | 276.7 µs (43) | 83.2 µs (144) | 233.5 µs (51) | 1.18× | 0.36× |
| mixed-100k | 2.37 ms (43) | 693.3 µs (146) | 2.03 ms (50) | 1.16× | 0.34× |
| mixed-1m | 26.05 ms (39) | 7.41 ms (138) | 20.07 ms (51) | 1.30× | 0.37× |
| **geomean** (real + syn) | | | | **1.01×** | **0.24×** |

### Our sub-costs

`editor_parse` is `parse` then `leaf_sources`, the Rust work of the wasm binding's `parse` before encoding. `leaf_sources`, `validate`, `definitions`, and `inline_leaves` run on a parsed document; `inline_leaves` calls `parse_inline` on every leaf's inline source; `inline_largest` on the longest one only, with MB/s over that leaf's text.

| corpus | parse | editor_parse | leaf_sources | validate | definitions | inline_leaves | inline_largest | editor_parse ÷ parse | writeback ÷ parse |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| real-spec | 1.89 ms (108) | 2.60 ms (79) | 706.2 µs (290) | 116.1 µs (1767) | 1.5 µs (137951) | 1.09 ms (187) | 7.2 µs (124) | 1.37× | 1.18× |
| real-rust-releases | 14.78 ms (51) | 21.34 ms (35) | 6.43 ms (117) | 4.30 ms (175) | 157.6 µs (4779) | 8.03 ms (94) | 15.4 µs (84) | 1.44× | 1.48× |
| real-docs | 11.37 ms (61) | 14.67 ms (47) | 2.70 ms (258) | 1.55 ms (450) | 11.3 µs (61682) | 8.01 ms (87) | 20.9 µs (126) | 1.29× | 1.33× |
| syn-cjk | 2.07 ms (133) | 2.40 ms (114) | 318.8 µs (862) | 101.3 µs (2714) | 490 ns (561288) | 1.83 ms (150) | 5.7 µs (117) | 1.16× | 1.10× |
| syn-code | 289.3 µs (347) | 508.2 µs (198) | 222.1 µs (452) | 660 ns (152363) | 133 ns (753265) | — | — | 1.76× | 1.09× |
| syn-directives | 1.80 ms (56) | 2.12 ms (47) | 307.1 µs (326) | 89.0 µs (1126) | 646 ns (155085) | 1.61 ms (62) | 10.1 µs (71) | 1.18× | 1.11× |
| syn-emphasis | 5.57 ms (18) | 5.71 ms (18) | 232.4 µs (430) | 225.9 µs (443) | 475 ns (210814) | 5.39 ms (19) | 11.2 µs (22) | 1.03× | 1.17× |
| syn-headings | 1.48 ms (68) | 2.13 ms (47) | 638.1 µs (157) | 79.4 µs (1260) | 2.5 µs (40513) | 1.02 ms (98) | 344 ns (116) | 1.44× | 1.17× |
| syn-html | 1.31 ms (76) | 1.72 ms (58) | 390.2 µs (257) | 75.0 µs (1338) | 858 ns (116913) | 933.3 µs (107) | 9.5 µs (68) | 1.31× | 1.15× |
| syn-lists | 1.71 ms (59) | 2.15 ms (47) | 424.9 µs (238) | 276.4 µs (366) | 2.4 µs (43068) | 1.43 ms (71) | 2.8 µs (85) | 1.26× | 1.52× |
| syn-paragraphs | 1.74 ms (57) | 1.92 ms (52) | 171.4 µs (583) | 87.9 µs (1138) | 203 ns (493846) | 1.71 ms (58) | 9.4 µs (78) | 1.10× | 1.10× |
| syn-quotes | 1.79 ms (56) | 2.06 ms (49) | 241.5 µs (417) | 91.6 µs (1099) | 799 ns (126013) | 1.68 ms (60) | 11.1 µs (60) | 1.15× | 1.11× |
| syn-references | 2.13 ms (47) | 2.87 ms (35) | 698.8 µs (143) | 478.0 µs (210) | 43.0 µs (2330) | 1.40 ms (72) | 7.5 µs (48) | 1.34× | 1.32× |
| syn-tables | 2.06 ms (49) | 3.10 ms (32) | 1.01 ms (99) | 147.6 µs (678) | 75 ns (1335664) | 1.66 ms (60) | 657 ns (65) | 1.51× | 1.25× |
| mixed-1k | 20.5 µs (66) | 26.1 µs (51) | 5.4 µs (251) | 2.8 µs (489) | 21 ns (63256) | 16.6 µs (81) | 3.4 µs (57) | 1.27× | 1.36× |
| mixed-10k | 195.3 µs (61) | 243.6 µs (49) | 47.4 µs (252) | 25.9 µs (461) | 186 ns (64387) | 161.0 µs (74) | 9.2 µs (58) | 1.25× | 1.42× |
| mixed-100k | 1.75 ms (58) | 2.19 ms (46) | 437.0 µs (232) | 218.6 µs (464) | 2.3 µs (44636) | 1.47 ms (69) | 12.8 µs (58) | 1.26× | 1.36× |
| mixed-1m | 18.77 ms (54) | 24.48 ms (42) | 4.81 ms (213) | 2.42 ms (422) | 35.0 µs (29229) | 14.79 ms (69) | 11.1 µs (67) | 1.30× | 1.39× |
| **geomean** (real + syn) | | | | | | | | **1.30×** | **1.21×** |

### Allocations per call

Cells are `allocation calls / MB requested / MB peak live`. `realloc` counts as a call requesting its new size.

| corpus | ours.parse | comrak.parse | mdrs.mdast | pulldown.events | ours.editor_parse | ours.writeback | ours.canonical | ours.html | pulldown.html | comrak.html |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| real-spec | 24468 / 10.1 / 3.9 | 10270 / 1.7 / 1.3 | 71778 / 73.9 / 19.4 | 71 / 1.0 / 0.7 | 33853 / 17.0 / 4.5 | 28066 / 10.7 / 3.9 | 8303 / 1.4 / 0.4 | 38407 / 11.7 / 3.9 | 92 / 1.6 / 0.9 | 11583 / 2.6 / 1.6 |
| real-rust-releases | 250021 / 57.9 / 18.4 | 80446 / 12.5 / 9.3 | 625141 / 421.5 / 94.4 | 1459 / 10.5 / 5.8 | 371003 / 102.3 / 28.6 | 440325 / 68.9 / 18.6 | 225252 / 11.8 / 0.9 | 528353 / 77.4 / 19.1 | 1477 / 12.6 / 6.8 | 80532 / 14.6 / 9.7 |
| real-docs | 159798 / 44.9 / 15.3 | 41148 / 10.3 / 9.3 | 302988 / 257.2 / 59.5 | 231 / 7.7 / 4.4 | 205899 / 72.4 / 22.8 | 214003 / 50.2 / 15.3 | 83412 / 8.0 / 0.8 | 278600 / 57.3 / 16.0 | 248 / 10.9 / 6.0 | 41193 / 13.7 / 10.5 |
| syn-cjk | 33507 / 7.8 / 2.3 | 8380 / 2.7 / 2.6 | 65102 / 34.8 / 9.3 | 16 / 1.3 / 0.9 | 37514 / 10.6 / 3.0 | 33515 / 8.1 / 2.4 | 5334 / 1.6 / 0.5 | 48868 / 10.2 / 3.0 | 29 / 2.2 / 1.3 | 8403 / 3.6 / 2.9 |
| syn-code | 1689 / 1.5 / 0.7 | 1427 / 0.2 / 0.2 | 12540 / 14.3 / 7.4 | 10 / 0.2 / 0.2 | 3157 / 2.7 / 0.9 | 1898 / 1.6 / 0.7 | 1135 / 0.5 / 0.2 | 2651 / 2.0 / 0.7 | 21 / 0.6 / 0.4 | 1771 / 0.7 / 0.3 |
| syn-directives | 26036 / 7.7 / 1.8 | 9906 / 2.4 / 2.2 | 48783 / 36.0 / 8.3 | 338 / 1.2 / 0.7 | 33068 / 10.7 / 2.4 | 27372 / 7.9 / 1.8 | 6574 / 1.2 / 0.1 | 44846 / 9.5 / 2.0 | 353 / 1.6 / 0.9 | 9930 / 2.8 / 2.3 |
| syn-emphasis | 152052 / 64.0 / 14.7 | 75585 / 18.0 / 17.8 | 202413 / 205.0 / 54.6 | 9 / 9.5 / 4.8 | 155809 / 66.5 / 15.1 | 152060 / 64.1 / 14.7 | 11350 / 0.5 / 0.1 | 238252 / 66.8 / 15.2 | 26 / 10.5 / 5.3 | 75612 / 19.1 / 17.8 |
| syn-headings | 24138 / 8.3 / 4.4 | 7368 / 2.6 / 2.1 | 85640 / 65.4 / 32.2 | 5 / 1.1 / 0.6 | 34846 / 14.8 / 5.3 | 24146 / 8.6 / 4.4 | 14227 / 0.5 / 0.1 | 40398 / 9.1 / 4.4 | 19 / 1.3 / 0.7 | 7394 / 3.0 / 2.2 |
| syn-html | 18042 / 6.4 / 1.9 | 6063 / 1.4 / 1.2 | 33954 / 29.4 / 7.6 | 206 / 1.2 / 0.7 | 24382 / 9.8 / 2.6 | 19525 / 6.5 / 1.9 | 4667 / 0.8 / 0.2 | 30564 / 7.6 / 1.9 | 218 / 1.5 / 0.8 | 6086 / 1.6 / 1.2 |
| syn-lists | 28489 / 7.2 / 2.3 | 9767 / 2.4 / 2.2 | 55794 / 38.5 / 9.2 | 327 / 1.2 / 0.7 | 36546 / 11.5 / 3.7 | 43122 / 8.9 / 2.3 | 13429 / 3.4 / 0.2 | 51842 / 12.1 / 2.6 | 342 / 1.6 / 0.9 | 9789 / 2.7 / 2.3 |
| syn-paragraphs | 22050 / 6.6 / 1.5 | 8095 / 1.4 / 1.2 | 44886 / 32.6 / 7.7 | 364 / 1.3 / 0.7 | 24432 / 8.3 / 1.8 | 22562 / 6.7 / 1.5 | 3718 / 0.6 / 0.2 | 36576 / 7.7 / 1.6 | 378 / 1.6 / 0.9 | 8118 / 1.8 / 1.3 |
| syn-quotes | 23680 / 7.0 / 1.7 | 8205 / 2.4 / 2.2 | 46359 / 40.1 / 9.5 | 356 / 1.3 / 0.7 | 27742 / 9.2 / 2.3 | 24858 / 7.1 / 1.7 | 5576 / 1.5 / 0.1 | 39811 / 9.2 / 1.9 | 677 / 1.7 / 0.9 | 8228 / 2.7 / 2.3 |
| syn-references | 54941 / 6.9 / 2.3 | 22944 / 3.5 / 2.2 | 97972 / 54.7 / 12.1 | 17 / 1.3 / 0.7 | 74381 / 9.7 / 2.6 | 78477 / 7.6 / 2.3 | 30640 / 1.1 / 0.2 | 88643 / 8.1 / 2.3 | 32 / 1.6 / 0.9 | 22964 / 3.7 / 2.2 |
| syn-tables | 47572 / 6.8 / 3.8 | 21562 / 5.5 / 4.1 | 171623 / 109.9 / 36.6 | 437 / 2.4 / 1.3 | 73879 / 12.4 / 6.1 | 52896 / 7.5 / 3.8 | 23169 / 1.5 / 0.2 | 85561 / 9.8 / 4.3 | 453 / 3.0 / 1.6 | 21585 / 6.0 / 4.3 |
| mixed-1k | 387 / 0.1 / 0.0 | 161 / 0.0 / 0.0 | 786 / 0.5 / 0.1 | 11 / 0.0 / 0.0 | 509 / 0.2 / 0.1 | 535 / 0.1 / 0.0 | 176 / 0.0 / 0.0 | 691 / 0.1 / 0.0 | 18 / 0.0 / 0.0 | 177 / 0.0 / 0.0 |
| mixed-10k | 3311 / 0.9 / 0.3 | 1060 / 0.3 / 0.3 | 5786 / 4.5 / 1.0 | 41 / 0.1 / 0.1 | 4223 / 1.3 / 0.4 | 4801 / 1.0 / 0.3 | 1362 / 0.4 / 0.0 | 5780 / 1.4 / 0.3 | 53 / 0.2 / 0.1 | 1075 / 0.3 / 0.3 |
| mixed-100k | 28620 / 7.4 / 2.4 | 10446 / 2.5 / 2.2 | 62160 / 53.1 / 12.5 | 319 / 1.3 / 0.7 | 36975 / 11.4 / 3.6 | 38303 / 8.5 / 2.4 | 12055 / 2.3 / 0.2 | 50684 / 10.8 / 2.5 | 361 / 1.6 / 0.9 | 10482 / 2.9 / 2.3 |
| mixed-1m | 278522 / 72.9 / 23.5 | 101399 / 21.2 / 17.8 | 594512 / 484.6 / 108.1 | 2790 / 12.6 / 7.0 | 360654 / 115.6 / 35.8 | 378481 / 83.9 / 23.5 | 118302 / 22.8 / 1.4 | 492243 / 107.7 / 24.1 | 2981 / 16.8 / 9.1 | 101563 / 25.6 / 19.3 |

### Scaling over mixed-1k … mixed-1m (ns per input byte, median)

| op | mixed-1k | mixed-10k | mixed-100k | mixed-1m | 1m ÷ 10k |
|---|---:|---:|---:|---:|---:|
| ours.parse | 15.27 | 16.33 | 17.20 | 18.35 | 1.12 |
| ours.editor_parse | 19.44 | 20.38 | 21.60 | 23.94 | 1.17 |
| ours.leaf_sources | 3.99 | 3.97 | 4.31 | 4.70 | 1.19 |
| ours.writeback | 20.76 | 23.14 | 23.35 | 25.47 | 1.10 |
| ours.canonical | 6.36 | 6.96 | 6.83 | 7.25 | 1.04 |
| ours.html | 24.14 | 25.58 | 27.72 | 29.42 | 1.15 |
| ours.inline_leaves | 12.38 | 13.47 | 14.45 | 14.46 | 1.07 |
| pulldown.events | 2.25 | 2.23 | 2.61 | 2.90 | 1.30 |
| comrak.parse | 6.27 | 5.82 | 7.38 | 8.45 | 1.45 |
| comrak.html | 7.99 | 6.95 | 9.07 | 10.08 | 1.45 |
| mdrs.mdast | 66.01 | 69.00 | 90.00 | 175.54 | 2.54 |

### Noise

Median ÷ min within a run, over 340 cases: median 1.040, 90th percentile 1.104, max 1.145 (real-docs/ours.inline_largest).
Largest ÷ smallest median across the merged runs: median 1.050, 90th percentile 1.108, max 1.286 (mixed-1m/ours.leaf_sources).
Cases above 1.10 across runs: mixed-1m/ours.leaf_sources (1.29), mixed-1m/ours.render (1.20), mixed-1m/ours.parse (1.19), real-docs/ours.leaf_sources (1.19), syn-quotes/mdrs.mdast (1.18), real-rust-releases/ours.leaf_sources (1.17), mixed-1m/ours.validate (1.17), mixed-1m/ours.writeback (1.17), mixed-1m/ours.editor_parse (1.16), real-rust-releases/ours.render (1.16), mixed-1m/ours.html (1.16), mixed-1m/mdrs.html (1.16), mixed-1m/mdrs.mdast (1.15), mixed-100k/mdrs.html (1.14), real-rust-releases/ours.html (1.14), real-docs/ours.render (1.14), syn-emphasis/comrak.html (1.13), syn-tables/ours.writeback (1.13), real-rust-releases/ours.writeback (1.13), syn-lists/ours.html (1.13), syn-lists/comrak.parse (1.13), syn-references/ours.html (1.12), syn-tables/mdrs.mdast (1.12), syn-quotes/mdrs.html (1.12), syn-references/ours.parse (1.12), syn-lists/ours.editor_parse (1.12), real-spec/ours.render (1.12), syn-tables/ours.html (1.12), syn-lists/ours.writeback (1.12), syn-lists/ours.definitions (1.12), real-rust-releases/ours.validate (1.11), real-rust-releases/ours.editor_parse (1.11), syn-emphasis/ours.writeback (1.11), syn-lists/ours.parse (1.11), mixed-100k/mdrs.mdast (1.11), real-rust-releases/mdrs.html (1.11), syn-quotes/comrak.html (1.11), syn-lists/comrak.html (1.11), mixed-1m/ours.definitions (1.10), syn-html/mdrs.mdast (1.10), syn-headings/ours.writeback (1.10), mixed-1m/comrak.render (1.10), syn-references/mdrs.mdast (1.10), syn-references/mdrs.html (1.10), syn-lists/ours.render (1.10), syn-directives/ours.writeback (1.10).

### Every case

| case | bytes | median | min | p90 | MB/s | samples × batch | allocs | MB req | MB peak | run spread |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| real-spec/comrak.commonmark | 205025 | 3.09 ms | 2.94 ms | 3.17 ms | 66.4 | 325 × 1 | 158380 | 0.75 | 0.29 | 1.05 |
| real-spec/comrak.html | 205025 | 1.20 ms | 1.09 ms | 1.25 ms | 171.1 | 834 × 1 | 11583 | 2.55 | 1.60 | 1.07 |
| real-spec/comrak.parse | 205025 | 896.7 µs | 816.9 µs | 922.2 µs | 228.7 | 1112 × 1 | 10270 | 1.69 | 1.31 | 1.08 |
| real-spec/comrak.render | 205025 | 258.5 µs | 232.5 µs | 265.0 µs | 793.3 | 3868 × 1 | 1313 | 0.86 | 0.37 | 1.05 |
| real-spec/mdrs.html | 205025 | 24.51 ms | 23.41 ms | 25.23 ms | 8.4 | 41 × 1 | 78615 | 71.55 | 19.36 | 1.08 |
| real-spec/mdrs.mdast | 205025 | 24.16 ms | 23.55 ms | 24.61 ms | 8.5 | 42 × 1 | 71778 | 73.90 | 19.36 | 1.08 |
| real-spec/ours.canonical | 205025 | 431.5 µs | 393.6 µs | 446.1 µs | 475.2 | 2312 × 1 | 8303 | 1.41 | 0.37 | 1.06 |
| real-spec/ours.definitions | 205025 | 1.5 µs | 1.3 µs | 1.6 µs | 137951.3 | 4950 × 136 | 0 | 0.00 | 0.00 | 1.05 |
| real-spec/ours.editor_parse | 205025 | 2.60 ms | 2.44 ms | 2.68 ms | 78.8 | 386 × 1 | 33853 | 17.00 | 4.45 | 1.08 |
| real-spec/ours.html | 205025 | 2.77 ms | 2.60 ms | 2.83 ms | 74.0 | 362 × 1 | 38407 | 11.68 | 3.89 | 1.10 |
| real-spec/ours.inline_largest | 891 | 7.2 µs | 6.4 µs | 7.6 µs | 124.1 | 4790 × 29 | 88 | 0.02 | 0.01 | 1.07 |
| real-spec/ours.inline_leaves | 205025 | 1.09 ms | 1.01 ms | 1.14 ms | 187.3 | 915 × 1 | 17408 | 3.03 | 0.07 | 1.07 |
| real-spec/ours.leaf_sources | 205025 | 706.2 µs | 635.6 µs | 752.2 µs | 290.3 | 1405 × 1 | 9385 | 6.95 | 2.56 | 1.04 |
| real-spec/ours.parse | 205025 | 1.89 ms | 1.74 ms | 1.97 ms | 108.4 | 528 × 1 | 24468 | 10.05 | 3.89 | 1.05 |
| real-spec/ours.render | 205025 | 757.4 µs | 686.2 µs | 787.1 µs | 270.7 | 1320 × 1 | 13939 | 1.63 | 0.61 | 1.12 |
| real-spec/ours.validate | 205025 | 116.1 µs | 105.9 µs | 122.0 µs | 1766.5 | 4314 × 2 | 810 | 0.08 | 0.02 | 1.05 |
| real-spec/ours.writeback | 205025 | 2.23 ms | 2.08 ms | 2.32 ms | 91.8 | 447 × 1 | 28066 | 10.66 | 3.89 | 1.10 |
| real-spec/pulldown.events | 205025 | 334.2 µs | 301.5 µs | 348.2 µs | 613.5 | 2982 × 1 | 71 | 1.03 | 0.67 | 1.02 |
| real-spec/pulldown.html | 205025 | 472.1 µs | 432.1 µs | 492.8 µs | 434.3 | 2111 × 1 | 92 | 1.56 | 0.94 | 1.06 |
| real-rust-releases/comrak.commonmark | 753227 | 12.14 ms | 11.74 ms | 12.70 ms | 62.1 | 82 × 1 | 595917 | 2.87 | 1.08 | 1.05 |
| real-rust-releases/comrak.html | 753227 | 7.43 ms | 6.77 ms | 7.86 ms | 101.4 | 133 × 1 | 80532 | 14.60 | 9.69 | 1.02 |
| real-rust-releases/comrak.parse | 753227 | 6.00 ms | 5.69 ms | 6.10 ms | 125.6 | 168 × 1 | 80446 | 12.50 | 9.28 | 1.08 |
| real-rust-releases/comrak.render | 753227 | 832.7 µs | 752.5 µs | 865.6 µs | 904.6 | 1200 × 1 | 86 | 2.10 | 1.05 | 1.06 |
| real-rust-releases/mdrs.html | 753227 | 169.92 ms | 168.33 ms | 172.69 ms | 4.4 | 10 × 1 | 573123 | 399.26 | 94.42 | 1.11 |
| real-rust-releases/mdrs.mdast | 753227 | 165.44 ms | 164.35 ms | 167.14 ms | 4.6 | 10 × 1 | 625141 | 421.55 | 94.42 | 1.08 |
| real-rust-releases/ours.canonical | 753227 | 8.33 ms | 8.05 ms | 8.45 ms | 90.4 | 121 × 1 | 225252 | 11.75 | 0.93 | 1.05 |
| real-rust-releases/ours.definitions | 753227 | 157.6 µs | 141.9 µs | 163.0 µs | 4778.6 | 3181 × 2 | 3068 | 0.23 | 0.13 | 1.04 |
| real-rust-releases/ours.editor_parse | 753227 | 21.34 ms | 20.68 ms | 21.89 ms | 35.3 | 47 × 1 | 371003 | 102.33 | 28.55 | 1.11 |
| real-rust-releases/ours.html | 753227 | 24.63 ms | 23.84 ms | 25.09 ms | 30.6 | 41 × 1 | 528353 | 77.44 | 19.06 | 1.14 |
| real-rust-releases/ours.inline_largest | 1294 | 15.4 µs | 13.7 µs | 16.1 µs | 84.0 | 4978 × 13 | 134 | 0.03 | 0.02 | 1.06 |
| real-rust-releases/ours.inline_leaves | 753227 | 8.03 ms | 7.83 ms | 8.19 ms | 93.8 | 125 × 1 | 164589 | 19.83 | 0.05 | 1.07 |
| real-rust-releases/ours.leaf_sources | 753227 | 6.43 ms | 6.17 ms | 6.66 ms | 117.1 | 155 × 1 | 120982 | 44.46 | 13.19 | 1.17 |
| real-rust-releases/ours.parse | 753227 | 14.78 ms | 13.73 ms | 15.68 ms | 51.0 | 68 × 1 | 250021 | 57.87 | 18.42 | 1.04 |
| real-rust-releases/ours.render | 753227 | 10.58 ms | 10.15 ms | 10.87 ms | 71.2 | 95 × 1 | 278332 | 19.57 | 3.70 | 1.16 |
| real-rust-releases/ours.validate | 753227 | 4.30 ms | 4.08 ms | 4.39 ms | 175.2 | 233 × 1 | 130547 | 3.72 | 0.00 | 1.11 |
| real-rust-releases/ours.writeback | 753227 | 21.91 ms | 21.53 ms | 22.40 ms | 34.4 | 46 × 1 | 440325 | 68.93 | 18.55 | 1.13 |
| real-rust-releases/pulldown.events | 753227 | 3.56 ms | 3.38 ms | 3.65 ms | 211.6 | 281 × 1 | 1459 | 10.54 | 5.77 | 1.08 |
| real-rust-releases/pulldown.html | 753227 | 4.12 ms | 3.87 ms | 4.22 ms | 183.0 | 244 × 1 | 1477 | 12.57 | 6.78 | 1.09 |
| real-docs/comrak.commonmark | 696780 | 13.81 ms | 13.52 ms | 13.93 ms | 50.5 | 73 × 1 | 676948 | 2.91 | 1.11 | 1.07 |
| real-docs/comrak.html | 696780 | 5.20 ms | 4.98 ms | 5.55 ms | 133.9 | 191 × 1 | 41193 | 13.69 | 10.52 | 1.05 |
| real-docs/comrak.parse | 696780 | 4.07 ms | 3.86 ms | 4.18 ms | 171.0 | 246 × 1 | 41148 | 10.35 | 9.29 | 1.07 |
| real-docs/comrak.render | 696780 | 974.5 µs | 952.4 µs | 996.4 µs | 715.0 | 1024 × 1 | 45 | 3.34 | 1.67 | 1.05 |
| real-docs/mdrs.html | 696780 | 106.59 ms | 105.99 ms | 107.53 ms | 6.5 | 10 × 1 | 290043 | 243.34 | 59.54 | 1.09 |
| real-docs/mdrs.mdast | 696780 | 107.65 ms | 105.41 ms | 108.28 ms | 6.5 | 10 × 1 | 302988 | 257.18 | 59.54 | 1.05 |
| real-docs/ours.canonical | 696780 | 4.03 ms | 3.82 ms | 4.10 ms | 173.0 | 249 × 1 | 83412 | 7.99 | 0.85 | 1.08 |
| real-docs/ours.definitions | 696780 | 11.3 µs | 10.5 µs | 11.7 µs | 61682.3 | 4924 × 18 | 0 | 0.00 | 0.00 | 1.02 |
| real-docs/ours.editor_parse | 696780 | 14.67 ms | 14.35 ms | 15.74 ms | 47.5 | 67 × 1 | 205899 | 72.42 | 22.82 | 1.08 |
| real-docs/ours.html | 696780 | 17.88 ms | 17.03 ms | 18.18 ms | 39.0 | 56 × 1 | 278600 | 57.28 | 15.96 | 1.09 |
| real-docs/ours.inline_largest | 2622 | 20.9 µs | 18.2 µs | 22.0 µs | 125.6 | 4769 × 10 | 202 | 0.05 | 0.03 | 1.03 |
| real-docs/ours.inline_leaves | 696780 | 8.01 ms | 7.71 ms | 8.21 ms | 87.0 | 125 × 1 | 138515 | 23.81 | 0.05 | 1.05 |
| real-docs/ours.leaf_sources | 696780 | 2.70 ms | 2.62 ms | 2.92 ms | 257.8 | 364 × 1 | 46101 | 27.47 | 9.54 | 1.19 |
| real-docs/ours.parse | 696780 | 11.37 ms | 10.92 ms | 11.80 ms | 61.3 | 88 × 1 | 159798 | 44.94 | 15.27 | 1.08 |
| real-docs/ours.render | 696780 | 6.39 ms | 6.21 ms | 6.60 ms | 109.1 | 155 × 1 | 118802 | 12.33 | 2.68 | 1.14 |
| real-docs/ours.validate | 696780 | 1.55 ms | 1.50 ms | 1.62 ms | 449.8 | 641 × 1 | 25850 | 1.19 | 0.00 | 1.08 |
| real-docs/ours.writeback | 696780 | 15.14 ms | 14.06 ms | 15.38 ms | 46.0 | 68 × 1 | 214003 | 50.19 | 15.27 | 1.07 |
| real-docs/pulldown.events | 696780 | 1.63 ms | 1.52 ms | 1.68 ms | 427.3 | 610 × 1 | 231 | 7.73 | 4.39 | 1.04 |
| real-docs/pulldown.html | 696780 | 2.14 ms | 2.01 ms | 2.20 ms | 326.1 | 468 × 1 | 248 | 10.94 | 5.99 | 1.03 |
| syn-cjk/comrak.commonmark | 274902 | 2.15 ms | 1.98 ms | 2.24 ms | 128.1 | 467 × 1 | 99466 | 1.36 | 0.54 | 1.01 |
| syn-cjk/comrak.html | 274902 | 813.5 µs | 752.5 µs | 850.1 µs | 337.9 | 1218 × 1 | 8403 | 3.59 | 2.85 | 1.04 |
| syn-cjk/comrak.parse | 274902 | 605.7 µs | 552.1 µs | 622.9 µs | 453.9 | 1643 × 1 | 8380 | 2.72 | 2.59 | 1.04 |
| syn-cjk/comrak.render | 274902 | 200.4 µs | 188.7 µs | 210.2 µs | 1371.9 | 2449 × 2 | 23 | 0.87 | 0.43 | 1.01 |
| syn-cjk/mdrs.html | 274902 | 12.36 ms | 11.87 ms | 12.56 ms | 22.2 | 81 × 1 | 69325 | 32.75 | 9.34 | 1.02 |
| syn-cjk/mdrs.mdast | 274902 | 12.27 ms | 11.99 ms | 12.70 ms | 22.4 | 81 × 1 | 65102 | 34.84 | 9.34 | 1.02 |
| syn-cjk/ours.canonical | 274902 | 404.5 µs | 360.1 µs | 417.9 µs | 679.5 | 2475 × 1 | 5334 | 1.61 | 0.47 | 1.04 |
| syn-cjk/ours.definitions | 274902 | 490 ns | 451 ns | 506 ns | 561288.4 | 5000 × 395 | 0 | 0.00 | 0.00 | 1.04 |
| syn-cjk/ours.editor_parse | 274902 | 2.40 ms | 2.24 ms | 2.47 ms | 114.4 | 416 × 1 | 37514 | 10.59 | 2.99 | 1.02 |
| syn-cjk/ours.html | 274902 | 2.88 ms | 2.72 ms | 2.96 ms | 95.3 | 345 × 1 | 48868 | 10.17 | 3.01 | 1.03 |
| syn-cjk/ours.inline_largest | 667 | 5.7 µs | 5.0 µs | 5.9 µs | 117.0 | 4950 × 35 | 111 | 0.03 | 0.01 | 1.03 |
| syn-cjk/ours.inline_leaves | 274902 | 1.83 ms | 1.77 ms | 1.88 ms | 150.2 | 545 × 1 | 34455 | 5.94 | 0.02 | 1.04 |
| syn-cjk/ours.leaf_sources | 274902 | 318.8 µs | 288.8 µs | 333.4 µs | 862.3 | 3123 × 1 | 4007 | 2.77 | 0.91 | 1.02 |
| syn-cjk/ours.parse | 274902 | 2.07 ms | 2.01 ms | 2.13 ms | 133.0 | 479 × 1 | 33507 | 7.82 | 2.28 | 1.06 |
| syn-cjk/ours.render | 274902 | 787.8 µs | 710.9 µs | 808.2 µs | 349.0 | 1268 × 1 | 15361 | 2.35 | 0.93 | 1.06 |
| syn-cjk/ours.validate | 274902 | 101.3 µs | 89.5 µs | 105.2 µs | 2714.0 | 4936 × 2 | 1 | 0.00 | 0.00 | 1.07 |
| syn-cjk/ours.writeback | 274902 | 2.27 ms | 2.12 ms | 2.33 ms | 121.3 | 441 × 1 | 33515 | 8.14 | 2.41 | 1.04 |
| syn-cjk/pulldown.events | 274902 | 310.9 µs | 280.1 µs | 324.2 µs | 884.3 | 3206 × 1 | 16 | 1.33 | 0.88 | 1.03 |
| syn-cjk/pulldown.html | 274902 | 444.5 µs | 401.2 µs | 465.7 µs | 618.5 | 2238 × 1 | 29 | 2.17 | 1.30 | 1.02 |
| syn-code/comrak.commonmark | 100485 | 1.79 ms | 1.75 ms | 1.84 ms | 56.1 | 557 × 1 | 96545 | 0.37 | 0.14 | 1.04 |
| syn-code/comrak.html | 100485 | 163.4 µs | 160.4 µs | 170.4 µs | 615.0 | 3032 × 2 | 1771 | 0.67 | 0.34 | 1.04 |
| syn-code/comrak.parse | 100485 | 136.5 µs | 133.3 µs | 141.6 µs | 736.3 | 3637 × 2 | 1427 | 0.24 | 0.15 | 1.04 |
| syn-code/comrak.render | 100485 | 27.0 µs | 24.6 µs | 27.9 µs | 3715.9 | 4587 × 8 | 344 | 0.43 | 0.20 | 1.03 |
| syn-code/mdrs.html | 100485 | 4.59 ms | 4.41 ms | 4.66 ms | 21.9 | 218 × 1 | 20034 | 14.02 | 7.44 | 1.01 |
| syn-code/mdrs.mdast | 100485 | 4.39 ms | 4.19 ms | 4.50 ms | 22.9 | 228 × 1 | 12540 | 14.30 | 7.44 | 1.05 |
| syn-code/ours.canonical | 100485 | 102.8 µs | 94.8 µs | 106.1 µs | 977.2 | 4786 × 2 | 1135 | 0.49 | 0.19 | 1.01 |
| syn-code/ours.definitions | 100485 | 133 ns | 120 ns | 136 ns | 753265.3 | 5000 × 1270 | 0 | 0.00 | 0.00 | 1.01 |
| syn-code/ours.editor_parse | 100485 | 508.2 µs | 459.0 µs | 528.5 µs | 197.7 | 1963 × 1 | 3157 | 2.74 | 0.86 | 1.04 |
| syn-code/ours.html | 100485 | 473.0 µs | 428.5 µs | 483.3 µs | 212.5 | 2115 × 1 | 2651 | 2.03 | 0.65 | 1.06 |
| syn-code/ours.leaf_sources | 100485 | 222.1 µs | 198.1 µs | 232.9 µs | 452.5 | 4492 × 1 | 1468 | 1.26 | 0.58 | 1.04 |
| syn-code/ours.parse | 100485 | 289.3 µs | 259.0 µs | 303.2 µs | 347.3 | 3438 × 1 | 1689 | 1.49 | 0.65 | 1.03 |
| syn-code/ours.render | 100485 | 184.5 µs | 174.5 µs | 190.9 µs | 544.6 | 2680 × 2 | 962 | 0.54 | 0.32 | 1.04 |
| syn-code/ours.validate | 100485 | 660 ns | 604 ns | 690 ns | 152362.7 | 5000 × 297 | 1 | 0.00 | 0.00 | 1.02 |
| syn-code/ours.writeback | 100485 | 314.9 µs | 290.2 µs | 323.3 µs | 319.1 | 3148 × 1 | 1898 | 1.61 | 0.65 | 1.01 |
| syn-code/pulldown.events | 100485 | 48.4 µs | 46.3 µs | 49.9 µs | 2074.4 | 4068 × 5 | 10 | 0.17 | 0.17 | 1.02 |
| syn-code/pulldown.html | 100485 | 82.4 µs | 77.8 µs | 83.9 µs | 1220.1 | 4022 × 3 | 21 | 0.57 | 0.36 | 1.02 |
| syn-directives/comrak.commonmark | 100202 | 2.06 ms | 1.99 ms | 2.11 ms | 48.6 | 483 × 1 | 99541 | 0.39 | 0.14 | 1.02 |
| syn-directives/comrak.html | 100202 | 810.5 µs | 769.2 µs | 859.1 µs | 123.6 | 1220 × 1 | 9930 | 2.79 | 2.27 | 1.01 |
| syn-directives/comrak.parse | 100202 | 644.2 µs | 604.4 µs | 681.9 µs | 155.6 | 1537 × 1 | 9906 | 2.41 | 2.16 | 1.01 |
| syn-directives/comrak.render | 100202 | 148.1 µs | 134.8 µs | 153.5 µs | 676.5 | 3350 × 2 | 24 | 0.38 | 0.19 | 1.01 |
| syn-directives/mdrs.html | 100202 | 7.89 ms | 7.66 ms | 8.48 ms | 12.7 | 124 × 1 | 48430 | 34.24 | 8.29 | 1.06 |
| syn-directives/mdrs.mdast | 100202 | 8.31 ms | 7.75 ms | 8.51 ms | 12.1 | 120 × 1 | 48783 | 35.95 | 8.29 | 1.01 |
| syn-directives/ours.canonical | 100202 | 389.9 µs | 383.9 µs | 401.4 µs | 257.0 | 2545 × 1 | 6574 | 1.17 | 0.15 | 1.06 |
| syn-directives/ours.definitions | 100202 | 646 ns | 624 ns | 666 ns | 155085.2 | 5000 × 302 | 0 | 0.00 | 0.00 | 1.03 |
| syn-directives/ours.editor_parse | 100202 | 2.12 ms | 2.02 ms | 2.18 ms | 47.3 | 470 × 1 | 33068 | 10.70 | 2.44 | 1.04 |
| syn-directives/ours.html | 100202 | 2.64 ms | 2.60 ms | 2.71 ms | 37.9 | 377 × 1 | 44846 | 9.51 | 1.98 | 1.06 |
| syn-directives/ours.inline_largest | 722 | 10.1 µs | 9.8 µs | 10.5 µs | 71.3 | 4907 × 20 | 115 | 0.03 | 0.02 | 1.04 |
| syn-directives/ours.inline_leaves | 100202 | 1.61 ms | 1.57 ms | 1.66 ms | 62.3 | 619 × 1 | 21220 | 5.78 | 0.03 | 1.07 |
| syn-directives/ours.leaf_sources | 100202 | 307.1 µs | 296.1 µs | 318.7 µs | 326.3 | 3226 × 1 | 7032 | 2.98 | 0.87 | 1.03 |
| syn-directives/ours.parse | 100202 | 1.80 ms | 1.71 ms | 1.86 ms | 55.6 | 553 × 1 | 26036 | 7.72 | 1.80 | 1.04 |
| syn-directives/ours.render | 100202 | 800.0 µs | 776.9 µs | 828.5 µs | 125.3 | 1243 × 1 | 18810 | 1.79 | 0.41 | 1.05 |
| syn-directives/ours.validate | 100202 | 89.0 µs | 84.2 µs | 93.1 µs | 1126.4 | 3717 × 3 | 474 | 0.00 | 0.00 | 1.03 |
| syn-directives/ours.writeback | 100202 | 2.00 ms | 1.96 ms | 2.06 ms | 50.2 | 499 × 1 | 27372 | 7.86 | 1.80 | 1.10 |
| syn-directives/pulldown.events | 100202 | 251.6 µs | 242.1 µs | 264.3 µs | 398.2 | 3905 × 1 | 338 | 1.23 | 0.70 | 1.02 |
| syn-directives/pulldown.html | 100202 | 329.8 µs | 311.8 µs | 354.1 µs | 303.8 | 2974 × 1 | 353 | 1.59 | 0.88 | 1.01 |
| syn-emphasis/comrak.commonmark | 100035 | 2.83 ms | 2.72 ms | 2.90 ms | 35.4 | 354 × 1 | 98575 | 0.39 | 0.15 | 1.05 |
| syn-emphasis/comrak.html | 100035 | 4.87 ms | 4.64 ms | 4.99 ms | 20.5 | 206 × 1 | 75612 | 19.11 | 17.82 | 1.13 |
| syn-emphasis/comrak.parse | 100035 | 3.74 ms | 3.64 ms | 3.86 ms | 26.8 | 267 × 1 | 75585 | 18.03 | 17.82 | 1.07 |
| syn-emphasis/comrak.render | 100035 | 949.0 µs | 909.7 µs | 977.3 µs | 105.4 | 1041 × 1 | 27 | 1.08 | 0.54 | 1.07 |
| syn-emphasis/mdrs.html | 100035 | 33.12 ms | 32.48 ms | 33.29 ms | 3.0 | 31 × 1 | 180737 | 182.75 | 54.64 | 1.03 |
| syn-emphasis/mdrs.mdast | 100035 | 34.74 ms | 34.49 ms | 35.04 ms | 2.9 | 29 × 1 | 202413 | 205.00 | 54.64 | 1.05 |
| syn-emphasis/ours.canonical | 100035 | 664.4 µs | 653.9 µs | 679.4 µs | 150.6 | 1498 × 1 | 11350 | 0.50 | 0.10 | 1.06 |
| syn-emphasis/ours.definitions | 100035 | 475 ns | 424 ns | 501 ns | 210813.6 | 5000 × 399 | 0 | 0.00 | 0.00 | 1.01 |
| syn-emphasis/ours.editor_parse | 100035 | 5.71 ms | 5.40 ms | 6.17 ms | 17.5 | 172 × 1 | 155809 | 66.53 | 15.08 | 1.03 |
| syn-emphasis/ours.html | 100035 | 8.54 ms | 7.96 ms | 8.76 ms | 11.7 | 118 × 1 | 238252 | 66.82 | 15.21 | 1.04 |
| syn-emphasis/ours.inline_largest | 243 | 11.2 µs | 11.0 µs | 11.7 µs | 21.6 | 4890 × 18 | 344 | 0.14 | 0.08 | 1.04 |
| syn-emphasis/ours.inline_leaves | 100035 | 5.39 ms | 5.29 ms | 5.44 ms | 18.6 | 186 × 1 | 154998 | 70.24 | 0.08 | 1.04 |
| syn-emphasis/ours.leaf_sources | 100035 | 232.4 µs | 220.4 µs | 241.5 µs | 430.5 | 4247 × 1 | 3757 | 2.53 | 0.73 | 1.00 |
| syn-emphasis/ours.parse | 100035 | 5.57 ms | 5.34 ms | 5.87 ms | 18.0 | 178 × 1 | 152052 | 64.00 | 14.72 | 1.01 |
| syn-emphasis/ours.render | 100035 | 2.56 ms | 2.45 ms | 2.65 ms | 39.1 | 390 × 1 | 86200 | 2.83 | 0.86 | 1.08 |
| syn-emphasis/ours.validate | 100035 | 225.9 µs | 209.8 µs | 254.2 µs | 442.8 | 4235 × 1 | 1 | 0.00 | 0.00 | 1.04 |
| syn-emphasis/ours.writeback | 100035 | 6.49 ms | 6.23 ms | 6.73 ms | 15.4 | 150 × 1 | 152060 | 64.14 | 14.72 | 1.11 |
| syn-emphasis/pulldown.events | 100035 | 2.05 ms | 1.99 ms | 2.13 ms | 48.9 | 486 × 1 | 9 | 9.46 | 4.81 | 1.06 |
| syn-emphasis/pulldown.html | 100035 | 2.17 ms | 2.09 ms | 2.25 ms | 46.1 | 459 × 1 | 26 | 10.51 | 5.34 | 1.04 |
| syn-headings/comrak.commonmark | 100011 | 1.93 ms | 1.88 ms | 1.97 ms | 51.9 | 518 × 1 | 92933 | 0.49 | 0.20 | 1.02 |
| syn-headings/comrak.html | 100011 | 856.7 µs | 823.3 µs | 887.3 µs | 116.7 | 1160 × 1 | 7394 | 2.98 | 2.21 | 1.03 |
| syn-headings/comrak.parse | 100011 | 651.7 µs | 635.0 µs | 667.8 µs | 153.5 | 1529 × 1 | 7368 | 2.60 | 2.15 | 1.03 |
| syn-headings/comrak.render | 100011 | 196.8 µs | 186.2 µs | 202.7 µs | 508.3 | 2529 × 2 | 26 | 0.38 | 0.19 | 1.03 |
| syn-headings/mdrs.html | 100011 | 34.61 ms | 33.92 ms | 34.79 ms | 2.9 | 29 × 1 | 96302 | 62.44 | 32.17 | 1.04 |
| syn-headings/mdrs.mdast | 100011 | 34.28 ms | 34.07 ms | 34.57 ms | 2.9 | 30 × 1 | 85640 | 65.40 | 32.17 | 1.04 |
| syn-headings/ours.canonical | 100011 | 525.9 µs | 516.0 µs | 541.4 µs | 190.2 | 1890 × 1 | 14227 | 0.48 | 0.14 | 1.05 |
| syn-headings/ours.definitions | 100011 | 2.5 µs | 2.3 µs | 2.5 µs | 40512.8 | 5000 × 81 | 0 | 0.00 | 0.00 | 1.05 |
| syn-headings/ours.editor_parse | 100011 | 2.13 ms | 2.08 ms | 2.19 ms | 47.0 | 468 × 1 | 34846 | 14.77 | 5.29 | 1.06 |
| syn-headings/ours.html | 100011 | 2.26 ms | 2.23 ms | 2.31 ms | 44.3 | 442 × 1 | 40398 | 9.06 | 4.43 | 1.08 |
| syn-headings/ours.inline_largest | 40 | 344 ns | 336 ns | 357 ns | 116.3 | 5000 × 560 | 8 | 0.00 | 0.00 | 1.09 |
| syn-headings/ours.inline_leaves | 100011 | 1.02 ms | 997.7 µs | 1.06 ms | 97.7 | 973 × 1 | 24085 | 2.55 | 0.00 | 1.04 |
| syn-headings/ours.leaf_sources | 100011 | 638.1 µs | 627.2 µs | 663.0 µs | 156.7 | 1553 × 1 | 10708 | 6.46 | 2.66 | 1.05 |
| syn-headings/ours.parse | 100011 | 1.48 ms | 1.45 ms | 1.53 ms | 67.6 | 672 × 1 | 24138 | 8.31 | 4.43 | 1.09 |
| syn-headings/ours.render | 100011 | 765.3 µs | 743.9 µs | 787.7 µs | 130.7 | 1300 × 1 | 16260 | 0.75 | 0.38 | 1.04 |
| syn-headings/ours.validate | 100011 | 79.4 µs | 77.4 µs | 83.5 µs | 1260.0 | 4142 × 3 | 1 | 0.00 | 0.00 | 1.04 |
| syn-headings/ours.writeback | 100011 | 1.73 ms | 1.69 ms | 1.78 ms | 57.9 | 576 × 1 | 24146 | 8.65 | 4.43 | 1.10 |
| syn-headings/pulldown.events | 100011 | 242.2 µs | 238.8 µs | 251.3 µs | 413.0 | 4092 × 1 | 5 | 1.06 | 0.61 | 1.02 |
| syn-headings/pulldown.html | 100011 | 331.3 µs | 325.6 µs | 345.8 µs | 301.9 | 2988 × 1 | 19 | 1.31 | 0.73 | 1.01 |
| syn-html/comrak.commonmark | 100278 | 1.90 ms | 1.86 ms | 1.95 ms | 52.7 | 524 × 1 | 97917 | 0.40 | 0.15 | 1.04 |
| syn-html/comrak.html | 100278 | 507.4 µs | 492.7 µs | 528.1 µs | 197.6 | 1958 × 1 | 6086 | 1.64 | 1.24 | 1.06 |
| syn-html/comrak.parse | 100278 | 414.2 µs | 403.6 µs | 425.4 µs | 242.1 | 2401 × 1 | 6063 | 1.40 | 1.20 | 1.05 |
| syn-html/comrak.render | 100278 | 82.8 µs | 78.5 µs | 84.5 µs | 1210.4 | 4027 × 3 | 23 | 0.24 | 0.12 | 1.04 |
| syn-html/mdrs.html | 100278 | 5.88 ms | 5.69 ms | 5.97 ms | 17.0 | 171 × 1 | 32549 | 27.96 | 7.61 | 1.09 |
| syn-html/mdrs.mdast | 100278 | 5.91 ms | 5.79 ms | 5.99 ms | 17.0 | 169 × 1 | 33954 | 29.42 | 7.61 | 1.10 |
| syn-html/ours.canonical | 100278 | 272.7 µs | 264.3 µs | 280.2 µs | 367.7 | 3656 × 1 | 4667 | 0.79 | 0.16 | 1.04 |
| syn-html/ours.definitions | 100278 | 858 ns | 838 ns | 876 ns | 116913.4 | 5000 × 229 | 0 | 0.00 | 0.00 | 1.01 |
| syn-html/ours.editor_parse | 100278 | 1.72 ms | 1.68 ms | 1.77 ms | 58.4 | 580 × 1 | 24382 | 9.82 | 2.61 | 1.05 |
| syn-html/ours.html | 100278 | 1.94 ms | 1.90 ms | 1.99 ms | 51.6 | 513 × 1 | 30564 | 7.56 | 1.95 | 1.09 |
| syn-html/ours.inline_largest | 646 | 9.5 µs | 9.2 µs | 9.8 µs | 68.2 | 4979 × 21 | 119 | 0.04 | 0.02 | 1.05 |
| syn-html/ours.inline_leaves | 100278 | 933.3 µs | 916.1 µs | 966.9 µs | 107.4 | 1064 × 1 | 12246 | 3.26 | 0.03 | 1.07 |
| syn-html/ours.leaf_sources | 100278 | 390.2 µs | 384.2 µs | 404.7 µs | 257.0 | 2542 × 1 | 6340 | 3.45 | 1.45 | 1.04 |
| syn-html/ours.parse | 100278 | 1.31 ms | 1.29 ms | 1.36 ms | 76.4 | 758 × 1 | 18042 | 6.37 | 1.95 | 1.06 |
| syn-html/ours.render | 100278 | 596.6 µs | 581.1 µs | 617.0 µs | 168.1 | 1667 × 1 | 12522 | 1.19 | 0.38 | 1.06 |
| syn-html/ours.validate | 100278 | 75.0 µs | 72.5 µs | 78.2 µs | 1337.8 | 4410 × 3 | 513 | 0.00 | 0.00 | 1.04 |
| syn-html/ours.writeback | 100278 | 1.51 ms | 1.47 ms | 1.55 ms | 66.6 | 662 × 1 | 19525 | 6.55 | 1.95 | 1.09 |
| syn-html/pulldown.events | 100278 | 192.1 µs | 189.5 µs | 198.9 µs | 521.9 | 2577 × 2 | 206 | 1.15 | 0.66 | 1.03 |
| syn-html/pulldown.html | 100278 | 233.9 µs | 230.5 µs | 245.4 µs | 428.8 | 4222 × 1 | 218 | 1.49 | 0.83 | 1.04 |
| syn-lists/comrak.commonmark | 101258 | 2.04 ms | 1.96 ms | 2.08 ms | 49.7 | 490 × 1 | 99144 | 0.38 | 0.13 | 1.04 |
| syn-lists/comrak.html | 101258 | 869.2 µs | 832.7 µs | 924.1 µs | 116.5 | 1139 × 1 | 9789 | 2.74 | 2.26 | 1.11 |
| syn-lists/comrak.parse | 101258 | 714.3 µs | 686.3 µs | 757.2 µs | 141.8 | 1351 × 1 | 9767 | 2.41 | 2.17 | 1.13 |
| syn-lists/comrak.render | 101258 | 134.7 µs | 131.0 µs | 139.0 µs | 751.6 | 3691 × 2 | 22 | 0.33 | 0.16 | 1.04 |
| syn-lists/mdrs.html | 101258 | 8.06 ms | 7.76 ms | 8.26 ms | 12.6 | 124 × 1 | 53436 | 36.53 | 9.16 | 1.08 |
| syn-lists/mdrs.mdast | 101258 | 8.32 ms | 8.15 ms | 8.44 ms | 12.2 | 121 × 1 | 55794 | 38.52 | 9.16 | 1.09 |
| syn-lists/ours.canonical | 101258 | 849.2 µs | 830.2 µs | 871.2 µs | 119.2 | 1173 × 1 | 13429 | 3.36 | 0.20 | 1.08 |
| syn-lists/ours.definitions | 101258 | 2.4 µs | 2.2 µs | 2.6 µs | 43067.9 | 5000 × 82 | 0 | 0.00 | 0.00 | 1.12 |
| syn-lists/ours.editor_parse | 101258 | 2.15 ms | 2.11 ms | 2.21 ms | 47.0 | 463 × 1 | 36546 | 11.50 | 3.71 | 1.12 |
| syn-lists/ours.html | 101258 | 2.88 ms | 2.82 ms | 2.95 ms | 35.2 | 347 × 1 | 51842 | 12.11 | 2.63 | 1.13 |
| syn-lists/ours.inline_largest | 237 | 2.8 µs | 2.7 µs | 2.9 µs | 84.5 | 5000 × 69 | 34 | 0.00 | 0.00 | 1.07 |
| syn-lists/ours.inline_leaves | 101258 | 1.43 ms | 1.39 ms | 1.48 ms | 70.6 | 693 × 1 | 24955 | 4.14 | 0.01 | 1.07 |
| syn-lists/ours.leaf_sources | 101258 | 424.9 µs | 412.6 µs | 440.8 µs | 238.3 | 2338 × 1 | 8057 | 4.27 | 1.49 | 1.03 |
| syn-lists/ours.parse | 101258 | 1.71 ms | 1.67 ms | 1.76 ms | 59.2 | 582 × 1 | 28489 | 7.24 | 2.32 | 1.11 |
| syn-lists/ours.render | 101258 | 1.10 ms | 1.06 ms | 1.14 ms | 91.7 | 901 × 1 | 23353 | 4.87 | 0.41 | 1.10 |
| syn-lists/ours.validate | 101258 | 276.4 µs | 261.2 µs | 289.0 µs | 366.4 | 3585 × 1 | 4024 | 0.24 | 0.00 | 1.05 |
| syn-lists/ours.writeback | 101258 | 2.60 ms | 2.54 ms | 2.67 ms | 39.0 | 383 × 1 | 43122 | 8.91 | 2.33 | 1.12 |
| syn-lists/pulldown.events | 101258 | 262.9 µs | 254.9 µs | 272.9 µs | 385.2 | 3766 × 1 | 327 | 1.24 | 0.70 | 1.04 |
| syn-lists/pulldown.html | 101258 | 329.6 µs | 322.2 µs | 348.2 µs | 307.2 | 2987 × 1 | 342 | 1.57 | 0.86 | 1.06 |
| syn-paragraphs/comrak.commonmark | 100006 | 2.03 ms | 1.96 ms | 2.08 ms | 49.3 | 492 × 1 | 99544 | 0.39 | 0.14 | 1.02 |
| syn-paragraphs/comrak.html | 100006 | 627.1 µs | 605.3 µs | 649.8 µs | 159.5 | 1589 × 1 | 8118 | 1.79 | 1.35 | 1.05 |
| syn-paragraphs/comrak.parse | 100006 | 484.0 µs | 467.0 µs | 497.8 µs | 206.6 | 2059 × 1 | 8095 | 1.41 | 1.24 | 1.06 |
| syn-paragraphs/comrak.render | 100006 | 124.3 µs | 116.8 µs | 127.5 µs | 804.6 | 4022 × 2 | 23 | 0.38 | 0.19 | 1.04 |
| syn-paragraphs/mdrs.html | 100006 | 6.94 ms | 6.69 ms | 7.04 ms | 14.4 | 145 × 1 | 44068 | 30.99 | 7.75 | 1.05 |
| syn-paragraphs/mdrs.mdast | 100006 | 6.96 ms | 6.84 ms | 7.07 ms | 14.4 | 144 × 1 | 44886 | 32.64 | 7.75 | 1.10 |
| syn-paragraphs/ours.canonical | 100006 | 281.0 µs | 271.4 µs | 288.4 µs | 355.9 | 3540 × 1 | 3718 | 0.61 | 0.16 | 1.04 |
| syn-paragraphs/ours.definitions | 100006 | 203 ns | 183 ns | 208 ns | 493846.2 | 5000 × 922 | 0 | 0.00 | 0.00 | 1.03 |
| syn-paragraphs/ours.editor_parse | 100006 | 1.92 ms | 1.88 ms | 1.98 ms | 52.1 | 519 × 1 | 24432 | 8.26 | 1.80 | 1.07 |
| syn-paragraphs/ours.html | 100006 | 2.45 ms | 2.39 ms | 2.51 ms | 40.9 | 408 × 1 | 36576 | 7.67 | 1.65 | 1.08 |
| syn-paragraphs/ours.inline_largest | 736 | 9.4 µs | 8.7 µs | 9.8 µs | 78.2 | 5000 × 21 | 76 | 0.01 | 0.01 | 1.04 |
| syn-paragraphs/ours.inline_leaves | 100006 | 1.71 ms | 1.67 ms | 1.76 ms | 58.4 | 583 × 1 | 22372 | 6.02 | 0.03 | 1.04 |
| syn-paragraphs/ours.leaf_sources | 100006 | 171.4 µs | 165.9 µs | 178.1 µs | 583.4 | 2888 × 2 | 2382 | 1.68 | 0.54 | 1.03 |
| syn-paragraphs/ours.parse | 100006 | 1.74 ms | 1.70 ms | 1.78 ms | 57.5 | 574 × 1 | 22050 | 6.57 | 1.50 | 1.07 |
| syn-paragraphs/ours.render | 100006 | 665.2 µs | 643.7 µs | 688.7 µs | 150.3 | 1491 × 1 | 14526 | 1.10 | 0.38 | 1.05 |
| syn-paragraphs/ours.validate | 100006 | 87.9 µs | 82.1 µs | 91.7 µs | 1137.9 | 3771 × 3 | 505 | 0.00 | 0.00 | 1.05 |
| syn-paragraphs/ours.writeback | 100006 | 1.91 ms | 1.87 ms | 1.95 ms | 52.4 | 522 × 1 | 22562 | 6.70 | 1.50 | 1.09 |
| syn-paragraphs/pulldown.events | 100006 | 229.8 µs | 220.1 µs | 238.4 µs | 435.3 | 4307 × 1 | 364 | 1.26 | 0.71 | 1.04 |
| syn-paragraphs/pulldown.html | 100006 | 300.4 µs | 289.0 µs | 314.9 µs | 332.9 | 3292 × 1 | 378 | 1.62 | 0.89 | 1.05 |
| syn-quotes/comrak.commonmark | 100693 | 2.00 ms | 1.90 ms | 2.15 ms | 50.4 | 494 × 1 | 97357 | 0.37 | 0.14 | 1.03 |
| syn-quotes/comrak.html | 100693 | 653.2 µs | 626.3 µs | 675.6 µs | 154.2 | 1522 × 1 | 8228 | 2.73 | 2.31 | 1.11 |
| syn-quotes/comrak.parse | 100693 | 502.2 µs | 481.1 µs | 517.4 µs | 200.5 | 1982 × 1 | 8205 | 2.36 | 2.20 | 1.07 |
| syn-quotes/comrak.render | 100693 | 132.4 µs | 125.2 µs | 135.9 µs | 760.8 | 3771 × 2 | 23 | 0.37 | 0.18 | 1.05 |
| syn-quotes/mdrs.html | 100693 | 7.44 ms | 7.15 ms | 7.56 ms | 13.5 | 135 × 1 | 45541 | 38.49 | 9.45 | 1.12 |
| syn-quotes/mdrs.mdast | 100693 | 7.53 ms | 7.39 ms | 7.66 ms | 13.4 | 133 × 1 | 46359 | 40.14 | 9.45 | 1.18 |
| syn-quotes/ours.canonical | 100693 | 463.8 µs | 430.9 µs | 474.9 µs | 217.1 | 2150 × 1 | 5576 | 1.45 | 0.14 | 1.04 |
| syn-quotes/ours.definitions | 100693 | 799 ns | 765 ns | 826 ns | 126012.7 | 5000 × 242 | 0 | 0.00 | 0.00 | 1.03 |
| syn-quotes/ours.editor_parse | 100693 | 2.06 ms | 2.01 ms | 2.11 ms | 48.9 | 484 × 1 | 27742 | 9.21 | 2.32 | 1.06 |
| syn-quotes/ours.html | 100693 | 2.60 ms | 2.53 ms | 2.65 ms | 38.8 | 385 × 1 | 39811 | 9.16 | 1.94 | 1.07 |
| syn-quotes/ours.inline_largest | 671 | 11.1 µs | 10.2 µs | 11.6 µs | 60.2 | 4937 × 18 | 121 | 0.03 | 0.01 | 1.03 |
| syn-quotes/ours.inline_leaves | 100693 | 1.68 ms | 1.60 ms | 1.72 ms | 60.1 | 596 × 1 | 21948 | 5.96 | 0.03 | 1.06 |
| syn-quotes/ours.leaf_sources | 100693 | 241.5 µs | 227.4 µs | 251.0 µs | 417.0 | 4100 × 1 | 4062 | 2.19 | 0.77 | 1.04 |
| syn-quotes/ours.parse | 100693 | 1.79 ms | 1.75 ms | 1.84 ms | 56.3 | 556 × 1 | 23680 | 7.01 | 1.73 | 1.08 |
| syn-quotes/ours.render | 100693 | 728.4 µs | 701.2 µs | 754.0 µs | 138.2 | 1367 × 1 | 16131 | 2.15 | 0.40 | 1.03 |
| syn-quotes/ours.validate | 100693 | 91.6 µs | 83.0 µs | 95.5 µs | 1098.8 | 3626 × 3 | 558 | 0.00 | 0.00 | 1.05 |
| syn-quotes/ours.writeback | 100693 | 1.98 ms | 1.93 ms | 2.03 ms | 50.8 | 503 × 1 | 24858 | 7.15 | 1.73 | 1.08 |
| syn-quotes/pulldown.events | 100693 | 241.3 µs | 230.6 µs | 250.6 µs | 417.2 | 4104 × 1 | 356 | 1.26 | 0.71 | 1.04 |
| syn-quotes/pulldown.html | 100693 | 320.6 µs | 308.0 µs | 336.9 µs | 314.1 | 3083 × 1 | 677 | 1.70 | 0.92 | 1.06 |
| syn-references/comrak.commonmark | 100276 | 1.43 ms | 1.37 ms | 1.47 ms | 70.3 | 698 × 1 | 69209 | 0.34 | 0.14 | 1.04 |
| syn-references/comrak.html | 100276 | 1.10 ms | 1.06 ms | 1.13 ms | 91.3 | 906 × 1 | 22964 | 3.73 | 2.21 | 1.05 |
| syn-references/comrak.parse | 100276 | 1.04 ms | 1.01 ms | 1.06 ms | 96.3 | 959 × 1 | 22944 | 3.45 | 2.21 | 1.05 |
| syn-references/comrak.render | 100276 | 46.8 µs | 43.5 µs | 47.8 µs | 2143.0 | 4283 × 5 | 20 | 0.28 | 0.14 | 1.04 |
| syn-references/mdrs.html | 100276 | 13.43 ms | 13.16 ms | 13.63 ms | 7.5 | 75 × 1 | 87028 | 52.03 | 12.07 | 1.10 |
| syn-references/mdrs.mdast | 100276 | 13.38 ms | 13.22 ms | 13.52 ms | 7.5 | 75 × 1 | 97972 | 54.68 | 12.07 | 1.10 |
| syn-references/ours.canonical | 100276 | 829.0 µs | 795.4 µs | 845.9 µs | 121.0 | 1206 × 1 | 30640 | 1.09 | 0.21 | 1.06 |
| syn-references/ours.definitions | 100276 | 43.0 µs | 40.0 µs | 44.1 µs | 2330.2 | 4623 × 5 | 918 | 0.05 | 0.03 | 1.04 |
| syn-references/ours.editor_parse | 100276 | 2.87 ms | 2.81 ms | 2.92 ms | 35.0 | 349 × 1 | 74381 | 9.67 | 2.58 | 1.07 |
| syn-references/ours.html | 100276 | 3.16 ms | 3.08 ms | 3.23 ms | 31.7 | 316 × 1 | 88643 | 8.13 | 2.28 | 1.12 |
| syn-references/ours.inline_largest | 362 | 7.5 µs | 7.0 µs | 7.8 µs | 48.4 | 4903 × 27 | 210 | 0.02 | 0.01 | 1.05 |
| syn-references/ours.inline_leaves | 100276 | 1.40 ms | 1.36 ms | 1.44 ms | 71.5 | 711 × 1 | 36732 | 3.95 | 0.01 | 1.06 |
| syn-references/ours.leaf_sources | 100276 | 698.8 µs | 675.6 µs | 718.7 µs | 143.5 | 1423 × 1 | 19440 | 2.80 | 0.76 | 1.04 |
| syn-references/ours.parse | 100276 | 2.13 ms | 2.10 ms | 2.18 ms | 47.0 | 468 × 1 | 54941 | 6.86 | 2.27 | 1.12 |
| syn-references/ours.render | 100276 | 984.3 µs | 949.1 µs | 1.01 ms | 101.9 | 1013 × 1 | 33702 | 1.26 | 0.47 | 1.04 |
| syn-references/ours.validate | 100276 | 478.0 µs | 459.8 µs | 496.3 µs | 209.8 | 2081 × 1 | 20783 | 0.36 | 0.00 | 1.05 |
| syn-references/ours.writeback | 100276 | 2.82 ms | 2.75 ms | 2.89 ms | 35.5 | 354 × 1 | 78477 | 7.60 | 2.30 | 1.09 |
| syn-references/pulldown.events | 100276 | 465.1 µs | 448.5 µs | 480.7 µs | 215.6 | 2137 × 1 | 17 | 1.32 | 0.74 | 1.06 |
| syn-references/pulldown.html | 100276 | 544.2 µs | 520.1 µs | 569.4 µs | 184.2 | 1820 × 1 | 32 | 1.58 | 0.87 | 1.04 |
| syn-tables/comrak.commonmark | 100121 | 2.19 ms | 2.14 ms | 2.24 ms | 45.7 | 456 × 1 | 99252 | 0.37 | 0.13 | 1.03 |
| syn-tables/comrak.html | 100121 | 2.10 ms | 2.03 ms | 2.15 ms | 47.7 | 475 × 1 | 21585 | 6.05 | 4.27 | 1.04 |
| syn-tables/comrak.parse | 100121 | 1.72 ms | 1.66 ms | 1.76 ms | 58.2 | 579 × 1 | 21562 | 5.52 | 4.12 | 1.05 |
| syn-tables/comrak.render | 100121 | 343.9 µs | 326.8 µs | 351.8 µs | 291.1 | 2893 × 1 | 23 | 0.53 | 0.26 | 1.05 |
| syn-tables/mdrs.html | 100121 | 116.66 ms | 115.35 ms | 116.75 ms | 0.9 | 10 × 1 | 164338 | 104.26 | 36.57 | 1.08 |
| syn-tables/mdrs.mdast | 100121 | 117.28 ms | 116.94 ms | 117.53 ms | 0.9 | 10 × 1 | 171623 | 109.86 | 36.57 | 1.12 |
| syn-tables/ours.canonical | 100121 | 910.3 µs | 889.0 µs | 930.5 µs | 110.0 | 1094 × 1 | 23169 | 1.54 | 0.20 | 1.08 |
| syn-tables/ours.definitions | 100121 | 75 ns | 68 ns | 76 ns | 1335664.0 | 5000 × 2284 | 0 | 0.00 | 0.00 | 1.03 |
| syn-tables/ours.editor_parse | 100121 | 3.10 ms | 3.02 ms | 3.18 ms | 32.3 | 322 × 1 | 73879 | 12.42 | 6.07 | 1.06 |
| syn-tables/ours.html | 100121 | 3.71 ms | 3.64 ms | 3.78 ms | 27.0 | 270 × 1 | 85561 | 9.83 | 4.33 | 1.12 |
| syn-tables/ours.inline_largest | 43 | 657 ns | 609 ns | 684 ns | 65.4 | 5000 × 294 | 15 | 0.00 | 0.00 | 1.07 |
| syn-tables/ours.inline_leaves | 100121 | 1.66 ms | 1.60 ms | 1.70 ms | 60.4 | 602 × 1 | 45973 | 5.18 | 0.00 | 1.04 |
| syn-tables/ours.leaf_sources | 100121 | 1.01 ms | 982.8 µs | 1.04 ms | 99.0 | 984 × 1 | 26307 | 5.61 | 2.42 | 1.05 |
| syn-tables/ours.parse | 100121 | 2.06 ms | 2.01 ms | 2.12 ms | 48.7 | 484 × 1 | 47572 | 6.81 | 3.82 | 1.08 |
| syn-tables/ours.render | 100121 | 1.57 ms | 1.52 ms | 1.62 ms | 63.6 | 634 × 1 | 37989 | 3.03 | 0.69 | 1.08 |
| syn-tables/ours.validate | 100121 | 147.6 µs | 138.4 µs | 154.2 µs | 678.3 | 3369 × 2 | 354 | 0.00 | 0.00 | 1.08 |
| syn-tables/ours.writeback | 100121 | 2.57 ms | 2.48 ms | 2.65 ms | 39.0 | 388 × 1 | 52896 | 7.55 | 3.82 | 1.13 |
| syn-tables/pulldown.events | 100121 | 376.6 µs | 363.7 µs | 392.4 µs | 265.9 | 2630 × 1 | 437 | 2.44 | 1.30 | 1.06 |
| syn-tables/pulldown.html | 100121 | 491.4 µs | 474.0 µs | 511.2 µs | 203.8 | 2020 × 1 | 453 | 3.03 | 1.59 | 1.05 |
| mixed-1k/comrak.commonmark | 1344 | 28.0 µs | 26.6 µs | 28.8 µs | 48.0 | 4441 × 8 | 1372 | 0.01 | 0.00 | 1.01 |
| mixed-1k/comrak.html | 1344 | 10.7 µs | 10.4 µs | 10.9 µs | 125.1 | 4871 × 19 | 177 | 0.04 | 0.04 | 1.02 |
| mixed-1k/comrak.parse | 1344 | 8.4 µs | 8.0 µs | 8.6 µs | 159.5 | 4919 × 24 | 161 | 0.04 | 0.03 | 1.03 |
| mixed-1k/comrak.render | 1344 | 2.0 µs | 1.9 µs | 2.0 µs | 680.6 | 4949 × 102 | 16 | 0.01 | 0.00 | 1.05 |
| mixed-1k/mdrs.html | 1344 | 88.5 µs | 85.1 µs | 89.8 µs | 15.2 | 3753 × 3 | 770 | 0.51 | 0.13 | 1.03 |
| mixed-1k/mdrs.mdast | 1344 | 88.7 µs | 87.5 µs | 90.3 µs | 15.1 | 3742 × 3 | 786 | 0.54 | 0.13 | 1.03 |
| mixed-1k/ours.canonical | 1344 | 8.6 µs | 7.8 µs | 8.9 µs | 157.2 | 4753 × 24 | 176 | 0.03 | 0.00 | 1.02 |
| mixed-1k/ours.definitions | 1344 | 21 ns | 20 ns | 23 ns | 63256.4 | 5000 × 6801 | 0 | 0.00 | 0.00 | 1.05 |
| mixed-1k/ours.editor_parse | 1344 | 26.1 µs | 23.7 µs | 27.3 µs | 51.4 | 4739 × 8 | 509 | 0.17 | 0.05 | 1.05 |
| mixed-1k/ours.html | 1344 | 32.4 µs | 30.2 µs | 33.4 µs | 41.4 | 4369 × 7 | 691 | 0.15 | 0.04 | 1.05 |
| mixed-1k/ours.inline_largest | 191 | 3.4 µs | 3.1 µs | 3.4 µs | 56.9 | 5000 × 59 | 72 | 0.02 | 0.01 | 1.03 |
| mixed-1k/ours.inline_leaves | 1344 | 16.6 µs | 15.2 µs | 17.0 µs | 80.8 | 4991 × 12 | 314 | 0.07 | 0.01 | 1.06 |
| mixed-1k/ours.leaf_sources | 1344 | 5.4 µs | 5.1 µs | 5.7 µs | 250.7 | 4824 × 38 | 122 | 0.06 | 0.02 | 1.04 |
| mixed-1k/ours.parse | 1344 | 20.5 µs | 18.9 µs | 21.4 µs | 65.5 | 4832 × 10 | 387 | 0.11 | 0.04 | 1.05 |
| mixed-1k/ours.render | 1344 | 11.8 µs | 10.8 µs | 12.4 µs | 114.0 | 4936 × 17 | 304 | 0.04 | 0.01 | 1.03 |
| mixed-1k/ours.validate | 1344 | 2.8 µs | 2.5 µs | 2.9 µs | 488.5 | 4959 × 72 | 50 | 0.00 | 0.00 | 1.03 |
| mixed-1k/ours.writeback | 1344 | 27.9 µs | 25.7 µs | 29.8 µs | 48.2 | 4206 × 8 | 535 | 0.13 | 0.04 | 1.04 |
| mixed-1k/pulldown.events | 1344 | 3.0 µs | 2.9 µs | 3.1 µs | 445.1 | 4995 × 66 | 11 | 0.02 | 0.02 | 1.03 |
| mixed-1k/pulldown.html | 1344 | 3.9 µs | 3.8 µs | 4.0 µs | 343.7 | 4998 × 51 | 18 | 0.02 | 0.02 | 1.02 |
| mixed-10k/comrak.commonmark | 11956 | 233.5 µs | 225.1 µs | 236.9 µs | 51.2 | 4264 × 1 | 11583 | 0.05 | 0.02 | 1.03 |
| mixed-10k/comrak.html | 11956 | 83.1 µs | 79.2 µs | 84.3 µs | 143.9 | 4008 × 3 | 1075 | 0.34 | 0.28 | 1.02 |
| mixed-10k/comrak.parse | 11956 | 69.6 µs | 62.5 µs | 70.4 µs | 171.8 | 4791 × 3 | 1060 | 0.30 | 0.27 | 1.01 |
| mixed-10k/comrak.render | 11956 | 12.7 µs | 11.8 µs | 12.8 µs | 944.1 | 4932 × 16 | 15 | 0.04 | 0.02 | 1.03 |
| mixed-10k/mdrs.html | 11956 | 811.0 µs | 798.6 µs | 822.2 µs | 14.7 | 1230 × 1 | 5619 | 4.32 | 1.00 | 1.04 |
| mixed-10k/mdrs.mdast | 11956 | 825.0 µs | 810.7 µs | 837.3 µs | 14.5 | 1208 × 1 | 5786 | 4.52 | 1.00 | 1.05 |
| mixed-10k/ours.canonical | 11956 | 83.2 µs | 75.3 µs | 84.5 µs | 143.7 | 3997 × 3 | 1362 | 0.36 | 0.04 | 1.03 |
| mixed-10k/ours.definitions | 11956 | 186 ns | 177 ns | 191 ns | 64386.8 | 5000 × 1036 | 0 | 0.00 | 0.00 | 1.04 |
| mixed-10k/ours.editor_parse | 11956 | 243.6 µs | 230.2 µs | 247.0 µs | 49.1 | 4089 × 1 | 4223 | 1.34 | 0.42 | 1.04 |
| mixed-10k/ours.html | 11956 | 305.8 µs | 286.6 µs | 310.3 µs | 39.1 | 3266 × 1 | 5780 | 1.37 | 0.29 | 1.03 |
| mixed-10k/ours.inline_largest | 530 | 9.2 µs | 8.3 µs | 9.3 µs | 57.7 | 4940 × 22 | 105 | 0.03 | 0.02 | 1.03 |
| mixed-10k/ours.inline_leaves | 11956 | 161.0 µs | 144.5 µs | 164.1 µs | 74.2 | 3091 × 2 | 2873 | 0.50 | 0.02 | 1.05 |
| mixed-10k/ours.leaf_sources | 11956 | 47.4 µs | 44.8 µs | 47.9 µs | 252.2 | 4222 × 5 | 912 | 0.49 | 0.18 | 1.03 |
| mixed-10k/ours.parse | 11956 | 195.3 µs | 188.2 µs | 199.3 µs | 61.2 | 2552 × 2 | 3311 | 0.86 | 0.26 | 1.05 |
| mixed-10k/ours.render | 11956 | 106.3 µs | 95.8 µs | 109.6 µs | 112.4 | 4681 × 2 | 2469 | 0.51 | 0.05 | 1.03 |
| mixed-10k/ours.validate | 11956 | 25.9 µs | 23.5 µs | 26.1 µs | 461.4 | 4834 × 8 | 391 | 0.02 | 0.00 | 1.02 |
| mixed-10k/ours.writeback | 11956 | 276.7 µs | 256.8 µs | 280.7 µs | 43.2 | 3610 × 1 | 4801 | 1.03 | 0.26 | 1.03 |
| mixed-10k/pulldown.events | 11956 | 26.7 µs | 24.1 µs | 27.0 µs | 448.4 | 4671 × 8 | 41 | 0.14 | 0.08 | 1.03 |
| mixed-10k/pulldown.html | 11956 | 33.1 µs | 30.4 µs | 33.7 µs | 361.3 | 5000 × 6 | 53 | 0.18 | 0.10 | 1.02 |
| mixed-100k/comrak.commonmark | 101484 | 2.03 ms | 2.00 ms | 2.05 ms | 49.9 | 491 × 1 | 97121 | 0.38 | 0.14 | 1.02 |
| mixed-100k/comrak.html | 101484 | 920.3 µs | 884.6 µs | 949.3 µs | 110.3 | 1083 × 1 | 10482 | 2.90 | 2.28 | 1.08 |
| mixed-100k/comrak.parse | 101484 | 748.8 µs | 713.0 µs | 764.5 µs | 135.5 | 1329 × 1 | 10446 | 2.54 | 2.19 | 1.05 |
| mixed-100k/comrak.render | 101484 | 142.8 µs | 134.0 µs | 147.1 µs | 710.9 | 3476 × 2 | 36 | 0.36 | 0.18 | 1.04 |
| mixed-100k/mdrs.html | 101484 | 9.02 ms | 8.91 ms | 9.10 ms | 11.3 | 111 × 1 | 60328 | 50.88 | 12.49 | 1.14 |
| mixed-100k/mdrs.mdast | 101484 | 9.13 ms | 9.04 ms | 9.23 ms | 11.1 | 110 × 1 | 62160 | 53.05 | 12.49 | 1.11 |
| mixed-100k/ours.canonical | 101484 | 693.3 µs | 638.6 µs | 704.7 µs | 146.4 | 1443 × 1 | 12055 | 2.31 | 0.17 | 1.05 |
| mixed-100k/ours.definitions | 101484 | 2.3 µs | 2.1 µs | 2.4 µs | 44635.6 | 4882 × 90 | 27 | 0.00 | 0.00 | 1.03 |
| mixed-100k/ours.editor_parse | 101484 | 2.19 ms | 2.13 ms | 2.23 ms | 46.3 | 456 × 1 | 36975 | 11.42 | 3.60 | 1.07 |
| mixed-100k/ours.html | 101484 | 2.81 ms | 2.76 ms | 2.85 ms | 36.1 | 355 × 1 | 50684 | 10.84 | 2.51 | 1.06 |
| mixed-100k/ours.inline_largest | 749 | 12.8 µs | 11.5 µs | 13.0 µs | 58.4 | 4858 × 16 | 135 | 0.06 | 0.03 | 1.04 |
| mixed-100k/ours.inline_leaves | 101484 | 1.47 ms | 1.44 ms | 1.49 ms | 69.2 | 681 × 1 | 25727 | 4.57 | 0.03 | 1.06 |
| mixed-100k/ours.leaf_sources | 101484 | 437.0 µs | 415.6 µs | 444.2 µs | 232.2 | 2285 × 1 | 8355 | 4.02 | 1.51 | 1.04 |
| mixed-100k/ours.parse | 101484 | 1.75 ms | 1.71 ms | 1.76 ms | 58.1 | 573 × 1 | 28620 | 7.41 | 2.40 | 1.09 |
| mixed-100k/ours.render | 101484 | 1.01 ms | 973.1 µs | 1.03 ms | 100.2 | 987 × 1 | 22064 | 3.43 | 0.42 | 1.05 |
| mixed-100k/ours.validate | 101484 | 218.6 µs | 200.4 µs | 221.7 µs | 464.3 | 4563 × 1 | 3195 | 0.16 | 0.00 | 1.03 |
| mixed-100k/ours.writeback | 101484 | 2.37 ms | 2.32 ms | 2.40 ms | 42.8 | 422 × 1 | 38303 | 8.48 | 2.40 | 1.09 |
| mixed-100k/pulldown.events | 101484 | 264.7 µs | 251.0 µs | 270.9 µs | 383.4 | 3757 × 1 | 319 | 1.25 | 0.71 | 1.05 |
| mixed-100k/pulldown.html | 101484 | 340.5 µs | 325.2 µs | 347.2 µs | 298.0 | 2926 × 1 | 361 | 1.60 | 0.88 | 1.05 |
| mixed-1m/comrak.commonmark | 1022808 | 20.07 ms | 19.99 ms | 20.32 ms | 51.0 | 50 × 1 | 970702 | 3.24 | 1.08 | 1.09 |
| mixed-1m/comrak.html | 1022808 | 10.31 ms | 10.12 ms | 10.40 ms | 99.2 | 97 × 1 | 101563 | 25.59 | 19.35 | 1.04 |
| mixed-1m/comrak.parse | 1022808 | 8.64 ms | 8.46 ms | 8.71 ms | 118.4 | 116 × 1 | 101399 | 21.16 | 17.82 | 1.08 |
| mixed-1m/comrak.render | 1022808 | 1.58 ms | 1.49 ms | 1.60 ms | 645.7 | 631 × 1 | 164 | 4.44 | 2.21 | 1.10 |
| mixed-1m/mdrs.html | 1022808 | 178.01 ms | 176.44 ms | 178.53 ms | 5.7 | 10 × 1 | 575644 | 465.21 | 108.09 | 1.16 |
| mixed-1m/mdrs.mdast | 1022808 | 179.55 ms | 177.94 ms | 179.94 ms | 5.7 | 10 × 1 | 594512 | 484.56 | 108.09 | 1.15 |
| mixed-1m/ours.canonical | 1022808 | 7.41 ms | 7.31 ms | 7.48 ms | 138.0 | 135 × 1 | 118302 | 22.80 | 1.38 | 1.08 |
| mixed-1m/ours.definitions | 1022808 | 35.0 µs | 31.7 µs | 36.9 µs | 29228.9 | 4670 × 6 | 385 | 0.03 | 0.01 | 1.10 |
| mixed-1m/ours.editor_parse | 1022808 | 24.48 ms | 24.18 ms | 24.80 ms | 41.8 | 41 × 1 | 360654 | 115.64 | 35.82 | 1.16 |
| mixed-1m/ours.html | 1022808 | 30.09 ms | 29.78 ms | 30.34 ms | 34.0 | 34 × 1 | 492243 | 107.75 | 24.13 | 1.16 |
| mixed-1m/ours.inline_largest | 741 | 11.1 µs | 9.8 µs | 11.5 µs | 66.6 | 4971 × 18 | 84 | 0.03 | 0.01 | 1.04 |
| mixed-1m/ours.inline_leaves | 1022808 | 14.79 ms | 14.54 ms | 14.85 ms | 69.2 | 68 × 1 | 245741 | 43.07 | 0.03 | 1.07 |
| mixed-1m/ours.leaf_sources | 1022808 | 4.81 ms | 4.70 ms | 4.90 ms | 212.6 | 208 × 1 | 82132 | 42.77 | 15.91 | 1.29 |
| mixed-1m/ours.parse | 1022808 | 18.77 ms | 18.60 ms | 18.98 ms | 54.5 | 54 × 1 | 278522 | 72.87 | 23.51 | 1.19 |
| mixed-1m/ours.render | 1022808 | 10.91 ms | 10.75 ms | 10.99 ms | 93.8 | 92 × 1 | 213721 | 34.88 | 4.23 | 1.20 |
| mixed-1m/ours.validate | 1022808 | 2.42 ms | 2.31 ms | 2.46 ms | 421.9 | 412 × 1 | 32834 | 1.53 | 0.00 | 1.17 |
| mixed-1m/ours.writeback | 1022808 | 26.05 ms | 25.73 ms | 26.29 ms | 39.3 | 39 × 1 | 378481 | 83.92 | 23.52 | 1.17 |
| mixed-1m/pulldown.events | 1022808 | 2.97 ms | 2.90 ms | 3.03 ms | 344.8 | 336 × 1 | 2790 | 12.56 | 7.04 | 1.08 |
| mixed-1m/pulldown.html | 1022808 | 3.82 ms | 3.77 ms | 3.85 ms | 267.6 | 262 × 1 | 2981 | 16.76 | 9.14 | 1.09 |
