// Strings that cross the boundary from JavaScript, which may hold lone
// surrogates, against the module `binding.test.mjs` loads.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { MarkdownSyntax } from "../index.js";

const wasmPath =
  process.env.MARKDOWN_SYNTAX_WASM ??
  fileURLToPath(
    new URL("../../../target/wasm32-unknown-unknown/wasm/markdown_syntax_wasm.wasm", import.meta.url),
  );
const binding = await MarkdownSyntax.load(readFile(wasmPath));

test("a lone surrogate ending one string and one starting the next stay apart", () => {
  const document = binding.parse("a\n");
  const tree = {
    type: "document",
    span: null,
    children: [
      {
        type: "paragraph",
        span: null,
        children: [
          { type: "text", span: null, value: "a\uD83D" },
          { type: "text", span: null, value: "\uDE00b" },
        ],
      },
    ],
  };
  // Each lone surrogate reads as U+FFFD; joined, the two would have read as
  // one astral char.
  assert.equal(document.toMarkdown(tree), "a��b\n");
  document.free();
});
