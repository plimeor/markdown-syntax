# Project: markdown-syntax

## Identity

- A single `no_std + alloc` Rust crate at the repo root. Parses Markdown → AST
  and serializes AST → canonical Markdown. Raw HTML and MDX are represented
  only as Markdown syntax nodes; HTML rendering and sanitization stay out of
  the default build.
- Hard invariants (do not break without an explicit decision): empty default
  features (`[features] default = []`), zero runtime dependencies, `#![no_std]`
  (+ `extern crate alloc`), MSRV 1.82, `publish = false`. The default build
  surface stays byte-stable.

## Commands

- Format: `cargo fmt --check`
- Build (default): `cargo build` — the empty-feature / zero-dep gate
- Test: `cargo test` — parse/serialize/validate/fixtures/roundtrip + the README
  doc-test + the AST→HTML conformance bench
- Docs: `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`
- wasm check: `cargo build --target wasm32-unknown-unknown`
  (`rustup target add wasm32-unknown-unknown` first)

## Conformance

- `tests/html_conformance/` is a measurement bench (AST→HTML vs vendored
  CommonMark/GFM oracles), **not** a CI gate. Current conformance numbers live in
  `tests/html_conformance/CONFORMANCE.md`.
- No bless flag: any `.ast` / `.canonical.md` golden a fix legitimately moves
  must be hand-regenerated in the same commit and verified to reflect correct
  structure — never edit a test to pass a wrong parse.
- Correctness work uses paired parser+serializer fixes (the serializer can mask
  a stably-wrong parse). See `docs/tasking/2026-06-19-markdown-syntax-conformance-fix.md`.

## HTML renderer

- No HTML renderer in the default build. Any renderer ships behind a non-default
  `html` cargo feature (safe-by-default) and leaves the parser/AST/serializer
  untouched. See `docs/plans/2026-06-20-markdown-syntax-html-renderer.md`.

## Directives

- Never conflate `:name` / `::name` / `:::name` directives with MDX.

## Docs

- `README.md` owns the public API, syntax scope, and stable-behavior contract.
- `docs/` is a typed historical substrate (`plans/`, `tasking/`, `archive/`,
  and future `requirements/` & `decisions/`), routed by `docs/index.md`, with
  the active cursor at `docs/agent/current.md`. These are historical records and
  durable authority, **not** current interface contracts. Follow the
  agentic-document-workflow when authoring them.
- Front matter (`date`, `status`) is the status surface — **do not restate the
  status in the body prose.**
- Orient from `docs/index.md` and `docs/agent/current.md` first.
