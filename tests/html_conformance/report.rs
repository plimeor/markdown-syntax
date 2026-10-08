//! Conformance result aggregation + reporting.

use std::collections::BTreeMap;
use std::fs;

use crate::deviations::{excerpt, input_hash, Listed, DEVIATIONS, KNOWN_DEFECTS};
use crate::types::Category;

/// Per-case outcome.
pub enum Outcome {
    /// Byte-equal after only trimming the document edge (strictest).
    PassRaw,
    /// Equal after full CommonMark normalization, but not byte-raw (cosmetic
    /// difference the normalizer legitimately erases).
    PassNormalized,
    /// Real mismatch after normalization.
    Fail { expected: String, actual: String },
    /// The renderer returned an error for this input.
    ParseError(String),
}

pub struct CaseResult {
    pub source_file: &'static str,
    pub index: usize,
    pub category: Category,
    pub label: Option<String>,
    /// The case's option tokens as its header writes them (`-` for none).
    pub options: String,
    /// The Markdown input.
    pub input: String,
    pub outcome: Outcome,
}

impl CaseResult {
    fn failed(&self) -> bool {
        matches!(self.outcome, Outcome::Fail { .. } | Outcome::ParseError(_))
    }

    fn listed_by(&self, entry: &Listed) -> bool {
        entry.names(self.source_file, &self.options, &self.input)
    }

    fn listed(&self) -> bool {
        listed().any(|entry| self.listed_by(entry))
    }
}

fn listed() -> impl Iterator<Item = &'static Listed> {
    DEVIATIONS.iter().chain(KNOWN_DEFECTS)
}

pub struct Report {
    pub results: Vec<CaseResult>,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    pass_raw: usize,
    pass_norm: usize,
    fail: usize,
    parse_error: usize,
}

impl Tally {
    fn add(&mut self, o: &Outcome) {
        match o {
            Outcome::PassRaw => self.pass_raw += 1,
            Outcome::PassNormalized => self.pass_norm += 1,
            Outcome::Fail { .. } => self.fail += 1,
            Outcome::ParseError(_) => self.parse_error += 1,
        }
    }
    fn ran(&self) -> usize {
        self.pass_raw + self.pass_norm + self.fail + self.parse_error
    }
    fn passed(&self) -> usize {
        self.pass_raw + self.pass_norm
    }
    fn pct(&self) -> f64 {
        let ran = self.ran();
        if ran == 0 {
            0.0
        } else {
            100.0 * self.passed() as f64 / ran as f64
        }
    }
}

impl Report {
    pub fn print_summary(&self) {
        let mut by_suite: BTreeMap<&'static str, Tally> = BTreeMap::new();
        let mut by_file: BTreeMap<&'static str, Tally> = BTreeMap::new();
        let mut total = Tally::default();

        for r in &self.results {
            let sname = category_name(r.category);
            by_suite.entry(sname).or_default().add(&r.outcome);
            by_file.entry(r.source_file).or_default().add(&r.outcome);
            total.add(&r.outcome);
        }

        println!("\n================ AST→HTML CONFORMANCE ================");
        println!(
            "total cases: {}   ran: {}   passed: {}   failed: {}   parse-errors: {}",
            self.results.len(),
            total.ran(),
            total.passed(),
            total.fail,
            total.parse_error,
        );
        println!(
            "HEADLINE conformance: pass {} / ran {} = {:.2}%   (byte-exact: {}, normalized-only: {})",
            total.passed(),
            total.ran(),
            total.pct(),
            total.pass_raw,
            total.pass_norm,
        );

        println!("\n-- by suite --");
        for (suite, t) in &by_suite {
            println!(
                "  {suite:<12} ran {:>5}  pass {:>5} ({:.2}%)  fail {:>5}  perr {:>4}",
                t.ran(),
                t.passed(),
                t.pct(),
                t.fail,
                t.parse_error,
            );
        }

        println!("\n-- files with failures (file: fail/ran, parse-errors) --");
        let mut files: Vec<(&&str, &Tally)> = by_file.iter().collect();
        files.sort_by(|a, b| {
            b.1.fail
                .cmp(&a.1.fail)
                .then(b.1.parse_error.cmp(&a.1.parse_error))
        });
        for (file, t) in files {
            if t.fail == 0 && t.parse_error == 0 {
                continue;
            }
            let short = file.rsplit('/').next().unwrap_or(file);
            println!(
                "  {short:<32} fail {:>4}/{:<5}  perr {:>4}  pass {:>4} ({:.1}%)",
                t.fail,
                t.ran(),
                t.parse_error,
                t.passed(),
                t.pct(),
            );
        }
        self.print_deviations();
        println!("=====================================================\n");
    }

    /// The cases that differ from their oracle are listed in
    /// `crate::deviations`. Prints, apart from each other, the problems with
    /// the lists and the unlisted cases that fail, each with an entry to fill
    /// in.
    fn print_deviations(&self) {
        let problems = self.list_problems();
        let unlisted: Vec<&CaseResult> = self
            .results
            .iter()
            .filter(|r| r.failed() && !r.listed())
            .collect();
        println!(
            "\n-- deviations: {} by design, {} known defects, {} list problems, {} unlisted failing --",
            DEVIATIONS.len(),
            KNOWN_DEFECTS.len(),
            problems.len(),
            unlisted.len()
        );
        for problem in &problems {
            println!("  {problem}");
        }
        for r in unlisted {
            println!(
                "  UNLISTED deviation: {} case {}\n    case({:?}, {:?}, {:#018x}, {:?}, \"<reason>\"),",
                r.source_file,
                r.index,
                r.source_file,
                r.options,
                input_hash(&r.input),
                excerpt(&r.input),
            );
        }
    }

    /// The entries in `crate::deviations` that are out of date: one that names
    /// no case, one whose case now passes, and one that names the same case as
    /// an earlier entry. A failing case no entry names is not a problem: the
    /// bench measures, it does not gate.
    pub fn list_problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let entries: Vec<&Listed> = listed().collect();
        for (n, entry) in entries.iter().enumerate() {
            let cases: Vec<&CaseResult> =
                self.results.iter().filter(|r| r.listed_by(entry)).collect();
            let at = format!("{} {:?} {:?}", entry.file, entry.options, entry.excerpt);
            if cases.is_empty() {
                problems.push(format!("listed, no such case: {at}"));
            }
            for r in cases.iter().filter(|r| !r.failed()) {
                problems.push(format!("listed, now passes: {at} (case {})", r.index));
            }
            if entries[..n].iter().any(|earlier| {
                earlier.file == entry.file
                    && earlier.options == entry.options
                    && earlier.input_hash == entry.input_hash
            }) {
                problems.push(format!("listed twice: {at}"));
            }
        }
        problems
    }

    /// Dump every failure (and parse error) as an inspectable block for triage.
    pub fn write_failures(&self, path: &str) {
        let mut out = String::new();
        let mut n = 0;
        for r in &self.results {
            match &r.outcome {
                Outcome::Fail { expected, actual } => {
                    n += 1;
                    out.push_str(&format!(
                        "### FAIL #{n} [{} case {}] {}\n--- input ---\n{}\n--- expected ---\n{}\n--- actual ---\n{}\n\n",
                        r.source_file,
                        r.index,
                        r.label.as_deref().unwrap_or(""),
                        show(&r.input),
                        show(expected),
                        show(actual),
                    ));
                }
                Outcome::ParseError(e) => {
                    n += 1;
                    out.push_str(&format!(
                        "### PARSE-ERROR #{n} [{}] {}\n{}\n\n",
                        r.source_file,
                        r.label.as_deref().unwrap_or(""),
                        e,
                    ));
                }
                _ => {}
            }
        }
        let header = format!("{n} failures/parse-errors\n\n");
        let _ = fs::write(path, format!("{header}{out}"));
        println!("wrote {n} failure blocks to {path}");
    }
}

fn show(s: &str) -> String {
    // make control chars / trailing space visible
    s.replace('\t', "\\t")
}

fn category_name(c: Category) -> &'static str {
    match c {
        Category::CommonMark => "commonmark",
        Category::Gfm => "gfm",
    }
}
