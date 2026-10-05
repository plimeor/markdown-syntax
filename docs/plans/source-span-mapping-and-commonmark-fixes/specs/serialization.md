# Serialization — spec changes

## ADDED Requirements

### Requirement: Literal backticks are always escaped
The serializer SHALL write every backtick in a text value as `` \` ``.

#### Scenario: Backtick run before a lone backtick
- **WHEN** a hand-built paragraph holding ```Text("b ``a`")``` is serialized
- **THEN** `to_markdown()` returns ``"b \`\`a\`\n"`` and reparsing it yields the same text and no code span

#### Scenario: Paired backticks
- **WHEN** ``parse("Test \\`hello world` here.").document.to_markdown()`` runs
- **THEN** it returns ``"Test \`hello world\` here.\n"``

## MODIFIED Requirements

### Requirement: Escaping keeps text literal
The serializer SHALL escape text so that reparsing the output yields the same
text, and leaves the nodes beside it as they were, rather than new constructs.

#### Scenario: Literal asterisks in text
- **WHEN** a hand-built paragraph holding `Text("*not emphasis*")` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same text and no `Emphasis`

#### Scenario: Underscore that can close inside underscore emphasis
- **WHEN** a hand-built paragraph holding an `Emphasis` around a `Strong` around `Text("(a b)_.")`, followed by `Text("*#")`, is serialized and reparsed with the CommonMark preset
- **THEN** `to_markdown()` returns `"_**(a b)\\_.**_\\*#\n"` and the reparsed paragraph equals the original apart from spans

#### Scenario: Parenthesis after a shortcut reference
- **WHEN** the document parsed from `"[foo]\\(a)\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed paragraph holds a shortcut `LinkReference` to `foo` followed by `Text("(a)")`

#### Scenario: Parenthesis after a shortcut image reference
- **WHEN** the document parsed from `"![foo]\\(a)\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed paragraph holds a shortcut `ImageReference` to `foo` followed by `Text("(a)")`

#### Scenario: Colon after a shortcut reference that starts a paragraph
- **WHEN** the document parsed from `"[foo]\\: /x\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed document still holds the paragraph, with a shortcut `LinkReference` to `foo` followed by `Text(": /x")`

#### Scenario: Pipe ending a level-two setext heading
- **WHEN** the document parsed from `"a |\n-"` is serialized and reparsed
- **THEN** `to_markdown()` returns `"a \\|\n---\n"` and the reparsed document holds the same setext `Heading` and no `Table`

#### Scenario: Empty fenced code block
- **WHEN** ``parse("```\n```").document.to_markdown()`` runs
- **THEN** it returns ``"```\n```\n"``

#### Scenario: Whitespace at the ends of an info string
- **WHEN** the document parsed from ``"```&#x20;a&#9;\nb\n```"`` is serialized
- **THEN** `to_markdown()` returns ``"``` &#x20;a&#x9;\nb\n```\n"`` and reparsing it yields the info string `" a\t"`

#### Scenario: Text right after a literal autolink
- **WHEN** the documents parsed from `"://&amp;"` and `"www.}"` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `Autolink` and `Text` as the parsed one

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
- **THEN** each reparsed paragraph equals the parsed one, and the first two outputs are `"&#x20; \na\n"` and `"a\n&#x20;\nb\n"`

#### Scenario: Continuation line inside a code span that would start a block
- **WHEN** the document parsed from ``"=```\n    ```"`` is serialized
- **THEN** `to_markdown()` returns ``"\\=```\n    ```\n"`` and reparsing it yields the same code span

#### Scenario: Delimiter run partly escapable
- **WHEN** the documents parsed from `"**\t*$"`, `"(*~\n**)"`, and `"($$]$="` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same single text, since a run of `*`, `_`, or `$` is escaped whole

#### Scenario: Underscore emphasis beside an alphanumeric
- **WHEN** the document parsed from `"y***b***"` is serialized
- **THEN** `to_markdown()` returns `"y***b***\n"`

#### Scenario: Abutting attention runs
- **WHEN** the documents parsed from `"__**)**&__"`, `"**:__$__**"`, `"****(*+***"`, `"***_|_***"`, `"__***/***__"`, `"**#****]***_**"`, and `"***_\\**#*"` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same nested `Strong` and `Emphasis` runs, since a paragraph whose runs abut, or touch a text `*` or `~`, is written with the first delimiter choice that reads back, which may leave that text `*` raw to join a run

#### Scenario: Text beside a literal autolink
- **WHEN** the documents parsed from `"a\\-://`"`, `"ab&#99;://x"`, `"*://*&mp;"`, `"**://**&mp;"`, and `"://^&mp;"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `Autolink` with the same text around it, since a scheme char before a `://` autolink and the first char of text after one, past any span delimiters, are written in a form the URL scan stops at

#### Scenario: Backtick after a reference's raw label
- **WHEN** the document parsed from `"[^`]``"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"[^`]&#96;&#96;\n"`, since an escaped backtick would close a code span that the label's backtick opens

#### Scenario: Bang before a wiki link
- **WHEN** the document parsed from `"![[$[]]a$>"` with `parse` is serialized
- **THEN** `to_markdown()` returns `"\\![[$\\[]]a$>\n"`, so the `!` cannot make the wiki link's `[` an image opener

#### Scenario: Tilde beside an attention run
- **WHEN** the documents parsed from `"b**~\n~**"` and `"a*~ **&*"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines, since a text `*` or `_` touching a `~` is escaped as one the strikethrough bonus lets open or close, and a `~` run at a strong's or emphasis's edge is written raw when only the raw `~` grants the run that bonus

#### Scenario: Delimiters a following construct writes
- **WHEN** the documents parsed from `":\\^:^[|]"`, `"*\\$#$>$"`, `"~\\$#://$"`, and `"b\\-p://"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines, since a text escapes a `^` that a footnote's `^` could close, a `$` that a math fence or a literal autolink's `$` could close, and a scheme char that would join a literal autolink's scheme

#### Scenario: Run of bars before a spoiler
- **WHEN** the document parsed from `")||||||\t||"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Text` and `Spoiler`, since every bar but the last of a text run that could open a spoiler is written as a reference

#### Scenario: Whitespace control chars in text
- **WHEN** the documents parsed from `"\u{c}:a"`, `"[^\u{c}]"`, `"[;\u{c}]:["`, and `"://y\u{c}c"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same inlines, since a line tabulation, form feed, or next-line char is written as itself, which reads as whitespace beside a construct as the source did, and a reference or footnote label is written as its source

#### Scenario: Hard break opening a span
- **WHEN** the document parsed from `"==&#x20; \n-=="` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Mark`, since a hard break that opens a span is written `&#x20; ` before its line ending

#### Scenario: Space between a literal autolink and a span delimiter
- **WHEN** the documents parsed from `"^://y ^"`, `"_&#x20;://_"`, and `"*&#x20;http://x*"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same span and `Autolink`, since a paragraph that does not read back also tries writing the spaces at its line edges raw, and writing a space before a literal autolink as a reference

#### Scenario: Text between a literal autolink and a span
- **WHEN** the document parsed from `"://\\~||>||"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Autolink`, `Text("~")`, and `Spoiler`, since a paragraph that does not read back also tries escaping the first char of a text after a literal autolink

#### Scenario: Text after a bare text directive
- **WHEN** the documents parsed from `":e{}1"`, `":e[]www.+"`, and `":e{}[^1]"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same `TextDirective` and what follows it, since a directive with no attributes followed by anything that could go on with its name, label, or attributes ends with an empty label, or with an empty attribute list before a `{`

#### Scenario: Email-local char before an email
- **WHEN** the documents parsed from `"]\\-a@b.c"`, `"++@b.c"`, and `"\\+@b.p://"` with `parse` are serialized and reparsed
- **THEN** each reparsed paragraph holds the same text and autolinks, since an email-local char before an email or an `@` takes a backslash or a reference, and a `+` run is escaped whole

#### Scenario: Alert title and empty container directive
- **WHEN** the documents parsed from `">[!NOTE]+\t*"` and `"]\n: :::e"` with `parse` are serialized and reparsed
- **THEN** each reparsed document holds the same blocks, since an alert title is written as its source and an empty container directive takes no blank line

#### Scenario: Paragraph opening with an ESM keyword
- **WHEN** the document parsed from `" import -"` with the MDX preset is serialized and reparsed with it
- **THEN** the reparsed document holds the same `Paragraph`, since a paragraph's leading `import ` or `export ` is written with its first char as a reference

#### Scenario: Literal autolink before a shortcode
- **WHEN** the document parsed from `"://\\::p:"` with `parse` is serialized and reparsed
- **THEN** the reparsed paragraph holds the same `Autolink`, `Text(":")`, and `Shortcode`

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
- **WHEN** the documents parsed with `parse` from `":b["`, `"\\:p"`, `":\\+:"`, `"\\:p://"`, and `"`\\$[<a>[$>"` are serialized and reparsed
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
- **WHEN** the document parsed from `"a\n   : `"` with `parse` is serialized and reparsed
- **THEN** the reparsed document holds the same single `Paragraph`
