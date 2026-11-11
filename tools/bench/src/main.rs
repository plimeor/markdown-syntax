//! Native benchmark of `markdown-syntax` against pulldown-cmark, comrak, and
//! markdown-rs, over the corpus in `tools/bench/corpus/out/` (generate it with
//! `node tools/bench/corpus/gen.mjs`).
//!
//! Run from `tools/bench/`:
//!
//! ```text
//! cargo run --release                          # every corpus and operation
//! cargo run --release -- --filter syn-tables   # cases whose key holds a substring
//! cargo run --release -- --filter ours.parse,comrak.parse
//! cargo run --release -- --time 2              # seconds of samples per case (default 1)
//! cargo run --release -- --save baseline       # write results/baseline.json
//! cargo run --release -- --compare baseline    # print this run against results/baseline.json
//! cargo run --release -- --report out.md       # write the markdown tables of this run
//! cargo run --release -- --merge a,b --save c  # per case, keep the run with the lower median
//! cargo run --release -- --load c --report r.md  # tables from a saved run, without measuring
//! cargo run --release -- --list               # print the case keys
//! ```
//!
//! A case key is `<corpus>/<impl>.<op>`, e.g. `real-spec/ours.parse`. A name
//! given to `--save`, `--compare`, `--merge`, or `--load` is a file under
//! `results/` (with `.json` added), unless it holds a `/` or ends in `.json`.
//!
//! Each case warms up, picks a batch size so one sample takes at least
//! 200 µs, and collects samples for `--time` seconds (at least 10). Time per
//! operation is reported as the median and the minimum over samples. One
//! further call runs with the counting allocator on and records the
//! allocations (`alloc` and `realloc` calls), the bytes they requested, and
//! the peak of live bytes above the level when the call began. Every
//! operation includes dropping what it built.

mod report;

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering::Relaxed};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Counting allocator: off while timing, on for one call per case.

struct Counting;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

fn grow(by: isize) {
    let live = LIVE.fetch_add(by, Relaxed) + by;
    if live > PEAK.load(Relaxed) {
        PEAK.store(live, Relaxed);
    }
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
            grow(layout.size() as isize);
        }
        System.alloc(layout)
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(layout.size(), Relaxed);
            grow(layout.size() as isize);
        }
        System.alloc_zeroed(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if COUNTING.load(Relaxed) {
            LIVE.fetch_sub(layout.size() as isize, Relaxed);
        }
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Relaxed) {
            ALLOCS.fetch_add(1, Relaxed);
            BYTES.fetch_add(new_size, Relaxed);
            grow(new_size as isize - layout.size() as isize);
        }
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count_allocations(f: &mut dyn FnMut() -> usize) -> (u64, u64, u64) {
    ALLOCS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    LIVE.store(0, Relaxed);
    PEAK.store(0, Relaxed);
    COUNTING.store(true, Relaxed);
    black_box(f());
    COUNTING.store(false, Relaxed);
    (
        ALLOCS.load(Relaxed) as u64,
        BYTES.load(Relaxed) as u64,
        PEAK.load(Relaxed).max(0) as u64,
    )
}

// ---------------------------------------------------------------------------
// Results.

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    pub corpus: String,
    /// The bytes throughput is computed over: the corpus file, or for
    /// `ours.inline_largest` the leaf text it parses.
    pub bytes: u64,
    pub imp: String,
    pub op: String,
    pub median_ns: f64,
    pub min_ns: f64,
    pub p90_ns: f64,
    pub samples: u64,
    pub batch: u64,
    pub allocs: u64,
    pub alloc_bytes: u64,
    pub peak_bytes: u64,
    /// With `--merge`: the largest median over the runs merged divided by
    /// the smallest. 1 for a single run.
    #[serde(default = "one")]
    pub run_spread: f64,
}

fn one() -> f64 {
    1.0
}

impl Row {
    pub fn key(&self) -> String {
        format!("{}/{}.{}", self.corpus, self.imp, self.op)
    }
    pub fn mbps(&self) -> f64 {
        self.bytes as f64 / self.median_ns * 1e3
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Run {
    pub environment: Environment,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Environment {
    pub cpu: String,
    pub rustc: String,
    pub crates: String,
    pub time_per_case_s: f64,
    pub started: String,
}

// ---------------------------------------------------------------------------
// Harness.

struct Harness {
    filters: Vec<String>,
    time: Duration,
    list: bool,
    rows: Vec<Row>,
}

impl Harness {
    fn wants(&self, key: &str) -> bool {
        self.filters.is_empty() || self.filters.iter().any(|f| key.contains(f.as_str()))
    }

    fn bench(
        &mut self,
        corpus: &str,
        bytes: usize,
        imp: &str,
        op: &str,
        mut f: impl FnMut() -> usize,
    ) {
        let key = format!("{corpus}/{imp}.{op}");
        if !self.wants(&key) {
            return;
        }
        if self.list {
            println!("{key}");
            return;
        }
        // Warm up for a tenth of the sample time, at least one call.
        let warm = self.time / 10;
        let start = Instant::now();
        let mut calls = 0u64;
        while calls == 0 || start.elapsed() < warm {
            black_box(f());
            calls += 1;
        }
        let per_call = start.elapsed().as_nanos() as f64 / calls as f64;
        let batch = ((200_000.0 / per_call).ceil() as u64).max(1);

        let mut samples = Vec::new();
        let start = Instant::now();
        while samples.len() < 10 || (start.elapsed() < self.time && samples.len() < 5000) {
            let t = Instant::now();
            for _ in 0..batch {
                black_box(f());
            }
            samples.push(t.elapsed().as_nanos() as f64 / batch as f64);
        }
        samples.sort_by(f64::total_cmp);
        let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
        let (allocs, alloc_bytes, peak_bytes) = count_allocations(&mut f);
        let row = Row {
            corpus: corpus.to_owned(),
            bytes: bytes as u64,
            imp: imp.to_owned(),
            op: op.to_owned(),
            median_ns: at(0.5),
            min_ns: samples[0],
            p90_ns: at(0.9),
            samples: samples.len() as u64,
            batch,
            allocs,
            alloc_bytes,
            peak_bytes,
            run_spread: 1.0,
        };
        println!(
            "{:<44} median {:>11} min {:>11} {:>8.1} MB/s  allocs {:>9}  bytes {:>11}  peak {:>11}",
            key,
            report::fmt_ns(row.median_ns),
            report::fmt_ns(row.min_ns),
            row.mbps(),
            allocs,
            alloc_bytes,
            peak_bytes
        );
        self.rows.push(row);
    }
}

// ---------------------------------------------------------------------------
// Peer configurations: GFM in each, plus the extensions this crate's one
// syntax also has where the peer offers them.

fn pulldown_options() -> pulldown_cmark::Options {
    use pulldown_cmark::Options;
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_WIKILINKS
}

fn comrak_options() -> comrak::Options<'static> {
    let mut o = comrak::Options::default();
    o.extension.strikethrough = true;
    o.extension.table = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.footnotes = true;
    o.extension.inline_footnotes = true;
    o.extension.front_matter_delimiter = Some("---".to_owned());
    o.extension.alerts = true;
    o.extension.math_dollars = true;
    o.extension.wikilinks_title_after_pipe = true;
    o
}

fn mdrs_parse_options() -> markdown::ParseOptions {
    let mut o = markdown::ParseOptions::gfm();
    o.constructs.frontmatter = true;
    o.constructs.math_flow = true;
    o.constructs.math_text = true;
    o
}

fn mdrs_options() -> markdown::Options {
    markdown::Options {
        parse: mdrs_parse_options(),
        compile: markdown::CompileOptions::gfm(),
    }
}

// ---------------------------------------------------------------------------
// Operations per corpus file.

fn run_corpus(h: &mut Harness, corpus: &str, src: &str) {
    use markdown_syntax::{parse, parse_inline, Definitions};
    let n = src.len();

    // --- ours
    h.bench(corpus, n, "ours", "parse", || {
        let out = parse(src);
        out.document.children.len() + out.diagnostics.len()
    });
    h.bench(corpus, n, "ours", "editor_parse", || {
        let out = parse(src);
        out.document.children.len() + out.document.leaf_sources().len()
    });
    h.bench(corpus, n, "ours", "html", || {
        parse(src).document.to_html().map_or(0, |s| s.len())
    });
    {
        let doc = parse(src).document;
        h.bench(corpus, n, "ours", "leaf_sources", || {
            doc.leaf_sources().len()
        });
        h.bench(corpus, n, "ours", "validate", || doc.validate().len());
        h.bench(corpus, n, "ours", "definitions", || {
            black_box(Definitions::of(&doc));
            1
        });
        h.bench(corpus, n, "ours", "writeback", || {
            doc.to_markdown().map_or(0, |s| s.len())
        });
        h.bench(corpus, n, "ours", "render", || {
            doc.to_html().map_or(0, |s| s.len())
        });
        let bare = doc.clone().without_source();
        h.bench(corpus, n, "ours", "canonical", || {
            bare.to_markdown().map_or(0, |s| s.len())
        });

        let leaves = doc.leaf_sources();
        let defs = Definitions::of(&doc);
        // A corpus without leaves (syn-code) has no inline work to time.
        if !leaves.is_empty() {
            h.bench(corpus, n, "ours", "inline_leaves", || {
                leaves
                    .iter()
                    .map(|(_, s)| parse_inline(&s.text, &defs).inlines.len())
                    .sum()
            });
        }
        if let Some((_, largest)) = leaves.iter().max_by_key(|(_, s)| s.text.len()) {
            let text = largest.text.as_str();
            h.bench(corpus, text.len(), "ours", "inline_largest", || {
                parse_inline(text, &defs).tokens.len()
            });
        }
    }

    // --- pulldown-cmark: a streaming pull parser, no tree.
    let popts = pulldown_options();
    h.bench(corpus, n, "pulldown", "events", || {
        let mut k = 0;
        for event in pulldown_cmark::Parser::new_ext(src, popts) {
            black_box(&event);
            k += 1;
        }
        k
    });
    h.bench(corpus, n, "pulldown", "html", || {
        let mut out = String::new();
        pulldown_cmark::html::push_html(&mut out, pulldown_cmark::Parser::new_ext(src, popts));
        out.len()
    });

    // --- comrak: arena AST.
    let copts = comrak_options();
    h.bench(corpus, n, "comrak", "parse", || {
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, src, &copts);
        black_box(root);
        arena.len()
    });
    h.bench(corpus, n, "comrak", "html", || {
        comrak::markdown_to_html(src, &copts).len()
    });
    {
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, src, &copts);
        h.bench(corpus, n, "comrak", "commonmark", || {
            let mut out = String::new();
            comrak::format_commonmark(root, &copts, &mut out).unwrap();
            out.len()
        });
        h.bench(corpus, n, "comrak", "render", || {
            let mut out = String::new();
            comrak::format_html(root, &copts, &mut out).unwrap();
            out.len()
        });
    }

    // --- markdown-rs: mdast.
    let mpopts = mdrs_parse_options();
    let mopts = mdrs_options();
    h.bench(corpus, n, "mdrs", "mdast", || {
        markdown::to_mdast(src, &mpopts).map_or(0, |_| 1)
    });
    h.bench(corpus, n, "mdrs", "html", || {
        markdown::to_html_with_options(src, &mopts).map_or(0, |s| s.len())
    });
}

/// Checks, once per corpus, that the operations do what they are timed as
/// doing: an unedited write-back is the source, and each output is non-empty.
fn sanity(corpus: &str, src: &str) {
    let out = markdown_syntax::parse(src);
    match out.document.to_markdown() {
        Ok(s) if s == src => {}
        Ok(_) => eprintln!("warning: {corpus}: unedited write-back differs from the source"),
        Err(e) => eprintln!("warning: {corpus}: to_markdown failed: {e:?}"),
    }
    if let Err(e) = out.document.to_html() {
        eprintln!("warning: {corpus}: to_html failed: {e:?}");
    }
    if let Err(e) = out.document.clone().without_source().to_markdown() {
        eprintln!("warning: {corpus}: canonical to_markdown failed: {e:?}");
    }
    if let Err(e) = markdown::to_mdast(src, &mdrs_parse_options()) {
        eprintln!("warning: {corpus}: markdown-rs to_mdast failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// CLI.

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn results_path(name: &str) -> PathBuf {
    if name.contains('/') || name.ends_with(".json") {
        PathBuf::from(name)
    } else {
        manifest_dir().join("results").join(format!("{name}.json"))
    }
}

fn load(name: &str) -> Run {
    let path = results_path(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()))
}

fn save(run: &Run, name: &str) {
    let path = results_path(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(&path, serde_json::to_string_pretty(run).unwrap()).unwrap();
    eprintln!("saved {}", path.display());
}

fn command(cmd: &str, args: &[&str]) -> String {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn environment(time: Duration) -> Environment {
    Environment {
        cpu: command("sysctl", &["-n", "machdep.cpu.brand_string"]),
        rustc: command("rustc", &["--version"]),
        crates: format!(
            "markdown-syntax {}, pulldown-cmark 0.13.5, comrak 0.59.0, markdown 1.0.0",
            markdown_syntax_version()
        ),
        time_per_case_s: time.as_secs_f64(),
        started: command("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
    }
}

fn markdown_syntax_version() -> String {
    let toml = std::fs::read_to_string(manifest_dir().join("../../Cargo.toml")).unwrap_or_default();
    toml.lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .map(|v| v.trim_end_matches('"').to_owned())
        .unwrap_or_else(|| "?".to_owned())
}

/// Asks the scheduler to keep this thread on the performance cores.
#[cfg(target_os = "macos")]
fn prefer_performance_cores() {
    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    const QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
    unsafe {
        pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn prefer_performance_cores() {}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut filters = Vec::new();
    let mut time = Duration::from_secs(1);
    let mut save_to = None;
    let mut compare_to = None;
    let mut report_to = None;
    let mut merge = None;
    let mut load_from = None;
    let mut list = false;
    let mut i = 0;
    let value = |i: &mut usize| {
        *i += 1;
        args.get(*i)
            .cloned()
            .unwrap_or_else(|| panic!("{} needs a value", args[*i - 1]))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--filter" | "-f" => filters.extend(value(&mut i).split(',').map(str::to_owned)),
            "--time" => {
                time = Duration::from_secs_f64(value(&mut i).parse().expect("--time seconds"))
            }
            "--save" => save_to = Some(value(&mut i)),
            "--compare" => compare_to = Some(value(&mut i)),
            "--report" => report_to = Some(value(&mut i)),
            "--merge" => merge = Some(value(&mut i)),
            "--load" => load_from = Some(value(&mut i)),
            "--list" => list = true,
            "--bench" => {} // passed by `cargo bench`
            other => panic!("unknown argument {other}; see the header of src/main.rs"),
        }
        i += 1;
    }

    let run = if let Some(names) = merge {
        let runs: Vec<Run> = names.split(',').map(load).collect();
        report::merge(runs)
    } else if let Some(name) = load_from {
        load(&name)
    } else {
        prefer_performance_cores();
        let dir = manifest_dir().join("corpus/out");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| {
                panic!(
                    "cannot read {}: {e}; run `node tools/bench/corpus/gen.mjs`",
                    dir.display()
                )
            })
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        files.sort_by_key(|p| report::corpus_order(&stem(p)));
        let mut h = Harness {
            filters,
            time,
            list,
            rows: Vec::new(),
        };
        for path in &files {
            let corpus = stem(path);
            let src = std::fs::read_to_string(path).unwrap();
            if !list {
                sanity(&corpus, &src);
            }
            run_corpus(&mut h, &corpus, &src);
        }
        if list {
            return;
        }
        Run {
            environment: environment(time),
            rows: h.rows,
        }
    };

    if let Some(name) = &save_to {
        save(&run, name);
    }
    if let Some(name) = &compare_to {
        report::compare(&load(name), &run);
    }
    if let Some(path) = &report_to {
        std::fs::write(path, report::markdown(&run)).unwrap();
        eprintln!("wrote {path}");
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().unwrap().to_string_lossy().into_owned()
}
