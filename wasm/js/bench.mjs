// Times the binding on the inputs it is given, or on a one-paragraph input
// and a document built from this repository's docs:
//
//   node wasm/js/bench.mjs [file.md ...]
//
// It loads the module `cargo build -p markdown-syntax-wasm --target
// wasm32-unknown-unknown --profile wasm` writes, or the one
// `MARKDOWN_SYNTAX_WASM` names. Its output is observed, not stored.

import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { MarkdownSyntax } from "./index.js";

const root = fileURLToPath(new URL("../../", import.meta.url));
const wasmPath =
  process.env.MARKDOWN_SYNTAX_WASM ?? join(root, "target/wasm32-unknown-unknown/wasm/markdown_syntax_wasm.wasm");
const binding = await MarkdownSyntax.load(readFile(wasmPath));

async function markdownUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true, recursive: true });
  return entries
    .filter((entry) => entry.isFile() && entry.name.endsWith(".md"))
    .map((entry) => join(entry.parentPath ?? entry.path, entry.name))
    .sort();
}

async function inputs() {
  const files = process.argv.slice(2);
  if (files.length > 0) {
    return Promise.all(files.map(async (file) => [file, await readFile(file, "utf8")]));
  }
  const paragraph =
    "这是一个比较长的段落，用来测量单段解析的速度。It mixes **bold**, _emphasis_, `code`, a [link](https://example.com) and a [[wiki/link]] with #tag text. ".repeat(
      2,
    );
  let document = "";
  for (const file of await markdownUnder(join(root, "docs"))) {
    if (document.length >= 110_000) break;
    document += `${await readFile(file, "utf8")}\n`;
  }
  return [
    ["one paragraph", paragraph],
    ["docs/ joined", document],
  ];
}

function time(label, runs, work) {
  for (let run = 0; run < Math.min(20, runs); run++) work();
  const started = performance.now();
  for (let run = 0; run < runs; run++) work();
  const ms = (performance.now() - started) / runs;
  const shown = ms < 0.1 ? `${(ms * 1000).toFixed(1)} µs` : `${ms.toFixed(3)} ms`;
  console.log(`  ${label.padEnd(36)} ${shown}`);
}

for (const [name, text] of await inputs()) {
  const runs = text.length > 10_000 ? 100 : 5_000;
  console.log(`${name}: ${new TextEncoder().encode(text).length} bytes, ${text.length} UTF-16 units`);
  time("parse (tree, diagnostics, leaves)", runs, () => binding.parse(text).free());
  time("parseInline", runs, () => binding.parseInline(text));
  const document = binding.parse(text);
  time("toMarkdown, untouched tree", runs, () => document.toMarkdown());
  const leaf = document.tree.children.findIndex((block) => block.type === "paragraph");
  if (leaf >= 0) time("inlineSource of a paragraph", runs, () => document.inlineSource([leaf]));
  // A save that sends every leaf as its source text.
  const saved = structuredClone(document.tree);
  for (const { path, source } of document.leafSources) {
    let node = saved;
    for (const index of path) node = (node.rows ?? node.cells ?? node.children)[index];
    if (node.type === "leafDirective" || node.type === "containerDirective") node.label = [];
    else node.children = [];
    node.source = source.text;
  }
  time("toMarkdown, every leaf as source", runs, () => document.toMarkdown(saved));
  document.free();
}
