---
date: 2026-06-20
status: active
---

# Default Crate Boundary

## Decision

`markdown-syntax` keeps the default build as the syntax-core surface:

- empty default feature set;
- zero runtime dependencies;
- `#![no_std]` with `alloc`;
- MSRV 1.82;
- `publish = false` until package-boundary and fixture-provenance review are
  explicitly done.

The public contract for this surface lives in `README.md`, `Cargo.toml`, and
the crate root, not in planning notes.

## Rationale

The crate's core job is Markdown source to AST and AST to canonical Markdown.
Keeping that surface dependency-free and feature-minimal makes default builds
predictable for embedded, wasm, and library consumers that only need syntax
structure.

The fixture corpus is intentionally present in package snapshots while
publishing is disabled, because it is useful for local test and audit coverage.
Publishing changes the distribution boundary and needs a separate license and
provenance review of bundled fixtures.

## Rejected Alternatives

- Default-on optional capabilities: rejected because they silently expand the
  default build surface for consumers that only need parse/serialize.
- Runtime dependencies in the syntax core: rejected because they weaken the
  zero-dependency and `no_std + alloc` boundary.
- Publishing with the current fixture corpus by default: rejected until the
  package boundary and fixture licenses are reviewed as a release task.

## Non-Goals

- This decision does not prohibit opt-in features.
- This decision does not define a publishing plan.
- This decision does not claim the syntax implementation is full CommonMark.
