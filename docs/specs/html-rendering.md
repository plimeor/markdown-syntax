# HTML rendering

## Purpose

Rendering a `Document` to HTML, safe by default, for callers that opt into the
`html` feature. Owned by `src/html/`.

## Requirements

### Requirement: Opt-in feature
HTML rendering SHALL exist only with the non-default `html` feature, through
`Document::to_html()` and `Document::to_html_with(&HtmlOptions)`.

#### Scenario: Default build
- **WHEN** the crate is built without the `html` feature
- **THEN** no HTML rendering API is exported

### Requirement: Validate before rendering
Rendering SHALL validate the document first and return
`HtmlError::InvalidDocument` with the validation diagnostics when it is invalid.

#### Scenario: Invalid heading depth
- **WHEN** a hand-built document holding a `Heading` of depth 0 is rendered
- **THEN** `to_html()` returns `Err(HtmlError::InvalidDocument(_))` with a diagnostic about the heading depth

### Requirement: Raw HTML is escaped by default
With default options the renderer SHALL escape raw HTML as text;
`safe_raw_html_form = SafeRawHtmlForm::OmitPlaceholder` SHALL emit the
`<!-- raw HTML omitted -->` placeholder instead; `allow_dangerous_html = true`
SHALL emit raw HTML verbatim, filtered by the GFM tagfilter when `gfm_tagfilter`
is set.

#### Scenario: Default
- **WHEN** `"<div>x</div>"` is rendered with default options
- **THEN** the output is `&lt;div&gt;x&lt;/div&gt;`

#### Scenario: Placeholder form
- **WHEN** `"<div>x</div>"` is rendered with `safe_raw_html_form = OmitPlaceholder`
- **THEN** the output is `<!-- raw HTML omitted -->`

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

### Requirement: Task list checkboxes are disabled by default
The renderer SHALL emit task list checkboxes with `disabled=""` unless
`tasklist_checkable` is set; `tasklist_attr_order` SHALL choose whether
`disabled` or `checked` comes first.

#### Scenario: Default checkbox
- **WHEN** `"- [x] done"` is rendered with default options
- **THEN** the item is `<li><input type="checkbox" disabled="" checked="" /> done</li>`

### Requirement: Document layout
The renderer SHALL join rendered top-level blocks with a single `\n`, with no
leading or trailing newline, and SHALL append the footnote section after them
when any footnote is referenced.

#### Scenario: Two paragraphs
- **WHEN** `"a\n\nb"` is rendered
- **THEN** the output is `<p>a</p>\n<p>b</p>`

### Requirement: Shortcodes render their glyph
The renderer SHALL write a `Shortcode` as the escaped text of its
`Shortcode::glyph()`, with no wrapper element, and SHALL read it as that glyph
in an image's alt text.

#### Scenario: Name from the gemoji table
- **WHEN** `":sparkles:"` is rendered
- **THEN** the output is `<p>✨</p>`

#### Scenario: Shortcode in image alt text
- **WHEN** `"![:tada: x](i.png)"` is rendered
- **THEN** the image's `alt` attribute is `🎉 x`

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
