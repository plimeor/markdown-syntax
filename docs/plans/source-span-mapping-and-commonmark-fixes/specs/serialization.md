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

#### Scenario: Parenthesis after a shortcut reference
- **WHEN** the document parsed from `"[foo]\\(a)\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed paragraph holds a shortcut `LinkReference` to `foo` followed by `Text("(a)")`

#### Scenario: Parenthesis after a shortcut image reference
- **WHEN** the document parsed from `"![foo]\\(a)\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed paragraph holds a shortcut `ImageReference` to `foo` followed by `Text("(a)")`

#### Scenario: Colon after a shortcut reference that starts a paragraph
- **WHEN** the document parsed from `"[foo]\\: /x\n\n[foo]: /u"` is serialized and reparsed
- **THEN** the reparsed document still holds the paragraph, with a shortcut `LinkReference` to `foo` followed by `Text(": /x")`
