// Writes the benchmark corpus into ./out: real documents (the CommonMark
// spec, Rust's release notes, this repository's docs) and seeded synthetic
// documents, one per construct, each about 100 KB, plus a mixed document at
// 1 KB, 10 KB, 100 KB, and 1 MB.
//
//   node tools/bench/corpus/gen.mjs
//
// The output is deterministic: the same script writes the same bytes.

import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL(".", import.meta.url));
const root = join(here, "../../..");
const out = join(here, "out");
await mkdir(out, { recursive: true });

let seed = 0x9e3779b9;
function rand() {
  seed ^= seed << 13;
  seed ^= seed >>> 17;
  seed ^= seed << 5;
  return (seed >>> 0) / 0x100000000;
}
const pick = (list) => list[Math.floor(rand() * list.length)];
const int = (lo, hi) => lo + Math.floor(rand() * (hi - lo + 1));

const words = "the quick brown fox jumps over lazy dog parser editor syntax tree node block inline source text write read keep change line span offset value list item quote table cell code fence heading link image".split(" ");
const cjk = "这是一个用于测试解析速度的中文段落其中包含标点符号和常见词语编辑器需要快速响应每一次输入".split("");
const word = () => pick(words);
const sentence = (n = int(6, 16)) => Array.from({ length: n }, word).join(" ");

function inline() {
  switch (int(0, 11)) {
    case 0: return `**${sentence(2)}**`;
    case 1: return `_${sentence(2)}_`;
    case 2: return `\`${word()}()\``;
    case 3: return `[${sentence(2)}](https://example.com/${word()})`;
    case 4: return `![${word()}](img/${word()}.png "title")`;
    case 5: return `~~${word()}~~`;
    case 6: return `<https://example.com/${word()}>`;
    case 7: return `&amp; &#123; :smile:`;
    case 8: return `[[${word()}/${word()}]]`;
    case 9: return `#${word()}`;
    default: return sentence(int(2, 6));
  }
}
const richLine = () => Array.from({ length: int(4, 9) }, () => (rand() < 0.4 ? inline() : sentence(int(2, 5)))).join(" ");
const paragraph = () => Array.from({ length: int(1, 4) }, richLine).join("\n") + "\n";
const cjkParagraph = () => {
  let s = "";
  for (let i = 0, n = int(40, 160); i < n; i++) {
    s += pick(cjk);
    if (rand() < 0.04) s += pick(["，", "。", "**加粗**", "*强调*", "`代码`", "（括号）", "[链接](https://例子.com)"]);
  }
  return s + "。\n";
};

function list(depth = 0) {
  const ordered = rand() < 0.4;
  let s = "";
  for (let i = 1, n = int(2, 6); i <= n; i++) {
    const indent = "  ".repeat(depth * (ordered ? 1.5 : 1));
    const marker = ordered ? `${i}. ` : pick(["- ", "* ", "- [ ] ", "- [x] "]);
    s += `${" ".repeat(depth * 3)}${marker}${richLine()}\n`;
    if (depth < 4 && rand() < 0.3) s += list(depth + 1);
  }
  void list;
  return s;
}
function quote(depth = 1) {
  let s = "";
  for (let i = 0, n = int(1, 4); i < n; i++) s += `${"> ".repeat(depth)}${richLine()}\n`;
  if (depth < 5 && rand() < 0.5) s += `${"> ".repeat(depth)}\n` + quote(depth + 1);
  return s;
}
function table() {
  const cols = int(2, 6);
  let s = `| ${Array.from({ length: cols }, word).join(" | ")} |\n|${Array.from({ length: cols }, () => pick([" --- ", " :-- ", " :-: ", " --: "])).join("|")}|\n`;
  for (let r = 0, n = int(3, 20); r < n; r++) s += `| ${Array.from({ length: cols }, () => (rand() < 0.3 ? inline() : sentence(int(1, 4)))).join(" | ")} |\n`;
  return s;
}
function code() {
  let s = "```" + pick(["rust", "js", "", "python"]) + "\n";
  for (let i = 0, n = int(3, 30); i < n; i++) s += `${"    ".repeat(int(0, 3))}let ${word()} = ${word()}(${int(0, 999)});\n`;
  return s + "```\n";
}
const heading = () => `${"#".repeat(int(1, 6))} ${sentence(int(2, 6))}\n`;
const html = () => pick([`<div class="${word()}">\n${sentence()}\n</div>\n`, `<!-- ${sentence()} -->\n`, `<details><summary>${word()}</summary>\n\n${paragraph()}\n</details>\n`]);
const directive = () => `:::${word()}{#${word()} .x}\n${paragraph()}:::\n`;
const definitions = () => Array.from({ length: int(2, 6) }, () => `[${word()}${int(0, 99)}]: https://example.com/${word()} "${word()}"\n`).join("");
const refLinks = () => Array.from({ length: int(4, 9) }, () => `[${sentence(2)}][${word()}${int(0, 99)}] ${sentence(3)}`).join(" ") + "\n";
const emphasisHeavy = () => Array.from({ length: int(10, 30) }, () => pick(["*a*", "**b**", "_c_", "__d__", "***e***", "*f **g** h*", "a*b*c", "**x*y*z**", "_a_b_", "~~s~~"])).join(" ") + "\n";

function mixed() {
  switch (int(0, 13)) {
    case 0: case 1: case 2: case 3: return paragraph();
    case 4: return heading();
    case 5: case 6: return list();
    case 7: return quote();
    case 8: return table();
    case 9: return code();
    case 10: return cjkParagraph();
    case 11: return html();
    case 12: return definitions() + refLinks();
    default: return `${sentence()}  \n${sentence()}\\\n${sentence()}\n`;
  }
}

function build(block, size) {
  let s = "";
  while (s.length < size) s += block() + "\n";
  return s;
}

const outputs = {
  "syn-paragraphs.md": build(paragraph, 100_000),
  "syn-cjk.md": build(cjkParagraph, 100_000),
  "syn-lists.md": build(list, 100_000),
  "syn-quotes.md": build(quote, 100_000),
  "syn-tables.md": build(table, 100_000),
  "syn-code.md": build(code, 100_000),
  "syn-headings.md": build(heading, 100_000),
  "syn-html.md": build(html, 100_000),
  "syn-directives.md": build(directive, 100_000),
  "syn-references.md": build(() => definitions() + refLinks(), 100_000),
  "syn-emphasis.md": build(emphasisHeavy, 100_000),
  "mixed-1k.md": build(mixed, 1_000),
  "mixed-10k.md": build(mixed, 10_000),
  "mixed-100k.md": build(mixed, 100_000),
  "mixed-1m.md": build(mixed, 1_000_000),
};
for (const [name, text] of Object.entries(outputs)) await writeFile(join(out, name), text);

// Real documents.
for (const name of ["spec.md", "rust-releases.md"]) await writeFile(join(out, `real-${name}`), await readFile(join(here, name)));
const docs = (await readdir(join(root, "docs"), { withFileTypes: true, recursive: true }))
  .filter((e) => e.isFile() && e.name.endsWith(".md"))
  .map((e) => join(e.parentPath ?? e.path, e.name))
  .sort();
let joined = "";
for (const file of docs) joined += `${await readFile(file, "utf8")}\n`;
await writeFile(join(out, "real-docs.md"), joined);

for (const name of (await readdir(out)).sort()) {
  const text = await readFile(join(out, name));
  console.log(`${name.padEnd(24)} ${text.length} bytes`);
}
