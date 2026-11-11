# markdown-syntax-wasm vs JavaScript Markdown parsers — baseline

What a parse, a write-back, and a keystroke cost a web editor that uses
`markdown-syntax-wasm` through `wasm/js/index.js`, measured against the
mainstream JavaScript parsers. Raw numbers: `wasm-baseline.json` next to
this file (cases, per-stage breakdowns, profile split, sizes, cold start,
memory).

## Environment

| | |
|---|---|
| CPU | Apple M4 (10 cores: 4 performance + 6 efficiency) |
| OS | macOS 27.0.1 |
| Runtime | Node v24.21.0, V8 13.6.233.17 (not Node 22, not WKWebView/JavaScriptCore) |
| Ours | commit `347b5d4`, `cargo build -p markdown-syntax-wasm --target wasm32-unknown-unknown --profile wasm` (rustc 1.95.0), 800,111 bytes, sha256 `e0309407…f7f7` |
| Peers | markdown-it 15.0.3, marked 18.1.2, commonmark 0.31.2, micromark 4.0.3 + micromark-extension-gfm 3.0.0, mdast-util-from-markdown 2.1.0, mdast-util-gfm 3.1.0, mdast-util-to-markdown 2.2.1, @lezer/markdown 1.8.1 + @lezer/common 1.5.4, markdown-wasm 1.2.0, esbuild 0.28.2 (sizes only) |
| Load | Other agents ran a native Rust bench on one core during the run (load average 3–5) |

## Method

- Corpus: `tools/bench/corpus/out/*.md` from `gen.mjs` (mixed-1k…1m, three
  real documents, twelve ~100 KB synthetic files, one construct each). MB/s
  counts UTF-8 bytes.
- Each case: warmup (≥3 calls, up to 300 ms), then 3 rounds of ≥400 ms and
  ≥10 samples (≥5 when a call takes >100 ms), `global.gc()` before each
  round. A call under 0.5 ms is batched and the sample is the batch mean.
  Reported: median and min over all samples, and **spread** = (max − min of
  the 3 round medians) / median.
- Subjects (each produces a different thing; see "What each one returns"):
  - **ours**: `binding.parse(text).free()` — tree, diagnostics, and
    leafSources as JS objects, document kept in wasm until `free()`.
  - **markdown-it** `md.parse(text, {})`, default preset (CommonMark + tables
    + strikethrough), tokens with inline children.
  - **marked** `marked.lexer(text)`, gfm on, tokens with inline tokens.
  - **commonmark** `new Parser().parse(text)` — CommonMark only, **no GFM**:
    tables, strikethrough, etc. stay paragraphs, so it does less work on
    syn-tables and anything GFM.
  - **mdast** `fromMarkdown(text, {gfm})` — the remark/Milkdown parse path.
  - **lezer** `parser.configure(GFM).parse(text)` — the CodeMirror parser; its
    `Tree` holds node types and positions in compact buffers, no text or
    attributes.
  - **markdown-wasm** `parse(text)` — md4c in wasm, **HTML string only, no
    tree**: a floor for "C parser in wasm", not a like-for-like peer.
- "Fastest tree peer" excludes markdown-wasm.
- Breakdown: the bench imports a copy of `wasm/js/index.js` with
  `Reader`/`Writer`/`encodeNode` exported (a `data:` URL; the file is not
  edited) and times each stage of the same pipeline `parse()` runs, in one
  loop.
- Inside wasm: a separate process runs the `parse` export in a loop under
  `node --cpu-prof` (100 µs sampling); each sample is attributed to the
  first function under the export that matches a part (core parse, core
  `leaf_sources`, `write_inline_source`, `wire::Writer`, `utf16_offsets`).
  Shares, not absolute times; inlining may move a little time between parts.

## Results

### 1. Full parse into a usable tree (median, MB/s in parentheses)

| file | size | ours | markdown-it | marked | commonmark | mdast | lezer | markdown-wasm (HTML) |
|---|---|---|---|---|---|---|---|---|
| mixed-1k | 1.3 KB | 64 µs (21) | 51 µs (26) | 48 µs (28) | 14 µs (97) | 476 µs (2.8) | 79 µs (17) | 6.7 µs (202) |
| mixed-10k | 11.7 KB | 611 µs (20) | 412 µs (29) | 487 µs (25) | 124 µs (96) | 5.14 ms (2.3) | 798 µs (15) | 52 µs (228) |
| mixed-100k | 99 KB | 5.69 ms (18) | 4.43 ms (23) | 4.49 ms (23) | 1.23 ms (83) | 54.6 ms (1.9) | 7.28 ms (14) | 0.78 ms (129) |
| mixed-1m | 999 KB | 60.0 ms (17) | 50.3 ms (20) | 46.2 ms (22) | 16.0 ms (64) | 650 ms (1.6) | 71.4 ms (14) | 8.1 ms (126) |
| real-docs | 680 KB | 36.1 ms (19) | 21.4 ms (33) | 28.0 ms (25) | 9.1 ms (77) | 403 ms (1.7) | 54.0 ms (13) | 4.5 ms (154) |
| real-rust-releases | 736 KB | 41.3 ms (18) | 35.9 ms (21) | 30.7 ms (25) | 17.6 ms (43) | 535 ms (1.4) | 40.1 ms (19) | 7.6 ms (99) |
| real-spec | 200 KB | 6.50 ms (32) | 3.81 ms (54) | 3.77 ms (54) | 2.17 ms (95) | 86.3 ms (2.4) | 10.1 ms (20) | 1.27 ms (162) |
| syn-cjk | 269 KB | 6.48 ms (42) | 4.09 ms (67) | 3.86 ms (71) | 1.12 ms (245) | 33.5 ms (8.2) | 8.09 ms (34) | 1.03 ms (267) |
| syn-code | 98 KB | 1.01 ms (99) | 0.36 ms (279) | 0.18 ms (547) | 0.51 ms (196) | 15.2 ms (6.6) | 0.34 ms (298) | 0.24 ms (420) |
| syn-directives | 98 KB | 5.35 ms (19) | 3.72 ms (27) | 2.98 ms (34) | 0.90 ms (112) | 44.6 ms (2.2) | 7.72 ms (13) | 0.50 ms (200) |
| syn-emphasis | 98 KB | 11.8 ms (8.5) | 23.4 ms (4.3) | 19.1 ms (5.2) | 4.92 ms (20) | 130 ms (0.8) | 11.3 ms (8.9) | 2.44 ms (41) |
| syn-headings | 98 KB | 4.93 ms (20) | 4.40 ms (23) | 2.49 ms (40) | 1.77 ms (57) | 49.3 ms (2.0) | 7.40 ms (14) | 0.99 ms (101) |
| syn-html | 98 KB | 3.86 ms (26) | 3.50 ms (29) | 1.90 ms (53) | 0.80 ms (125) | 29.2 ms (3.4) | 4.52 ms (22) | 0.39 ms (256) |
| syn-lists | 99 KB | 5.31 ms (19) | 3.94 ms (26) | 4.82 ms (21) | 1.31 ms (77) | 54.1 ms (1.9) | 7.55 ms (13) | 0.55 ms (183) |
| syn-paragraphs | 98 KB | 5.01 ms (20) | 3.31 ms (30) | 2.92 ms (34) | 0.84 ms (120) | 40.9 ms (2.4) | 7.64 ms (13) | 0.51 ms (197) |
| syn-quotes | 98 KB | 5.66 ms (18) | 3.48 ms (29) | 3.98 ms (25) | 0.90 ms (112) | 52.7 ms (1.9) | 7.99 ms (13) | 0.52 ms (195) |
| syn-references | 98 KB | 5.29 ms (19) | 3.55 ms (28) | 3.36 ms (30) | 1.87 ms (54) | 67.3 ms (1.5) | 5.21 ms (19) | 0.79 ms (127) |
| syn-tables | 98 KB | 7.35 ms (14) | 9.56 ms (11) | 4.08 ms (25) | 0.81 ms (123)¹ | 89.2 ms (1.1) | 6.59 ms (15) | 1.38 ms (72) |

¹ commonmark.js has no GFM tables; it parses these lines as paragraphs.

Ours ÷ each peer (median ratio; above 1 means ours is slower):

| file | markdown-it | marked | commonmark | mdast | lezer | markdown-wasm |
|---|---|---|---|---|---|---|
| mixed-1k | 1.25 | 1.32 | 4.61 | 0.13 | 0.80 | 9.6 |
| mixed-10k | 1.48 | 1.25 | 4.93 | 0.12 | 0.77 | 11.6 |
| mixed-100k | 1.29 | 1.27 | 4.64 | 0.10 | 0.78 | 7.3 |
| mixed-1m | 1.19 | 1.30 | 3.75 | 0.09 | 0.84 | 7.4 |
| real-docs | 1.68 | 1.29 | 3.96 | 0.09 | 0.67 | 8.0 |
| real-rust-releases | 1.15 | 1.35 | 2.35 | 0.08 | 1.03 | 5.4 |
| real-spec | 1.71 | 1.73 | 3.00 | 0.08 | 0.64 | 5.1 |
| syn-cjk | 1.58 | 1.68 | 5.76 | 0.19 | 0.80 | 6.3 |
| syn-code | 2.81 | 5.51 | 1.98 | 0.07 | 3.00 | 4.2 |
| syn-directives | 1.44 | 1.80 | 5.96 | 0.12 | 0.69 | 10.7 |
| syn-emphasis | **0.51** | **0.62** | 2.40 | 0.09 | 1.05 | 4.9 |
| syn-headings | 1.12 | 1.98 | 2.79 | 0.10 | 0.67 | 5.0 |
| syn-html | 1.10 | 2.02 | 4.79 | 0.13 | 0.85 | 9.8 |
| syn-lists | 1.35 | 1.10 | 4.05 | 0.10 | 0.70 | 9.6 |
| syn-paragraphs | 1.52 | 1.72 | 6.01 | 0.12 | 0.66 | 9.9 |
| syn-quotes | 1.62 | 1.42 | 6.28 | 0.11 | 0.71 | 10.9 |
| syn-references | 1.49 | 1.58 | 2.82 | 0.08 | 1.02 | 6.7 |
| syn-tables | **0.77** | 1.80 | 9.03¹ | 0.08 | 1.11 | 5.3 |

Ours runs at 17–20 MB/s on mixed and real documents (8–99 MB/s across the
synthetic files). Against markdown-it and marked it is 1.1–2× slower, mostly
1.2–1.7×, and 2.8–5.5× slower on code blocks; 2.4–6.3× slower than
commonmark.js; 5–11× slower than md4c (HTML only); 8–13× faster than mdast.
Against lezer's full parse it is faster on mixed, real-docs, real-spec, and
most synthetic files (0.64–0.85×), even on rust-releases, emphasis, and
references (1.02–1.05×), and slower on tables (1.11×) and code (3×).

### 2. Our parse pipeline, stage by stage

Medians of each stage, timed together in one loop (the sum is within a few
percent of the end-to-end `parse` in section 1).

| file | total | wasm `parse` export | transfer² | JS decode³ | of which: tree / leafSources+diag / Reader init | `free()` | wire bytes ÷ source bytes |
|---|---|---|---|---|---|---|---|
| mixed-1k | 64 µs | 52 µs (82%) | 1.5 µs (2%) | 8.3 µs (13%) | 6.9% / 1.2% / 4.5% | 2.2% | 5.5× |
| mixed-10k | 580 µs | 499 µs (86%) | 6.3 µs (1%) | 62 µs (11%) | 6.2% / 1.1% / 3.4% | 2.0% | 5.7× |
| mixed-100k | 5.81 ms | 4.75 ms (82%) | 167 µs (3%) | 643 µs (11%) | 5.9% / 1.3% / 3.9% | 3.7% | 5.8× |
| mixed-1m | 58.5 ms | 47.8 ms (84%) | 1.56 ms (3%) | 5.71 ms (10%) | 5.5% / 1.2% / 3.2% | 3.8% | 5.6× |
| real-docs | 37.2 ms | 31.4 ms (85%) | 1.09 ms (3%) | 3.24 ms (9%) | 4.4% / 1.0% / 3.4% | 3.1% | 5.5× |
| real-rust-releases | 42.3 ms | 36.0 ms (85%) | 1.16 ms (3%) | 3.67 ms (9%) | 4.7% / 1.0% / 3.0% | 3.2% | 5.3× |
| real-spec | 6.39 ms | 5.42 ms (85%) | 296 µs (5%) | 534 µs (8%) | 3.6% / 0.8% / 3.9% | 2.2% | 3.3× |
| syn-cjk | 6.31 ms | 5.32 ms (85%) | 278 µs (4%) | 534 µs (9%) | 4.4% / 0.8% / 3.3% | 2.6% | 2.1× |
| syn-code | 1.01 ms | 896 µs (89%) | 26 µs (3%) | 87 µs (9%) | 1.2% / 0.0% / 7.4% | 0.3% | 2.1× |
| syn-emphasis | 12.6 ms | 8.52 ms (68%) | 94 µs (1%) | 2.67 ms (21%) | 18.0% / 0.4% / 2.9% | 9.4% | 15.3× |
| syn-headings | 5.03 ms | 4.22 ms (84%) | 43 µs (1%) | 671 µs (13%) | 5.4% / 4.3% / 3.6% | 1.9% | 6.3× |
| syn-tables | 7.47 ms | 5.73 ms (77%) | 65 µs (1%) | 1.26 ms (17%) | 8.1% / 4.8% / 4.0% | 4.9% | 9.0× |

(All 18 files are in the JSON; the rest look like mixed-100k.)

² `new TextEncoder().encode` of the source + copy into wasm memory + copy of
the result buffer out (`result()`).
³ `new Reader` (a second copy of the result, `TextDecoder('utf-16le')` of the
string table, one `slice` per string) + building the tree objects +
diagnostics and leafSources + the `ParsedDocument`.

The `Reader` constructor on mixed-1m: copy 0.23 ms, UTF-16 decode 1.06 ms,
58,146 string slices 0.81 ms.

### 2b. Inside the wasm `parse` export (CPU profile shares)

| part | mixed-1m | real-docs | syn-cjk | syn-paragraphs |
|---|---|---|---|---|
| core parse (`markdown_syntax::parse`) | 47.7% | 40.9% | 49.6% | 51.9% |
| wire: leaf source maps (`write_inline_source`) | 25.9% | 34.9% | 26.7% | 29.6% |
| core `leaf_sources` | 11.5% | 9.4% | 5.9% | 4.3% |
| wire: tree + diagnostics (`wire::Writer`) | 9.6% | 9.1% | 8.0% | 9.2% |
| wire: UTF-16 offset table (`utf16_offsets`) | 3.1% | 3.3% | 4.6% | 3.8% |
| other (allocator, glue) | 2.3% | 2.4% | 5.1% | 1.3% |
| `document_free`, relative to `parse` | 4.6% | 3.4% | 2.9% | 3.3% |

So only 41–52% of the export is the core parser; leafSources (core
`leaf_sources` + their wire maps) take 34–44%, and the tree encoding plus
offset table 12–13%. The hottest single function in the mixed-1m profile is
`source_map::Translator::span` (14% self time), reached from
`write_inline_source`, which maps every UTF-16 offset of every leaf through
`to_source` and `span_to_source` one at a time.

Cross-check, not measured by this bench: the native Rust bench another agent
was running at the same time (`tools/bench/results/run1.json`, in progress,
not verified here) reports `ours parse` at 22.1 ms on mixed-1m and 11.4 ms on
real-docs; the profile puts core parse inside wasm at ~22.8 ms and ~12.9 ms.
If those hold, wasm itself costs the core parser little (~1.0–1.15×); the
export's extra work is what doubles it.

### 3. Write-back (tree → Markdown)

| file | ours: untouched tree | ours: one leaf's text edited | ours: every leaf as source | mdast-util-to-markdown + gfm | (ours parse, for scale) |
|---|---|---|---|---|---|
| mixed-10k | 584 µs | 942 µs | 920 µs | 2.23 ms | 611 µs |
| mixed-100k | 5.51 ms | 8.60 ms | 10.55 ms | 22.6 ms | 5.69 ms |
| mixed-1m | 55.7 ms | 86.0 ms | 108 ms | 217 ms | 60.0 ms |
| real-docs | 33.7 ms | 48.9 ms | 50.3 ms | 138 ms | 36.1 ms |
| real-spec | 5.76 ms | 9.52 ms | 8.98 ms | 18.5 ms | 6.50 ms |
| syn-lists | 5.66 ms | 8.65 ms | 8.76 ms | 22.1 ms | 5.31 ms |
| syn-tables | 6.70 ms | 11.0 ms | 11.3 ms | 27.9 ms | 7.35 ms |

"One leaf's text edited": the middle paragraph (or a table cell in
syn-tables) has its first text node's value changed; the output was checked
to contain the edit. Untouched writes were checked to equal the source.
mdast-util-to-markdown writes canonical Markdown (it does not keep source
bytes), so it does a different job; it is the write path a remark-based
editor would use.

Where an untouched write spends its time (mixed-1m: 57.0 ms): JS
`encodeNode` + `Writer.finish` 16.1 ms (28%), wasm `to_markdown` (decode the
wire, compare, write) 40.5 ms (71%), copies and UTF-8 decode <1%. With one
leaf edited: 17.1 ms JS (19%), 71.9 ms wasm (80%). The same shares hold on
every file measured (JS encode 28–33% untouched, 19–21% edited).

An untouched write costs about as much as a full parse; writing a document
in which one paragraph changed costs ~1.5× a parse.

### 4. Keystroke: one character typed in the middle, then reparse

The character goes into the paragraph nearest the middle of the document,
between two word characters. Checks: lezer's incremental tree equals its full
parse; ours `parseInline` of the edited paragraph equals that paragraph's
inlines in a full parse (spans aside).

| doc | reparse | median | p90 | share of a 16.7 ms frame |
|---|---|---|---|---|
| 10 KB | ours: full `parse` + `free` | 559 µs | 599 µs | 3.4% |
| 10 KB | ours: `parseInline` of the edited paragraph (336 chars) | 16 µs | 17 µs | 0.1% |
| 10 KB | lezer incremental (`TreeFragment.applyChanges` + `parse`) | 97 µs | 114 µs | 0.6% |
| 10 KB | lezer full parse | 918 µs | 1.05 ms | 5.5% |
| 10 KB | markdown-it full parse | 423 µs | 497 µs | 2.5% |
| 10 KB | mdast full parse | 4.98 ms | 5.46 ms | 30% |
| 100 KB | ours: full `parse` + `free` | 5.42 ms | 5.63 ms | 32% |
| 100 KB | ours: `parseInline` of the edited paragraph (197 chars) | 11 µs | 12 µs | 0.1% |
| 100 KB | lezer incremental | 31 µs | 35 µs | 0.2% |
| 100 KB | lezer full parse | 7.36 ms | 8.58 ms | 44% |
| 100 KB | markdown-it full parse | 4.29 ms | 5.11 ms | 26% |
| 100 KB | mdast full parse | 53.9 ms | 71.0 ms | 323% |
| 1 MB | ours: full `parse` + `free` | 56.2 ms | 59.0 ms | **337%** |
| 1 MB | ours: `parseInline` of the edited paragraph (276 chars) | 113 µs | 119 µs | 0.7% |
| 1 MB | ours: same, no definitions passed | 8.5 µs | 8.9 µs | 0.1% |
| 1 MB | lezer incremental | 187 µs | 207 µs | 1.1% |
| 1 MB | lezer full parse | 73.6 ms | 91.8 ms | 442% |
| 1 MB | markdown-it full parse | 47.6 ms | 66.7 ms | 286% |
| 1 MB | mdast full parse | 699 ms | 714 ms | 4193% |

A full reparse per keystroke fits a frame up to roughly 100 KB for ours
(5.4 ms, a third of the frame before any editor work) and misses it 3.4× at
1 MB. Reparsing only the edited paragraph with `parseInline` costs 8–16 µs,
below lezer's incremental reparse, except that `parseInline` with the
document's definitions grows with their number: mixed-1m has 377 definition
labels and the call costs 113 µs instead of 8.5 µs (~0.25 µs per label,
re-encoded in JS and rebuilt as `Definitions` in wasm on every call). Lezer's
incremental parse handles any edit, including ones that change block
structure; `parseInline` is valid only while the edit stays inside one
leaf's inline content, which the editor must decide.

### 5. Module size and load

| module | raw | gzip -9 | brotli 11 |
|---|---|---|---|
| ours: `.wasm` as built | 781 KB | 285 KB | 220 KB |
| ours: `.wasm` without custom sections (name 42 KB, producers) | 739 KB | 273 KB | 207 KB |
| ours: `index.js`, minified | 13.8 KB | 4.7 KB | 4.2 KB |
| markdown-it (bundled, minified) | 97 KB | 41 KB | 36 KB |
| marked | 47 KB | 14 KB | 13 KB |
| commonmark.js | 156 KB | 47 KB | 39 KB |
| mdast-util-from-markdown + micromark GFM | 81 KB | 23 KB | 20 KB |
| mdast-util-to-markdown + GFM | 35 KB | 11 KB | 10 KB |
| @lezer/markdown + @lezer/common | 60 KB | 20 KB | 18 KB |
| markdown-wasm: `markdown.wasm` + `markdown.es.js` | 55 + 9 KB | 25 + 4 KB | 22 + 4 KB |

Peers are esbuild bundles of their entry points (minified, ESM, browser), so
dependencies are included and unused exports may be shaken out. Ours at
273–285 KB gzip is 6–14× each JS peer and ~10× md4c.

Load, in this process, with bytes made unique per load so V8's in-isolate
module cache does not hide the compile (15 loads):

| step | median | min |
|---|---|---|
| `WebAssembly.compile(bytes)` | 1.38 ms | 1.17 ms |
| `MarkdownSyntax.load(bytes)` (compile + instantiate) | 0.74 ms | 0.65 ms |
| first `parse` of mixed-100k on the fresh instance | 22.0 ms | 18.8 ms |
| second `parse` on it | 11.5 ms | 7.2 ms |
| steady state (long-lived instance) | 5.48 ms | — |

V8 compiles wasm lazily, so `load` is cheap and the cost lands on the first
parses: the first is 4× the steady state, the second 2×.

Cold start, a fresh `node` process per run that imports one subject, then
parses mixed-100k twice (median of 5):

| subject | import + load | first parse | second parse |
|---|---|---|---|
| ours (import index.js, read + compile + instantiate the .wasm) | 10.9 ms | 27.7 ms | 10.6 ms |
| markdown-it | 7.2 ms | 15.8 ms | 7.1 ms |
| marked | 5.7 ms | 30.5 ms | 7.6 ms |
| commonmark | 6.9 ms | 8.7 ms | 4.7 ms |
| mdast | 23.3 ms | 88.5 ms | 61.5 ms |
| lezer | 4.5 ms | 17.0 ms | 9.6 ms |
| markdown-wasm | 2.1 ms | 3.9 ms | 2.1 ms |

### 6. Memory after parsing mixed-1m

Fresh process per subject, result kept alive, `gc()` twice before each
reading; the parser is warmed on a tiny input first. Three runs each; they
agree to 0.1 MB.

| subject | JS heap Δ | wasm / external Δ | total |
|---|---|---|---|
| ours | 18.1 MB (tree 11.5 MB, leafSources 6.6 MB) | wasm memory 1.2 → 38.4 MB; 47.9 MB after a second parse; still 47.9 MB after 20 parse/free cycles | ~56 MB, ~66 MB from the second parse on |
| markdown-it | 18.0 MB | — | 18.0 MB |
| marked | 15.6 MB | — | 15.6 MB |
| commonmark | 21.9 MB | — | 21.9 MB |
| mdast | 16.5 MB | — | 16.5 MB |
| lezer | 14.5 MB | 0.4 MB | 14.9 MB |
| markdown-wasm | ~0 (HTML string not kept) | 2.5 MB | 2.5 MB |

WebAssembly memory never shrinks, so the peak stays committed for the
page's life. It plateaus (no growth from cycle 2 to 20), so this is a
high-water mark, not a leak. What fills it is inferred from the code, not
measured: the kept document (AST and source), its `to_utf16` table (4 bytes
per source byte, ~4 MB here, kept per document), the result buffer of the
last call (the wire, ~5.6× the source, kept until the next call), and the
parse's transient vectors.

### Variance

- A second full run of the timed sections (parse, breakdown, write,
  keystroke) against this baseline, same build: median |change| 2.2%, p90
  6.1%, max 17% (mdast on syn-references). Our own cases: median 2.2%, max
  8.7% (syn-emphasis). Round spreads within one run are mostly 0.5–5%,
  smaller than the run-to-run change, so `--compare` uses a 6% floor.
- Repeated in isolation (3 runs): mixed-100k ours 5.52 / 5.56 / 5.56 ms
  (full runs: 5.69, 5.81); syn-emphasis ours 11.9 / 12.1 / 12.2 ms (full runs:
  11.8, 12.8).
- JIT state moves the JS peers: lezer on syn-emphasis is 8.6–8.9 ms when run
  alone and 11.3–11.9 ms inside the full run, after it has seen the other
  files. The full-run order is fixed, so comparisons between runs of this
  bench are consistent; absolute peer numbers carry this caveat.
- Cold start and memory: a smoke run earlier gave ours 11.5 ms import+load
  and 30.2 ms first parse (vs 10.9 / 27.7 here); memory was identical.

## Gap analysis

**Full parse.** On documents that look like real editing content (mixed,
real-*, most syn-*), ours is 1.2–1.7× slower than markdown-it and marked and
2.4–6.3× slower than commonmark.js; md4c-in-wasm writes HTML 5–11× faster than
ours builds a tree. The exceptions: emphasis-heavy text, where ours is ~2×
faster than markdown-it/marked; tables, where it beats markdown-it; code
blocks, where it is 3–5.5× slower than marked/lezer (1 ms for 98 KB, of which
the wasm export is 0.9 ms).

**Where our time goes, for 1 MB of mixed Markdown (58.5 ms):**

1. wasm `parse` export, 47.8 ms (82–89% of a parse on every file but
   syn-emphasis and syn-tables), which splits into
   - core parser ~23 ms (41–52%),
   - leafSources ~18 ms (34–44%): core `leaf_sources` 9–12% plus
     `write_inline_source` 26–35%, which maps each UTF-16 offset of each leaf
     one at a time (`Translator::span` is the hottest function),
   - tree + diagnostics encoding ~4.6 ms (8–10%) and the UTF-16 offset table
     ~1.5 ms (3–5%).
2. JS decode 5.7 ms (9–11%; 21% on syn-emphasis, where the wire is 15× the
   source): tree objects 5.5%, Reader init 3.2% (a second full copy of the
   result plus a UTF-16 decode and 58k string slices), leafSources +
   diagnostics 1.2%.
3. `free()` 2.2 ms (3–4%).
4. Transfer 1.6 ms (2–3%): `TextEncoder` 1.3 ms, copies <0.3 ms.

String transfer is not where the gap is. If leafSources left the parse
(computed lazily through `inlineSource(path)`, which already exists) and
nothing else changed, the export would drop by about 34–44% and a parse of
mixed-1m to ~40 ms, ahead of markdown-it and marked (an estimate from the
profile shares, not measured). Beyond that, the core parser's own speed is
the floor: ~23 ms/MB in wasm, ~1.4× commonmark.js's whole parse.

**Write-back.** `toMarkdown` of an untouched tree costs a full parse (55.7 ms
on 1 MB), because the whole tree is re-encoded in JS (28%) and decoded and
compared in wasm (71%) even when nothing changed; one edited paragraph costs
~1.5× a parse. It is 1.9–3.9× faster than mdast-util-to-markdown, which does
a different (canonical) job. Saving per keystroke is affordable at 10 KB
(~0.9 ms) but not at 1 MB (86 ms).

**Keystroke.** Full reparse fits a frame to ~100 KB (5.4 ms) and is 3.4
frames at 1 MB. The local path (`parseInline` of the edited paragraph) is
8–16 µs, as cheap as lezer's incremental reparse or cheaper, except that
passing the document's definitions adds ~0.25 µs per label per call (113 µs
with 377 labels). The editor still needs a rule for when a local inline
reparse is valid and a block-level fallback; lezer gets that for free from
fragment reuse.

**Size and start-up.** 273–285 KB gzip against 11–47 KB for JS peers and
25 KB for md4c; stripping custom sections saves 13 KB gzip. Lazy wasm
compilation makes `load` sub-millisecond, but the first parse after load runs
4× slower than steady state (22 ms vs 5.5 ms on 100 KB) and the second 2×.

**Memory.** For a 1 MB document ours holds ~56 MB (18 MB JS heap, 38 MB wasm)
against 15–22 MB for the JS peers, and its wasm memory grows to 48 MB on the
second parse and is never returned. leafSources are 36% of our JS heap.

## Reproduce

```sh
cargo build -p markdown-syntax-wasm --target wasm32-unknown-unknown --profile wasm
(cd tools/bench/js && npm install)
node tools/bench/corpus/gen.mjs   # only if tools/bench/corpus/out/ is missing

# Full run (~7 min), saved as results/<name>.json
node --expose-gc tools/bench/js/bench.mjs --save <name>
# Compare a new run against this baseline
node --expose-gc tools/bench/js/bench.mjs --compare wasm-baseline
# Subset: by case id regex and/or sections
node --expose-gc tools/bench/js/bench.mjs --sections parse,breakdown --filter 'mixed-(100k|1m)' --compare wasm-baseline
# Two saved runs, nothing re-run
node tools/bench/js/bench.mjs --diff wasm-baseline <name>
```

Case ids: `parse/<file>/<subject>`, `breakdown/<file>`, `wasm-split/<file>`,
`write/<file>/<variant>`, `write-breakdown/<file>`, `key/<10k|100k|1m>/<variant>`,
`size/<module>`, `load/ours`, `cold/<subject>`, `memory/<subject>`.
`--merge` with `--save` folds a subset run into an existing file.
`MARKDOWN_SYNTAX_WASM=<path>` benches another build of the module.

## Not measured

- **Browsers.** Everything ran in Node 24 / V8. WKWebView (JavaScriptCore)
  and Chrome were not run; JSC's wasm tiers and JS JIT differ, so ratios may
  move there. Node 22, which the spec names, was not installed.
- **Wasm parse vs wire encoding as times.** The split inside the export is
  profile shares (sampling, with inlining); the module has no timing hooks,
  and `wasm/` was out of scope to change.
- **What fills wasm memory.** Peak and plateau are measured; the
  attribution to document, offset table, and result buffer is inferred from
  the code.
- **The editor's own work.** Mapping trees to ProseMirror, deciding when
  `parseInline` is enough, and CodeMirror's viewport-limited parsing are not
  modeled; lezer is measured as a full parse and as an incremental parse of
  the whole document.
- **Peers' write-back.** Only mdast-util-to-markdown; markdown-it, marked,
  commonmark.js, and lezer have no Markdown writer (a CodeMirror editor keeps
  the text and does not need one).
