//! Parses each suite case with the crate's one syntax, maps its captured
//! render option tokens to [`HtmlOptions`], runs parse→render→compare, and
//! collects [`Report`].
//!
//! This is the single place the option vocabulary is interpreted, so the
//! suite fixtures can stay faithful token-capturers and the renderer a pure
//! function of `(Document, HtmlOptions)`. Parse tokens (`gfm`, `math`,
//! `extension.*`, …) selected a dialect when the crate had several; every case
//! now parses with `parse`, and a case whose oracle needs another dialect is
//! listed in `crate::deviations` with its reason.

use markdown_syntax::{parse, HtmlOptions, SafeRawHtmlForm, TasklistAttrOrder};

use crate::extractor;
use crate::normalizer::compare;
use crate::report::{CaseResult, Outcome, Report};
use crate::types::{Category, OracleTuple};

fn token(t: &OracleTuple, name: &str) -> bool {
    t.option_tokens.iter().any(|tok| tok == name)
}

fn ext(t: &OracleTuple, name: &str) -> bool {
    // GFM bracket entry, e.g. "extension.table"
    let needle = format!("extension.{name}");
    t.option_tokens.iter().any(|tok| tok == &needle)
}

/// Map a case's render option tokens to render options.
fn plan(t: &OracleTuple) -> HtmlOptions {
    // Render options: single GFM math form; flags from both token vocabularies.
    let mut cfg = HtmlOptions::default();
    cfg.safe_raw_html_form = match t.category {
        Category::CommonMark => SafeRawHtmlForm::EscapeText,
        Category::Gfm => SafeRawHtmlForm::OmitPlaceholder,
    };
    cfg.tasklist_attr_order = match t.category {
        Category::CommonMark => TasklistAttrOrder::DisabledFirst,
        Category::Gfm => TasklistAttrOrder::CheckedFirst,
    };
    cfg.allow_dangerous_html = token(t, "allow_dangerous_html");
    cfg.allow_dangerous_protocol = token(t, "allow_dangerous_protocol");
    cfg.allow_any_img_src = token(t, "allow_any_img_src");
    cfg.gfm_tagfilter = token(t, "gfm_tagfilter");
    cfg.tasklist_checkable = token(t, "tasklist_checkable");
    // GFM `render.unsafe_` (raw identifier `render.r#unsafe`) → danger.
    if token(t, "render.unsafe_") || token(t, "render.r#unsafe") || token(t, "render.unsafe") {
        cfg.allow_dangerous_html = true;
        cfg.allow_dangerous_protocol = true;
    }
    if ext(t, "tagfilter") {
        cfg.gfm_tagfilter = true;
    }
    if token(t, "render.tasklist_classes") {
        cfg.tasklist_checkable = true;
    }

    cfg
}

pub fn run_all() -> Report {
    let tuples = extractor::load_all();
    let mut results = Vec::with_capacity(tuples.len());

    for t in &tuples {
        let cfg = plan(t);
        let output = parse(&t.input);
        let outcome = match output.document.to_html_with(&cfg) {
            Ok(html) => {
                let cmp = compare(&html, &t.expected_html);
                if cmp.raw_match {
                    Outcome::PassRaw
                } else if cmp.normalized_match {
                    Outcome::PassNormalized
                } else {
                    Outcome::Fail {
                        expected: t.expected_html.clone(),
                        actual: html,
                    }
                }
            }
            Err(e) => Outcome::ParseError(format!("html render error: {e:?}")),
        };
        results.push(CaseResult {
            source_file: t.source_file,
            index: t.index,
            category: t.category,
            label: t.label.clone(),
            options: t.options(),
            input: t.input.clone(),
            outcome,
        });
    }

    Report { results }
}
