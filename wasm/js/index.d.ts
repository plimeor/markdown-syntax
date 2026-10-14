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
  | ContainerDirective;

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
export interface HtmlTag {
  span: Span;
  name: string;
  raw: string;
}

export interface Paragraph extends Node<"paragraph"> {
  children: Inline[];
}
export interface Heading extends Node<"heading"> {
  depth: 1 | 2 | 3 | 4 | 5 | 6;
  kind: "atx" | "setext";
  children: Inline[];
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
  start: number | null;
  delimiter: "dash" | "asterisk" | "plus" | "period" | "paren";
  tight: boolean;
  children: ListItem[];
}
export interface ListItem extends Node<"listItem"> {
  checked: boolean | null;
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
}
export interface ContainerDirective extends Node<"containerDirective"> {
  name: string;
  label: Inline[];
  attributes: DirectiveAttribute[];
  children: Block[];
}

export interface Text extends Node<"text"> {
  value: string;
}
export interface Escape extends Node<"escape"> {
  value: string;
}
export interface CharacterReference extends Node<"characterReference"> {
  reference: string;
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
  /** The labels of the document's link reference definitions. */
  readonly definitions: string[];
  /** The inline source of the paragraph or heading at `path`, or null. */
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
  ): { inlines: Inline[]; diagnostics: Diagnostic[] };
}
