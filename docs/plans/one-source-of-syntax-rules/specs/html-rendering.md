# HTML rendering — spec changes

## ADDED Requirements

### Requirement: Shortcodes render their glyph
The renderer SHALL write a `Shortcode` as the escaped text of its
`Shortcode::glyph()`, with no wrapper element.

#### Scenario: Name outside the old built-in list
- **WHEN** `":sparkles:"` is rendered
- **THEN** the output is `<p>✨</p>`

### Requirement: Wiki embeds are marked
The renderer SHALL write a `WikiLink` marked as an embed as the same anchor it
writes for a wiki link, with a `data-wikilink-embed="true"` attribute after
`data-wikilink="true"`, and SHALL NOT resolve what the target is.

#### Scenario: Image target
- **WHEN** `"![[x.png]]"` is rendered
- **THEN** the output is `<p><a href="x.png" data-wikilink="true" data-wikilink-embed="true">x.png</a></p>`

## MODIFIED Requirements

### Requirement: Dangerous protocols are blanked by default
With default options the renderer SHALL blank the `href` or `src` of links,
including those parsed from autolinks, and images whose destination uses a
dangerous protocol such as `javascript:`; `allow_dangerous_protocol` SHALL keep
them and `allow_any_img_src` SHALL exempt image sources.

#### Scenario: Link
- **WHEN** `"[x](javascript:alert(1))"` is rendered with default options
- **THEN** the output is `<p><a href="">x</a></p>`

#### Scenario: Autolink
- **WHEN** `"<javascript:alert(1)>"` is rendered with default options
- **THEN** the output is `<p><a href="">javascript:alert(1)</a></p>`
