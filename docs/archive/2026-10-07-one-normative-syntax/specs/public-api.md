# Public API — spec changes

## ADDED Requirements

### Requirement: One syntax
`parse(input)` SHALL be the only way to parse, and SHALL recognize one fixed
syntax with no configuration: CommonMark with raw HTML and indented code,
`<details>` HTML containers, GFM tables, task list items, literal autolinks,
and `~~` strikethrough, footnotes and inline footnotes, GitHub alerts,
frontmatter, shortcodes, `==` highlight, wiki links, math, and directives.
Subscript, superscript, insert, spoiler, underline, description lists,
literal autolinks with a scheme other than `http`, `https`, `mailto`, and
`xmpp`, and MDX SHALL NOT be recognized.

#### Scenario: Double underscore is strong
- **WHEN** `parse("a __b__ c")` runs
- **THEN** `__b__` is `Strong`

#### Scenario: Removed marks stay text
- **WHEN** `parse("H~2~O and x^2^ ++a++ ||b||")` runs
- **THEN** the paragraph holds only text

#### Scenario: GFM strikethrough
- **WHEN** `parse("~~done~~")` runs
- **THEN** the paragraph holds a `Delete`

### Requirement: Recorded syntax forms
Where one node kind has more than one Markdown spelling, a parsed node SHALL
record the spelling its source used: `Emphasis` and `Strong` record whether
their delimiter is `*` or `_`, and `Autolink` records whether it was written
in angle brackets `<destination>` or as a literal autolink. A `Link` is an
inline link `[text](destination)`. A node built with a constructor SHALL take
the default spelling: `*`.

#### Scenario: Underscore delimiters
- **WHEN** `parse("_a_ __b__")` runs
- **THEN** the `Emphasis` and the `Strong` both record the `_` delimiter

#### Scenario: Link forms
- **WHEN** `parse("www.a.b <http://c.d> [e](f)")` runs
- **THEN** the paragraph holds a literal `Autolink`, an angle-bracket `Autolink`, and a `Link`, in that order

### Requirement: Serialize option types
`SerializeOptions` SHALL be `#[non_exhaustive]` and hold `line_ending:
LineEnding`, `final_newline: bool`, `bullet: Option<BulletMarker>`,
`ordered_delimiter: Option<OrderedDelimiter>`, and `fence_marker:
Option<FenceMarker>`, each marker option `None` by default. `BulletMarker`
SHALL hold only `Dash`, `Asterisk`, and `Plus`, and `OrderedDelimiter` only
`Period` and `Paren`. Both SHALL be exported from the crate root and the
prelude.

#### Scenario: Bullet that is not a bullet
- **WHEN** code assigns `ListDelimiter::Period` to `SerializeOptions::bullet`
- **THEN** it does not compile

### Requirement: Each written fact is stored once
A node SHALL hold what its source writes once and derive the rest from it.
`CharacterReference` SHALL hold only the reference as written, and
`CharacterReference::value()` SHALL return the character it decodes to, or
`None` when the reference is not exactly one character reference.
`CodeInline` SHALL hold only its value, as the parser normalizes it.
`Autolink` SHALL hold its form and its text as written, and
`Autolink::destination()` SHALL return the link target that text writes: an
angle-bracket URI itself and an angle-bracket email after `mailto:`; a literal
`www.` domain after `http://`, a literal email without a scheme after
`mailto:`, and any other literal autolink itself; or `None` when the text is
not exactly one autolink of its form.
A `Link`, an `Image`, and a `Definition` SHALL hold a title, when they have
one, as one `Title` holding its value and the quotes it is written in, so a
title without quotes or quotes without a title cannot be built.

#### Scenario: Character reference value
- **WHEN** `value()` is called on the `CharacterReference` parsed from `"&amp;"`
- **THEN** it returns `Some("&")`

#### Scenario: Hand-built reference that does not decode
- **WHEN** `value()` is called on a hand-built `CharacterReference` holding `"&amp"`
- **THEN** it returns `None`

#### Scenario: Literal autolink destination
- **WHEN** `destination()` is called on the `Autolink` parsed from `"www.a.b"`
- **THEN** it returns `Some("http://www.a.b")`

#### Scenario: Text the parser would trim
- **WHEN** `destination()` is called on a hand-built literal `Autolink` holding `"http://a.b."`
- **THEN** it returns `None`

#### Scenario: Link title
- **WHEN** `parse("[a](/u 'b')")` runs
- **THEN** the paragraph holds a `Link` whose `title` is `Some(Title::new("b", LinkTitleKind::SingleQuote))`

### Requirement: Wiki link decoding
`WikiLink::decoded_target()` and `WikiLink::decoded_label()` SHALL return the
target and the label with each backslash escape of an ASCII punctuation
character replaced by that character and each valid character reference
replaced by the text it names, the decoding CommonMark applies to a link
destination. A backslash before any other character SHALL stay as written.

#### Scenario: Escapes and references
- **WHEN** `decoded_target()` and `decoded_label()` are called on the `WikiLink` parsed from `"[[a\\|b &amp; c|x &#65; \\y]]"`
- **THEN** they return `a|b & c` and `x A \y`

#### Scenario: Compared with a link destination
- **WHEN** `"[[a&amp;b]] [x](a&amp;b)"` is parsed with `parse`
- **THEN** the `WikiLink`'s target is `a&amp;b`, and its `decoded_target()` equals the `Link`'s destination, `a&b`

## MODIFIED Requirements

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

#### Scenario: Multi-byte whitespace before an email-like run
- **WHEN** `"\u{a0}e+@"` is parsed
- **THEN** the call returns a document

#### Scenario: Generated whitespace and autolink pieces
- **WHEN** every seeded generated input built from Unicode whitespace chars and literal-autolink pieces (`www.`, `://`, `@`, `.`, `+`, `_`, letters, CJK punctuation, `[[`) is parsed
- **THEN** no call panics

### Requirement: Emphasis-like spans cover their delimiters
The span of a parsed emphasis-like container (`Emphasis`, `Strong`, `Delete`,
or `Mark`) SHALL run from the first character of the delimiters that open it
to the last character of the delimiters that close it, and SHALL lie within
the span of the node that contains it.

#### Scenario: Strong inside emphasis
- **WHEN** `parse("***a***")` runs
- **THEN** the paragraph holds an `Emphasis` spanning bytes 0..7 that holds a `Strong` spanning bytes 1..6

#### Scenario: Emphasis inside strong
- **WHEN** `parse("x ***a* b**")` runs
- **THEN** the paragraph holds a `Strong` spanning bytes 2..11 that holds an `Emphasis` spanning bytes 4..7

#### Scenario: Leftover opening delimiter
- **WHEN** `parse("**a*")` runs
- **THEN** the paragraph holds `Text("*")` spanning bytes 0..1 and an `Emphasis` spanning bytes 1..4

#### Scenario: Emphasis on a block quote continuation line
- **WHEN** `parse("> a\n> *b*")` runs
- **THEN** the block quote's paragraph holds an `Emphasis` spanning bytes 6..9

### Requirement: Spans nest
Every parsed node's span SHALL lie on UTF-8 character boundaries within the
input and within the span of the node that contains it, and the spans of a
node's children SHALL be in source order and SHALL NOT overlap.

#### Scenario: Fixture corpus and generated inputs
- **WHEN** every fixture input and every seeded generated input is parsed
- **THEN** every node, at every depth, satisfies these conditions

### Requirement: Build surface
The crate SHALL build as `no_std + alloc` with an empty default feature set, zero
runtime dependencies, and MSRV 1.82; the `html` feature SHALL be additive and
keep the same constraints.

#### Scenario: Default build
- **WHEN** the crate is built with default features
- **THEN** it compiles for `wasm32-unknown-unknown` and with Rust 1.82, pulls in no runtime dependency, and exports no HTML renderer; `scripts/pre-release-check.sh` checks the two builds before a release

#### Scenario: HTML feature
- **WHEN** the crate is built with `--features html`
- **THEN** `Document::to_html` and `Document::to_html_with` become available and the default-build parse, AST, and serializer behavior is unchanged

### Requirement: Hand construction
The AST SHALL provide `From` conversions into `Block` and `Inline` for every node
type, `From<&str>` and `From<String>` for `Text`, and `new` constructors for
`Text`, `Paragraph`, `Heading`, `Link`, `Autolink`, `CodeInline`,
`CharacterReference`, and `List`, all defaulting `meta` to no span; `markdown_syntax::prelude::*` SHALL import this surface.

#### Scenario: Build and serialize
- **WHEN** a `Document` is built from `Heading::new(1, [Text::from("Title")]).into()` and `Paragraph::new([Text::from("hello")]).into()`
- **THEN** `to_markdown()` returns `"# Title\n\nhello\n"`

## REMOVED Requirements

### Requirement: Default dialect
**Reason**: There is one syntax and no dialect to default to; "One syntax" states what `parse` recognizes.

### Requirement: Named presets
**Reason**: `SyntaxOptions` and its `commonmark()`, `gfm()`, and `mdx()` presets are removed.

### Requirement: Construct builder
**Reason**: `Construct`, `SyntaxOptions::enable`, and `SyntaxOptions::disable` are removed.

### Requirement: Configuration conflicts
**Reason**: With no configuration there is nothing to conflict; `SyntaxOptions::validate` and `SyntaxConfigError` are removed.

### Requirement: Strict parse
**Reason**: `parse_strict` and `ParseStrictError` are removed; callers check `diagnostics` for error-severity entries, as "Infallible parse" describes.
