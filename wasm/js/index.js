// The JavaScript side of markdown-syntax-wasm: loads the module, moves trees
// across in the private encoding of `wasm/src/wire.rs`, and hands out plain
// objects shaped like the Rust AST. No dependencies; Node 22 and WKWebView.

const NO_SPAN = 0xffffffff;
// A leading U+FEFF is content, in the string table and in the written text.
const UTF16 = new TextDecoder("utf-16le", { ignoreBOM: true });
const UTF8 = new TextDecoder("utf-8", { ignoreBOM: true });

const ENUMS = {
  headingKind: ["atx", "setext"],
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

// Each kind's wire number, name, and fields in the order of its Rust struct.
const KINDS = [
  [0, "document", [["children", "blocks"]]],
  [1, "paragraph", [["children", "inlines"]]],
  [2, "heading", [["depth", "depth"], ["kind", "headingKind"], ["children", "inlines"]]],
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
  [15, "leafDirective", [["name", "string"], ["label", "inlines"], ["attributes", "attributes"]]],
  [
    16,
    "containerDirective",
    [["name", "string"], ["label", "inlines"], ["attributes", "attributes"], ["children", "blocks"]],
  ],
  [17, "blankLine", []],
  [30, "listItem", [["checked", "checked"], ["number", "number"], ["children", "blocks"]]],
  [31, "tableRow", [["cells", "cells"]]],
  [32, "tableCell", [["children", "inlines"]]],
  [40, "text", [["value", "string"]]],
  [41, "escape", [["value", "char"]]],
  [42, "characterReference", [["reference", "string"]]],
  [43, "emphasis", [["delimiter", "emphasisDelimiter"], ["children", "inlines"]]],
  [44, "strong", [["delimiter", "emphasisDelimiter"], ["children", "inlines"]]],
  [45, "delete", [["children", "inlines"]]],
  [46, "mark", [["children", "inlines"]]],
  [47, "shortcode", [["name", "string"]]],
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

const BY_NUMBER = new Map(KINDS.map(([number, name, fields]) => [number, { name, fields }]));
const BY_NAME = new Map(KINDS.map(([number, name, fields]) => [name, { number, fields }]));

// The kinds each list holds. Wire numbers group by list, so a new kind needs
// only its row in KINDS: blocks below 30, items, rows, and cells at 30 to 32,
// and inlines from 40.
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

/** Reads one message of the wire encoding. */
class Reader {
  constructor(bytes) {
    const copy = bytes.slice();
    const header = new Uint32Array(copy.buffer, 0, 2);
    const [wordCount, stringCount] = header;
    this.words = new Uint32Array(copy.buffer, 8, wordCount);
    this.at = 0;
    const table = new Uint32Array(copy.buffer, 8 + 4 * wordCount, 2 * stringCount);
    const unitsStart = 8 + 4 * wordCount + 8 * stringCount;
    const units = UTF16.decode(copy.subarray(unitsStart));
    this.strings = new Array(stringCount);
    for (let index = 0; index < stringCount; index++) {
      const start = table[2 * index];
      this.strings[index] = units.slice(start, start + table[2 * index + 1]);
    }
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

  node() {
    const number = this.word();
    const kind = BY_NUMBER.get(number);
    if (!kind) throw new Error(`markdown-syntax: unknown node kind ${number}`);
    const node = { type: kind.name, span: this.span() };
    for (const [field, type] of kind.fields) node[field] = this.field(type);
    return node;
  }

  field(type) {
    switch (type) {
      case "blocks":
      case "inlines":
      case "items":
      case "rows":
      case "cells":
        return this.list(() => this.node());
      case "string":
        return this.string();
      case "optionalString":
        return this.optionalString();
      case "boolean":
        return this.word() === 1;
      case "depth":
        return this.word();
      case "char":
        return String.fromCodePoint(this.word());
      case "checked": {
        const state = this.word();
        return state === 0 ? null : state === 2;
      }
      case "number": {
        if (this.word() === 0) return null;
        const low = this.word();
        return this.word() * 2 ** 32 + low;
      }
      case "title":
        return this.word() === 0 ? null : { value: this.string(), kind: ENUMS.titleKind[this.word()] };
      case "tag":
        return { span: this.span(), name: this.string(), raw: this.string() };
      case "attributes":
        return this.list(() => ({ name: this.string(), value: this.optionalString() }));
      case "alignments":
        return this.list(() => ENUMS.tableAlignment[this.word()]);
      case "codeBlockKind":
        return this.word() === 0
          ? { type: "indented" }
          : { type: "fenced", marker: ENUMS.fenceMarker[this.word()], length: this.word() };
      case "mathKind":
        return this.word() === 0 ? { type: "code" } : { type: "dollar", dollars: this.word() };
      case "htmlContent":
        return this.word() === 0
          ? { blocks: this.list(() => this.node()) }
          : { inlines: this.list(() => this.node()) };
      default:
        return ENUMS[type][this.word()];
    }
  }

  diagnostics() {
    return this.list(() => ({
      severity: camel(this.string()),
      code: camel(this.string()),
      span: this.span(),
      message: this.string(),
    }));
  }
}

/** Builds one message of the wire encoding, checking each node it takes. */
class Writer {
  constructor() {
    this.words = [];
    this.strings = [];
  }

  string(value) {
    this.words.push(this.strings.length);
    this.strings.push(value);
  }

  finish() {
    let unitCount = 0;
    for (const value of this.strings) unitCount += value.length;
    const unitsStart = 8 + 4 * this.words.length + 8 * this.strings.length;
    const buffer = new ArrayBuffer(unitsStart + 2 * unitCount);
    const head = new Uint32Array(buffer, 0, 2 + this.words.length + 2 * this.strings.length);
    head[0] = this.words.length;
    head[1] = this.strings.length;
    head.set(this.words, 2);
    const units = new Uint16Array(buffer, unitsStart, unitCount);
    let at = 0;
    let entry = 2 + this.words.length;
    for (const value of this.strings) {
      head[entry++] = at;
      head[entry++] = value.length;
      for (let index = 0; index < value.length; index++) units[at++] = value.charCodeAt(index);
    }
    return new Uint8Array(buffer);
  }
}

function fail(path, message) {
  throw new TypeError(`markdown-syntax: malformed tree at [${path.join(", ")}]: ${message}`);
}

function isIndex(value) {
  return Number.isInteger(value) && value >= 0 && value < NO_SPAN;
}

/** Encodes `node`, at `path`, as one of the kinds `accepts` takes. */
function encodeNode(writer, node, path, accepts, what) {
  if (node === null || typeof node !== "object") fail(path, `${what} is not an object`);
  const kind = BY_NAME.get(node.type);
  if (!kind) fail(path, `unknown type ${JSON.stringify(node.type)}`);
  if (!accepts(kind.number)) fail(path, `a ${node.type} where ${what} belongs`);
  writer.words.push(kind.number);
  encodeSpan(writer, node.span, path, node.type);
  for (const [field, type] of kind.fields) encodeField(writer, node, field, type, path);
}

function encodeSpan(writer, span, path, what) {
  if (span === null || span === undefined) {
    writer.words.push(NO_SPAN, 0);
  } else if (Array.isArray(span) && span.length === 2 && isIndex(span[0]) && isIndex(span[1])) {
    writer.words.push(span[0], span[1]);
  } else {
    fail(path, `the span of the ${what} is not [from, to] or null`);
  }
}

function encodeField(writer, node, field, type, path) {
  const value = node[field];
  const missing = (expected) => fail(path, `the ${node.type} needs ${field}, ${expected}`);
  const string = (text, expected = "a string") => {
    if (typeof text !== "string") missing(expected);
    writer.string(text);
  };
  const optionalString = (text) => {
    if (text === null || text === undefined) {
      writer.words.push(0);
    } else if (typeof text === "string") {
      writer.string(text);
      writer.words[writer.words.length - 1] += 1;
    } else {
      missing("a string or null");
    }
  };
  const enumerated = (values, item, expected) => {
    const index = values.indexOf(item);
    if (index < 0) missing(expected ?? `one of ${values.join(", ")}`);
    writer.words.push(index);
  };
  const list = (items, each) => {
    if (!Array.isArray(items)) missing("an array");
    writer.words.push(items.length);
    items.forEach(each);
  };
  switch (type) {
    case "blocks":
    case "inlines":
    case "items":
    case "rows":
    case "cells":
      return list(value, (child, index) =>
        encodeNode(writer, child, [...path, index], LISTS[type], `a ${field} entry`),
      );
    case "string":
      return string(value);
    case "optionalString":
      return optionalString(value);
    case "boolean":
      if (typeof value !== "boolean") missing("a boolean");
      return writer.words.push(value ? 1 : 0);
    case "depth":
      if (!Number.isInteger(value) || value < 1 || value > 6) missing("an integer from 1 to 6");
      return writer.words.push(value);
    case "char": {
      const point = typeof value === "string" ? value.codePointAt(0) : undefined;
      if (point === undefined || String.fromCodePoint(point) !== value) missing("one char");
      return writer.words.push(point);
    }
    case "checked":
      if (value === null || value === undefined) return writer.words.push(0);
      if (typeof value !== "boolean") missing("a boolean or null");
      return writer.words.push(value ? 2 : 1);
    case "number":
      if (value === null || value === undefined) return writer.words.push(0);
      if (!Number.isSafeInteger(value) || value < 0) missing("a non-negative integer or null");
      return writer.words.push(1, value % 2 ** 32, Math.floor(value / 2 ** 32));
    case "title":
      if (value === null || value === undefined) return writer.words.push(0);
      if (typeof value !== "object") missing("{ value, kind } or null");
      writer.words.push(1);
      string(value.value, "{ value, kind } or null");
      return enumerated(ENUMS.titleKind, value.kind, "{ value, kind } or null");
    case "tag":
      if (value === null || typeof value !== "object") missing("{ span, name, raw }");
      encodeSpan(writer, value.span, path, `${field} tag`);
      string(value.name, "{ span, name, raw }");
      return string(value.raw, "{ span, name, raw }");
    case "attributes":
      return list(value, (attribute) => {
        if (attribute === null || typeof attribute !== "object") missing("[{ name, value }]");
        string(attribute.name, "[{ name, value }]");
        optionalString(attribute.value);
      });
    case "alignments":
      return list(value, (alignment) => enumerated(ENUMS.tableAlignment, alignment));
    case "codeBlockKind":
      if (value?.type === "indented") return writer.words.push(0);
      if (value?.type !== "fenced" || !Number.isInteger(value.length) || value.length < 0) {
        missing("{ type: 'indented' } or { type: 'fenced', marker, length }");
      }
      writer.words.push(1);
      enumerated(ENUMS.fenceMarker, value.marker);
      return writer.words.push(value.length);
    case "mathKind":
      if (value?.type === "code") return writer.words.push(0);
      if (value?.type !== "dollar" || !Number.isInteger(value.dollars) || value.dollars < 1) {
        missing("{ type: 'code' } or { type: 'dollar', dollars }");
      }
      return writer.words.push(1, value.dollars);
    case "htmlContent":
      if (Array.isArray(value?.blocks)) {
        writer.words.push(0);
        return list(value.blocks, (child, index) =>
          encodeNode(writer, child, [...path, index], LISTS.blocks, "a block"),
        );
      }
      if (Array.isArray(value?.inlines)) {
        writer.words.push(1);
        return list(value.inlines, (child, index) =>
          encodeNode(writer, child, [...path, index], LISTS.inlines, "an inline"),
        );
      }
      return missing("{ blocks } or { inlines }");
    default:
      return enumerated(ENUMS[type], value);
  }
}

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

  constructor(binding, handle, source, tree, diagnostics) {
    this.#binding = binding;
    this.#handle = handle;
    binding.track(this, handle);
    /** The source the document was parsed from. */
    this.source = source;
    /** The document's tree. */
    this.tree = tree;
    /** The parser's diagnostics. */
    this.diagnostics = diagnostics;
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
   * The inline content the parser read for the paragraph or heading at
   * `path`, with `toSource(offset)` giving the source offset of a text
   * offset and `spanToSource(span)` the source span of a text span, as the
   * parser maps a leaf's spans; or null.
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
    const reader = new Reader(this.#binding.result());
    const text = reader.string();
    const starts = reader.words.slice(1, text.length + 2);
    const ends = reader.words.slice(text.length + 2);
    const check = (offset) => {
      if (!Number.isInteger(offset) || offset < 0 || offset > text.length) {
        throw new RangeError(`markdown-syntax: ${offset} is not an offset of the text`);
      }
    };
    // An offset between the halves of a surrogate pair maps after the first.
    const mapped = (table, offset) => (table[offset] === NO_SPAN ? table[offset - 1] + 1 : table[offset]);
    return {
      text,
      toSource(offset) {
        check(offset);
        return mapped(starts, offset);
      },
      spanToSource(span) {
        if (!Array.isArray(span) || span.length !== 2) {
          throw new TypeError("markdown-syntax: a span is [from, to]");
        }
        span.forEach(check);
        const from = mapped(starts, span[0]);
        return [from, Math.max(from, mapped(ends, span[1]))];
      },
    };
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
    const message = writer.finish();
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
      this.#binding.withBytes(message, (pointer, length) =>
        exports.to_markdown(handle, pointer, length, ...settings),
      ),
    );
    const result = this.#binding.result();
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
    const bytes = new TextEncoder().encode(String(source));
    const handle = this.call((exports) => this.withBytes(bytes, (pointer, length) => exports.parse(pointer, length)));
    const reader = new Reader(this.result());
    const tree = reader.node();
    const diagnostics = reader.diagnostics();
    return new ParsedDocument(this, handle, String(source), tree, diagnostics);
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
    const writer = new Writer();
    writer.words.push(labels.length);
    for (const label of labels) writer.string(label);
    const message = writer.finish();
    const bytes = new TextEncoder().encode(String(text));
    const status = this.call((exports) =>
      this.withBytes(bytes, (pointer, length) =>
        this.withBytes(message, (labelsPointer, labelsLength) =>
          exports.parse_inline(pointer, length, labelsPointer, labelsLength),
        ),
      ),
    );
    if (status !== 0) throw new TypeError(`markdown-syntax: ${UTF8.decode(this.result())}`);
    const reader = new Reader(this.result());
    const inlines = reader.list(() => reader.node());
    return { inlines, diagnostics: reader.diagnostics() };
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
    const exports = this.#exports;
    return new Uint8Array(exports.memory.buffer, exports.result_ptr(), exports.result_len()).slice();
  }
}
