# Public API — spec changes

## ADDED Requirements

### Requirement: One syntax
`parse(input)` SHALL be the only way to parse, and SHALL recognize one fixed
syntax with no configuration: CommonMark with raw HTML and indented code,
`<details>` HTML containers, GFM tables, task list items, literal autolinks,
and `~~` strikethrough, footnotes and inline footnotes, GitHub alerts,
frontmatter, shortcodes, `==` highlight, wiki links, math, and directives.
Subscript, superscript, insert, spoiler, underline, description lists,
scheme-less and non-HTTP literal autolinks, and MDX SHALL NOT be recognized.

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
their delimiter is `*` or `_`, and `Link` records whether it was written as an
inline link `[text](destination)`, an angle-bracket autolink `<destination>`,
or a literal autolink. A node built with a constructor SHALL take the default
spelling: `*`, and an inline link.

#### Scenario: Underscore delimiters
- **WHEN** `parse("_a_ __b__")` runs
- **THEN** the `Emphasis` and the `Strong` both record the `_` delimiter

#### Scenario: Link forms
- **WHEN** `parse("www.a.b <http://c.d> [e](f)")` runs
- **THEN** the three `Link` nodes record the literal-autolink, angle-bracket-autolink, and inline forms, in that order

#### Scenario: Constructed link
- **WHEN** a `Link` is built with `Link::new("u", [Text::from("a")])`
- **THEN** it records the inline form

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
