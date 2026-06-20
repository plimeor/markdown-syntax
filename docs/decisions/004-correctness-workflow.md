---
date: 2026-06-20
status: active
---

# Correctness Workflow

## Decision

Parser correctness work uses paired parser and serializer reasoning. The
serializer can mask a stably wrong parse, so parse fixes must be checked against
both AST shape and serialization behavior.

There is no bless flag. When a correct parser or serializer fix legitimately
moves `.ast` or `.canonical.md` goldens, the changed goldens are regenerated in
the same change and read for structural correctness. A golden is not edited just
to make a failing test pass.

The HTML conformance bench is used as observed evidence for AST correctness
against CommonMark/GFM expected-HTML oracles. Focused regression tests own stable
public contracts.

## Rationale

Round-trip stability alone proves that parse and serialize are stable together;
it does not prove the first parse is semantically correct. Pairing parser,
serializer, fixture, regression, and conformance evidence makes a fix harder to
hide behind a compensating serializer behavior.

Manual review of changed goldens is required because a generated snapshot can
faithfully record a wrong parse. The human/agent check is not "does the test
pass"; it is "does this structure express the correct Markdown semantics."

## Rejected Alternatives

- Parser-only fixes with no serializer review: rejected because serializer
  behavior can mask AST mistakes.
- Serializer-only shims for parser defects: rejected because they preserve
  invalid AST state.
- Bulk blessing changed goldens: rejected because it converts tests into output
  recorders rather than correctness checks.
- Relying only on the broad conformance bench: rejected because stable public
  behavior also needs focused regression tests with known expected output.

## Non-Goals

- Encoding a full task graph for future agents.
- Storing current conformance numbers in documentation.
- Guaranteeing every hand-written AST invariant beyond what validators and tests
  explicitly cover.
