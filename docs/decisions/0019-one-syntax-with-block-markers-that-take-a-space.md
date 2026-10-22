# 0019: One normative syntax, with block markers that take a space

Status: Accepted
Date: 2026-10-21
Supersedes: 0018

## Context

0018 fixed two parse entry points and one syntax, CommonMark with
CJK-friendly emphasis and the extensions Settle uses. CommonMark lets a
block marker alone at the end of its line open an empty block: `#` is an
empty heading, `-`, `*`, `+`, `1.`, and `1)` an empty list item, and `>` an
empty block quote, and a list item may start with a blank line, so `-`
followed by an indented line puts that line in the item. It also reads a
paragraph followed by a line of `=` or `-` as a setext heading. In Settle's
editor a user who types a paragraph `-` or `#`, or writes `---` or `===`
under a line of text, sees the paragraph turn into another block, because
the editor shows the blocks the parse finds.

## Decision

There are two parse entry points, and both read one fixed syntax:
`parse(input)` reads a document, and `parse_inline(text, definitions)` reads
the inline content of one leaf block, applying no block rule and forming
reference links only for the labels `definitions` holds. The syntax is:

- CommonMark, with raw HTML and indented code, except for its block markers
  and setext headings, as below, where delimiter runs flank by the
  CommonMark CJK-friendly amendments
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

Block markers take a space:

- A block marker opens its block only when a space or a tab follows it on
  its line: an ATX heading's `#` to `######`, a bullet list marker `-`, `*`,
  or `+`, an ordered list marker `1.` or `1)`, and the `>` that opens a
  block quote, an alert's included. A marker that the line's end or any
  other char follows is paragraph text, as `#a` already is: `#`, `##`, `-`,
  `1.`, `>`, `>a`, and `>[!NOTE]` are paragraphs, and `>>` opens no quote,
  where `> >` opens two.
- An empty block is its marker and the space: `# ` is an empty heading,
  `- ` and `1. ` an empty item, `> ` an empty block quote. A list item whose
  marker line holds nothing after the marker but spaces and tabs is empty,
  and takes no line after it: no list item starts with a blank line. As in
  CommonMark, an empty item does not interrupt a paragraph.
- Inside an open block quote, a line continues the quote with `>` whether a
  space follows it or not: a bare `>` is a blank line in the quote and
  `>text` a line of it. Only opening a quote takes the space.

There are no setext headings: a line of `=` after a paragraph is the
paragraph's text, and a line of `-` is its text or, from three `-` on, a
thematic break. A run of `-` alone, without a pipe or a colon, delimits no
table. `Heading` has no kind, since every heading is an ATX heading.

Link text holds no link: an autolink inside link text, image alt included,
reads as its text, an angle-bracket one without its brackets, and a wiki link,
also inside a text directive's label, keeps the brackets around it from forming
a link. A tag is not a link, so link text holds tags.

Subscript, superscript, insert, spoiler, underline, description lists,
single-tilde strikethrough, relaxed literal autolinks, setext headings, and
MDX are not part of the syntax, and their AST nodes and fields do not exist.
The syntax is hard-coded: no public or private configuration value selects
constructs.

## Considered options

- Keep CommonMark's markers and have the editor escape a paragraph on write
  (`\-`, `\#`, `\>`): a paragraph the user typed should not read as another
  block, in Settle or in any other reader of the file.
- Keep the empty-block exception, where a marker alone at the end of its
  line still opens an empty heading, item, or quote: the lone `-`, `#`, or
  `>` a user types is the case the rule is for.
- Require the space on a quote's later lines too, and write a blank line in
  a quote as `> `: prosemirror-markdown, mdast-util-to-markdown (remark),
  Tiptap, Mark Text, and Prettier all write that blank line as a bare `>`,
  and Prettier rewrites `> ` to `>`, so a quote from those tools would end
  at its first blank line.
- Continue a quote with a bare `>` but not with `>text`: both continue it.
- Let a run of `-` alone delimit a one-column table once setext underlines
  are gone: `a\n---` would be a table, not a paragraph and a thematic
  break.
- From 0018: CommonMark flanking, under which CJK writers must add a space
  or a zero-width space after a closing delimiter, which shows in the text
  or hides in it; the CJK-friendly amendments as an option of `parse`, where
  the syntax has no configuration; the amendments for `*` and `_` only,
  under which `~~重点。~~然后` would stay text where `**重点。**然后` forms;
  a CJK set generated from the crate's Unicode 15.1.0 tables, which lack the
  emoji data the set needs.
- From 0018: `parse` as the only entry point, re-parsing the whole document
  per keystroke (3–6 ms for 124 KB in wasm); a paragraph parsed alone with
  `parse`, where block rules apply and reference links lose their
  definitions; presets with a normative default, or a crate-private
  constant `Constructs`, which keep a configuration space and dead branches
  alive.
- From 0018: relaxed literal-autolink extents for `http(s)://` and `www.`,
  which match only comrak's relaxed mode; blocking a bare text directive
  only before `@` or `.letter`, which keeps `(:note)` and `:h1` as
  directives; a defined reference label beating a wiki link, under which a
  reading would depend on definitions elsewhere; nesting an autolink's link
  inside link text, which the AST, validation, and the serializer rule out.
- From 0018: leaving tags to the caller, which the editor cannot get from
  the tree it renders; meowdown's tag rule, which also reads `#1a`; reading
  `#` in the inline scanner, before delimiter pairing has decided whether
  `*` is transparent.

## Consequences

- A document that used a setext heading reads it as a paragraph, followed by
  a thematic break when the underline is three `-` or more; one that used a
  lone marker reads a paragraph, and a list item that started with a blank
  line reads as an empty item, or a paragraph, followed by its former
  content. A document's source is unchanged; only its tree differs.
- `HeadingKind` and `Heading::kind` are gone, and `Heading::with_source`
  takes no kind; the wasm binding's heading has no `kind`. This is a
  breaking change.
- The serializer writes an empty heading `# `, an empty item `- `, and an
  empty quote, or one whose first line is blank, opening with `> `; a blank
  line later in a quote stays a bare `>`.
- An item's content always starts on its marker's line, so an item whose
  first line would read as more space after the marker, or as one thematic
  break with a `-` or `*` bullet (a paragraph `--`, a `---` break, three
  nested empty items), has no spelling, and validation refuses it.
- Text beside CJK characters that read as literal delimiters may read as
  emphasis, strikethrough, or highlight.
- A caller who relied on a removed construct reads it as text.
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
