# Inline syntax — spec changes

## ADDED Requirements

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

## MODIFIED Requirements

### Requirement: Superscript and inline footnotes share the caret
The parser SHALL read `^[…]` as an inline footnote when inline footnotes are
enabled, unless a superscript on the same line is waiting to close at that `^`,
and `^x^` as `Superscript` otherwise.

#### Scenario: Inline footnote
- **WHEN** `"note^[x] tail"` is parsed with `parse`
- **THEN** the second inline is an `InlineFootnote`

#### Scenario: Superscript
- **WHEN** `"x^2^"` is parsed with `parse`
- **THEN** the second inline is a `Superscript`

#### Scenario: Superscript closing before a bracket
- **WHEN** `"a^b^[link](u)"` is parsed with `parse`
- **THEN** the paragraph holds `Text("a")`, a `Superscript` containing `b`, and a `Link` whose text is `link`

## REMOVED Requirements

### Requirement: Extension marks claim the nearest closer
**Reason**: Extension marks now pair on the shared delimiter stack in closing
order (see "Marks pair in closing order"), so a mark no longer claims its first
closer ahead of marks that close earlier.
