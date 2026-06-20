---
date: 2026-06-20
status: active
---

# Public API Ergonomics

A single ergonomics-driven reshaping of the public surface, spanning the
parse-side config, the output verbs, the diagnostic taxonomy, the AST node
types, and crate exports. The unifying goal: the common path costs one concept,
the configured path reads as intent, and no redundant or leaked surface greets a
caller. The final API contract lives in `README.md`; the shapes below are the
direction, not the authoritative signatures.

## Decision

### A. Parse-side config

1. **`parse(input) -> ParseOutput` is the maximal non-MDX dialect**, infallible:
   raw HTML + full GFM (tables, task lists, strikethrough, autolinks, alerts) +
   footnotes + inline/block math + frontmatter + the fancy inline marks
   (insert, highlight, subscript, superscript, spoiler) + description lists +
   wikilinks + directives + shortcodes. The maximal set is every non-MDX construct
   **that does not reinterpret a core CommonMark delimiter**; `underline` is
   excluded on that principle (Slice 0 characterization confirmed it parses
   `__bold__` as underline, overriding CommonMark strong, while every other mark
   adds meaning only to CommonMark-inert characters). Underline stays opt-in via
   `Construct::Underline`. This intentionally overrides the
   prior CommonMark-only default-output stability (an owner decision, cheap while
   `publish = false`, decisions/001); every construct already ships in the default
   build, so default features stay empty and dependencies zero.
2. **CommonMark / GFM / MDX are presets, not free functions**, run through a
   method on the configured options: `SyntaxOptions::gfm().parse(input)`. The verb
   lives on the configured thing. `parse(input)` is sugar for
   `SyntaxOptions::default().parse(input)`. `SyntaxOptions::parse_strict(input) ->
   Result<ParseOutput, ParseStrictError>` is the strict domain;
   `SyntaxOptions::validate() -> Result<(), SyntaxConfigError>` is the explicit
   config check. `.parse()` is infallible to match the tolerant-by-default
   philosophy — a config conflict (reachable only through raw field construction,
   since presets are valid and the builder folds dependencies) surfaces as a
   diagnostic; fail-fast callers call `validate()` first.
3. **MDX is a separate mode, never in the default**: MDX JSX and raw HTML both own
   `<` (the `MdxHtmlConflict` guard, options.rs:176), and MDX reinterprets
   `{…}`/`<…>` ubiquitous in hand-written Markdown. The directive family
   (`:name`/`::name`/`:::`) stays on and remains distinct from MDX.
4. **Delete `SyntaxProfile` and the `profile` field.** Re-key the only two reads
   (`profile == Gfm` at parse.rs:6012 and parse.rs:7256) onto
   `constructs.gfm_autolink_literal` — byte-identical for the shipped presets and
   closing the latent divergence where `custom(Constructs::gfm())` parsed those
   edge cases as non-Gfm.
5. **Delete `ResolvedSyntaxOptions`.** Rename `resolve()` to `validate(&self) ->
   Result<(), SyntaxConfigError>`, keeping both conflict guards and both error
   variants; thread `&SyntaxOptions` through the ~50 private parser signatures.
6. **Add a `Construct` enum + `SyntaxOptions::enable`/`disable`** consuming-builder
   (a plain `match` over the existing bools, no dependency), folding the
   parser-only latent dependencies (e.g. footnotes need definition+reference) into
   the mapping so silently-inert combinations cannot be expressed through it.
7. **Delete `SyntaxOptions::custom`.** With `profile` gone, `SyntaxOptions` is a
   two-field public struct (`constructs`, `parse`); the escape hatch is its public
   fields (struct literal / mutate-from-preset) plus the builder.
8. **Delimiter collision picks** baked into the maximal default (parser-enforced):
   `~~x~~` strikethrough vs `~x~` subscript (single-tilde strikethrough off);
   `^[…]` inline footnote vs `^x^` superscript; `:word:` shortcode vs `:name[…]`
   directive; `[[target|label]]` wikilinks (title-after-pipe).

### B. Output verbs, on `Document`

9. **Replace the free `to_markdown` / `to_markdown_with_options` / `to_html` /
   `to_html_with_options` / `validate_document` functions with methods on
   `Document`**: `to_markdown()`, `to_markdown_with(&SerializeOptions)`,
   `to_html()` and `to_html_with(&HtmlOptions)` (behind the `html` feature), and
   `validate() -> Vec<Diagnostic>`. The crate-wide rule is "configure → `parse`;
   then ask the `Document`"; the round-trip reads
   `parse(input).document.to_markdown()?`.

### C. Diagnostics

10. **Unify to a single `Diagnostic`.** Delete `ValidationDiagnostic`; make
    `Diagnostic.span` an `Option<Span>` and add `DiagnosticCode::InvalidDocument`
    (severity `Error`). `validate()` returns `Vec<Diagnostic>`;
    `SerializeError`/`HtmlError` carry `Vec<Diagnostic>`. One diagnostic type is
    learned and rendered uniformly across parse, validate, serialize, and html.

### D. AST ergonomics

11. **Uniform read accessors**: `Block::meta()/span()` and `Inline::meta()/span()`
    (the only `impl` in `ast.rs` today is `impl NodeMeta`, so a generic span read
    needs a 19-/30-arm match), plus `Inline::children() -> &[Inline]` (covers the
    `Image.alt`/`ImageReference.alt` naming hole). Block-level children stay
    match-based — they are heterogeneous, so no uniform typed accessor is forced.
12. **A minimal construction layer** over the struct literals, defaulting `meta` to
    `NodeMeta::default()`: `From<&str>`/`From<String>` for `Text`; `From<struct>`
    for `Block`/`Inline` for every variant (auto-wrap); `new(..)` on high-traffic
    nodes (`Text`, `Paragraph`, `Heading`, `Link`, `Code`, `List`).
13. **Keep `NodeMeta` (one-field wrapper) and the wrapped-struct enum idiom.**
    `NodeMeta` is retained for forward-compat (the `span()` accessor hides the
    `.meta.span` indirection); `Block::Paragraph(Paragraph)` and the variant/struct
    name drift (`Code`→`CodeInline`, etc.) stay so each node is a nameable type.

### E. Exports / hygiene

14. **`pub(crate)` the render internals** — `html::Ctx` (and any sibling `pub`
    render internals) must not be reachable as `markdown_syntax::html::Ctx`.
15. **Curate the crate root and add a `prelude`.** Replace the `diagnostic::*` /
    `options::*` root globs with explicit re-export lists (so future internals do
    not auto-export), and add a `prelude` module (`use markdown_syntax::prelude::*`)
    as the recommended one-line import. `ast::*` stays glob-exported at the root:
    the AST is the core data model (the `syn` pattern), an explicit 74-name list is
    high-maintenance for marginal gain, and the types remain reachable via the
    `ast::` path and the `prelude` regardless.
16. **Drop the unused `ParseOutput<T = Document>` generic.**

### Resolved leans

- `SyntaxOptions::default()` / `Constructs::default()` = **maximal** (consistent
  with `parse()`); `commonmark()`/`gfm()`/`mdx()` are the named non-default presets.
- **Fields stay public** (leanest while `publish = false`); revisit encapsulation
  (`#[non_exhaustive]` + builder-only) at publish time.
- **Wikilink tri-state** is surfaced only as `Construct::Wikilinks(order)` at the
  builder; the two raw bools + guard stay on `Constructs` for direct-field callers.

## Rationale

The surface audit (parser/serializer/validator/HTML grep + 3-persona walk +
ecosystem comparison) found the weight is not field count but redundant axes,
dead twins, missing on-ramps, and inconsistent verb placement. Highlights, with
evidence:

- `SyntaxProfile` is read only as `== Gfm` (parse.rs:6012/7256); `CommonMark`/
  `Mdx`/`Custom` are never matched, and `custom(Constructs::gfm())` stamped
  `profile = Custom` (options.rs:169) so it diverged from `gfm()` silently — a
  latent bug, not a feature.
- `ResolvedSyntaxOptions` (options.rs:199-204) is a field-identical clone read by
  nothing outside `options.rs`+`parse.rs`; `resolve()` only runs two guards then
  copies fields.
- `to_markdown_with_options` (serialize.rs:60) / `to_html_with_options`
  (html/mod.rs:113) are the verbose pattern the parse side dropped; `Diagnostic`
  (diagnostic.rs:22) vs `ValidationDiagnostic` (validate.rs:13) are two shapes for
  one concept; `html::Ctx` (html/mod.rs:85) leaks; `pub use ast::*` dumps 74 types.
- The `~33`-flag `Constructs` struct is *not* a problem — every flag is consumed
  and the common path never sees it; it stays as the exhaustive escape hatch. The
  `resolve()`/`validate()` conflict guards are a genuine, ecosystem-rare safety
  feature and are kept.

## Rejected Alternatives

- **Keep `parse()` CommonMark / GFM-only, or put MDX in the default.** Rejected:
  the owner chose maximal out-of-box recognition; MDX conflicts with raw HTML and
  mis-parses ordinary `{…}`/`<…>`, so it stays the `mdx()` mode.
- **`parse_commonmark`/`parse_gfm`/`parse_mdx` free functions, or keep
  `*_with_options`.** Rejected: presets via `.parse()` cover them and the suffix
  re-grows the surface this decision shrinks.
- **Keep `SyntaxOptions::custom`, `ResolvedSyntaxOptions`, `SyntaxProfile`, or two
  diagnostic types.** Rejected: each is redundant once the others land (see
  Rationale).
- **Output verbs on the options structs, or a full `build`/`md` node module, or a
  uniform typed `Block::children()`.** Rejected: the document is the value in hand
  on output; a per-node builder is too much surface for a secondary use case;
  block children are heterogeneous.
- **Shrink/group `Constructs`, inline enum variants, rename `alt`/`term`, or a
  `bitflags` dependency.** Rejected: lose real power/naming, churn the AST, or add
  a dependency.

## Non-Goals

- Changing the `Constructs` struct, the `resolve()`/`validate()` conflict guards,
  `parse_strict` / `ParseStrictError`, the `ParseOptions` boundary, AST node
  field shapes, serialize/html behavior or safe-by-default policy (decisions/002),
  or conflating `directive_*` with `mdx_*`.

## Verification Notes

Per the no-bless golden policy (decisions/004): making `parse()` maximal means
every `.ast`/`.canonical.md` golden produced through bare `parse()` now reflects
maximal output and must be hand-regenerated and verified correct in the same
commit — audit every bare `parse()` call site in `tests/` first; fixtures pinned
to a dialect via `profile_options()` are unaffected. The predicate re-key (item 4)
is independently byte-identical for the presets. All output/diagnostic/AST changes
are additive or mechanical (method moves, one diagnostic type, accessors, `From`/
`new`); the README doc-test moves to `output.document.to_markdown()?`. `to_html*`
stay `#[cfg(feature = "html")]` (decisions/002 boundary intact). `no_std + alloc`,
zero dependencies, empty default features, and MSRV 1.82 hold throughout; only the
default *output* is intentionally changed.
