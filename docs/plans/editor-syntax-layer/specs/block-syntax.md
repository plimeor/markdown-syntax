# Block syntax — spec changes

## MODIFIED Requirements

### Requirement: Block directives
The parser SHALL recognize `::name[label]{attrs}` leaf directives standing
alone on their line, followed by nothing but spaces and tabs, and `:::name`
container directives, whose container closes at a fence of at least the
opening fence's length. A directive name SHALL be one or more runs of ASCII
letters joined by single `-` chars. A closing fence SHALL close the innermost
open container directive it can close, and SHALL do so before any block
inside that directive reads the line. A container directive is a container on
the open-block stack, whose content lines are read as the document's lines
are: a line that looks like a directive opener inside a fenced code block or
HTML block is that block's content, and one inside a list item opens a
directive in that item. A line with a malformed opener, or with text after a
leaf directive, SHALL NOT end a paragraph and SHALL be read as paragraph text.
An attribute without a valid name SHALL be dropped and reported as a
warning-severity diagnostic.

#### Scenario: Container directive
- **WHEN** `":::note\nbody\n:::"` is parsed with `parse`
- **THEN** the document holds a `ContainerDirective` named `note` whose children hold a paragraph `body`

#### Scenario: Unclosed container
- **WHEN** `":::note\nunclosed container"` is parsed
- **THEN** a `ContainerDirective` holds the remaining content and an error-severity `UnclosedDirectiveContainer` diagnostic is reported

#### Scenario: Invalid name
- **WHEN** a leaf directive opener has a malformed name
- **THEN** an error-severity `InvalidDirectiveName` diagnostic is reported

#### Scenario: Digit or underscore in a name
- **WHEN** `"::h1"` and `"::my_note"` are parsed with `parse`
- **THEN** neither document holds a `LeafDirective`, and each reports an error-severity `InvalidDirectiveName` diagnostic

#### Scenario: Hyphenated name
- **WHEN** `":::call-out\nbody\n:::"` is parsed with `parse`
- **THEN** the document holds a `ContainerDirective` named `call-out`

#### Scenario: Directive attribute without a valid name
- **WHEN** `":b{<} :c{a <=1 d}"` is parsed with `parse` and serialized without its source
- **THEN** `to_markdown()` returns `":b :c{a d}\n"`, and one warning-severity `InvalidDirectiveAttribute` diagnostic is reported for each of `<` and `<=1`

#### Scenario: Dotted and colon-led attribute names
- **WHEN** `"::a{x.y=1 :b=2 data-x=3}"` is parsed with `parse`
- **THEN** the `LeafDirective` keeps only `data-x`, and two warning-severity `InvalidDirectiveAttribute` diagnostics span `x.y=1` and `:b=2`

#### Scenario: Text after a leaf directive
- **WHEN** `"x\n::a b"` is parsed with `parse`
- **THEN** the document holds one `Paragraph` holding both lines and no `LeafDirective`

#### Scenario: Malformed directive line inside a paragraph
- **WHEN** `"a\n::1bad"` or `"a\n:::"` is parsed with `parse`
- **THEN** the document holds one `Paragraph` holding both lines

#### Scenario: Nested directives close innermost first
- **WHEN** `":::outer\n:::inner\nx\n:::\n:::\nafter"` is parsed with `parse`
- **THEN** the `ContainerDirective` named `outer` holds the `ContainerDirective` named `inner`, which holds a paragraph `x`, and a `Paragraph` holding `Text("after")` follows `outer`

#### Scenario: Directive opener inside an HTML block
- **WHEN** `":::e\n<y>\n:::e"` is parsed with `parse`
- **THEN** the `ContainerDirective` named `e` holds an `HtmlBlock` whose value is `"<y>\n:::e"`, an `UnclosedDirectiveContainer` diagnostic is reported, and the document round-trips

#### Scenario: Closing-fence-like line inside a list item's fence
- **WHEN** ``":::t\n- ```\n  :::e\n  ```\n:::\nafter"`` is parsed with `parse`
- **THEN** the `ContainerDirective` named `t` holds a `List` whose item holds a `CodeBlock` with value `":::e\n"`, and a `Paragraph` holding `Text("after")` follows the directive

#### Scenario: Leaf directive in a list item before lazy lines
- **WHEN** `"- ::name[x]\n  foo\nbar"` is parsed with `parse`
- **THEN** the item holds a `LeafDirective` followed by one `Paragraph` holding `Text("foo")`, a `SoftBreak`, and `Text("bar")`
