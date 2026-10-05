# Inline syntax — spec changes

## MODIFIED Requirements

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

#### Scenario: Shortcut reference before an unclosed label
- **WHEN** `"[foo][bar\n\n[foo]: /u"` is parsed with the CommonMark preset
- **THEN** the paragraph holds a shortcut `LinkReference` to `foo` followed by `Text("[bar")`

#### Scenario: Rule of three counts whole delimiter runs
- **WHEN** `"*a***b*"` is parsed with the CommonMark preset
- **THEN** the paragraph holds an `Emphasis` containing `a`, `Text("*")`, and an `Emphasis` containing `b`

#### Scenario: Image whose resource is invalid
- **WHEN** `"![foo](a b)\n\n[foo]: /u"` is parsed with the CommonMark preset
- **THEN** the paragraph holds a shortcut `ImageReference` to `foo` followed by `Text("(a b)")`

#### Scenario: Underscore after Unicode punctuation
- **WHEN** `"«_**]**_"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("«")` and an `Emphasis` containing a `Strong` containing `]`

#### Scenario: Escaped backslash before a line ending
- **WHEN** `"a\\\\\nb"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("a\\")`, a `SoftBreak`, and `Text("b")`

#### Scenario: Space inside a bare destination's parentheses
- **WHEN** `"[a](( ))"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("[a](( ))")` and no `Link`

#### Scenario: CommonMark oracle cases
- **WHEN** the inline cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** the output matches the expected HTML

## ADDED Requirements

### Requirement: Footnote labels
The parser SHALL read `[^label]` as a footnote reference, and `[^label]:` as a
footnote definition, only when the label is non-empty, holds no space, tab, or
line ending, and, as a link label, holds no unescaped `[` or `]`.

#### Scenario: Bracket inside a footnote label
- **WHEN** `"^*[^[^]]"` and `"[^a[b]"` are parsed with `parse`
- **THEN** neither paragraph holds a `FootnoteReference`, while `"[^a\\[b]"` holds one

### Requirement: Reference label matching
Two link labels SHALL match when they agree after Unicode case folding,
trimming, and collapsing each run of spaces, tabs, and line endings to one
space; any other whitespace char is matched as written.

#### Scenario: No-break space in a label
- **WHEN** `"[a\u{a0}b]\n\n[a b]: /u"` is parsed
- **THEN** the paragraph holds no `LinkReference`

### Requirement: Angle-bracket autolink URI
The parser SHALL read `<scheme:rest>` as an autolink when the scheme is valid
and the rest holds no space, ASCII control char, `<`, or `>`; any other
whitespace char is part of the URI.

#### Scenario: No-break space in an angle-bracket autolink
- **WHEN** `"<http://a\u{a0}b>"` is parsed with the CommonMark preset
- **THEN** the paragraph holds an `Autolink` to `http://a\u{a0}b`
