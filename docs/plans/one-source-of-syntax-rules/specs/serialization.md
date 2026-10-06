# Serialization — spec changes

## ADDED Requirements

### Requirement: Tree comparison
Where serialization compares a reparsed document with the one it wrote, it
SHALL compare them apart from spans, reading each `Escape` as a `Text` holding
its char and each `CharacterReference` as a `Text` holding its value, and
merging adjacent `Text` nodes; the reparse SHALL read the written document as
if it also held a definition of each reference label the document uses
without defining.

#### Scenario: Escape the serializer adds
- **WHEN** a hand-built paragraph holding `Text("*a*")` is serialized and reparsed
- **THEN** `to_markdown()` returns `"\\*a\\*\n"`, the reparsed paragraph holds `Escape('*')`, `Text("a")`, and `Escape('*')`, which compare equal to the original, and serializing the reparsed document returns the same text

#### Scenario: Reference without its definition
- **WHEN** a hand-built paragraph holding a shortcut `LinkReference` to `foo`, in a document holding no `Definition`, is serialized
- **THEN** `to_markdown()` returns `"[foo]\n"`

#### Scenario: Split text
- **WHEN** a hand-built paragraph holding `Text("a")` followed by `Text("b")` is serialized and reparsed
- **THEN** `to_markdown()` returns `"ab\n"` and the reparsed `Text("ab")` compares equal to the original

### Requirement: Syntax rules come from the parser
The serializer SHALL decide which text chars to escape, which delimiter each
`Emphasis` and `Strong` takes, and when a block needs a layout other than the
default one, by parsing its own rendering under `SerializeOptions::syntax` and
reading what that parse took as syntax and which container or block each
written line landed in. A text char
SHALL be written raw unless that parse reads it, written raw, as part of a
construct, a delimiter run counting whole; the exceptions are a backtick,
which is always escaped, and the encodings that "Escape forms" requires for a
node or block that does not read back. Emphasis and strong SHALL be written
with `*` and `**` unless that does not read back.

#### Scenario: Unpaired delimiters stay raw
- **WHEN** a hand-built paragraph holding `Text("x_y_ a*b x^2 ~5")` is serialized
- **THEN** `to_markdown()` returns `"x_y_ a*b x^2 ~5\n"`

#### Scenario: Brackets with and without a definition
- **WHEN** a hand-built paragraph holding `Text("[x]")` is serialized alone, and again in a document that also holds a `Definition` of `x`
- **THEN** the first output is `"[x]\n"` and the second writes the paragraph as `\[x\]`

#### Scenario: Mark delimiters escaped whole
- **WHEN** a hand-built paragraph holding `Text("==a==")` is serialized
- **THEN** `to_markdown()` returns `"\\=\\=a\\=\\=\n"`

#### Scenario: Line-start check in mid-line
- **WHEN** the documents parsed from `` "`x`<div" `` and `"a *b*::c"` with `parse` are serialized
- **THEN** `to_markdown()` returns `` "`x`<div\n" `` and `"a *b*::c\n"`

#### Scenario: Colon before a span's closing delimiter
- **WHEN** the document parsed from `"++a:++ b:"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"++a:++ b:\n"`

#### Scenario: Nested list before an indented block
- **WHEN** the document parsed from `"- a\n  - b\n   <div>"` is serialized
- **THEN** the nested item is written as `  - b`, and reparsing the output yields the same tree

### Requirement: Escape forms
An escaped ASCII punctuation char SHALL be written with a backslash, or as a
character reference when the parse still reads its backslash form as syntax;
any other escaped char SHALL be written as a character reference. When a node
does not read back, the text chars touching its delimiters, inside and
outside, SHALL be written as character references. When a block still does
not read back after three rounds of escaping, every ASCII punctuation char of
its text SHALL be escaped, and when it then still does not read back,
serialization SHALL return `SerializeError::Unrepresentable`.

#### Scenario: Space at an emphasis edge
- **WHEN** a hand-built paragraph holding an `Emphasis` around `Text("a ")` is serialized and reparsed
- **THEN** `to_markdown()` returns `"*&#97;&#x20;*\n"` and the reparsed paragraph compares equal to the original

#### Scenario: Letter before a shortcode
- **WHEN** a hand-built paragraph holding `Text("a")` followed by a `Shortcode` named `smile` is serialized and reparsed
- **THEN** `to_markdown()` returns `"&#97;:smile:\n"` and the reparsed paragraph compares equal to the original

### Requirement: Links written as autolinks
A `Link` with no title whose one child is a `Text` equal to its destination,
or for a `mailto:` destination equal to the address after it, SHALL be written
as an angle-bracket autolink when that autolink reads back as the same `Link`,
and as `[text](destination)` otherwise.

#### Scenario: Literal URL
- **WHEN** `parse("see http://a.b").document.to_markdown()` runs
- **THEN** it returns `"see <http://a.b>\n"`

#### Scenario: `www` link
- **WHEN** `parse("www.a.b").document.to_markdown()` runs
- **THEN** it returns `"[www.a.b](http://www.a.b)\n"`

#### Scenario: Inline link whose text is its URL
- **WHEN** `parse("[http://a.b](http://a.b)").document.to_markdown()` runs
- **THEN** it returns `"<http://a.b>\n"`

#### Scenario: Literal email
- **WHEN** `parse("a@b.c").document.to_markdown()` runs
- **THEN** it returns `"<a@b.c>\n"`

#### Scenario: Scheme too short for an angle-bracket autolink
- **WHEN** the document parsed from `"a://x"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"[a://x](a://x)\n"`

## MODIFIED Requirements

### Requirement: Canonical output
`Document::to_markdown` SHALL emit canonical Markdown: for each construct, the
spelling the AST records for it, such as a list marker, a fence's char and
length, a heading's style, a reference's kind, an escaped char, a character
reference as written, or a wiki link's embed mark, or else one fixed spelling,
independent of source details the AST does not record.

#### Scenario: Paragraph and heading
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `"# Title\n\nHello *world*.\n"`

#### Scenario: Marker the AST records
- **WHEN** `parse("+ a").document.to_markdown()` runs
- **THEN** it returns `"+ a\n"`

#### Scenario: Escape the author wrote
- **WHEN** `parse("a\\.b \\#tag").document.to_markdown()` runs
- **THEN** it returns `"a\\.b \\#tag\n"`

#### Scenario: Character reference the author wrote
- **WHEN** `parse("&#35;tag &amp; x").document.to_markdown()` runs
- **THEN** it returns `"&#35;tag &amp; x\n"`

#### Scenario: Wiki embed
- **WHEN** `parse("see ![[x.png]]").document.to_markdown()` runs
- **THEN** it returns `"see ![[x.png]]\n"`

### Requirement: Round-trip stability
For a document parsed with a `SyntaxOptions`, parsing the Markdown that
`to_markdown_with` writes with `syntax` set to those options SHALL yield, under
the same options, a document equal to the first as "Tree comparison" defines,
and serializing that reparsed document the same way SHALL yield the same text
byte for byte.

#### Scenario: Round-trip fixtures
- **WHEN** each fixture under `tests/fixtures/roundtrip/` is parsed with its profile's options, serialized with `syntax` set to them, reparsed with them, and serialized again
- **THEN** the reparsed AST matches the first and the two serialized texts are identical

#### Scenario: Seeded round-trip generators
- **WHEN** the inline, block-oriented, and emphasis-heavy generators in `tests/serialize_roundtrip_fuzz.rs` run with the seeds recorded in that file, in each dialect
- **THEN** every generated document round-trips, or serialization returns `SerializeError::Unrepresentable`

### Requirement: Serialize options
`SerializeOptions` SHALL control the line ending, the trailing newline, and the
syntax options the output is read back under (`syntax`, the maximal default
dialect by default); a bullet marker, ordered-list delimiter, or code fence
character other than its default SHALL replace the one the AST records, while
the default keeps it. Options SHALL be constructed by mutating
`SerializeOptions::default()`.

#### Scenario: CRLF without final newline
- **WHEN** `parse("# Title").document.to_markdown_with(&options)` runs with `line_ending = LineEnding::CrLf` and `final_newline = false`
- **THEN** it returns `"# Title"`

#### Scenario: Bullet override
- **WHEN** `parse("- a\n\n+ b").document.to_markdown_with(&options)` runs with `bullet = ListDelimiter::Plus`
- **THEN** both lists are written with `+`

#### Scenario: Escapes follow the read-back dialect
- **WHEN** a hand-built paragraph holding `Text("==a==")` is serialized once with default options and once with `syntax = SyntaxOptions::commonmark()`
- **THEN** the first output is `"\\=\\=a\\=\\=\n"` and the second is `"==a==\n"`

### Requirement: Invalid documents are rejected
Serialization SHALL validate the document first and return
`SerializeError::InvalidDocument` with the validation diagnostics when it is
invalid, and `SerializeError::UnsupportedNode` for a node kind it cannot write.
When no Markdown it can write reads back, under `SerializeOptions::syntax`, as
the same tree, compared as "Tree comparison" defines, it SHALL return
`SerializeError::Unrepresentable` with a diagnostic naming the first node that
reads back differently.

#### Scenario: Empty table
- **WHEN** a hand-built document holding a `Table` with no rows is serialized
- **THEN** `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`

#### Scenario: Link inside a link
- **WHEN** a hand-built paragraph holding a `Link` whose children hold another `Link` is serialized
- **THEN** `to_markdown()` returns `Err(SerializeError::Unrepresentable(_))`

### Requirement: Escaping keeps text literal
The serializer SHALL escape text so that reparsing the output yields the same
text and leaves the nodes beside it unchanged, rather than forming new
constructs.

#### Scenario: Literal asterisks in text
- **WHEN** a hand-built paragraph holding `Text("*not emphasis*")` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same text and no `Emphasis`

#### Scenario: Text delimiter after a closing run
- **WHEN** a hand-built paragraph holding an `Emphasis` around a `Strong` around `Text("(a b)_.")`, followed by `Text("*#")`, is serialized with `syntax = SyntaxOptions::commonmark()` and reparsed with the CommonMark preset
- **THEN** `to_markdown_with()` returns `"***(a b)_.***\\*#\n"` and the reparsed paragraph compares equal to the original

#### Scenario: Parenthesis after a shortcut reference
- **WHEN** a hand-built paragraph holding a shortcut `LinkReference` to `foo` followed by `Text("(a)")` is serialized and reparsed with a definition of `foo`
- **THEN** the reparsed paragraph holds the shortcut `LinkReference` followed by the text `(a)`

#### Scenario: Parenthesis after a shortcut image reference
- **WHEN** a hand-built paragraph holding a shortcut `ImageReference` to `foo` followed by `Text("(a)")` is serialized and reparsed with a definition of `foo`
- **THEN** the reparsed paragraph holds the shortcut `ImageReference` followed by the text `(a)`

#### Scenario: Colon after a shortcut reference that starts a paragraph
- **WHEN** a hand-built paragraph holding a shortcut `LinkReference` to `foo` followed by `Text(": /x")` is serialized and reparsed with a definition of `foo`
- **THEN** the reparsed document still holds the paragraph, with the shortcut `LinkReference` followed by the text `: /x`

#### Scenario: Pipe ending a level-two setext heading
- **WHEN** the document parsed from `"a |\n-"` is serialized and reparsed
- **THEN** `to_markdown()` returns `"a |\n---\n"` and the reparsed document holds the same setext `Heading` and no `Table`

#### Scenario: Empty fenced code block
- **WHEN** ``parse("```\n```").document.to_markdown()`` runs
- **THEN** it returns ``"```\n```\n"``

#### Scenario: Whitespace at the ends of an info string
- **WHEN** the document parsed from ``"```&#x20;a&#9;\nb\n```"`` is serialized
- **THEN** `to_markdown()` returns ``"``` &#x20;a&#x9;\nb\n```\n"`` and reparsing it yields the info string `" a\t"`

#### Scenario: Text right after a literal autolink
- **WHEN** the documents parsed from `"://&amp;"` and `"www.}"` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `Link` and the same inlines after it

#### Scenario: Paragraph that opens with a soft break
- **WHEN** the document parsed from `"&#x20;\na"` is serialized
- **THEN** `to_markdown()` returns `"&#x20;\na\n"`

#### Scenario: Text line that would open a block
- **WHEN** the documents parsed from `"a\n\\<div>"` and `"a\n\\::b"` are serialized and reparsed
- **THEN** each reparsed document holds the same single `Paragraph`

#### Scenario: HTML block value
- **WHEN** the document parsed from `"<!--\n\n"` is serialized and reparsed
- **THEN** the reparsed `HtmlBlock` value is `"<!--\n"`

#### Scenario: Indented code ending in a carriage return
- **WHEN** the document parsed from `"\ta\r\tb"` is serialized
- **THEN** `to_markdown()` returns `"    a\r    b\r"` and reparsing it yields the value `"a\rb\r"`

#### Scenario: CRLF output keeps a value's CRLF
- **WHEN** the document parsed from ``"```\r\na\r\n```\r\nb"`` is serialized with `LineEnding::CrLf`
- **THEN** the output is ``"```\r\na\r\n```\r\n\r\nb\r\n"``

#### Scenario: Break that opens a line or a delimited span
- **WHEN** the documents parsed from `"&#x20; \na"`, `"a\n&#x20;\nb"`, and `"_&#x20;\n=_"` are serialized and reparsed
- **THEN** each reparsed paragraph equals the parsed one, and the first two outputs are `"&#x20;\na\n"` and `"a\n&#x20;\nb\n"`

#### Scenario: Continuation line inside a code span that would start a block
- **WHEN** the document parsed from ``"=```\n    ```"`` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same code span

#### Scenario: Delimiter run partly escapable
- **WHEN** the documents parsed from `"**\t*$"`, `"(*~\n**)"`, and `"($$]$="` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same single text, since a run of `*`, `_`, or `$` is escaped whole

#### Scenario: Underscore emphasis beside an alphanumeric
- **WHEN** the document parsed from `"y***b***"` is serialized
- **THEN** `to_markdown()` returns `"y***b***\n"`

#### Scenario: Abutting attention runs
- **WHEN** the documents parsed from `"__**)**&__"`, `"**:__$__**"`, `"****(*+***"`, `"***_|_***"`, `"__***/***__"`, `"**#****]***_**"`, `"***_\\**#*"`, `"***b_*_b_*"`, `` "__<__y_`__" ``, and `"_# _*#***___"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same nested `Strong` and `Emphasis` runs

#### Scenario: Abutting attention runs under CommonMark
- **WHEN** the documents parsed from `"_^*^*_c__"` and `"_# _*#***___"` with the CommonMark preset are serialized with `syntax` set to it and reparsed with it
- **THEN** each reparsed paragraph equals the parsed one

#### Scenario: Text beside a literal autolink
- **WHEN** the documents parsed from `` "a\\-://`" ``, `"ab&#99;://x"`, `"*://*&mp;"`, `"**://**&mp;"`, `"://^&mp;"`, `"://~&mp;~"`, `"://__&mp;__"`, `"://~&mp;&p;~"`, and `"www.\\[]_("` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `Link` with the same inlines around it

#### Scenario: Character references beside a literal autolink under GFM
- **WHEN** the document parsed from `"://&#x0;&mp;"` with the GFM preset is serialized with `syntax` set to it and reparsed with it
- **THEN** the reparsed paragraph equals the parsed one

#### Scenario: Emphasis delimiters beside a literal autolink
- **WHEN** the documents parsed from `"**a *b*www.x.com**"`, `"**a *b*x@y.com**"`, `"**x@y.com***x@y.com*"`, and `"**\\*www.x.com**"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph equals the parsed one

#### Scenario: Backtick after a reference's raw label
- **WHEN** the document parsed from ``` "[^`]``" ``` with `parse` is serialized
- **THEN** `to_markdown()` returns `` "[^`]&#96;&#96;\n" ``, since an escaped backtick would close a code span that the label's backtick opens

#### Scenario: Bang before a wiki link
- **WHEN** a hand-built paragraph holding `Text("a!")` followed by a `WikiLink` to `x` that is not an embed is serialized and reparsed
- **THEN** `to_markdown()` returns `"a\\![[x]]\n"` and the reparsed paragraph holds the text `a!` and the same `WikiLink`

#### Scenario: Escaped backslash before a bang
- **WHEN** the document parsed from `"\\\\&#33;[a](b)"` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Escape`, `CharacterReference`, and `Link`, and no `Image`

#### Scenario: Tilde beside an attention run
- **WHEN** the documents parsed from `"b**~\n~**"`, `"a*~ **&*"`, and `"t_~>___~"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines

#### Scenario: Delimiters a following construct writes
- **WHEN** the documents parsed from `":\\^:^[|]"`, `"*\\$#$>$"`, `"~\\$#://$"`, and `"b\\-p://"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines

#### Scenario: Run of bars before a spoiler
- **WHEN** the document parsed from `")||||||\t||"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Text` and `Spoiler`

#### Scenario: Whitespace control chars in text
- **WHEN** the documents parsed from `"\u{c}:a"`, `"[^\u{c}]"`, `"[;\u{c}]:["`, and `"://y\u{c}c"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines, and a line tabulation, form feed, or next-line char in text is written as itself

#### Scenario: Hard break opening a span
- **WHEN** the document parsed from `"==&#x20; \n-=="` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Mark`

#### Scenario: Space between a literal autolink and a span delimiter
- **WHEN** the documents parsed from `"^://y ^"`, `"_&#x20;://_"`, and `"*&#x20;http://x*"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same span and `Link`

#### Scenario: Text between a literal autolink and a span
- **WHEN** the document parsed from `"://\\~||>||"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Link`, `Escape('~')`, and `Spoiler`

#### Scenario: Text after a bare text directive
- **WHEN** the documents parsed from `":e{}1"`, `":e[]www.+"`, and `":e{}[^1]"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `TextDirective` and what follows it

#### Scenario: Email-local char before an email
- **WHEN** the documents parsed from `"]\\-a@b.c"`, `"++@b.c"`, and `"\\+@b.p://"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines and links

#### Scenario: Alert title and empty container directive
- **WHEN** the documents parsed from `">[!NOTE]+\t*"` and `"]\n: :::e"` with `parse` are serialized and reparsed
- **THEN** each reparsed document holds the same blocks, since an alert title is written as its source and an empty container directive takes no blank line

#### Scenario: Paragraph opening with an ESM keyword
- **WHEN** the document parsed from `" import -"` with the MDX preset is serialized with `syntax` set to it and reparsed with it
- **THEN** the reparsed document holds the same `Paragraph`

#### Scenario: Content that reads back only under its preset
- **WHEN** the documents parsed from `"**=* ++@b.c*"` with the GFM preset and `"\\{[]()}"` with the MDX preset are serialized with `syntax` set to the same preset and reparsed with it
- **THEN** each reparsed paragraph holds the same inlines

#### Scenario: Heading content
- **WHEN** the document parsed from `"# _*www._"` with `parse` is serialized and reparsed
- **THEN** the reparsed heading holds the same `Emphasis` and `Link`

#### Scenario: Flow-like first line under MDX
- **WHEN** the documents parsed from `"{}&#x20;\n\\"`, `"{}&#x20; \n\\"`, and `"<!--@b>"` with the MDX preset are serialized with `syntax` set to it and reparsed with it
- **THEN** each reparsed document holds the same `Paragraph`

#### Scenario: Task item text opening with whitespace
- **WHEN** the document parsed from `"+ [x]  :e"` with `parse` is serialized and reparsed
- **THEN** the reparsed item holds the same `Text(" ")` and `TextDirective`

#### Scenario: Literal autolink before a shortcode
- **WHEN** the document parsed from `"://\\::+1:"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Link`, `Escape(':')`, and `Shortcode`

#### Scenario: Space in a bare destination
- **WHEN** the document parsed from `"[o]:&#x20;"` is serialized
- **THEN** `to_markdown()` returns `"[o]: &#x20;\n"` and the reparsed definition keeps a `Bare` destination `" "`

#### Scenario: Whitespace that opens a list item's first block
- **WHEN** the document parsed from `"-\n   <v>"` is serialized
- **THEN** `to_markdown()` returns `"-\n   <v>\n"` and the reparsed `HtmlBlock` value is `" <v>"`

#### Scenario: Fence length
- **WHEN** the document parsed from ``"```\n```*"`` is serialized
- **THEN** `to_markdown()` returns ``"```\n```*\n```\n"``, keeping the fence length 3

#### Scenario: Text that would open an extension construct
- **WHEN** the documents parsed with `parse` from `":b["`, `"\\:p"`, `":\\+:"`, `"\\:p://"`, and `` "`\\$[<a>[$>" `` are serialized and reparsed
- **THEN** each reparsed paragraph equals the parsed one

#### Scenario: Literal tilde beside an emphasis run
- **WHEN** the document parsed from `"a**~**"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"a**~**\n"`

#### Scenario: Pipe written by an inline in a table cell
- **WHEN** the document parsed from `"$\\|$||\n-|-"` with `parse` is serialized and reparsed
- **THEN** the reparsed cell holds the same dollar `Math` with value `"|"`

#### Scenario: List before an indented HTML block
- **WHEN** the document parsed from `"-\t(\n  <v>"` is serialized and reparsed
- **THEN** the reparsed document holds the `List` followed by the `HtmlBlock` `"  <v>"`

#### Scenario: Thematic break opening a list item
- **WHEN** the document parsed from `"-\n  ---"` is serialized
- **THEN** `to_markdown()` returns `"-\n  ---\n"`

#### Scenario: Code fence a content line would close
- **WHEN** the document parsed from `" ~~~\n    ~~~"` is serialized
- **THEN** `to_markdown()` returns `" ~~~\n    ~~~\n ~~~\n"`, keeping the fence length 3

#### Scenario: Raw HTML after a definition
- **WHEN** the document parsed from `"[o]:u\n\t<div>"` is serialized and reparsed
- **THEN** the reparsed document holds the `Definition` and a `Paragraph` holding the `Html` inline

#### Scenario: Line that would open description details
- **WHEN** the document parsed from `` "a\n   : `" `` with `parse` is serialized and reparsed
- **THEN** the reparsed document holds the same single `Paragraph`

#### Scenario: Run delimiters inside a link or mark
- **WHEN** the documents parsed from `"[__**)**&__](u)"` and `"==__***/***__=="` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph equals the parsed one

#### Scenario: Strong after an emphasis with underline enabled
- **WHEN** the document parsed from `"*a***b**"` with underline enabled is serialized with `syntax` set to the same options
- **THEN** `to_markdown()` returns `"*a***b**\n"`, which reparses with a `Strong` and no `Underline`

#### Scenario: Doubled delimiter that could close its span
- **WHEN** the document parsed from `"==a\\== b=="` with `parse` is serialized
- **THEN** `to_markdown()` returns `"==a\\== b==\n"`

#### Scenario: Math opening a definition's paragraph
- **WHEN** the document parsed from `"[o]:u\n\t$$\na$$"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"[o]: u\n    $$\na$$\n"`, which keeps the math inline in the paragraph the definition was read from

#### Scenario: Cell pipe after an escaped backslash
- **WHEN** the document parsed from `"| <a b=\"x\\\\\\|y\"> |\n| --- |"` with the GFM preset is serialized with `syntax` set to it and reparsed with it
- **THEN** the reparsed table holds the same raw HTML in one cell

#### Scenario: Raw label backtick before a span
- **WHEN** the document parsed from `` "*[foo`bar]* &#96;\n\n[foo`bar]: /u" `` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph equals the parsed one

#### Scenario: Paragraph after an empty quote line in an item
- **WHEN** the documents parsed from `"- > a\n  >\n  b\n  ---"` and `"- > a\n  >\n  | b |\n  | - |"` with `parse` are serialized and reparsed
- **THEN** each reparsed document equals the parsed one, the second still holding its `Table`

#### Scenario: What borders spans, cells, items, and quotes
- **WHEN** the documents parsed with `parse` from `` "-[^\\`]://\\`" ``, `"++:++\\:"`, `"&#x20;://>|>\n-|-"`, `"- *  (\n    <a>"`, `"~~:~ :e~"`, `"++\\+>++"`, and `">\n>[!NOTE]:>"` are serialized and reparsed
- **THEN** each reparsed document equals the parsed one
