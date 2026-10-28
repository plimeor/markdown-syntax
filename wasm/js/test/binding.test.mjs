// The requirements of `docs/specs/wasm-binding.md`, against the module that
// `cargo build -p markdown-syntax-wasm --target wasm32-unknown-unknown
// --profile wasm` writes, or the one `MARKDOWN_SYNTAX_WASM` names.

import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { describe, test } from "node:test";
import { fileURLToPath } from "node:url";

import { MarkdownSyntax } from "../index.js";

const wasmPath =
  process.env.MARKDOWN_SYNTAX_WASM ??
  fileURLToPath(
    new URL("../../../target/wasm32-unknown-unknown/wasm/markdown_syntax_wasm.wasm", import.meta.url),
  );
const binding = await MarkdownSyntax.load(readFile(wasmPath));

/** The node of `tree` at `path`. */
function nodeAt(tree, path) {
  let node = tree;
  for (const index of path) node = (node.rows ?? node.cells ?? node.children)[index];
  return node;
}

/** Every node of `tree` with its path of indices. */
function* nodes(node, path = []) {
  yield [node, path];
  const children = [
    ...(node.children ?? []),
    ...(node.rows ?? []),
    ...(node.cells ?? []),
    ...(node.label && typeof node.label !== "string" ? node.label : []),
    ...(node.alt ?? []),
  ];
  for (const [index, child] of children.entries()) yield* nodes(child, [...path, index]);
}

describe("Build", () => {
  // wasm-binding: Build
  test("the module loads from bytes and from a compiled module", async () => {
    const bytes = await readFile(wasmPath);
    const compiled = await WebAssembly.compile(bytes);
    for (const wasm of [bytes, compiled]) {
      const loaded = await MarkdownSyntax.load(wasm);
      assert.equal(loaded.parse("a").tree.children[0].type, "paragraph");
    }
    assert.deepEqual(WebAssembly.Module.imports(compiled), []);
  });
});

describe("Parse from JavaScript", () => {
  // wasm-binding: Parse from JavaScript
  test("heading and paragraph", () => {
    const { tree, diagnostics } = binding.parse("# T\n\na *b*");
    assert.deepEqual(diagnostics, []);
    const [heading, paragraph] = tree.children;
    assert.equal(heading.type, "heading");
    assert.equal(heading.depth, 1);
    assert.equal(paragraph.type, "paragraph");
    const [text, emphasis] = paragraph.children;
    assert.deepEqual([text.type, text.value], ["text", "a "]);
    assert.deepEqual([emphasis.type, emphasis.delimiter], ["emphasis", "asterisk"]);
  });

  test("every node kind arrives with its fields", () => {
    const source = [
      "---\na: b\n---",
      "# H *e* **s** ~~d~~ ==m== :smile: `c` \\* &amp; <b>",
      "> [!NOTE]\n> a",
      "2) a\n3) [x] b",
      "```rust\nfn\n```",
      "<details>\n<summary>s</summary>\n\nx\n\n</details>",
      "[d]: </u u> 'title'",
      "[^f]: note",
      "| a |\n|:-:|\n| c |",
      "$$\nx\n$$",
      "",
      "::leaf[l]{a=b c}",
      ":::box\nx\n:::",
      "[l](/u) <http://a> ![i](/j) [d][] ![d] a\\\nb $x$ [^f] ^[n] [[w]] :t[l]{k} #tag",
    ].join("\n\n");
    const types = new Set([...nodes(binding.parse(source).tree)].map(([node]) => node.type));
    for (const type of [
      "frontmatter", "heading", "emphasis", "strong", "delete", "mark", "shortcode", "code",
      "escape", "characterReference", "html", "alert", "list", "listItem", "codeBlock",
      "htmlContainer", "definition", "footnoteDefinition", "table", "tableRow", "tableCell",
      "mathBlock", "leafDirective", "containerDirective", "link", "autolink", "image",
      "linkReference", "imageReference", "lineBreak", "math", "footnoteReference",
      "inlineFootnote", "wikiLink", "textDirective", "tag", "blankLine",
    ]) {
      assert.ok(types.has(type), type);
    }
    const list = binding.parse("2) a\n3) [x] b").tree.children[0];
    assert.deepEqual(
      [list.ordered, list.delimiter, list.children.map((item) => [item.number, item.checked])],
      [true, "paren", [[2, null], [3, true]]],
    );
    const fence = binding.parse("~~~~ js\nx\n~~~~").tree.children[0];
    assert.deepEqual(fence.kind, { type: "fenced", marker: "tilde", length: 4 });
    assert.equal(fence.info, "js");
  });
});

describe("What references and shortcodes stand for", () => {
  // wasm-binding: What references and shortcodes stand for
  test("reference and shortcode", () => {
    const [paragraph] = binding.parse("&amp; :smile:\n").tree.children;
    const [reference, , shortcode] = paragraph.children;
    assert.deepEqual(reference, { type: "characterReference", span: [0, 5], reference: "&amp;", value: "&" });
    assert.deepEqual(shortcode, { type: "shortcode", span: [6, 13], name: "smile", glyph: "😄" });
  });

  test("numeric references, aliases, and inline parses", () => {
    const { inlines } = binding.parseInline("&#x1F600;&#169;&nbsp;:+1:", []);
    assert.deepEqual(
      inlines.map((node) => node.value ?? node.glyph),
      ["😀", "©", "\u00a0", "👍"],
    );
  });

  test("ignored on the way back", () => {
    const document = binding.parse("a");
    const paragraph = (children) => ({
      type: "document",
      span: null,
      children: [{ type: "paragraph", span: null, children }],
    });
    const tree = paragraph([
      { type: "characterReference", span: null, reference: "&lt;", value: "x" },
      { type: "shortcode", span: null, name: "tada", glyph: "x" },
      { type: "characterReference", span: null, reference: "&gt;" },
      { type: "shortcode", span: null, name: "smile" },
    ]);
    assert.equal(document.toMarkdown(tree), "&lt;:tada:&gt;:smile:\n");
  });
});

describe("UTF-16 offsets", () => {
  // wasm-binding: UTF-16 offsets
  test("text beyond the BMP", () => {
    const [paragraph] = binding.parse("😀 *a*").tree.children;
    assert.deepEqual(paragraph.children[1].span, [3, 6]);
  });

  test("every span slices the JavaScript string", () => {
    const source = "é 😀 [x](/y) — `z`\n\n> 中文 *強調*";
    for (const [node] of nodes(binding.parse(source).tree)) {
      if (node.type === "text" && !node.value.includes("\n")) {
        assert.equal(source.slice(...node.span), node.value);
      }
    }
  });
});

describe("Inline parse and inline source from JavaScript", () => {
  // wasm-binding: Inline parse and inline source from JavaScript
  test("paragraph in a list item", () => {
    const document = binding.parse("- a\n  *b*");
    const source = document.inlineSource([0, 0, 0]);
    assert.equal(source.text, "a\n*b*");
    assert.equal(source.toSource(2), 6);
    assert.equal(document.inlineSource([0, 0]), null);
  });

  test("inline parse with the document's definitions", () => {
    const document = binding.parse("[r]: /u\n\n> [s]: /v");
    assert.deepEqual(document.definitions, ["r", "s"]);
    const { inlines, diagnostics } = binding.parseInline("[x][r] 😀[s]", document);
    assert.deepEqual(diagnostics, []);
    assert.deepEqual(
      inlines.map((inline) => [inline.type, inline.span]),
      [["linkReference", [0, 6]], ["text", [6, 9]], ["linkReference", [9, 12]]],
    );
    assert.equal(binding.parseInline("[x][r]").inlines[0].type, "text");
    assert.equal(binding.parseInline("# a").inlines[0].value, "# a");
  });

  test("a leading U+FEFF is content in every string", () => {
    const { inlines } = binding.parseInline("\ufeffa [x](/u)");
    assert.deepEqual(
      inlines.map((inline) => [inline.type, inline.value ?? inline.destination]),
      [["text", "\ufeffa "], ["link", "/u"]],
    );
    const [paragraph] = binding.parse("\ufeff\ufeffx *y*").tree.children;
    assert.equal(paragraph.children[0].value, "\ufeffx ");
    assert.equal(paragraph.children[1].children[0].value, "y");
  });

  test("tokens mark a link's destination and title beyond the BMP", () => {
    const text = '😀 [a *b*](/u "t")';
    const { tokens, nodes } = binding.parseInline(text);
    assert.deepEqual(
      nodes.map((node) => node.type),
      ["text", "link", "text", "emphasis", "text"],
    );
    const parts = tokens.map(({ span, node, role }) => [text.slice(...span), nodes[node].type, role]);
    assert.deepEqual(parts, [
      ["😀 ", "text", "text"],
      ["[", "link", "syntax"],
      ["a ", "text", "text"],
      ["*", "emphasis", "syntax"],
      ["b", "text", "text"],
      ["*", "emphasis", "syntax"],
      ["](", "link", "syntax"],
      ["/u", "link", "destination"],
      [' "', "link", "syntax"],
      ["t", "link", "title"],
      ['")', "link", "syntax"],
    ]);
    assert.equal(tokens.at(-1).span[1], text.length);
  });

  test("each token lies in the node it names, through every container", () => {
    const text =
      "*a **b** ~~c~~ ==d==* [e *f*](u) ![g [h](v)](w) [i *j*][r] ![k][r] ^[l *m*] :n[o *p*]{q} 😀 #t \nx";
    const { tokens, nodes } = binding.parseInline(text, ["r"]);
    const types = new Set(nodes.map((node) => node.type));
    for (const type of ["emphasis", "strong", "delete", "mark", "link", "image", "linkReference",
      "imageReference", "inlineFootnote", "textDirective", "tag", "softBreak"]) {
      assert.ok(types.has(type), type);
    }
    let at = 0;
    for (const { span, node } of tokens) {
      assert.equal(span[0], at);
      at = span[1];
      const [from, to] = nodes[node].span;
      assert.ok(from <= span[0] && span[1] <= to, `${JSON.stringify(span)} in ${nodes[node].type}`);
    }
    assert.equal(at, text.length);
  });

  test("a container with very many children keeps its tokens", () => {
    const text = `[${"\\*".repeat(200000)}](u)`;
    const { tokens, nodes } = binding.parseInline(text);
    assert.equal(nodes.length, 200001);
    assert.equal(tokens.at(-2).role, "destination");
  });

  test("the inline source reproduces the leaf beyond the BMP", () => {
    const source = "> 😀 *a*\n> b";
    const document = binding.parse(source);
    const leaf = document.inlineSource([0, 0]);
    assert.equal(leaf.text, "😀 *a*\nb");
    const { inlines } = binding.parseInline(leaf.text, document);
    const mapped = inlines.map((inline) => leaf.spanToSource(inline.span));
    assert.deepEqual(
      mapped,
      document.tree.children[0].children[0].children.map((inline) => inline.span),
    );
  });

  // wasm-binding: Parse from JavaScript
  test("leaf sources arrive with the parse, as inline sources", () => {
    const source = "# 😀 *h*\n\n| a\\|b | c |\n|-|-|\n\n> - x\n>   y\n\n:::d[l *m*]\nz\n:::";
    const document = binding.parse(source);
    const paths = document.leafSources.map(({ path }) => path);
    assert.deepEqual(paths, [[0], [1, 0, 0], [1, 0, 1], [2, 0, 0, 0], [3], [3, 0]]);
    for (const { path, source: leaf } of document.leafSources) {
      const single = document.inlineSource(path);
      assert.equal(leaf.text, single.text, `${path}`);
      for (let offset = 0; offset <= leaf.text.length; offset++) {
        assert.equal(leaf.toSource(offset), single.toSource(offset), `${path} ${offset}`);
        assert.deepEqual(leaf.spanToSource([0, offset]), single.spanToSource([0, offset]));
      }
    }
    assert.deepEqual(
      document.leafSources.map(({ source }) => source.text),
      ["😀 *h*", "a|b", "c", "x\ny", "l *m*", "z"],
    );
    const [, cell] = document.leafSources;
    assert.equal(source.slice(...cell.source.spanToSource([1, 2])), "\\|");
  });
});

describe("Write from JavaScript", () => {
  // wasm-binding: Write from JavaScript
  test("untouched tree", () => {
    const source = "|a|b|\n|-|-|\n\n*  x\n\n\n# T #";
    const document = binding.parse(source);
    assert.equal(document.toMarkdown(document.tree), source);
    assert.equal(document.toMarkdown(), source);
  });

  test("a tag built in JavaScript", () => {
    const document = binding.parse("a");
    const tree = {
      type: "document",
      span: null,
      children: [
        {
          type: "paragraph",
          span: null,
          children: [
            { type: "text", span: null, value: "see " },
            { type: "tag", span: null, name: "ai/agents" },
          ],
        },
      ],
    };
    assert.equal(document.toMarkdown(tree), "see #ai/agents\n");
    const [tag] = binding.parse("#读书笔记").tree.children[0].children;
    assert.deepEqual([tag.type, tag.name, tag.span], ["tag", "读书笔记", [0, 5]]);
  });

  test("an edited block is written canonically between kept ones", () => {
    const source = "|a|b|\n|-|-|\n\n*  x\n\n\n# T #";
    const document = binding.parse(source);
    const tree = structuredClone(document.tree);
    tree.children[1] = { type: "paragraph", span: null, children: [{ type: "text", span: null, value: "y" }] };
    // The kept blank line stays between the edit and the heading.
    assert.equal(document.toMarkdown(tree), "|a|b|\n|-|-|\n\ny\n\n\n# T #");
    tree.children[1].children[0].value = "z";
    // The kept table and blank line keep their own line endings.
    assert.equal(
      document.toMarkdown(tree, { lineEnding: "crlf", finalNewline: false }),
      "|a|b|\n|-|-|\n\r\nz\r\n\r\n\n# T #",
    );
  });

  // wasm-binding: Write from JavaScript
  test("leaf given as text", () => {
    const document = binding.parse("> a\n\nb\n");
    const tree = structuredClone(document.tree);
    tree.children[0].children[0] = { type: "paragraph", source: "x `` y ``", span: null };
    assert.equal(document.toMarkdown(tree), "> x `` y ``\n\nb\n");
  });

  test("a save that sends every leaf as its source returns the source", () => {
    const source = "# T  *u*\n\n- a  `` b ``\n\n  [c]( /d )\n\n| x  | y\\|z |\n|--|--|\n\n::e[f  *g*]\n\n[c]: /e\n";
    const document = binding.parse(source);
    const tree = structuredClone(document.tree);
    for (const { path, source: leaf } of document.leafSources) {
      const node = nodeAt(tree, path);
      delete node.children;
      if (node.type === "leafDirective" || node.type === "containerDirective") node.label = [];
      node.source = leaf.text;
    }
    assert.equal(document.toMarkdown(tree), source);
    // A leaf edited in its text is written as that text.
    nodeAt(tree, [1, 0, 1]).source = "[c](  /d )";
    assert.equal(document.toMarkdown(tree), source.replace("[c]( /d )", "[c](  /d )"));
  });

  test("a save that ticks one task item keeps every other byte", () => {
    const source = "Todo:\n\n*   [ ] a  *b*\n*   [ ] c\n\n    > d\n*   [x] e\n";
    const document = binding.parse(source);
    const tree = structuredClone(document.tree);
    for (const { path, source: leaf } of document.leafSources) {
      const node = nodeAt(tree, path);
      delete node.children;
      node.source = leaf.text;
    }
    nodeAt(tree, [1, 1]).checked = true;
    assert.equal(document.toMarkdown(tree), source.replace("[ ] c", "[x] c"));
  });

  test("the file's byte order mark is kept", () => {
    const source = "\ufeff# a\n\nb\n";
    const document = binding.parse(source);
    assert.equal(document.toMarkdown(), source);
    const tree = structuredClone(document.tree);
    tree.children[1].children[0] = { type: "text", span: null, value: "c" };
    tree.children[1].span = null;
    assert.equal(document.toMarkdown(tree), "\ufeff# a\n\nc\n");
  });

  // wasm-binding: Write from JavaScript
  test("a refused tree returns the diagnostics", () => {
    const document = binding.parse("- a\n\np\n\n- b");
    const tree = structuredClone(document.tree);
    tree.children.splice(1, 1);
    const result = document.toMarkdown(tree);
    assert.equal(typeof result, "object");
    assert.equal(result.diagnostics[0].code, "invalidDocument");
    assert.equal(result.diagnostics[0].severity, "error");
    // The later of the two adjacent lists.
    assert.deepEqual(result.diagnostics[0].path, [1]);
  });

  test("a parser diagnostic has no path", () => {
    const { diagnostics } = binding.parse(":::1bad\n");
    assert.ok(diagnostics.length > 0);
    assert.ok(diagnostics.every((diagnostic) => diagnostic.path === null));
  });

  test("every value survives the round trip", () => {
    const source = "😀 *a* `b` \\* [c](</d e> 'f') ![g][h] :i[j]{k=l m}\n\n[h]: /x\n\n1. [ ] z";
    const document = binding.parse(source);
    const tree = structuredClone(document.tree);
    tree.children.push({ type: "thematicBreak", span: null, marker: "underscore" });
    assert.equal(document.toMarkdown(tree), `${source}\n\n___\n`);
  });
});

/** The tree `toMarkdownBlocks(blocks)` writes as, for a document whose tree is untouched. */
function resolved(document, blocks) {
  const children = blocks.map((block) => (typeof block === "number" ? document.tree.children[block] : block));
  return { type: "document", span: document.tree.span, children };
}

/** Each outcome of `write`: its text or diagnostics, or that it threw a TypeError. */
function outcome(write) {
  try {
    return write();
  } catch (error) {
    return error instanceof TypeError ? "TypeError" : error;
  }
}

/**
 * `count` edits of the document's top-level blocks that an editor makes, each
 * a list of blocks: indices of untouched blocks, some blocks changed, removed,
 * added, moved, or repeated, by a seeded random choice.
 */
function* blockEdits(document, count, seed) {
  const random = () => ((seed = (seed * 1103515245 + 12345) >>> 0) / 2 ** 32);
  const pick = (length) => Math.floor(random() * length);
  const base = document.tree.children;
  const added = [
    () => ({ type: "paragraph", span: null, children: [{ type: "text", span: null, value: "added *x*" }] }),
    () => ({ type: "heading", span: null, depth: 2, source: "added [a] `b`" }),
    () => ({ type: "thematicBreak", span: null, marker: "dash" }),
    () => ({ type: "codeBlock", span: null, kind: { type: "fenced", marker: "backtick", length: 3 }, info: null, value: "c" }),
    () => ({ type: "blankLine", span: null }),
  ];
  const changed = (index) => {
    const block = structuredClone(base[index]);
    const leaves = document.leafSources.filter(({ path }) => path[0] === index);
    switch (pick(3)) {
      case 0:
        // An edit of its text.
        for (const [node] of nodes(block)) {
          if (node.type === "text") {
            node.value += " edited";
            return block;
          }
        }
        return block;
      case 1:
        // A leaf given as its text, edited or not.
        if (leaves.length > 0 && leaves[0].path.length === 1) {
          const text = leaves[0].source.text + (random() < 0.5 ? "" : " typed");
          return { ...block, children: undefined, label: undefined, source: text };
        }
        return block;
      default:
        // An equal copy.
        return block;
    }
  };
  for (let edit = 0; edit < count; edit++) {
    const blocks = base.map((_, index) => index);
    for (let step = 1 + pick(4); step > 0; step--) {
      const at = pick(blocks.length + 1);
      switch (pick(6)) {
        case 0:
          if (at < blocks.length && typeof blocks[at] === "number") blocks[at] = changed(blocks[at]);
          break;
        case 1:
          blocks.splice(at, 1);
          break;
        case 2:
          blocks.splice(at, 0, added[pick(added.length)]());
          break;
        case 3:
          if (base.length > 0) blocks.splice(at, 0, pick(base.length));
          break;
        case 4: {
          const other = pick(blocks.length + 1);
          if (at < blocks.length && other < blocks.length) [blocks[at], blocks[other]] = [blocks[other], blocks[at]];
          break;
        }
        default:
          if (base.length > 0) blocks.splice(at, 0, changed(pick(base.length)));
      }
    }
    yield blocks;
  }
}

describe("Write the changed blocks", () => {
  // wasm-binding: Write the changed blocks
  test("untouched blocks", () => {
    const source = "# T\n\n> a\n> b\n\n- [ ] c\n\n|x|\n|-|\n";
    const document = binding.parse(source);
    assert.equal(document.toMarkdownBlocks(document.tree.children.map((_, index) => index)), source);
  });

  test("one changed block among untouched ones", () => {
    const document = binding.parse("a\n\n> b\n\n  c *d*\n");
    const changed = { type: "paragraph", span: null, source: "x `` y ``" };
    assert.equal(document.toMarkdownBlocks([0, changed, 2]), "a\n\nx `` y ``\n\n  c *d*\n");
    assert.equal(document.toMarkdownBlocks([2, 0]), document.toMarkdown(resolved(document, [2, 0])));
  });

  test("an index stands for the block as parsed, whatever the tree holds now", () => {
    const document = binding.parse("a\n\nb\n");
    document.tree.children[1].children[0].value = "changed";
    assert.equal(document.toMarkdownBlocks([0, 1]), "a\n\nb\n");
  });

  test("a block given to a footnote definition that held none", () => {
    const document = binding.parse("a\n\n[^5]:\n\nb\n");
    const note = structuredClone(document.tree.children[1]);
    note.children.push({ type: "paragraph", span: null, children: [{ type: "text", span: null, value: "x" }] });
    assert.equal(document.toMarkdownBlocks([0, note, 2]), "a\n\n[^5]: x\n\nb\n");
    assert.equal(document.toMarkdown(resolved(document, [0, note, 2])), "a\n\n[^5]: x\n\nb\n");
    assert.equal(binding.parse("c\n").toMarkdown(), "c\n");
  });

  test("a refused document returns the diagnostics of the blocks as listed", () => {
    const document = binding.parse("a\n");
    const empty = { type: "paragraph", span: null, children: [] };
    const blocks = [0, 0, empty];
    const written = document.toMarkdownBlocks(blocks);
    assert.deepEqual(written, document.toMarkdown(resolved(document, blocks)));
    assert.deepEqual(written.diagnostics[0].path, [2]);
  });

  test("an index that names no block of the parse throws a TypeError naming its place", () => {
    const document = binding.parse("a\n\nb\n");
    for (const index of [-1, 2, 1.5, NaN, Infinity]) {
      assert.throws(
        () => document.toMarkdownBlocks([0, index]),
        (error) => error instanceof TypeError && /\[1\]: .* is not the index of a top-level block/.test(error.message),
      );
    }
    assert.throws(() => document.toMarkdownBlocks([0, "1"]), TypeError);
    // A hole is no block.
    assert.throws(
      () => document.toMarkdownBlocks([, 0]),
      (error) => error instanceof TypeError && /\[0\]: a block is not an object/.test(error.message),
    );
    assert.throws(() => document.toMarkdownBlocks(null), TypeError);
    assert.equal(document.toMarkdownBlocks([1]), "b\n");
    document.free();
    assert.throws(() => document.toMarkdownBlocks([0]), /freed/);
  });

  test("blocks write as the tree they stand for, over every fixture", async () => {
    const root = new URL("../../../tests/fixtures/", import.meta.url);
    const files = (await readdir(root, { recursive: true })).filter((file) => file.endsWith(".md")).sort();
    assert.ok(files.length > 100, `only ${files.length} fixture files`);
    let edits = 0;
    for (const [number, file] of files.entries()) {
      const source = await readFile(new URL(file, root), "utf8");
      const document = binding.parse(source);
      for (const blocks of blockEdits(document, 8, number + 1)) {
        const tree = resolved(document, blocks);
        assert.deepEqual(outcome(() => document.toMarkdownBlocks(blocks)), outcome(() => document.toMarkdown(tree)), file);
        assert.deepEqual(
          outcome(() => document.toMarkdownBlocks(blocks, { lineEnding: "crlf", bullet: "plus" })),
          outcome(() => document.toMarkdown(tree, { lineEnding: "crlf", bullet: "plus" })),
          file,
        );
        edits++;
      }
      document.free();
    }
    assert.ok(edits > 800);
  });
});

describe("Malformed trees", () => {
  // wasm-binding: Malformed trees
  test("each throws a TypeError naming the node's path", () => {
    const document = binding.parse("> a\n\n- b");
    const malformed = [
      [(tree) => (tree.children[0].children[0].type = "para"), /\[0, 0\]: unknown type "para"/],
      [(tree) => delete tree.children[0].children[0].children[0].value, /\[0, 0, 0\]: the text needs value/],
      [(tree) => (tree.children[1].children[0].children = [{ type: "text", span: null, value: "x" }]), /\[1, 0, 0\]: a text where/],
      [(tree) => (tree.children[0].span = [1]), /\[0\]: the span/],
    ];
    for (const [corrupt, message] of malformed) {
      const tree = structuredClone(document.tree);
      corrupt(tree);
      assert.throws(() => document.toMarkdown(tree), (error) => error instanceof TypeError && message.test(error.message));
    }
    assert.equal(document.toMarkdown(), "> a\n\n- b");
  });

  test("a tree nested too deeply throws and leaves the module usable", () => {
    const document = binding.parse("a");
    const nested = (depth) => {
      let inline = { type: "text", span: null, value: "x" };
      for (let level = 2; level < depth; level++) {
        inline = { type: "emphasis", span: null, delimiter: "asterisk", children: [inline] };
      }
      return { type: "document", span: null, children: [{ type: "paragraph", span: null, children: [inline] }] };
    };
    assert.doesNotThrow(() => document.toMarkdown(nested(128)));
    assert.throws(
      () => document.toMarkdown(nested(129)),
      (error) => error instanceof TypeError && /deeper than 128 levels/.test(error.message),
    );
    assert.equal(document.toMarkdown(), "a");
    assert.equal(binding.parse("b").toMarkdown(), "b");
  });
});

describe("Released memory", () => {
  // wasm-binding: Released memory
  test("a freed document throws and others are unaffected", () => {
    const freed = binding.parse("a");
    const kept = binding.parse("b *c*");
    freed.free();
    assert.throws(() => freed.toMarkdown(), /freed/);
    assert.throws(() => freed.inlineSource([0]), /freed/);
    assert.throws(() => freed.free(), /freed/);
    assert.equal(kept.toMarkdown(), "b *c*");
    assert.equal(kept.inlineSource([0]).text, "b *c*");
    const reused = binding.parse("d");
    assert.equal(reused.toMarkdown(), "d");
    assert.equal(kept.toMarkdown(), "b *c*");
  });

  test("parsing and freeing many documents keeps memory bounded", () => {
    const source = "# T\n\n".concat("para *x* [y](/z)\n\n".repeat(200));
    binding.parse(source).free();
    const memory = () => binding.call((exports) => exports.memory.buffer.byteLength);
    const before = memory();
    for (let round = 0; round < 200; round++) binding.parse(source).free();
    assert.ok(memory() <= before + (1 << 20), `${before} -> ${memory()}`);
  });
});
