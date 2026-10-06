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
enabled, unless a `^` earlier on the same line, and not directly before it,
opened a superscript that no `^` has closed since and that is not inside a
bracket label resolved since, in which case that `^` closes the superscript; it
SHALL read `^x^` as `Superscript` otherwise. Whether the superscript survives
the marks around it is not considered: when a crossing mark leaves it unpaired,
the closing `^` stays text.

#### Scenario: Inline footnote
- **WHEN** `"note^[x] tail"` is parsed with `parse`
- **THEN** the second inline is an `InlineFootnote`

#### Scenario: Superscript
- **WHEN** `"x^2^"` is parsed with `parse`
- **THEN** the second inline is a `Superscript`

#### Scenario: Superscript closing before a bracket
- **WHEN** `"a^b^[link](u)"` is parsed with `parse`
- **THEN** the paragraph holds `Text("a")`, a `Superscript` containing `b`, and a `Link` whose text is `link`

#### Scenario: Superscript dropped by a crossing mark
- **WHEN** `"*a ^b* ^[x]"` is parsed with `parse`
- **THEN** the paragraph holds an `Emphasis` containing `a ^b` followed by `Text(" ^[x]")`

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

### Requirement: Marks pair in closing order
The parser SHALL pair every emphasis-like mark (`*`, `_`, `~~`, `~`, `^`, `++`,
`==`, `||`, and underline `__`) on one delimiter stack: each closer, taken in
source order, pairs with the nearest earlier opener it can close, and openers
left between the two stay literal text. A run that can both open and close
SHALL NOT close an opener outside a mark span (`++`, `==`, `||`, `~`, `^`, or
underline `__`) that encloses it.

#### Scenario: Strong closes before a highlight
- **WHEN** `"**a ==b** c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Strong` containing `a ==b` followed by `Text(" c==")`

#### Scenario: Emphasis closes before an insert
- **WHEN** `"*a ++b* c++"` is parsed with `parse`
- **THEN** the paragraph holds an `Emphasis` containing `a ++b` followed by `Text(" c++")`

#### Scenario: Highlight closes before strong
- **WHEN** `"==a **b== c**"` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a **b` followed by `Text(" c**")`

#### Scenario: Marks that do not cross
- **WHEN** `"==a *b* c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a `, an `Emphasis` containing `b`, and ` c`

#### Scenario: A run that can also open stays inside its mark
- **WHEN** `"*a ||~~*b*~~|| c*"` is parsed with `parse`
- **THEN** the paragraph holds an `Emphasis` containing `a `, a `Spoiler` holding a `Delete` holding an `Emphasis` containing `b`, and ` c`

### Requirement: Atomic constructs bind tighter than marks
A code span, inline math, raw HTML, or autolink SHALL form before any mark
around it pairs, and a mark delimiter inside one SHALL be part of its content.

#### Scenario: Caret inside a code span
- **WHEN** `"^a `^` b^"` is parsed with `parse`
- **THEN** the paragraph holds a `Superscript` containing `a `, a code span `^`, and ` b`

#### Scenario: Highlight delimiters inside a code span
- **WHEN** `"==a `b== c` d=="` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a `, a code span `b== c`, and ` d`

### Requirement: Marks inside a link stay inside it
A mark opened inside a link, image, or inline footnote label SHALL pair only
with a closer inside the same label.

#### Scenario: Highlight opener inside a link
- **WHEN** `"[a ==b](u) c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Link` whose text is `a ==b` followed by `Text(" c==")`

#### Scenario: Emphasis opener before a link
- **WHEN** `"*[foo*](/u)"` is parsed with `parse`
- **THEN** the paragraph holds `Text("*")` followed by a `Link` whose text is `foo*`

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

### Requirement: Hard line breaks from spaces
A line ending SHALL be a hard break when two or more spaces the source holds,
and no tab, end the line; spaces or tabs a character reference writes are
text, and only the source's spaces and tabs before a soft break are removed.

#### Scenario: Referenced space before a line ending
- **WHEN** `"a&#x20; \nb"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("a ")`, a `SoftBreak`, and `Text("b")`

### Requirement: Processing instructions
Raw inline HTML SHALL read `<?` as a processing instruction only when a `?>`
after the `<?` closes it.

#### Scenario: `<?>` is text
- **WHEN** `"a<?> b"` is parsed with the CommonMark preset
- **THEN** the paragraph holds no `Html` inline
