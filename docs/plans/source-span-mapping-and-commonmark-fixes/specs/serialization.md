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
