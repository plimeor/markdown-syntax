# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/plimeor/markdown-syntax/compare/v0.3.0...v0.4.0) - 2026-10-08

### Added

- [**breaking**] read one normative syntax and render Markdown without reading it back ([#16](https://github.com/plimeor/markdown-syntax/pull/16))
- [**breaking**] rebuild block parsing and serializer escaping on one source of syntax rules ([#15](https://github.com/plimeor/markdown-syntax/pull/15))

### Fixed

- map spans to their source and fix CommonMark parse and round-trip defects ([#12](https://github.com/plimeor/markdown-syntax/pull/12))

### Migration

This release reads one fixed Markdown syntax, records in the AST the spelling
a node was written with, writes Markdown by fixed rules, and rebuilds block
parsing. Code that configures the parser, matches on the AST, builds nodes
with struct literals, or compares canonical output needs these updates.

- **One syntax, one entry point.** `parse(input)` is the only way to parse.
  It reads CommonMark with raw HTML and indented code, `<details>` HTML
  containers, GFM tables, task list items, literal autolinks, and `~~`
  strikethrough, footnotes and inline footnotes `^[…]`, GitHub alerts,
  frontmatter, gemoji shortcodes, `==` highlight, wiki links (title after the
  pipe), math, and directives. The `options` module is removed, with
  `SyntaxOptions` and its presets, `Constructs`, `Construct`, `ParseOptions`,
  `WikiLinkOrder`, `SyntaxConfigError`, `SyntaxOptions::parse`,
  `parse_strict`, and `ParseStrictError`.
- **Removed constructs.** Subscript, superscript, insert, spoiler, underline,
  description lists, single-tilde strikethrough, non-HTTP and scheme-less
  literal autolinks, and MDX ESM, JSX, and expressions are not recognized.
  Their source reads as the one syntax reads it: `H~2~O`, `x^2^`, `++a++`,
  `||b||`, and `~a~` are text, `__a__` is strong, `Term` followed by
  `: Detail` is one paragraph, `import x` and `{x}` are paragraphs, and
  `<X />` is raw HTML.
- **Removed AST items.** `Block::DescriptionList` with `DescriptionList`,
  `DescriptionItem`, and `DescriptionDetails`; `Block::MdxEsm`,
  `Block::MdxExpression`, and `Block::MdxJsx` with their node types;
  `Inline::Underline`, `Inline::Insert`, `Inline::Subscript`,
  `Inline::Superscript`, `Inline::Spoiler`, `Inline::MdxExpression`, and
  `Inline::MdxJsx` with their node types; `Delete.marker` with
  `DeleteMarker`; and `WikiLink.label_order` with `WikiLinkLabelOrder`.
- **Removed diagnostic codes.** `DiagnosticCode::StrictParse` and
  `DiagnosticCode::InvalidMdx`.
- **New diagnostic code.** `DiagnosticCode::InvalidDirectiveAttribute` warns
  about a directive attribute without a valid name, which is dropped.
- **Recorded spellings.** `Emphasis` and `Strong` gain
  `delimiter: EmphasisDelimiter` (`Asterisk` or `Underscore`). The parser
  fills it from the source and `EmphasisDelimiter` defaults to `Asterisk`.
  Struct literals of these nodes need the new field.
- **Each fact is stored once.**
  - `Autolink` holds `form: AutolinkForm` (`Angle` or `Literal`) and `text`,
    the URL or address as written; `Autolink::destination()` derives the
    href (`www.a.b` → `http://www.a.b`, `a@b.c` → `mailto:a@b.c`), and
    returns `None` when `text` is not exactly one autolink of its form.
    `Autolink.destination`, `Autolink.kind`, and `AutolinkKind` are removed.
    `Autolink::new(form, text)` builds one.
  - `CodeInline` holds only its `value`: `raw` and `fence_length` are
    removed, and the serializer picks the fence and padding.
  - `CharacterReference` holds only its `reference` as written:
    `CharacterReference.value` is removed, and
    `CharacterReference::value()` decodes the reference, or returns `None`
    when it is not exactly one character reference.
    `CharacterReference::new(reference)` builds one.
  - `Link`, `Image`, and `Definition` hold `title: Option<Title>`, where
    `Title { value, kind }` keeps the title text with its `LinkTitleKind`;
    the separate `title_kind` field is removed, so a title without quotes,
    which was dropped when written, cannot be built. `Title::new(value,
    kind)` builds one.
- **Escapes and character references are always nodes.**
  `ParseOptions::preserve_character_escapes` and
  `ParseOptions::preserve_character_references` are removed with
  `ParseOptions`: the parser always produces `Inline::Escape` and
  `Inline::CharacterReference`. In a table cell, `\|` is an `Escape('|')` in
  text and `|` inside a raw-text construct.
- **Precedence changes.**
  - Literal autolinks take the strict GFM extents and schemes (`http://`,
    `https://`, `www.`, emails, `mailto:`, `xmpp:`); a bare URL with any
    other scheme stays text. A literal autolink ends at Unicode whitespace,
    `<`, a non-ASCII punctuation or symbol char (`，`, `。`, `、`), or `[[`.
    `parse` no longer panics on a no-break space before an email-like run.
    As in cmark-gfm, a URL after a `[` or `![` no `]` has closed yet stays
    text (`[https://foo.com]`), while an email address still links, and a
    bracketed IPv6 host (`https://[fe80::1]`) is not a URL host. An inline
    footnote's `^[` opens no link text, so `^[see https://a.b]` links.
  - Link text holds no link. An autolink inside it, in an image's alt
    included, reads as its text, an angle-bracket one without its brackets
    (`[this <http://and.com> that](url)` is one link whose text is
    `this http://and.com that`), and a wiki link, also in a text directive's
    label, keeps the brackets around it from forming a link
    (`[a [[b]] c](u)` is text, a wiki link, and text).
  - A directive name is one or more runs of ASCII letters joined by single
    `-`, in all three directive forms. A text directive forms only when its
    name is followed by `[`, `{`, a space, a tab, or a line ending, or ends
    the inline content, so `:noreply@x.com` is an email link and
    `:www.x.com` is text. The whole text directive is followed by
    whitespace or the end of the content, or by ASCII punctuation when it
    has a non-empty label or non-blank attribute braces: `:badge[ok].` is a
    directive, and `:e{}x`, `:e{}.`, and `:e[a]b` are text. A directive
    whose name breaks the rule is text: `:h1[x]` silently, and a leaf or
    container opener such as `::my_note` with
    `DiagnosticCode::InvalidDirectiveName`.
  - A wiki link's content holds no unescaped `[` or `]`, so `[[[foo]]]` is
    `[`, a wiki link, and `]`. A wiki link still wins over a defined
    reference label.
  - A literal autolink ending in a hex character reference such as
    `www.a.b&#x41;` loses only its `;`, as with a decimal one and as in
    cmark-gfm.
  - Table rows split at every unescaped `|`, including one inside a code
    span.
  - A `_` run gets no strikethrough bonus beside a `~`: `d_~_` is text.
- **Wiki embeds.** `WikiLink` gains `embed: bool`: `![[x]]` is an embed and
  `\![[x]]` is an escaped `!` before a plain wiki link. The HTML renderer
  marks an embed with `data-wikilink-embed="true"`.
- **Wiki link text as written.** `WikiLink.target` and `WikiLink.label` hold
  their source as written, backslash escapes and character references
  included (`[[a\$b]]` has target `a\$b`), and are written back as they
  are. `WikiLink::decoded_target()` and `WikiLink::decoded_label()` decode
  them as the HTML renderer does.
- **Wiki link hrefs are filtered.** The HTML renderer blanks a wiki link
  whose decoded target uses `javascript:`, `vbscript:`, `file:`, or a
  non-image `data:` URI, unless `allow_dangerous_protocol` is set, and
  encodes the target once, as a link destination (`[[a&b]]` gets
  `href="a&amp;b"`).
- **Shortcodes come from gemoji.** A shortcode needs a name in the pinned
  github/gemoji v4.1.0 table and no letter or digit directly outside either
  colon, so clock times and `a:b:c` stay text. A `:word:` outside the table
  stays text too. `Shortcode::glyph()` returns the emoji, validation rejects
  a name outside the table, and the HTML renderer writes the glyph.
- **New validation rejections.** `validate` reports, and `to_markdown` and
  `to_html` return `InvalidDocument` for:
  - an empty `Paragraph`;
  - an `Emphasis`, `Strong`, `Delete`, or `Mark` holding nothing but empty
    text, or whose content, past empty text, starts with Unicode whitespace,
    a soft break, or a hard break of trailing spaces, or ends with Unicode
    whitespace, a soft break, or a hard break;
  - a `Link`, `Autolink`, `LinkReference`, or `WikiLink` inside the text of
    a `Link` or `LinkReference`;
  - an `Autolink` whose text is not exactly one autolink of its form;
  - a `CharacterReference` that is not exactly one character reference;
  - a `CodeInline` whose value is empty or holds a line ending;
  - an empty `MathInline`; code-form math whose value holds a backtick
    followed by `$`; dollar math that, written alone, does not read back
    with its value and fence (a `$` that closes it early, whitespace at an
    edge or a line ending in the single-`$` form, a fence of three or more);
  - two adjacent lists in one container written with the same marker char;
  - inside a table cell, a code span, math, raw HTML, autolink, reference
    label, or wiki link value holding a `|` after an odd run of
    backslashes, which no cell source can spell;
  - a directive name outside the name rule;
  - an `Emphasis` or `Strong` whose only child is an `Emphasis` with the
    same delimiter, a `Delete` holding only a `Delete`, a `Mark` holding
    only a `Mark`, and two adjacent `Delete`s;
  - a `Text` holding a line ending; a soft break, or a hard break of
    trailing spaces, right after a break; a break in a table cell or in a
    leaf or container directive label;
  - a reference or definition identifier other than the parser's
    normalization of its label; a definition or full reference label the
    parser does not read back as a label (an unescaped bracket, an escaped
    close, a blank line, over 999 chars, or blank); a footnote label that is
    not one; a definition label that is `^` and a footnote label where the
    parser would read a footnote definition;
  - a task list item without a non-empty paragraph after its leading
    definitions;
  - in a tight list item, a block after a paragraph that cannot interrupt
    it: a paragraph, a definition, indented code, frontmatter, a setext
    heading, an ordered list not starting at 1, a list whose first item is
    empty or is written starting on the line after its bullet, or an HTML
    block that is a lone tag or opens no HTML block;
  - a loose `List` of one item holding at most one block, which no source
    spells;
  - an empty `List`; a `Frontmatter` inside a container or holding its own
    fence line; an empty `InlineFootnote`; an `Alert` title that is empty or
    has spaces or tabs around it; a `CodeBlock` info string of `Some("")`;
    an empty bare destination; an indented `CodeBlock` that is empty or
    starts or ends with a blank line.
- **Validation is the only gate.** `SerializeError::UnsupportedNode` is
  removed: every tree that validates is written.
- **Serialize options keep or replace.** `SerializeOptions::bullet` is an
  `Option<BulletMarker>` (`Dash`, `Asterisk`, `Plus`),
  `SerializeOptions::ordered_delimiter` an `Option<OrderedDelimiter>`
  (`Period`, `Paren`), and `SerializeOptions::fence_marker` an
  `Option<FenceMarker>`. `None`, the default, keeps the marker or fence the
  AST records; `Some` writes every list or fence with it, so `* a` with
  `Some(BulletMarker::Dash)` is written `- a`.
- **The serializer only renders.** It writes each node in the spelling the
  AST records, or else by one fixed rule, and never parses its output: when
  the output would read back as a different tree, it is still returned.
  Text is written as it is, each `Escape` as a backslash and its char, and
  each `CharacterReference` as written; the serializer adds no escape of its
  own. A hand-built `Text("*a*")` is written `*a*`, which reads back as
  emphasis: build literal punctuation with `Escape` nodes.
- **Canonical output changes.**
  - Recorded spellings come back as written: `_a_ __b__`, `a\.b \#tag`,
    `&#35;tag &amp; x`, `www.a.b`, `a@b.c`, `<http://a.b>`, and
    `[http://a.b](http://a.b)` are each written as they were parsed.
  - A table cell is written by the ordinary rules and then encoded once: a
    `|` after zero or an even number of backslashes gets one more, so a
    hand-built `Text("a|b")` in a cell is written `a\|b`.
  - A code span is written with the shortest backtick fence that neither
    occurs in its value nor closes an earlier unmatched backtick run, and
    with a padding space at each end only where the value needs one.
  - Unpaired delimiters stay raw: `x_y_ a*b x^2 ~5`.
  - Every line inside a block quote, alert, list item, or footnote
    definition takes its container's full prefix, a lazy continuation line
    included (`> a\nb` → `> a\n> b`).
  - A soft break inside a heading is written as a space (`a\nb\n===` →
    `a b\n===`).
  - A dash thematic break that opens the document or follows a paragraph
    line is written `- - -`; a list item whose content is a thematic break
    of its bullet's char, or begins with a space or a tab, starts that
    content on the line after the bullet.
  - A list marker override yields where the list before it in the same
    container is written with the same marker.
  - An ordered list's item numbers stop at 999999999, the largest a marker
    holds, so `999999999. a` followed by a second item reads back as one
    list.
  - An ATX heading whose content ends in a `#` run after a space, or is all
    `#`, gets a closing sequence of its own: `# C # #` is written as it is.
  - The empty last line of a value inside a block quote or alert keeps its
    `>`, and footnote content that opens with a space or a tab, such as
    indented code, starts on the line after the label.
- **Linear time on more inputs.** Email candidates in a long run before one
  `@` (`"-a".repeat(n) + "@b"`, `"a+".repeat(n) + "@b"`), `www.` hosts after
  backslash escapes, long `~` runs in a paragraph, runs of escaped `$`, and
  multi-line definition labels parse in linear time.
- **Block structure follows CommonMark's algorithm.** Block quotes, list
  items, container directives, footnote definitions, HTML containers, and
  alerts are read in one pass over a stack of open blocks, matching
  commonmark.js on the cases plimeor/markdown-syntax#11 lists. A task item's
  checkbox is part of its marker, so its paragraph starts after it.
  `::name text` is a paragraph; a leaf directive stands alone on its line.
- **Block parse changes that move the AST of existing input:**
  - footnote definitions, directive openers, and math fences open only up to
    three columns in;
  - a container directive's closing fence closes the innermost open one,
    and its last child's span ends after its line ending;
  - footnote definitions and alert paragraphs take lazy continuation lines;
  - tabs after a tab split by a container prefix keep their columns;
  - blank lines inside a fence or HTML block do not loosen a list;
  - an unclosed fence keeps its trailing blank lines;
  - a multi-line definition title drops its continuation lines'
    indentation;
  - an alert's marker line keeps a following indented line from starting
    indented code, as a paragraph line does.

## [0.3.0](https://github.com/plimeor/markdown-syntax/compare/v0.2.0...v0.3.0) - 2026-10-05

### Added

- [**breaking**] bound untrusted-input cost and parse inlines on a delimiter stack ([#4](https://github.com/plimeor/markdown-syntax/pull/4))

## [0.2.0](https://github.com/plimeor/markdown-syntax/compare/v0.1.2...v0.2.0) - 2026-07-05

### Other

- Add details HTML container parsing

## [0.1.2](https://github.com/plimeor/markdown-syntax/compare/v0.1.1...v0.1.2) - 2026-06-23

### Other

- Honor raw source spans in parser

## [0.1.1](https://github.com/plimeor/markdown-syntax/compare/v0.1.0...v0.1.1) - 2026-06-20

### Other

- Rewrite README for usage-first readability
- Add README badges and release-plz automation

## [0.1.0] - 2026-06-20

Initial release.

### Added

- `no_std + alloc` Markdown parser: source to an owned AST via `parse`, with
  optional half-open byte `Span`s and a `LineIndex` for line/column mapping.
- Canonical serializer: AST to Markdown via `Document::to_markdown` /
  `Document::to_markdown_with`.
- AST validation via `Document::validate`, sharing one `Diagnostic` type with
  the parser.
- Configurable dialects: `parse` recognizes a maximal non-MDX dialect (GFM plus
  footnotes, math, frontmatter, wikilinks, directives, and the extra inline
  marks); `SyntaxOptions::{commonmark, gfm, mdx}` presets and a `Construct`
  enable/disable builder sit on top of the exhaustive `Constructs` /
  `ParseOptions` flags.
- Opt-in `html` feature: safe-by-default AST to HTML rendering
  (`Document::to_html` / `Document::to_html_with`) that validates first, escapes
  raw HTML, and filters dangerous link/image protocols.
- A `prelude` module for one-line imports.

[0.1.0]: https://github.com/plimeor/markdown-syntax/releases/tag/v0.1.0
