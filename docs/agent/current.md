# Cursor — markdown-syntax

_Active execution pointer. Rewrite in place as work moves. Not a store of full
content — it points at the docs that hold it._

## Current goal

The crate was migrated into its own dedicated repo (from the `labs` monorepo,
2026-06-20). No task is in flight. One open workstream remains:

1. **Ship an opt-in HTML renderer** — promote the test-only renderer into
   `src/html/` behind a non-default `html` feature, per the active plan
   `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` (not yet started).

Completed workstream: parse↔serialize correctness via
`docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md`; current
CommonMark/GFM AST→HTML conformance is 2260/2260 in
`tests/html_conformance/CONFORMANCE.md`.

## Scope

- In: the renderer workstream above; keep the default build byte-stable
  (`no_std`, zero runtime deps, empty default features).
- Out: publishing the crate; pluggable HTML sanitization; reopening the completed
  test-tree reorganization unless explicitly requested
  (`docs/plans/2026-06-19-markdown-syntax-test-reorg.md`).

## Next step

None pending. Pick up the renderer workstream when work resumes.

## Verification state

Current verification: `cargo fmt --check`, `cargo build`, `cargo test`,
`RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, and
`cargo build --target wasm32-unknown-unknown` all green. Current conformance
numbers: 2260/2260; see `tests/html_conformance/CONFORMANCE.md`.
Observed test layout: `tests/fixtures/roundtrip/`,
`tests/fixtures/conformance/{commonmark,gfm}/`, and the four regression test
files under `tests/`.

## Open docs

- Plan (renderer): `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` — active

## Stop condition

n/a — no active task.
