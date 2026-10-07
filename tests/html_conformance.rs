#![cfg(feature = "html")]

//! AST → HTML conformance bench.
//!
//! This harness uses the crate's opt-in public HTML renderer purely to MEASURE
//! how faithfully the parser's AST reflects CommonMark/GFM semantics, by
//! comparing `parse(input) → to_html_with(AST) → HTML` against this
//! bench's own conformance suite under
//! `tests/fixtures/conformance/<category>/<source>.cases`.
//!
//! It exists because the rest of the suite verifies round-trip STABILITY, not
//! CORRECTNESS; this is the only place an actual conformance number is produced.
//!
//! Layout (each declared with an explicit `#[path]` from this crate root so the
//! submodules live under `tests/html_conformance/`):
//!   - `types`      — the types the modules share (OracleTuple, Category, …)
//!   - `normalizer` — faithful port of CommonMark `normalize.py`
//!   - `extractor`  — reads (input, expected_html, options) cases from our suite fixtures
//!   - `runner`     — parses each case and maps its render options → public render+compare
//!   - `report`     — pass/fail tallies, headline %, deviation report, failure dump
//!   - `deviations` — the cases that differ from their oracle, named by content, with reasons

#![allow(dead_code)]

use std::sync::OnceLock;

#[path = "html_conformance/types.rs"]
mod types;

#[path = "html_conformance/normalizer.rs"]
mod normalizer;

#[path = "html_conformance/extractor.rs"]
mod extractor;

#[path = "html_conformance/runner.rs"]
mod runner;

#[path = "html_conformance/report.rs"]
mod report;

#[path = "html_conformance/deviations.rs"]
mod deviations;

/// Snapshot-integrity check: our CommonMark-spec source fixture must carry
/// exactly 643 cases: the 652 of the upstream CommonMark spec corpus, less the
/// 9 whose input reads as a construct the CommonMark oracle lacks (literal
/// autolinks, wiki links, frontmatter), which cannot be compared.
#[test]
fn corpus_counts_match() {
    let tuples = extractor::load_all();
    let commonmark = tuples
        .iter()
        .filter(|t| t.source_file.ends_with("commonmark/commonmark.cases"))
        .count();
    assert_eq!(
        commonmark, 643,
        "commonmark/commonmark.cases must carry exactly 643 cases, got {commonmark}"
    );
}

/// Every case run once, shared by the tests below.
fn report() -> &'static report::Report {
    static REPORT: OnceLock<report::Report> = OnceLock::new();
    REPORT.get_or_init(runner::run_all)
}

/// The measurement: parse → render → compare every runnable oracle tuple and
/// print a per-suite / per-file conformance breakdown. Does NOT assert a
/// threshold — it reports a number and dumps failures for triage.
#[test]
fn html_conformance_report() {
    let report = report();
    report.print_summary();
    report.write_failures("target/html_conformance_failures.txt");
}

/// The exception lists in `deviations` stay current: every entry names a case
/// that exists and still fails. A failing case no entry names does not fail
/// this test, and neither does the pass rate; the report prints both.
#[test]
fn exception_lists_are_current() {
    let problems = report().list_problems();
    assert!(
        problems.is_empty(),
        "the exception lists in tests/html_conformance/deviations.rs are out of date:\n  {}",
        problems.join("\n  ")
    );
}
