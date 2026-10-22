// Types of `index.js`: the trees mirror the Rust AST of markdown-syntax,
// with fields in camelCase and every span in UTF-16 code units.

/** `[from, to]` in UTF-16 code units, or null for a node without a span. */
export type Span = [number, number] | null;

interface Node<T extends string> {
  type: T;
  span: Span;
}

export interface Document extends Node<"document"> {
  children: Block[];
}

export type Block =
  | Paragraph
  | Heading
  | ThematicBreak
  | BlockQuote
  | Alert
  | List
  | CodeBlock
  | HtmlBlock
  | HtmlContainer
  | Definition
  | FootnoteDefinition
  | Table
  | MathBlock
  | Frontmatter
  | LeafDirective
  | ContainerDirective
  | BlankLine;

export type Inline =
  | Text
  | Escape
  | CharacterReference
  | Emphasis
  | Strong
  | Delete
  | Mark
  | Shortcode
  | Code
  | Link
  | Autolink
  | Image
  | LinkReference
  | ImageReference
  | Html
  | SoftBreak
  | LineBreak
  | MathInline
  | FootnoteReference
  | InlineFootnote
  | WikiLink
  | TextDirective
  | Tag;

export type DestinationKind = "bare" | "angle" | "omitted";
export type ReferenceKind = "full" | "collapsed" | "shortcut";
export type EmphasisDelimiter = "asterisk" | "underscore";
export interface Title {
  value: string;
  kind: "doubleQuote" | "singleQuote" | "paren";
}
export interface DirectiveAttribute {
  name: string;
  value: string | null;
}
/** What part of its node a token is. */
export type TokenRole =
  | "text"
  | "syntax"
  | "destination"
  | "title"
  | "label"
  | "target"
  | "name"
  | "attributes";
/**
 * A run of an inline parse's text: the innermost inline that holds it, as an
 * index into `nodes`, and the part of that inline it is. The tokens tile the
 * text in order.
 */
export interface InlineToken {
  span: [number, number];
  node: number;
  role: TokenRole;
}
export interface HtmlTag {
  span: Span;
  name: string;
  raw: string;
}

export interface Paragraph extends Node<"paragraph"> {
  children: Inline[];
  /** In a tree for `toMarkdown`: the text to build the content from; `children` is then ignored and may be `[]`. */
  source?: string;
}
export interface Heading extends Node<"heading"> {
  depth: 1 | 2 | 3 | 4 | 5 | 6;
  children: Inline[];
  /** In a tree for `toMarkdown`: the text to build the content from; `children` is then ignored and may be `[]`. */
  source?: string;
}
export interface ThematicBreak extends Node<"thematicBreak"> {
  marker: "dash" | "asterisk" | "underscore";
}
export interface BlockQuote extends Node<"blockQuote"> {
  children: Block[];
}
export interface Alert extends Node<"alert"> {
  kind: "note" | "tip" | "important" | "warning" | "caution";
  title: string | null;
  children: Block[];
}
export interface List extends Node<"list"> {
  ordered: boolean;
  delimiter: "dash" | "asterisk" | "plus" | "period" | "paren";
  tight: boolean;
  children: ListItem[];
}
export interface ListItem extends Node<"listItem"> {
  checked: boolean | null;
  /** The item's number in an ordered list; null follows the previous item (1 for the first). */
  number: number | null;
  children: Block[];
}
export interface CodeBlock extends Node<"codeBlock"> {
  kind: { type: "indented" } | { type: "fenced"; marker: "backtick" | "tilde"; length: number };
  info: string | null;
  value: string;
}
export interface HtmlBlock extends Node<"htmlBlock"> {
  value: string;
}
export interface HtmlContainer extends Node<"htmlContainer"> {
  opening: HtmlTag;
  content: { blocks: Block[] } | { inlines: Inline[] };
  closing: HtmlTag;
}
export interface Definition extends Node<"definition"> {
  label: string;
  identifier: string;
  destination: string;
  destinationKind: DestinationKind;
  title: Title | null;
}
export interface FootnoteDefinition extends Node<"footnoteDefinition"> {
  label: string;
  identifier: string;
  children: Block[];
}
export interface Table extends Node<"table"> {
  alignments: ("none" | "left" | "center" | "right")[];
  rows: TableRow[];
}
export interface TableRow extends Node<"tableRow"> {
  cells: TableCell[];
}
export interface TableCell extends Node<"tableCell"> {
  children: Inline[];
  /** In a tree for `toMarkdown`: the text to build the content from; `children` is then ignored and may be `[]`. */
  source?: string;
}
export interface MathBlock extends Node<"mathBlock"> {
  value: string;
}
export interface Frontmatter extends Node<"frontmatter"> {
  kind: "yaml" | "toml";
  value: string;
}
export interface LeafDirective extends Node<"leafDirective"> {
  name: string;
  label: Inline[];
  attributes: DirectiveAttribute[];
  /** In a tree for `toMarkdown`: the text to build the label from; `label` is then ignored and may be `[]`. */
  source?: string;
}
export interface ContainerDirective extends Node<"containerDirective"> {
  name: string;
  label: Inline[];
  attributes: DirectiveAttribute[];
  children: Block[];
  /** In a tree for `toMarkdown`: the text to build the label from; `label` is then ignored and may be `[]`. */
  source?: string;
}
/** A blank line beyond the one that separates two blocks. */
export interface BlankLine extends Node<"blankLine"> {}

export interface Text extends Node<"text"> {
  value: string;
}
export interface Escape extends Node<"escape"> {
  value: string;
}
export interface CharacterReference extends Node<"characterReference"> {
  reference: string;
  /**
   * The character the reference stands for, as Rust's `CharacterReference::value()`
   * decodes it; null for a reference that does not decode. Every tree the binding
   * returns holds it; `toMarkdown` ignores it, so a tree built for it may leave it out.
   */
  value?: string | null;
}
export interface Emphasis extends Node<"emphasis"> {
  delimiter: EmphasisDelimiter;
  children: Inline[];
}
export interface Strong extends Node<"strong"> {
  delimiter: EmphasisDelimiter;
  children: Inline[];
}
export interface Delete extends Node<"delete"> {
  children: Inline[];
}
export interface Mark extends Node<"mark"> {
  children: Inline[];
}
export interface Shortcode extends Node<"shortcode"> {
  name: string;
  /**
   * The emoji the shortcode stands for, as Rust's `Shortcode::glyph()` gives it from
   * the pinned gemoji table; null for a name the table does not hold. Every tree the
   * binding returns holds it; `toMarkdown` ignores it, so a tree built for it may leave it out.
   */
  glyph?: string | null;
}
export interface Tag extends Node<"tag"> {
  /** The name as written, without the `#`; `/` separates levels. */
  name: string;
}
export interface Code extends Node<"code"> {
  value: string;
}
export interface Link extends Node<"link"> {
  destination: string;
  destinationKind: DestinationKind;
  title: Title | null;
  children: Inline[];
}
export interface Autolink extends Node<"autolink"> {
  form: "angle" | "literal";
  text: string;
}
export interface Image extends Node<"image"> {
  destination: string;
  destinationKind: DestinationKind;
  title: Title | null;
  alt: Inline[];
}
export interface LinkReference extends Node<"linkReference"> {
  identifier: string;
  label: string;
  kind: ReferenceKind;
  children: Inline[];
}
export interface ImageReference extends Node<"imageReference"> {
  identifier: string;
  label: string;
  kind: ReferenceKind;
  alt: Inline[];
}
export interface Html extends Node<"html"> {
  value: string;
}
export interface SoftBreak extends Node<"softBreak"> {}
export interface LineBreak extends Node<"lineBreak"> {
  kind: "backslash" | "spaces";
}
export interface MathInline extends Node<"math"> {
  value: string;
  kind: { type: "code" } | { type: "dollar"; dollars: number };
}
export interface FootnoteReference extends Node<"footnoteReference"> {
  label: string;
  identifier: string;
}
export interface InlineFootnote extends Node<"inlineFootnote"> {
  children: Inline[];
}
export interface WikiLink extends Node<"wikiLink"> {
  target: string;
  label: string;
  embed: boolean;
}
export interface TextDirective extends Node<"textDirective"> {
  name: string;
  label: Inline[];
  attributes: DirectiveAttribute[];
}

export interface Diagnostic {
  severity: "warning" | "error";
  /** The Rust `DiagnosticCode` in camelCase, such as `invalidDocument`. */
  code: string;
  span: Span;
  /** The path of the node a validation diagnostic is about; `null` for a parser diagnostic. */
  path: number[] | null;
  message: string;
}

export interface InlineSource {
  /** The text the inline pass read for the leaf. */
  text: string;
  /** The source offset of a text offset; `text.length` maps to the end. */
  toSource(offset: number): number;
  /** The source span of a span of the text, as the parser maps a leaf's. */
  spanToSource(span: [number, number]): [number, number];
}

export interface SerializeOptions {
  lineEnding?: "lf" | "crlf";
  finalNewline?: boolean;
  bullet?: "dash" | "asterisk" | "plus" | null;
  orderedDelimiter?: "period" | "paren" | null;
  fenceMarker?: "backtick" | "tilde" | null;
}

export class ParsedDocument {
  private constructor();
  /** The source the document was parsed from. */
  readonly source: string;
  readonly tree: Document;
  readonly diagnostics: Diagnostic[];
  /**
   * The path and inline source of every leaf, in document order. A path is
   * child indices from the document (a list item counts as its list's
   * child); a table cell's is its table's, then its row's index and its own;
   * a directive's label has its directive's path.
   */
  readonly leafSources: { path: number[]; source: InlineSource }[];
  /** The labels of the document's link reference definitions. */
  readonly definitions: string[];
  /**
   * The inline source of the leaf at `path` (a paragraph, a heading, a table
   * cell, or a directive's label), or null.
   */
  inlineSource(path: number[]): InlineSource | null;
  /**
   * Writes `tree` with this document's source; `{ diagnostics }` when
   * validation refuses it. Throws a TypeError for a malformed tree.
   */
  toMarkdown(tree?: Document, options?: SerializeOptions): string | { diagnostics: Diagnostic[] };
  /** Releases the document in WebAssembly; later calls on it throw. */
  free(): void;
}

export class MarkdownSyntax {
  private constructor();
  static load(
    wasm: BufferSource | WebAssembly.Module | Response | PromiseLike<BufferSource | WebAssembly.Module | Response>,
  ): Promise<MarkdownSyntax>;
  parse(source: string): ParsedDocument;
  parseInline(
    text: string,
    definitions?: string[] | ParsedDocument,
  ): {
    inlines: Inline[];
    tokens: InlineToken[];
    /** Every inline of `inlines`, nested ones included, in pre-order: what a token's `node` indexes. */
    nodes: Inline[];
    diagnostics: Diagnostic[];
  };
}
