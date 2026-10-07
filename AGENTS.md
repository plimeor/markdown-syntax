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

CI (`.github/workflows/ci.yml`) runs these in this order:

- Format: `cargo fmt --check`
- Build (default features): `cargo build` — the empty-feature / zero-dep gate
- Compile the default-feature tests: `cargo test --no-run`
- Test: `cargo test --profile ci --features html` — every test once:
  parse/serialize/validate/fixtures/roundtrip, the README doc-test, the HTML
  renderer tests, and the commonmark.js reference cases for nested containers.
  `[profile.ci]` in `Cargo.toml` is release with debug assertions and overflow
  checks on.
- Docs: `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`

Observed on 4 cores from an empty `target/`: about 1 min for the sequence,
of which ~20 s compiles the ci profile and ~22 s runs the tests;
`tests/pathological_inputs.rs` takes ~13 s and `tests/linear_growth.rs` ~7 s
of that. Plain `cargo test` (debug) takes ~2.5 min, nearly all in those two
files.

Inner loop: `cargo test --profile ci --features html --test <name>`; a fast
file also runs quickly as plain `cargo test --test <name>`.

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
- Before a release, run `scripts/pre-release-check.sh`: it builds for
  `wasm32-unknown-unknown` and with Rust 1.82, with default features and with
  `html`, the invariants CI does not check.

## Conformance

- `tests/html_conformance/` is a measurement bench (AST→HTML vs vendored
  CommonMark/GFM oracles); its pass rate is **not** a CI gate. To observe
  current numbers, run
  `cargo test --profile ci --features html --test html_conformance -- --nocapture`.
  It fails only when an entry in `tests/html_conformance/deviations.rs` names
  no case, names a case twice, or names a case that now passes, or when
  `commonmark/commonmark.cases` no longer holds its 643 cases.
- An oracle case that differs by decision of this syntax (a construct the
  syntax drops, one the oracle lacks or turns off, or a rule shared with
  another reference renderer) leaves the bench. Its input moves to
  `tests/fixtures/syntax_decisions/`, with HTML verified against a reference
  renderer (cmark-gfm, commonmark.js, micromark) or the decision, and
  `tests/syntax_decisions.rs` checks it exactly.
- Parse and serialize tests are split by flow, each with one oracle:
  Markdown → AST (`.ast` goldens, `tests/fixtures.rs`); AST → Markdown
  (hand-built trees, exact output, `tests/serialize_regressions.rs`);
  Markdown → AST → Markdown (`.canonical.md` goldens and `CANONICAL_INPUTS`,
  `tests/fixtures.rs`); source read-back (one corpus, one `NOT_READING_BACK`
  list, `tests/fixtures.rs`); generated read-back (seeded, with its own list,
  `tests/serialize_roundtrip_fuzz.rs`). Goldens stay together by topic under
  `tests/fixtures/roundtrip/`; each test finds the ones it reads.
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
