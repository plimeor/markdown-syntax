# Project: markdown-syntax

## Identity

- A single `no_std + alloc` Rust crate at the repo root. Parses Markdown → AST
  and serializes AST → canonical Markdown. Raw HTML and MDX are represented
  only as Markdown syntax nodes; HTML rendering and sanitization stay out of
  the default build.
- Hard invariants (do not break without an explicit decision): empty default
  features (`[features] default = []`), zero runtime dependencies, `#![no_std]`
  (+ `extern crate alloc`), MSRV 1.82. The default build surface stays
  byte-stable. (The crate is published to crates.io as of 0.1.0; the
  zero-dependency/`no_std`/MSRV invariants now also protect downstream users.)

## Commands

- Format: `cargo fmt --check`
- Build (default): `cargo build` — the empty-feature / zero-dep gate
- Test: `cargo test` — parse/serialize/validate/fixtures/roundtrip + the README
  doc-test
- Docs: `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`
- wasm check: `cargo build --target wasm32-unknown-unknown`
  (`rustup target add wasm32-unknown-unknown` first)

## Conformance

- `tests/html_conformance/` is a measurement bench (AST→HTML vs vendored
  CommonMark/GFM oracles), **not** a CI gate. To observe current numbers, run
  `cargo test --features html --test html_conformance -- --nocapture`.
- No bless flag: any `.ast` / `.canonical.md` golden a fix legitimately moves
  must be hand-regenerated in the same commit and verified to reflect correct
  structure — never edit a test to pass a wrong parse.
- Correctness work uses paired parser+serializer fixes when the serializer can
  mask a stably-wrong parse.

## HTML renderer

- No HTML renderer in the default build. The shipped renderer lives behind the
  non-default `html` cargo feature (safe-by-default) and must not change the
  default parser/AST/serializer surface.

## Directives

- Never conflate `:name` / `::name` / `:::name` directives with MDX.

## Docs

- `README.md` owns the public API, syntax scope, and stable-behavior contract.
- `docs/decisions/` owns durable design rationale and rejected alternatives.
  Read `docs/index.md` when you need to understand why a boundary exists.
- Do not keep hand-maintained status ledgers for test results, conformance
  numbers, old plans, or completed task graphs. Derive current state from
  `README.md`, source files, fixtures, and runnable commands.
