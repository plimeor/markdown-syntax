# 0018: One normative syntax, with CJK-friendly emphasis

Status: Superseded by 0019
Date: 2026-10-18
Supersedes: 0015

## Context

0015 fixed two parse entry points and one syntax, with tags. Its emphasis is
CommonMark's, whose flanking rules treat CJK punctuation like ASCII
punctuation: `这是**重点。**然后` stays text, because the closing `**` follows
`。` and precedes `然`, and Chinese and Japanese put no space there. Settle's
notes are mostly Chinese, and the editor shows the bold the parse finds, so
such bold silently fails to form. The markdown-cjk-friendly amendments fix
flanking beside CJK characters; micromark and markdown-it implement them, and
they leave every CommonMark example unchanged.

## Decision

There are two parse entry points, and both read one fixed syntax:
`parse(input)` reads a document, and `parse_inline(text, definitions)` reads
the inline content of one leaf block, applying no block rule and forming
reference links only for the labels `definitions` holds. The syntax is:

- CommonMark, with raw HTML and indented code, where delimiter runs flank
  by the CommonMark CJK-friendly amendments
  (<https://github.com/tats-u/markdown-cjk-friendly>, at commit `5b17173`,
  Unicode 18 ranges) for `*`, `_`, `~~`, and `==` alike, reading `‘’“”`
  with U+FE01 before a run as CJK punctuation as
  `micromark-extension-cjk-friendly` does;
- `<details>` HTML containers;
- GFM tables, task list items, literal autolinks (the strict GFM extents,
  plus `mailto:` and `xmpp:`; as in cmark-gfm, a `www.`, `http://`, or
  `https://` literal does not form after a link or image `[` that no `]` has
  closed yet, while an email address still links there), and `~~`
  strikethrough;
- footnote definitions, references, and inline footnotes `^[…]`, whose `^[`
  opens no link text, so a URL inside one links;
- GFM alerts, frontmatter, shortcodes, `==` highlight;
- wiki links with the title after the pipe, whose content holds no unescaped
  `[` or `]`, and which win over a defined reference label;
- inline and block math;
- directives, whose names are runs of ASCII letters joined by single `-`, and
  whose text form is followed by whitespace or the end of the content, or by
  ASCII punctuation when it has a non-empty label or non-blank attribute
  braces;
- tags: `#` followed by a Unicode letter and then any letters, numbers, `/`,
  `_`, or `-`, where the character before the `#` is whitespace or the start
  of the leaf's inline content. The delimiters of emphasis, strong,
  strikethrough, and highlight are transparent to that test; the edges of a
  link, image, inline footnote, or text directive, and every other inline
  node, are not whitespace, and nothing inside code, math, autolinks, wiki
  links, raw HTML, escapes, character references, shortcodes, or footnote
  references is read.

Link text holds no link: an autolink inside link text, image alt included,
reads as its text, an angle-bracket one without its brackets, and a wiki link,
also inside a text directive's label, keeps the brackets around it from forming
a link. A tag is not a link, so link text holds tags.

Subscript, superscript, insert, spoiler, underline, description lists,
single-tilde strikethrough, relaxed literal autolinks, and MDX are not part of
the syntax, and their AST nodes do not exist. The syntax is hard-coded: no
public or private configuration value selects constructs.

## Considered options

- Keep CommonMark flanking: CJK writers must add a space or a zero-width
  space after a closing delimiter, which shows in the text or hides in it.
- Make the amendments an option of `parse`: the syntax is hard-coded and has
  no configuration, and Settle, the crate's user, wants them on.
- Apply the amendments to `*` and `_` only: `~~重点。~~然后` would stay text
  where `**重点。**然后` forms, and the reference applies them to
  strikethrough too.
- Generate the CJK set from the Unicode tables the crate already uses
  (15.1.0): the set excludes default emoji presentation characters, which
  needs emoji data those tables do not carry; the amendments publish the set.
- Keep `parse` as the only entry point and re-parse the whole document per
  keystroke: 3–6 ms for 124 KB in wasm, and the editor must rebuild the
  file's text before every parse.
- Parse a paragraph alone with `parse`: block rules apply to it, and
  reference links lose their definitions.
- Keep the presets and add a normative one as the default: the configuration
  space, the profile-by-profile tests, and the serializer's per-syntax read
  back stay.
- Keep a crate-private constant `Constructs`: dead branches stay alive for a
  configuration that no longer exists.
- Keep relaxed literal-autolink extents for `http(s)://` and `www.`: they
  match only comrak's relaxed mode.
- Block a bare text directive only before `@` or `.letter`: narrower, and it
  keeps `(:note)` and `:h1` as directives.
- Let a defined reference label beat a wiki link: a reading would depend on
  definitions elsewhere in the document.
- Nest an autolink's link inside link text, as cmark-gfm, commonmark.js, and
  micromark do for `[this <http://and.com> that](url)`: link text would hold
  a link, which the AST, validation, and the serializer otherwise rule out.
- Leave tags to the caller: the editor and the index read them separately,
  and the editor cannot get them from the tree it renders.
- meowdown's tag rule, which also reads a tag that starts with a digit, such
  as `#1a`: Settle's spec decided against it.
- Recognize `#` in the inline scanner: whether `*` is transparent depends on
  delimiter pairing, which the scanner has not done yet.

## Consequences

- Text beside CJK characters that read as literal delimiters may read as
  emphasis, strikethrough, or highlight; a document's source is unchanged,
  and only its tree differs.
- A caller who relied on a removed construct reads it as text.
- Text that held a tag holds a `Tag` node; a caller that read tags from text
  reads the nodes.
- Text that would form a tag where it is written is the builder's to escape
  with `Escape('#')`, as for any shape that depends on its neighbours. A
  `Tag` the builder places after a character that is not whitespace, or
  before a character a name can hold, is written as it is and reads back
  differently; validation checks only the name.
- Tests assert one result per input; fixture profiles and preset loops go.
- An oracle case that this syntax reads differently by decision leaves the
  conformance bench; its input is checked exactly in
  `tests/fixtures/syntax_decisions/`, with HTML verified against a reference
  renderer or the decision.
