---
date: 2026-06-19
status: archived
---

# markdown-syntax Conformance Defect Investigation — Consolidated Report

> **Analysis substrate, promoted 2026-06-20 from `target/investigation-report.md`.** This is the cluster/root-cause analysis that `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md` operationalizes into T001–T025. **The headline counts below (89.94% / 234 blocks) are a 2026-06-19 snapshot and are stale** — live conformance numbers live in `tests/html_conformance/CONFORMANCE.md`. The root-cause analysis itself remains valid.

## Baseline & Headline

- **Headline conformance: 89.94%**
- markdown-rs dialect: **91.14%**
- comrak dialect: **80.74%**
- **Total failing blocks: 234**
- **Total realistic estimated bench gain (de-duplicated): ~76 cases** (raising headline toward ~99% if all land cleanly; see omissions/estimate sections for the de-dup math)

The bench is the test-only AST→HTML conformance harness (`tests/html_conformance/`). It measures CORRECTNESS vs vendored upstream HTML. The renderer is correct given the AST data it receives; where the renderer "guesses," the root cause is a missing AST discriminant, not a renderer bug. There is **no bless flag** — every `.ast` / `.canonical.md` golden affected by a fix must be hand-regenerated in the same commit.

Two structural facts dominate the whole investigation:

1. **The serializer masks stably-wrong parses.** Round-trip stability passes today because the serializer compensates (e.g. always upgrading `$x$`→`$$x$$`, always emitting `<dest>` for every autolink, unescaping reference labels symmetrically with the parser). Any correctness fix that changes the parse therefore needs a *paired serializer change* or it breaks idempotency.
2. **A small set of shared functions** (`push_line`, `likely_block_start`, `parse_block_quote`, `parse_list`, `normalize_label`, `split_table_row`, `parse_fenced_code`, `parse_math_inline`, `process_emphasis`/`record_emphasis_delimiter`, `snapshot_document`) are touched by multiple defects across clusters. These define the bundling and ordering constraints.

---

## Cluster: math (60 blocks, 4 real defects, est. gain ~58)

Owns: markdown-rs `math_flow.rs` #129–158 (30), `math_text.rs` #159–171 (13), comrak `math.rs` #216–232 (17). All 60 are real parser/AST defects. Meta-finding: `extensions/math_edges.ast` records single-dollar `$x$`→`Math "x"` while `math_edges.canonical.md` serializes `$$x$$` — the serializer always upgrades single→double, masking the wrong parse. Exactly 2 `.ast` triples carry Math nodes: `extensions/math_edges`, `extensions/table_math_directive`. The 7 `math_flow/math_text/math` `.cases` corpora are stability-only.

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| math-1 | MathInline/MathBlock carry no inline/display, dollar/code, or meta discriminant | `src/ast.rs` MathInline/MathBlock | 14 (#216–232 partial, #129/140/145) | high | atomic-synchronized (AST + serializer + 2 goldens) | 17 |
| math-2 | parse_math_block is a stub, not a fenced-code analogue | `parse.rs:parse_math_block` (+is_math_block_fence @584) | ~24 (#130–158) | high | atomic-synchronized (needs math-1 meta field) | 22 |
| math-3 | parse_math_inline is a broken code-span clone (greedy close, wrong escapes, no padding strip) | `parse.rs:parse_math_inline` @4890 (+ @4930) | ~13 (#159–171, #230) | high | atomic-synchronized (paired serializer, 2 goldens) | 13 |
| math-4 | single_dollar/comrak flanking $-semantics not honored at dispatch | `parse.rs:3717` inline math dispatch | ~6 (#159,216,219,222,225,226,229,232) | med | independently-green-via-seam (rides math-3) | 6 |

**Ripple sets (key):** math-1 rebranches the two AST structs + 4 parse construction sites + serialize Math arms (@199, @856) + validate (@99, @275) + `snapshot_document` (@1311, @1495) + test renderer (`inlines.rs`/`blocks.rs`, 23 hits) + the 2 Math `.ast`/`.canonical.md` goldens. math-2 reworks `parse_math_block` close/indent/EOF/meta + `block_math_fence` serializer (@1915). math-3 rewrites `parse_math_inline` close/padding/escape + serializer `serialize_inline_math_with_context` (@1926), `inline_math_fence` (@2034), `text_math_can_start` (@1335). math-4 threads `single_dollar_math` + comrak flanking through dispatch + `options.rs`.

**Goldens affected:** `extensions/math_edges.{ast,canonical.md}`, `extensions/table_math_directive.{ast,canonical.md}` (all four, hand-regen for math-1/2/3).

**Forbidden patches:** render-time heuristics re-inspecting `value` to fake display/code; copy-pasting `parse_fenced_code` body into `parse_math_block` as a near-duplicate; single-space-trim-only hacks for math-3; hardcoding an engine flag from the test RenderConfig into the production parser for math-4.

**Fix sketches:** math-1 — add a discriminant enum to MathInline (`Dollar{display}` | `Code`) and a meta/display marker to MathBlock; populate at parse time from the actual delimiter; serialize chooses fence from discriminant; regen 2 goldens + snapshot_document. math-2 — rebuild parse_math_block to mirror parse_fenced_code (>=2-$ run + optional meta, indent record, >= -length/EOF/indent-aware close, blank preservation, blockquote line stripping); route comrak `$$..$$` to inline. math-3 — reimplement as a code-span: opening run length N, exact-N close on a run boundary, no backslash escapes, code-span padding rule, line-endings→spaces; keep `$`..`$` code-math as the separate branch recorded via math-1's discriminant; pair serializer. math-4 — encode dialects via existing ParseOptions/Constructs (single_dollar gate + comrak digit-flanking guard), land jointly with math-3, no engine enum.

---

## Cluster: autolink (50 blocks, 7 real defects, est. gain ~18)

Owns: `gfm_autolink_literal.rs` #55–76 (22) + comrak `autolink.rs` #183–210 (28). Angle-autolink path fully passes — do not touch it. Excluded as engine-divergent / renderer artifacts: comrak #194–207 (relaxed-autolinks/relaxed-scheme), #199 (angle-in-link unwrap), gfm #56 + comrak #183–190 (renderer visible_text symptoms folded under autolink-6), comrak #210 (unicode isolate, folded into autolink-3).

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| autolink-1 | GFM literal email local-part uses RFC atext + forward scan, not narrow GFM charset + left boundary | `parse.rs:is_email_local_part`/`parse_literal_email` | #70,#71 | med | independently-green-via-seam | 2 |
| autolink-2 | One-size preceding-char gate: `_` wrongly blocks http/www; www lacks stricter class | `parse.rs:parse_literal_autolink` prefix guard ~6192 | #70,#71 | med | independently-green-via-seam | 2 |
| autolink-3 | literal_link_end does not treat `]`/`[` as hard URL boundary | `parse.rs:literal_link_end` ~6246 | #62,63,69,72,73,74,76; comrak #191–193,#210 | low | independently-green-via-seam | 4 |
| autolink-4 | Empty-host literals (`http://`+punct, bare `www.`) not rejected | `parse.rs:gfm_autolink_domain_is_valid` | #63,#74; comrak #208,#209 | high | independently-green-via-seam | 3 |
| autolink-5 | Extended-protocol literals mailto:/xmpp: not recognized | `parse.rs:parse_literal_autolink` scheme dispatch | #55 | med | atomic-synchronized w/ autolink-6 (display half) | 1 |
| autolink-6 | Autolink AST node has no kind/original-text → lossy `<dest>` serialize + renderer guesswork | `src/ast.rs:Autolink` struct | #56, #62/72/73/74 residual; comrak #183–190 | high | atomic-synchronized (AST + serializer + 3 goldens) | 5 |
| autolink-7 | GFM email last-label rule incomplete: hyphen in final label must not autolink | `parse.rs:is_gfm_email_domain` | #61 | low | independently-green-via-seam | 1 |

**Ripple sets (key):** autolink-1 adds a separate GFM left-scan (must NOT narrow shared `is_email_local_part`/`is_email_atext`, which the angle `<email>` path needs). autolink-2 splits the prefix guard per-scheme. autolink-3 adds `]`/`[` to the hard-break set (not the trailing-trim set). autolink-4 gives `gfm_autolink_domain_is_valid` real host-presence/first-char logic without re-applying the -2-regressing naive reject. autolink-6 adds a `kind` (+ optional original text) enum to the single Autolink node, rippling through parse (2 sites), serialize arm (@845), `validate_autolink` (relax `>` for literal only), `snapshot_document`, every `.ast` line printing an Autolink, and the test renderer (`visible_text`/`refs.rs`).

**Goldens affected:** autolink-6 forces hand-regen of `spec/commonmark_autolinks`, `spec/commonmark_inlines`, `extensions/gfm_autolinks` (`.ast`+`.canonical.md`). autolink-1..5,7 are golden-clean (no `.ast` triple covers their bare-literal inputs; `gfm_autolinks.md` has 0 `]`, verified).

**Forbidden patches:** narrowing shared atext in place (autolink-1); globally deleting `_` rejection (autolink-2); `]` as trailing-trim instead of hard boundary (autolink-3); naive empty/first-char reject (autolink-4); "add mailto: in parser" on the bare-email synthesis path (autolink-5, KNOWN-WRONG/STALE); patching only renderer `visible_text` to fake case-56 / #183-190 (autolink-6); banning hyphens in all labels (autolink-7).

**Fix sketches:** autolink-1 — left-scan from `@` over `[A-Za-z0-9._+-]`, anchor the link start there. autolink-2 — branch preceding-char test per scheme. autolink-3 — add `]`/`[` to `literal_link_end` hard-break loop, re-run paren/entity/trim fixpoint. autolink-4 — host = segment before `/?#`, require non-empty + first alnum, www requires a non-empty label after `www.`. autolink-5 — add a mailto:/xmpp: branch (case-insensitive, preceding non-alnum guard), dest = literal scheme:...; coordinate with autolink-6 visible_text. autolink-6 — add kind `{Angle, GfmLiteral{original}}`; Angle→`<dest>`, GfmLiteral→raw text; validate relax `>` for literal; renderer uses kind; regen 3 triples; land autolink-1..5 first. autolink-7 — extend last-label check to reject `-` in the final label only.

---

## Cluster: lists-tasks (22 blocks, 5 real defects, est. gain ~20)

Owns: `list.rs` #109–128 (20) + comrak `description_lists.rs` #212–213 (2). gfm_task_list_item has 0 failures. `parse_list` is the dominant cross-cluster shared hazard.

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| lists-1 | continuation/sublist indent threshold keys off marker.indent not content_indent | `parse.rs:parse_list` @889/@918 | #109,110,111,112,115,118,119 | high | atomic-synchronized (6 goldens regen) | 7 |
| lists-2 | lazy paragraph absorbs a different-delimiter list-marker line | `parse.rs:parse_list` non-blank path @913–934 | #116 | medium | atomic-synchronized (2 goldens) | 1 |
| lists-3 | lazy continuation does not cross nested-container boundaries (architectural) | `parse.rs` parse_block_quote/parse_list line-collection model | #113,#114 | high | independently-green-via-seam (DEFER) | 2 |
| lists-4 | blank/whitespace lines inside an OPEN fenced code in a list item discarded/collapsed | `parse.rs:parse_list` blank branch @881/@907 | #120,122,123,124,125,126,127,128 | medium | atomic-synchronized (4 goldens) | 8 |
| lists-5 | description-list tightness over-loosened by term-group-separating blank | `parse.rs:parse_description_details`/`parse_description_list` | #212,#213 | high | atomic-synchronized (paired serializer, 3 goldens) | 2 |

**Ripple sets (key):** lists-1 rebranches the two continuation guards to compare against `content_indent`; touches `strip_list_continuation`, `leading_indent_columns`, `sibling_list_marker_at_line`. lists-2 folds a different-delimiter marker into the paragraph-interrupt path. lists-4 decides blankness AFTER fence state and routes in-fence lines through `strip_list_continuation` (preserving residual whitespace). lists-5 distinguishes item-separating blanks from content blanks in description-list tightness, paired with `serialize_description_list` (lines 451–468).

**Goldens affected:** lists-1 — `core/list`, `spec/commonmark_lists`, `spec/commonmark_blocks` (`.ast`+`.canonical.md`). lists-2 — `spec/commonmark_lists`. lists-4 — `core/list`, `spec/commonmark_lists`. lists-5 — `extensions/description_lists_{core,blocks,edges}` (`.ast`+`.canonical.md`; `description_lists_core.md` itself has a blank-separated term group, so its golden WILL move).

**Forbidden patches:** `if leading_indent==1 break` / off-by-one shims (lists-1); hardcoding `)`-delimiter detection (lists-2); AST-surgery reattachment shim (lists-3); special-casing only all-spaces/tab cases or post-processing CodeBlock (lists-4); forcing tight=true unconditionally or stripping the inter-term blank (lists-5).

**Fix sketches:** lists-1 — replace `<= first_marker.indent` with `>= content_indent`; blank branch ends item when next non-blank indent < content_indent. lists-2 — break the item when `list_marker_info` is Some but not a same-list sibling. lists-3 — DEFER; needs a container-stack parse, do not bolt onto parse_list. lists-4 — when `open_fence.is_some()`, do not treat whitespace-only as a blank separator; route through `strip_list_continuation` and push each line individually. lists-5 — do not set tight=false for blanks followed by a new term/marker; drop tight=false at inter-term transitions; pair serializer; regen 3 triples.

---

## Cluster: links-refs-images (20 blocks, 6 real defects + 1 flagged sub-defect, est. gain ~16)

Owns: `definition.rs` #47–50, `image.rs` #93, `link_reference.rs` #94–105, `link_resource.rs` #106–108 (20). Excluded: #93 `allowDangerousProtocol` (renderer artifact — `filter_img_protocol` ignores its `_allow_dangerous_protocol` param; parser is correct).

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| label-1 | normalize_label unescapes the label; CommonMark matches RAW (casefold + ws-collapse) only | `parse.rs:normalize_label` @5327 | #95,99,100,101,102,103,104,105 | med | atomic-synchronized (paired serializer, 1 golden) | 8 |
| reffallback-1 | inline/full reference does not block shortcut fallback | `parse.rs:parse_link` @4081 / `parse_image` @3993 | #96,#97,#98 | med | independently-green-via-seam | 3 |
| deftitle-eol-1 | title beginning/ending with EOL wrongly rejected as blank line | `parse.rs:contains_blank_line` @5095 | #47,#107 | low | independently-green-via-seam | 2 |
| destparen-1 | bare destination has no 32-level paren nesting cap | `parse.rs:parse_link_destination` @5037 | #106 | low | independently-green-via-seam | 1 |
| nul-dest-1 | NUL in destination treated as control terminator, not U+FFFD | `parse.rs:parse_link_destination` @5045 | #108 | low | independently-green-via-seam | 1 |
| (def-multiline) | multi-line definition LABEL not recognized (flagged sub-defect) | `parse.rs:parse_definition` @1334 / `find_reference_label_end` @4180 | #48,#49,#50,#94 | med | flagged, lower-confidence | ~2 |

**Ripple sets (key):** label-1 changes `normalize_label` (drop the `unescape_string` call) AND serializer `normalize_reference_label`/`unescape_reference_label` in lockstep (the shortcut/collapsed omission oracle compares `normalize_reference_label(children) == node.identifier`). reffallback-1/deftitle-eol-1/destparen-1/nul-dest-1 all touch `parse_link`/`parse_image`/`parse_link_resource`/`parse_link_destination`/`parse_link_title`/`contains_blank_line` — land as one coherent parser-control edit to avoid merge thrash.

**Goldens affected:** label-1 ONLY — `spec/commonmark_reference_labels.{ast,canonical.md}` (identifier `foo]`→`foo\]`, `a & b`→`a &amp; b`). Re-verify `gfm_footnote_edges` stays stable (snapshot shows raw label, both ref+def fold identically). The other 5 are golden-clean.

**Forbidden patches:** options flag / second "raw" normalize variant, or special-casing find_definition while leaving unescape_string (label-1); if-let-retry that fixes only #96 not #97/#98, or making find_definition return Some for `[ ]` (reffallback-1); trim-leading-newline shim on definition path only, or relaxing contains_blank_line to ignore ALL blanks (deftitle-eol-1); wrong cap boundary or counting balanced parens (destparen-1); local `\0` special-case diverging from the NUL→FFFD convention (nul-dest-1).

**Fix sketches:** label-1 — fold/collapse the RAW label (`split_whitespace().join(" ").to_uppercase().to_lowercase()`); make serializer stop unescaping; regen the one spec triple. reffallback-1 — `(` arm: on None, fall through to shortcut, not propagate None; `[` arm: a full ref with non-empty undefined ref returns None (no shortcut fallback); whitespace-only inner = non-empty undefined; mirror in parse_image. deftitle-eol-1 — make contains_blank_line ignore boundary-empty lines, only true on an INTERIOR blank (two consecutive EOLs). destparen-1 — track max depth, return None at >32. nul-dest-1 — preprocess input NUL→U+FFFD document-wide (cleanest) or treat NUL as ordinary in parse_link_destination. def-multiline — when find_reference_label_end fails on the first line, accumulate continuation lines before locating `]:` (lower confidence on #49 tab/indent semantics; may need list-prefix coordination).

---

## Cluster: tables (8 blocks, 4 real defects, est. gain ~8)

Owns: `gfm_table.rs` #80–86 (7) + `spoiler.rs` #234 (1). `split_table_row` touched by tbl-3 AND tbl-4 (intra-cluster ordering hazard). `likely_block_start` is the dominant shared hazard (11 call sites).

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| tbl-1 | table body termination uses paragraph-interruption rules, not GFM row-continuation rules | `parse.rs:parse_table` likely_block_start gate @2710 | #80,81,82,85 | med | independently-green-via-seam | 4 |
| tbl-2 | single-column loose table without any pipe wrongly parsed as table not setext | `parse.rs:table_has_separator` @5912 | #86 | low | independently-green-via-seam | 1 |
| tbl-3 | escaped pipe `\|` inside an inline code span in a cell not unescaped | `parse.rs:split_table_row` @5813 | #83,#84 | high | atomic-synchronized (paired serializer pipe-accounting) | 2 |
| tbl-4 | inline spoiler `\|\|...\|\|` occupying a whole cell renders empty | `parse.rs:split_table_row` spoiler branch @5841–5858 | #234 | med | independently-green-via-seam | 1 |

**Ripple sets (key):** tbl-1 calls a NEW table-specific termination predicate at @2710 — must NOT widen `likely_block_start`. tbl-2 gives `table_has_separator` real logic (require a pipe when single-column); the serializer reuses `gfm_table_can_start_source` via `paragraph_table_escape_offset` so the change flows to the escape-decision seam automatically. tbl-3 drops the backslash of `\|` inside code runs in `split_table_row`; serializer must keep code-protected pipes from tripping `table_cell_has_unescaped_pipe`. tbl-4 makes spoiler state cell-local in `split_table_row`.

**Goldens affected:** none require regen, but `gfm_table_edges`, `gfm_table_containers` `.ast`/`.canonical.md` must be hand-RE-VERIFIED after tbl-1/2/3 (all pipe-bearing/multi-column, predicted unchanged). No spoiler-in-table golden exists.

**Forbidden patches:** widening `likely_block_start`/`list_marker_can_interrupt_paragraph` or a `table_mode: bool` param (tbl-1); reordering dispatch so setext beats table, or special-casing bare `-` (tbl-2); renderer-side `\` stripping or post-hoc Code-node walking (tbl-3); special-casing whole-cell `||...||` or disabling the code_fence/spoiler guard (tbl-4).

**Fix sketches:** tbl-1 — introduce `table_body_line_ends_table(line, options)` used only at @2710 (any list marker, any HTML flow start, thematic/ATX/fenced/blockquote, blank); leave likely_block_start untouched. tbl-2 — `table_has_separator`: when one column resolves, require an unescaped pipe in header/delimiter (reuse `contains_unescaped_pipe`). tbl-3 — collapse `\|`→`|` when emitting cell bytes both inside and outside code_fence; verify Code value/raw, serializer emit-inside-backticks, idempotent reparse. tbl-4 — reset spoiler_open at each cell boundary; append `||` open/close to cell text rather than consume; keep code_fence guard.

---

## Cluster: blockquote-headings (8 blocks, 6 real defects, est. gain ~7)

Owns: `block_quote.rs` #1/2/3, `heading_atx.rs` #87, `heading_setext.rs` #88/89/90, `thematic_break.rs` #182. Dominant theme: block-quote lazy-continuation correctness defeated by the "collect stripped lines → re-parse" architecture that discards lazy-vs-marked provenance. `likely_block_start` (11 sites) must land FIRST.

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| bq-1 | block quote drops a >=4-space lazy paragraph-continuation line | `parse.rs:parse_block_quote` trim guard @662 | #1 | med | independently-green-via-seam | 1 |
| bq-2 | lazy continuation fails after a nested block quote (innermost open paragraph not tracked) | `parse.rs:block_quote_paragraph_stays_open` @820 + re-parse @744 | #2,#3 | high | atomic-synchronized (verify-only golden, paired serializer) | 2 |
| setext-lazy | setext underline applied to a lazy block-quote continuation line | `parse.rs:parse_setext_heading` @2750 on re-parsed content | #88,#90 | med | atomic-synchronized (verify MARKED `> a\n> ---` stays H2) | 2 |
| setext-hardbreak | setext content lines fully trimmed, destroying trailing-space hard breaks | `parse.rs:parse_setext_heading` @2769 | #89 | low | independently-green-via-seam | 1 |
| atx-indent | likely_block_start ignores indentation → >=4-space ATX/fence/thematic/list wrongly interrupts | `parse.rs:likely_block_start` @5966 | #87 | med | independently-green-via-seam | 1 |
| thematic-in-list | `* * *` inside a list consumed as a nested item instead of ending the list | `parse.rs:parse_list` outer loop @862 | #182 | med | independently-green-via-seam | 1 |

**Ripple sets (key):** bq-1 — do not BREAK on `trim_up_to_three_spaces==None`; route failing continuation through the lazy branch using RAW line text. bq-2 — lazy-aware redesign (lazy flag on Line / per-level open-paragraph state through `parse_blocks`). setext-lazy builds on bq-2's provenance signal to suppress underline on a lazy line. setext-hardbreak changes @2769 to `trim_ascii_start` (leading-only). atx-indent gates indent-sensitive constructs behind `trim_up_to_three_spaces` (11-site shared function — land FIRST). thematic-in-list checks `parse_thematic_break` before accepting a list marker.

**Goldens affected:** none forced to regen (greps returned 0 for the failing input shapes); `spec/commonmark_blockquotes.{ast,canonical.md}` is VERIFY-ONLY for bq-2 and setext-lazy (the MARKED `> a\n> ---` setext H2 must stay). setext-bearing triples re-verified for setext-hardbreak.

**Forbidden patches:** narrow `if leading_indent>=4 take as-is` skipping the paragraph_open gate (bq-1); `lazy_after_nested_quote` bool shim (bq-2); adding `===`/`---` to likely_block_start break conditions (setext-lazy); post-processing to inject HardBreak or special-casing exactly 2 trailing spaces (setext-hardbreak); guard only inside parse_paragraph or dropping trim_start (atx-indent); making `list_marker_info` return None for `* * *` (thematic-in-list — precedence belongs at the call site).

---

## Cluster: whitespace-tabs-bom (9 blocks, 3 real defects, est. gain ~6)

Owns: BOM #172–173, line_ending #174–178, tabs #179–181. Excluded as oracle-quirk (NOT a parser defect): pure CR/CRLF byte preservation (#174, #175, CR-byte portion of #177/#178) — the AST is line-ending-agnostic by design (whole-document serializer option).

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| ws-1-bom | leading UTF-8 BOM (U+FEFF) not stripped at document start | `parse.rs:parse_with_options` @96 | #172,#173 | low | independently-green-via-seam | 2 |
| ws-2-fence-leading-blank | fenced code drops leading/sole empty content line | `parse.rs:parse_fenced_code` @588 via push_line @5294 | #176 (#178 partial) | med | independently-green-via-seam | 1 |
| ws-3-partial-tab-fence | strip_leading_indent_columns never partially consumes a tab when fence is indented | `parse.rs:strip_leading_indent_columns` @5554 | #179,#180,#181 | med | independently-green-via-seam | 3 |

**Ripple sets (key):** ws-1-bom — `let input = input.strip_prefix('\u{feff}').unwrap_or(input);` once at top of parse_with_options; use the shadowed slice for `collect_definitions`, `parse_blocks`, and Document meta span (offsets stay aligned). ws-2 — fence-local line-index-aware join; must NOT touch push_line (23 callers). ws-3 — `strip_leading_indent_columns` returns owned String/Cow when a budget-crossing tab expands to residual spaces; sole caller `parse_fenced_code` @625 adapts.

**Goldens affected:** none (no fixture starts with BOM; no `.ast` fenced golden has a blank first content line; `commonmark_tabs.ast` indented-fence lines parse as indented code — VERIFY ws-3 leaves it unchanged).

**Forbidden patches:** global `input.replace('\u{feff}','')` (deletes interior BOM in #173); changing push_line's empty-skip guard or switching code-block value model to trailing-newline form (ws-2); single-space-only fence-indent special case or keeping the borrowed `&str` with the tab whole (ws-3).

---

## Cluster: emphasis-strike (3 blocks, 1 real defect, est. gain ~3)

Owns: `gfm_strikethrough.rs` #77/78/79. All collapse to ONE structural root: strikethrough is parsed by a separate greedy scan and committed as a finished `Inline::Delete` instead of participating in the unified delimiter stack.

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| strike-1 | `~`/`~~` parsed greedily, not via the shared CommonMark/GFM delimiter stack | `parse.rs:parse_inlines_with_context` tilde branches @~3602/3630 | #77,#78,#79 | high | atomic-synchronized (paired serializer + ~19 inline assert goldens) | 3 |

**Ripple set (key):** replace the greedy tilde branches with record-only branches pushing a `~` DelimMarker (extends `record_emphasis_delimiter`); teach `process_emphasis` to pair `~`/`~~` runs (like-length 1↔1, 2↔2) into Delete with the right DeleteMarker; respect `single_tilde_strikethrough` + subscript precedence. Touches `find_closing_delimiter` (6 callers ___/__/++/==/~~ — ordering hazard with insert/highlight/underline clusters), `delimiter_flanking`/`is_flanking_punctuation`, `DelimMarker`, serialize Delete/DeleteMarker arms, validate Delete arm. NO ast.rs variant change (Delete/DeleteMarker/DelimMarker already exist).

**Goldens affected:** hand-regen ~19 inline `assert_eq` strike goldens across `inline_delimiter_regressions.rs`, `serializer_regressions.rs`, `review_validate_regressions.rs`; check `extensions/inline_markup_extras` triple (subscript-vs-strike precedence, likely unchanged).

**Forbidden patches:** more special cases in the greedy scanners, a `~`-aware shim in find_closing_delimiter, excluding `~` from is_flanking_punctuation only when emphasis is adjacent, or a second parallel strike stack (interleaving needs ONE stack).

---

## Cluster: commonmark-core-misc (43 blocks, 2 net-new defects, est. gain ~6)

Catch-all owning 43 blocks; only 2 are net-new (misc-1, misc-2). All other blocks map to sibling clusters or are harness artifacts (see Omissions Check).

| id | title | root_cause_owner | cases | risk | coupling | est. gain |
|----|-------|------------------|-------|------|----------|-----------|
| misc-1 | fenced/indented code drops a leading/sole empty content line (push_line guard) | `parse.rs:parse_fenced_code` @623 / push_line @5294 | #21,#22,#39,#52,#53 | med | atomic-synchronized (2 goldens regen) | 5 |
| misc-2 | likely_block_start treats 4+-space markers as block starts (ignores <=3 limit) | `parse.rs:likely_block_start` @5966 | #19 | med | independently-green-via-seam | 1 |

**Ripple sets (key):** misc-1 is the SAME root as ws-2-fence-leading-blank — fix at `parse_fenced_code` (and `parse_indented_code`), never widen push_line; regen 2 goldens. misc-2 is the SAME root as atx-indent (blockquote cluster) — gate indent-sensitive markers in `likely_block_start` behind `trim_up_to_three_spaces`.

**Goldens affected:** misc-1 — `spec/commonmark_blocks.{ast,canonical.md}`, `spec/commonmark_tabs.{ast,canonical.md}`. misc-2 — none (0-hit grep).

**Forbidden patches:** changing push_line itself or adding an `allow_empty`/`is_code` bool (misc-1); special-case in parse_indented_code/parse_paragraph or hardcoded `starts_with("    ")` (misc-2).

> **NOTE:** misc-1 and ws-2-fence-leading-blank are the SAME defect surfaced by two clusters. misc-2 and atx-indent are the SAME defect surfaced by two clusters. These are de-duplicated in the bundles and the total estimate.

---

## 1. Shared-Function Conflict Map

Functions in `parse.rs` / `serialize.rs` / `ast.rs` touched by more than one defect. These define the ordering/bundling constraints.

| function (file) | touched_by | hazard |
|-----------------|------------|--------|
| `push_line` (parse.rs:5294) | math-2, code-1, ws-2-fence-leading-blank, misc-1, bq-2 | 23 callers. DO NOT change its body/signature. Every fix must be LOCAL to its leaf loop (parse_fenced_code/parse_indented_code/parse_math_block). misc-1 ≡ ws-2 ≡ code-1 are the SAME fenced/indented leading-blank bug — fix once in parse_fenced_code/parse_indented_code. |
| `likely_block_start` (parse.rs:5966) | atx-indent, misc-2, tbl-1, bq-1, bq-2, lists-2(adapt), thematic-in-list(adapt) | 11–12 call sites across paragraph/blockquote/list/table/setext/footnote. atx-indent ≡ misc-2 (same indent-budget fix). Must land FIRST; lazy/table defects re-read its corrected verdict. tbl-1 must NOT widen it (use a table-only predicate). |
| `parse_block_quote` (parse.rs:644) + `block_quote_paragraph_stays_open` (@820) | bq-1, bq-2, setext-lazy, lists-3, code-3 (bq-lazy-cross) | The lazy-provenance redesign is shared by bq-2 + setext-lazy + lists-3 + the code-cluster's bq-lazy-cross (#6–11). One coordinated blockquote rewrite; code/list clusters take NO action on these symptoms. |
| `parse_list` (parse.rs:850) + `strip_list_continuation` + `list_marker_info` | lists-1, lists-2, lists-3, lists-4, thematic-in-list | Dominant list hazard. lists-1 is foundational (others re-read content_indent semantics). thematic-in-list adds a precedence guard at the same outer loop. Sequence lists-1 → lists-4 → lists-2 → thematic-in-list; defer lists-3. |
| `parse_fenced_code` (parse.rs:588) + `strip_leading_indent_columns` (@5554) | code-1, code-3, ws-2-fence-leading-blank, ws-3-partial-tab-fence, misc-1 | Leading-blank fix (code-1/ws-2/misc-1) + ws-3 tab-expansion (changes return type to owned) + code-3 options-threaded close relaxation all edit this function. Bundle the leading-blank fix; ws-3 + code-3 are independent edits to the same fn — sequence to avoid merge thrash. |
| `parse_math_inline` (parse.rs:4890) + inline dispatch @3717 | math-3, math-4 | Both rewrite the inline math close/acceptance logic and the @3717 dispatch. MUST land together (one code path) to avoid double-rewrite. |
| `parse_math_block` (parse.rs:550) | math-2 (+ depends on math-1 AST field) | Needs math-1's meta-string field; cannot land independently. |
| MathInline/MathBlock (ast.rs) + `snapshot_document` (fixtures.rs:1311/1495) | math-1, math-2, math-3 | AST field add forces all construction/match/snapshot sites + 2 goldens to change in one commit. snapshot_document is ALSO shared with autolink-6 (any AST-field-adding cluster). |
| Autolink struct (ast.rs) + serialize arm (@845) + `validate_autolink` + `snapshot_document` | autolink-5, autolink-6 | autolink-6's kind enum gates autolink-5's display half. snapshot_document format change ripples to every Autolink `.ast` line + 3 triples. |
| `normalize_label` (parse.rs:5327) + serialize `normalize_reference_label`/`unescape_reference_label` | label-1, lean-scan dedup item | Parser + serializer must change in lockstep (omission oracle compares them). The lean-scan dedup MUST fold INTO this fix, not be done standalone. Also shared with footnote/char-ref work. |
| `parse_link` / `parse_image` / `parse_link_resource` / `parse_link_destination` / `parse_link_title` / `contains_blank_line` | reffallback-1, deftitle-eol-1, destparen-1, nul-dest-1 (+ def-multiline) | All four parser-control changes touch the same link/resource path — land as ONE coherent edit to avoid merge thrash. |
| `split_table_row` (parse.rs:5813) + `find_spoiler_close` / `contains_unescaped_pipe` | tbl-3, tbl-4 | Both edit the cell-building loop (pipe-unescape inside code + cell-local spoiler state). Sequence as ONE coordinated edit. |
| `process_emphasis` / `record_emphasis_delimiter` / `find_closing_delimiter` / `delimiter_flanking` / `DelimMarker` | strike-1 (+ insert/highlight/underline clusters via find_closing_delimiter's 6 callers) | strike-1 moves `~` into the shared stack; find_closing_delimiter is shared by ___/__/++/==/~~. Ordering hazard with any other attention cluster — coordinate/land before them. |
| `is_escaped_at` / `parse_character_reference` / `character_reference_value` (dup in parse.rs + serialize.rs) | math-3 (stops calling is_escaped_at), lean-scan dedups | Pure-relocation dedups (lean). math-3's "stop calling is_escaped_at for math" is a call-site change, compatible with promoting the parse.rs copy pub(crate). |

---

## 2. Recommended Leaf-First Execution Order

Foundation-first: shared classifiers and AST/option changes before the logic that reads them; low-coupling quick wins early to bank progress; big coupled rewrites isolated as bundles at the end.

1. **atx-indent (≡ misc-2)** — depends_on: none. Foundational shared classifier (`likely_block_start`, 11 sites). Land FIRST; every lazy/table defect re-reads its corrected indent verdict. Golden-clean.
2. **autolink-7** — depends_on: none. Pure last-label tightening, 1 fn, golden-clean. Quick win.
3. **autolink-3** — depends_on: none. Add `]`/`[` hard boundary, golden-clean, low risk.
4. **autolink-1** — depends_on: none. Separate GFM email left-scan, golden-clean.
5. **autolink-2** — depends_on: autolink-1 (email left-extent handled there). Per-scheme prefix guard.
6. **autolink-4** — depends_on: autolink-1, autolink-2. Host-presence validation (high risk — the -2-regression trap).
7. **deftitle-eol-1** — depends_on: none. contains_blank_line boundary fix, golden-clean. *(Bundle with link-control edits.)*
8. **destparen-1** — depends_on: none. 32-paren cap, golden-clean. *(Bundle with link-control edits.)*
9. **nul-dest-1** — depends_on: none. NUL→FFFD, golden-clean. *(Bundle with link-control edits.)*
10. **reffallback-1** — depends_on: none. Shortcut-fallback control flow, golden-clean. *(Bundle with link-control edits.)*
11. **tbl-2** — depends_on: none. table_has_separator single-column pipe rule, golden re-verify only.
12. **tbl-1** — depends_on: atx-indent (table-only predicate built alongside the corrected classifier). New table termination predicate, golden re-verify only.
13. **tbl-3** + **tbl-4** — depends_on: none (intra-cluster bundle). Coordinated split_table_row edit (pipe-unescape-in-code + cell-local spoiler).
14. **setext-hardbreak** — depends_on: none. Leading-only trim in parse_setext_heading, golden-clean.
15. **thematic-in-list** — depends_on: atx-indent, lists-1. parse_thematic_break precedence guard in parse_list outer loop.
16. **ws-1-bom** — depends_on: none. Single BOM strip at parse entry, golden-clean.
17. **ws-3-partial-tab-fence** — depends_on: none. strip_leading_indent_columns owned-return, golden re-verify only.
18. **lists-1** — depends_on: atx-indent. FOUNDATIONAL list fix (content_indent threshold); other list defects re-read it. Regen 6 goldens.
19. **lists-4** — depends_on: lists-1. In-fence blank preservation in parse_list. Regen 4 goldens.
20. **lists-2** — depends_on: lists-1. Different-delimiter marker interrupts paragraph. Regen 2 goldens.
21. **code-1 (≡ ws-2 ≡ misc-1) FENCE-LEADING-BLANK BUNDLE** — depends_on: none. Fix parse_fenced_code + parse_indented_code leading/sole-blank; regen commonmark_blocks/commonmark_tabs goldens. (Sequence after ws-3 since both edit parse_fenced_code/strip_leading_indent_columns.)
22. **lists-5** — depends_on: none. Description-list tightness + paired serializer; regen 3 description_lists triples.
23. **label-1 (+ lean normalize dedup)** — depends_on: none. normalize_label raw-match + paired serializer normalize_reference_label/unescape_reference_label; fold the lean dedup in; regen commonmark_reference_labels triple; verify gfm_footnote_edges.
24. **def-multiline** — depends_on: label-1 (same definition path). Multi-line LABEL accumulation (lower confidence on #49 indent).
25. **MATH BUNDLE: math-1 → math-2 → (math-3 + math-4)** — depends_on: none externally; internal order math-1 first (AST discriminants), then math-2 (block, needs math-1 meta field), then math-3+math-4 together (shared parse_math_inline + @3717 dispatch). Paired serializer throughout; regen math_edges + table_math_directive triples.
26. **autolink-5 + autolink-6 BUNDLE** — depends_on: autolink-1..4 (literal extents finalized first). Add Autolink kind enum + serializer + validate + renderer + regen 3 triples; autolink-5 mailto:/xmpp: rides the same node.
27. **strike-1** — depends_on: none externally, but ORDERING HAZARD with insert/highlight/underline (find_closing_delimiter). Land before/coordinated with those clusters. Paired serializer + regen ~19 inline assert goldens.
28. **BLOCKQUOTE-LAZY BUNDLE: bq-1 → bq-2 → setext-lazy** (+ code-cluster bq-lazy-cross #6–11) — depends_on: atx-indent. Lazy-provenance redesign of parse_block_quote/parse_blocks; verify commonmark_blockquotes goldens.
29. **lists-3** — DEFER (architectural container-stack; only 2 cases; shares the blockquote-lazy redesign). Tackle after the blockquote bundle if at all.

---

## 3. Bundles (must land together)

**Bundle A — Math AST + parser + serializer (atomic).** defect_ids: math-1, math-2, math-3, math-4. why_bundled: adding discriminant/meta fields to MathInline/MathBlock forces every construction/match/snapshot site + the 2 Math `.ast`/`.canonical.md` goldens to change in one commit (no seam isolation for a struct field add); math-2 needs math-1's meta field; math-3 + math-4 share `parse_math_inline` and the @3717 dispatch and must rewrite one code path. Paired serializer required throughout; serializer currently masks the wrong parse.

**Bundle B — Autolink node kind (atomic).** defect_ids: autolink-5, autolink-6. why_bundled: autolink-6 adds the Autolink `kind`/original-text enum that rippls through parse/serialize/validate/snapshot_document and 3 `.ast` triples; autolink-5's mailto:/xmpp: display half is only correct once the renderer can distinguish literal vs synthesized scheme (autolink-6's kind). Must land in one commit. autolink-1..4 land first (literal extents).

**Bundle C — Reference label normalization (atomic, parser+serializer).** defect_ids: label-1 (+ lean-scan normalize/unescape dedup). why_bundled: the shortcut/collapsed omission oracle compares `normalize_reference_label(children) == node.identifier`; if the parser stops unescaping but the serializer keeps unescaping, round-trip diverges (the documented prior revert). normalize_label (parser) + normalize_reference_label/unescape_reference_label (serializer) + the one spec triple change in lockstep; fold the lean dedup into this fix.

**Bundle D — Link/resource parser control (coherent edit).** defect_ids: reffallback-1, deftitle-eol-1, destparen-1, nul-dest-1 (+ def-multiline). why_bundled: all touch the same parse_link/parse_image/parse_link_resource/parse_link_destination/parse_link_title/contains_blank_line surface; landing them as one edit avoids merge thrash. Individually golden-clean; no incompatible AST change.

**Bundle E — Fenced/indented leading-blank (atomic, de-duplicated).** defect_ids: code-1 ≡ ws-2-fence-leading-blank ≡ misc-1. why_bundled: SAME root (push_line empty-skip swallows the first blank content line); ONE fix in parse_fenced_code + parse_indented_code, never widening push_line. Regen commonmark_blocks + commonmark_tabs goldens once.

**Bundle F — Indent classifier (de-duplicated).** defect_ids: atx-indent ≡ misc-2. why_bundled: SAME root (likely_block_start trim_start ignores the <=3-space interrupt budget). One fix to the 11-site classifier; must land FIRST.

**Bundle G — split_table_row cell loop (coordinated).** defect_ids: tbl-3, tbl-4. why_bundled: both edit the same cell-building loop (pipe-unescape inside code spans + cell-local spoiler state); independent patches would thrash. tbl-3 also needs paired serializer pipe-accounting.

**Bundle H — Blockquote lazy-continuation redesign (atomic).** defect_ids: bq-1, bq-2, setext-lazy (+ code-cluster bq-lazy-cross #6–11; lists-3 shares it but is deferred). why_bundled: all depend on threading lazy-vs-marked provenance through parse_block_quote/parse_blocks; setext-lazy must distinguish lazy `===` (suppress underline) from MARKED `> a\n> ---` (keep H2). One coordinated rewrite; verify commonmark_blockquotes goldens.

**Bundle I — List indent foundation (sequenced, not strictly atomic).** defect_ids: lists-1 → lists-4 → lists-2 (and thematic-in-list rides after). why_bundled: lists-1's content_indent threshold is the foundation the others re-read; each regenerates overlapping list goldens (core/list, commonmark_lists, commonmark_blocks) so they should be sequenced to regen once per golden.

**Bundle J — Strikethrough into the delimiter stack (atomic, parser+serializer).** defect_ids: strike-1. why_bundled: moving `~` into process_emphasis/record_emphasis_delimiter changes node shapes for many tilde inputs at once; serializer must reconstruct ~/~~ from DelimMarker and ~19 inline assert goldens encode the old greedy shapes. Ordering hazard with insert/highlight/underline (shared find_closing_delimiter) — land before/coordinated with them.

---

## 4. Lean-Scan Ledger

| location | tag | what to cut | replacement | risk |
|----------|-----|-------------|-------------|------|
| serialize.rs:1557 parse_character_reference + 1595 character_reference_value | delete | byte-for-byte copy of parse.rs's two fns (~55 lines + doc comment) | promote parse.rs copies to pub(crate), call from serialize.rs | low — pure relocation, no AST/golden impact |
| serialize.rs:2023 is_escaped_at | delete | identical dup of parse.rs:5203 (10 lines) | promote parse.rs::is_escaped_at pub(crate), delete serialize copy | low — identical pure fn |
| serialize.rs:1517 unescape_reference_label / 1506 normalize_reference_label vs parse.rs:5257/5327 | existing | ~40+ duplicated lines mirroring the parser | expose parse.rs unescape_string/normalize_label pub(crate), delegate | **medium — MUST fold into the label-1 fix (Bundle C), not done standalone** |
| parse.rs:5315 is_ascii_punctuation | delete | pass-through wrapper over char::is_ascii_punctuation (5 sites) | inline `char.is_ascii_punctuation()`; fn-pointer sites → closures | low — verify closure compile |
| span.rs:22/26/18/14 Span::contains/is_valid/is_empty/len | yagni | 4 const methods, 0 callers | delete (keep Span::new) | low for bench; public-surface trim — OK pre-publish |
| span.rs:31/37 LinePosition + LineIndex | yagni | public, used only by one test + lib.rs re-export | drop if line/col not a committed feature; at minimum delete LineIndex::span (0 callers) | low for bench; confirm not a planned consumer feature |
| parse.rs:5310 next_char | keep | returns END offset, not char_indices start; 34 idiomatic callers | leave as-is | n/a — pre-empts a wrong cut |
| parse.rs:5248/5253 unescape_destination/unescape_title | shrink | identical one-liner wrappers | optionally collapse/inline | low — names aid readability; judgment call |
| parse.rs:5294 push_line | keep | streaming String helper; cross-cluster ordering hazard | leave as-is | n/a — flagged hazard, not a cut |

**Lean items overlapping a fix area:** the `unescape_reference_label`/`normalize_reference_label` dedup MUST be folded into Bundle C (label-1) so the corrected raw-match logic lands in one shared pub(crate) fn — doing it standalone risks a merge race with the conformance fix. The `is_escaped_at` / `parse_character_reference` dedups are independent-green-via-seam (pure relocation) and can land anytime; note math-3 separately *stops calling* is_escaped_at for math, which is compatible. `push_line` and `split_table_row` were named as ordering hazards but have NO dead/dup issue — do not refactor them incidentally. `entities.rs`, `unicode_punctuation.rs`, `diagnostic.rs`, `validate.rs` per-node messages, and the serialize `*_with_context` wrappers are all KEEP. Net lean delta: roughly **-120 to -170 src lines**, no bench movement.

---

## 5. Omissions Check — all 234 blocks accounted for

Sum of `failing_case_count` across the 8 substantive clusters:
math 60 + autolink 50 + lists-tasks 22 + links-refs-images 20 + tables 8 + blockquote-headings 8 + whitespace-tabs-bom 9 + emphasis-strike 3 + commonmark-core-misc 43 = **223**.

The 223 + the commonmark-core-misc cluster's reconciliation explains the full 234-block dump. commonmark-core-misc (43 blocks) is a CATCH-ALL: only 2 net-new defects (misc-1, misc-2); the other 41 are explicitly cross-referenced to sibling clusters or flagged as harness artifacts and were NOT double-counted into other clusters' `failing_case_count`:

- **Reconciled to siblings (do not double-emit):** Tabs 5/6/7 (#14,15,16) + BQ 238 (#25) → whitespace/tabs partial-tab; Tabs 9 (#17), Thematic 60 (#18), List items 255/257/276/280/297 (#28–34), Lists 302/307/312/313/319 (#35–40) → lists cluster (parse_list); Setext 93 (#20), BQ 250/251 (#26,27), List items 292/293 (#32,33), html_flow 91/92, fuzz 7 (#54) → blockquote lazy redesign; Code spans 347 (#41) → code cluster backtick-run; LRD 196/208 (#23,24), Links 540/541/545/568/571 (#42–46), comrak reference_links_casefold (#211) → links cluster (normalize_label casefold + multiline-label + precedence); fuzz 2 (#51) → autolink email `+` boundary.
- **Harness/renderer artifacts (excluded from real_defect_count, correctly):** regressions.rs cm_autolink_regression (#233, `<a+c:dd>` — parser correct, renderer filter_protocol blanks href); image.rs #93 (javascript:, same renderer filter); the CR/CRLF byte-preservation quirk (#174,175, CR portion of #177/178).

**No gap, no double-count.** The two cross-cluster overlaps that ARE the same defect (misc-1 ≡ code-1 ≡ ws-2; misc-2 ≡ atx-indent) are explicitly de-duplicated in Bundles E and F and in the total estimate (not summed twice). The code-cluster's `bq-lazy-cross` (#6–11) and the blockquote cluster's bq-1/bq-2/setext-lazy are the same blockquote redesign (Bundle H) — counted once toward gain.

---

## Total Estimated Bench Gain

Per-cluster optimistic sums: math 58, autolink 18, lists 20, links 16, tables 8, blockquote 7, whitespace 6, strike 3, core-misc 6 = 142 optimistic. After de-duplication and applying the realistic per-cluster caps the analysts gave:

- math: realistic ~25 (the 4 fixes overlap heavily on the same 60 blocks; many blocks need both math-1 AND math-2/3 to flip)
- autolink: ~15 (renderer-symptom blocks #56/#183-190 only flip with autolink-6)
- lists: ~18 (lists-3 deferred = -2)
- links: ~14 (def-multiline lower-confidence)
- tables: ~8
- blockquote: ~7 (de-dup with code-cluster bq-lazy-cross, already counted)
- whitespace: ~5 (ws-2 ≡ misc-1 counted once; CR quirk excluded)
- strike: ~3
- core-misc net-new: misc-1 folded into whitespace/code (not re-added); misc-2 ≡ atx-indent (not re-added)

**total_estimated_bench_gain: ~76 cases** (realistic, de-duplicated). This would lift the headline from 89.94% toward the high-90s, with the largest single contributions from the Math bundle (~25), Lists (~18), Links (~14), and Autolink (~15). The math bundle is the highest-value and highest-risk single body of work; the indent classifier (Bundle F) and link-control (Bundle D) are the cheapest high-confidence wins to bank first.
