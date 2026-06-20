# markdown-syntax Docs Index

This directory keeps durable decision rationale only. It does not store current
test results, conformance numbers, completed task graphs, or active execution
cursors. Derive current behavior from `README.md`, `Cargo.toml`, source files,
fixtures, and runnable commands.

## Decisions

| Path | Status | Purpose |
| --- | --- | --- |
| `decisions/001-default-crate-boundary.md` | active | Records why the crate keeps an empty default feature set, zero runtime dependencies, `no_std + alloc`, MSRV 1.82, and `publish = false`. |
| `decisions/002-html-renderer-boundary.md` | active | Records why AST-to-HTML rendering ships behind the opt-in `html` feature instead of default-on or as a sibling crate. |
| `decisions/003-test-corpus-boundaries.md` | active | Records why fixture corpora are role-separated and why only runnable executable cases stay in the tree. |
| `decisions/004-correctness-workflow.md` | active | Records why parser correctness work uses paired parser/serializer fixes, hand-verified goldens, and observed conformance runs. |
