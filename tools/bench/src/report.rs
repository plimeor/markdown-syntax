//! Comparing runs, merging repeated runs, and writing a run's tables as
//! Markdown.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::{Row, Run};

pub fn fmt_ns(ns: f64) -> String {
    if ns >= 1e9 {
        format!("{:.2} s", ns / 1e9)
    } else if ns >= 1e6 {
        format!("{:.2} ms", ns / 1e6)
    } else if ns >= 1e3 {
        format!("{:.1} µs", ns / 1e3)
    } else {
        format!("{ns:.0} ns")
    }
}

/// Real documents first, then the synthetic ones by name, then the mixed
/// scaling series by size.
pub fn corpus_order(name: &str) -> (u8, u64, String) {
    let size = |s: &str| match s {
        "1k" => 1,
        "10k" => 2,
        "100k" => 3,
        "1m" => 4,
        _ => 9,
    };
    if let Some(rest) = name.strip_prefix("real-") {
        let rank = match rest {
            "spec" => 0,
            "rust-releases" => 1,
            "docs" => 2,
            _ => 3,
        };
        (0, rank, name.to_owned())
    } else if name.starts_with("syn-") {
        (1, 0, name.to_owned())
    } else if let Some(rest) = name.strip_prefix("mixed-") {
        (2, size(rest), name.to_owned())
    } else {
        (3, 0, name.to_owned())
    }
}

fn geomean(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    Some((xs.iter().map(|x| x.ln()).sum::<f64>() / xs.len() as f64).exp())
}

pub fn merge(runs: Vec<Run>) -> Run {
    let mut by_key: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    let mut order = Vec::new();
    let mut started = Vec::new();
    let mut environment = None;
    for run in runs {
        started.push(run.environment.started.clone());
        environment.get_or_insert(run.environment);
        for row in run.rows {
            let key = row.key();
            if !by_key.contains_key(&key) {
                order.push(key.clone());
            }
            by_key.entry(key).or_default().push(row);
        }
    }
    let rows = order
        .into_iter()
        .map(|key| {
            let rows = &by_key[&key];
            let lo = rows
                .iter()
                .map(|r| r.median_ns)
                .fold(f64::INFINITY, f64::min);
            let hi = rows.iter().map(|r| r.median_ns).fold(0.0, f64::max);
            let mut best = rows
                .iter()
                .min_by(|a, b| a.median_ns.total_cmp(&b.median_ns))
                .unwrap()
                .clone();
            best.min_ns = rows.iter().map(|r| r.min_ns).fold(f64::INFINITY, f64::min);
            best.run_spread = hi / lo;
            best
        })
        .collect();
    let mut environment = environment.unwrap_or_default();
    environment.started = started.join(", ");
    Run { environment, rows }
}

pub fn compare(old: &Run, new: &Run) {
    let old_rows: BTreeMap<String, &Row> = old.rows.iter().map(|r| (r.key(), r)).collect();
    let mut per_op: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    println!();
    println!(
        "{:<44} {:>11} {:>11} {:>8} {:>8} {:>10}",
        "case", "old median", "new median", "median", "min", "allocs"
    );
    for row in &new.rows {
        let Some(prev) = old_rows.get(&row.key()) else {
            continue;
        };
        let ratio = row.median_ns / prev.median_ns;
        let min_ratio = row.min_ns / prev.min_ns;
        let mark = if ratio < 0.95 {
            "  faster"
        } else if ratio > 1.05 {
            "  SLOWER"
        } else {
            ""
        };
        println!(
            "{:<44} {:>11} {:>11} {:>7.3}x {:>7.3}x {:>+10}{mark}",
            row.key(),
            fmt_ns(prev.median_ns),
            fmt_ns(row.median_ns),
            ratio,
            min_ratio,
            row.allocs as i64 - prev.allocs as i64,
        );
        per_op
            .entry(format!("{}.{}", row.imp, row.op))
            .or_default()
            .push(ratio);
    }
    println!();
    println!("geometric mean of new/old median time, per operation:");
    for (op, ratios) in per_op {
        println!(
            "  {:<24} {:.3}x over {} corpora",
            op,
            geomean(&ratios).unwrap(),
            ratios.len()
        );
    }
}

struct Index<'a> {
    rows: BTreeMap<(String, String), &'a Row>,
    corpora: Vec<String>,
}

impl<'a> Index<'a> {
    fn new(run: &'a Run) -> Self {
        let mut corpora: Vec<String> = Vec::new();
        let mut rows = BTreeMap::new();
        for row in &run.rows {
            if !corpora.contains(&row.corpus) {
                corpora.push(row.corpus.clone());
            }
            rows.insert((row.corpus.clone(), format!("{}.{}", row.imp, row.op)), row);
        }
        corpora.sort_by_key(|c| corpus_order(c));
        Index { rows, corpora }
    }
    fn get(&self, corpus: &str, op: &str) -> Option<&'a Row> {
        self.rows.get(&(corpus.to_owned(), op.to_owned())).copied()
    }
}

/// The corpora a geometric mean is taken over: the real documents and the
/// ~100 KB synthetic ones, not the mixed scaling series.
fn in_geomean(corpus: &str) -> bool {
    corpus.starts_with("real-") || corpus.starts_with("syn-")
}

fn cell(row: Option<&Row>) -> String {
    match row {
        Some(r) => format!("{} ({:.0})", fmt_ns(r.median_ns), r.mbps()),
        None => "—".to_owned(),
    }
}

/// A table with one column per operation, `time (MB/s)` in each cell, and
/// one ratio column per `(label, ours, peers)`: ours' median time over the
/// fastest peer's.
fn comparison(
    out: &mut String,
    ix: &Index,
    columns: &[(&str, &str)],
    ratios: &[(&str, &str, &[&str])],
) {
    let _ = write!(out, "| corpus |");
    for (label, _) in columns {
        let _ = write!(out, " {label} |");
    }
    for (label, _, _) in ratios {
        let _ = write!(out, " {label} |");
    }
    out.push_str("\n|---|");
    for _ in 0..columns.len() + ratios.len() {
        out.push_str("---:|");
    }
    out.push('\n');
    let mut collected: Vec<Vec<f64>> = vec![Vec::new(); ratios.len()];
    for corpus in &ix.corpora {
        if columns.iter().all(|(_, op)| ix.get(corpus, op).is_none()) {
            continue;
        }
        let _ = write!(out, "| {corpus} |");
        for (_, op) in columns {
            let _ = write!(out, " {} |", cell(ix.get(corpus, op)));
        }
        for (k, (_, ours, peers)) in ratios.iter().enumerate() {
            let fastest = peers
                .iter()
                .filter_map(|p| ix.get(corpus, p))
                .map(|r| r.median_ns)
                .fold(f64::INFINITY, f64::min);
            match ix.get(corpus, ours) {
                Some(r) if fastest.is_finite() => {
                    let ratio = r.median_ns / fastest;
                    if in_geomean(corpus) {
                        collected[k].push(ratio);
                    }
                    let _ = write!(out, " {ratio:.2}× |");
                }
                _ => out.push_str(" — |"),
            }
        }
        out.push('\n');
    }
    if collected.iter().any(|c| !c.is_empty()) {
        let _ = write!(out, "| **geomean** (real + syn) |");
        for _ in columns {
            out.push_str(" |");
        }
        for c in &collected {
            match geomean(c) {
                Some(g) => {
                    let _ = write!(out, " **{g:.2}×** |");
                }
                None => out.push_str(" — |"),
            }
        }
        out.push('\n');
    }
    out.push('\n');
}

pub fn markdown(run: &Run) -> String {
    let ix = Index::new(run);
    let mut out = String::new();
    let env = &run.environment;
    let _ = writeln!(
        out,
        "<!-- generated by `cargo run --release -- --report` -->"
    );
    let _ = writeln!(out, "- CPU: {}", env.cpu);
    let _ = writeln!(out, "- {}", env.rustc);
    let _ = writeln!(out, "- {}", env.crates);
    let _ = writeln!(
        out,
        "- sample time per case: {} s; run(s) started {}",
        env.time_per_case_s, env.started
    );
    out.push('\n');
    out.push_str("Cells are `median time (MB/s)`. A ratio is our median time over the fastest peer's for the same work: above 1 is slower.\n\n");

    out.push_str("### Parse to a tree\n\n");
    out.push_str("pulldown-cmark builds no tree: `pulldown.events` iterates every event of its streaming parser, a lower bound for any tree builder.\n\n");
    comparison(
        &mut out,
        &ix,
        &[
            ("ours.parse", "ours.parse"),
            ("comrak.parse", "comrak.parse"),
            ("mdrs.mdast", "mdrs.mdast"),
            ("pulldown.events (stream)", "pulldown.events"),
        ],
        &[
            (
                "ours ÷ fastest tree",
                "ours.parse",
                &["comrak.parse", "mdrs.mdast"],
            ),
            ("ours ÷ pulldown", "ours.parse", &["pulldown.events"]),
        ],
    );

    out.push_str("### Markdown to HTML (parse + render)\n\n");
    comparison(
        &mut out,
        &ix,
        &[
            ("ours.html", "ours.html"),
            ("pulldown.html", "pulldown.html"),
            ("comrak.html", "comrak.html"),
            ("mdrs.html", "mdrs.html"),
        ],
        &[
            (
                "ours ÷ fastest",
                "ours.html",
                &["pulldown.html", "comrak.html", "mdrs.html"],
            ),
            ("ours ÷ comrak", "ours.html", &["comrak.html"]),
        ],
    );

    out.push_str("### HTML from a parsed tree (render only)\n\n");
    comparison(
        &mut out,
        &ix,
        &[
            ("ours.render", "ours.render"),
            ("comrak.render", "comrak.render"),
        ],
        &[("ours ÷ comrak", "ours.render", &["comrak.render"])],
    );

    out.push_str("### Markdown from a parsed tree\n\n");
    out.push_str("`ours.writeback` writes a parsed, unedited document with its source (the editor's save path); `ours.canonical` writes the same tree `without_source()`; `comrak.commonmark` formats its tree as CommonMark, which never reuses the source.\n\n");
    comparison(
        &mut out,
        &ix,
        &[
            ("ours.writeback", "ours.writeback"),
            ("ours.canonical", "ours.canonical"),
            ("comrak.commonmark", "comrak.commonmark"),
        ],
        &[
            (
                "writeback ÷ comrak",
                "ours.writeback",
                &["comrak.commonmark"],
            ),
            (
                "canonical ÷ comrak",
                "ours.canonical",
                &["comrak.commonmark"],
            ),
        ],
    );

    out.push_str("### Our sub-costs\n\n");
    out.push_str("`editor_parse` is `parse` then `leaf_sources`, the Rust work of the wasm binding's `parse` before encoding. `leaf_sources`, `validate`, `definitions`, and `inline_leaves` run on a parsed document; `inline_leaves` calls `parse_inline` on every leaf's inline source; `inline_largest` on the longest one only, with MB/s over that leaf's text.\n\n");
    comparison(
        &mut out,
        &ix,
        &[
            ("parse", "ours.parse"),
            ("editor_parse", "ours.editor_parse"),
            ("leaf_sources", "ours.leaf_sources"),
            ("validate", "ours.validate"),
            ("definitions", "ours.definitions"),
            ("inline_leaves", "ours.inline_leaves"),
            ("inline_largest", "ours.inline_largest"),
        ],
        &[
            ("editor_parse ÷ parse", "ours.editor_parse", &["ours.parse"]),
            ("writeback ÷ parse", "ours.writeback", &["ours.parse"]),
        ],
    );

    out.push_str("### Allocations per call\n\n");
    out.push_str("Cells are `allocation calls / MB requested / MB peak live`. `realloc` counts as a call requesting its new size.\n\n");
    let alloc_ops = [
        "ours.parse",
        "comrak.parse",
        "mdrs.mdast",
        "pulldown.events",
        "ours.editor_parse",
        "ours.writeback",
        "ours.canonical",
        "ours.html",
        "pulldown.html",
        "comrak.html",
    ];
    let _ = write!(out, "| corpus |");
    for op in alloc_ops {
        let _ = write!(out, " {op} |");
    }
    out.push_str("\n|---|");
    for _ in alloc_ops {
        out.push_str("---:|");
    }
    out.push('\n');
    let mb = |b: u64| b as f64 / 1e6;
    for corpus in &ix.corpora {
        let _ = write!(out, "| {corpus} |");
        for op in alloc_ops {
            match ix.get(corpus, op) {
                Some(r) => {
                    let _ = write!(
                        out,
                        " {} / {:.1} / {:.1} |",
                        r.allocs,
                        mb(r.alloc_bytes),
                        mb(r.peak_bytes)
                    );
                }
                None => out.push_str(" — |"),
            }
        }
        out.push('\n');
    }
    out.push('\n');

    out.push_str("### Scaling over mixed-1k … mixed-1m (ns per input byte, median)\n\n");
    let scale_ops = [
        "ours.parse",
        "ours.editor_parse",
        "ours.leaf_sources",
        "ours.writeback",
        "ours.canonical",
        "ours.html",
        "ours.inline_leaves",
        "pulldown.events",
        "comrak.parse",
        "comrak.html",
        "mdrs.mdast",
    ];
    let mixed: Vec<&String> = ix
        .corpora
        .iter()
        .filter(|c| c.starts_with("mixed-"))
        .collect();
    let _ = write!(out, "| op |");
    for c in &mixed {
        let _ = write!(out, " {c} |");
    }
    out.push_str(" 1m ÷ 10k |\n|---|");
    for _ in &mixed {
        out.push_str("---:|");
    }
    out.push_str("---:|\n");
    for op in scale_ops {
        let _ = write!(out, "| {op} |");
        let per_byte = |c: &str| ix.get(c, op).map(|r| r.median_ns / r.bytes as f64);
        for c in &mixed {
            match per_byte(c) {
                Some(v) => {
                    let _ = write!(out, " {v:.2} |");
                }
                None => out.push_str(" — |"),
            }
        }
        match (per_byte("mixed-10k"), per_byte("mixed-1m")) {
            (Some(a), Some(b)) => {
                let _ = writeln!(out, " {:.2} |", b / a);
            }
            _ => out.push_str(" — |\n"),
        }
    }
    out.push('\n');

    out.push_str("### Noise\n\n");
    let mut spreads: Vec<(f64, String)> = run
        .rows
        .iter()
        .map(|r| (r.median_ns / r.min_ns, r.key()))
        .collect();
    spreads.sort_by(|a, b| b.0.total_cmp(&a.0));
    let within: Vec<f64> = spreads.iter().map(|s| s.0).collect();
    let mut sorted = within.clone();
    sorted.sort_by(f64::total_cmp);
    let _ = writeln!(
        out,
        "Median ÷ min within a run, over {} cases: median {:.3}, 90th percentile {:.3}, max {:.3} ({}).",
        sorted.len(),
        sorted[sorted.len() / 2],
        sorted[(sorted.len() - 1) * 9 / 10],
        spreads[0].0,
        spreads[0].1
    );
    let mut runs: Vec<(f64, String)> = run.rows.iter().map(|r| (r.run_spread, r.key())).collect();
    runs.sort_by(|a, b| b.0.total_cmp(&a.0));
    if runs.iter().any(|r| r.0 > 1.0) {
        let mut rs: Vec<f64> = runs.iter().map(|r| r.0).collect();
        rs.sort_by(f64::total_cmp);
        let _ = writeln!(
            out,
            "Largest ÷ smallest median across the merged runs: median {:.3}, 90th percentile {:.3}, max {:.3} ({}).",
            rs[rs.len() / 2],
            rs[(rs.len() - 1) * 9 / 10],
            runs[0].0,
            runs[0].1
        );
        out.push_str("Cases above 1.10 across runs:");
        let noisy: Vec<String> = runs
            .iter()
            .filter(|r| r.0 > 1.10)
            .map(|r| format!(" {} ({:.2})", r.1, r.0))
            .collect();
        if noisy.is_empty() {
            out.push_str(" none.\n");
        } else {
            out.push_str(&noisy.join(","));
            out.push_str(".\n");
        }
    }
    out.push('\n');

    out.push_str("### Every case\n\n");
    out.push_str("| case | bytes | median | min | p90 | MB/s | samples × batch | allocs | MB req | MB peak | run spread |\n");
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    let mut rows: Vec<&Row> = run.rows.iter().collect();
    rows.sort_by_key(|r| (corpus_order(&r.corpus), r.imp.clone(), r.op.clone()));
    for r in rows {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {:.1} | {} × {} | {} | {:.2} | {:.2} | {:.2} |",
            r.key(),
            r.bytes,
            fmt_ns(r.median_ns),
            fmt_ns(r.min_ns),
            fmt_ns(r.p90_ns),
            r.mbps(),
            r.samples,
            r.batch,
            r.allocs,
            mb(r.alloc_bytes),
            mb(r.peak_bytes),
            r.run_spread
        );
    }
    out
}
