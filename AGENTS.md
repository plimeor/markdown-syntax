# Project: markdown-syntax

Read `docs/overview.md` before starting work.

## Identity

- A single `no_std + alloc` Rust crate at the repo root. Parses one fixed
  Markdown syntax → AST and serializes AST → canonical Markdown. Raw HTML is
  represented only as Markdown syntax nodes; HTML rendering and sanitization
  stay out of the default build.
- Hard invariants (do not break without an explicit decision): empty default
  features (`[features] default = []`), zero runtime dependencies, `#![no_std]`
  (+ `extern crate alloc`), MSRV 1.82. The default build surface stays
  byte-stable. (The crate is published to crates.io as of 0.1.0; the
  zero-dependency/`no_std`/MSRV invariants now also protect downstream users.)

## Commands

- Format: `cargo fmt --check`
- Build (default): `cargo build` — the empty-feature / zero-dep gate
- Test: `cargo test` — parse/serialize/validate/fixtures/roundtrip + the README
  doc-test; `cargo test --features html` also runs the HTML renderer tests and
  the commonmark.js reference cases for nested containers
- Docs: `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`
- wasm check: `cargo build --target wasm32-unknown-unknown`
  (`rustup target add wasm32-unknown-unknown` first)

## Release

<!-- 2026-07-05: release-plz is configured and non-Conventional commits were observed landing in CHANGELOG.md under `Other`. -->
- Commits, PR titles, and squash titles that can land on `main` must use
  Conventional Commits: `<type>[optional scope]: <description>`.
- Use a specific release-plz-friendly type: `feat`, `fix`, `perf`,
  `refactor`, `docs`, `test`, `ci`, `build`, `chore`, `style`, or `revert`;
  prefer `feat` / `fix` / `perf` for user-visible released behavior so the
  generated `CHANGELOG.md` does not collapse into `Other`.
- Mark SemVer-breaking changes with `!` after the type/scope or a
  `BREAKING CHANGE:` footer.
- Enable the local commit-message hook with `cog install-hook commit-msg`;
  `cog.toml` owns the executable check for this rule.

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

## Docs conventions
- `docs/overview.md`: what this project is, its principles, current focus, and non-goals. Read it before starting work. Non-goals are not to be implemented or proposed.
- `docs/specs/`: the behavior contract of each shipped feature. Update the spec in the same change as the code. When a spec and the code disagree, the code is right and the spec gets fixed.
- `docs/plans/`: intent for features being built or queued. Everything under it, including the spec change files in `docs/plans/<slug>/specs/`, is a proposal and not a description of current behavior. When implementing a plan, read the Tasks in its `plan.md` to find what is open, and tick each task as its work lands.
- `docs/archive/`: shipped plans kept as history. Nothing in it describes current behavior or is work to do; its spec change files are already merged into `docs/specs/`. Read it only to learn why something was built the way it was. Its contents are not edited.
- `docs/decisions/`: only records with `Status: Accepted` are in force. A changed decision is a new record that supersedes the old one. An approach recorded as `Rejected` is not to be proposed again.
