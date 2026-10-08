# 0010: Test evidence

Status: Accepted
Date: 2026-10-08
Supersedes: 0003, 0004

## Context

Round-trip stability alone proves that parse and serialize are stable
together; it does not prove the first parse is correct, and a serializer can
mask a stably wrong parse. A generated snapshot can faithfully record a wrong
parse, and a status file that no test checks drifts and is then taken as
authority.

## Decision

Fixtures are organized by role:

- `tests/fixtures/roundtrip/` holds parse and serialize goldens by topic and
  the executable round-trip case corpus; a `.cases` file there declares
  `role: upstream-input`, or it is a fixture error.
- `tests/fixtures/conformance/{commonmark,gfm}/` holds byte-counted
  expected-HTML oracle cases for the AST-to-HTML conformance bench, a
  measurement that is not a CI gate.
- `tests/fixtures/syntax_decisions/` holds the inputs of oracle cases this
  syntax reads differently by decision, with HTML verified against a
  reference renderer (cmark-gfm, commonmark.js, micromark) or the decision,
  checked exactly by `tests/syntax_decisions.rs`.

The repository keeps executable cases and provenance and license notices, not
inventories of upstream material that no test runs. Exception lists live in
code beside the test that derives them and fail when an entry goes stale.
Current results, such as conformance numbers, are observed from commands,
never stored in documents.

Correctness work pairs parser and serializer reasoning: a parse fix is
checked against both the AST shape and the written Markdown. There is no
bless flag: a golden a correct fix moves is regenerated in the same change
and read for structural correctness, never edited to make a failing test
pass. Focused regression tests own stable public behavior; the bench is
evidence, not a contract.

## Considered options

- Engine-owned fixture directories (`markdown-rs/`, `comrak/`): consumers need
  role and option metadata, not engine ownership in paths.
- Keeping decision cases on the bench in a deviation list: the bench would
  carry expected failures, and a list entry says why a case differs without
  checking what the syntax produces.
- Audit snapshots that are not executed, or hand-maintained result documents:
  they imply coverage or results that no command derives.
- Parser-only fixes, serializer shims for parser defects, or bulk blessing
  changed goldens: each lets a wrong AST survive.
- Relying on the bench alone: stable public behavior needs focused tests with
  known output.

## Consequences

- Upstream test repositories are not mirrored, and upstream cases not
  represented locally are not explained.
- Conformance output is not a release claim without running the bench.
- Hand-built AST invariants hold only as far as validation and tests cover
  them.
