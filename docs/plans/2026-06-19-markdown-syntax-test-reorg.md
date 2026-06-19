---
date: 2026-06-19
status: completed
---

# markdown-syntax test reorganization

> **Implemented shape.** The current test tree is role-separated:
> `tests/fixtures/roundtrip/` owns parse/serialize stability fixtures and
> generated round-trip case snapshots; `tests/fixtures/conformance/{commonmark,gfm}/`
> owns byte-counted HTML oracle cases for the measurement bench. The engine-owned
> `markdown-rs/` / `comrak/` fixture split is absent from the current tree.
> Current conformance numbers live in `tests/html_conformance/CONFORMANCE.md`.
> The private renderer remains under `tests/html_conformance/renderer/` until
> `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` lands.

The sections below preserve the proposal that led to the implemented layout.

Goal: satisfy the user's 3 hard constraints —
1. stop distinguishing the `markdown-rs/` and `comrak/` directories,
2. more stable + lower cognitive load,
3. not messy / not deeply nested —
without breaking either consumer (`tests/fixtures.rs` round-trip stability AND
the `tests/html_conformance` bench correctness) and without losing the engine
information the bench needs (markdown-rs vs comrak expected-HTML dialect +
`RenderConfig`).

---

## 0. The one fact that makes this safe

The bench does **not** derive engine from the directory. `extractor/mod.rs`
runs `markdown_rs::extract` on every entry produced by the `md_src!` macro/list
and `comrak::extract` on every entry produced by `cm_src!`. Engine = which
list/macro an entry lives in (a code-level decision), stamped onto
`OracleTuple.engine` at construction. The directory NAME is load-bearing in
exactly two ways:

- the `include_str!` prefix inside each macro (compile-time file location), and
- the `source_file` string the macro stores (reporting + the 652 `.ends_with`
  anchor in `html_conformance.rs`).

For `fixtures.rs`, engine reaches the consumer only through the `.cases`
`origin:` header, which is then **path-joined** as `corpus/{origin}/{source}`
in `assert_source_exists`. So `origin` must keep resolving to a real on-disk
location.

Conclusion: we can merge the two engine directories if we (a) resolve 3 basename
collisions, (b) update the two `include_str!` prefixes + `source_file` prefixes,
(c) rebase the `origin`/`source` path-join, and (d) keep engine alive in the
extractor lists (already true) and in the `.cases` `origin:` header (already
true). Nothing about `RenderConfig`, `runner.rs`, `report.rs`, `types.rs`
selection needs to change — they all key on `OracleTuple.engine`, never the path.

---

## 1. Recommended target layout (flat, engine-agnostic, role-separated)

```
tests/
├── fixtures.rs                      # round-trip-stability consumer (unchanged role)
├── html_conformance.rs              # bench entry (#[path] decls; unchanged role)
├── html_conformance/                # bench pipeline (already clean, 2 levels — KEEP AS-IS)
│   ├── types.rs  normalizer.rs  runner.rs  report.rs  CONFORMANCE.md
│   ├── extractor/{mod,lexer,markdown_rs,comrak}.rs
│   └── renderer/{mod,blocks,escape,footnotes,inlines,refs,tables}.rs
├── regressions/                     # the 11 inline-AST regression files, grouped by SUBJECT
│   ├── parse_inline.rs              # = emphasis + inline_delimiter + review_inline + review_unicode
│   ├── parse_block.rs               # = review_block + parser
│   ├── serialize.rs                 # = serializer + serializer_escape + review_serialize
│   └── validate.rs                  # = validation + review_validate
└── fixtures/
    ├── roundtrip/                   # ROLE A: engine-agnostic round-trip triples (.md/.ast/.canonical.md)
    │   ├── core/                    #   3 stems (package smoke)
    │   ├── spec/                    #   20 commonmark_* stems
    │   ├── extensions/              #   33 ext stems
    │   └── stability/               #   the 8 comrak/upstream/*.md standalone stability inputs
    └── conformance/                 # ROLE B: HTML-oracle material + generated executable cases
        ├── commonmark-examples/     #   official-inputs.cases + official-stable-inputs.cases (engine-free)
        ├── oracles/                 #   the vendored upstream .rs (engine = per-file metadata, see §6)
        │   ├── upstream-tests/      #     merged comrak+markdown-rs test files, collisions renamed
        │   ├── upstream-fuzz/       #     merged fuzz files (audit-only)
        │   ├── mdast-to-markdown/   #     markdown-rs mdast tests (audit-only)
        │   └── LICENSES/            #     COPYING.comrak, LICENSE.markdown-rs, NOTICE.md
        └── cases/                   #   generated executable .cases (the live semantic corpus)
            ├── MANIFEST.md
            └── *.cases              #     flat: <stem>.cases, engine in `origin:` header (no engine dir)
```

Per-top-dir rationale (one line each):

- `tests/regressions/` — collapses 11 confusingly-named flat files (`review_*`
  vs plain, two orthogonal axes) into 4 subject files; pure cosmetic, zero path
  coupling.
- `tests/html_conformance/` + `.rs` — already a clean 6-stage 2-level pipeline;
  leave the module tree untouched, only its corpus include paths change.
- `tests/fixtures/roundtrip/` — ROLE A. Everything `fixtures.rs` round-trips for
  stability, engine-free, by construct (core/spec/extensions/stability).
- `tests/fixtures/conformance/` — ROLE B. Everything the bench + the generated
  case pipeline consume; the only place engine exists, and it lives as metadata,
  not as a directory.

The merge of the two engine dirs happens inside `conformance/oracles/` (the
vendored `.rs`) and `conformance/cases/` (the generated `.cases`): both become
single flat namespaces with engine demoted to per-file metadata.

---

## 2. Key decision (the single biggest fork)

**Question:** how does engine survive once `comrak/` and `markdown-rs/` are no
longer directories — and specifically how does `fixtures.rs`'s
`assert_source_exists` (which joins `corpus/{origin}/{source}`) keep resolving?

Options:

- **(A) In-list engine tag, no engine subdir at all (full flat).** Drop the
  prefix from `md_src!`/`cm_src!`, point both at a single merged
  `conformance/oracles/upstream-tests/`, rename the 3 colliding basenames, and
  change the `.cases` `source:` header to the new flat relative path so
  `assert_source_exists` joins `corpus_root/source` (origin no longer a path
  segment, only a count-bucket label). Engine in bench = the list; engine in
  cases = the `origin:` header (label only).
- **(B) Keep engine as a per-file filename suffix** (`autolink.comrak.rs`,
  `autolink.markdown-rs.rs`). Resolves collisions for free, engine is greppable
  in the filename, but every `include_str!` literal and every `.cases` `source:`
  string must carry the suffix; arguably reintroduces the distinction the user
  wants gone, just at filename granularity.
- **(C) Sidecar manifest mapping basename→engine**, files fully flat with no tag.
  Most invisible, but adds a new parsed file and a new failure mode, and the
  bench lists already encode engine so the manifest would be redundant for the
  bench.

**Recommendation: (A).** It is the truest reading of constraint (1) — no engine
in any path — while costing the least: engine already lives in the extractor
lists (bench) and in the `origin:` header (cases), so we are *removing* the
redundant directory carrier, not adding a new one. The only genuinely new work
is renaming 3 collisions and rebasing the `source:`/`origin` join. (B) keeps a
per-file engine token in the name, which the user explicitly wants to stop
seeing; (C) adds a parsed sidecar with no payoff because the bench does not need
it.

The sub-fork inside (A): **should `origin` stay a path segment or become a label
only?** Recommendation: make it a **label only** and store the full flat
relative path in the `.cases` `source:` field, so `assert_source_exists` becomes
`corpus_root.join(&metadata.source)`. This is the change that lets the engine
dir truly disappear; keeping origin as a path segment would force a
`conformance/oracles/<engine>/...` subdir and re-create the split.

---

## 3. Alternatives considered (rejected)

- **Keep per-engine subfolders under a merged parent**
  (`conformance/oracles/{comrak,markdown-rs}/upstream-tests/`). Lowest risk —
  resolves collisions automatically, only the two `include_str!` prefixes change
  and `assert_source_exists` still works with a one-segment rebase. **Rejected as
  the headline** because it violates constraint (1): the engine directories
  still exist, just one level deeper. (Offered as the safe fallback if the user
  rejects collision renames.)
- **Single fully-flat `tests/fixtures/` with no role split** (corpus + cases +
  oracles all in one dir). **Rejected:** mixes three distinct consumer contracts
  (round-trip triples, HTML oracles, generated cases) into one bag, raising
  cognitive load and making the `collect_files` walks ambiguous — the opposite of
  constraint (2). Role separation is the cheaper clarity win than engine merging.
- **Delete the legacy `derived-cases/` tree entirely** and rely only on
  `semantic-inputs`. Tempting (it is ~113 files that exist mostly to satisfy two
  magic-number asserts) but **out of scope** for "stop distinguishing the dirs";
  flagged as a follow-up cleanup, not part of this reorg, because it changes test
  coverage semantics, not layout.

---

## 4. Migration steps (ordered, git mv granularity)

Collisions to resolve up front (real on-disk collisions in `upstream-tests/`):
`autolink.rs`, `commonmark.rs`, `fuzz.rs` exist in BOTH engines. Same 3 (plus
the consumed subset) collide as `.cases`. Resolution: keep the markdown-rs name
bare, suffix the comrak one with `__comrak` (engine token in the few colliding
names only, not a blanket scheme) — OR keep both bare under a thin
`upstream-tests/{md,cm}/` only for the 3 collisions. Recommended: explicit
rename of the comrak collision files.

1. `git mv tests/fixtures/corpus/core tests/fixtures/roundtrip/core`
2. `git mv tests/fixtures/corpus/spec tests/fixtures/roundtrip/spec`
3. `git mv tests/fixtures/corpus/extensions tests/fixtures/roundtrip/extensions`
4. `git mv tests/fixtures/corpus/comrak/upstream tests/fixtures/roundtrip/stability`
   (this is the live copy `fixtures.rs:320` reads; the dead
   `comrak/upstream-tests/fixtures/` byte-duplicate is NOT moved — delete it,
   step 13).
5. `git mv tests/fixtures/corpus/commonmark-examples tests/fixtures/conformance/commonmark-examples`
6. Create `tests/fixtures/conformance/oracles/upstream-tests/`. `git mv` all
   `markdown-rs/upstream-tests/*.rs` into it (bare names).
7. `git mv` all `comrak/upstream-tests/*.rs` into the same dir, renaming the 3
   collisions: `autolink.rs→autolink__comrak.rs`,
   `commonmark.rs→commonmark__comrak.rs`, `fuzz.rs→fuzz__comrak.rs`. (Non-colliding
   comrak files keep bare names.)
8. `git mv markdown-rs/upstream-tests/test_utils/*` →
   `oracles/upstream-tests/test_utils/` (audit-only; keep the one nested level
   since it is a Rust module group, or flatten with `test_utils_` prefix).
9. `git mv` both engines' `upstream-fuzz/*.rs` →
   `oracles/upstream-fuzz/` (collision: `commonmark.rs` appears in comrak fuzz
   AND comrak tests — already separated by the `upstream-fuzz/` vs
   `upstream-tests/` dirs, so no new collision here; markdown-rs fuzz names are
   unique).
10. `git mv markdown-rs/upstream-mdast-util-to-markdown-tests/*` →
    `oracles/mdast-to-markdown/`.
11. `git mv` `COPYING.comrak`, `LICENSE.markdown-rs`, both `NOTICE.md` →
    `oracles/LICENSES/` (rename the two `NOTICE.md` to
    `NOTICE.comrak.md` / `NOTICE.markdown-rs.md` to avoid collision).
12. Flatten the generated cases: `git mv` every
    `derived-cases/semantic-inputs/**/<stem>.cases` →
    `conformance/cases/<stem>.cases`. For the 3 collisions among consumed cases,
    suffix the comrak ones (`autolink__comrak.cases` etc.). Move
    `derived-cases/semantic-inputs/MANIFEST.md` → `conformance/cases/MANIFEST.md`.
13. Delete dead weight: `rm -r comrak/upstream-tests/fixtures/` (byte-identical
    duplicate read by no consumer). Decide legacy `derived-cases/{comrak,markdown-rs}`
    tree: either keep flattened into `conformance/cases/legacy/` or drop (see
    alternative 3 — out of scope; if kept, flatten one level).
14. `rmdir` the now-empty `corpus/comrak`, `corpus/markdown-rs`, `corpus/derived-cases`,
    `corpus/` itself.

---

## 5. Consumer changes (exact edits)

### tests/html_conformance/extractor/mod.rs (compile-time `include_str!`)

- `md_src!` (lines 87–97): change the stored prefix at L90
  `"markdown-rs/upstream-tests/"` → `"upstream-tests/"` (or whatever flat suffix
  the 652 anchor keys on — see below), and the `include_str!` prefix at L91–94
  `"../../fixtures/corpus/markdown-rs/upstream-tests/"` →
  `"../../fixtures/conformance/oracles/upstream-tests/"`.
- `cm_src!` (lines 99–109): L102 prefix `"comrak/upstream-tests/"` →
  `"upstream-tests/"`; L103–106 `include_str!` prefix
  `"../../fixtures/corpus/comrak/upstream-tests/"` →
  `"../../fixtures/conformance/oracles/upstream-tests/"`.
- The comrak source LIST (mod.rs:163–199): update the 3 collision entries to the
  renamed files: `cm_src!("autolink__comrak.rs")`, `cm_src!("commonmark__comrak.rs")`,
  `cm_src!("fuzz__comrak.rs")`. (These are compile-time `include_str!` sites — each
  must be edited or the bench will not compile.)
- The markdown-rs list (mod.rs:111–161) keeps bare names — no edits beyond the
  shared prefix already covered by the macro.
- 652 anchor (mod.rs:44): `path.ends_with("commonmark.rs")` is prefix-robust —
  unchanged. (The markdown-rs commonmark keeps its bare name, so this still
  matches; the comrak one is now `commonmark__comrak.rs` and does NOT match,
  which is correct — the anchor is markdown-rs-only.)

### tests/html_conformance.rs

- 652 anchor (corpus_counts_match, L54–55): `t.source_file.ends_with(
  "markdown-rs/upstream-tests/commonmark.rs")`. With prefix dropped to
  `"upstream-tests/"`, source_file becomes `upstream-tests/commonmark.rs`.
  Change the suffix to `.ends_with("upstream-tests/commonmark.rs")`. The comrak
  file is `commonmark__comrak.rs` so it will not collide with this match. (Edit
  required — this is the brittle full-suffix anchor flagged in the area maps.)
- `#[path]` module decls (L23–39): UNCHANGED. The bench module tree stays in
  `tests/html_conformance/`; only its corpus include paths moved.

### tests/html_conformance/runner.rs, types.rs, report.rs

- NO CHANGES. All key on `OracleTuple.engine`, never on the path. Verified:
  `runner.rs` plan switch on `t.engine`; `types.rs RenderConfig::for_engine`;
  `report.rs engine_name`.

### tests/fixtures.rs

- Stability inputs (L320): `format!("tests/fixtures/corpus/comrak/upstream/{fixture}.md")`
  → `format!("tests/fixtures/roundtrip/stability/{fixture}.md")`. The per-stem
  profile match (L312–318) is unchanged (it keys on the bare stem, not engine).
- The 31 `assert_fixture` literals (L16–296): prefix
  `tests/fixtures/corpus/{core,spec,extensions}/` →
  `tests/fixtures/roundtrip/{core,spec,extensions}/`.
- commonmark-examples (L358–375): `tests/fixtures/corpus/commonmark-examples/...`
  → `tests/fixtures/conformance/commonmark-examples/...` (official-inputs.cases
  count==652/role==None and official-stable-inputs.cases count==8 unchanged).
- Semantic corpus walk root (assert_semantic_input_corpus_stable, L329) and the
  legacy walk (L782–783): repoint `derived-cases/semantic-inputs` →
  `conformance/cases`; remove/replace the `semantic-inputs` os_str component
  filter (L787) since legacy/semantic are no longer disambiguated by a subdir —
  use the `role:` header (already parsed) to split executable vs accounted.
- MANIFEST (L867–895): repoint root to `conformance/cases/MANIFEST.md`. Logic
  (total + profile-name substring) unchanged.
- `assert_source_exists` (L944–954): change
  `Path::new("tests/fixtures/corpus").join(&metadata.origin).join(&metadata.source)`
  → `Path::new("tests/fixtures/conformance/oracles").join(&metadata.source)`,
  AND update the `.cases` `source:` headers so `source` is the new flat relative
  path (e.g. `upstream-tests/attention.cases`-style → the `.rs` oracle path
  `upstream-tests/attention.rs`, with comrak collisions `__comrak`). origin
  becomes a label, no longer joined.
- `assert_copied_upstream_sources_accounted` (L897–927): the two walk roots
  `corpus/markdown-rs` + `corpus/comrak` (L900, L905) → single
  `conformance/oracles`; the strip_prefix base (L915) →
  `tests/fixtures/conformance/oracles`; the relative strings now lack the engine
  segment.
- `ignored_upstream_sources` (L929–942): rewrite the 10 engine-prefixed strings
  to the new flat relative paths (drop `comrak/`/`markdown-rs/` segment; the fuzz
  + test_utils files keep their `upstream-fuzz/`/`test_utils/` segment).
- `assert_promoted_semantic_sources` (L850–865): the 5 literal
  `markdown-rs/upstream-tests/*.cases` → the flat
  `<stem>.cases` paths under `conformance/cases`.
- origin count-bucketing (L762–766): UNCHANGED logic — it reads `metadata.origin`
  from the header and splits `markdown_rs_cases` vs `comrak_cases`. Origin
  survives as the header label, so the >1400 / >500 thresholds (L344/348) still
  work. This proves engine remains a real, separately-counted dimension after
  the dir merge.

---

## 6. Engine-metadata plan (how engine survives, no collisions)

- **Bench (ROLE B oracles):** engine is the LIST/MACRO an entry belongs to.
  `md_src!` entries → `Engine::MarkdownRs` (stamped at markdown_rs.rs:153,550);
  `cm_src!` entries → `Engine::Comrak` (comrak.rs:211). This is already
  path-independent; merging the directory does not touch it. The only edits are
  the `include_str!` prefix (file location) and the 3 collision filenames.
  `RenderConfig::for_engine(t.engine)` continues to pick MathStyle/FootnoteStyle
  per file. No sidecar, no header needed on the `.rs` files.
- **Generated cases (ROLE B cases):** engine = the `origin: comrak|markdown-rs`
  header line, already parsed by `read_derived_metadata`. After the merge origin
  is a LABEL (count bucket + reporting), no longer a path segment. The `source:`
  header carries the full flat relative oracle path used by
  `assert_source_exists`.
- **Collisions:** only 3 basenames truly collide
  (`autolink`, `commonmark`, `fuzz`). Resolved by `__comrak` suffix on the comrak
  side for both the `.rs` oracle and its `.cases`. markdown-rs keeps bare names,
  so the 652 anchor (`commonmark.rs`) still matches the markdown-rs file and not
  the comrak one. The two `NOTICE.md` collide → `NOTICE.{engine}.md`.
- **Single source of truth per role:** bench engine = extractor list (1 place);
  cases engine = `origin:` header (1 place). The directory — the previously
  redundant 3rd/4th carrier — is removed. This is a net REDUCTION in the number
  of places engine is encoded (from 4 down to 2).

---

## 7. Risks

- **Compile-time `include_str!`**: the macro prefix edits (mod.rs:90–94, 102–106)
  and the 3 renamed comrak entries (mod.rs:166/175/the autolink line) are
  compile-gated — get any literal wrong and the bench target fails to build.
  Verify by `cargo test --no-run` first.
- **652 half-failure**: the anchor exists in TWO places with different
  strictness (mod.rs:44 prefix-robust; html_conformance.rs:55 full-suffix). Edit
  BOTH; otherwise one passes and one fails confusingly. The new full suffix must
  be `upstream-tests/commonmark.rs` and must NOT also match
  `commonmark__comrak.rs` (it does not, good).
- **`source:`/`origin` rebase**: if the `.cases` `source:` headers and
  `assert_source_exists` are not changed in lockstep, every legacy + semantic
  case fails the existence check. Highest-coordination edit.
- **Count thresholds**: legacy.files>=110, legacy.cases>10000,
  markdown_rs_cases>1400, comrak_cases>500, official==652, stable==8,
  MANIFEST==2401 — none should change if files are moved not deleted, but if the
  legacy tree is dropped (step 13 option) the first two asserts must be relaxed
  or removed.
- **`semantic-inputs` os_str filter (L787)**: removing the disambiguating subdir
  means the legacy/executable split must move onto the `role:` header; a missed
  spot silently re-includes files and corrupts counts.
- **Regression regroup**: merging 11 files into 4 risks duplicate test-fn names
  across merged files; namespace by `mod` or prefix fn names. Zero path coupling
  so no consumer breaks, but Cargo discovers `tests/regressions/*.rs` only if
  they are top-level — a `tests/regressions/` SUBDIR needs either a single
  `tests/regressions.rs` shim with `#[path]` mods or the 4 files kept at
  `tests/` top level. Recommend: keep the 4 files at `tests/` top level
  (`tests/parse_inline_regressions.rs` etc.) to preserve auto-discovery; the
  "regressions/" dir in the tree above is conceptual grouping, realize it as a
  filename prefix not a subdir.

---

## 8. Before / after tree

### Before (corpus, abbreviated)

```
tests/fixtures/corpus/
├── commonmark-examples/{official-inputs,official-stable-inputs}.cases + README/NOTICE
├── comrak/                          # ENGINE DIR
│   ├── COPYING.comrak NOTICE.md
│   ├── upstream/*.md (8, live stability)
│   ├── upstream-fuzz/*.rs (9)
│   └── upstream-tests/{*.rs (44), fixtures/*.md (8, DEAD dup)}
├── markdown-rs/                     # ENGINE DIR
│   ├── LICENSE.markdown-rs NOTICE.md
│   ├── upstream-fuzz/*.rs (2)
│   ├── upstream-mdast-util-to-markdown-tests/*.rs (21)
│   └── upstream-tests/{*.rs (48), test_utils/*.rs (3)}
├── core/ spec/ extensions/          # engine-free round-trip triples
└── derived-cases/
    ├── README.md
    ├── comrak/upstream-tests/*.cases        # LEGACY (audit-only)
    ├── markdown-rs/upstream-*/*.cases       # LEGACY
    └── semantic-inputs/{MANIFEST.md, comrak/..., markdown-rs/...}   # EXECUTABLE (5-6 levels deep)
```

### After

```
tests/fixtures/
├── roundtrip/{core,spec,extensions,stability}/        # ROLE A, engine-free, ≤3 levels
└── conformance/                                       # ROLE B
    ├── commonmark-examples/*.cases                    # engine-free
    ├── oracles/                                       # engine = metadata, NOT a dir
    │   ├── upstream-tests/*.rs (collisions: *__comrak.rs)
    │   ├── upstream-fuzz/*.rs
    │   ├── mdast-to-markdown/*.rs
    │   └── LICENSES/{COPYING.comrak,LICENSE.markdown-rs,NOTICE.*.md}
    └── cases/{MANIFEST.md, *.cases}                   # flat, origin: header carries engine
```

Engine directories: GONE. Max depth under `tests/fixtures/`: 3
(`conformance/oracles/upstream-tests/`). Engine encoded in 2 places (extractor
list + `origin:` header) instead of 4.

---

## 9. Verification checklist

- [ ] `cargo test --no-run` compiles (proves all `include_str!` prefixes + 3
      renamed comrak entries resolve).
- [ ] All test binaries green:
      `fixtures`, `html_conformance`, and the regression binaries
      (`parse_inline_regressions`, `parse_block_regressions`, `serialize_regressions`,
      `validate_regressions` after regroup — was 11, now 4; if kept as 11 the
      count is unchanged). Confirm the full set is green.
- [ ] `corpus_counts_match` passes: markdown-rs `commonmark.rs` still == 652
      tuples (both anchors: html_conformance.rs:55 + extractor/mod.rs:44).
- [ ] Bench headline unchanged at **96.65%** (re-run `html_conformance_report`,
      compare CONFORMANCE.md headline) — no oracle was dropped or mis-dialected.
- [ ] Per-engine RenderConfig intact: markdown-rs tuples still use
      `MathStyle::MarkdownRs`, comrak tuples `MathStyle::Comrak` (the renamed
      `*__comrak.rs` files still land in `cm_src!` → `Engine::Comrak`).
- [ ] `assert_source_exists` resolves for every `.cases` (origin rebased to
      label, source = flat oracle path).
- [ ] Count thresholds intact: markdown_rs_cases>1400, comrak_cases>500,
      official==652, stable==8, MANIFEST total==2401.
- [ ] No engine directory remains under `tests/fixtures/` (constraint 1).
- [ ] Dead `comrak/upstream-tests/fixtures/` removed; no consumer referenced it.
```
