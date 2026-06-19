---
date: 2026-06-19
status: completed
---

# markdown-syntax Conformance Fix — Task Graph (code-tasking)

> **Migration note (2026-06-20).** Promoted from the untracked `target/task-graph.md` scratch into the doc substrate when markdown-syntax moved to its own repo. Two edits accompany the promotion: (1) the executor rule *"NEVER add an HTML renderer to src/"* is **narrowed** (see the global rule below) to align with the active renderer plan, which feature-gates the renderer rather than touching the parser/AST/serializer these tasks own; (2) the upstream investigation citation is repathed to its new home. **The baseline numbers below (89.94% / 234 blocks) are a 2026-06-19 snapshot** — live conformance numbers are in `tests/html_conformance/CONFORMANCE.md`. The task body (T001–T025) is otherwise unchanged.

Plan: standing /goal (advance markdown-syntax parse↔serialize correctness; no HTML renderer in the *default* build — feature-gated only, parser/AST/serializer untouched) + user-authorized strategy "成对修 + 重生黄金快照" (paired parser+serializer fix + regenerate bench-verified `.ast`/`.canonical.md` goldens) + the consolidated investigation `docs/archive/2026-06-19-markdown-syntax-conformance-investigation.md`.

Baseline (verified this turn): headline **89.94%**, markdown-rs 91.14%, comrak 80.74%, 234 failure blocks; round-trip suite green. Est. total gain ~76 cases.

Execution order (leaf-first; run exactly in this order):
T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 → T010 → T011 → T012 → T013 → T014 → T015 → T016 → T017 → T018 → T019 → T020 → T021 → T022 → T023 → (T024 defer) → T025

Groups: F:T001 · autolink-extents:T002-T006,T021 · linkctrl:T007,T019 · tables:T008-T010 · ws/code:T011-T014 · lists:T015-T017 · labels:T018-T019 · math:T020 · attention:T022 · blockquote:T023-T024 · lean:T025

Global rule for the executor (every task): each task is one verified unit. Acceptance = (1) the round-trip suite `cargo test --no-default-features` stays GREEN (regenerate any named `.ast`/`.canonical.md` golden the fix legitimately moves — never edit a test to pass a wrong parse), AND (2) the bench headline does NOT drop and the targeted cases flip (`cargo test --no-default-features --test html_conformance -- --nocapture`), AND (3) `cargo fmt --check`, wasm32 check, and `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` stay green. A green build alone is NOT acceptance — the targeted oracle cases must move. If a fix's coupling proves larger than its task scope (a golden explosion or a cross-cluster regression that won't localize), REVERT it and report rather than banking a shim. Sweep any scratch `.rs` probe files before finishing. No HTML renderer in the DEFAULT build (preserve empty-default / zero-dep / no_std); renderer work must be feature-gated and must not modify the parser/AST/serializer these tasks own — see `docs/plans/2026-06-20-markdown-syntax-html-renderer.md`. NEVER conflate `:name`/`::name`/`:::name` directives with MDX.

Regen procedure (no bless flag exists): for a moved `assert_fixture` stem, regenerate `{stem}.ast` from `snapshot_document(parse(...))` and `{stem}.canonical.md` from `to_markdown(...)`, by writing a throwaway `#[test]` printer (or `examples/regen.rs`), capturing output, pasting it into the golden, then DELETING the throwaway. Verify the moved golden still reflects CORRECT structure (read it), not just "whatever the parser now emits."

---

### T001 — MODIFY `likely_block_start` to honor the ≤3-space block-interrupt budget (Bundle F: atx-indent ≡ misc-2)
- Group / Order / Depends-on: F / 1 / none
- Classification: independently-green-via-seam — the fix is internal to one classifier; callers keep compiling; only behavior on ≥4-space-indented lines changes. Foundational: 7+ later tasks (tbl-1, lists-1, bq-1/2, thematic-in-list) re-read its verdict, so it lands FIRST.
- Root-cause owner: `src/parse.rs:likely_block_start` @~5966 — it `trim_start`s before testing ATX/fence/thematic/list markers, so a ≥4-space-indented marker wrongly counts as a block start that interrupts a paragraph. Origin confirmed: the 11 call sites all consume its boolean; none re-encode the indent rule.
- Ripple set (this task): `likely_block_start` body only. Callers: `rg "likely_block_start" src/parse.rs` → 12 hits / 1 file (verify count at execution; all are boolean consumers that mechanically adapt). No AST/serialize/golden change (grep of failing input shapes returned 0 golden hits).
- Forbidden patch: do NOT add an `indent: usize`/`table_mode: bool` param, do NOT special-case `starts_with("    ")`, do NOT fix only inside `parse_paragraph`. Edit the classifier's own indent gate.
- Change: gate the indent-sensitive marker tests (ATX `#`, fenced ```` ``` ````/`~~~`, thematic break, list marker) behind a "leading indent ≤3 columns" check computed via the existing `trim_up_to_three_spaces` (indented code at ≥4 is NOT an interrupter). Leave non-indent constructs (blank, HTML flow already handled) intact.
- Definition of done: commonmark.rs #19 + heading_atx.rs #87 flip to pass; no other commonmark case regresses; exactly one classifier path; no new param/flag; no `if indent==N` shim. atx-indent and misc-2 are the SAME defect — both flip from this one edit.
- Verify: `cargo test --no-default-features --test html_conformance -- --nocapture` shows #87 + #19 fixed and headline ≥ baseline; `cargo test --no-default-features` green; `rg "table_mode|allow_block|indent:.*bool" src/parse.rs` → must be empty (no shim param added).

### T002 — MODIFY `is_gfm_email_domain` to reject a hyphen in the final label (autolink-7)
- Group / Order / Depends-on: autolink-extents / 2 / none
- Classification: independently-green-via-seam — one predicate, golden-clean (`gfm_autolinks.md` has the relevant inputs only in `.cases`, stability-only).
- Root-cause owner: `src/parse.rs:is_gfm_email_domain` — last-label validation is incomplete; GFM forbids autolinking an email whose final domain label contains `-`.
- Ripple set: `is_gfm_email_domain` body; `rg "is_gfm_email_domain" src/parse.rs` (verify single caller in the email-literal path). No golden.
- Forbidden patch: do NOT ban hyphens in ALL labels — only the final label.
- Change: extend the existing all-digit final-label check to also reject a final label containing `-`.
- Definition of done: gfm_autolink_literal.rs #61 flips; no other autolink case regresses; one predicate edit, no new helper.
- Verify: bench shows #61 fixed, headline ≥ baseline; round-trip green; fmt green.

### T003 — MODIFY `literal_link_end` to treat `]` and `[` as hard URL boundaries (autolink-3)
- Group / Order / Depends-on: autolink-extents / 3 / none
- Classification: independently-green-via-seam — golden-clean (`gfm_autolinks.md` has 0 `]`, verified in report).
- Root-cause owner: `src/parse.rs:literal_link_end` @~6246 — `]`/`[` are not in the hard-break set, so a literal URL run swallows bracket-delimited boundaries.
- Ripple set: `literal_link_end` hard-break loop; re-runs the existing paren/entity/trim fixpoint. No golden.
- Forbidden patch: do NOT add `]` to the trailing-TRIM set (that nibbles legitimate trailing chars) — add it to the hard-BOUNDARY set that stops the scan.
- Change: add `]`/`[` to the hard-break condition in `literal_link_end`; ensure the paren/entity/trim fixpoint still runs on the truncated run.
- Definition of done: gfm_autolink_literal #62,63,69,72,73,74,76 + comrak autolink #191,192,193,210 partial-flip (some need T004/T021); at least the bracket-boundary subset flips; no entity-run regression (the prior `;` trap stays fixed).
- Verify: bench shows the bracket-boundary cases fixed, headline ≥ baseline; round-trip green incl. `hg2_literal_link_excludes_trailing_entity_run`.

### T004 — MODIFY GFM email literal to use a left-boundary scan over the GFM local-part charset (autolink-1)
- Group / Order / Depends-on: autolink-extents / 4 / none
- Classification: independently-green-via-seam — golden-clean; adds a GFM-specific scan WITHOUT narrowing the shared angle-autolink atext.
- Root-cause owner: `src/parse.rs:is_email_local_part`/`parse_literal_email` — the GFM literal email uses RFC atext + forward scan; GFM requires a narrow charset `[A-Za-z0-9._+-]` and a LEFT boundary anchored at the start of that run.
- Ripple set: a NEW gfm-left-scan path in the literal-email branch; `rg "is_email_local_part|is_email_atext" src/parse.rs` (verify the angle-`<email>` path still uses the unchanged shared predicate). No golden.
- Forbidden patch: do NOT narrow `is_email_local_part`/`is_email_atext` in place — the angle `<email>` autolink path depends on the full RFC set. Add a separate GFM scan.
- Change: in the GFM literal path, left-scan from `@` over `[A-Za-z0-9._+-]` and anchor the autolink start at that run's left edge.
- Definition of done: gfm #70,#71 flip; angle-email tests (l2 contract) unchanged; shared atext predicate byte-identical.
- Verify: bench #70,#71 fixed; round-trip green incl. angle-email; `rg "is_email_atext" src/parse.rs` shows no signature/body change.

### T005 — MODIFY the literal-autolink preceding-char guard to branch per scheme (autolink-2)
- Group / Order / Depends-on: autolink-extents / 5 / T004
- Classification: independently-green-via-seam — golden-clean.
- Root-cause owner: `src/parse.rs:parse_literal_autolink` prefix guard @~6192 — a single preceding-char gate wrongly lets `_` block http/www and lacks the stricter www class.
- Ripple set: the prefix-guard branch; email left-extent already handled by T004. No golden.
- Forbidden patch: do NOT globally delete the `_` rejection — branch it per scheme.
- Change: split the preceding-char test into per-scheme branches (http(s)/www vs email), applying the correct boundary class to each.
- Definition of done: gfm #70,#71 remain green and any www/_ case flips; no regression to email path.
- Verify: bench ≥ baseline with the targeted cases green; round-trip green.

### T006 — MODIFY `gfm_autolink_domain_is_valid` to require a non-empty host with a valid first char (autolink-4)
- Group / Order / Depends-on: autolink-extents / 6 / T004, T005
- Classification: independently-green-via-seam — golden-clean; HIGH RISK (this is the −2-regression trap from the prior campaign).
- Root-cause owner: `src/parse.rs:gfm_autolink_domain_is_valid` — empty-host literals (`http://`+punct, bare `www.`) are not rejected.
- Ripple set: the validity predicate; depends on finalized literal extents from T004/T005. No golden.
- Forbidden patch: do NOT re-apply the naive empty/first-char reject that previously cost −2 on the bench. Compute host = segment before `/?#`, require non-empty + first char alnum; www requires a non-empty label after `www.`.
- Change: real host-presence logic as above.
- Definition of done: gfm #63,#74 + comrak #208,#209 flip; NET bench gain (no −2 regression); headline strictly ≥ before T006.
- Verify: bench headline must be ≥ the post-T005 number (this is the regression-trap gate); round-trip green. If it drops, REVERT T006 and report.

### T007 — MODIFY the link/resource parser control path (Bundle D: reffallback-1 + deftitle-eol-1 + destparen-1 + nul-dest-1)
- Group / Order / Depends-on: linkctrl / 7 / none
- Classification: independently-green-via-seam (one coherent edit to a shared path; all golden-clean). Bundled because all four touch `parse_link`/`parse_image`/`parse_link_resource`/`parse_link_destination`/`parse_link_title`/`contains_blank_line` — separate patches would merge-thrash.
- Root-cause owners: reffallback-1 `parse_link` @4081 / `parse_image` @3993; deftitle-eol-1 `contains_blank_line` @5095; destparen-1 `parse_link_destination` @5037; nul-dest-1 `parse_link_destination` @5045.
- Ripple set: the named functions; `rg "contains_blank_line|parse_link_destination|parse_link_title" src/parse.rs` (verify caller counts at execution). No golden (all golden-clean per report).
- Forbidden patch: per sub-defect — no if-let-retry that fixes only #96 (reffallback); no relaxing `contains_blank_line` to ignore ALL blanks (deftitle); no counting balanced parens / wrong cap (destparen); no local `\0` special-case diverging from NUL→FFFD (nul-dest).
- Change: (a) reffallback — `(` arm falls through to shortcut on None; a full ref with non-empty UNDEFINED label returns None (no shortcut fallback); mirror in parse_image. (b) deftitle — `contains_blank_line` true only on an INTERIOR blank (two consecutive EOLs), not boundary EOLs. (c) destparen — track paren depth, return None at >32. (d) nul-dest — NUL→U+FFFD (document-wide preprocess is cleanest; coordinate with T012 BOM entry point if both touch parse entry).
- Definition of done: definition.rs #47, link_reference #96,97,98,99..105 subset, link_resource #106,107,108, image #93-non-renderer flip; round-trip green; no shim branch per the forbidden list.
- Verify: bench shows the targeted link/def/resource cases fixed, headline ≥ baseline; round-trip green; fmt green.

### T008 — MODIFY `table_has_separator` to require a pipe for a single-column table (tbl-2)
- Group / Order / Depends-on: tables / 8 / none
- Classification: independently-green-via-seam — golden re-verify only.
- Root-cause owner: `src/parse.rs:table_has_separator` @5912 — a single-column loose table without any pipe is parsed as a table instead of setext/paragraph.
- Ripple set: `table_has_separator`; the serializer reuses `gfm_table_can_start_source` via `paragraph_table_escape_offset` so the change flows to the escape seam automatically (re-verify `gfm_table_edges`/`gfm_table_containers` unchanged).
- Forbidden patch: do NOT reorder dispatch so setext beats table globally; do NOT special-case bare `-`. Require an unescaped pipe when one column resolves (reuse `contains_unescaped_pipe`).
- Change: as above.
- Definition of done: gfm_table.rs #86 flips; multi-column tables unchanged; goldens re-verified unchanged.
- Verify: bench #86 fixed; round-trip green; read `gfm_table_*.ast` to confirm unchanged.

### T009 — ADD a table-body termination predicate used only by `parse_table` (tbl-1)
- Group / Order / Depends-on: tables / 9 / T001
- Classification: independently-green-via-seam — golden re-verify only.
- Root-cause owner: `src/parse.rs:parse_table` likely_block_start gate @2710 — uses paragraph-interruption rules instead of GFM row-continuation rules.
- Ripple set: a NEW `table_body_line_ends_table(line, options)` called ONLY at @2710; must NOT widen `likely_block_start` (T001 already corrected its indent budget).
- Forbidden patch: do NOT widen `likely_block_start`/`list_marker_can_interrupt_paragraph`; do NOT add a `table_mode: bool`. Use the dedicated predicate.
- Change: introduce the table-only predicate (ends on any list marker, HTML flow start, thematic/ATX/fenced/blockquote, blank).
- Definition of done: gfm_table.rs #80,81,82,85 flip; `likely_block_start` body byte-unchanged from T001; goldens re-verified.
- Verify: bench #80-82,85 fixed; `rg "likely_block_start" src/parse.rs` count unchanged vs post-T001; round-trip green.

### T010 — MODIFY `split_table_row` cell loop: unescape `\|` inside code spans + cell-local spoiler state (Bundle G: tbl-3 + tbl-4)
- Group / Order / Depends-on: tables / 10 / none (intra-bundle: tbl-3 then tbl-4)
- Classification: tbl-3 atomic-synchronized (paired serializer pipe-accounting); tbl-4 independently-green. Bundled: both edit the same cell-building loop.
- Root-cause owner: `src/parse.rs:split_table_row` @5813 (cell loop) + spoiler branch @5841-5858.
- Ripple set: `split_table_row` + paired serializer `table_cell_has_unescaped_pipe` (must not trip on code-protected pipes); `rg "split_table_row|table_cell_has_unescaped_pipe" src/` (verify). No golden regen (re-verify gfm_table_edges/containers).
- Forbidden patch: no renderer-side `\` stripping / post-hoc Code-node walking (tbl-3); no whole-cell `||...||` special-case / disabling the code_fence guard (tbl-4).
- Change: tbl-3 — collapse `\|`→`|` when emitting cell bytes inside AND outside code runs; serializer keeps code-protected pipes from tripping the unescaped-pipe check; verify idempotent reparse. tbl-4 — reset spoiler state at each cell boundary; append `||` open/close to cell text.
- Definition of done: gfm_table.rs #83,84 + spoiler.rs #234 flip; round-trip green + idempotent; goldens re-verified unchanged.
- Verify: bench #83,84,#234 fixed; round-trip green incl. serializer idempotency; fmt green.

### T011 — MODIFY `parse_setext_heading` to trim setext content lines leading-only (setext-hardbreak)
- Group / Order / Depends-on: ws/code / 11 / none
- Classification: independently-green-via-seam — golden-clean.
- Root-cause owner: `src/parse.rs:parse_setext_heading` @2769 — content lines are fully trimmed, destroying trailing-space hard breaks.
- Ripple set: the trim call @2769 → `trim_ascii_start` (leading-only). No golden (re-verify setext triples).
- Forbidden patch: no post-processing to inject HardBreak; no special-casing exactly 2 trailing spaces.
- Change: leading-only trim.
- Definition of done: heading_setext.rs #89 flips; other setext cases unchanged.
- Verify: bench #89 fixed; round-trip green; setext goldens re-verified.

### T012 — MODIFY `parse_with_options` to strip a single leading BOM (ws-1-bom)
- Group / Order / Depends-on: ws/code / 12 / none
- Classification: independently-green-via-seam — golden-clean (no fixture starts with BOM).
- Root-cause owner: `src/parse.rs:parse_with_options` @96 — a leading U+FEFF is not stripped.
- Ripple set: one `strip_prefix('\u{feff}')` at the top; the shadowed slice feeds `collect_definitions`/`parse_blocks`/Document meta span (offsets stay aligned). If T007's nul-dest preprocess also edits parse entry, coordinate to one preprocessing site.
- Forbidden patch: NO global `input.replace('\u{feff}','')` (deletes interior BOM in #173).
- Change: `let input = input.strip_prefix('\u{feff}').unwrap_or(input);` once at entry.
- Definition of done: misc_bom.rs #172,173 flip; interior BOM (#173) preserved.
- Verify: bench #172,173 fixed; round-trip green.

### T013 — MODIFY `strip_leading_indent_columns` to partially consume a tab when a fence is indented (ws-3-partial-tab-fence)
- Group / Order / Depends-on: ws/code / 13 / none
- Classification: independently-green-via-seam — golden re-verify (commonmark_tabs).
- Root-cause owner: `src/parse.rs:strip_leading_indent_columns` @5554 — never partially consumes a tab when the fence is indented, losing residual columns.
- Ripple set: return type becomes owned `String`/`Cow` when a budget-crossing tab expands to residual spaces; sole caller `parse_fenced_code` @625 adapts. `rg "strip_leading_indent_columns" src/parse.rs` (verify single caller). Sequence BEFORE T014 (both edit parse_fenced_code).
- Forbidden patch: no single-space-only fence-indent special case; do NOT keep the borrowed `&str` with the tab whole.
- Change: expand a partially-consumed tab into residual spaces, return owned when needed.
- Definition of done: misc_tabs.rs #179,180,181 flip; commonmark_tabs golden re-verified unchanged (indented-fence lines still parse as indented code).
- Verify: bench #179-181 fixed; round-trip green; read commonmark_tabs.ast unchanged.

### T014 — MODIFY `parse_fenced_code` + `parse_indented_code` to preserve a leading/sole blank content line (Bundle E: code-1 ≡ ws-2 ≡ misc-1)
- Group / Order / Depends-on: ws/code / 14 / T013
- Classification: atomic-synchronized (2 goldens regen) — SAME root across three clusters.
- Root-cause owner: `src/parse.rs:parse_fenced_code` @623 (and `parse_indented_code`) — the `push_line` empty-skip swallows the first blank content line because the leaf loop can't distinguish "no content yet" from "leading blank line".
- Ripple set: the two leaf loops ONLY; `push_line` (23 callers) MUST NOT change body/signature (`rg "push_line" src/parse.rs` → confirm 23). Goldens: regen `spec/commonmark_blocks.{ast,canonical.md}`, `spec/commonmark_tabs.{ast,canonical.md}`.
- Forbidden patch: do NOT change `push_line` or add an `allow_empty`/`is_code` bool to it. Fix locally in each leaf loop by tracking "content started" state.
- Change: each code leaf loop tracks whether content has begun; a leading/sole blank is preserved as content rather than skipped.
- Definition of done: commonmark.rs #21,22,39,52,53 + code_fenced/code_indented + misc_line_ending #176 flip; `push_line` byte-unchanged; 2 goldens regenerated and READ to confirm correct structure (blank first line now present).
- Verify: bench shows the leading-blank cases fixed, headline ≥ baseline; round-trip green; `rg "fn push_line" src/parse.rs` body unchanged; goldens reflect the corrected structure.

### T015 — MODIFY `parse_list` indent foundation (Bundle I: lists-1 → lists-4 → lists-2)
- Group / Order / Depends-on: lists / 15 / T001
- Classification: atomic-synchronized (lists-1 regens 6 goldens; the three are sequenced to regen each overlapping golden once).
- Root-cause owner: `src/parse.rs:parse_list` @889/@918 (continuation/sublist indent keys off `marker.indent` not `content_indent`); blank branch @881/@907 (lists-4 in-fence blank discarded); non-blank path @913-934 (lists-2 different-delimiter marker).
- Ripple set: `parse_list` + `strip_list_continuation` + `list_marker_info` + `leading_indent_columns` + `sibling_list_marker_at_line`; `rg "content_indent|strip_list_continuation|list_marker_info" src/parse.rs` (verify). Goldens: `core/list`, `spec/commonmark_lists`, `spec/commonmark_blocks` (lists-1); `core/list`,`spec/commonmark_lists` (lists-4); `spec/commonmark_lists` (lists-2) — regen each ONCE after all three land.
- Forbidden patch: no `if leading_indent==1 break` off-by-one shim (lists-1); no special-casing all-spaces/tab or post-processing CodeBlock (lists-4); no hardcoded `)`-delimiter detection (lists-2).
- Change: lists-1 — compare continuation/sublist indent against `content_indent` (`>= content_indent`), blank branch ends item when next non-blank indent `< content_indent`. lists-4 — when `open_fence.is_some()`, do not treat whitespace-only as a blank separator; route through `strip_list_continuation`, push each line. lists-2 — break the item when `list_marker_info` is Some but not a same-list sibling.
- Definition of done: list.rs #109,110,111,112,115,116,118,119,120,122-128 subset flip; round-trip green; 6 goldens regenerated and READ for correctness; no off-by-one shim.
- Verify: bench shows the list cases fixed, headline ≥ baseline; round-trip green; goldens reflect correct nesting/loose-tight.

### T016 — MODIFY `parse_list` outer loop to let a thematic break end the list (thematic-in-list)
- Group / Order / Depends-on: lists / 16 / T001, T015
- Classification: independently-green-via-seam.
- Root-cause owner: `src/parse.rs:parse_list` outer loop @862 — `* * *` inside a list is consumed as a nested item instead of ending the list.
- Ripple set: a precedence guard at the outer loop (check `parse_thematic_break` before accepting a list marker). No golden (0-hit grep).
- Forbidden patch: do NOT make `list_marker_info` return None for `* * *` — precedence belongs at the call site.
- Change: at the list outer loop, test `parse_thematic_break` before accepting a marker line.
- Definition of done: thematic_break.rs #182 flips; lists unaffected.
- Verify: bench #182 fixed; round-trip green.

### T017 — MODIFY description-list tightness + paired serializer (lists-5)
- Group / Order / Depends-on: lists / 17 / none
- Classification: atomic-synchronized (paired serializer, 3 goldens regen).
- Root-cause owner: `src/parse.rs:parse_description_details`/`parse_description_list` — tightness over-loosened by a term-group-separating blank.
- Ripple set: parser tightness decision + `serialize_description_list` (serialize.rs ~451-468); goldens `extensions/description_lists_{core,blocks,edges}.{ast,canonical.md}` (core has a blank-separated term group → golden WILL move).
- Forbidden patch: do NOT force tight=true unconditionally; do NOT strip the inter-term blank.
- Change: do not set tight=false for blanks followed by a new term/marker; pair the serializer so it does not emit a loose-making blank where the model is tight; regen 3 triples.
- Definition of done: description_lists.rs #212,213 flip; round-trip green + idempotent; 3 triples regenerated and READ.
- Verify: bench #212,213 fixed; round-trip green; goldens correct.

### T018 — MODIFY `normalize_label` to match the RAW label + paired serializer (Bundle C: label-1 + lean normalize dedup)
- Group / Order / Depends-on: labels / 18 / none
- Classification: atomic-synchronized (parser + serializer in lockstep, 1 golden regen). This is the documented prior-revert; it only works coupled.
- Root-cause owner: `src/parse.rs:normalize_label` @5327 — it unescapes the label before matching; CommonMark matches the RAW label (casefold + whitespace-collapse only).
- Ripple set: `normalize_label` (drop the `unescape_string` call) AND serializer `normalize_reference_label`/`unescape_reference_label` (serialize.rs ~1506/1517) in lockstep — the omission oracle compares `normalize_reference_label(children)==node.identifier`. Fold the lean dedup (expose `unescape_string`/`normalize_label` pub(crate), delegate) INTO this fix. Golden: regen `spec/commonmark_reference_labels.{ast,canonical.md}` (identifier `foo]`→`foo\]`, `a & b`→`a &amp; b`); verify `gfm_footnote_edges` stays stable.
- Forbidden patch: no options flag / second "raw" normalize variant; no special-casing `find_definition` while leaving `unescape_string`; the lean dedup must NOT be a standalone edit (merge-race).
- Change: fold/collapse the RAW label (`split_whitespace().join(" ")` then casefold); make the serializer stop unescaping; dedup the parser/serializer copies via pub(crate); regen the one spec triple.
- Definition of done: link_reference.rs #95,99-105 subset flip; round-trip green incl. gfm_footnote_edges; 1 triple regenerated and READ; serializer no longer unescapes labels (`rg "unescape_reference_label" src/serialize.rs` → gone or delegating to parse.rs).
- Verify: bench shows label cases fixed, headline ≥ baseline; round-trip green; `rg "fn unescape_reference_label|fn normalize_reference_label" src/serialize.rs` shows the dedup landed.

### T019 — MODIFY definition parsing to accept a multi-line LABEL (def-multiline)
- Group / Order / Depends-on: labels / 19 / T018
- Classification: independently-green-via-seam — lower-confidence (#49 tab/indent semantics).
- Root-cause owner: `src/parse.rs:parse_definition` @1334 / `find_reference_label_end` @4180 — a label spanning lines is not recognized.
- Ripple set: definition label accumulation; same path as T018. May need list-prefix coordination for #49.
- Forbidden patch: no first-line-only shim that ignores continuation indent semantics.
- Change: when `find_reference_label_end` fails on the first line, accumulate continuation lines before locating `]:`.
- Definition of done: definition.rs #48,49,50 + link_reference #94 flip (or #49 documented as deferred if indent semantics don't localize); round-trip green.
- Verify: bench shows the multi-line-label cases fixed; round-trip green. If #49 won't localize, scope it out and report.

### T020 — REPLACE the math subsystem: AST discriminants + block/inline parsers + paired serializer (Bundle A: math-1 → math-2 → math-3 → math-4)
- Group / Order / Depends-on: math / 20 / none (internal: math-1 → math-2 → math-3+math-4)
- Classification: atomic-synchronized — an AST struct-field add cannot be seam-isolated; every construction/match/snapshot site + 2 Math goldens move in one bundle. Highest value (~25) and highest risk.
- Root-cause owners: `src/ast.rs` MathInline/MathBlock (no inline/display, dollar/code, or meta discriminant) — math-1; `parse.rs:parse_math_block` (+`is_math_block_fence` @584) is a stub not a fenced-code analogue — math-2; `parse.rs:parse_math_inline` @4890 (+@4930) is a broken code-span clone — math-3; inline math dispatch @3717 ignores single-dollar/comrak flanking — math-4.
- Ripple set: math-1 — MathInline/MathBlock structs + 4 parse construction sites + serialize Math arms (@199,@856,@1915,@1926,@2034) + validate (@99,@275) + `snapshot_document` (fixtures.rs @1311,@1495) + test renderer (inlines.rs/blocks.rs ~23 hits, NOT production) + goldens `extensions/math_edges.{ast,canonical.md}` + `extensions/table_math_directive.{ast,canonical.md}`. Run `rg "MathInline|MathBlock" src/ tests/fixtures.rs` to itemize before editing.
- Forbidden patch: NO render-time heuristic re-inspecting `value` to fake display/code; NO copy-paste of `parse_fenced_code` into `parse_math_block`; NO single-space-trim hack (math-3); NO hardcoding a test RenderConfig engine flag into the production parser (math-4 → use ParseOptions/Constructs).
- Change: math-1 — add a discriminant to MathInline (`Dollar{display:bool}` | `Code`) and a meta/info marker to MathBlock; populate from the actual delimiter at parse time; serializer chooses the fence from the discriminant (kills the always-`$$` upgrade). math-2 — rebuild `parse_math_block` to mirror `parse_fenced_code` (≥2-`$` run + optional meta, indent record, ≥-length/EOF/indent-aware close, blank preservation, blockquote stripping); route comrak `$$..$$` inline appropriately. math-3 — reimplement `parse_math_inline` as a code-span (opening run length N, exact-N close on a run boundary, code-span padding rule, line-endings→spaces, NO backslash escapes), keep `$`-code-math via math-1's discriminant; pair serializer. math-4 — encode dialects via ParseOptions/Constructs (single_dollar gate + comrak digit-flanking guard), land jointly with math-3. Regen both Math triples (all 4 files).
- Definition of done: math_flow.rs #129-158, math_text.rs #159-171, comrak math.rs #216-232 majority flip (target ~25 net); round-trip green + idempotent; serializer no longer force-upgrades `$x$`→`$$x$$` (`extensions/math_edges.canonical.md` now matches the single-dollar input); 2 Math goldens (4 files) regenerated and READ for correctness; `rg "MathInline|MathBlock" src/` shows the discriminant populated at all construction sites (no default-variant shim).
- Verify: bench shows the math cluster jump and headline rise; round-trip green; fmt + wasm32 + doc green; goldens reflect correct display/code/meta. If coupling explodes (golden cascade beyond the 2 Math triples), split math-1→math-2 from math-3→math-4 into two sequential commits but keep each atomic; never leave a default-variant shim.

### T021 — ADD an Autolink node kind + recognize mailto:/xmpp: literals (Bundle B: autolink-6 + autolink-5)
- Group / Order / Depends-on: autolink-extents / 21 / T003, T004, T005, T006
- Classification: atomic-synchronized — adds a `kind` enum to the single Autolink node (AST change), rippling through parse/serialize/validate/snapshot + 3 `.ast` triples. autolink-5's display half only works once the node distinguishes literal vs synthesized scheme.
- Root-cause owner: `src/ast.rs:Autolink` struct (no kind/original-text → lossy `<dest>` serialize + renderer guesswork) — autolink-6; `parse.rs:parse_literal_autolink` scheme dispatch — autolink-5.
- Ripple set: Autolink struct + parse (2 sites) + serialize arm @845 + `validate_autolink` (relax `>` for literal only) + `snapshot_document` + every `.ast` line printing an Autolink + test renderer (visible_text/refs.rs, NOT production); goldens `spec/commonmark_autolinks`, `spec/commonmark_inlines`, `extensions/gfm_autolinks` (`.ast`+`.canonical.md`). `rg "Autolink" src/ tests/fixtures.rs` to itemize.
- Forbidden patch: NO "add mailto: in parser" on the bare-email synthesis path (KNOWN-WRONG, breaks the l2 angle-email contract); NO patching only renderer `visible_text` to fake case-56/#183-190.
- Change: add `kind { Angle, GfmLiteral{original} }` to Autolink; Angle→`<dest>` serialize, GfmLiteral→raw original text; validate relaxes `>` for literal only; add a mailto:/xmpp: literal branch (case-insensitive, preceding-non-alnum guard) recorded as GfmLiteral; regen 3 triples.
- Definition of done: autolink #55,56 + gfm #62,72,73,74 residual + comrak #183-190 flip; round-trip green incl. angle-email (l2); 3 triples regenerated and READ; `validate_autolink` relaxes `>` ONLY for the literal kind.
- Verify: bench shows the autolink-kind cases fixed, headline rise; round-trip green incl. l2; goldens correct.

### T022 — MODIFY strikethrough to participate in the shared delimiter stack + paired serializer (Bundle J: strike-1)
- Group / Order / Depends-on: attention / 22 / none (ORDERING HAZARD: land before/coordinated with insert/highlight/underline via `find_closing_delimiter`)
- Classification: atomic-synchronized (paired serializer + ~19 inline assert goldens). No ast.rs variant change (Delete/DeleteMarker/DelimMarker exist).
- Root-cause owner: `src/parse.rs:parse_inlines_with_context` tilde branches @~3602/3630 — `~`/`~~` are parsed greedily and committed as a finished `Inline::Delete` instead of entering the unified delimiter stack.
- Ripple set: replace the greedy tilde branches with record-only branches pushing a `~` DelimMarker (extends `record_emphasis_delimiter`); teach `process_emphasis` to pair `~`/`~~` runs; touches `find_closing_delimiter` (6 callers ___/__/++/==/~~ — coordinate), `delimiter_flanking`/`is_flanking_punctuation`, serialize Delete/DeleteMarker arms, validate Delete arm; respect `single_tilde_strikethrough` + subscript precedence. Goldens: ~19 inline `assert_eq` strike goldens in `inline_delimiter_regressions.rs`, `serializer_regressions.rs`, `review_validate_regressions.rs`; check `extensions/inline_markup_extras`. `rg "Delete|DeleteMarker|~" tests/*regression*.rs` to itemize.
- Forbidden patch: no more special cases in the greedy scanners; no `~`-aware shim in `find_closing_delimiter`; no second parallel strike stack (interleaving needs ONE stack).
- Change: as above — `~` becomes a delimiter run in the shared stack.
- Definition of done: gfm_strikethrough.rs #77,78,79 flip; round-trip green + idempotent; ~19 inline goldens regenerated and READ (correct `~`/`~~` reconstruction); subscript precedence intact.
- Verify: bench #77-79 fixed; round-trip green; goldens correct; `rg "Inline::Delete" src/parse.rs` shows it is built by `process_emphasis`, not the old greedy branch.

### T023 — REPLACE the block-quote lazy-continuation model with lazy-vs-marked provenance (Bundle H: bq-1 → bq-2 → setext-lazy)
- Group / Order / Depends-on: blockquote / 23 / T001
- Classification: atomic-synchronized — threading lazy provenance through `parse_block_quote`/`parse_blocks` is one coordinated redesign; subsumes the code-cluster bq-lazy-cross (#6-11). HIGH RISK (the report's biggest behavioral change after math).
- Root-cause owner: `src/parse.rs:parse_block_quote` trim guard @662 (bq-1: drops a ≥4-space lazy paragraph continuation), `block_quote_paragraph_stays_open` @820 + re-parse @744 (bq-2: lazy fails after a nested quote because the innermost open paragraph isn't tracked), `parse_setext_heading` @2750 on re-parsed content (setext-lazy: underline wrongly applied to a lazy continuation line).
- Ripple set: add a `lazy` flag to `struct Line` (or per-level open-paragraph state) threaded through `parse_block_quote`/`parse_blocks`; `block_quote_paragraph_stays_open`; `parse_setext_heading` must distinguish lazy `===`/`---` (suppress underline) from MARKED `> a\n> ---` (keep H2). `rg "struct Line|parse_blocks|block_quote_paragraph_stays_open" src/parse.rs` to itemize. Goldens: `spec/commonmark_blockquotes.{ast,canonical.md}` VERIFY-ONLY (MARKED setext H2 must stay); re-verify setext triples.
- Forbidden patch: no narrow `if leading_indent>=4 take as-is` skipping the paragraph_open gate (bq-1); no `lazy_after_nested_quote` bool shim (bq-2); no adding `===`/`---` to `likely_block_start` break conditions (setext-lazy).
- Change: bq-1 — don't BREAK on `trim_up_to_three_spaces==None`; route the failing continuation through the lazy branch using RAW line text. bq-2 — thread lazy provenance / per-level open-paragraph state through `parse_blocks`. setext-lazy — suppress the underline when the underline line is a lazy continuation; keep it when the `---` is itself marked.
- Definition of done: block_quote.rs #1,2,3 + heading_setext.rs #88,90 + the code-cluster bq-lazy-cross spec cases flip; round-trip green; commonmark_blockquotes golden VERIFIED unchanged (MARKED setext H2 intact); no bool shim.
- Verify: bench shows the blockquote/lazy cases fixed, headline rise; round-trip green; `rg "lazy" src/parse.rs` shows provenance threaded through `struct Line`/`parse_blocks`, not a local shim; commonmark_blockquotes.ast unchanged.

### T024 — (DEFER) lists-3 cross-container lazy propagation
- Group / Order / Depends-on: blockquote / 24 / T023
- Classification: independently-green-via-seam — DEFER. Architectural container-stack parse; only 2 cases (list.rs #113,114), high blast radius, shares T023's redesign.
- Decision: do NOT attempt unless T023 already generalizes to it cleanly. If attempted, it MODIFYs the container line-collection model, not a bolt-on to parse_list. Otherwise report as a documented known-gap (2 cases).

### T025 — DELETE duplicated parser/serializer helpers (lean cleanup, independent)
- Group / Order / Depends-on: lean / 25 / (normalize dedup already folded into T018)
- Classification: independently-green-via-seam — pure relocation, no AST/golden/bench impact.
- Root-cause owner: serialize.rs duplicates of parse.rs functions.
- Ripple set: `serialize.rs:parse_character_reference` @1557 + `character_reference_value` @1595 (byte copies of parse.rs) → promote parse.rs copies pub(crate), delete serialize copies; `serialize.rs:is_escaped_at` @2023 (dup of parse.rs:5203) → promote, delete; `parse.rs:is_ascii_punctuation` @5315 (pass-through over `char::is_ascii_punctuation`, 5 sites) → inline; optional: `span.rs` 0-caller const methods (`Span::contains/is_valid/is_empty/len`, `LineIndex::span`) → delete pre-publish (confirm not a planned feature first). `rg "parse_character_reference|is_escaped_at|is_ascii_punctuation" src/` to itemize.
- Forbidden patch: do NOT change behavior; pure relocation only. Do NOT delete `push_line`, `split_table_row`, `next_char`, entities.rs, unicode_punctuation.rs (KEEP per lean ledger).
- Change: promote the parse.rs originals to pub(crate), delete the serialize.rs duplicates, inline the `is_ascii_punctuation` wrapper.
- Definition of done: ~-120 to -170 src lines; round-trip + bench unchanged (no movement); `rg "fn parse_character_reference|fn is_escaped_at" src/serialize.rs` → empty (only parse.rs defines them).
- Verify: full suite + bench identical to pre-T025; `rg` shows no remaining duplicate definitions.

---

## Self-Review (code-tasking)
- Adversarial test: each task names its specific forbidden patch (not generic) and a negative DoD with a pasteable `rg` check; the atomic bundles (T014/T015/T017/T018/T020/T021/T022/T023) name the goldens that move so a "kept stale golden" can't pass as done.
- Every classification carries the investigation's grounded ripple set (grep counts to be re-confirmed at execution).
- Order is strictly foundation-first: T001 (classifier) before all indent/lazy/table dependents; T013 before T014 (shared fn); math-1 before math-2/3; autolink extents (T002-T006) before the kind enum (T021); label parser+serializer locked in T018.
- No seam/expand-migrate-contract chain is used — every change is reachable in-repo, so all are atomic or internal-seam; no published/persisted external consumer exists.
- Verbs are MODIFY/REPLACE/ADD-predicate/DELETE; no "support/handle the new case" framing.
- Backward-compat is forbidden by default; the only "compat" (round-trip stability) is the inherited correctness bar, upheld via paired serializer + golden regen, not a dual path.
