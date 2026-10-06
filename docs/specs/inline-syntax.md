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
- **THEN** the paragraph holds `Text("a")`, `Escape('\\')`, a `SoftBreak`, and `Text("b")`

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
The parser SHALL read `:name:` as a `Shortcode` when shortcodes are enabled,
`name` is a name in the crate's pinned gemoji table, the source char before
the opening `:` is not a Unicode letter or digit, and the source char after
the closing `:` is not one either; it SHALL read `:name[label]{attrs}` as a
`TextDirective` when text directives are enabled, except where `name` is
followed directly by a `:`.

#### Scenario: Shortcode
- **WHEN** `"a :tada: b"` is parsed with `parse`
- **THEN** the second inline is a `Shortcode` named `tada`

#### Scenario: Digit-only gemoji name
- **WHEN** `"score :100: today"` is parsed with `parse`
- **THEN** the second inline is a `Shortcode` named `100`

#### Scenario: Clock times
- **WHEN** `"meet at 10:30:45 today"` and `"时间:10:30"` are parsed with `parse`
- **THEN** neither paragraph holds a `Shortcode`

#### Scenario: Colon after a letter
- **WHEN** `"a:smile:b"` is parsed with `parse`
- **THEN** the paragraph holds no `Shortcode`

#### Scenario: Character reference before a shortcode
- **WHEN** `"&#97;:smile:"` is parsed with `parse`
- **THEN** the paragraph holds a `CharacterReference` whose value is `a`, followed by a `Shortcode` named `smile`

#### Scenario: Name outside the gemoji table
- **WHEN** `"a :not_an_emoji_name: b"` is parsed with `parse`
- **THEN** the paragraph is the text `a :not_an_emoji_name: b`, with no `Shortcode` and no `TextDirective`

#### Scenario: Shortcode blocked by a letter after it
- **WHEN** `"x :smile:b"` is parsed with `parse`
- **THEN** the paragraph is the text `x :smile:b`

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
`http://`, `https://`, and email addresses, and when relaxed autolinks are
enabled, bare `scheme://` URLs, into `Link` nodes whose one child is a `Text`
holding the matched source text and whose destination is that text with
`http://` or `mailto:` prepended where the form needs it. A literal autolink
of either kind SHALL end before the first Unicode whitespace char, `<`, or
non-ASCII char in CommonMark's Unicode punctuation set (the Unicode `P` and
`S` categories) other than the replacement char U+FFFD, and, with wikilinks
enabled, before `[[`; the GFM trailing-punctuation trimming then applies to
what remains. The check on the text before an email SHALL read whitespace as
Unicode whitespace on char boundaries; a `www.` literal SHALL start, as on
GitHub, only after one of `*_~([]` or a space, tab, or line ending.

#### Scenario: Bare URL
- **WHEN** `"see https://example.com"` is parsed with the GFM preset
- **THEN** the paragraph holds `Text("see ")` and a `Link` to `https://example.com` whose text is `https://example.com`

#### Scenario: Full-width punctuation after a URL
- **WHEN** `"见 https://example.com/page，然后 [[笔记]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("见 ")`, a `Link` to `https://example.com/page`, `Text("，然后 ")`, and a `WikiLink` with target `笔记`

#### Scenario: Full stop after a `www` domain
- **WHEN** `"www.example.com。下一句"` is parsed with `parse`
- **THEN** the paragraph holds a `Link` to `http://www.example.com` followed by `Text("。下一句")`

#### Scenario: Attached wikilink
- **WHEN** `"see https://example.com/a[[b]] end"` is parsed with `parse`
- **THEN** the paragraph holds a `Link` to `https://example.com/a` followed by a `WikiLink` with target `b`

#### Scenario: Enumeration comma between links
- **WHEN** `"https://example.com/page#section、[[笔记#小节]]、"` is parsed with `parse`
- **THEN** the paragraph holds a `Link` to `https://example.com/page#section`, `Text("、")`, a `WikiLink`, and `Text("、")`

#### Scenario: Non-ASCII letters in a path
- **WHEN** `"https://zh.wikipedia.org/wiki/中文 x"` is parsed with the GFM preset
- **THEN** the paragraph holds a `Link` to `https://zh.wikipedia.org/wiki/中文` followed by `Text(" x")`

#### Scenario: Relaxed scheme before full-width punctuation
- **WHEN** `"见 smb://host/share，然后"` is parsed with `parse`
- **THEN** the paragraph holds `Text("见 ")`, a `Link` to `smb://host/share`, and `Text("，然后")`

#### Scenario: No-break space before an email
- **WHEN** `"\u{a0}e+@"` is parsed with `parse` and with the GFM preset
- **THEN** each parse returns a document whose paragraph holds only text

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
The parser SHALL read `<scheme:rest>` as a `Link` whose one child is a `Text`
holding the URI as written and whose destination is the URI, and `<email>` as
a `Link` to `mailto:` and the address, when the scheme is valid and the rest
holds no space, ASCII control char, `<`, or `>`; any other whitespace char is
part of the URI.

#### Scenario: No-break space in an angle-bracket autolink
- **WHEN** `"<http://a\u{a0}b>"` is parsed with the CommonMark preset
- **THEN** the paragraph holds a `Link` to `http://a\u{a0}b` whose text is `http://a\u{a0}b`

#### Scenario: Email
- **WHEN** `"<a@b.c>"` is parsed with the CommonMark preset
- **THEN** the paragraph holds a `Link` to `mailto:a@b.c` whose text is `a@b.c`

### Requirement: Hard line breaks from spaces
A line ending SHALL be a hard break when two or more spaces the source holds,
and no tab, end the line; spaces or tabs a character reference writes are
text, and only the source's spaces and tabs before a soft break are removed.

#### Scenario: Referenced space before a line ending
- **WHEN** `"a&#x20; \nb"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("a")`, a `CharacterReference` whose value is a space, a `SoftBreak`, and `Text("b")`

### Requirement: Processing instructions
Raw inline HTML SHALL read `<?` as a processing instruction only when a `?>`
after the `<?` closes it.

#### Scenario: `<?>` is text
- **WHEN** `"a<?> b"` is parsed with the CommonMark preset
- **THEN** the paragraph holds no `Html` inline

### Requirement: Escapes and character references are nodes
Under every option set, the parser SHALL produce an `Escape` for every
backslash escape and a `CharacterReference` for every entity or numeric
character reference in inline content, including a `\|` in a table cell, and
SHALL NOT fold either into a neighbouring `Text`. Inside a code span,
autolink, raw HTML, or another construct whose content is raw text, a
backslash or `&` stays part of that raw text, except that in a table cell a
`\|` inside such a construct (a code span, inline math, raw HTML, an
autolink, a wikilink, a directive's attributes, or MDX) is read as `|` in its
value.

#### Scenario: Escaped punctuation
- **WHEN** `"\\*not em\\* and \\#tag"` is parsed with `parse`
- **THEN** the paragraph holds `Escape('*')`, `Text("not em")`, `Escape('*')`, `Text(" and ")`, `Escape('#')`, and `Text("tag")`

#### Scenario: Numeric character reference
- **WHEN** `"&#35;tag"` is parsed with `parse`
- **THEN** the paragraph holds a `CharacterReference` whose value is `#`, followed by `Text("tag")`

#### Scenario: Escaped pipe in a table cell
- **WHEN** `"| a\\|b |\n|-|"` is parsed with the GFM preset
- **THEN** the header cell holds `Text("a")`, `Escape('|')`, and `Text("b")`

#### Scenario: Escaped pipe inside raw HTML in a table cell
- **WHEN** `"| <a b=\"x\\|y\"> |\n|-|"` is parsed with the GFM preset
- **THEN** the header cell holds one `Html` inline whose value is `<a b="x|y">`

#### Scenario: Escaped pipe inside math in a table cell
- **WHEN** `"$\\|$||\n-|-"` is parsed with `parse`
- **THEN** the first header cell holds a dollar `Math` whose value is `|`

#### Scenario: Backslash inside a code span
- **WHEN** ``"`\\*`"`` is parsed with `parse`
- **THEN** the paragraph holds one code span whose value is `\*` and no `Escape`

### Requirement: Wiki embeds
When wikilinks are enabled, a `!` directly before a `[[` where a wikilink
forms SHALL be part of that `WikiLink`, which is then marked as an embed,
spans from the `!`, and wins over the image the `![` would open, as a wikilink
wins over a link at a lone `[`; an escaped `\!` SHALL leave the `WikiLink`
unmarked.

#### Scenario: Embed
- **WHEN** `"see ![[x.png]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("see ")` and a `WikiLink` with target `x.png` marked as an embed, spanning bytes 4..14

#### Scenario: Escaped bang
- **WHEN** `"\\![[x.png]]"` is parsed with `parse`
- **THEN** the paragraph holds `Escape('!')` and a `WikiLink` with target `x.png` not marked as an embed

#### Scenario: Embed before a link destination
- **WHEN** `"![[a]](u)"` is parsed with `parse`
- **THEN** the paragraph holds a `WikiLink` with target `a` marked as an embed, followed by `Text("(u)")`, and no `Image`

#### Scenario: Bang before brackets that form no wikilink
- **WHEN** `"![[x]"` is parsed with `parse`
- **THEN** the paragraph holds no `WikiLink`

### Requirement: Underscore runs beside a tilde
A `_` run SHALL get no strikethrough bonus from a neighbouring `~`: beside a
`~` it opens and closes by the same rules that apply to it beside any other
punctuation char, under every option set.

#### Scenario: Underscores around a tilde
- **WHEN** `"d_~_"` is parsed with `parse`
- **THEN** the paragraph holds only text, reading `d_~_`, and no `Emphasis`
