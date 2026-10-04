# Untrusted input cost — spec changes

## MODIFIED Requirements

### Requirement: Inline nesting limit
Link and image labels, inline footnotes, directive labels, and `++`, `==`, `~`,
`^`, `||`, and underline spans SHALL nest at most 32 levels; an opener past the
limit SHALL stay literal text.

#### Scenario: Deeply nested highlights
- **WHEN** 40 nested `==` spans are parsed
- **THEN** at most 32 levels of `Mark` are produced and deeper content is literal text

#### Scenario: Deeply nested images
- **WHEN** 40 images nested in each other's alt text are parsed
- **THEN** at most 32 levels of `Image` are produced and the deeper brackets stay literal text

## REMOVED Requirements

### Requirement: Closer search budget
**Reason**: `++`, `==`, and underline spans pair on the delimiter stack, which
needs no start-dependent closer search, so no budget limits them and no opener
is dropped for exceeding one.
