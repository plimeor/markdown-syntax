# 0015: Parse entry points and one normative syntax, with tags

Status: Accepted
Date: 2026-10-14
Supersedes: 0012

## Context

0012 fixed one syntax and two entry points that read it. Settle, the crate's
user, reads `#tag` from note bodies with its own pattern over a flattened copy
of the inline tree. Its editor styles text from this crate's parse, so the
editor cannot show the tags the index stores, and the two readings drift
whenever either changes. A tag is syntax in Settle's notes: it decides what a
`#` means in the text around it, as an autolink decides what `www.` means.

Settle's `tags` spec decides the rule. meowdown reads a tag that starts with
a digit; Settle does not, and Settle is the crate's user.

## Decision

There are two parse entry points, and both read one fixed syntax:
`parse(input)` reads a document, and `parse_inline(text, definitions)` reads
the inline content of one leaf block, applying no block rule and forming
reference links only for the labels `definitions` holds. The syntax is:

- CommonMark, with raw HTML and indented code;
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
