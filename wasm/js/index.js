// The JavaScript side of markdown-syntax-wasm: loads the module, moves trees
// across in the private encoding of `wasm/src/wire.rs`, and hands out plain
// objects shaped like the Rust AST. No dependencies; Node 22 and WKWebView.

const NO_SPAN = 0xffffffff;
// In the blocks `toMarkdownBlocks` sends, it and the index after it stand for
// a top-level block of the parse: `kind::BASE_BLOCK` in `wasm/src/wire.rs`,
// outside the range of every node kind.
const BASE_BLOCK = 0xfffffffe;
// A leading U+FEFF is content, in the strings and in the written text.
const UTF8 = new TextDecoder("utf-8", { ignoreBOM: true });
const ENCODER = new TextEncoder();
// Stands between two strings in a message, so that a lone surrogate at the
// end of one and one at the start of the next stay lone, each read as U+FFFD.
const SEPARATOR = "\n";

const ENUMS = {
  thematicBreakMarker: ["dash", "asterisk", "underscore"],
  alertKind: ["note", "tip", "important", "warning", "caution"],
  listDelimiter: ["dash", "asterisk", "plus", "period", "paren"],
  fenceMarker: ["backtick", "tilde"],
  destinationKind: ["bare", "angle", "omitted"],
  titleKind: ["doubleQuote", "singleQuote", "paren"],
  tableAlignment: ["none", "left", "center", "right"],
  frontmatterKind: ["yaml", "toml"],
  emphasisDelimiter: ["asterisk", "underscore"],
  autolinkForm: ["angle", "literal"],
  referenceKind: ["full", "collapsed", "shortcut"],
  lineBreakKind: ["backslash", "spaces"],
};

// Each kind's wire number, name, and fields in the order of its Rust struct,
// then what a node stands for where Rust decodes it (`decoded`): a character
// reference's `value()` and a shortcode's `glyph()`. `Reader.node` and
// `ENCODE_FIELDS` take each kind's fields as its row here lists them; both
// are written out per kind, and `test/kinds.test.mjs` keeps them in step
// with the rows.
const KINDS = [
  [0, "document", [["children", "blocks"]]],
  [1, "paragraph", [["children", "leafInlines"]]],
  [2, "heading", [["depth", "depth"], ["children", "leafInlines"]]],
  [3, "thematicBreak", [["marker", "thematicBreakMarker"]]],
  [4, "blockQuote", [["children", "blocks"]]],
  [5, "alert", [["kind", "alertKind"], ["title", "optionalString"], ["children", "blocks"]]],
  [
    6,
    "list",
    [
      ["ordered", "boolean"],
      ["delimiter", "listDelimiter"],
      ["tight", "boolean"],
      ["children", "items"],
    ],
  ],
  [7, "codeBlock", [["kind", "codeBlockKind"], ["info", "optionalString"], ["value", "string"]]],
  [8, "htmlBlock", [["value", "string"]]],
  [9, "htmlContainer", [["opening", "tag"], ["content", "htmlContent"], ["closing", "tag"]]],
  [
    10,
    "definition",
    [
      ["label", "string"],
      ["identifier", "string"],
      ["destination", "string"],
      ["destinationKind", "destinationKind"],
      ["title", "title"],
    ],
  ],
  [11, "footnoteDefinition", [["label", "string"], ["identifier", "string"], ["children", "blocks"]]],
  [12, "table", [["alignments", "alignments"], ["rows", "rows"]]],
  [13, "mathBlock", [["value", "string"]]],
  [14, "frontmatter", [["kind", "frontmatterKind"], ["value", "string"]]],
  [15, "leafDirective", [["name", "string"], ["label", "leafInlines"], ["attributes", "attributes"]]],
  [
    16,
    "containerDirective",
    [["name", "string"], ["label", "leafInlines"], ["attributes", "attributes"], ["children", "blocks"]],
  ],
  [17, "blankLine", []],
  [30, "listItem", [["checked", "checked"], ["number", "number"], ["children", "blocks"]]],
  [31, "tableRow", [["cells", "cells"]]],
  [32, "tableCell", [["children", "leafInlines"]]],
  [40, "text", [["value", "string"]]],
  [41, "escape", [["value", "char"]]],
  [42, "characterReference", [["reference", "string"], ["value", "decoded"]]],
  [43, "emphasis", [["delimiter", "emphasisDelimiter"], ["children", "inlines"]]],
  [44, "strong", [["delimiter", "emphasisDelimiter"], ["children", "inlines"]]],
  [45, "delete", [["children", "inlines"]]],
  [46, "mark", [["children", "inlines"]]],
  [47, "shortcode", [["name", "string"], ["glyph", "decoded"]]],
  [48, "code", [["value", "string"]]],
  [
    49,
    "link",
    [
      ["destination", "string"],
      ["destinationKind", "destinationKind"],
      ["title", "title"],
      ["children", "inlines"],
    ],
  ],
  [50, "autolink", [["form", "autolinkForm"], ["text", "string"]]],
  [
    51,
    "image",
    [
      ["destination", "string"],
      ["destinationKind", "destinationKind"],
      ["title", "title"],
      ["alt", "inlines"],
    ],
  ],
  [
    52,
    "linkReference",
    [["identifier", "string"], ["label", "string"], ["kind", "referenceKind"], ["children", "inlines"]],
  ],
  [
    53,
    "imageReference",
    [["identifier", "string"], ["label", "string"], ["kind", "referenceKind"], ["alt", "inlines"]],
  ],
  [54, "html", [["value", "string"]]],
  [55, "softBreak", []],
  [56, "lineBreak", [["kind", "lineBreakKind"]]],
  [57, "math", [["value", "string"], ["kind", "mathKind"]]],
  [58, "footnoteReference", [["label", "string"], ["identifier", "string"]]],
  [59, "inlineFootnote", [["children", "inlines"]]],
  [60, "wikiLink", [["target", "string"], ["label", "string"], ["embed", "boolean"]]],
  [61, "textDirective", [["name", "string"], ["label", "inlines"], ["attributes", "attributes"]]],
  [62, "tag", [["name", "string"]]],
];

// The kinds each list holds. Wire numbers group by list, so a new kind needs
// no change here: blocks below 30, items, rows, and cells at 30 to 32, and
// inlines from 40.
const LISTS = {
  blocks: (number) => number >= 1 && number < 30,
  inlines: (number) => number >= 40,
  items: (number) => number === 30,
  rows: (number) => number === 31,
  cells: (number) => number === 32,
};

const camel = (name) => name.charAt(0).toLowerCase() + name.slice(1);
const OPTION_NUMBERS = {
  lineEnding: { lf: 0, crlf: 1 },
  bullet: { dash: 1, asterisk: 2, plus: 3 },
  orderedDelimiter: { period: 1, paren: 2 },
  fenceMarker: { backtick: 1, tilde: 2 },
};

/**
 * The inlines and every inline they hold, in pre-order: the order a token's
 * `node` counts in.
 */
function preorder(inlines) {
  const nodes = [];
  const work = [...inlines].reverse();
  while (work.length > 0) {
    const node = work.pop();
    nodes.push(node);
    for (const [field, type] of BY_NAME.get(node.type).fields) {
      if (type !== "inlines") continue;
      const children = node[field];
      for (let index = children.length - 1; index >= 0; index--) work.push(children[index]);
    }
  }
  return nodes;
}

/** Throws unless `offset` is an offset of the text that is `this`. */
function checkOffset(offset) {
  if (!Number.isInteger(offset) || offset < 0 || offset > this.length) {
    throw new RangeError(`markdown-syntax: ${offset} is not an offset of the text`);
  }
}

/**
 * Where `offset` maps by the table of an inline source at `table` in
 * `words`: its run count, then its runs. The run holding `offset` is found by
 * binary search; an offset between the halves of a surrogate pair maps after
 * the first.
 */
function mapOffset(words, table, offset) {
  let low = 0;
  let high = words[table] - 1;
  while (low < high) {
    const middle = (low + high + 1) >> 1;
    if (words[table + 1 + 2 * middle] <= offset) low = middle;
    else high = middle - 1;
  }
  const from = words[table + 1 + 2 * low];
  const to = words[table + 2 + 2 * low];
  return to === NO_SPAN ? mapOffset(words, table, offset - 1) + 1 : to + (offset - from);
}

/** Reads one message of the wire encoding. */
class Reader {
  /**
   * Reads `bytes` in place where they are aligned for words; until
   * `detach()`, the words left to read are the bytes'.
   */
  constructor(bytes) {
    const message = bytes.byteOffset % 4 === 0 ? bytes : bytes.slice();
    const { buffer, byteOffset } = message;
    const [wordCount, stringCount] = new Uint32Array(buffer, byteOffset, 2);
    this.words = new Uint32Array(buffer, byteOffset + 8, wordCount);
    this.at = 0;
    // Whether `words` is a copy of its own, which an inline source may keep.
    this.detached = false;
    const table = new Uint32Array(buffer, byteOffset + 8 + 4 * wordCount, 2 * stringCount);
    const text = UTF8.decode(message.subarray(8 + 4 * wordCount + 8 * stringCount));
    this.strings = new Array(stringCount);
    for (let index = 0; index < stringCount; index++) {
      const start = table[2 * index];
      this.strings[index] = text.slice(start, start + table[2 * index + 1]);
    }
  }

  /** Copies the words left to read, which inline sources keep, off the bytes. */
  detach() {
    this.words = this.words.slice(this.at);
    this.at = 0;
    this.detached = true;
    return this;
  }

  word() {
    return this.words[this.at++];
  }

  span() {
    const start = this.word();
    const end = this.word();
    return start === NO_SPAN ? null : [start, end];
  }

  string() {
    return this.strings[this.word()];
  }

  optionalString() {
    const index = this.word();
    return index === 0 ? null : this.strings[index - 1];
  }

  list(read) {
    const count = this.word();
    const nodes = new Array(count);
    for (let index = 0; index < count; index++) nodes[index] = read();
    return nodes;
  }

  nodes() {
    const count = this.word();
    const nodes = new Array(count);
    for (let index = 0; index < count; index++) nodes[index] = this.node();
    return nodes;
  }

  /**
   * A node: its type, its span, and its fields as its row in KINDS lists
   * them, each kind built whole so that its nodes share one shape.
   */
  node() {
    const number = this.word();
    switch (number) {
      case 0:
        return { type: "document", span: this.span(), children: this.nodes() };
      case 1:
        return { type: "paragraph", span: this.span(), children: this.leafInlines() };
      case 2:
        return { type: "heading", span: this.span(), depth: this.word(), children: this.leafInlines() };
      case 3:
        return { type: "thematicBreak", span: this.span(), marker: ENUMS.thematicBreakMarker[this.word()] };
      case 4:
        return { type: "blockQuote", span: this.span(), children: this.nodes() };
      case 5:
        return {
          type: "alert",
          span: this.span(),
          kind: ENUMS.alertKind[this.word()],
          title: this.optionalString(),
          children: this.nodes(),
        };
      case 6:
        return {
          type: "list",
          span: this.span(),
          ordered: this.boolean(),
          delimiter: ENUMS.listDelimiter[this.word()],
          tight: this.boolean(),
          children: this.nodes(),
        };
      case 7:
        return {
          type: "codeBlock",
          span: this.span(),
          kind: this.codeBlockKind(),
          info: this.optionalString(),
          value: this.string(),
        };
      case 8:
        return { type: "htmlBlock", span: this.span(), value: this.string() };
      case 9:
        return {
          type: "htmlContainer",
          span: this.span(),
          opening: this.tag(),
          content: this.htmlContent(),
          closing: this.tag(),
        };
      case 10:
        return {
          type: "definition",
          span: this.span(),
          label: this.string(),
          identifier: this.string(),
          destination: this.string(),
          destinationKind: ENUMS.destinationKind[this.word()],
          title: this.title(),
        };
      case 11:
        return {
          type: "footnoteDefinition",
          span: this.span(),
          label: this.string(),
          identifier: this.string(),
          children: this.nodes(),
        };
      case 12:
        return {
          type: "table",
          span: this.span(),
          alignments: this.list(() => ENUMS.tableAlignment[this.word()]),
          rows: this.nodes(),
        };
      case 13:
        return { type: "mathBlock", span: this.span(), value: this.string() };
      case 14:
        return {
          type: "frontmatter",
          span: this.span(),
          kind: ENUMS.frontmatterKind[this.word()],
          value: this.string(),
        };
      case 15:
        return {
          type: "leafDirective",
          span: this.span(),
          name: this.string(),
          label: this.leafInlines(),
          attributes: this.attributes(),
        };
      case 16:
        return {
          type: "containerDirective",
          span: this.span(),
          name: this.string(),
          label: this.leafInlines(),
          attributes: this.attributes(),
          children: this.nodes(),
        };
      case 17:
        return { type: "blankLine", span: this.span() };
      case 30:
        return {
          type: "listItem",
          span: this.span(),
          checked: this.checked(),
          number: this.number(),
          children: this.nodes(),
        };
      case 31:
        return { type: "tableRow", span: this.span(), cells: this.nodes() };
      case 32:
        return { type: "tableCell", span: this.span(), children: this.leafInlines() };
      case 40:
        return { type: "text", span: this.span(), value: this.string() };
      case 41:
        return { type: "escape", span: this.span(), value: String.fromCodePoint(this.word()) };
      case 42:
        return {
          type: "characterReference",
          span: this.span(),
          reference: this.string(),
          value: this.optionalString(),
        };
      case 43:
        return {
          type: "emphasis",
          span: this.span(),
          delimiter: ENUMS.emphasisDelimiter[this.word()],
          children: this.nodes(),
        };
      case 44:
        return {
          type: "strong",
          span: this.span(),
          delimiter: ENUMS.emphasisDelimiter[this.word()],
          children: this.nodes(),
        };
      case 45:
        return { type: "delete", span: this.span(), children: this.nodes() };
      case 46:
        return { type: "mark", span: this.span(), children: this.nodes() };
      case 47:
        return { type: "shortcode", span: this.span(), name: this.string(), glyph: this.optionalString() };
      case 48:
        return { type: "code", span: this.span(), value: this.string() };
      case 49:
        return {
          type: "link",
          span: this.span(),
          destination: this.string(),
          destinationKind: ENUMS.destinationKind[this.word()],
          title: this.title(),
          children: this.nodes(),
        };
      case 50:
        return { type: "autolink", span: this.span(), form: ENUMS.autolinkForm[this.word()], text: this.string() };
      case 51:
        return {
          type: "image",
          span: this.span(),
          destination: this.string(),
          destinationKind: ENUMS.destinationKind[this.word()],
          title: this.title(),
          alt: this.nodes(),
        };
      case 52:
        return {
          type: "linkReference",
          span: this.span(),
          identifier: this.string(),
          label: this.string(),
          kind: ENUMS.referenceKind[this.word()],
          children: this.nodes(),
        };
      case 53:
        return {
          type: "imageReference",
          span: this.span(),
          identifier: this.string(),
          label: this.string(),
          kind: ENUMS.referenceKind[this.word()],
          alt: this.nodes(),
        };
      case 54:
        return { type: "html", span: this.span(), value: this.string() };
      case 55:
        return { type: "softBreak", span: this.span() };
      case 56:
        return { type: "lineBreak", span: this.span(), kind: ENUMS.lineBreakKind[this.word()] };
      case 57:
        return { type: "math", span: this.span(), value: this.string(), kind: this.mathKind() };
      case 58:
        return { type: "footnoteReference", span: this.span(), label: this.string(), identifier: this.string() };
      case 59:
        return { type: "inlineFootnote", span: this.span(), children: this.nodes() };
      case 60:
        return {
          type: "wikiLink",
          span: this.span(),
          target: this.string(),
          label: this.string(),
          embed: this.boolean(),
        };
      case 61:
        return {
          type: "textDirective",
          span: this.span(),
          name: this.string(),
          label: this.nodes(),
          attributes: this.attributes(),
        };
      case 62:
        return { type: "tag", span: this.span(), name: this.string() };
      default:
        throw new Error(`markdown-syntax: unknown node kind ${number}`);
    }
  }

  /** A leaf's inline content: a parsed leaf holds no source of its own. */
  leafInlines() {
    this.optionalString();
    return this.nodes();
  }

  boolean() {
    return this.word() === 1;
  }

  checked() {
    const state = this.word();
    return state === 0 ? null : state === 2;
  }

  number() {
    if (this.word() === 0) return null;
    const low = this.word();
    return this.word() * 2 ** 32 + low;
  }

  title() {
    return this.word() === 0 ? null : { value: this.string(), kind: ENUMS.titleKind[this.word()] };
  }

  tag() {
    return { span: this.span(), name: this.string(), raw: this.string() };
  }

  attributes() {
    return this.list(() => ({ name: this.string(), value: this.optionalString() }));
  }

  codeBlockKind() {
    return this.word() === 0
      ? { type: "indented" }
      : { type: "fenced", marker: ENUMS.fenceMarker[this.word()], length: this.word() };
  }

  mathKind() {
    return this.word() === 0 ? { type: "code" } : { type: "dollar", dollars: this.word() };
  }

  htmlContent() {
    return this.word() === 0 ? { blocks: this.nodes() } : { inlines: this.nodes() };
  }

  /**
   * An inline source: its text, with `toSource(offset)` giving the source
   * offset of a text offset and `spanToSource(span)` the source span of a
   * text span, as the parser maps a leaf's spans.
   */
  inlineSource() {
    // Its tables are read from the words, which must not be the bytes of a
    // result the next call overwrites.
    if (!this.detached) throw new Error("markdown-syntax: an inline source is read from a detached Reader");
    const text = this.string();
    const { words } = this;
    // Each table is its run count, then its runs.
    const starts = this.at;
    this.at += 1 + 2 * words[starts];
    const ends = this.at;
    this.at += 1 + 2 * words[ends];
    return {
      text,
      toSource(offset) {
        checkOffset.call(text, offset);
        return mapOffset(words, starts, offset);
      },
      spanToSource(span) {
        if (!Array.isArray(span) || span.length !== 2) {
          throw new TypeError("markdown-syntax: a span is [from, to]");
        }
        span.forEach(checkOffset, text);
        const from = mapOffset(words, starts, span[0]);
        return [from, Math.max(from, mapOffset(words, ends, span[1]))];
      },
    };
  }

  /**
   * The leaf sources of a parse, each its path and its inline source; they
   * keep their tables, and the rest of the result is not kept.
   */
  leafSources() {
    this.detach();
    return this.list(() => ({
      path: this.list(() => this.word()),
      source: this.inlineSource(),
    }));
  }

  /** The tokens of an inline parse, each with its role in camelCase. */
  tokens() {
    return this.list(() => ({ span: this.span(), node: this.word(), role: camel(this.string()) }));
  }

  diagnostics() {
    return this.list(() => ({
      severity: camel(this.string()),
      code: camel(this.string()),
      span: this.span(),
      path: this.word() === 1 ? this.list(() => this.word()) : null,
      message: this.string(),
    }));
  }
}

/** Builds one message of the wire encoding, checking each node it takes. */
class Writer {
  constructor() {
    this.words = new Uint32Array(256);
    this.length = 0;
    this.strings = [];
  }

  word(value) {
    if (this.length === this.words.length) {
      const words = new Uint32Array(2 * this.length);
      words.set(this.words);
      this.words = words;
    }
    this.words[this.length++] = value;
  }

  string(value) {
    this.word(this.strings.length);
    this.strings.push(value);
  }

  finish() {
    const wordCount = this.length;
    const text = this.strings.join(SEPARATOR);
    const textStart = 8 + 4 * wordCount + 8 * this.strings.length;
    // Room for the text's UTF-8, which it encodes into in place: a UTF-16
    // code unit takes at most three bytes.
    const bytes = new Uint8Array(textStart + 3 * text.length);
    const head = new Uint32Array(bytes.buffer, 0, 2 + wordCount + 2 * this.strings.length);
    head[0] = wordCount;
    head[1] = this.strings.length;
    head.set(this.words.subarray(0, wordCount), 2);
    let at = 0;
    let entry = 2 + wordCount;
    for (const value of this.strings) {
      head[entry++] = at;
      head[entry++] = value.length;
      at += value.length + SEPARATOR.length;
    }
    const { written } = ENCODER.encodeInto(text, bytes.subarray(textStart));
    return bytes.subarray(0, textStart + written);
  }
}

function fail(path, message) {
  throw new TypeError(`markdown-syntax: malformed tree at [${path.join(", ")}]: ${message}`);
}

function isIndex(value) {
  return Number.isInteger(value) && value >= 0 && value < NO_SPAN;
}

/**
 * Encodes `node`, at `path`, as one of the kinds `accepts` takes. `path` is
 * extended in place for the node's children and restored after them.
 */
function encodeNode(writer, node, path, accepts, what) {
  if (node === null || typeof node !== "object") fail(path, `${what} is not an object`);
  const kind = BY_NAME.get(node.type);
  if (!kind) fail(path, `unknown type ${JSON.stringify(node.type)}`);
  if (!accepts(kind.number)) fail(path, `a ${node.type} where ${what} belongs`);
  writer.word(kind.number);
  encodeSpan(writer, node.span, path, node.type);
  kind.encode(writer, node, path);
}

/** Encodes the nodes of `items`, the `field` of `node`, as `forEach` visits them. */
function encodeNodes(writer, node, field, items, path, accepts, what) {
  if (!Array.isArray(items)) missing(node, field, path, "an array");
  writer.word(items.length);
  const length = items.length;
  for (let index = 0; index < length; index++) {
    if (!(index in items)) continue;
    path.push(index);
    encodeNode(writer, items[index], path, accepts, what);
    path.pop();
  }
}

function encodeSpan(writer, span, path, what) {
  if (span === null || span === undefined) {
    writer.word(NO_SPAN);
    writer.word(0);
  } else if (Array.isArray(span) && span.length === 2 && isIndex(span[0]) && isIndex(span[1])) {
    writer.word(span[0]);
    writer.word(span[1]);
  } else {
    fail(path, `the span of the ${what} is not [from, to] or null`);
  }
}

function missing(node, field, path, expected) {
  fail(path, `the ${node.type} needs ${field}, ${expected}`);
}

function encodeString(writer, node, field, path, text, expected = "a string") {
  if (typeof text !== "string") missing(node, field, path, expected);
  writer.string(text);
}

function encodeOptionalString(writer, node, field, path, text) {
  if (text === null || text === undefined) {
    writer.word(0);
  } else if (typeof text === "string") {
    writer.string(text);
    writer.words[writer.length - 1] += 1;
  } else {
    missing(node, field, path, "a string or null");
  }
}

function encodeEnumerated(writer, node, field, path, values, item, expected) {
  const index = values.indexOf(item);
  if (index < 0) missing(node, field, path, expected ?? `one of ${values.join(", ")}`);
  writer.word(index);
}

function encodeList(writer, node, field, path, items, each) {
  if (!Array.isArray(items)) missing(node, field, path, "an array");
  writer.word(items.length);
  items.forEach(each);
}

// How `encodeNode` encodes a field of each type, given the field's value.
const ENCODE = {
  blocks: (writer, node, field, value, path) =>
    encodeNodes(writer, node, field, value, path, LISTS.blocks, `a ${field} entry`),
  inlines: (writer, node, field, value, path) =>
    encodeNodes(writer, node, field, value, path, LISTS.inlines, `a ${field} entry`),
  items: (writer, node, field, value, path) =>
    encodeNodes(writer, node, field, value, path, LISTS.items, `a ${field} entry`),
  rows: (writer, node, field, value, path) =>
    encodeNodes(writer, node, field, value, path, LISTS.rows, `a ${field} entry`),
  cells: (writer, node, field, value, path) =>
    encodeNodes(writer, node, field, value, path, LISTS.cells, `a ${field} entry`),
  leafInlines: (writer, node, field, value, path) => {
    // A leaf may give its source in place of its inline content.
    if (typeof node.source === "string") {
      encodeOptionalString(writer, node, field, path, node.source);
      return writer.word(0);
    }
    encodeOptionalString(writer, node, field, path, null);
    return encodeNodes(writer, node, field, value, path, LISTS.inlines, `a ${field} entry`);
  },
  string: (writer, node, field, value, path) => encodeString(writer, node, field, path, value),
  optionalString: (writer, node, field, value, path) => encodeOptionalString(writer, node, field, path, value),
  // Rust decodes it from the node's other fields; whatever the tree holds is ignored.
  decoded: (writer) => writer.word(0),
  boolean: (writer, node, field, value, path) => {
    if (typeof value !== "boolean") missing(node, field, path, "a boolean");
    return writer.word(value ? 1 : 0);
  },
  depth: (writer, node, field, value, path) => {
    if (!Number.isInteger(value) || value < 1 || value > 6) missing(node, field, path, "an integer from 1 to 6");
    return writer.word(value);
  },
  char: (writer, node, field, value, path) => {
    const point = typeof value === "string" ? value.codePointAt(0) : undefined;
    if (point === undefined || String.fromCodePoint(point) !== value) missing(node, field, path, "one char");
    return writer.word(point);
  },
  checked: (writer, node, field, value, path) => {
    if (value === null || value === undefined) return writer.word(0);
    if (typeof value !== "boolean") missing(node, field, path, "a boolean or null");
    return writer.word(value ? 2 : 1);
  },
  number: (writer, node, field, value, path) => {
    if (value === null || value === undefined) return writer.word(0);
    if (!Number.isSafeInteger(value) || value < 0) missing(node, field, path, "a non-negative integer or null");
    writer.word(1);
    writer.word(value % 2 ** 32);
    return writer.word(Math.floor(value / 2 ** 32));
  },
  title: (writer, node, field, value, path) => {
    if (value === null || value === undefined) return writer.word(0);
    if (typeof value !== "object") missing(node, field, path, "{ value, kind } or null");
    writer.word(1);
    encodeString(writer, node, field, path, value.value, "{ value, kind } or null");
    return encodeEnumerated(writer, node, field, path, ENUMS.titleKind, value.kind, "{ value, kind } or null");
  },
  tag: (writer, node, field, value, path) => {
    if (value === null || typeof value !== "object") missing(node, field, path, "{ span, name, raw }");
    encodeSpan(writer, value.span, path, `${field} tag`);
    encodeString(writer, node, field, path, value.name, "{ span, name, raw }");
    return encodeString(writer, node, field, path, value.raw, "{ span, name, raw }");
  },
  attributes: (writer, node, field, value, path) =>
    encodeList(writer, node, field, path, value, (attribute) => {
      if (attribute === null || typeof attribute !== "object") missing(node, field, path, "[{ name, value }]");
      encodeString(writer, node, field, path, attribute.name, "[{ name, value }]");
      encodeOptionalString(writer, node, field, path, attribute.value);
    }),
  alignments: (writer, node, field, value, path) =>
    encodeList(writer, node, field, path, value, (alignment) =>
      encodeEnumerated(writer, node, field, path, ENUMS.tableAlignment, alignment),
    ),
  codeBlockKind: (writer, node, field, value, path) => {
    if (value?.type === "indented") return writer.word(0);
    if (value?.type !== "fenced" || !Number.isInteger(value.length) || value.length < 0) {
      missing(node, field, path, "{ type: 'indented' } or { type: 'fenced', marker, length }");
    }
    writer.word(1);
    encodeEnumerated(writer, node, field, path, ENUMS.fenceMarker, value.marker);
    return writer.word(value.length);
  },
  mathKind: (writer, node, field, value, path) => {
    if (value?.type === "code") return writer.word(0);
    if (value?.type !== "dollar" || !Number.isInteger(value.dollars) || value.dollars < 1) {
      missing(node, field, path, "{ type: 'code' } or { type: 'dollar', dollars }");
    }
    writer.word(1);
    return writer.word(value.dollars);
  },
  htmlContent: (writer, node, field, value, path) => {
    if (Array.isArray(value?.blocks)) {
      writer.word(0);
      return encodeNodes(writer, node, field, value.blocks, path, LISTS.blocks, "a block");
    }
    if (Array.isArray(value?.inlines)) {
      writer.word(1);
      return encodeNodes(writer, node, field, value.inlines, path, LISTS.inlines, "an inline");
    }
    return missing(node, field, path, "{ blocks } or { inlines }");
  },
};
for (const [type, values] of Object.entries(ENUMS)) {
  ENCODE[type] = (writer, node, field, value, path) => encodeEnumerated(writer, node, field, path, values, value);
}

// How `encodeNode` encodes the fields of each kind, as its row in KINDS lists
// them. Each kind reads its fields in code of its own, where its nodes share
// one shape.
const ENCODE_FIELDS = {
  document: (writer, node, path) => ENCODE.blocks(writer, node, "children", node.children, path),
  paragraph: (writer, node, path) => ENCODE.leafInlines(writer, node, "children", node.children, path),
  heading: (writer, node, path) => {
    ENCODE.depth(writer, node, "depth", node.depth, path);
    ENCODE.leafInlines(writer, node, "children", node.children, path);
  },
  thematicBreak: (writer, node, path) => ENCODE.thematicBreakMarker(writer, node, "marker", node.marker, path),
  blockQuote: (writer, node, path) => ENCODE.blocks(writer, node, "children", node.children, path),
  alert: (writer, node, path) => {
    ENCODE.alertKind(writer, node, "kind", node.kind, path);
    ENCODE.optionalString(writer, node, "title", node.title, path);
    ENCODE.blocks(writer, node, "children", node.children, path);
  },
  list: (writer, node, path) => {
    ENCODE.boolean(writer, node, "ordered", node.ordered, path);
    ENCODE.listDelimiter(writer, node, "delimiter", node.delimiter, path);
    ENCODE.boolean(writer, node, "tight", node.tight, path);
    ENCODE.items(writer, node, "children", node.children, path);
  },
  codeBlock: (writer, node, path) => {
    ENCODE.codeBlockKind(writer, node, "kind", node.kind, path);
    ENCODE.optionalString(writer, node, "info", node.info, path);
    ENCODE.string(writer, node, "value", node.value, path);
  },
  htmlBlock: (writer, node, path) => ENCODE.string(writer, node, "value", node.value, path),
  htmlContainer: (writer, node, path) => {
    ENCODE.tag(writer, node, "opening", node.opening, path);
    ENCODE.htmlContent(writer, node, "content", node.content, path);
    ENCODE.tag(writer, node, "closing", node.closing, path);
  },
  definition: (writer, node, path) => {
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.string(writer, node, "identifier", node.identifier, path);
    ENCODE.string(writer, node, "destination", node.destination, path);
    ENCODE.destinationKind(writer, node, "destinationKind", node.destinationKind, path);
    ENCODE.title(writer, node, "title", node.title, path);
  },
  footnoteDefinition: (writer, node, path) => {
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.string(writer, node, "identifier", node.identifier, path);
    ENCODE.blocks(writer, node, "children", node.children, path);
  },
  table: (writer, node, path) => {
    ENCODE.alignments(writer, node, "alignments", node.alignments, path);
    ENCODE.rows(writer, node, "rows", node.rows, path);
  },
  mathBlock: (writer, node, path) => ENCODE.string(writer, node, "value", node.value, path),
  frontmatter: (writer, node, path) => {
    ENCODE.frontmatterKind(writer, node, "kind", node.kind, path);
    ENCODE.string(writer, node, "value", node.value, path);
  },
  leafDirective: (writer, node, path) => {
    ENCODE.string(writer, node, "name", node.name, path);
    ENCODE.leafInlines(writer, node, "label", node.label, path);
    ENCODE.attributes(writer, node, "attributes", node.attributes, path);
  },
  containerDirective: (writer, node, path) => {
    ENCODE.string(writer, node, "name", node.name, path);
    ENCODE.leafInlines(writer, node, "label", node.label, path);
    ENCODE.attributes(writer, node, "attributes", node.attributes, path);
    ENCODE.blocks(writer, node, "children", node.children, path);
  },
  blankLine: () => {},
  listItem: (writer, node, path) => {
    ENCODE.checked(writer, node, "checked", node.checked, path);
    ENCODE.number(writer, node, "number", node.number, path);
    ENCODE.blocks(writer, node, "children", node.children, path);
  },
  tableRow: (writer, node, path) => ENCODE.cells(writer, node, "cells", node.cells, path),
  tableCell: (writer, node, path) => ENCODE.leafInlines(writer, node, "children", node.children, path),
  text: (writer, node, path) => ENCODE.string(writer, node, "value", node.value, path),
  escape: (writer, node, path) => ENCODE.char(writer, node, "value", node.value, path),
  characterReference: (writer, node, path) => {
    ENCODE.string(writer, node, "reference", node.reference, path);
    ENCODE.decoded(writer, node, "value", node.value, path);
  },
  emphasis: (writer, node, path) => {
    ENCODE.emphasisDelimiter(writer, node, "delimiter", node.delimiter, path);
    ENCODE.inlines(writer, node, "children", node.children, path);
  },
  strong: (writer, node, path) => {
    ENCODE.emphasisDelimiter(writer, node, "delimiter", node.delimiter, path);
    ENCODE.inlines(writer, node, "children", node.children, path);
  },
  delete: (writer, node, path) => ENCODE.inlines(writer, node, "children", node.children, path),
  mark: (writer, node, path) => ENCODE.inlines(writer, node, "children", node.children, path),
  shortcode: (writer, node, path) => {
    ENCODE.string(writer, node, "name", node.name, path);
    ENCODE.decoded(writer, node, "glyph", node.glyph, path);
  },
  code: (writer, node, path) => ENCODE.string(writer, node, "value", node.value, path),
  link: (writer, node, path) => {
    ENCODE.string(writer, node, "destination", node.destination, path);
    ENCODE.destinationKind(writer, node, "destinationKind", node.destinationKind, path);
    ENCODE.title(writer, node, "title", node.title, path);
    ENCODE.inlines(writer, node, "children", node.children, path);
  },
  autolink: (writer, node, path) => {
    ENCODE.autolinkForm(writer, node, "form", node.form, path);
    ENCODE.string(writer, node, "text", node.text, path);
  },
  image: (writer, node, path) => {
    ENCODE.string(writer, node, "destination", node.destination, path);
    ENCODE.destinationKind(writer, node, "destinationKind", node.destinationKind, path);
    ENCODE.title(writer, node, "title", node.title, path);
    ENCODE.inlines(writer, node, "alt", node.alt, path);
  },
  linkReference: (writer, node, path) => {
    ENCODE.string(writer, node, "identifier", node.identifier, path);
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.referenceKind(writer, node, "kind", node.kind, path);
    ENCODE.inlines(writer, node, "children", node.children, path);
  },
  imageReference: (writer, node, path) => {
    ENCODE.string(writer, node, "identifier", node.identifier, path);
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.referenceKind(writer, node, "kind", node.kind, path);
    ENCODE.inlines(writer, node, "alt", node.alt, path);
  },
  html: (writer, node, path) => ENCODE.string(writer, node, "value", node.value, path),
  softBreak: () => {},
  lineBreak: (writer, node, path) => ENCODE.lineBreakKind(writer, node, "kind", node.kind, path),
  math: (writer, node, path) => {
    ENCODE.string(writer, node, "value", node.value, path);
    ENCODE.mathKind(writer, node, "kind", node.kind, path);
  },
  footnoteReference: (writer, node, path) => {
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.string(writer, node, "identifier", node.identifier, path);
  },
  inlineFootnote: (writer, node, path) => ENCODE.inlines(writer, node, "children", node.children, path),
  wikiLink: (writer, node, path) => {
    ENCODE.string(writer, node, "target", node.target, path);
    ENCODE.string(writer, node, "label", node.label, path);
    ENCODE.boolean(writer, node, "embed", node.embed, path);
  },
  textDirective: (writer, node, path) => {
    ENCODE.string(writer, node, "name", node.name, path);
    ENCODE.inlines(writer, node, "label", node.label, path);
    ENCODE.attributes(writer, node, "attributes", node.attributes, path);
  },
  tag: (writer, node, path) => ENCODE.string(writer, node, "name", node.name, path),
};

// Each kind's wire number, its fields, and the encoder of its fields, by name.
const BY_NAME = new Map(
  KINDS.map(([number, name, fields]) => [name, { number, fields, encode: ENCODE_FIELDS[name] }]),
);

/** The labels of the definitions in `blocks`, at every depth. */
function definitionLabels(blocks, labels = []) {
  for (const block of blocks) {
    switch (block.type) {
      case "definition":
        labels.push(block.label);
        break;
      case "list":
        for (const item of block.children) definitionLabels(item.children, labels);
        break;
      case "htmlContainer":
        definitionLabels(block.content.blocks ?? [], labels);
        break;
      case "blockQuote":
      case "alert":
      case "footnoteDefinition":
      case "containerDirective":
        definitionLabels(block.children, labels);
        break;
    }
  }
  return labels;
}

/** A document parsed in WebAssembly, which keeps its source there. */
export class ParsedDocument {
  #binding;
  #handle;
  #definitions;
  // How many top-level blocks the parse returned.
  #blockCount;

  constructor(binding, handle, source, tree, diagnostics, leafSources) {
    this.#binding = binding;
    this.#handle = handle;
    this.#blockCount = tree.children.length;
    binding.track(this, handle);
    /** The source the document was parsed from. */
    this.source = source;
    /** The document's tree. */
    this.tree = tree;
    /** The parser's diagnostics. */
    this.diagnostics = diagnostics;
    /** The path and inline source of every leaf, in document order. */
    this.leafSources = leafSources;
  }

  #live() {
    if (this.#handle === 0) throw new Error("markdown-syntax: the document was freed");
    return this.#handle;
  }

  /** The labels of the document's link reference definitions. */
  get definitions() {
    this.#live();
    this.#definitions ??= definitionLabels(this.tree.children);
    return this.#definitions;
  }

  /**
   * The inline content the parser read for the leaf at `path` (a paragraph,
   * a heading, a table cell, or a directive's label), with
   * `toSource(offset)` giving the source offset of a text offset and
   * `spanToSource(span)` the source span of a text span, as the parser maps
   * a leaf's spans; or null.
   */
  inlineSource(path) {
    const handle = this.#live();
    if (!Array.isArray(path) || !path.every(isIndex)) {
      throw new TypeError("markdown-syntax: a path is an array of indices");
    }
    const bytes = new Uint8Array(new Uint32Array(path).buffer);
    const status = this.#binding.call((exports) =>
      this.#binding.withBytes(bytes, (pointer, length) => exports.inline_source(handle, pointer, length)),
    );
    if (status === 0) return null;
    return new Reader(this.#binding.view()).detach().inlineSource();
  }

  /**
   * The Markdown that `tree` writes as with this document's source: the
   * source of each unchanged top-level block, and canonical Markdown for the
   * rest. Returns `{ diagnostics }` when validation refuses the tree, and
   * throws a TypeError for a malformed one.
   */
  toMarkdown(tree = this.tree, options = {}) {
    const handle = this.#live();
    const writer = new Writer();
    encodeNode(writer, tree, [], (number) => number === 0, "the document");
    return this.#write("to_markdown", handle, writer.finish(), options);
  }

  /**
   * What `toMarkdown` writes for the document whose top-level blocks are
   * `blocks`, with the span of this document's tree: each entry a block, or
   * the index of a top-level block of this document's parse, which stands
   * for that block as the parse returned it, whatever `tree` holds now. Only
   * the blocks given whole cross into WebAssembly.
   */
  toMarkdownBlocks(blocks, options = {}) {
    const handle = this.#live();
    const writer = new Writer();
    const path = [];
    if (!Array.isArray(blocks)) fail(path, "the blocks are not an array");
    writer.word(blocks.length);
    const length = blocks.length;
    for (let index = 0; index < length; index++) {
      path.push(index);
      const block = blocks[index];
      if (typeof block !== "number") {
        encodeNode(writer, block, path, LISTS.blocks, "a block");
      } else if (Number.isInteger(block) && block >= 0 && block < this.#blockCount) {
        writer.word(BASE_BLOCK);
        writer.word(block);
      } else {
        fail(path, `${block} is not the index of a top-level block of the parse`);
      }
      path.pop();
    }
    return this.#write("to_markdown_blocks", handle, writer.finish(), options);
  }

  /** Writes `message` by the export `write` names, with `options`. */
  #write(write, handle, message, options) {
    const number = (option, values) => {
      const value = options[option];
      if (value === null || value === undefined) return 0;
      if (!(value in values)) throw new TypeError(`markdown-syntax: ${value} is not a ${option}`);
      return values[value];
    };
    const settings = [
      number("lineEnding", OPTION_NUMBERS.lineEnding),
      options.finalNewline === false ? 0 : 1,
      number("bullet", OPTION_NUMBERS.bullet),
      number("orderedDelimiter", OPTION_NUMBERS.orderedDelimiter),
      number("fenceMarker", OPTION_NUMBERS.fenceMarker),
    ];
    const status = this.#binding.call((exports) =>
      this.#binding.withBytes(message, (pointer, length) => exports[write](handle, pointer, length, ...settings)),
    );
    const result = this.#binding.view();
    if (status === 0) return UTF8.decode(result);
    if (status === 1) return { diagnostics: new Reader(result).diagnostics() };
    throw new TypeError(`markdown-syntax: malformed tree: ${UTF8.decode(result)}`);
  }

  /** Releases the document in WebAssembly; later calls on it throw. */
  free() {
    const handle = this.#live();
    this.#binding.untrack(this);
    this.#binding.call((exports) => exports.document_free(handle));
    this.#handle = 0;
  }
}

/** markdown-syntax compiled to WebAssembly. */
export class MarkdownSyntax {
  #exports;
  // Releases the document of a ParsedDocument collected without `free()`.
  #documents = new FinalizationRegistry((handle) => this.#exports.document_free(handle));
  // The labels `parseInline` took last, as they were then, and their message.
  #labels = null;
  #labelsMessage = null;

  constructor(instance) {
    this.#exports = instance.exports;
  }

  /**
   * Loads the module from its bytes, a compiled `WebAssembly.Module`, or a
   * `Response` (or a promise of one) that fetches it.
   */
  static async load(wasm) {
    let source = await wasm;
    if (typeof Response !== "undefined" && source instanceof Response) source = await source.arrayBuffer();
    const instance =
      source instanceof WebAssembly.Module
        ? await WebAssembly.instantiate(source, {})
        : (await WebAssembly.instantiate(source, {})).instance;
    return new MarkdownSyntax(instance);
  }

  /** Parses `source`; the document keeps it in WebAssembly until `free()`. */
  parse(source) {
    const bytes = ENCODER.encode(String(source));
    const handle = this.call((exports) => this.withBytes(bytes, (pointer, length) => exports.parse(pointer, length)));
    const reader = new Reader(this.view());
    const tree = reader.node();
    const diagnostics = reader.diagnostics();
    const leafSources = reader.leafSources();
    return new ParsedDocument(this, handle, String(source), tree, diagnostics, leafSources);
  }

  /**
   * Parses `text` as one leaf block's inline content, resolving references
   * against `definitions`: labels, or a ParsedDocument whose own to take.
   */
  parseInline(text, definitions = []) {
    const labels = definitions instanceof ParsedDocument ? definitions.definitions : definitions;
    if (!Array.isArray(labels) || !labels.every((label) => typeof label === "string")) {
      throw new TypeError("markdown-syntax: definitions are an array of labels");
    }
    // An editor passes the same labels on every keystroke: they are encoded
    // again only when they changed.
    if (!sameLabels(labels, this.#labels)) {
      const writer = new Writer();
      writer.word(labels.length);
      for (const label of labels) writer.string(label);
      this.#labelsMessage = writer.finish();
      this.#labels = labels.slice();
    }
    const message = this.#labelsMessage;
    const bytes = ENCODER.encode(String(text));
    const status = this.call((exports) =>
      this.withBytes(bytes, (pointer, length) =>
        this.withBytes(message, (labelsPointer, labelsLength) =>
          exports.parse_inline(pointer, length, labelsPointer, labelsLength),
        ),
      ),
    );
    if (status !== 0) throw new TypeError(`markdown-syntax: ${UTF8.decode(this.result())}`);
    const reader = new Reader(this.view());
    const inlines = reader.nodes();
    const tokens = reader.tokens();
    return { inlines, tokens, nodes: preorder(inlines), diagnostics: reader.diagnostics() };
  }

  /** @internal Frees `handle` once `document` is collected. */
  track(document, handle) {
    this.#documents.register(document, handle, document);
  }

  /** @internal */
  untrack(document) {
    this.#documents.unregister(document);
  }

  /** @internal */
  call(work) {
    return work(this.#exports);
  }

  /** @internal Copies `bytes` into WebAssembly memory for one call. */
  withBytes(bytes, work) {
    const exports = this.#exports;
    const length = bytes.length;
    const pointer = exports.buffer_alloc(length);
    try {
      new Uint8Array(exports.memory.buffer, pointer, length).set(bytes);
      return work(pointer, length);
    } finally {
      exports.buffer_free(pointer, length);
    }
  }

  /** @internal A copy of the last call's result buffer. */
  result() {
    return this.view().slice();
  }

  /** @internal The last call's result buffer in place, valid until the next call. */
  view() {
    const exports = this.#exports;
    return new Uint8Array(exports.memory.buffer, exports.result_ptr(), exports.result_len());
  }
}

/** Whether `labels` holds the same strings `previous` held. */
function sameLabels(labels, previous) {
  if (previous === null || labels.length !== previous.length) return false;
  for (let index = 0; index < labels.length; index++) {
    if (labels[index] !== previous[index]) return false;
  }
  return true;
}
