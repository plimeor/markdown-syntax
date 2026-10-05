# 0001: Default crate boundary

Status: Accepted
Date: 2026-06-20

## Context

The crate's core job is Markdown source to AST and AST to canonical Markdown.
Keeping that surface dependency-free and feature-minimal makes default builds
predictable for embedded, wasm, and library consumers that only need syntax
structure. The crate is published to crates.io, so the build surface is also a
promise to downstream users.

The test fixture corpus vendors third-party CommonMark, GFM, comrak, and
markdown-rs cases. It is useful for local test and audit coverage, but
redistributing it would need a license and provenance review of every bundled
case.

## Decision

`markdown-syntax` keeps the default build as the syntax-core surface:

- empty default feature set;
- zero runtime dependencies;
- `#![no_std]` with `alloc`;
- MSRV 1.82.

The published package ships only the library source, `Cargo.toml`, the README,
the changelog, and the licenses (the `include` list in `Cargo.toml`). The
vendored fixture corpus under `tests/` stays in the repository as dev-only
material and is not redistributed.

The public contract for this surface lives in `docs/specs/public-api.md`,
`Cargo.toml`, and the crate root, not in planning notes.

## Considered options

- Default-on optional capabilities: rejected because they silently expand the
  default build surface for consumers that only need parse/serialize.
- Runtime dependencies in the syntax core: rejected because they weaken the
  zero-dependency and `no_std + alloc` boundary.
- Publishing the package with the fixture corpus: rejected because it would
  redistribute third-party cases without a per-case license and provenance
  review.

## Consequences

- Opt-in features remain allowed.
- Tests cannot run from the published package; they run from the repository.
- This decision does not claim the syntax implementation is full CommonMark.
