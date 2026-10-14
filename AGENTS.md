# Project: markdown-syntax

Read `docs/overview.md` before starting work.

## Identity

- A `no_std + alloc` Rust crate at the repo root. Parses one fixed Markdown
  syntax → AST and writes AST → Markdown: a parsed document keeps its source,
  each unchanged node is written as its source, an edited one with its own
  syntax copied from the source around what changed, a leaf built from text
  (`with_source`) as that text, and the rest as canonical Markdown (decision
  0017). Raw HTML is represented only as Markdown syntax nodes;
  HTML rendering and sanitization stay out of the default build.
- The repository is a Cargo workspace whose default member is the root
  crate; `wasm/` holds `markdown-syntax-wasm`, the unpublished WebAssembly
  binding (zero-dependency C ABI) and its JS wrapper `wasm/js/` (no npm
  dependency). The binding may use `std`; the core crate's invariants below
  do not extend to it.
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
- The `wasm` job: `cargo test -p markdown-syntax-wasm` (the encoding's unit
  tests, on the host), `cargo build -p markdown-syntax-wasm --target
  wasm32-unknown-unknown --profile wasm`, then
  `node --test 'wasm/js/test/*.test.mjs'` (Node 22).

Observed on 4 cores: about 1.5 min for the ci-profile tests, plus ~20 s to
compile the ci profile from an empty `target/`; `tests/linear_growth.rs`
takes ~40 s of that, `tests/pathological_inputs.rs` ~15 s, and
`tests/fixtures.rs` ~8 s. Plain `cargo test` (debug) takes over 5 min,
nearly all in `linear_growth.rs` (~4.5 min) and `pathological_inputs.rs`
(~1 min).

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
- No bless in CI (decision 0014): a golden a correct fix moves is regenerated
  in the same commit, by hand or by a local command, and every changed golden
  is read in review for correct structure, with the reason it moved — never
  edit a test to pass a wrong parse.
- Tests that check canonical output write the parsed document
  `without_source()`; with its source, an unedited parse writes the source.
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
- `docs/decisions/`: cross-feature technical decisions. Records directly in it are live: `Status: Accepted` is in force; an approach recorded as `Rejected`, or turned down in a live record's Considered options, is not to be proposed again unless the user reopens it. `docs/decisions/superseded/` is history; read it only to learn why a decision changed. A record on the default branch is changed by a new record that supersedes it; a record not yet there is edited in place.
