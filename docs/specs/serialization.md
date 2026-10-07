# Serialization

## Purpose

Turning a `Document`, parsed or hand-built, into canonical Markdown text. Owned
by `src/serialize.rs`.

## Requirements

### Requirement: Canonical output
`Document::to_markdown` SHALL emit canonical Markdown: for each construct, the
spelling the AST records for it, such as a list marker, a fence's char and
length, a heading's style, a reference's kind, an emphasis or strong
delimiter, an autolink's form, an escaped char, a character reference as written,
a wiki link's target and label as written, or a wiki link's embed mark, or else one fixed spelling, independent of source
details the AST does not record.

#### Scenario: Paragraph and heading
- **WHEN** `parse("# Title\n\nHello *world*.").document.to_markdown()` runs
- **THEN** it returns `"# Title\n\nHello *world*.\n"`

#### Scenario: Marker the AST records
- **WHEN** `parse("+ a").document.to_markdown()` runs
- **THEN** it returns `"+ a\n"`

#### Scenario: Delimiters the AST records
- **WHEN** `parse("_a_ __b__").document.to_markdown()` runs
- **THEN** it returns `"_a_ __b__\n"`

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
For a parsed document, parsing the Markdown that `to_markdown` writes SHALL
yield a document equal to the first as "Tree comparison" defines, and
serializing that reparsed document SHALL yield the same text byte for byte.
This SHALL hold for every fixture under `tests/fixtures/roundtrip/` and for
every document the seeded generators build from their recorded seeds, apart
from the generated documents that `tests/serialize_roundtrip_fuzz.rs` lists
one by one, each with the reason it does not read back.

#### Scenario: Round-trip fixtures
- **WHEN** each fixture under `tests/fixtures/roundtrip/` is parsed, serialized, reparsed, and serialized again
- **THEN** the reparsed AST matches the first and the two serialized texts are identical

#### Scenario: Seeded round-trip generators
- **WHEN** the inline, block-oriented, and emphasis-heavy generators in `tests/serialize_roundtrip_fuzz.rs` run with the seeds recorded in that file
- **THEN** every generated document round-trips, except those the file lists with a reason, which serialize without panicking

### Requirement: Serialize options
`SerializeOptions` SHALL control the line ending and the trailing newline. The
`bullet`, `ordered_delimiter`, and `fence_marker` options SHALL keep the
marker each node records when `None`, the default, and SHALL write every
unordered list, ordered list, or fenced code block with the marker they hold
when `Some`, whichever marker that is; a fenced block whose info string holds a
backtick SHALL take tildes regardless. A replaced list marker SHALL yield where
the list before it in the same container is written with the same marker: that
list takes the next marker in the order `-`, `*`, `+`, or `.`, `)` for an
ordered list. Options SHALL be constructed by mutating
`SerializeOptions::default()`.

#### Scenario: CRLF without final newline
- **WHEN** `parse("# Title").document.to_markdown_with(&options)` runs with `line_ending = LineEnding::CrLf` and `final_newline = false`
- **THEN** it returns `"# Title"`

#### Scenario: Bullet override
- **WHEN** `parse("- a\n\n+ b\n\n* c").document.to_markdown_with(&options)` runs with `bullet = Some(BulletMarker::Plus)`
- **THEN** it returns `"+ a\n\n- b\n\n+ c\n"`

#### Scenario: Normalizing to the dash
- **WHEN** `parse("* a").document.to_markdown_with(&options)` runs with `bullet = Some(BulletMarker::Dash)`
- **THEN** it returns `"- a\n"`

#### Scenario: Tilde fences
- **WHEN** `parse("```\na\n```").document.to_markdown_with(&options)` runs with `fence_marker = Some(FenceMarker::Tilde)`
- **THEN** it returns `"~~~\na\n~~~\n"`

#### Scenario: Recorded markers by default
- **WHEN** `parse("* a\n\n~~~\nb\n~~~").document.to_markdown()` runs
- **THEN** it returns `"* a\n\n~~~\nb\n~~~\n"`

### Requirement: Invalid documents are rejected
Serialization SHALL validate the document first and return
`SerializeError::InvalidDocument` with the validation diagnostics when it is
invalid. Validation is the only check that refuses a document: every valid
document is written.

#### Scenario: Empty table
- **WHEN** a hand-built document holding a `Table` with no rows is serialized
- **THEN** `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`

#### Scenario: Link inside a link
- **WHEN** a hand-built paragraph holding a `Link` whose children hold another `Link` is serialized
- **THEN** `to_markdown()` returns `Err(SerializeError::InvalidDocument(_))`

### Requirement: No HTML filtering or style preservation
The serializer SHALL write raw HTML node values as they are, without safety
filtering, and SHALL NOT recover source spelling that the AST does not
record.

#### Scenario: Raw HTML passes through
- **WHEN** `parse("<script>alert(1)</script>").document.to_markdown()` runs
- **THEN** the output contains `<script>alert(1)</script>`

### Requirement: Tree comparison
Round-trip stability SHALL compare a reparsed document with the parsed one
apart from spans, reading each `Escape` as a `Text` holding its char, each
`CharacterReference` as a `Text` holding its value, and each `SoftBreak`
inside a heading as a `Text` holding a space, and merging adjacent `Text`
nodes.

#### Scenario: Escape against text
- **WHEN** a paragraph holding `Escape('*')` and `Text("a")` is compared with one holding `Text("*a")`
- **THEN** the two compare equal

#### Scenario: Split text
- **WHEN** a hand-built paragraph holding `Text("a")` followed by `Text("b")` is serialized and reparsed
- **THEN** `to_markdown()` returns `"ab\n"` and the reparsed `Text("ab")` compares equal to the original

#### Scenario: Soft break in a heading
- **WHEN** the document parsed from `"a\nb\n==="` is serialized and reparsed
- **THEN** the reparsed heading holding `Text("a b")` compares equal to the parsed one

### Requirement: Rendering only
`Document::to_markdown` SHALL produce its output from the document and the
options alone, by fixed rules, without parsing the output; when the output
would read back as a different tree, it SHALL still return it.

#### Scenario: Text that reads as syntax
- **WHEN** a hand-built paragraph holding `Text("==a==")` is serialized
- **THEN** `to_markdown()` returns `"==a==\n"`, which reads back as a `Mark`

#### Scenario: Recorded delimiters reproduce the source
- **WHEN** the document parsed from `"__#$***~**b~**__|#"` is serialized
- **THEN** `to_markdown()` returns `"__#$***~**b~**__|#\n"`

### Requirement: Text is written as recorded
The serializer SHALL write each `Text` value as it is, each `Escape` as a
backslash and its char, and each `CharacterReference` as its reference, and
SHALL add no escape of its own, apart from the backslashes a table cell's
encoding adds before pipes.

#### Scenario: Literal asterisks in hand-built text
- **WHEN** a hand-built paragraph holding `Text("*not emphasis*")` is serialized
- **THEN** `to_markdown()` returns `"*not emphasis*\n"`

#### Scenario: Backticks
- **WHEN** ``parse("Test \\`hello world` here.").document.to_markdown()`` runs
- **THEN** it returns ``"Test \\`hello world` here.\n"``, the second backtick written raw

#### Scenario: Unpaired delimiters
- **WHEN** `parse("x_y_ a*b x^2 ~5").document.to_markdown()` runs
- **THEN** it returns `"x_y_ a*b x^2 ~5\n"`

### Requirement: Table cells encoded by one rule
The serializer SHALL write a table cell's inline content as it writes any
inline content and then encode it as cell source by one rule: a `\` is added
before each `|` that no backslash or an even run of backslashes precedes, and
nothing else changes. The row splits at none of the cell's pipes, and the cell
reads each added `\|` as the `|` written, so its inline content reads as the
content written. A `|` that the content already writes after an odd run of
backslashes, as an `Escape('|')` does, is read with one backslash less, as an
escaped `|`, which it also is outside a cell. The rule covers every inline
alike: a `Text` holding `|` is written with `\|`.

#### Scenario: Escaped pipe written once
- **WHEN** `parse("| a\\|b |\n| - |").document.to_markdown()` runs
- **THEN** it returns `"| a\\|b |\n| --- |\n"`

#### Scenario: Pipe in hand-built text
- **WHEN** a hand-built table whose body cell holds `Text("a|b")` under a header cell holding `Text("h")` is serialized
- **THEN** `to_markdown()` returns `"| h |\n| --- |\n| a\\|b |\n"`, which reads back with the cell holding `Text("a")`, `Escape('|')`, and `Text("b")`

#### Scenario: Pipe after an even backslash run
- **WHEN** a hand-built table cell holds a `CodeInline` whose value is `a`, two backslashes, `|`, and `b`
- **THEN** the cell is written as that code span with a third backslash before the `|`, and reads back with the same value

#### Scenario: Pipes in destinations and titles
- **WHEN** a hand-built table cell holds a `Link` whose text is `Text("a")`, to `b|c` with the double-quoted title `t|u`
- **THEN** the cell is written `[a](b\|c "t\|u")`, and reads back as the same link

### Requirement: Container lines take their full prefix
The serializer SHALL write every line inside a block quote or alert with the
quote's `> ` prefix, and every line inside a list item or footnote definition
with the item's or definition's content indentation; it SHALL NOT write a lazy
continuation line.

#### Scenario: Lazy line in a quote
- **WHEN** `parse("> a\nb").document.to_markdown()` runs
- **THEN** it returns `"> a\n> b\n"`

#### Scenario: Code span across a lazy delimiter-row line
- **WHEN** the document parsed from ``"> `|a\n|-|-|\nb`"`` is serialized
- **THEN** `to_markdown()` returns ``"> `|a\n> |-|-|\n> b`\n"``, which reads back as a block quote holding a `Table`

### Requirement: Heading soft breaks
The serializer SHALL write a `SoftBreak` inside a heading as one space.

#### Scenario: Setext heading on two lines
- **WHEN** `parse("a\nb\n===").document.to_markdown()` runs
- **THEN** it returns `"a b\n===\n"`

#### Scenario: Hand-built ATX heading
- **WHEN** a hand-built level-1 `Heading` holding `Text("a")`, a `SoftBreak`, and `Text("b")` is serialized
- **THEN** `to_markdown()` returns `"# a b\n"`

### Requirement: Values are encoded by rule
The serializer SHALL write a value that a construct holds as raw text (a code
block, math, an info string, a link destination, raw HTML) so that the construct
reads back with the same value: a fence longer than any fence-like run in the
value, whitespace at the ends of an info string or in a bare destination as
character references, and a value's own line endings as they are.

#### Scenario: Empty fenced code block
- **WHEN** ``parse("```\n```").document.to_markdown()`` runs
- **THEN** it returns ``"```\n```\n"``

#### Scenario: Whitespace at the ends of an info string
- **WHEN** the document parsed from ``"```&#x20;a&#9;\nb\n```"`` is serialized
- **THEN** `to_markdown()` returns ``"``` &#x20;a&#x9;\nb\n```\n"`` and reparsing it yields the info string `" a\t"`

#### Scenario: Fence length
- **WHEN** the document parsed from ``"```\n```*"`` is serialized
- **THEN** `to_markdown()` returns ``"```\n```*\n```\n"``, keeping the fence length 3

#### Scenario: Code fence a content line would close
- **WHEN** the document parsed from `" ~~~\n    ~~~"` is serialized
- **THEN** `to_markdown()` returns `" ~~~\n    ~~~\n ~~~\n"`, keeping the fence length 3

#### Scenario: Indented code ending in a carriage return
- **WHEN** the document parsed from `"\ta\r\tb"` is serialized
- **THEN** `to_markdown()` returns `"    a\r    b\r"` and reparsing it yields the value `"a\rb\r"`

#### Scenario: CRLF output keeps a value's CRLF
- **WHEN** the document parsed from ``"```\r\na\r\n```\r\nb"`` is serialized with `LineEnding::CrLf`
- **THEN** the output is ``"```\r\na\r\n```\r\n\r\nb\r\n"``

#### Scenario: Space in a bare destination
- **WHEN** the document parsed from `"[o]:&#x20;"` is serialized
- **THEN** `to_markdown()` returns `"[o]: &#x20;\n"` and the reparsed definition keeps a `Bare` destination `" "`

#### Scenario: HTML block value
- **WHEN** the document parsed from `"<!--\n\n"` is serialized and reparsed
- **THEN** the reparsed `HtmlBlock` value is `"<!--\n"`

### Requirement: Code spans written from their value
The serializer SHALL write a code span from its value alone: fenced by the
shortest backtick run that the value holds no run of and that would not close
a backtick run written before it in the same inline pass that opens no code
span (runs inside code spans, raw HTML, math, autolinks, wiki links, text
directives, and link and image destinations, titles, and reference labels do
not count); and with one space added at each end when the value starts or
ends with a backtick, or starts and ends with a space and is not all spaces.

#### Scenario: Backticks in the value
- **WHEN** a hand-built paragraph holding `CodeInline::new("a``b")` is serialized
- **THEN** `to_markdown()` returns ``"`a``b`\n"``

#### Scenario: Padding
- **WHEN** the document parsed from ``"`` `code` ``"`` is serialized
- **THEN** `to_markdown()` returns ``"`` `code` ``\n"``

#### Scenario: Line endings in the source
- **WHEN** the document parsed from ``"``\nfoo\nbar\n``"`` is serialized
- **THEN** `to_markdown()` returns ``"`foo bar`\n"``

#### Scenario: Backtick run written before the span
- **WHEN** the document parsed from ``"`foo``bar``"`` is serialized
- **THEN** `to_markdown()` returns ``"`foo``bar``\n"``, whose first backtick stays text

### Requirement: Breaks and item content placed by rule
The serializer SHALL write a dash thematic break as `- - -` when it opens the
document or directly follows a paragraph line, where `---` would open
frontmatter or underline a setext heading; and SHALL start a list item's
content on the line after its bullet when that content is a thematic break of
the bullet's char or begins with a space or a tab.

#### Scenario: Dash break opening the document
- **WHEN** `parse("---").document.to_markdown()` runs
- **THEN** it returns `"- - -\n"`

#### Scenario: Dash break after a paragraph line
- **WHEN** `parse("- a\n  - - -").document.to_markdown()` runs
- **THEN** it returns `"- a\n  - - -\n"`

#### Scenario: Break of the bullet's char
- **WHEN** `parse("-\n  ---").document.to_markdown()` runs
- **THEN** it returns `"-\n  ---\n"`

#### Scenario: Item content opening with spaces
- **WHEN** `parse("-\n   <v>").document.to_markdown()` runs
- **THEN** it returns `"-\n   <v>\n"`

### Requirement: Links and autolinks written in their recorded form
The serializer SHALL write a literal `Autolink` as its text, an angle-bracket
`Autolink` as `<` and its text and `>`, and a `Link` as
`[text](destination "title")`.

#### Scenario: Literal URL
- **WHEN** `parse("see http://a.b").document.to_markdown()` runs
- **THEN** it returns `"see http://a.b\n"`

#### Scenario: `www` link
- **WHEN** `parse("www.a.b").document.to_markdown()` runs
- **THEN** it returns `"www.a.b\n"`

#### Scenario: Literal email
- **WHEN** `parse("a@b.c").document.to_markdown()` runs
- **THEN** it returns `"a@b.c\n"`

#### Scenario: Angle-bracket autolink
- **WHEN** `parse("<a@b.c> <http://a.b>").document.to_markdown()` runs
- **THEN** it returns `"<a@b.c> <http://a.b>\n"`

#### Scenario: Inline link whose text is its URL
- **WHEN** `parse("[http://a.b](http://a.b)").document.to_markdown()` runs
- **THEN** it returns `"[http://a.b](http://a.b)\n"`

#### Scenario: Constructed link
- **WHEN** a document holding a paragraph with `Link::new("u", [Text::from("a")])` is serialized
- **THEN** `to_markdown()` returns `"[a](u)\n"`
