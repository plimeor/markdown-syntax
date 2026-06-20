---
date: 2026-06-20
status: active
---

# HTML Renderer Boundary

## Decision

AST-to-HTML rendering belongs in `markdown-syntax` behind the non-default
`html` cargo feature. The default build exports no HTML renderer.

The renderer is safe-by-default: it validates the AST first, escapes or omits
raw HTML unless configured otherwise, and filters dangerous link/image
protocols unless configured otherwise. It does not evaluate MDX, run syntax
highlighting, build a DOM, or provide pluggable sanitization policy.

The public HTML contract is covered by focused expected-output tests in
`tests/html_regressions.rs`. The CommonMark/GFM AST-to-HTML conformance bench is
a measurement harness; current numbers come from running
`cargo test --features html --test html_conformance -- --nocapture`.

## Rationale

An opt-in feature preserves the default crate invariants while removing the
drift risk of having a private test renderer and a separate public renderer.
The conformance bench exercises the same public renderer that users call under
the `html` feature.

Safe-by-default rendering is part of the renderer boundary because raw Markdown
HTML and dangerous protocols are observable HTML output behavior. More advanced
sanitization policy is a product concern and would add a larger API surface than
the syntax crate needs.

## Rejected Alternatives

- Default-on `src/html`: rejected because every parse/serialize-only consumer
  would receive the HTML/XSS-emitting surface by default.
- Sibling `markdown-html` crate: rejected because the bench would either depend
  on a downstream consumer crate or keep a second private renderer, recreating
  drift.
- Test-only renderer plus public renderer later: rejected because it duplicates
  the highest-risk behavior.
- Conformance threshold gate: rejected because the bench is a measurement of a
  broad oracle corpus, while stable public HTML behavior is covered by focused
  regression tests.

## Non-Goals

- HTML sanitization plugins or custom allowlists.
- Syntax highlighting, table of contents generation, DOM editing, or MDX
  evaluation.
- Changing parser, AST, or serializer behavior merely to support HTML output.
