---
date: 2026-06-20
status: active
---

# Test Corpus Boundaries

## Decision

The fixture tree is organized by role:

- `tests/fixtures/roundtrip/` owns parse/serialize stability fixtures and
  executable round-trip case snapshots.
- `tests/fixtures/conformance/{commonmark,gfm}/` owns byte-counted expected-HTML
  oracle cases for the AST-to-HTML conformance bench.

The repository keeps executable cases and provenance/license notices, not a
hand-maintained inventory of upstream material that is absent from the runnable
surface. A `.cases` file under the executable round-trip corpus must declare
`role: upstream-input`; otherwise it is a fixture error, not an ignored audit
artifact.

Current conformance numbers are not stored in a Markdown ledger. They are
observed from the conformance command output.

## Rationale

Role-separated fixtures make the test owner clear: round-trip fixtures prove
stability of the public parse/serialize boundary, while conformance fixtures
compare parse-to-AST-to-HTML behavior against expected-HTML oracles.

Keeping only runnable cases avoids the misleading state where a file appears in
the project but is not actually tested. Provenance identifiers may remain in
case headers, but they are not on-disk paths to copied upstream Rust sources.

Manual result files such as a conformance ledger drift unless a test updates or
checks them. A stale status file is worse than no status file because agents may
treat it as authority.

## Rejected Alternatives

- Engine-owned fixture directories such as separate `markdown-rs/` and `comrak/`
  trees: rejected because the current consumers need role and option metadata,
  not engine ownership encoded in paths.
- Broad audit snapshots that are not executed: rejected because they imply test
  coverage that does not exist.
- Hand-maintained current-result documents: rejected because current results
  must be derived from commands.

## Non-Goals

- Mirroring all upstream test repositories.
- Explaining every upstream case that is not represented locally.
- Treating conformance output as a release claim without running the bench.
