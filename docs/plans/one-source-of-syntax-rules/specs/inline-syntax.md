# Inline syntax — spec changes

## ADDED Requirements

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
When wikilinks are enabled, a `!` directly before a wikilink's `[[` SHALL be
part of that `WikiLink`, which is then marked as an embed and spans from the
`!`; an escaped `\!` SHALL leave the `WikiLink` unmarked.

#### Scenario: Embed
- **WHEN** `"see ![[x.png]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("see ")` and a `WikiLink` with target `x.png` marked as an embed, spanning bytes 4..14

#### Scenario: Escaped bang
- **WHEN** `"\\![[x.png]]"` is parsed with `parse`
- **THEN** the paragraph holds `Escape('!')` and a `WikiLink` with target `x.png` not marked as an embed

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
- **THEN** the paragraph holds `Text("a")`, `Escape('\\')`, a `SoftBreak`, and `Text("b")`

#### Scenario: Space inside a bare destination's parentheses
- **WHEN** `"[a](( ))"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("[a](( ))")` and no `Link`

#### Scenario: CommonMark oracle cases
- **WHEN** the inline cases under `tests/fixtures/conformance/commonmark/` are parsed and rendered with the `html` feature
- **THEN** the output matches the expected HTML

### Requirement: Hard line breaks from spaces
A line ending SHALL be a hard break when two or more spaces the source holds,
and no tab, end the line; spaces or tabs a character reference writes are
text, and only the source's spaces and tabs before a soft break are removed.

#### Scenario: Referenced space before a line ending
- **WHEN** `"a&#x20; \nb"` is parsed with the CommonMark preset
- **THEN** the paragraph holds `Text("a")`, a `CharacterReference` whose value is a space, a `SoftBreak`, and `Text("b")`

### Requirement: Shortcodes and text directives share the colon
The parser SHALL read `:name:` as a `Shortcode` when shortcodes are enabled,
`name` is a name in the crate's pinned gemoji table, the source char before
the opening `:` is not a Unicode letter or digit, and the source char after
the closing `:` is not one either; it SHALL read `:name[label]{attrs}` as a
`TextDirective` when text directives are enabled.

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
- **THEN** the paragraph holds no `Shortcode`

#### Scenario: Text directive
- **WHEN** `":abbr[HTML]{title=\"Hyper\"}"` is parsed with `parse`
- **THEN** the paragraph holds a `TextDirective` named `abbr` with label `HTML` and attribute `title="Hyper"`

### Requirement: Literal autolinks
When GFM literal autolinks are enabled, the parser SHALL turn bare `www.`,
`http://`, `https://`, and email addresses, and when relaxed autolinks are
enabled, bare `scheme://` URLs, into `Link` nodes whose one child is a `Text`
holding the matched source text and whose destination is that text with
`http://` or `mailto:` prepended where the form needs it. A literal autolink
of either kind SHALL end before the first Unicode whitespace char, `<`, or
non-ASCII char in CommonMark's Unicode punctuation set (the Unicode `P` and
`S` categories), and, with wikilinks enabled, before `[[`;
the GFM trailing-punctuation trimming then applies to what remains. Every
boundary check SHALL read whitespace as Unicode whitespace.

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
