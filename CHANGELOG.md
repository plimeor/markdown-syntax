# Changelog

All notable changes to this project are documented in this file. The format is
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Migration

This release changes the AST, the parse of block structure, and the
serializer's output. Code that matches on the AST or compares canonical
output needs these updates.

- **Autolinks are links.** `Inline::Autolink` and `AutolinkKind` are removed.
  Literal autolinks (`https://a.b`, `www.a.b`, `a@b.c`), angle-bracket
  autolinks (`<https://a.b>`), and links whose text equals their URL are all
  `Link` nodes whose one child is a `Text` holding the URL as written. Such a
  link is written `<url>` when that reads back, and `[text](url)` otherwise
  (`www.a.b` → `[www.a.b](http://www.a.b)`).
- **Escapes and character references are always nodes.**
  `ParseOptions::preserve_character_escapes` and
  `ParseOptions::preserve_character_references` are removed: the parser
  always produces `Inline::Escape` and `Inline::CharacterReference`, and the
  serializer writes them as recorded. In a table cell, `\|` is an
  `Escape('|')` in text and `|` inside a raw-text construct.
- **Wiki embeds.** `WikiLink` gains `embed: bool`: `![[x]]` is an embed and
  `\![[x]]` is an escaped `!` before a plain wiki link. The HTML renderer
  marks an embed with `data-wikilink-embed="true"`.
- **Shortcodes come from gemoji.** A shortcode needs a name in the pinned
  github/gemoji v4.1.0 table and no letter or digit directly outside either
  colon, so clock times and `a:b:c` stay text. `Shortcode::glyph()` returns
  the emoji, validation rejects a name outside the table, and the HTML
  renderer writes the glyph.
- **Serialization reads its output back.** `SerializeOptions` gains
  `syntax: SyntaxOptions`, the dialect the output is read back under (the
  maximal dialect by default). `SerializeError::Unrepresentable` is returned,
  with `DiagnosticCode::Unrepresentable`, when no Markdown the serializer can
  write reads back as the same tree.
- **New diagnostic codes.** `DiagnosticCode::InvalidDirectiveAttribute`
  warns about a directive attribute without a valid name, which is dropped;
  `DiagnosticCode::Unrepresentable` names the node serialization cannot
  write.
- **Canonical output changes.** Only what a parse reads as syntax is
  escaped:
  - an unpaired delimiter stays raw (`x_y_`, not `x_y\_`);
  - a pipe ending a setext heading stays raw (`a |`, not `a \|`);
  - brackets that form no reference stay raw (`[x]` when nothing defines
    `x`);
  - a delimiter run the parse uses is escaped whole (`\=\=a\=\=`);
  - emphasis and strong take `*` unless that does not read back
    (`***(a b)_.***\*#`);
  - an invalid character reference such as `&unknown;` stays raw;
  - a nested list is indented only when the block after it would join its
    last item, and a list marker override yields where two adjacent lists
    would read as one.
- **Block structure follows CommonMark's algorithm.** Block quotes, list
  items, container directives, footnote definitions, HTML containers,
  description details, and alerts are read in one pass over a stack of open
  blocks, matching commonmark.js on the cases plimeor/markdown-syntax#11
  lists. A task item's checkbox is part of its marker, so its paragraph
  starts after it. `::name text` is a paragraph; a leaf directive stands alone
  on its line.
- **Literal autolinks end earlier.** A literal or relaxed-scheme autolink
  ends at Unicode whitespace, `<`, a non-ASCII punctuation or symbol char
  (`，`, `。`, `、`), or, with wikilinks on, `[[`. `parse` no longer panics on
  a no-break space before an email-like run.
- **A `_` run gets no strikethrough bonus** beside a `~`: `d_~_` is text.

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
