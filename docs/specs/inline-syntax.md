# Inline syntax
Type: api

## Purpose

Recognition of inline content inside paragraphs, headings, table cells, and other
leaf blocks: emphasis and extension marks, links, code, math, autolinks, raw
HTML, and inline directives. Owned by the inline parser.

## Requirements

### Requirement: CommonMark inlines
The parser SHALL recognize CommonMark inline constructs as the CommonMark
specification 0.31.2 defines them, including its precedence of code spans,
links, and emphasis, with commonmark.js as the reference where the
specification leaves a case open, except that link text holds no link: an
autolink inside link text, image alt included, reads as its text, an
angle-bracket one without its brackets, and a wikilink, alone or in a text
directive's label, keeps the brackets around it from forming a link.

#### Scenario: Angle-bracket autolink inside link text
- **WHEN** `"[this <http://and.com> that](url)"` is parsed
- **THEN** the paragraph holds a `Link` to `url` whose only child is `Text("this http://and.com that")`

#### Scenario: Wikilink inside a link label
- **WHEN** `"[a [[b]] c](u)"` and `"[x :d[[[w]]] y](u)"` are parsed
- **THEN** neither paragraph holds a `Link`: a wikilink, alone or in a text directive's label, keeps the brackets around it from forming a link, and the first paragraph holds `Text("[a ")`, a `WikiLink` with target `b`, and `Text(" c](u)")`

### Requirement: Extension marks
The parser SHALL read `==x==` as `Mark`, parsing its content as inline content,
and `~~x~~` as `Delete`; a single `~`, a run of three or more `~`, and `++` and
`||` runs SHALL stay text.

### Requirement: Shortcodes and text directives share the colon
The parser SHALL read `:name:` as a `Shortcode` when `name` is a name in the
crate's pinned gemoji table, the source char before the opening `:` is not a
Unicode letter or digit, and the source char after the closing `:` is not one
either; which names the table holds is unspecified. It SHALL read
`:name[label]{attrs}`, with label and attributes each optional, as a
`TextDirective` only when `name` is one or more runs of ASCII letters joined by
single `-` chars, the char right after `name` is `[`, `{`, a space, a tab, or a
line ending, or `name` ends the inline content, and the char right after the
whole directive is a space, a tab, or a line ending, or the directive ends the
inline content, or that char is ASCII punctuation and the directive has a
non-empty label or attribute braces holding a non-whitespace char.

#### Scenario: Punctuation after a written label or attributes
- **WHEN** `"Inside :badge[ok]{flag}."` and `"(:e[x])"` are parsed with `parse`
- **THEN** each paragraph holds one `TextDirective`

### Requirement: Wikilinks
The parser SHALL read `[[target|label]]` on one line as a `WikiLink`, with the
target before the first `|` and the label after it, or both equal to the
whole content when it holds no `|`. A `[[` whose content up to the closing
`]]` holds an unescaped `[` or `]` SHALL open no wikilink. A `[[…]]` that
forms a wikilink SHALL be one whether or not its content matches a defined
link label. The target and label SHALL hold their source as written, backslash
escapes and character references included; `WikiLink::decoded_target()` and
`WikiLink::decoded_label()` give the text they decode to. A `!` directly before
a `[[` where a wikilink forms SHALL be part of that `WikiLink`, which is then
marked as an embed, spans from the `!`, and wins over the image the `![` would
open, as a wikilink wins over a link at a lone `[`; an escaped `\!` SHALL leave
the `WikiLink` unmarked.

#### Scenario: Extra brackets around a wikilink
- **WHEN** `"[[[foo]]]"` is parsed with `parse`
- **THEN** the paragraph holds `Text("[")`, a `WikiLink` with target `foo`, and `Text("]")`

### Requirement: Inline math needs tight delimiters
The parser SHALL form inline math only when the opening `$` is not followed and
the closing `$` is not preceded by whitespace.

### Requirement: Literal autolinks
The parser SHALL turn bare `www.`, `http://`, and `https://` URLs and email
addresses, including those written with a `mailto:` or `xmpp:` prefix, into
literal `Autolink` nodes holding the matched source text, whose destination is
that text with `http://` or `mailto:` prepended where the form needs it; it
SHALL NOT link a bare URL with any other scheme. The extent of a literal
autolink SHALL follow the GFM specification 0.29-gfm, with cmark-gfm as the
reference where the specification leaves a case open, except that it SHALL
also end before the first Unicode whitespace char, `<`, non-ASCII char in
CommonMark's Unicode punctuation set (the Unicode `P` and `S` categories)
other than the replacement char U+FFFD, or `[[`, before a `]` outside
backticks when no `[` came before it in the URL, and before a `\` followed by
ASCII punctuation other than `.`; the GFM trailing-punctuation trimming then
applies to what remains. The check on the text before an email SHALL read
whitespace as Unicode whitespace on char boundaries; a `www.` literal SHALL
start, as on GitHub, only at the start of the inline content or after one of
`*_~([]` or a space, tab, or line ending. As in cmark-gfm, a `www.`,
`http://`, or `https://` literal SHALL NOT form after a `[` or `![` that no `]`
in the same inline content has closed yet; an email address, with or without a
`mailto:` or `xmpp:` prefix, still links there. An inline footnote's `^[` opens
no link text, so a URL in its content links.

#### Scenario: URL after an open bracket
- **WHEN** `"[https://foo.com]"` and `"[a [b](c) https://x.y]"` are parsed with `parse`
- **THEN** neither holds an `Autolink`: each URL is text

### Requirement: Marks pair in closing order
The parser SHALL pair every emphasis-like mark (`*`, `_`, `~~`, and `==`) on
one delimiter stack: each closer, taken in source order, pairs with the
nearest earlier opener it can close, and openers left between the two stay
literal text. A run that can both open and close SHALL NOT close an opener
outside a `==` mark span that encloses it. A code span, inline math, raw HTML,
or autolink SHALL form before any mark around it pairs, and a mark delimiter
inside one SHALL be part of its content. A mark opened inside a link, image,
or inline footnote label SHALL pair only with a closer inside the same label.

#### Scenario: Strong closes before a highlight
- **WHEN** `"**a ==b** c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Strong` containing `a ==b` followed by `Text(" c==")`

#### Scenario: A run that can also open stays inside its mark
- **WHEN** `"*a ==~~*b*~~== c*"` is parsed with `parse`
- **THEN** the paragraph holds an `Emphasis` containing `a `, a `Mark` holding a `Delete` holding an `Emphasis` containing `b`, and ` c`

### Requirement: Footnote labels
The parser SHALL read `[^label]` as a footnote reference, and `[^label]:` as a
footnote definition, only when the label is non-empty, holds no space, tab, or
line ending, and, as a link label, holds no unescaped `[` or `]`.

### Requirement: Hard line breaks from spaces
A line ending SHALL be a hard break when two or more spaces the source holds,
and no tab, end the line; spaces or tabs a character reference writes are
text, and only the source's spaces and tabs before a soft break are removed.

### Requirement: Escapes and character references are nodes
The parser SHALL produce an `Escape` for every backslash escape and a
`CharacterReference` for every entity or numeric character reference in
inline content, including a `\|` in a table cell, and SHALL NOT fold either
into a neighbouring `Text`. Inside a code span, autolink, raw HTML, or another
construct whose content is raw text, a backslash or `&` stays part of that raw
text, except that in a table cell a `\|` inside such a construct (a code span,
inline math, raw HTML, an autolink, a wikilink, or a directive's attributes)
is read as `|` in its value.

#### Scenario: Escaped pipe inside raw HTML in a table cell
- **WHEN** `"| <a b=\"x\\|y\"> |\n|-|"` is parsed
- **THEN** the header cell holds one `Html` inline whose value is `<a b="x|y">`

### Requirement: Inline footnotes
The parser SHALL read `^[…]` as an `InlineFootnote`; a `^` that opens no
inline footnote SHALL stay text.
