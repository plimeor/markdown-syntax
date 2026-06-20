# Cursor — markdown-syntax

_Active execution pointer. Rewrite in place as work moves. Not a store of full
content — it points at the docs that hold it._

## Current goal

No active workstream is in flight.

Completed workstreams:

1. **Parse↔serialize correctness** — completed via
   `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md`.
2. **Opt-in HTML renderer** — completed via
   `docs/plans/2026-06-20-markdown-syntax-html-renderer.md`.

Current CommonMark/GFM AST→HTML conformance is 2265/2265 in
`tests/html_conformance/CONFORMANCE.md`.

## Scope

- In: maintenance work that keeps the default build byte-stable
  (`no_std`, zero runtime deps, empty default features).
- Out: publishing the crate; pluggable HTML sanitization; reopening the completed
  test-tree reorganization unless explicitly requested
  (`docs/plans/2026-06-19-markdown-syntax-test-reorg.md`).

## Next step

None pending.

## Verification state

Latest observed after rebasing onto `main`:

- `cargo fmt --check`
- `cargo test`
- `cargo test --features html --test html_regressions` (7 tests)
- `cargo test --features html --test html_conformance -- --nocapture`
  - headline: 2265 / 2265 = 100.00%
  - CommonMark spec: 652 / 652 = 100.00%
  - failures: 0, parse errors: 0

No `MEMORY.md` file exists in this repo to update.

## Open docs

- Plan (renderer): `docs/plans/2026-06-20-markdown-syntax-html-renderer.md` — completed
- Tasking (correctness): `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md` — completed

## Stop condition

n/a — no active task.
