# Inline syntax

## Purpose

Recognition of inline content inside paragraphs, headings, table cells, and other
leaf blocks: emphasis and extension marks, links, code, math, autolinks, raw
HTML, and the inline directive and MDX forms. Owned by the inline parser in
`src/parse.rs`.

## Requirements

### Requirement: CommonMark inlines
The parser SHALL recognize CommonMark inline constructs (backslash escapes,
entity and numeric character references, code spans, emphasis and strong
emphasis, links, images, autolinks, raw HTML, and hard and soft line breaks) as
the CommonMark specification defines them, including its precedence of code
spans, links, and emphasis.

#### Scenario: Emphasis
- **WHEN** `"Hello *world*."` is parsed
- **THEN** the paragraph holds `Text("Hello ")`, an `Emphasis` containing `world`, and `Text(".")`

#### Scenario: Link inside a link label
- **WHEN** `"[foo [bar](/u)](/v)"` is parsed
- **THEN** only `[bar](/u)` becomes a link and the surrounding brackets and `(/v)` stay text

#### Scenario: CommonMark oracle cases
- **WHEN** the inline cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** the output matches the expected HTML

### Requirement: Strikethrough and subscript share the tilde
With strikethrough and subscript both enabled and single-tilde strikethrough off,
the parser SHALL read `~~x~~` as `Delete` and `~x~` as `Subscript`.

#### Scenario: Double tilde
- **WHEN** `"~~s~~"` is parsed with `parse`
- **THEN** the paragraph holds a `Delete`

#### Scenario: Single tilde
- **WHEN** `"H~2~O"` is parsed with `parse`
- **THEN** the paragraph holds `Text("H")`, a `Subscript` containing `2`, and `Text("O")`

### Requirement: Superscript and inline footnotes share the caret
The parser SHALL read `^[…]` as an inline footnote when inline footnotes are
enabled, and `^x^` as `Superscript` otherwise.

#### Scenario: Inline footnote
- **WHEN** `"note^[x] tail"` is parsed with `parse`
- **THEN** the second inline is an `InlineFootnote`

#### Scenario: Superscript
- **WHEN** `"x^2^"` is parsed with `parse`
- **THEN** the second inline is a `Superscript`

### Requirement: Single-line subscript and superscript
A `~` subscript or `^` superscript SHALL close at the first same marker on the
same line, and SHALL NOT form when no such marker follows on that line or when it
would be empty.

#### Scenario: First closer wins
- **WHEN** `"~a ~b~"` is parsed with `parse`
- **THEN** the paragraph holds a `Subscript` containing `a ` followed by `Text("b~")`

### Requirement: Underline is opt-in
With `underline` enabled, the parser SHALL read `__x__` as `Underline` instead of
`Strong`; with it disabled, `__x__` SHALL stay CommonMark strong.

#### Scenario: Default
- **WHEN** `"a __b__ c"` is parsed with `parse`
- **THEN** the second inline is `Strong`

#### Scenario: Enabled
- **WHEN** `"a __b__ c"` is parsed with `Construct::Underline` enabled
- **THEN** the second inline is `Underline`

### Requirement: Extension marks
When enabled, the parser SHALL read `++x++` as `Insert`, `==x==` as `Mark`,
and `||x||` as `Spoiler`, parsing their content as inline content.

#### Scenario: Highlight with nested emphasis
- **WHEN** `"==a *b* c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a `, an `Emphasis` containing `b`, and ` c`

### Requirement: Extension marks claim the nearest closer
An extension mark (`++`, `==`, `||`, `~`, `^`, or underline `__`) with a closer
SHALL span up to its first closer, with its content parsed as a nested span that
emphasis delimiters cannot pair across.

#### Scenario: Strong crossing a highlight
- **WHEN** `"**a ==b** c=="` is parsed with `parse`
- **THEN** the paragraph holds `Text("**a ")` and a `Mark` containing `b** c`

#### Scenario: Emphasis crossing an insert
- **WHEN** `"++a *b++ c*"` is parsed with `parse`
- **THEN** the paragraph holds an `Insert` containing `a *b` followed by `Text(" c*")`

### Requirement: Shortcodes and text directives share the colon
The parser SHALL read `:word:` as a `Shortcode` and `:name[label]{attrs}` as a
`TextDirective` when the respective constructs are enabled.

#### Scenario: Shortcode
- **WHEN** `"a :tada: b"` is parsed with `parse`
- **THEN** the second inline is a `Shortcode`

#### Scenario: Text directive
- **WHEN** `":abbr[HTML]{title=\"Hyper\"}"` is parsed with `parse`
- **THEN** the paragraph holds a `TextDirective` named `abbr` with label `HTML` and attribute `title="Hyper"`

### Requirement: Wikilinks
When enabled, the parser SHALL read `[[target|label]]` on one line as a
`WikiLink`, splitting target and label by the configured title order
(title-after-pipe by default).

#### Scenario: Default order
- **WHEN** `"see [[target|label]] here"` is parsed with `parse`
- **THEN** the second inline is a `WikiLink` with target `target`, label `label`, and order `AfterPipe`

### Requirement: Inline math needs tight delimiters
The parser SHALL form inline math only when the opening `$` is not followed and
the closing `$` is not preceded by whitespace.

#### Scenario: Dollar amounts
- **WHEN** `"price $5 to $10 today"` is parsed with `parse`
- **THEN** no inline is `Math`

### Requirement: Literal autolinks
When GFM literal autolinks are enabled, the parser SHALL turn bare `www.`,
`http://`, `https://`, and email addresses into `Autolink` nodes.

#### Scenario: Bare URL
- **WHEN** `"see https://example.com"` is parsed with the GFM preset
- **THEN** the paragraph holds an `Autolink` to `https://example.com`

### Requirement: MDX inline constructs
In MDX mode the parser SHALL recognize inline JSX elements and inline `{…}`
expressions, keeping each element's source text as the node value, and SHALL
not recognize raw HTML.

#### Scenario: Inline JSX
- **WHEN** `"A <Note kind=\"x\">Para {props.v}</Note> inline."` is parsed with the MDX preset
- **THEN** the paragraph holds `Text("A ")`, an inline MDX JSX node whose value is `<Note kind="x">Para {props.v}</Note>`, and `Text(" inline.")`
