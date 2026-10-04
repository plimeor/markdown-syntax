# Inline syntax — spec changes

## ADDED Requirements

### Requirement: Marks pair in closing order
The parser SHALL pair every emphasis-like mark (`*`, `_`, `~~`, `~`, `^`, `++`,
`==`, `||`, and underline `__`) on one delimiter stack: each closer, taken in
source order, pairs with the nearest earlier opener it can close, and openers
left between the two stay literal text.

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

### Requirement: Marks inside a link stay inside it
A mark opened inside a link, image, or inline footnote label SHALL pair only
with a closer inside the same label.

#### Scenario: Highlight opener inside a link
- **WHEN** `"[a ==b](u) c=="` is parsed with `parse`
- **THEN** the paragraph holds a `Link` whose text is `a ==b` followed by `Text(" c==")`

#### Scenario: Emphasis opener before a link
- **WHEN** `"*[foo*](/u)"` is parsed with `parse`
- **THEN** the paragraph holds `Text("*")` followed by a `Link` whose text is `foo*`

### Requirement: No literal autolinks inside brackets
When GFM literal autolinks are enabled and relaxed autolinks are not, the parser
SHALL NOT recognize a literal autolink while an unclosed `[` precedes it in the
same inline content; angle-bracket autolinks SHALL be recognized as before.

#### Scenario: Bracketed URL
- **WHEN** `"[https://foo.com]"` is parsed with the GFM preset
- **THEN** the paragraph is the text `[https://foo.com]`

#### Scenario: Double-bracketed URL
- **WHEN** `"[[https://foo.com]]"` is parsed with the GFM preset
- **THEN** the paragraph is the text `[[https://foo.com]]`

#### Scenario: Angle autolink in brackets
- **WHEN** `"[<https://foo.com>]"` is parsed with the GFM preset
- **THEN** the paragraph holds `Text("[")`, an `Autolink` to `https://foo.com`, and `Text("]")`

#### Scenario: Relaxed autolinks
- **WHEN** `"[https://foo.com]"` is parsed with the GFM preset and relaxed autolinks enabled
- **THEN** the paragraph holds `Text("[")`, an `Autolink` to `https://foo.com`, and `Text("]")`

## REMOVED Requirements

### Requirement: Extension marks claim the nearest closer
**Reason**: Extension marks now pair on the shared delimiter stack in closing
order (see "Marks pair in closing order"), so a mark no longer claims its first
closer ahead of marks that close earlier.
