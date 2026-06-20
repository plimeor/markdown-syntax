# markdown-syntax Docs Index

Routing index for the collaboration document set. Read this first, then drill
into a specific doc. Every Requirement / Plan / Tasking / Decision carries YAML
front matter (`date`, `status`); treat that front matter as the source of truth
for status, not this file.

## Document system

Typed substrate (one home per fact; the rest cite it):

- `decisions/` — durable authority: chosen direction, rejected alternatives with
  reasons, the non-goals. Conclusions that outlive the task that produced them.
  Sequential `NNN-<slug>.md`. *(empty — see Decisions below.)*
- `requirements/` — goals, scope, non-goals, UX expectations, acceptance
  criteria. *(empty — the only requirement so far was an in-session request,
  recorded inline in the renderer plan rather than as a doc.)*
- `plans/` — recommended implementation approach, phase breakdown, risks,
  verification, stop condition. Derived from a requirement.
- `tasking/` — the concrete execution graph derived from an accepted plan.
- `archive/` — cold storage for completed evidence and old analysis substrate.
- `agent/current.md` — the active execution cursor (no front matter,
  rewrite-in-place).

These docs are **historical records and durable authority, not current interface
contracts.** Public API, crate behavior, and the canonical syntax scope live in
`README.md`; **current conformance numbers** live in
`tests/html_conformance/CONFORMANCE.md` — not in these historical docs.

## Decisions

*None yet.* The HTML-renderer architecture (opt-in `html` feature; reject
default-on and a sibling `markdown-html` crate) is recorded in the completed
renderer plan below; promoting it to a `decisions/` record is a deferred
`后续` candidate, not yet created.

## Requirements

*None yet.*

## Plans

| Path | Status | Purpose |
| ---- | ------ | ------- |
| `plans/2026-06-20-markdown-syntax-html-renderer.md` | completed | Promote the test-only CommonMark/GFM renderer into `src/html/` behind an opt-in, non-default `html` cargo feature (safe-by-default, `no_std`+`alloc`, zero-dep); re-point the conformance bench at the public `to_html` API. Holds the renderer architecture decision in-body. |
| `plans/2026-06-19-markdown-syntax-test-reorg.md` | completed | Test tree reorganization: role-separated `tests/fixtures/roundtrip/` and `tests/fixtures/conformance/{commonmark,gfm}/`; no `markdown-rs` vs `comrak` engine-owned fixture split. |

## Tasking

| Path | Status | Purpose |
| ---- | ------ | ------- |
| `tasking/2026-06-19-markdown-syntax-conformance-fix.md` | completed | Parser correctness workstream substrate: original leaf-first T001–T025 graph for paired parser+serializer fixes; completed with current numbers in `tests/html_conformance/CONFORMANCE.md`. |

## Archive

| Path | Status | Purpose |
| ---- | ------ | ------- |
| `archive/2026-06-19-markdown-syntax-conformance-investigation.md` | archived | Consolidated conformance-defect analysis substrate (clusters, root causes, ripple sets) that the conformance-fix tasking operationalizes. Headline counts (89.94% / 234 blocks) are stale; the root-cause analysis remains valid. |

## Status lifecycle

- Supersession (any type): a doc is overturned only by a **newer same-type** doc.
  Both stamps land in the same edit — the new doc carries `supersedes:` +
  `status: active`; the old doc is stamped `superseded_by:` + `status:
  superseded`, its body preserved verbatim. At most one doc per question is
  `active` authority.
- **Decision**: `draft` → `active` → `superseded` only (never `completed`).
- **Requirement / Plan / Tasking**: `draft` → `active` → `completed` →
  `archived` (or branch to `superseded`).
