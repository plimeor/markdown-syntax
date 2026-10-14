# HTML rendering — spec changes

## ADDED Requirements

### Requirement: Tags render as their text
The renderer SHALL write a `Tag` as the text `#` followed by its name,
escaped as text, with no element around it.
