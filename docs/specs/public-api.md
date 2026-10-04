# Public API

## Purpose

The Rust surface through which callers parse Markdown, choose a dialect, read
diagnostics and source positions, build ASTs by hand, and ask a `Document` for
Markdown, HTML, or validation results. Owned by `src/lib.rs`, `src/options.rs`,
`src/diagnostic.rs`, `src/span.rs`, and `src/ast.rs`.

## Requirements

### Requirement: Build surface
The crate SHALL build as `no_std + alloc` with an empty default feature set, zero
runtime dependencies, and MSRV 1.82; the `html` feature SHALL be additive and
keep the same constraints.

#### Scenario: Default build
- **WHEN** the crate is built with default features
- **THEN** it compiles for `wasm32-unknown-unknown` and with Rust 1.82, pulls in no runtime dependency, and exports no HTML renderer

#### Scenario: HTML feature
- **WHEN** the crate is built with `--features html`
- **THEN** `Document::to_html` and `Document::to_html_with` become available and the default-build parse, AST, and serializer behavior is unchanged

### Requirement: Infallible parse
`parse(input)` SHALL return a `ParseOutput { document, diagnostics }` for every
`&str` input without panicking or returning an error; parse problems SHALL be
reported as diagnostics.

#### Scenario: Clean input
- **WHEN** `parse("# Title\n\nHello *world*.")` runs
- **THEN** `diagnostics` is empty and the document holds a heading and a paragraph

#### Scenario: Problem input
- **WHEN** `parse(":::note\nunclosed container")` runs
- **THEN** a document is returned and `diagnostics` contains an error-severity `UnclosedDirectiveContainer`

### Requirement: Default dialect
`parse(input)` SHALL behave exactly as `SyntaxOptions::default().parse(input)`,
the maximal non-MDX dialect: every construct except MDX and `underline`.

#### Scenario: Underline stays off
- **WHEN** `parse("a __b__ c")` runs
- **THEN** `__b__` is `Strong`

#### Scenario: Extensions are on
- **WHEN** `parse("H~2~O and x^2^")` runs
- **THEN** the paragraph contains a `Subscript` and a `Superscript`

### Requirement: Named presets
`SyntaxOptions::commonmark()`, `SyntaxOptions::gfm()`, and `SyntaxOptions::mdx()`
SHALL select, respectively, CommonMark core only; CommonMark plus tables, task
lists, strikethrough, autolinks, and footnotes; and MDX JSX, expressions, and
ESM with raw HTML off.

#### Scenario: CommonMark keeps extensions literal
- **WHEN** `SyntaxOptions::commonmark().parse("~~kept literal~~")` runs
- **THEN** the paragraph is text only

#### Scenario: GFM strikethrough
- **WHEN** `SyntaxOptions::gfm().parse("~~done~~")` runs
- **THEN** the paragraph contains a `Delete`

#### Scenario: MDX mode
- **WHEN** `SyntaxOptions::mdx().parse("<Component/>\n\ntext")` runs
- **THEN** the first block is an MDX JSX node, not raw HTML

### Requirement: Construct builder
`SyntaxOptions::enable` and `SyntaxOptions::disable` SHALL toggle one
`Construct`; grouped constructs (`Math`, `Footnotes`, `Directives`) SHALL toggle
every flag in their group, and `Wikilinks` SHALL take the title order as a
parameter.

#### Scenario: Opt into underline
- **WHEN** `SyntaxOptions::default().enable(Construct::Underline).parse("a __b__ c")` runs
- **THEN** `__b__` is `Underline`

#### Scenario: Enable wikilinks on CommonMark
- **WHEN** `SyntaxOptions::commonmark().enable(Construct::Wikilinks(WikiLinkOrder::TitleAfterPipe)).parse("see [[target|label]]")` runs
- **THEN** the paragraph contains a `WikiLink` with target `target` and label `label`

### Requirement: Configuration conflicts
`SyntaxOptions::validate` SHALL return `SyntaxConfigError::MdxHtmlConflict` when
MDX JSX and raw HTML are both enabled and
`SyntaxConfigError::WikilinkTitleOrderConflict` when both wikilink title orders
are enabled; `parse` SHALL still return a document for such options and report
the conflict as a diagnostic.

#### Scenario: Conflicting fields
- **WHEN** a `SyntaxOptions` with both `mdx_jsx_inline` and `html_inline` enabled is validated
- **THEN** `validate()` returns `Err(SyntaxConfigError::MdxHtmlConflict)`

### Requirement: Strict parse
`SyntaxOptions::parse_strict` SHALL return `Err(ParseStrictError::Config)` for a
configuration conflict, `Err(ParseStrictError::Diagnostic)` when parsing produces
an error-severity diagnostic, and `Ok(ParseOutput)` otherwise.

#### Scenario: Clean strict parse
- **WHEN** `SyntaxOptions::default().parse_strict("# clean input")` runs
- **THEN** it returns `Ok` with no error-severity diagnostics

#### Scenario: Error diagnostic promoted
- **WHEN** `SyntaxOptions::default().parse_strict(":::note\nunclosed container")` runs
- **THEN** it returns `Err(ParseStrictError::Diagnostic(_))`

### Requirement: Output verbs on Document
A `Document` SHALL offer `to_markdown()`, `to_markdown_with(&SerializeOptions)`,
and `validate()`, and with the `html` feature `to_html()` and
`to_html_with(&HtmlOptions)`.

#### Scenario: Round trip through the document
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `Ok("# Title\n\nHello *world*.\n")`

### Requirement: One diagnostic type
Parser diagnostics, AST validation, and serializer and HTML pre-validation SHALL
all report a single `Diagnostic { severity, code, span: Option<Span>, message }`.

#### Scenario: Parser diagnostic has a span
- **WHEN** a parse reports a diagnostic
- **THEN** its `span` is `Some` and lies within the input

#### Scenario: Validation diagnostic on a hand-built node
- **WHEN** `validate()` reports a problem on a node without a span
- **THEN** the diagnostic's `span` is `None` and its code is `InvalidDocument`

### Requirement: Source spans
Every parsed node SHALL carry an absolute, half-open UTF-8 byte range into the
original input; hand-built nodes SHALL carry `None`; `LineIndex` SHALL convert a
span to 1-based line and column positions.

#### Scenario: First block position
- **WHEN** `parse("# Title\n\nHello.")` runs and the first block's span is passed to `LineIndex::new(source).span(span)`
- **THEN** the start position is line 1, column 1

#### Scenario: Hand-built node
- **WHEN** a `Heading` is built with `Heading::new(1, [Text::from("Title")])`
- **THEN** its `span()` is `None`

### Requirement: Top-level spans tile the source
The spans of a parsed document's top-level blocks SHALL be in source order,
non-overlapping, on UTF-8 character boundaries, within the input, and separated
only by whitespace.

#### Scenario: Multi-block input with CRLF
- **WHEN** an input of several blocks with CRLF line endings is parsed
- **THEN** slicing the input with each top-level span yields the block's source and every gap between spans is whitespace

#### Scenario: Unclosed fence
- **WHEN** an input ending in an unclosed code fence is parsed
- **THEN** the fence's span ends within the input and the tiling holds

### Requirement: Input is parsed without preprocessing
The parser SHALL recognize structure on the input exactly as given, treating a
leading U+FEFF and any U+0000 as ordinary characters, and SHALL write U+FFFD in
place of U+0000 only in inline text values.

#### Scenario: Leading BOM
- **WHEN** `parse("\u{feff}# title")` runs
- **THEN** the document holds a paragraph whose text starts with U+FEFF, not a heading

#### Scenario: NUL in text
- **WHEN** `parse("a\u{0}b")` runs
- **THEN** the paragraph text is `a\u{FFFD}b` and its span covers bytes 0..3

#### Scenario: NUL in a link destination
- **WHEN** `parse("[a](\u{0})")` runs
- **THEN** no link forms

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Text`, `Paragraph`, `Heading`, `Link`, `Code`, and `List`, all defaulting `meta`
to no span; `markdown_syntax::prelude::*` SHALL import this surface.

#### Scenario: Build and serialize
- **WHEN** a `Document` is built from `Heading::new(1, [Text::from("Title")]).into()` and `Paragraph::new([Text::from("hello")]).into()`
- **THEN** `to_markdown()` returns `"# Title\n\nhello\n"`
