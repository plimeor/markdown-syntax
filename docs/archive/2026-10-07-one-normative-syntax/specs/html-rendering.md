# HTML rendering — spec changes

## MODIFIED Requirements

### Requirement: Task list checkboxes are disabled by default
The renderer SHALL emit task list checkboxes with `disabled=""` unless
`tasklist_checkable` is set; `tasklist_attr_order` SHALL choose whether
`disabled` or `checked` comes first.

#### Scenario: Default checkbox
- **WHEN** `"- [x] done"` is rendered with default options
- **THEN** the item is `<li><input type="checkbox" disabled="" checked="" /> done</li>`

### Requirement: Dangerous protocols are blanked by default
With default options the renderer SHALL blank the `href` or `src` of links,
including those parsed from autolinks, and images whose destination uses a
dangerous protocol such as `javascript:`, and the `href` of a wiki link whose
decoded target uses `javascript:`, `vbscript:`, `file:`, or a `data:` URI other
than an image; `allow_dangerous_protocol` SHALL keep them and
`allow_any_img_src` SHALL exempt image sources.

#### Scenario: Link
- **WHEN** `"[x](javascript:alert(1))"` is rendered with default options
- **THEN** the output is `<p><a href="">x</a></p>`

#### Scenario: Autolink
- **WHEN** `"<javascript:alert(1)>"` is rendered with default options
- **THEN** the output is `<p><a href="">javascript:alert(1)</a></p>`

#### Scenario: Wiki link
- **WHEN** `"[[javascript&#58;alert(1)|x]]"` and `"[[Note: x]]"` are rendered with default options
- **THEN** the outputs are `<p><a href="" data-wikilink="true">x</a></p>` and `<p><a href="Note:%20x" data-wikilink="true">Note: x</a></p>`

### Requirement: Wiki embeds are marked
The renderer SHALL write a `WikiLink` marked as an embed as the same anchor it
writes for a wiki link, with a `data-wikilink-embed="true"` attribute after
`data-wikilink="true"`, and SHALL NOT resolve what the target is. It SHALL
write a wiki link's `decoded_target()` and `decoded_label()`, the target and
label with their escapes and character references decoded, the target encoded
as a link destination is.

#### Scenario: Ampersand in a target
- **WHEN** `"[[a&b]]"` is rendered
- **THEN** the output is `<p><a href="a&amp;b" data-wikilink="true">a&amp;b</a></p>`

#### Scenario: Image target
- **WHEN** `"![[x.png]]"` is rendered
- **THEN** the output is `<p><a href="x.png" data-wikilink="true" data-wikilink-embed="true">x.png</a></p>`

## REMOVED Requirements

### Requirement: MDX emits no HTML
**Reason**: MDX is removed, so no MDX node reaches the renderer.
