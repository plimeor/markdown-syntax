// Each node kind's fields, which `KINDS` lists and `Reader.node` and
// `ENCODE_FIELDS` each write out per kind, against the module
// `binding.test.mjs` loads: the reader and the encoder take every kind, and
// only those, with the fields of its row in their order.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

// The wrapper with its internals exported; index.js itself exports only its
// public surface.
const wrapper = await readFile(new URL("../index.js", import.meta.url), "utf8");
const { KINDS, ENCODE, ENCODE_FIELDS, BY_NAME, BASE_BLOCK, Reader, MarkdownSyntax } = await import(
  `data:text/javascript;base64,${Buffer.from(
    `${wrapper}\nexport { KINDS, ENCODE, ENCODE_FIELDS, BY_NAME, BASE_BLOCK, Reader };\n`,
  ).toString("base64")}`
);

const wasmPath =
  process.env.MARKDOWN_SYNTAX_WASM ??
  fileURLToPath(
    new URL("../../../target/wasm32-unknown-unknown/wasm/markdown_syntax_wasm.wasm", import.meta.url),
  );
const binding = await MarkdownSyntax.load(readFile(wasmPath));

const NO_SPAN = 0xffffffff;
const NAMES = KINDS.map(([, name]) => name);
/** The keys of a decoded node of each kind, in order. */
const KEYS = new Map(KINDS.map(([, name, fields]) => [name, ["type", "span", ...fields.map(([field]) => field)]]));

/**
 * A message holding one node of kind `number` with no span, every other
 * word zero, and one string: each field reads as its empty value.
 */
function zeroNode(number) {
  const words = [number, NO_SPAN, 0, ...new Array(64).fill(0)];
  const text = new TextEncoder().encode("s");
  const bytes = new Uint8Array(8 + 4 * words.length + 8 + text.length);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, words.length, true);
  view.setUint32(4, 1, true);
  words.forEach((word, index) => view.setUint32(8 + 4 * index, word, true));
  view.setUint32(8 + 4 * words.length, 0, true);
  view.setUint32(12 + 4 * words.length, text.length, true);
  bytes.set(text, 16 + 4 * words.length);
  return new Reader(bytes).node();
}

/** Every node of `node` with its own fields, by its row in KINDS. */
function* nodes(node) {
  yield node;
  for (const [field, type] of BY_NAME.get(node.type)?.fields ?? []) {
    const value = node[field];
    if (["blocks", "inlines", "items", "rows", "cells", "leafInlines"].includes(type)) {
      for (const child of value) yield* nodes(child);
    } else if (type === "htmlContent") {
      for (const child of value.blocks ?? value.inlines) yield* nodes(child);
    }
  }
}

// A document holding a node of every kind, the second blank line after the
// break a blank line node; its footnote and reference labels differ from
// their identifiers.
const EVERY_KIND = `---
a: b
---

# H *e* **s** ~~d~~ ==m== :smile: \`c\` \\* &amp; <b> $x$ [^F] ^[i] [[w|l]] #tag :t[l]{k=v}

***


> q

> [!NOTE] t
> a

2) a
3) [x] b

    indented

\`\`\`rust
fn
\`\`\`

<div>
x
</div>

<details>
<summary>s</summary>

x

</details>

$$
m
$$

[D]: </u u> 'title'

[^F]: note

| a | b |
|:-:|--|
| c | d |

::leaf[l]{a=b c}

:::box[lab]{#i}
x
:::

[l](/u "t") <http://a> www.b.c ![i](/j) [D][] ![D] a
b\\
c
`;

test("the reader and the encoder take every kind, and only those", () => {
  assert.deepEqual(Object.keys(ENCODE_FIELDS).sort(), [...NAMES].sort());
  for (const [name, kind] of BY_NAME) assert.equal(typeof kind.encode, "function", name);
  for (const [number, name] of KINDS) assert.equal(zeroNode(number).type, name);
  const numbers = new Set(KINDS.map(([number]) => number));
  for (let number = 0; number < 256; number++) {
    if (!numbers.has(number)) assert.throws(() => zeroNode(number), /unknown node kind/, `${number}`);
  }
});

test("the base block's word is the one wire.rs reads, and no node kind's", async () => {
  const wire = await readFile(new URL("../../src/wire.rs", import.meta.url), "utf8");
  const [, word] = wire.match(/pub const BASE_BLOCK: u32 = (0x[0-9A-Fa-f_]+);/);
  assert.equal(BASE_BLOCK, Number(word.replaceAll("_", "")));
  assert.ok(!KINDS.some(([number]) => number === BASE_BLOCK));
  assert.throws(() => zeroNode(BASE_BLOCK), /unknown node kind/);
});

test("the reader writes each kind's fields in the order of its row", () => {
  for (const [number, name] of KINDS) assert.deepEqual(Object.keys(zeroNode(number)), KEYS.get(name), name);
  const document = binding.parse(EVERY_KIND);
  const seen = new Set();
  for (const node of nodes(document.tree)) {
    seen.add(node.type);
    assert.deepEqual(Object.keys(node), KEYS.get(node.type), node.type);
  }
  document.free();
  assert.deepEqual(
    NAMES.filter((name) => !seen.has(name)),
    [],
    "the document holds every kind",
  );
});

test("the encoder visits each kind's fields in the order of its row", () => {
  const original = { ...ENCODE };
  const visited = [];
  for (const type of Object.keys(ENCODE)) ENCODE[type] = (_writer, _node, field) => visited.push([field, type]);
  try {
    for (const [, name, fields] of KINDS) {
      visited.length = 0;
      ENCODE_FIELDS[name](null, { type: name }, []);
      assert.deepEqual(visited, fields, name);
    }
  } finally {
    Object.assign(ENCODE, original);
  }
});

test("a document holding every kind writes back as its source", () => {
  const document = binding.parse(EVERY_KIND);
  assert.deepEqual(document.diagnostics, []);
  assert.equal(document.toMarkdown(), EVERY_KIND);
  assert.equal(document.toMarkdown(structuredClone(document.tree)), EVERY_KIND);
  document.free();
});
