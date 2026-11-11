// Benchmarks markdown-syntax-wasm (the WebAssembly binding and its JS
// wrapper, wasm/js/index.js) against JavaScript Markdown parsers, from the
// point of view of a web editor: what a parse, a write, and a keystroke cost
// in JavaScript.
//
//   cargo build -p markdown-syntax-wasm --target wasm32-unknown-unknown --profile wasm
//   cd tools/bench/js && npm install
//   node --expose-gc tools/bench/js/bench.mjs [options]
//
// Options:
//   --filter <regex>     run only the cases whose id matches (ids look like
//                        parse/mixed-100k/ours, write/real-spec/ours-untouched,
//                        key/100k/lezer-incremental, breakdown/..., size/...,
//                        memory/..., cold/..., wasm-split/...)
//   --sections <a,b>     parse,breakdown,wasm-split,write,keystroke,size,cold,memory
//                        (default: all)
//   --rounds <n>         timed rounds per case (default 3); the spread across
//                        round medians is reported as the case's noise
//   --time <ms>          minimum time per round (default 400)
//   --quick              --rounds 2 --time 150
//   --save <name>        write results/<name>.json
//   --merge              with --save: merge this run into an existing file,
//                        replacing only the cases this run measured
//   --compare <name>     compare this run against results/<name>.json
//   --diff <a> <b>       compare two saved results without running anything
//
// The module is the one cargo writes, or the one MARKDOWN_SYNTAX_WASM names.

import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { cpus, tmpdir } from "node:os";
import { basename, dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "../../..");
const CORPUS = join(ROOT, "tools/bench/corpus/out");
const RESULTS = join(ROOT, "tools/bench/results");
const WRAPPER = join(ROOT, "wasm/js/index.js");
const WASM = process.env.MARKDOWN_SYNTAX_WASM ?? join(ROOT, "target/wasm32-unknown-unknown/wasm/markdown_syntax_wasm.wasm");
const FRAME_MS = 1000 / 60;
const now = () => performance.now();
const gc = () => {
  if (typeof global.gc === "function") {
    global.gc();
    global.gc();
  }
};

// ---------------------------------------------------------------------------
// Arguments

function parseArgs(argv) {
  const args = { rounds: 3, time: 400, sections: null, filter: null, save: null, compare: null, diff: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => argv[++i];
    switch (a) {
      case "--filter":
        args.filter = new RegExp(next());
        break;
      case "--sections":
        args.sections = new Set(next().split(","));
        break;
      case "--rounds":
        args.rounds = Number(next());
        break;
      case "--time":
        args.time = Number(next());
        break;
      case "--quick":
        args.rounds = 2;
        args.time = 150;
        break;
      case "--save":
        args.save = next();
        break;
      case "--compare":
        args.compare = next();
        break;
      case "--merge":
        args.merge = true;
        break;
      case "--diff":
        args.diff = [next(), next()];
        break;
      case "--child-memory":
        args.childMemory = [next(), next()];
        break;
      case "--child-cold":
        args.childCold = [next(), next()];
        break;
      case "--child-profile":
        args.childProfile = [next(), Number(next())];
        break;
      default:
        throw new Error(`unknown argument ${a}`);
    }
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
const wants = (section) => !args.sections || args.sections.has(section);
const selected = (id) => !args.filter || args.filter.test(id);

// ---------------------------------------------------------------------------
// Subjects

/** Ours, through the published wrapper, and an instrumented copy of it. */
async function loadOurs() {
  const wasmBytes = await readFile(WASM);
  const { MarkdownSyntax } = await import(WRAPPER);
  const binding = await MarkdownSyntax.load(wasmBytes);
  // The same wrapper source with its internals exported, for the breakdown;
  // wasm/js/index.js itself is not touched.
  const source = await readFile(WRAPPER, "utf8");
  const internals = await import(
    `data:text/javascript;base64,${Buffer.from(`${source}\nexport { Reader, Writer, encodeNode };\n`).toString("base64")}`
  );
  const instrumented = await internals.MarkdownSyntax.load(wasmBytes);
  return { wasmBytes, MarkdownSyntax, binding, internals, instrumented };
}

/** The peers, each a parse function from text to its result. */
async function loadPeers() {
  const MarkdownIt = (await import("markdown-it")).default;
  const markdownIt = new MarkdownIt(); // "default" preset: CommonMark + tables + strikethrough
  const { marked } = await import("marked"); // gfm: true by default
  const commonmark = await import("commonmark");
  const commonmarkParser = new commonmark.Parser();
  const { fromMarkdown } = await import("mdast-util-from-markdown");
  const { gfm } = await import("micromark-extension-gfm");
  const { gfmFromMarkdown, gfmToMarkdown } = await import("mdast-util-gfm");
  const { toMarkdown } = await import("mdast-util-to-markdown");
  const lezerMarkdown = await import("@lezer/markdown");
  const { TreeFragment } = await import("@lezer/common");
  const lezer = lezerMarkdown.parser.configure(lezerMarkdown.GFM);
  const markdownWasm = (await import("markdown-wasm/dist/markdown.node.js")).default;
  await markdownWasm.ready;
  const mdastOptions = { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] };
  const mdastWrite = { extensions: [gfmToMarkdown()] };
  return {
    "markdown-it": { label: "markdown-it parse (tokens)", parse: (t) => markdownIt.parse(t, {}) },
    marked: { label: "marked lexer (tokens)", parse: (t) => marked.lexer(t) },
    commonmark: { label: "commonmark.js Parser.parse (AST, no GFM)", parse: (t) => commonmarkParser.parse(t) },
    mdast: { label: "mdast-util-from-markdown + gfm (mdast)", parse: (t) => fromMarkdown(t, mdastOptions) },
    lezer: { label: "@lezer/markdown + GFM (Tree)", parse: (t) => lezer.parse(t) },
    "markdown-wasm": { label: "markdown-wasm (md4c) → HTML string, no tree", parse: (t) => markdownWasm.parse(t), html: true },
    _: { markdownIt, lezer, TreeFragment, fromMarkdown, mdastOptions, toMarkdown, mdastWrite },
  };
}

const PEER_NAMES = ["markdown-it", "marked", "commonmark", "mdast", "lezer", "markdown-wasm"];

/** One subject alone, for a fresh process: what an app imports to parse. */
const SINGLE_LOADERS = {
  async ours() {
    const { MarkdownSyntax } = await import(WRAPPER);
    const binding = await MarkdownSyntax.load(readFile(WASM));
    return (t) => binding.parse(t).free();
  },
  async "markdown-it"() {
    const md = new (await import("markdown-it")).default();
    return (t) => md.parse(t, {});
  },
  async marked() {
    const { marked } = await import("marked");
    return (t) => marked.lexer(t);
  },
  async commonmark() {
    const parser = new (await import("commonmark")).Parser();
    return (t) => parser.parse(t);
  },
  async mdast() {
    const { fromMarkdown } = await import("mdast-util-from-markdown");
    const { gfm } = await import("micromark-extension-gfm");
    const { gfmFromMarkdown } = await import("mdast-util-gfm");
    const options = { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] };
    return (t) => fromMarkdown(t, options);
  },
  async lezer() {
    const { parser, GFM } = await import("@lezer/markdown");
    const configured = parser.configure(GFM);
    return (t) => configured.parse(t);
  },
  async "markdown-wasm"() {
    const markdownWasm = (await import("markdown-wasm/dist/markdown.node.js")).default;
    await markdownWasm.ready;
    return (t) => markdownWasm.parse(t);
  },
};

// ---------------------------------------------------------------------------
// Timing

function stats(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
  const mean = sorted.reduce((s, x) => s + x, 0) / sorted.length;
  return { n: sorted.length, min: sorted[0], median: at(0.5), p90: at(0.9), mean };
}

/**
 * Times `fn` in `rounds` rounds of at least `time` ms (and 5 to 10 samples)
 * after a warmup, collecting garbage before each round. A call shorter than
 * 0.5 ms is batched, and its sample is the batch's mean.
 */
function measure(fn, { rounds = args.rounds, time = args.time } = {}) {
  gc();
  const warmStart = now();
  let calls = 0;
  while (calls < 3 || (now() - warmStart < Math.min(time, 300) && calls < 2000)) {
    fn();
    calls++;
  }
  const per = (now() - warmStart) / calls;
  const batch = per < 0.5 ? Math.ceil(0.5 / Math.max(per, 1e-4)) : 1;
  const minSamples = per > 100 ? 5 : 10;
  const all = [];
  const roundStats = [];
  for (let round = 0; round < rounds; round++) {
    gc();
    const samples = [];
    const started = now();
    while (samples.length < minSamples || (now() - started < time && samples.length < 5000)) {
      const a = now();
      for (let i = 0; i < batch; i++) fn();
      samples.push((now() - a) / batch);
    }
    roundStats.push(stats(samples));
    all.push(...samples);
  }
  const pooled = stats(all);
  const medians = roundStats.map((r) => r.median);
  const spread = (Math.max(...medians) - Math.min(...medians)) / pooled.median;
  return { ...pooled, batch, roundMedians: medians, spread };
}

/**
 * Times the stages of one pipeline together: `run` returns the timestamps
 * between its stages, and each stage's samples are its differences.
 */
function measureStages(names, run, { rounds = args.rounds, time = args.time } = {}) {
  gc();
  const warmStart = now();
  let calls = 0;
  while (calls < 3 || (now() - warmStart < 300 && calls < 2000)) {
    run();
    calls++;
  }
  const samples = names.map(() => []);
  const totals = [];
  const roundTotals = [];
  for (let round = 0; round < rounds; round++) {
    gc();
    const started = now();
    const before = totals.length;
    while (totals.length - before < 10 || (now() - started < time && totals.length - before < 5000)) {
      const marks = run();
      for (let i = 0; i < names.length; i++) samples[i].push(marks[i + 1] - marks[i]);
      totals.push(marks[names.length] - marks[0]);
    }
    roundTotals.push(stats(totals.slice(before)).median);
  }
  const total = stats(totals);
  return {
    total: { ...total, roundMedians: roundTotals, spread: (Math.max(...roundTotals) - Math.min(...roundTotals)) / total.median },
    stages: Object.fromEntries(names.map((name, i) => [name, stats(samples[i])])),
  };
}

// ---------------------------------------------------------------------------
// Output

const results = { meta: {}, cases: {}, breakdown: {}, wasmSplit: {}, sizes: {}, cold: {}, memory: {}, keystroke: {} };

function fmtMs(ms) {
  if (ms === undefined || ms === null || Number.isNaN(ms)) return "-";
  if (ms < 0.01) return `${(ms * 1000).toFixed(2)} µs`;
  if (ms < 1) return `${(ms * 1000).toFixed(0)} µs`;
  if (ms < 100) return `${ms.toFixed(2)} ms`;
  return `${ms.toFixed(0)} ms`;
}
const fmtPct = (x) => `${(x * 100).toFixed(1)}%`;
const fmtKB = (b) => (b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(2)} MB` : `${(b / 1024).toFixed(1)} KB`);

function table(headers, rows) {
  const widths = headers.map((h, i) => Math.max(h.length, ...rows.map((r) => String(r[i]).length)));
  const line = (cells) => `| ${cells.map((c, i) => String(c).padEnd(widths[i])).join(" | ")} |`;
  console.log(line(headers));
  console.log(`|${widths.map((w) => "-".repeat(w + 2)).join("|")}|`);
  for (const row of rows) console.log(line(row));
  console.log();
}

function record(id, meta, m) {
  results.cases[id] = { ...meta, median: m.median, min: m.min, p90: m.p90, mean: m.mean, n: m.n, batch: m.batch, roundMedians: m.roundMedians, spread: m.spread };
  return results.cases[id];
}

// ---------------------------------------------------------------------------
// Corpus

async function corpus() {
  if (!existsSync(CORPUS)) execFileSync(process.execPath, [join(ROOT, "tools/bench/corpus/gen.mjs")], { stdio: "inherit" });
  const names = (await readdir(CORPUS)).filter((n) => n.endsWith(".md")).sort();
  const order = (n) => (n.startsWith("mixed-") ? 0 : n.startsWith("real-") ? 1 : 2);
  const sizeOrder = { "1k": 0, "10k": 1, "100k": 2, "1m": 3 };
  names.sort((a, b) => order(a) - order(b) || (sizeOrder[a.slice(6, -3)] ?? 0) - (sizeOrder[b.slice(6, -3)] ?? 0) || a.localeCompare(b));
  return Promise.all(
    names.map(async (n) => {
      const text = await readFile(join(CORPUS, n), "utf8");
      return { name: n.replace(/\.md$/, ""), text, bytes: Buffer.byteLength(text), units: text.length };
    }),
  );
}

// ---------------------------------------------------------------------------
// 1. Full parse

function sectionParse(files, ours, peers) {
  console.log("## Full parse into a usable tree\n");
  const rows = [];
  for (const file of files) {
    const subjects = [["ours", () => ours.binding.parse(file.text).free()], ...PEER_NAMES.map((p) => [p, () => peers[p].parse(file.text)])];
    const measured = {};
    for (const [subject, fn] of subjects) {
      const id = `parse/${file.name}/${subject}`;
      if (!selected(id)) continue;
      const m = measure(fn);
      measured[subject] = record(id, { group: "parse", file: file.name, subject, bytes: file.bytes, mbps: file.bytes / 1e6 / (m.median / 1000) }, m);
    }
    if (Object.keys(measured).length === 0) continue;
    const treePeers = PEER_NAMES.filter((p) => p !== "markdown-wasm" && measured[p]);
    const fastest = treePeers.reduce((best, p) => (!best || measured[p].median < measured[best].median ? p : best), null);
    for (const [subject, c] of Object.entries(measured)) {
      c.ratioVsFastestTreePeer = fastest ? c.median / measured[fastest].median : null;
      rows.push([file.name, subject, fmtKB(file.bytes), fmtMs(c.median), fmtMs(c.min), (c.mbps).toFixed(1), fmtPct(c.spread), c.ratioVsFastestTreePeer ? `${c.ratioVsFastestTreePeer.toFixed(2)}× (${fastest})` : "-"]);
    }
  }
  table(["file", "subject", "size", "median", "min", "MB/s", "spread", "vs fastest tree peer"], rows);
}

// ---------------------------------------------------------------------------
// 2. Our pipeline, stage by stage

function sectionBreakdown(files, ours) {
  console.log("## Our parse pipeline, stage by stage\n");
  // The stages of `MarkdownSyntax.parse`, step by step, on the instrumented
  // copy of the wrapper: the same code, with its Reader exported.
  const { internals, instrumented } = ours;
  const { Reader, ParsedDocument } = internals;
  const exports = instrumented.call((e) => e);
  const encoder = new TextEncoder();
  const rows = [];
  const names = ["encode", "copyIn", "wasm", "view", "readerInit", "tree", "diagnosticsLeaves", "wrap", "free"];
  for (const file of files) {
    const id = `breakdown/${file.name}`;
    if (!selected(id)) continue;
    const { text } = file;
    const run = () => {
      const t0 = now();
      const bytes = encoder.encode(String(text));
      const t1 = now();
      const pointer = exports.buffer_alloc(bytes.length);
      new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
      const t2 = now();
      const handle = exports.parse(pointer, bytes.length);
      const t3 = now();
      exports.buffer_free(pointer, bytes.length);
      const view = instrumented.view();
      const t4 = now();
      const reader = new Reader(view);
      const t5 = now();
      const tree = reader.node();
      const t6 = now();
      const diagnostics = reader.diagnostics();
      const leafSources = reader.leafSources();
      const t7 = now();
      const document = new ParsedDocument(instrumented, handle, String(text), tree, diagnostics, leafSources);
      const t8 = now();
      document.free();
      const t9 = now();
      return [t0, t1, t2, t3, t4, t5, t6, t7, t8, t9];
    };
    const m = measureStages(names, run);
    // The message of one parse, for its shape.
    const result = (() => {
      const handle = instrumented.withBytes(encoder.encode(text), (p, l) => exports.parse(p, l));
      const r = instrumented.result();
      exports.document_free(handle);
      return r;
    })();
    const [wordCount, stringCount] = new Uint32Array(result.buffer, 0, 2);
    const textStart = 8 + 4 * wordCount + 8 * stringCount;
    const doc = instrumented.parse(text);
    const leafCount = doc.leafSources.length;
    doc.free();
    const wire = { bytes: result.length, words: wordCount, strings: stringCount, textBytes: result.length - textStart, leafSources: leafCount };
    const total = m.total.median;
    const sum = Object.values(m.stages).reduce((s, x) => s + x.median, 0);
    results.breakdown[file.name] = { bytes: file.bytes, wire, total: m.total, stages: m.stages };
    const st = m.stages;
    const wasmSide = st.wasm.median;
    const transfer = st.encode.median + st.copyIn.median + st.view.median;
    const decode = st.readerInit.median + st.tree.median + st.diagnosticsLeaves.median + st.wrap.median;
    rows.push([
      file.name,
      fmtMs(total),
      `${fmtMs(wasmSide)} (${fmtPct(wasmSide / sum)})`,
      `${fmtMs(transfer)} (${fmtPct(transfer / sum)})`,
      `${fmtMs(decode)} (${fmtPct(decode / sum)})`,
      `${fmtMs(st.readerInit.median)} / ${fmtMs(st.tree.median)} / ${fmtMs(st.diagnosticsLeaves.median)}`,
      `${fmtMs(st.free.median)} (${fmtPct(st.free.median / sum)})`,
      `${(wire.bytes / file.bytes).toFixed(2)}×`,
      fmtPct(m.total.spread),
    ]);
  }
  table(["file", "total", "wasm (parse+encode)", "transfer (TextEncoder+copy in)", "JS decode", "readerInit / tree / diag+leaves", "free", "wire/source", "spread"], rows);
}

// ---------------------------------------------------------------------------
// 2b. Inside wasm: CPU profile of the `parse` export, split by top-level call

// Each sample counts for the first frame under the export that a rule names;
// `parse_with_leaf_contents` names none, so its samples count for what it calls.
const SPLIT_RULES = [
  ["core parse (read_source)", /markdown_syntax5parse11read_source|markdown_syntax5parse5parse17|markdown_syntax5parse6blocks13read_document/],
  ["core leaf sources (leaf_sources_of)", /leaf_sources_of|find_leaf_sources|LeafContents|markdown_syntax6leaves/],
  ["wire: leaf source maps (write_inline_source)", /write_inline_source|tabulate/],
  ["wire: tree + diagnostics (Writer)", /4wire6Writer/],
  ["wire: UTF-16 offset table (OffsetTable)", /OffsetTable|utf16_offsets/],
  ["drop temporaries", /drop_in_place/],
];

async function childProfile(fileName, iterations) {
  const wasmBytes = await readFile(WASM);
  const { exports } = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {});
  const bytes = await readFile(join(CORPUS, `${fileName}.md`));
  const pointer = exports.buffer_alloc(bytes.length);
  new Uint8Array(exports.memory.buffer, pointer, bytes.length).set(bytes);
  for (let i = 0; i < iterations; i++) exports.document_free(exports.parse(pointer, bytes.length));
}

async function sectionWasmSplit(files) {
  console.log("## Inside wasm: where `parse` spends its time (CPU profile, 100 µs sampling)\n");
  const rows = [];
  for (const file of files) {
    const id = `wasm-split/${file.name}`;
    if (!selected(id)) continue;
    const dir = join(tmpdir(), `md-bench-prof-${process.pid}-${file.name}`);
    await rm(dir, { recursive: true, force: true });
    const iterations = Math.max(20, Math.round(3e7 / file.bytes));
    spawnSync(process.execPath, ["--cpu-prof", `--cpu-prof-dir=${dir}`, "--cpu-prof-interval", "100", fileURLToPath(import.meta.url), "--child-profile", file.name, String(iterations)], { stdio: "inherit" });
    const [profileName] = await readdir(dir);
    const profile = JSON.parse(await readFile(join(dir, profileName), "utf8"));
    await rm(dir, { recursive: true, force: true });
    const byId = new Map(profile.nodes.map((n) => [n.id, n]));
    const parent = new Map();
    for (const n of profile.nodes) for (const c of n.children ?? []) parent.set(c, n.id);
    const buckets = new Map();
    let inParse = 0;
    let inFree = 0;
    for (let i = 0; i < profile.samples.length; i++) {
      const dt = profile.timeDeltas[i + 1] ?? 0;
      const stack = [];
      for (let at = profile.samples[i]; at !== undefined; at = parent.get(at)) stack.push(byId.get(at).callFrame.functionName);
      stack.reverse();
      const exportAt = stack.findIndex((f) => f === "parse" || f === "document_free");
      if (exportAt < 0) continue;
      if (stack[exportAt] === "document_free") {
        inFree += dt;
        continue;
      }
      inParse += dt;
      let bucket = "other (allocator, glue)";
      // The binding writes inside the core parse's callback: a frame of the
      // writer or of the leaf walk counts for its own part wherever it is.
      const below = stack.slice(exportAt + 1);
      const inner = [2, 3, 1, 4].map((i) => SPLIT_RULES[i]).find(([, re]) => below.some((frame) => re.test(frame)));
      for (const frame of inner ? [] : stack.slice(exportAt + 1)) {
        const rule = SPLIT_RULES.find(([, re]) => re.test(frame));
        if (rule) {
          bucket = rule[0];
          break;
        }
      }
      if (inner) bucket = inner[0];
      buckets.set(bucket, (buckets.get(bucket) ?? 0) + dt);
    }
    const split = Object.fromEntries([...buckets].sort((a, b) => b[1] - a[1]).map(([k, v]) => [k, v / inParse]));
    results.wasmSplit[file.name] = { iterations, parseShare: split, freeVsParse: inFree / inParse };
    for (const [k, v] of Object.entries(split)) rows.push([file.name, k, fmtPct(v)]);
    rows.push([file.name, "(document_free, relative to parse)", fmtPct(inFree / inParse)]);
  }
  table(["file", "part of the parse export", "share"], rows);
}

// ---------------------------------------------------------------------------
// 3. Write-back

/**
 * The tree with the text of its middle leaf edited, and the blocks
 * `toMarkdownBlocks` takes for the same edit: the index of each top-level
 * block but the one the leaf lies in.
 */
function editedParagraphTree(tree) {
  const clone = structuredClone(tree);
  const paragraphs = [];
  const walk = (blocks, top) => {
    for (const [index, block] of blocks.entries()) {
      const leaf = ["paragraph", "heading", "tableCell"].includes(block.type);
      if (leaf && block.children.some((c) => c.type === "text")) paragraphs.push([block, top ?? index]);
      for (const key of ["children", "rows", "cells"]) if (!leaf && Array.isArray(block[key])) walk(block[key], top ?? index);
    }
  };
  walk(clone.children);
  if (paragraphs.length === 0) return null;
  const [paragraph, top] = paragraphs[Math.floor(paragraphs.length / 2)];
  paragraph.children.find((c) => c.type === "text").value += " edited";
  const blocks = clone.children.map((block, index) => (index === top ? block : index));
  return { tree: clone, blocks };
}

function everyLeafAsSource(document) {
  const saved = structuredClone(document.tree);
  for (const { path, source } of document.leafSources) {
    let node = saved;
    for (const index of path) node = (node.rows ?? node.cells ?? node.children ?? node.content?.blocks ?? node.content?.inlines)[index];
    if (node.type === "leafDirective" || node.type === "containerDirective") node.label = [];
    else node.children = [];
    node.source = source.text;
  }
  return saved;
}

const WRITE_VARIANTS = [
  "ours-untouched",
  "ours-edit-one-paragraph",
  "ours-blocks-untouched",
  "ours-blocks-edit-one-paragraph",
  "ours-every-leaf-as-source",
  "mdast-to-markdown",
];

function sectionWrite(files, ours, peers) {
  console.log("## Write-back (tree → Markdown)\n");
  const { toMarkdown, mdastWrite, fromMarkdown, mdastOptions } = peers._;
  const { internals, instrumented } = ours;
  const exports = instrumented.call((e) => e);
  const UTF8 = new TextDecoder("utf-8", { ignoreBOM: true });
  const rows = [];
  for (const file of files) {
    if (!["mixed-10k", "mixed-100k", "mixed-1m", "real-spec", "real-docs", "syn-tables", "syn-lists"].includes(file.name)) continue;
    const ids = [...WRITE_VARIANTS.map((v) => `write/${file.name}/${v}`), `write-breakdown/${file.name}`];
    if (!ids.some(selected)) continue;
    const doc = ours.binding.parse(file.text);
    const edited = editedParagraphTree(doc.tree);
    if (!edited) throw new Error(`no paragraph to edit in ${file.name}`);
    const editedOut = doc.toMarkdown(edited.tree);
    if (typeof editedOut !== "string" || !editedOut.includes(" edited")) throw new Error(`edited write failed on ${file.name}`);
    if (doc.toMarkdown() !== file.text) console.warn(`  note: untouched write of ${file.name} differs from its source`);
    const untouchedBlocks = doc.tree.children.map((_, index) => index);
    if (doc.toMarkdownBlocks(untouchedBlocks) !== doc.toMarkdown() || doc.toMarkdownBlocks(edited.blocks) !== editedOut) {
      throw new Error(`toMarkdownBlocks differs from toMarkdown on ${file.name}`);
    }
    const asSource = everyLeafAsSource(doc);
    const mdast = fromMarkdown(file.text, mdastOptions);
    const variants = [
      ["ours-untouched", "ours: untouched tree", () => doc.toMarkdown()],
      ["ours-edit-one-paragraph", "ours: one leaf's text edited (middle paragraph, or cell)", () => doc.toMarkdown(edited.tree)],
      ["ours-blocks-untouched", "ours: toMarkdownBlocks, every block an index", () => doc.toMarkdownBlocks(untouchedBlocks)],
      [
        "ours-blocks-edit-one-paragraph",
        "ours: toMarkdownBlocks, the edited leaf's top-level block sent",
        () => doc.toMarkdownBlocks(edited.blocks),
      ],
      ["ours-every-leaf-as-source", "ours: every leaf given as source", () => doc.toMarkdown(asSource)],
      ["mdast-to-markdown", "mdast-util-to-markdown + gfm (canonical)", () => toMarkdown(mdast, mdastWrite)],
    ];
    for (const [subject, label, fn] of variants) {
      const id = `write/${file.name}/${subject}`;
      if (!selected(id)) continue;
      const m = measure(fn);
      const c = record(id, { group: "write", file: file.name, subject, label, bytes: file.bytes, mbps: file.bytes / 1e6 / (m.median / 1000) }, m);
      rows.push([file.name, label, fmtMs(c.median), fmtMs(c.min), c.mbps.toFixed(1), fmtPct(c.spread)]);
    }
    // Where an untouched write spends its time.
    const id = `write-breakdown/${file.name}`;
    if (selected(id)) {
      const handle = instrumented.withBytes(new TextEncoder().encode(file.text), (p, l) => exports.parse(p, l));
      for (const [variant, tree] of [["untouched", doc.tree], ["edit-one-paragraph", edited.tree]]) {
        const m = measureStages(["jsEncode", "copyIn", "wasm", "copyOut", "utf8Decode"], () => {
          const t0 = now();
          const writer = new internals.Writer();
          internals.encodeNode(writer, tree, [], (n) => n === 0, "the document");
          const message = writer.finish();
          const t1 = now();
          const pointer = exports.buffer_alloc(message.length);
          new Uint8Array(exports.memory.buffer, pointer, message.length).set(message);
          const t2 = now();
          const status = exports.to_markdown(handle, pointer, message.length, 0, 1, 0, 0, 0);
          const t3 = now();
          exports.buffer_free(pointer, message.length);
          const result = instrumented.result();
          const t4 = now();
          if (status !== 0) throw new Error(`to_markdown status ${status}`);
          UTF8.decode(result);
          const t5 = now();
          return [t0, t1, t2, t3, t4, t5];
        });
        results.breakdown[`write:${file.name}:${variant}`] = { total: m.total, stages: m.stages };
      }
      exports.document_free(handle);
    }
    doc.free();
  }
  table(["file", "variant", "median", "min", "MB/s", "spread"], rows);
  const brows = Object.entries(results.breakdown)
    .filter(([k]) => k.startsWith("write:"))
    .map(([k, v]) => {
      const sum = Object.values(v.stages).reduce((s, x) => s + x.median, 0);
      return [k.slice(6), fmtMs(v.total.median), ...["jsEncode", "copyIn", "wasm", "copyOut", "utf8Decode"].map((s) => `${fmtMs(v.stages[s].median)} (${fmtPct(v.stages[s].median / sum)})`)];
    });
  if (brows.length) table(["file:variant", "total", "JS encode tree", "copy in", "wasm decode+write", "copy out", "UTF-8 decode"], brows);
}

// ---------------------------------------------------------------------------
// 4. Keystroke

/** The doc with one character typed in the middle of a paragraph near its middle. */
function keystroke(binding, text) {
  const doc = binding.parse(text);
  const middle = text.length / 2;
  let best = null;
  for (const leaf of doc.leafSources) {
    if (leaf.path.length !== 1) continue;
    const block = doc.tree.children[leaf.path[0]];
    if (block.type !== "paragraph" || leaf.source.text.length < 40) continue;
    const distance = Math.abs(block.span[0] - middle);
    if (!best || distance < best.distance) best = { leaf, block, distance };
  }
  const { leaf, block } = best;
  let at = Math.floor(leaf.source.text.length / 2);
  const markup = /[\s*_`[\]()<>!\\~#|&:$=^@{}]/;
  while (at < leaf.source.text.length - 1 && (markup.test(leaf.source.text[at]) || markup.test(leaf.source.text[at - 1]))) at++;
  const sourceAt = leaf.source.toSource(at);
  const newText = `${text.slice(0, sourceAt)}x${text.slice(sourceAt)}`;
  const paragraphText = `${leaf.source.text.slice(0, at)}x${leaf.source.text.slice(at)}`;
  const definitions = doc.definitions;
  doc.free();
  return { newText, sourceAt, paragraphText, definitions, paragraphSpan: block.span };
}

function sectionKeystroke(files, ours, peers) {
  console.log("## Keystroke: type one character in the middle and reparse\n");
  const { lezer, TreeFragment, markdownIt, fromMarkdown, mdastOptions } = peers._;
  const rows = [];
  for (const [label, name] of [["10k", "mixed-10k"], ["100k", "mixed-100k"], ["1m", "mixed-1m"]]) {
    const file = files.find((f) => f.name === name);
    if (!file) continue;
    const k = keystroke(ours.binding, file.text);
    const baseTree = lezer.parse(file.text);
    const baseFragments = TreeFragment.addTree(baseTree);
    const change = [{ fromA: k.sourceAt, toA: k.sourceAt, fromB: k.sourceAt, toB: k.sourceAt + 1 }];
    // Incremental and full lezer parses must agree.
    const incremental = lezer.parse(k.newText, TreeFragment.applyChanges(baseFragments, change));
    if (incremental.toString() !== lezer.parse(k.newText).toString()) throw new Error(`lezer incremental parse differs on ${name}`);
    // Ours: the edited paragraph's inline parse must match the paragraph a full parse gives.
    const full = ours.binding.parse(k.newText);
    const local = ours.binding.parseInline(k.paragraphText, k.definitions);
    const fullParagraph = full.tree.children.find((b) => b.span && b.span[0] === k.paragraphSpan[0]);
    const strip = (nodes) => JSON.stringify(nodes, (key, v) => (key === "span" ? undefined : v));
    if (!fullParagraph || strip(fullParagraph.children) !== strip(local.inlines)) console.warn(`  note: parseInline of the edited paragraph differs from the full parse on ${name}`);
    full.free();
    const variants = [
      ["ours-full", "ours: full parse (tree+leafSources) + free", () => ours.binding.parse(k.newText).free()],
      ["ours-parseInline", `ours: parseInline of the edited paragraph (${k.paragraphText.length} chars)`, () => ours.binding.parseInline(k.paragraphText, k.definitions)],
      ["ours-parseInline-no-definitions", "ours: parseInline of the edited paragraph, no definitions passed", () => ours.binding.parseInline(k.paragraphText, [])],
      ["lezer-incremental", "@lezer/markdown incremental (TreeFragment)", () => lezer.parse(k.newText, TreeFragment.applyChanges(baseFragments, change))],
      ["lezer-full", "@lezer/markdown full parse", () => lezer.parse(k.newText)],
      ["markdown-it-full", "markdown-it full parse", () => markdownIt.parse(k.newText, {})],
      ["mdast-full", "mdast-util-from-markdown + gfm full parse", () => fromMarkdown(k.newText, mdastOptions)],
    ];
    for (const [subject, vlabel, fn] of variants) {
      const id = `key/${label}/${subject}`;
      if (!selected(id)) continue;
      const m = measure(fn);
      const c = record(id, { group: "keystroke", file: name, subject, label: vlabel, bytes: file.bytes, frameShare: m.median / FRAME_MS }, m);
      rows.push([label, vlabel, fmtMs(c.median), fmtMs(c.min), fmtMs(c.p90), fmtPct(c.frameShare), c.median <= FRAME_MS ? "yes" : "NO", fmtPct(c.spread)]);
    }
  }
  table(["doc", "reparse", "median", "min", "p90", "of 16.7 ms frame", "fits", "spread"], rows);
}

// ---------------------------------------------------------------------------
// 5. Size and load

function stripCustomSections(bytes) {
  const out = [bytes.subarray(0, 8)];
  let at = 8;
  const leb = () => {
    let value = 0;
    let shift = 0;
    let byte;
    do {
      byte = bytes[at++];
      value += (byte & 0x7f) * 2 ** shift;
      shift += 7;
    } while (byte & 0x80);
    return value;
  };
  while (at < bytes.length) {
    const start = at;
    const id = bytes[at++];
    const size = leb();
    const end = at + size;
    if (id !== 0) out.push(bytes.subarray(start, end));
    at = end;
  }
  return Buffer.concat(out);
}

function sizes(buffer) {
  return {
    raw: buffer.length,
    gzip: gzipSync(buffer, { level: 9 }).length,
    brotli: brotliCompressSync(buffer, { params: { [zlibConstants.BROTLI_PARAM_QUALITY]: 11 } }).length,
  };
}

async function sectionSize(ours) {
  console.log("## Module size and load\n");
  const esbuild = await import("esbuild");
  const bundle = async (contents) => {
    const out = await esbuild.build({ stdin: { contents, resolveDir: HERE, loader: "js" }, bundle: true, minify: true, format: "esm", platform: "browser", write: false, logLevel: "silent" });
    return Buffer.from(out.outputFiles[0].contents);
  };
  const wasmRaw = ours.wasmBytes;
  const wasmStripped = stripCustomSections(wasmRaw);
  const wrapperMin = await bundle(`export * from ${JSON.stringify(WRAPPER)};`);
  const mdwasmDir = join(HERE, "node_modules/markdown-wasm/dist");
  const entries = {
    "ours: .wasm as built": sizes(wasmRaw),
    "ours: .wasm without custom sections (name, producers)": sizes(wasmStripped),
    "ours: index.js wrapper, minified": sizes(wrapperMin),
    "markdown-it (bundled, minified)": sizes(await bundle(`export { default } from "markdown-it";`)),
    "marked (bundled, minified)": sizes(await bundle(`export { marked, Lexer } from "marked";`)),
    "commonmark.js (bundled, minified)": sizes(await bundle(`export { Parser } from "commonmark";`)),
    "mdast-util-from-markdown + micromark gfm (bundled, minified)": sizes(await bundle(`export { fromMarkdown } from "mdast-util-from-markdown"; export { gfm } from "micromark-extension-gfm"; export { gfmFromMarkdown } from "mdast-util-gfm";`)),
    "mdast-util-to-markdown + gfm (bundled, minified)": sizes(await bundle(`export { toMarkdown } from "mdast-util-to-markdown"; export { gfmToMarkdown } from "mdast-util-gfm";`)),
    "@lezer/markdown + @lezer/common (bundled, minified)": sizes(await bundle(`export { parser, GFM } from "@lezer/markdown"; export { TreeFragment } from "@lezer/common";`)),
    "markdown-wasm: markdown.wasm": sizes(await readFile(join(mdwasmDir, "markdown.wasm"))),
    "markdown-wasm: markdown.es.js": sizes(await readFile(join(mdwasmDir, "markdown.es.js"))),
  };
  for (const [k, v] of Object.entries(entries)) if (selected(`size/${k}`)) results.sizes[k] = v;
  table(["module", "raw", "gzip -9", "brotli 11"], Object.entries(results.sizes).map(([k, v]) => [k, fmtKB(v.raw), fmtKB(v.gzip), fmtKB(v.brotli)]));

  if (!selected("load/ours")) return;
  // Load: compile + instantiate from bytes, as MarkdownSyntax.load does, then
  // a first parse on the fresh instance against the steady state.
  const text = await readFile(join(CORPUS, "mixed-100k.md"), "utf8");
  const loads = [];
  const compiles = [];
  const firstParses = [];
  const secondParses = [];
  // V8 reuses a compiled module for identical bytes within an isolate, so
  // each load gets bytes made unique by a trailing custom section.
  const unique = (tag) => {
    const name = Buffer.from(`bench-${process.pid}-${tag}-${Math.random()}`);
    return Buffer.concat([wasmRaw, Buffer.from([0, name.length + 1, name.length]), name]);
  };
  for (let i = 0; i < 15; i++) {
    gc();
    let t = now();
    await WebAssembly.compile(unique(`c${i}`));
    compiles.push(now() - t);
    const bytes = unique(`l${i}`);
    t = now();
    const binding = await ours.MarkdownSyntax.load(bytes);
    loads.push(now() - t);
    t = now();
    binding.parse(text).free();
    firstParses.push(now() - t);
    t = now();
    binding.parse(text).free();
    secondParses.push(now() - t);
  }
  const steady = measure(() => ours.binding.parse(text).free(), { rounds: 1 });
  results.sizes.load = { compile: stats(compiles), load: stats(loads), firstParse100k: stats(firstParses), secondParse100k: stats(secondParses), steadyParse100k: steady.median };
  const L = results.sizes.load;
  table(
    ["step", "median", "min"],
    [
      ["WebAssembly.compile(bytes)", fmtMs(L.compile.median), fmtMs(L.compile.min)],
      ["MarkdownSyntax.load(bytes) (compile + instantiate)", fmtMs(L.load.median), fmtMs(L.load.min)],
      ["first parse of mixed-100k on a fresh instance", fmtMs(L.firstParse100k.median), fmtMs(L.firstParse100k.min)],
      ["second parse on that instance", fmtMs(L.secondParse100k.median), fmtMs(L.secondParse100k.min)],
      ["steady-state parse (warm, long-lived instance)", fmtMs(L.steadyParse100k), "-"],
    ],
  );
}

// ---------------------------------------------------------------------------
// 5b. Cold start in a fresh process: import + first parse

async function childCold(subject, fileName) {
  const text = await readFile(join(CORPUS, `${fileName}.md`), "utf8");
  const t0 = now();
  const parse = await SINGLE_LOADERS[subject]();
  const t1 = now();
  parse(text);
  const t2 = now();
  parse(text);
  const t3 = now();
  process.stdout.write(JSON.stringify({ load: t1 - t0, first: t2 - t1, second: t3 - t2 }));
}

async function sectionCold() {
  console.log("## Cold start in a fresh process (load + first parse of mixed-100k)\n");
  console.log("Each run is a fresh node process that imports one subject (ours: index.js + read + compile + instantiate the .wasm).\n");
  const rows = [];
  for (const subject of ["ours", ...PEER_NAMES]) {
    const id = `cold/${subject}`;
    if (!selected(id)) continue;
    const runs = [];
    for (let i = 0; i < 5; i++) {
      const out = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "--child-cold", subject, "mixed-100k"], { encoding: "utf8" });
      runs.push(JSON.parse(out.stdout));
    }
    const s = (k) => stats(runs.map((r) => r[k]));
    results.cold[subject] = { load: s("load"), first: s("first"), second: s("second") };
    rows.push([subject, fmtMs(s("load").median), fmtMs(s("first").median), fmtMs(s("second").median)]);
  }
  table(["subject", "load (median of 5)", "first parse", "second parse"], rows);
}

// ---------------------------------------------------------------------------
// 6. Memory

async function childMemory(subject, fileName) {
  const text = await readFile(join(CORPUS, `${fileName}.md`), "utf8");
  let parse;
  let wasmMemory = null;
  let ours;
  if (subject.startsWith("ours")) {
    ours = await loadOurs();
    wasmMemory = ours.binding.call((e) => e.memory);
    parse = (t) => ours.binding.parse(t);
  } else {
    const peers = await loadPeers();
    parse = peers[subject].parse;
  }
  // Warm the parser on a small input so lazily built tables are in the baseline.
  const warm = parse("# a\n\n*b* [c](d)\n\n| a |\n|---|\n| b |\n");
  if (warm?.free) warm.free();
  gc();
  const before = process.memoryUsage();
  const wasmBefore = wasmMemory?.buffer.byteLength ?? 0;
  globalThis.kept = parse(text);
  gc();
  const after = process.memoryUsage();
  const out = {
    heapUsed: after.heapUsed - before.heapUsed,
    external: after.external - before.external,
    arrayBuffers: after.arrayBuffers - before.arrayBuffers,
    rss: after.rss - before.rss,
  };
  if (wasmMemory) {
    out.wasmBefore = wasmBefore;
    out.wasmAfter = wasmMemory.buffer.byteLength;
    // What the JS tree alone holds: drop leafSources and measure again.
    globalThis.kept.leafSources = null;
    gc();
    out.heapWithoutLeafSources = process.memoryUsage().heapUsed - before.heapUsed;
    globalThis.kept.free();
    // Parse again into the grown memory: no further growth means the peak is reused.
    const again = ours.binding.parse(text);
    out.wasmAfterSecondParse = wasmMemory.buffer.byteLength;
    again.free();
    for (let i = 0; i < 18; i++) ours.binding.parse(text).free();
    out.wasmAfter20Parses = wasmMemory.buffer.byteLength;
  }
  process.stdout.write(JSON.stringify(out));
}

async function sectionMemory() {
  console.log("## Memory after parsing mixed-1m (fresh process each, after gc, result kept alive)\n");
  const rows = [];
  for (const subject of ["ours", ...PEER_NAMES]) {
    const id = `memory/${subject}`;
    if (!selected(id)) continue;
    const runs = [];
    for (let i = 0; i < 3; i++) {
      const out = spawnSync(process.execPath, ["--expose-gc", fileURLToPath(import.meta.url), "--child-memory", subject, "mixed-1m"], { encoding: "utf8" });
      if (out.status !== 0) throw new Error(out.stderr);
      runs.push(JSON.parse(out.stdout));
    }
    const med = (k) => (runs[0][k] === undefined ? undefined : stats(runs.map((r) => r[k])).median);
    const m = Object.fromEntries(Object.keys(runs[0]).map((k) => [k, med(k)]));
    m.runs = runs;
    results.memory[subject] = m;
    rows.push([
      subject,
      fmtKB(m.heapUsed),
      fmtKB(m.external),
      m.wasmAfter !== undefined ? `${fmtKB(m.wasmBefore)} → ${fmtKB(m.wasmAfter)} (after 2nd parse: ${fmtKB(m.wasmAfterSecondParse)}, after 20: ${fmtKB(m.wasmAfter20Parses)})` : "-",
      m.heapWithoutLeafSources !== undefined ? fmtKB(m.heapWithoutLeafSources) : "-",
    ]);
  }
  table(["subject", "JS heap used Δ", "external Δ", "wasm memory", "heap without leafSources"], rows);
}

// ---------------------------------------------------------------------------
// Compare

function compare(base, current, baseName, currentName) {
  console.log(`## Compare ${currentName} against ${baseName}\n`);
  const rows = [];
  for (const [id, now_] of Object.entries(current.cases)) {
    const old = base.cases[id];
    if (!old) continue;
    const ratio = now_.median / old.median;
    if (!Number.isFinite(ratio)) continue;
    // 6% is the p90 run-to-run change observed between two runs of the same
    // build (wasm-baseline.md); the within-run spread alone understates it.
    const noise = Math.max(old.spread ?? 0, now_.spread ?? 0, 0.06);
    const verdict = ratio < 1 - noise ? "faster" : ratio > 1 + noise ? "SLOWER" : "~";
    rows.push([id, fmtMs(old.median), fmtMs(now_.median), `${ratio.toFixed(3)}×`, `±${fmtPct(noise)}`, verdict]);
  }
  for (const [file, b] of Object.entries(current.breakdown)) {
    const old = base.breakdown?.[file];
    if (!old) continue;
    for (const stage of Object.keys(b.stages)) {
      if (!old.stages[stage]) continue;
      rows.push([`breakdown/${file}/${stage}`, fmtMs(old.stages[stage].median), fmtMs(b.stages[stage].median), `${(b.stages[stage].median / old.stages[stage].median).toFixed(3)}×`, "", ""]);
    }
  }
  table(["case", baseName, currentName, "ratio", "noise", "verdict"], rows);
  const sizeRows = Object.entries(current.sizes)
    .filter(([k]) => k.startsWith("ours") && base.sizes?.[k])
    .map(([k, v]) => [k, fmtKB(base.sizes[k].raw), fmtKB(v.raw), fmtKB(base.sizes[k].gzip), fmtKB(v.gzip)]);
  if (sizeRows.length) table(["module", `raw ${baseName}`, `raw ${currentName}`, `gzip ${baseName}`, `gzip ${currentName}`], sizeRows);
}

const loadSaved = async (name) => JSON.parse(await readFile(name.endsWith(".json") ? resolve(name) : join(RESULTS, `${name}.json`), "utf8"));

// ---------------------------------------------------------------------------
// Main

async function main() {
  if (args.childMemory) return childMemory(...args.childMemory);
  if (args.childCold) return childCold(...args.childCold);
  if (args.childProfile) return childProfile(...args.childProfile);
  if (args.diff) {
    const [a, b] = args.diff;
    return compare(await loadSaved(a), await loadSaved(b), basename(a), basename(b));
  }
  if (typeof global.gc !== "function") console.warn("warning: run with node --expose-gc so gc runs between cases\n");

  const files = await corpus();
  const ours = await loadOurs();
  const peers = await loadPeers();
  const pkg = (name) => JSON.parse(execFileSync(process.execPath, ["-p", `JSON.stringify(require(${JSON.stringify(join(HERE, "node_modules", name, "package.json"))}).version)`], { encoding: "utf8" }));
  let commit = null;
  try {
    commit = execFileSync("git", ["-C", ROOT, "rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
  } catch {}
  results.meta = {
    date: new Date().toISOString(),
    commit,
    cpu: (() => {
      try {
        return execFileSync("sysctl", ["-n", "machdep.cpu.brand_string"], { encoding: "utf8" }).trim();
      } catch {
        return cpus()[0]?.model;
      }
    })(),
    cores: cpus().length,
    node: process.version,
    v8: process.versions.v8,
    wasm: { path: relative(ROOT, WASM), bytes: ours.wasmBytes.length },
    packages: Object.fromEntries(["markdown-it", "marked", "commonmark", "micromark", "mdast-util-from-markdown", "micromark-extension-gfm", "mdast-util-gfm", "mdast-util-to-markdown", "@lezer/markdown", "@lezer/common", "markdown-wasm", "esbuild"].map((n) => [n, pkg(n)])),
    args: { rounds: args.rounds, time: args.time, filter: args.filter?.source ?? null, sections: args.sections ? [...args.sections] : null },
    corpus: Object.fromEntries(files.map((f) => [f.name, { bytes: f.bytes, utf16Units: f.units }])),
  };
  console.log(`markdown-syntax-wasm bench — ${results.meta.cpu}, node ${process.version}, commit ${commit}, rounds ${args.rounds} × ≥${args.time} ms\n`);

  if (wants("parse")) sectionParse(files, ours, peers);
  if (wants("breakdown")) sectionBreakdown(files, ours);
  if (wants("wasm-split")) await sectionWasmSplit(files.filter((f) => ["mixed-1m", "real-docs", "syn-cjk", "syn-paragraphs"].includes(f.name)));
  if (wants("write")) sectionWrite(files, ours, peers);
  if (wants("keystroke")) sectionKeystroke(files, ours, peers);
  if (wants("size")) await sectionSize(ours);
  if (wants("cold")) await sectionCold();
  if (wants("memory")) await sectionMemory();

  if (args.save) {
    await mkdir(RESULTS, { recursive: true });
    const path = join(RESULTS, `${args.save}.json`);
    let saved = results;
    if (args.merge && existsSync(path)) {
      saved = JSON.parse(await readFile(path, "utf8"));
      for (const key of Object.keys(results)) {
        if (key === "meta") continue;
        saved[key] = { ...saved[key], ...results[key] };
      }
      saved.meta.merged = [...(saved.meta.merged ?? []), { date: results.meta.date, commit: results.meta.commit, args: results.meta.args }];
    }
    await writeFile(path, `${JSON.stringify(saved, null, 1)}\n`);
    console.log(`saved ${path}`);
  }
  if (args.compare) compare(await loadSaved(args.compare), results, args.compare, args.save ?? "current");
}

await main();
