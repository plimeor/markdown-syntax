# Project overview
Updated 2026-10-06

## What this is

`markdown-syntax` is a `no_std + alloc` Rust crate that parses Markdown into an
owned AST and serializes the AST back to canonical Markdown. Raw HTML and MDX are
represented only as Markdown syntax nodes; safe-by-default HTML rendering is
available behind the opt-in `html` feature.

## Principles

- The default build has an empty feature set, zero runtime dependencies,
  `#![no_std]` with `alloc`, and MSRV 1.82; the crate is published, so these
  also protect downstream users.
- Default-build output stays byte-stable unless a plan changes it on purpose.
- Parsing, serialization, rendering, and validation stay linear in time and
  bounded in stack on any input.
- Parser correctness fixes are checked against the serializer too, because the
  serializer can mask a stably wrong parse.
- Changed `.ast` / `.canonical.md` goldens are regenerated and read for
  correctness one by one; there is no bless flag.

## Status
Shipped features are described in `docs/specs/`.

## Current focus

Releasing the delimiter-stack inline parser, CommonMark input handling (leading
BOM, NUL), and source-mapped spans as a SemVer-breaking version.

## Next

- `parse` panics on `"\u{a0}e+@"`; fix it before the release.
- Rebuild block parsing on one stack of open blocks, take the serializer's
  escapes and delimiter choices from the parser, and fold autolinks into
  `Link` (plimeor/markdown-syntax#11).

## Non-goals

- HTML rendering or sanitization in the default build.
- Treating `:name` / `::name` / `:::name` directives as MDX.
- Hand-maintained ledgers of test results, conformance numbers, or task
  progress.
