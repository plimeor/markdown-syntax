# Inline syntax — spec changes

## ADDED Requirements

### Requirement: Tags
The parser SHALL read a `#` followed by one Unicode letter and then any number
of Unicode letters, Unicode digits, `/`, `_`, or `-` as an `Inline::Tag`
whose `name` is that run without the `#`, where the character before the `#`
is whitespace or the start of the leaf's inline content. In deciding that
character, the content of `Emphasis`, `Strong`, `Delete`, and `Mark` SHALL
join the text around it as if their delimiters were absent; the content of a
link, image, inline footnote, or text directive SHALL be read with an edge
that is not whitespace on each side; and every other inline SHALL count as one
character that is not whitespace, nothing inside it being read. A tag SHALL
end at the first character it cannot hold.

#### Scenario: Delimiters are transparent
- **WHEN** `parse("*#a* **b**#c")` runs
- **THEN** the paragraph holds one `Tag` named `a`, inside the `Emphasis`, and `#c` stays text

#### Scenario: Link text
- **WHEN** `parse("[see #rust](u) [#go](u)")` runs
- **THEN** the first link's text holds a `Tag` named `rust`, and the second link's text holds only text
