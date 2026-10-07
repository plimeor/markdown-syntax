# Inline syntax

## Purpose

Recognition of inline content inside paragraphs, headings, table cells, and other
leaf blocks: emphasis and extension marks, links, code, math, autolinks, raw
HTML, and inline directives. Owned by the inline parser in `src/parse.rs`.

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
- **WHEN** `"[foo][bar\n\n[foo]: /u"` is parsed
- **THEN** the paragraph holds a shortcut `LinkReference` to `foo` followed by `Text("[bar")`

#### Scenario: Rule of three counts whole delimiter runs
- **WHEN** `"*a***b*"` is parsed
- **THEN** the paragraph holds an `Emphasis` containing `a`, `Text("*")`, and an `Emphasis` containing `b`

#### Scenario: Image whose resource is invalid
- **WHEN** `"![foo](a b)\n\n[foo]: /u"` is parsed
- **THEN** the paragraph holds a shortcut `ImageReference` to `foo` followed by `Text("(a b)")`

#### Scenario: Underscore after Unicode punctuation
- **WHEN** `"«_**]**_"` is parsed
- **THEN** the paragraph holds `Text("«")` and an `Emphasis` containing a `Strong` containing `]`

#### Scenario: Escaped backslash before a line ending
- **WHEN** `"a\\\\\nb"` is parsed
- **THEN** the paragraph holds `Text("a")`, `Escape('\\')`, a `SoftBreak`, and `Text("b")`

#### Scenario: Space inside a bare destination's parentheses
- **WHEN** `"[a](( ))"` is parsed
- **THEN** the paragraph holds `Text("[a](( ))")` and no `Link`

#### Scenario: CommonMark oracle cases
- **WHEN** the inline cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** each output matches the expected HTML, or the case is in the bench's deviation list with the reason it differs

### Requirement: Extension marks
The parser SHALL read `==x==` as `Mark`, parsing its content as inline content;
`++` and `||` runs SHALL stay text.

#### Scenario: Highlight with nested emphasis
- **WHEN** `"==a *b* c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a `, an `Emphasis` containing `b`, and ` c`

#### Scenario: Plus and bar runs
- **WHEN** `"++a++ ||b||"` is parsed with `parse`
- **THEN** the paragraph holds only text

### Requirement: Shortcodes and text directives share the colon
The parser SHALL read `:name:` as a `Shortcode` when `name` is a name in the
crate's pinned gemoji table, the source char before the opening `:` is not a
Unicode letter or digit, and the source char after the closing `:` is not one
either. It SHALL read `:name[label]{attrs}`, with label and attributes each
optional, as a `TextDirective` only when `name` is one or more runs of ASCII
letters joined by single `-` chars, the char right after `name` is `[`, `{`,
a space, a tab, or a line ending, or `name` ends the inline content, and the
char right after the whole directive is a space, a tab, or a line ending, or
the directive ends the inline content, or that char is ASCII punctuation and
the directive has a non-empty label or attribute braces holding a
non-whitespace char.

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

#### Scenario: Bare text directive
- **WHEN** `"see :call-out here"` is parsed with `parse`
- **THEN** the paragraph holds a `TextDirective` named `call-out` with no label and no attributes

#### Scenario: Email after a colon
- **WHEN** `":noreply@example.com"` is parsed with `parse`
- **THEN** the paragraph holds `Text(":")` and an `Autolink` to `mailto:noreply@example.com`, and no `TextDirective`

#### Scenario: Domain after a colon
- **WHEN** `":www.example.com"` is parsed with `parse`
- **THEN** the paragraph holds only text

#### Scenario: Name with a digit, an underscore, or trailing punctuation
- **WHEN** `":h1[x]"`, `":my_note"`, `"(:note)"`, and `"see :note."` are parsed with `parse`
- **THEN** none of the paragraphs holds a `TextDirective`

#### Scenario: Punctuation after a written label or attributes
- **WHEN** `"Inside :badge[ok]{flag}."` and `"(:e[x])"` are parsed with `parse`
- **THEN** each paragraph holds one `TextDirective`

#### Scenario: Other chars after a directive
- **WHEN** `":e{}x"`, `":e{}1"`, `":e[]www.a.b"`, `":e{}a@b.c"`, `":e{}[^1]"`, `":e[a]b"`, and `":e{}."` are parsed with `parse`
- **THEN** none of the paragraphs holds a `TextDirective`

### Requirement: Wikilinks
The parser SHALL read `[[target|label]]` on one line as a `WikiLink`, with the
target before the first `|` and the label after it, or both equal to the
whole content when it holds no `|`. A `[[` whose content up to the closing
`]]` holds an unescaped `[` or `]` SHALL open no wikilink. A `[[…]]` that
forms a wikilink SHALL be one whether or not its content matches a defined
link label. The target and label SHALL hold their source as written, backslash
escapes and character references included; `WikiLink::decoded_target()` and
`WikiLink::decoded_label()` give the text they decode to.

#### Scenario: Target and label
- **WHEN** `"see [[target|label]] here"` is parsed with `parse`
- **THEN** the second inline is a `WikiLink` with target `target` and label `label`

#### Scenario: Escapes stay as written
- **WHEN** `"[[a\\$b|x &amp; y]]"` is parsed with `parse`
- **THEN** the `WikiLink` has target `a\$b` and label `x &amp; y`

#### Scenario: Extra brackets around a wikilink
- **WHEN** `"[[[foo]]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("[")`, a `WikiLink` with target `foo`, and `Text("]")`

#### Scenario: Defined label
- **WHEN** `"[[foo]]\n\n[foo]: /u"` is parsed with `parse`
- **THEN** the paragraph holds a `WikiLink` with target `foo` and no `LinkReference`

### Requirement: Inline math needs tight delimiters
The parser SHALL form inline math only when the opening `$` is not followed and
the closing `$` is not preceded by whitespace.

#### Scenario: Dollar amounts
- **WHEN** `"price $5 to $10 today"` is parsed with `parse`
- **THEN** no inline is `Math`

### Requirement: Literal autolinks
The parser SHALL turn bare `www.`, `http://`, and `https://` URLs and email
addresses, including those written with a `mailto:` or `xmpp:` prefix, into
literal `Autolink` nodes holding the matched source text, whose destination is
that text with `http://` or `mailto:` prepended where the form needs it; it
SHALL NOT link a bare URL with any other scheme. The extent
of a literal autolink SHALL follow the GFM specification, except that it SHALL
also end before the first Unicode whitespace char, `<`, non-ASCII char in
CommonMark's Unicode punctuation set (the Unicode `P` and `S` categories)
other than the replacement char U+FFFD, or `[[`, before a `]` outside
backticks when no `[` came before it in the URL, and before a `\` followed by
ASCII punctuation other than `.`; the GFM trailing-punctuation trimming then
applies to what remains. The check on the text before an email
SHALL read whitespace as Unicode whitespace on char boundaries; a `www.`
literal SHALL start, as on GitHub, only after one of `*_~([]` or a space,
tab, or line ending. As in cmark-gfm, a `www.`, `http://`, or `https://`
literal SHALL NOT form after a `[` that no `]` in the same inline content has
closed yet; an email address, with or without a `mailto:` or `xmpp:` prefix,
still links there.

#### Scenario: URL after an open bracket
- **WHEN** `"[https://foo.com]"` and `"[a [b](c) https://x.y]"` are parsed with `parse`
- **THEN** neither holds an `Autolink`: each URL is text

#### Scenario: Email after an open bracket
- **WHEN** `"[a@b.com]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("[")`, a literal `Autolink` whose text is `a@b.com`, and `Text("]")`

#### Scenario: Backslash escape after a URL
- **WHEN** `"www.a.com\\*x"` is parsed with `parse`
- **THEN** the paragraph holds a literal `Autolink` whose text is `www.a.com`, an `Escape` of `*`, and `Text("x")`

#### Scenario: Unopened bracket after a URL
- **WHEN** `"https://a.b/c]d"` is parsed with `parse`
- **THEN** the paragraph holds a literal `Autolink` whose text is `https://a.b/c` followed by `Text("]d")`

#### Scenario: Trailing numeric reference
- **WHEN** `"www.a.b&#x41;"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `http://www.a.b&#x41` and `Text(";")`

#### Scenario: Bare URL
- **WHEN** `"see https://example.com"` is parsed with `parse`
- **THEN** the paragraph holds `Text("see ")` and an `Autolink` to `https://example.com` whose text is `https://example.com`

#### Scenario: Full-width punctuation after a URL
- **WHEN** `"见 https://example.com/page，然后 [[笔记]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("见 ")`, an `Autolink` to `https://example.com/page`, `Text("，然后 ")`, and a `WikiLink` with target `笔记`

#### Scenario: Full stop after a `www` domain
- **WHEN** `"www.example.com。下一句"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `http://www.example.com` followed by `Text("。下一句")`

#### Scenario: Attached wikilink
- **WHEN** `"see https://example.com/a[[b]] end"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `https://example.com/a` followed by a `WikiLink` with target `b`

#### Scenario: Enumeration comma between links
- **WHEN** `"https://example.com/page#section、[[笔记#小节]]、"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `https://example.com/page#section`, `Text("、")`, a `WikiLink`, and `Text("、")`

#### Scenario: Non-ASCII letters in a path
- **WHEN** `"https://zh.wikipedia.org/wiki/中文 x"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `https://zh.wikipedia.org/wiki/中文` followed by `Text(" x")`

#### Scenario: Other schemes stay text
- **WHEN** `"见 smb://host/share，然后 ftp://a.b"` is parsed with `parse`
- **THEN** the paragraph holds only text

#### Scenario: Prefixed email forms
- **WHEN** `"mailto:a@b.c and xmpp:a@b.c/r"` is parsed with `parse`
- **THEN** the paragraph holds an `Autolink` to `mailto:a@b.c`, `Text(" and ")`, and an `Autolink` to `xmpp:a@b.c/r`

#### Scenario: No-break space before an email
- **WHEN** `"\u{a0}e+@"` is parsed with `parse`
- **THEN** the parse returns a document whose paragraph holds only text

### Requirement: Marks pair in closing order
The parser SHALL pair every emphasis-like mark (`*`, `_`, `~~`, and `==`) on
one delimiter stack: each closer, taken in source order, pairs with the
nearest earlier opener it can close, and openers left between the two stay
literal text. A run that can both open and close SHALL NOT close an opener
outside a `==` mark span that encloses it.

#### Scenario: Strong closes before a highlight
- **WHEN** `"**a ==b** c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Strong` containing `a ==b` followed by `Text(" c==")`

#### Scenario: Highlight closes before strong
- **WHEN** `"==a **b== c**"` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a **b` followed by `Text(" c**")`

#### Scenario: Marks that do not cross
- **WHEN** `"==a *b* c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Mark` containing `a `, an `Emphasis` containing `b`, and ` c`

#### Scenario: A run that can also open stays inside its mark
- **WHEN** `"*a ==~~*b*~~== c*"` is parsed with `parse`
- **THEN** the paragraph holds an `Emphasis` containing `a `, a `Mark` holding a `Delete` holding an `Emphasis` containing `b`, and ` c`

### Requirement: Atomic constructs bind tighter than marks
A code span, inline math, raw HTML, or autolink SHALL form before any mark
around it pairs, and a mark delimiter inside one SHALL be part of its content.

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
The parser SHALL read `<scheme:rest>` as an angle-bracket `Autolink` holding
the URI as written, whose destination is the URI, and `<email>` as one to
`mailto:` and the address, when the scheme is valid and the rest
holds no space, ASCII control char, `<`, or `>`; any other whitespace char is
part of the URI.

#### Scenario: No-break space in an angle-bracket autolink
- **WHEN** `"<http://a\u{a0}b>"` is parsed
- **THEN** the paragraph holds an `Autolink` to `http://a\u{a0}b` whose text is `http://a\u{a0}b`

#### Scenario: Email
- **WHEN** `"<a@b.c>"` is parsed
- **THEN** the paragraph holds an `Autolink` to `mailto:a@b.c` whose text is `a@b.c`

### Requirement: Hard line breaks from spaces
A line ending SHALL be a hard break when two or more spaces the source holds,
and no tab, end the line; spaces or tabs a character reference writes are
text, and only the source's spaces and tabs before a soft break are removed.

#### Scenario: Referenced space before a line ending
- **WHEN** `"a&#x20; \nb"` is parsed
- **THEN** the paragraph holds `Text("a")`, a `CharacterReference` whose value is a space, a `SoftBreak`, and `Text("b")`

### Requirement: Processing instructions
Raw inline HTML SHALL read `<?` as a processing instruction only when a `?>`
after the `<?` closes it.

#### Scenario: `<?>` is text
- **WHEN** `"a<?> b"` is parsed
- **THEN** the paragraph holds no `Html` inline

### Requirement: Escapes and character references are nodes
The parser SHALL produce an `Escape` for every backslash escape and a
`CharacterReference` for every entity or numeric character reference in
inline content, including a `\|` in a table cell, and SHALL NOT fold either
into a neighbouring `Text`. Inside a code span, autolink, raw HTML, or another
construct whose content is raw text, a backslash or `&` stays part of that raw
text, except that in a table cell a `\|` inside such a construct (a code span,
inline math, raw HTML, an autolink, a wikilink, or a directive's attributes)
is read as `|` in its value.

#### Scenario: Escaped punctuation
- **WHEN** `"\\*not em\\* and \\#tag"` is parsed with `parse`
- **THEN** the paragraph holds `Escape('*')`, `Text("not em")`, `Escape('*')`, `Text(" and ")`, `Escape('#')`, and `Text("tag")`

#### Scenario: Numeric character reference
- **WHEN** `"&#35;tag"` is parsed with `parse`
- **THEN** the paragraph holds a `CharacterReference` whose value is `#`, followed by `Text("tag")`

#### Scenario: Escaped pipe in a table cell
- **WHEN** `"| a\\|b |\n|-|"` is parsed
- **THEN** the header cell holds `Text("a")`, `Escape('|')`, and `Text("b")`

#### Scenario: Escaped pipe inside raw HTML in a table cell
- **WHEN** `"| <a b=\"x\\|y\"> |\n|-|"` is parsed
- **THEN** the header cell holds one `Html` inline whose value is `<a b="x|y">`

#### Scenario: Escaped pipe inside math in a table cell
- **WHEN** `"$\\|$||\n-|-"` is parsed with `parse`
- **THEN** the first header cell holds a dollar `Math` whose value is `|`

#### Scenario: Backslash inside a code span
- **WHEN** ``"`\\*`"`` is parsed with `parse`
- **THEN** the paragraph holds one code span whose value is `\*` and no `Escape`

### Requirement: Wiki embeds
A `!` directly before a `[[` where a wikilink forms SHALL be part of that
`WikiLink`, which is then marked as an embed, spans from the `!`, and wins
over the image the `![` would open, as a wikilink wins over a link at a lone
`[`; an escaped `\!` SHALL leave the `WikiLink` unmarked.

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
punctuation char.

#### Scenario: Underscores around a tilde
- **WHEN** `"d_~_"` is parsed with `parse`
- **THEN** the paragraph holds only text, reading `d_~_`, and no `Emphasis`

### Requirement: Strikethrough takes two tildes
The parser SHALL read `~~x~~` as `Delete`; a single `~`, and a run of three or
more, SHALL stay text.

#### Scenario: Double tilde
- **WHEN** `"~~s~~"` is parsed with `parse`
- **THEN** the paragraph holds a `Delete`

#### Scenario: Single and triple tildes
- **WHEN** `"H~2~O ~~~a~~~"` is parsed with `parse`
- **THEN** the paragraph holds only text

### Requirement: Inline footnotes
The parser SHALL read `^[…]` as an `InlineFootnote`; a `^` that opens no
inline footnote SHALL stay text.

#### Scenario: Inline footnote
- **WHEN** `"note^[x] tail"` is parsed with `parse`
- **THEN** the second inline is an `InlineFootnote`

#### Scenario: Caret pair
- **WHEN** `"x^2^"` is parsed with `parse`
- **THEN** the paragraph holds only text

### Requirement: Double underscore is strong
The parser SHALL read `__x__` as CommonMark strong emphasis.

#### Scenario: Strong
- **WHEN** `"a __b__ c"` is parsed with `parse`
- **THEN** the second inline is `Strong`
