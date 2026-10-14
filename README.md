# markdown-syntax

[![crates.io](https://img.shields.io/crates/v/markdown-syntax.svg)](https://crates.io/crates/markdown-syntax)
[![docs.rs](https://docs.rs/markdown-syntax/badge.svg)](https://docs.rs/markdown-syntax)
[![CI](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml/badge.svg)](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/markdown-syntax.svg)](#license)

A `no_std + alloc` Rust crate that parses Markdown source into an owned AST and writes the AST back as Markdown, keeping the source of every block you did not change — with opt-in, safe-by-default HTML rendering behind the `html` feature.

## At a glance

- **AST-first** — `parse` returns an owned enum tree over `alloc`; the output verbs live on the `Document` you hold.
- **Tolerant** — problems are collected as diagnostics, never thrown; `parse` is infallible.
- **The source is the only truth** — a parsed `Document` keeps its source, and writing it back reuses the bytes of every unchanged block.
- **One syntax** — CommonMark + GFM + footnotes + alerts + math + frontmatter + wikilinks + directives + `==` highlight + shortcodes, with nothing to configure.
- **Lean core** — zero runtime dependencies, `no_std + alloc`, MSRV 1.82.

## Install

```console
cargo add markdown-syntax
```

For the opt-in HTML renderer:

```console
cargo add markdown-syntax --features html
```

Or in `Cargo.toml`:

```toml
[dependencies]
markdown-syntax = "0.3"
```

## Quickstart

```rust
use markdown_syntax::parse;

// `parse` is infallible and returns a `ParseOutput { document, diagnostics }`.
let output = parse("# Title\n\nHello *world*.");
assert!(output.diagnostics.is_empty());

// Writing an unedited parse returns its source (this is the fallible step).
assert_eq!(output.document.to_markdown()?, "# Title\n\nHello *world*.");

// Without its source, the document is written as canonical Markdown.
let markdown = output.document.without_source().to_markdown()?;
assert_eq!(markdown, "# Title\n\nHello *world*.\n");
# Ok::<(), markdown_syntax::SerializeError>(())
```

[`parse`](https://docs.rs/markdown-syntax/latest/markdown_syntax/fn.parse.html) is infallible — the output verbs live on the [`Document`](https://docs.rs/markdown-syntax/latest/markdown_syntax/ast/struct.Document.html) you hold.

## Common tasks

`parse` reads a document and `parse_inline` one block's inline content, with the same syntax. When you need to read diagnostics, walk the tree, edit and write back, or render HTML, each task is one small snippet below.

- [Walk the AST](#walk-the-ast)
- [Handle diagnostics](#handle-diagnostics)
- [Edit and write back](#edit-and-write-back)
- [Customize serialization](#customize-serialization)
- [Parse one block's inline content](#parse-one-blocks-inline-content)
- [Source positions (optional)](#source-positions-optional)
- [Build an AST by hand](#build-an-ast-by-hand)

### Walk the AST

```rust
use markdown_syntax::{parse, Block, Inline};

let document = parse("Hello *world*.").document;

for block in &document.children {
    if let Block::Paragraph(paragraph) = block {
        for inline in &paragraph.children {
            if let Inline::Text(text) = inline {
                assert_eq!(text.value, "Hello ");
                break;
            }
        }
    }
}
```

`document.children` is a `Vec<Block>`; block content (like `Paragraph.children`) is a `Vec<Inline>`. See the [`ast`](https://docs.rs/markdown-syntax/latest/markdown_syntax/ast/index.html) module, [`Block`](https://docs.rs/markdown-syntax/latest/markdown_syntax/ast/enum.Block.html), and [`Inline`](https://docs.rs/markdown-syntax/latest/markdown_syntax/ast/enum.Inline.html).

### Handle diagnostics

```rust
use markdown_syntax::{parse, DiagnosticSeverity};

// Problems are collected, never thrown.
let output = parse(":::note\nunclosed container");
for diagnostic in &output.diagnostics {
    let _ = (diagnostic.severity, diagnostic.code, diagnostic.span, &diagnostic.message);
    if diagnostic.severity == DiagnosticSeverity::Error {
        // handle an error-severity diagnostic
    }
}
```

`span` is `Option<Span>` because a hand-built node may lack a source location. Parser diagnostics, AST validation, and serializer/HTML pre-validation are three separate domains that share one [`Diagnostic`](https://docs.rs/markdown-syntax/latest/markdown_syntax/diagnostic/struct.Diagnostic.html) type.

### Edit and write back

```rust
use markdown_syntax::prelude::*;

let source = "|a|b|\n|-|-|\n\n*  x\n\n\n# T #";
let mut document = parse(source).document;

// An unchanged block is written as its source, with the bytes around it.
assert_eq!(document.to_markdown()?, source);

// An edited or new block, one without the spans of the block the source
// parses to there, is written as canonical Markdown.
document.children[1] = Paragraph::new([Text::from("y")]).into();
assert_eq!(document.to_markdown()?, "|a|b|\n|-|-|\n\ny\n\n# T #");
# Ok::<(), markdown_syntax::SerializeError>(())
```

A block keeps its source when it equals, spans included, the block the source parses to at the same span. Where its source would read differently in its new place, as an unclosed fence before a new block would, it is written canonically too.

### Customize serialization

```rust
use markdown_syntax::{parse, BulletMarker, LineEnding, SerializeOptions};

// `SerializeOptions` is #[non_exhaustive]: mutate a default rather than using a
// struct literal.
let mut options = SerializeOptions::default();
options.line_ending = LineEnding::CrLf;
options.final_newline = false;

let markdown = parse("# Title").document.without_source().to_markdown_with(&options)?;
assert_eq!(markdown, "# Title");

// List-marker and fence options are `None` by default, keeping the marker
// each node records; `Some` writes every list or fence with the one given.
let mut options = SerializeOptions::default();
options.bullet = Some(BulletMarker::Dash);
assert_eq!(parse("* a").document.without_source().to_markdown_with(&options)?, "- a\n");

// Each node is written in the spelling it records: `_` emphasis stays `_`,
// an escape stays an escape, and a literal autolink stays bare.
let document = parse("_a_ \\*b\\* https://example.com").document.without_source();
assert_eq!(document.to_markdown()?, "_a_ \\*b\\* https://example.com\n");
# Ok::<(), markdown_syntax::SerializeError>(())
```

Options apply to the Markdown the writer writes, not to reused source; the examples above drop the source to show them.

Because `SerializeOptions` is `#[non_exhaustive]`, external code cannot struct-literal-construct it (even with `..Default::default()`, E0639) — mutate a `default()` instead.

The serializer only renders: it writes text, escapes, and character references as the AST records them and never parses its own output. A hand-built `Text("*a*")` is written `*a*`, which reads back as emphasis; build literal punctuation with `Escape` nodes instead. A hand-built tree that no Markdown can express — a link inside a link, say — fails validation, and `to_markdown` returns `SerializeError::InvalidDocument` with the diagnostics.

### Parse one block's inline content

```rust
use markdown_syntax::prelude::*;

let document = parse("> a [*b*][r]\n> c\n\n[r]: /u").document;

// The text the parser read for the paragraph in the block quote, and where
// each of its offsets sits in the source.
let leaf = document.inline_source(&NodePath::from([0, 0])).unwrap();
assert_eq!(leaf.text, "a [*b*][r]\nc");
assert_eq!(leaf.to_source(11), 15);

// Parsing that text again, with the document's definitions, gives the
// paragraph's inlines; no block rule applies, so `# c` would stay text.
let inline = parse_inline(&leaf.text, &Definitions::of(&document));
assert!(matches!(inline.inlines[1], Inline::LinkReference(_)));
```

`parse_inline` spans are byte ranges into the text; `InlineSource::span_to_source` maps them to the document's source.

### Source positions (optional)

```rust
use markdown_syntax::{parse, LineIndex};

let source = "# Title\n\nHello.";
let document = parse(source).document;
let index = LineIndex::new(source);

// Spans are absolute, half-open UTF-8 byte ranges; `None` for hand-built nodes.
if let Some(first) = document.children.first() {
    if let Some(span) = first.span() {
        let (start, end) = index.span(span);
        // 1-based line/column.
        assert_eq!(start.line, 1);
        assert_eq!(start.column, 1);
        let _ = (span.start, span.end, span.len(), end.line, end.column);
    }
}
```

Spans are absolute half-open UTF-8 byte ranges, `None` for hand-built nodes. [`LineIndex`](https://docs.rs/markdown-syntax/latest/markdown_syntax/span/struct.LineIndex.html) turns a [`Span`](https://docs.rs/markdown-syntax/latest/markdown_syntax/span/struct.Span.html) into 1-based [`LinePosition`](https://docs.rs/markdown-syntax/latest/markdown_syntax/span/struct.LinePosition.html) line/column.

### Build an AST by hand

The [`prelude`](https://docs.rs/markdown-syntax/latest/markdown_syntax/prelude/index.html) imports the common surface in one line:

```rust
use markdown_syntax::prelude::*;

let document = Document::new(vec![
    Heading::new(1, [Text::from("Title")]).into(),
    Paragraph::new([Text::from("hello")]).into(),
]);
// Hand-built nodes carry no span.
assert_eq!(document.children[0].span(), None);
assert_eq!(document.to_markdown().unwrap(), "# Title\n\nhello\n");

// Text is written as it is; `Escape` writes a literal punctuation char.
let escape = |value| Inline::from(Escape { meta: NodeMeta::default(), value });
let literal = Document::new(vec![Paragraph::new([escape('*'), Text::from("a").into(), escape('*')]).into()]);
assert_eq!(literal.to_markdown().unwrap(), "\\*a\\*\n");
```

## HTML rendering (opt-in)

The HTML renderer ships behind the non-default `html` feature and is safe by default: it validates the AST first, escapes raw HTML, blanks dangerous link/image protocols, and disables task-list checkboxes.

```console
cargo add markdown-syntax --features html
```

```rust,ignore
// Requires `--features html`; the default doctest build has no html feature,
// so this block is `rust,ignore`.
use markdown_syntax::{parse, HtmlOptions, HtmlError, SafeRawHtmlForm};

let document = parse("# Hi\n\n<script>alert(1)</script>").document;

// Default is safe: raw HTML is escaped, dangerous link/image protocols blanked.
let safe: Result<String, HtmlError> = document.to_html();
assert!(safe.is_ok());

// `HtmlOptions` is #[non_exhaustive]: mutate a default to opt into raw HTML.
let mut options = HtmlOptions::default();
options.allow_dangerous_html = true;
options.safe_raw_html_form = SafeRawHtmlForm::OmitPlaceholder;
let _ = document.to_html_with(&options);
```

See [`HtmlOptions`](https://docs.rs/markdown-syntax/latest/markdown_syntax/html/struct.HtmlOptions.html). docs.rs builds with the `html` feature enabled, so the renderer's API is fully documented there.

## WebAssembly and JavaScript

The `markdown-syntax-wasm` crate in `wasm/` builds the parser for `wasm32-unknown-unknown` with a zero-dependency C ABI, and `wasm/js/index.js` (types in `index.d.ts`) wraps it with no npm dependency: `parse`, `parseInline`, `inlineSource`, and `toMarkdown`, with every span in UTF-16 code units. It is not published; build it with

```console
cargo build -p markdown-syntax-wasm --target wasm32-unknown-unknown --profile wasm
```

## Syntax reference

`parse` recognizes one fixed syntax:

| Family | Constructs |
| --- | --- |
| CommonMark | everything, including raw HTML and indented code |
| GFM | tables, task list items, `~~` strikethrough, literal autolinks (`http://`, `https://`, `www.`, emails, `mailto:`, `xmpp:`), alerts |
| Footnotes | `[^id]` references and definitions, inline `^[note]` |
| Extensions | frontmatter (`---` / `+++`), inline and block math, wikilinks (`[[target\|title]]`, embeds `![[x]]`), `==` highlight, gemoji shortcodes (`:tada:`), `:name` / `::name` / `:::name` directives, `<details>` HTML containers |

Where one node has more than one spelling, the parse records it: `Emphasis` and `Strong` record `*` or `_`, and `Autolink` records an angle-bracket or a literal autolink; `Link` is the inline `[text](destination)` form. A node stores each written fact once: an `Autolink` derives its `destination()` from its text, a `CharacterReference` decodes its `value()` from the reference, and a `CodeInline` holds only its value, its fence chosen when written.

Cargo features:

| Feature | Default | What it adds |
| --- | --- | --- |
| `default` | `[]` (empty) | Byte-stable `no_std + alloc` core: parser, AST, serializer, validation, `Span`/`LineIndex`, prelude. Zero runtime deps. |
| `html` | off | Opt-in, additive, safe-by-default `to_html` / `to_html_with` and the `html` module. Stays `no_std + alloc`, zero runtime deps. |

## How it works

- **AST-first public API** — `parse` produces an owned `Document` holding its source; parser event streams and internal block operations are private, not v1 compatibility surfaces.
- **Owned enum tree** over `alloc` types.
- **Optional source spans** — half-open absolute byte ranges on every node, `None` for hand-built nodes; line/column derived via `LineIndex`.
- **Tolerant by default** — diagnostics are collected, not thrown.

## Scope & limitations

In scope — the one syntax in the [syntax reference](#syntax-reference). Subscript, superscript, insert, spoiler, underline, description lists, and MDX are not recognized: `__a__` is strong, and the rest stay text or raw HTML.

Not in the default build: HTML rendering or sanitization, and syntax highlighting. A block you edit is written in canonical Markdown, so its authoring style is kept only as far as the AST records it. Directives (`:name` / `::name` / `:::name`) are their own family and are never MDX.

Parsing, inline parsing, serialization, HTML rendering, and validation take linear time and bounded stack on any input; nesting past fixed limits stays literal text.

The full behavior contract — syntax, serialization, validation, HTML rendering, and the cost limits — lives in [`docs/specs/`](docs/specs/).

## Compatibility

`no_std + alloc` (crate root is `#![no_std]` + `extern crate alloc`). Default features are empty; the opt-in `html` feature also stays `no_std + alloc`. Zero runtime dependencies. MSRV 1.82 (edition 2021).

## Contributing & conformance

Tests live in `tests/`. AST→HTML correctness is measured against vendored CommonMark/GFM oracles; observe the current numbers with `cargo test --features html --test html_conformance -- --nocapture`.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
