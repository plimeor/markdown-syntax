# HTML rendering — spec changes

## MODIFIED Requirements

### Requirement: Task list checkboxes are disabled by default
The renderer SHALL emit task list checkboxes with `disabled=""` unless
`tasklist_checkable` is set; `tasklist_attr_order` SHALL choose whether
`disabled` or `checked` comes first.

#### Scenario: Default checkbox
- **WHEN** `"- [x] done"` is rendered with default options
- **THEN** the item is `<li><input type="checkbox" disabled="" checked="" /> done</li>`

## REMOVED Requirements

### Requirement: MDX emits no HTML
**Reason**: MDX is removed, so no MDX node reaches the renderer.
