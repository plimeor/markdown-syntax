# markdown-syntax

[![crates.io](https://img.shields.io/crates/v/markdown-syntax.svg)](https://crates.io/crates/markdown-syntax)
[![docs.rs](https://docs.rs/markdown-syntax/badge.svg)](https://docs.rs/markdown-syntax)
[![CI](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml/badge.svg)](https://github.com/plimeor/markdown-syntax/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/markdown-syntax.svg)](#license)

A `no_std + alloc` Rust crate that parses Markdown source into an owned AST and serializes the AST back to canonical Markdown — with opt-in, safe-by-default HTML rendering behind the `html` feature.

## At a glance

- **AST-first** — `parse` returns an owned enum tree over `alloc`; the output verbs live on the `Document` you hold.
- **Tolerant** — problems are collected as diagnostics, never thrown; `parse` is infallible.
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

// Serialize the AST back to canonical Markdown (this is the fallible step).
let markdown = output.document.to_markdown()?;
assert_eq!(markdown, "# Title\n\nHello *world*.\n");
# Ok::<(), markdown_syntax::SerializeError>(())
```

[`parse`](https://docs.rs/markdown-syntax/latest/markdown_syntax/fn.parse.html) is infallible — the output verbs live on the [`Document`](https://docs.rs/markdown-syntax/latest/markdown_syntax/ast/struct.Document.html) you hold.

## Common tasks

`parse` is the one way in. When you need to read diagnostics, walk the tree, or render HTML, each task is one small snippet below.

- [Walk the AST](#walk-the-ast)
- [Handle diagnostics](#handle-diagnostics)
- [Customize serialization](#customize-serialization)
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

### Customize serialization

```rust
use markdown_syntax::{parse, BulletMarker, LineEnding, SerializeOptions};

// `SerializeOptions` is #[non_exhaustive]: mutate a default rather than using a
// struct literal.
let mut options = SerializeOptions::default();
options.line_ending = LineEnding::CrLf;
options.final_newline = false;

let markdown = parse("# Title").document.to_markdown_with(&options)?;
assert_eq!(markdown, "# Title");

// List-marker and fence options are `None` by default, keeping the marker
// each node records; `Some` writes every list or fence with the one given.
let mut options = SerializeOptions::default();
options.bullet = Some(BulletMarker::Dash);
assert_eq!(parse("* a").document.to_markdown_with(&options)?, "- a\n");

// Each node is written in the spelling it records: `_` emphasis stays `_`,
// an escape stays an escape, and a literal autolink stays bare.
let document = parse("_a_ \\*b\\* https://example.com").document;
assert_eq!(document.to_markdown()?, "_a_ \\*b\\* https://example.com\n");
# Ok::<(), markdown_syntax::SerializeError>(())
```

Because `SerializeOptions` is `#[non_exhaustive]`, external code cannot struct-literal-construct it (even with `..Default::default()`, E0639) — mutate a `default()` instead.

The serializer only renders: it writes text, escapes, and character references as the AST records them and never parses its own output. A hand-built `Text("*a*")` is written `*a*`, which reads back as emphasis; build literal punctuation with `Escape` nodes instead. A hand-built tree that no Markdown can express — a link inside a link, say — fails validation, and `to_markdown` returns `SerializeError::InvalidDocument` with the diagnostics.

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

let document = Document {
    meta: NodeMeta::default(),
    children: vec![
        Heading::new(1, [Text::from("Title")]).into(),
        Paragraph::new([Text::from("hello")]).into(),
    ],
};
// Hand-built nodes carry no span.
assert_eq!(document.children[0].span(), None);
assert_eq!(document.to_markdown().unwrap(), "# Title\n\nhello\n");

// Text is written as it is; `Escape` writes a literal punctuation char.
let escape = |value| Inline::from(Escape { meta: NodeMeta::default(), value });
let literal = Document {
    meta: NodeMeta::default(),
    children: vec![Paragraph::new([escape('*'), Text::from("a").into(), escape('*')]).into()],
};
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

- **AST-first public API** — `parse` produces an owned `Document`; parser event streams and internal block operations are private, not v1 compatibility surfaces.
- **Owned enum tree** over `alloc` types.
- **Optional source spans** — half-open absolute byte ranges on every node, `None` for hand-built nodes; line/column derived via `LineIndex`.
- **Tolerant by default** — diagnostics are collected, not thrown.

## Scope & limitations

In scope — the one syntax in the [syntax reference](#syntax-reference). Subscript, superscript, insert, spoiler, underline, description lists, and MDX are not recognized: `__a__` is strong, and the rest stay text or raw HTML.

Not in the default build: HTML rendering or sanitization, syntax highlighting, and byte-for-byte preservation of the source's authoring style. Directives (`:name` / `::name` / `:::name`) are their own family and are never MDX.

Parsing, serialization, HTML rendering, and validation take linear time and bounded stack on any input; nesting past fixed limits stays literal text.

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
