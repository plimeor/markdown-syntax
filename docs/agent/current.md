# Cursor — markdown-syntax

_Active execution pointer. Rewrite in place as work moves. Not a store of full
content — it points at the docs that hold it._

## Current goal

The crate was just migrated into its own dedicated repo (from the `labs`
monorepo, 2026-06-20). No task is in flight. Two open workstreams exist:

1. **Parse↔serialize correctness** — advance toward CommonMark/GFM conformance
   via the task graph `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md`
   (T001–T025; leaf-first; paired parser+serializer fixes + regenerated
   bench-verified goldens). Substrate: `docs/archive/2026-06-19-markdown-syntax-conformance-investigation.md`.
2. **Ship an opt-in HTML renderer** — promote the test-only renderer into
   `src/html/` behind a non-default `html` feature, per the active plan
   `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` (not yet started).

## Scope

- In: the two workstreams above; both keep the default build byte-stable
  (`no_std`, zero runtime deps, empty default features).
- Out: publishing the crate; pluggable HTML sanitization; the test-tree
  reorganization (`docs/plans/2026-06-19-markdown-syntax-test-reorg.md`, draft).

## Next step

None pending. Pick up workstream 1 or 2 above when work resumes.

## Verification state

Post-migration: `cargo fmt --check`, `cargo build`, and `cargo test` (146 tests
+ README doc-test) all green; `cargo tree` shows zero runtime deps. Current
conformance numbers: see `tests/html_conformance/CONFORMANCE.md`.

## Open docs

- Plan (renderer): `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` — active
- Tasking (correctness): `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md` — active

## Stop condition

n/a — no active task. Sequencing note for whoever resumes: the renderer
promotion is a byte-for-byte port pinned to the *current* bench headline, while
the correctness tasks move parser output and regenerate goldens — land the
renderer plan first (frozen oracle) or coordinate golden regeneration if both
run together.
