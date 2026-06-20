---
date: 2026-06-20
status: completed
---

# Public API Ergonomics — Implementation Plan

Source decision: `decisions/005-public-api-ergonomics.md` (active). This plan
turns that decision into an ordered, verifiable implementation. It does not
re-argue the design; it sequences it, names the regression surface, and isolates
the one genuinely risky part (the never-before-exercised maximal parser config).

## Clarification status

Clear enough to execute, with two labeled assumptions and one open `Test gap
decision` (below):

- **Assumption A1** — the parser already de-conflicts the maximal delimiter
  collisions correctly *except where Slice 0 proves otherwise*; Slice 0 is the
  empirical gate that converts this assumption into fact.
- **Assumption A2** — `SyntaxOptions::default()` / `Constructs::default()` flipping
  from CommonMark to maximal does not silently break test/fixture code, because
  fixtures pin dialects via `profile_options()` and regression tests pass explicit
  options. Verified-by-grep so far; Slice 3 re-audits `::default()` reliance.

## Background / problem

The config and surrounding public surface accreted redundant and inconsistent
shapes: a `SyntaxProfile` axis that duplicates `Constructs` (and carries a latent
bug), a `ResolvedSyntaxOptions` twin read by nothing, a verbose
`*_with_options` free-function family, two diagnostic types for one concept, a
leaked render context, ~80 glob-exported root names, and an AST with no generic
accessors and verbose hand-construction. The decision reshapes all of this around
one rule — "configure → `parse`; then ask the `Document`" — and makes the default
parse recognize what people actually write (maximal non-MDX).

## Objective

The public API matches `decisions/005-public-api-ergonomics.md`: one infallible
`parse()` (maximal non-MDX) plus preset/builder `.parse()`, output verbs on
`Document`, a single `Diagnostic`, uniform AST accessors with a minimal build
layer, and a curated export surface — with the default build still `no_std +
alloc`, zero-dependency, empty-feature, MSRV 1.82, and the only intentional
behavior change being the default parse dialect.

## Scope

- `src/options.rs`, `src/parse.rs`, `src/serialize.rs`, `src/html/mod.rs`,
  `src/validate.rs`, `src/diagnostic.rs`, `src/ast.rs`, `src/span.rs` (read-only
  unless accessors touch it), `src/lib.rs`.
- `tests/**` call-site migration; `tests/support/fixtures.rs` (`profile_options`,
  `extra_constructs`); the `README.md` doctest and API narrative.
- Authorized boundary changes (per decision 005, owner-approved): the public
  config/output/diagnostic/AST/export API, and the default parse *output*.

## Non-goals

- Changing the `Constructs` flag set, the `validate()` conflict guards,
  `parse_strict`/`ParseStrictError`, the `ParseOptions` boundary, AST node field
  shapes, serialize/html behavior or safe-by-default policy (decisions/002), or
  conflating `directive_*` with `mdx_*`.
- MDX-in-default, output-verbs-on-options, a per-node builder module, or
  encapsulating fields (`#[non_exhaustive]`) — all rejected/deferred in 005.

## Required context (read first)

- `decisions/005-public-api-ergonomics.md` (the contract), `decisions/001` (build
  invariants), `decisions/002` (html boundary), `decisions/004` (no-bless golden
  policy).
- `src/options.rs` (whole), `src/parse.rs:83-140` (entry points), `:6004-6020` &
  `:7240-7260` (profile predicates), the `&ResolvedSyntaxOptions` signatures.
- `tests/support/fixtures.rs` (`profile_options`, `extra_constructs`),
  `tests/serialize_regressions.rs:1108` (local `parse` helper), `README.md`.

## Planning iteration (Design Gate)

Most shapes are fixed by decision 005; the plan-level design work is ownership and
irreducibility:

- **Maximal-config correctness — the one material fork.** When a delimiter is
  claimed by two enabled constructs, the behavior is OWNED by the parser's
  de-confliction logic, not the config. Option (a): fix/confirm parser
  de-confliction so the decided picks hold (`~~`=strike / `~`=subscript,
  `^[`=footnote / `^`=superscript, `:word:`=shortcode / `:name[`=directive).
  Option (b): if a pair is genuinely ambiguous, back off the maximal set for that
  construct rather than ship a wrong parse. Chosen: (a) by default, (b) only where
  Slice 0 proves an irreducible ambiguity — recorded there, not assumed. APOSD:
  pushing collision logic into the config (every caller re-reasons about `~`) is
  higher cognitive load than owning it once in the parser.
- **`ResolvedSyntaxOptions` removal.** Option (a) thread `&SyntaxOptions` (chosen,
  per 005); option (b) a private validated newtype (rejected — reintroduces the
  twin). Irreducible: one cut across ~50 signatures.
- **Diagnostic unification, output verbs, accessors, exports** — one viable shape
  each, mechanical given 005; owners are `diagnostic.rs`, `Document`, the
  `Block`/`Inline` enums, and `lib.rs` respectively. Each type/representation
  change (`Diagnostic.span → Option`, delete free fns, delete twin) is an
  indivisible cut (Work Sequence flags them).

## Proposed approach

Land the changes as a sequence of irreducible cuts, each leaving the tree green,
ordered risk-first for human review (Slice 0 characterization before any edit).
`code-tasking` will discard this order and re-sort leaf-first by compile
dependency. The default-dialect change is treated as a *parser-correctness*
problem (Slice 0) far more than a golden-regeneration problem, because the corpus
pins dialects and the bare default has near-zero golden coverage.

## Work sequence

Each slice is one independently-green cut unless noted.

- **Slice 0 — Characterize the maximal config (discovery, no product edit).**
  Build the maximal `Constructs` (all non-MDX true; `single_tilde_strikethrough`
  off; wikilink title-after-pipe) behind a test-only constructor. Run it over the
  roundtrip corpus and a crafted collision stress set (`~x~`, `~~x~~`, `x^2^`,
  `note^[fn]`, `:tada:`, `:note[c]{a=b}`, `price $5 to $10`, `==hl==`, `++ins++`,
  `||spoiler||`, `[[a|b]]`, `:::note\n…\n:::`, `H~2~O`). Forward evidence: a
  written list of any pair that mis-parses, classified as parser-fix or
  back-off-the-pick. Regression evidence: `cargo test` green baseline captured
  first, so later slices distinguish new breakage. **Checkpoint** before Slice 3.
- **Slice 1 — Delete `SyntaxProfile`; re-key predicates (indivisible).** Remove the
  enum + `SyntaxOptions::profile`; rewrite `parse.rs:6012` and `:7256` to read
  `options.constructs.gfm_autolink_literal`; drop the `profile` param from
  `parse_literal_autolink` (7077) and its forwards (3977/4284/7110/7133/7163).
  Regression: full `cargo test` + `cargo test --features html --test
  html_conformance` numbers unchanged for commonmark/gfm.
- **Slice 2 — `resolve()` → `validate()`; delete `ResolvedSyntaxOptions`
  (indivisible).** `validate(&self) -> Result<(), SyntaxConfigError>` keeps both
  guards; delete the twin; rethread ~50 private `&ResolvedSyntaxOptions` →
  `&SyntaxOptions`; `parse_with_options` (soon a method) calls `validate()?`-or-
  diagnostic. Regression: full suite.
- **Slice 3 — Parse entry reshape (indivisible core + additive builder).** Add the
  maximal `Constructs` (and `Constructs::default()` = maximal); `SyntaxOptions::
  default()` = maximal, presets `commonmark/gfm/mdx`; `SyntaxOptions::parse(&self,
  &str) -> ParseOutput` (infallible; config conflict → `Diagnostic`),
  `parse_strict(&self, &str)`; free `parse(input)` = `default().parse(input)`.
  Delete `parse_with_options`/`parse_strict_with_options` free fns and
  `SyntaxOptions::custom`. Migrate all `tests/**` call sites to `o.parse(x)` /
  `o.parse_strict(x)` and `extra_constructs` off `custom`. Re-audit `::default()`
  reliance (Assumption A2). *Then* (additive sub-slice 3b) add the `Construct`
  enum + `enable`/`disable`, folding footnote/strikethrough dependencies and the
  wikilink tri-state. Apply Slice 0's resolution here. Regression: full suite +
  conformance bench + README doctest; golden regen only where Slice 0 / the
  `::default()` audit prove a real move (hand-regenerate per decisions/004).
- **Slice 4 — Unify `Diagnostic` (indivisible).** `Diagnostic.span → Option<Span>`;
  add `DiagnosticCode::InvalidDocument`; delete `ValidationDiagnostic`;
  `validate_document → Vec<Diagnostic>`; `SerializeError`/`HtmlError` carry
  `Vec<Diagnostic>`; wrap existing `Diagnostic::new(..)` spans in `Some(..)`.
  Regression: `validate_regressions`, `serialize_regressions`, html tests.
- **Slice 5 — Output verbs on `Document` (indivisible).** Add
  `Document::{to_markdown, to_markdown_with, validate}` and (under `html`)
  `{to_html, to_html_with}`; delete the five free fns; migrate internal callers,
  tests, and the README doctest to `doc.to_markdown()` etc. Regression:
  serialize/html/validate suites + doctest.
- **Slice 6 — AST accessors + minimal build layer (additive).** `impl Block`/`impl
  Inline` `meta()`/`span()`; `Inline::children()`; `From<&str>`/`From<String>` for
  `Text`; `From<struct>` for `Block`/`Inline` (all variants); `new(..)` on
  `Text`/`Paragraph`/`Heading`/`Link`/`Code`/`List`. Forward: unit tests for each
  accessor/constructor (incl. `Inline::children()` over `Image.alt`). Regression:
  full suite stays green (additive).
- **Slice 7 — Export hygiene (mechanical).** `pub(crate)` `html::Ctx` and any
  sibling `pub` render internals; replace `pub use {ast,diagnostic,options}::*`
  with explicit lists; add a `prelude` module; drop the `ParseOutput<T=Document>`
  generic. Regression: `cargo build`, `cargo build --target
  wasm32-unknown-unknown`, `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps`, full
  test suite (fix imports), and confirm the explicit re-exports cover everything
  tests/README use.
- **Slice 8 — README + docs.** Rewrite the `README.md` API narrative and doctest to
  the new surface (maximal `parse()`, presets, `Document` methods, `prelude`).
  Regression: `cargo test` (doctest).

## Acceptance, regression evidence, and verification

- `cargo test` and `cargo test --features html` pass; the README doctest passes
  with the new surface.
- `cargo build` (default), `cargo build --target wasm32-unknown-unknown`, and
  `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps` succeed.
- No `SyntaxProfile`, `ResolvedSyntaxOptions`, `SyntaxOptions::custom`,
  `ValidationDiagnostic`, `parse_with_options`, `parse_strict_with_options`,
  `to_markdown_with_options`, `to_html_with_options`, or `validate_document`
  remain in the public surface; `markdown_syntax::html::Ctx` is no longer
  reachable (compile-checked by a `pub(crate)` and the curated re-exports).
- `cargo test --features html --test html_conformance -- --nocapture` shows
  commonmark/gfm numbers unchanged through Slices 1–2 and 4–8 (the predicate
  re-key and the mechanical refactors are behavior-preserving for the presets).
- The only intentional behavior change is the default dialect: any golden that
  legitimately moves is hand-regenerated and verified correct in the same commit
  (decisions/004), never edited to pass.
- Default-build invariants hold: empty features, zero deps, `no_std + alloc`, MSRV
  1.82 (a `cargo +1.82 build` or equivalent check).

## Risks and rabbit holes

- **Maximal-config mis-parse (highest).** All non-MDX delimiters live at once for
  the first time. Containment: Slice 0 characterizes before any edit; unresolved
  ambiguity backs off the pick rather than shipping a wrong parse (pause if a
  back-off would contradict 005's stated picks).
- **`::default()` semantic flip.** Flipping `Constructs`/`SyntaxOptions`
  `Default` to maximal could change code relying on the old CommonMark default.
  Containment: the Slice 3 `::default()` audit; A2.
- **`ResolvedSyntaxOptions` rethread churn.** ~50 signatures; a missed one fails to
  compile (safe, not silent). No runtime risk.
- **Diagnostic span optionality.** Making `Diagnostic.span` optional ripples to
  every constructor and matcher; the compiler enforces completeness.
- **Export curation gaps.** An under-curated explicit re-export list breaks
  downstream imports; Slice 7 verifies against tests + README.

## Checkpoints

- After **Slice 0**: report the collision-characterization result (which pairs are
  clean, which need a parser fix vs a pick back-off) before starting Slice 3.
- After **Slice 3**: report conformance-bench numbers and the `::default()` audit
  result before proceeding.

## Stop condition

All Acceptance/verification checks pass, the public surface matches decision 005,
the README contract is updated, and `decisions/005` can be cited as implemented.
Stop there; do not also undertake encapsulation (`#[non_exhaustive]`), a per-node
builder module, or any 005 non-goal.

## Pause conditions

- Slice 0 finds a delimiter collision with no correct de-confliction, such that
  honoring a 005 pick requires a parser change beyond de-confliction, or backing
  off would drop a construct from the maximal default — pause for an owner call.
- Any required golden move looks like a *wrong* parse rather than the expected
  maximal-dialect difference — pause (decisions/004 forbids editing-to-pass).

## Test gap decision (needs your input)

The maximal default is new behavior with **no existing coverage** (the config
never existed). The rest of the surface is covered by the regression suites,
fixtures, conformance bench, and doctest. How should the maximal-dialect behavior
be locked in? This is a `no_std` library, not a web project, so targeted
behavior cases are the natural fit, but confirm:

- **(recommended) Targeted behavior fixtures** — add the Slice 0 collision stress
  cases as roundtrip/parse fixtures pinned to a new `maximal` profile in
  `profile_options()`, so the decided picks are regression-locked.
- **Broaden the conformance pass** — also run the maximal config across the
  existing corpus as a second oracle (heavier; mostly redundant with pinned
  fixtures).
