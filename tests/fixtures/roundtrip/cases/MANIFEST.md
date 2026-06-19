# Semantic Input Corpus

Generated from copied upstream Rust tests by evaluating only Markdown input arguments from recognized public parser-facing test calls and macros, plus package-owned semantic inputs that internalize exposed upstream/GFM behavior without copying broad upstream text. Expected HTML strings, assertion messages, AST text fields, deferred broad stress groups outside the current parser stability boundary, dynamic performance inputs, render-only/sourcepos variants, and helper code are not executable cases.

Total executable input cases: 2401

## Profile Counts

- `commonmark`: 1528
- `extras`: 48
- `frontmatter`: 27
- `gfm`: 355
- `math`: 107
- `mdx`: 314
- `wikilink-after`: 10
- `wikilink-before`: 12

## Source Files

Dialect indicates which executable `cases/<dialect>/` bucket the input lives in.
The oracle column points at the merged upstream test source under
`oracles/upstream-tests/`.

| Oracle Source | Dialect | Cases | Profiles |
| --- | --- | ---: | --- |
| `oracles/upstream-tests/alerts.rs` | gfm | 7 | `extras` |
| `oracles/upstream-tests/autolink.rs` | gfm | 40 | `commonmark, gfm, wikilink-after, wikilink-before` |
| `oracles/upstream-tests/block_directive.rs` | gfm | 6 | `extras` |
| `oracles/upstream-tests/cjk_friendly_emphasis.rs` | gfm | 4 | `commonmark, gfm` |
| `oracles/upstream-tests/code.rs` | gfm | 14 | `commonmark` |
| `oracles/upstream-tests/commonmark.rs` | gfm | 7 | `commonmark` |
| `oracles/upstream-tests/compact_html.rs` | gfm | 10 | `commonmark, gfm` |
| `oracles/upstream-tests/core.rs` | gfm | 7 | `commonmark` |
| `oracles/upstream-tests/description_lists.rs` | gfm | 9 | `extras` |
| `oracles/upstream-tests/empty.rs` | gfm | 4 | `commonmark` |
| `oracles/upstream-tests/escape.rs` | gfm | 7 | `commonmark, extras` |
| `oracles/upstream-tests/escaped_char_spans.rs` | gfm | 5 | `commonmark` |
| `oracles/upstream-tests/footnotes.rs` | gfm | 12 | `gfm` |
| `oracles/upstream-tests/front_matter.rs` | gfm | 4 | `frontmatter` |
| `oracles/upstream-tests/fuzz.rs` | gfm | 31 | `commonmark, gfm` |
| `oracles/upstream-tests/greentext.rs` | gfm | 9 | `commonmark, extras` |
| `oracles/upstream-tests/header_id_prefix.rs` | gfm | 3 | `commonmark` |
| `oracles/upstream-tests/highlight.rs` | gfm | 1 | `extras` |
| `oracles/upstream-tests/html.rs` | gfm | 62 | `commonmark` |
| `oracles/upstream-tests/inline_footnotes.rs` | gfm | 29 | `extras, gfm` |
| `oracles/upstream-tests/insert.rs` | gfm | 1 | `extras` |
| `oracles/upstream-tests/math.rs` | gfm | 42 | `math` |
| `oracles/upstream-tests/multiline_block_quotes.rs` | gfm | 13 | `commonmark` |
| `oracles/upstream-tests/options.rs` | gfm | 6 | `commonmark` |
| `oracles/upstream-tests/phoenix_heex.rs` | gfm | 114 | `commonmark` |
| `oracles/upstream-tests/regressions.rs` | gfm | 21 | `commonmark, extras, gfm` |
| `oracles/upstream-tests/shortcodes.rs` | gfm | 5 | `extras` |
| `oracles/upstream-tests/sourcepos_chars.rs` | gfm | 40 | `commonmark, extras, frontmatter, gfm` |
| `oracles/upstream-tests/spoiler.rs` | gfm | 4 | `extras, gfm` |
| `oracles/upstream-tests/strikethrough.rs` | gfm | 1 | `gfm` |
| `oracles/upstream-tests/subscript.rs` | gfm | 3 | `extras, gfm` |
| `oracles/upstream-tests/subtext.rs` | gfm | 1 | `commonmark` |
| `oracles/upstream-tests/supersubscript.rs` | gfm | 4 | `extras` |
| `oracles/upstream-tests/table.rs` | gfm | 19 | `gfm` |
| `oracles/upstream-tests/tagfilter.rs` | gfm | 1 | `commonmark` |
| `oracles/upstream-tests/tasklist.rs` | gfm | 21 | `commonmark, gfm` |
| `oracles/upstream-tests/underline.rs` | gfm | 3 | `extras` |
| `oracles/upstream-tests/wikilinks.rs` | gfm | 19 | `wikilink-after, wikilink-before` |
| `oracles/upstream-tests/xml.rs` | gfm | 2 | `commonmark` |
| `oracles/upstream-tests/attention.rs` | commonmark | 125 | `commonmark` |
| `oracles/upstream-tests/autolink.rs` | commonmark | 42 | `commonmark` |
| `oracles/upstream-tests/block_quote.rs` | commonmark | 34 | `commonmark` |
| `oracles/upstream-tests/character_escape.rs` | commonmark | 13 | `commonmark` |
| `oracles/upstream-tests/character_reference.rs` | commonmark | 31 | `commonmark` |
| `oracles/upstream-tests/code_fenced.rs` | commonmark | 47 | `commonmark` |
| `oracles/upstream-tests/code_indented.rs` | commonmark | 29 | `commonmark` |
| `oracles/upstream-tests/code_text.rs` | commonmark | 28 | `commonmark` |
| `oracles/upstream-tests/definition.rs` | commonmark | 80 | `commonmark` |
| `oracles/upstream-tests/frontmatter.rs` | commonmark | 22 | `commonmark, frontmatter` |
| `oracles/upstream-tests/fuzz.rs` | commonmark | 18 | `commonmark, gfm` |
| `oracles/upstream-tests/gfm_autolink_literal.rs` | commonmark | 62 | `commonmark, gfm` |
| `oracles/upstream-tests/gfm_footnote.rs` | commonmark | 50 | `commonmark, gfm` |
| `oracles/upstream-tests/gfm_strikethrough.rs` | commonmark | 15 | `commonmark, gfm` |
| `oracles/upstream-tests/gfm_table.rs` | commonmark | 66 | `commonmark, gfm` |
| `oracles/upstream-tests/gfm_tagfilter.rs` | commonmark | 6 | `gfm` |
| `oracles/upstream-tests/gfm_task_list_item.rs` | commonmark | 8 | `commonmark, gfm` |
| `oracles/upstream-tests/hard_break_escape.rs` | commonmark | 8 | `commonmark` |
| `oracles/upstream-tests/hard_break_trailing.rs` | commonmark | 19 | `commonmark` |
| `oracles/upstream-tests/heading_atx.rs` | commonmark | 32 | `commonmark` |
| `oracles/upstream-tests/heading_setext.rs` | commonmark | 47 | `commonmark` |
| `oracles/upstream-tests/html_flow.rs` | commonmark | 151 | `commonmark` |
| `oracles/upstream-tests/html_text.rs` | commonmark | 69 | `commonmark` |
| `oracles/upstream-tests/image.rs` | commonmark | 37 | `commonmark` |
| `oracles/upstream-tests/link_reference.rs` | commonmark | 56 | `commonmark` |
| `oracles/upstream-tests/link_resource.rs` | commonmark | 76 | `commonmark` |
| `oracles/upstream-tests/list.rs` | commonmark | 94 | `commonmark` |
| `oracles/upstream-tests/math_flow.rs` | commonmark | 40 | `commonmark, math` |
| `oracles/upstream-tests/math_text.rs` | commonmark | 27 | `commonmark, math` |
| `oracles/upstream-tests/mdx_esm.rs` | commonmark | 53 | `mdx` |
| `oracles/upstream-tests/mdx_expression_flow.rs` | commonmark | 40 | `mdx` |
| `oracles/upstream-tests/mdx_expression_text.rs` | commonmark | 39 | `mdx` |
| `oracles/upstream-tests/mdx_jsx_flow.rs` | commonmark | 36 | `mdx` |
| `oracles/upstream-tests/mdx_jsx_text.rs` | commonmark | 141 | `mdx` |
| `oracles/upstream-tests/mdx_swc.rs` | commonmark | 5 | `mdx` |
| `oracles/upstream-tests/misc_bom.rs` | commonmark | 2 | `commonmark` |
| `oracles/upstream-tests/misc_dangerous_html.rs` | commonmark | 3 | `commonmark` |
| `oracles/upstream-tests/misc_dangerous_protocol.rs` | commonmark | 31 | `commonmark` |
| `oracles/upstream-tests/misc_default_line_ending.rs` | commonmark | 6 | `commonmark` |
| `oracles/upstream-tests/misc_line_ending.rs` | commonmark | 31 | `commonmark` |
| `oracles/upstream-tests/misc_soft_break.rs` | commonmark | 2 | `commonmark` |
| `oracles/upstream-tests/misc_tabs.rs` | commonmark | 44 | `commonmark` |
| `oracles/upstream-tests/misc_url.rs` | commonmark | 5 | `commonmark` |
| `oracles/upstream-tests/misc_zero.rs` | commonmark | 5 | `commonmark` |
| `oracles/upstream-tests/text.rs` | commonmark | 3 | `commonmark` |
| `oracles/upstream-tests/thematic_break.rs` | commonmark | 29 | `commonmark` |

## Package-Owned Files

| Corpus File | Cases | Profiles | Basis |
| --- | ---: | --- | --- |
| `cases/gfm/gfm_tilde_runs.cases` | 3 | `gfm` | `oracles/upstream-tests/strikethrough.rs` |

## Deferred Source Groups

- `oracles/upstream-tests/commonmark.rs` (commonmark dialect): full generated official examples; stable subset runs under `commonmark-examples/official-stable-inputs.cases`.
- `oracles/upstream-tests/attention.rs` (commonmark dialect): stable parser-facing subset now runs under `cases/commonmark/attention.cases`; the remaining deferred delimiter cases serialize to canonical emphasis forms that reparse to a different AST.
- `oracles/upstream-tests/gfm_strikethrough.rs` (commonmark dialect): stable parser-facing subset now runs under `cases/commonmark/gfm_strikethrough.cases`; the remaining broad strikethrough/attention/link/code interplay stress still changes AST after serialize/reparse, and the `singleTilde: false` option variant has no executable semantic profile here.
- `oracles/upstream-tests/link_reference.rs` (commonmark dialect): stable static parser-facing subset now runs under `cases/commonmark/link_reference.cases`; dynamic 999/1000-character generated reference labels and construct-disabled option variants remain deferred.
- `oracles/upstream-tests/list.rs` (commonmark dialect): stable parser-facing subset now runs under `cases/commonmark/list.cases`; construct-disabled option variants remain deferred.
- `oracles/upstream-tests/mdx_esm.rs` (commonmark dialect): stable parser-facing subset now runs under `cases/commonmark/mdx_esm.cases`; the remaining indented ESM-looking paragraph serializes to unindented ESM and reparses as `MdxEsm`.
- `oracles/upstream-mdast-util-to-markdown-tests/**` (commonmark dialect): serializer-focused output fixtures; not materialized as package-owned stability inputs.
- Dynamic performance/fuzz helpers and render-only/sourcepos variants are not materialized as package-owned stability inputs.

## Intentionally Excluded From Executable Promotion

These upstream sources are deliberately NOT promoted to executable semantic
inputs (this list distinguishes "intentionally dropped" from "forgotten"). Their
non-executable string-literal artifacts are not materialized here.

- `oracles/upstream-tests/sourcepos.rs` (gfm dialect): source-position (line/column) assertions, not
  parser round-trip behavior. The Markdown inputs only exist to anchor sourcepos spans,
  which this crate's snapshot does not model.
- `oracles/upstream-tests/pathological.rs` (gfm dialect): dynamic performance/stress inputs (deeply
  repeated constructs sized in code) that exercise allocator/time behavior, not stable
  AST shape.
- `oracles/upstream-tests/raw.rs` (gfm dialect): render-only `unsafe_`/raw-HTML output behavior driven
  by render options that have no parser-facing analogue here.
- `oracles/upstream-tests/plugins.rs` (gfm dialect): syntax-highlighter / render-plugin API
  configuration; the inputs are render-pipeline fixtures, not parser inputs.
- `oracles/upstream-tests/rewriter.rs` (gfm dialect): AST-rewriter / node-callback API behavior,
  not a parse→serialize→reparse property.
- `oracles/upstream-tests/serde.rs` (commonmark dialect): AST serde (JSON) serialization API
  coverage, not Markdown source round-trip.

### Promotion Skips (latent round-trip gaps)

The `escape.rs`, `header_id_prefix.rs`, and `xml.rs` gfm-dialect inputs were promoted in full:
every extracted Markdown input round-trips stably under its profile, so none had to be
skipped. No latent serializer/parser round-trip gaps were observed in these three sources.
