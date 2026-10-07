//! Markdown → HTML for the inputs where this syntax reads differently from an
//! upstream oracle by decision: a construct the syntax drops, one an oracle
//! lacks, or a rule it shares with another reference implementation.
//!
//! Each case in `tests/fixtures/syntax_decisions/` names the upstream case it
//! came from, numbered as in its file under `tests/fixtures/conformance/` at
//! commit 8ba83e2, before it left the bench, and what verified its expected
//! HTML: the reference renderers that
//! produce the same HTML (cmark-gfm, commonmark.js, micromark with GFM, math,
//! and frontmatter extensions and single-tilde strikethrough off), or the
//! decision that sets the reading where no reference shares it. The HTML must
//! match exactly.

#![cfg(feature = "html")]

use std::fs;
use std::path::Path;

use markdown_syntax::{parse, HtmlOptions};

const ROOT: &str = "tests/fixtures/syntax_decisions";

/// One case of a `.cases` file.
struct Case {
    index: usize,
    options: Vec<String>,
    label: String,
    input: String,
    expected: String,
}

/// Reads a byte-counted `.cases` file: each case is a header line
/// `--- case <i> options <tokens|-> label-bytes <L> input-bytes <I>
/// expected-bytes <E>`, then the label, the input, and the expected HTML.
fn read_cases(path: &Path) -> Vec<Case> {
    let source =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let declared: usize = source
        .lines()
        .find_map(|line| line.strip_prefix("count: "))
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("{}: no count", path.display()));
    let mut cases = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find("--- case ") {
        let header_start = cursor + offset;
        let header_end = header_start + source[header_start..].find('\n').expect("header line");
        let parts: Vec<&str> = source[header_start..header_end]
            .split_whitespace()
            .collect();
        assert_eq!(parts.len(), 11, "{}: header {:?}", path.display(), parts);
        let number = |at: usize| parts[at].parse::<usize>().expect("byte count");
        let (label_len, input_len, expected_len) = (number(6), number(8), number(10));
        let take = |start: usize, len: usize, marker: &str| {
            let text = source[start..start + len].to_string();
            assert!(
                source[start + len..].starts_with(marker),
                "{}: expected {marker:?} after case {}",
                path.display(),
                parts[2]
            );
            (text, start + len + marker.len())
        };
        let (label, next) = take(header_end + 1, label_len, "\n--- input\n");
        let (input, next) = take(next, input_len, "\n--- expected\n");
        let (expected, next) = take(next, expected_len, "\n--- end\n");
        cases.push(Case {
            index: number(2),
            options: if parts[4] == "-" {
                Vec::new()
            } else {
                parts[4].split(',').map(String::from).collect()
            },
            label,
            input,
            expected,
        });
        cursor = next;
    }
    assert_eq!(cases.len(), declared, "{}: declared count", path.display());
    cases
}

fn options(case: &Case) -> HtmlOptions {
    let mut options = HtmlOptions::default();
    for token in &case.options {
        match token.as_str() {
            "allow_dangerous_html" => options.allow_dangerous_html = true,
            "allow_dangerous_protocol" => options.allow_dangerous_protocol = true,
            "gfm_tagfilter" => options.gfm_tagfilter = true,
            other => panic!("unknown option token {other:?}"),
        }
    }
    options
}

#[test]
fn decision_cases_render_as_verified() {
    let mut paths: Vec<_> = fs::read_dir(ROOT)
        .expect("syntax decision fixtures")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "cases")
        })
        .collect();
    paths.sort();
    let mut checked = 0;
    for path in &paths {
        for case in read_cases(path) {
            let html = parse(&case.input)
                .document
                .to_html_with(&options(&case))
                .unwrap_or_else(|error| {
                    panic!("{} case {}: {error:?}", path.display(), case.index)
                });
            assert_eq!(
                html,
                case.expected,
                "{} case {} ({})\ninput: {:?}",
                path.display(),
                case.index,
                case.label,
                case.input
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 97, "syntax decision cases drifted");
}
