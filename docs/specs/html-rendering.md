# HTML rendering
Type: api

## Purpose

Rendering a `Document` to HTML, safe by default, for callers that opt into the
`html` feature. Owned by the HTML renderer.

## Requirements

### Requirement: Opt-in feature
HTML rendering SHALL exist only with the non-default `html` feature, through
`Document::to_html()` and `Document::to_html_with(&HtmlOptions)`.

### Requirement: Validate before rendering
Rendering SHALL validate the document first and return
`HtmlError::InvalidDocument` with the validation diagnostics when it is invalid.

### Requirement: Raw HTML is escaped by default
With default options the renderer SHALL escape raw HTML as text;
`safe_raw_html_form = SafeRawHtmlForm::OmitPlaceholder` SHALL emit the
`<!-- raw HTML omitted -->` placeholder instead; `allow_dangerous_html = true`
SHALL emit raw HTML verbatim, filtered by the GFM tagfilter when `gfm_tagfilter`
is set.

### Requirement: Dangerous protocols are blanked by default
With default options the renderer SHALL blank the `href` of a link, including
one parsed from an autolink, whose destination has a scheme other than `http`,
`https`, `irc`, `ircs`, `mailto`, or `xmpp` and is not a `data:image/png`,
`data:image/gif`, `data:image/jpeg`, or `data:image/webp` URI; the `src` of an
image whose destination has a scheme other than `http` or `https` and is not
one of those `data:` image URIs; and the `href` of a wiki link whose decoded
target uses `javascript:`, `vbscript:`, `file:`, or a `data:` URI other than
those image ones. A destination with no scheme SHALL be kept.
`allow_dangerous_protocol` SHALL keep them and `allow_any_img_src` SHALL exempt
image sources.

#### Scenario: Wiki link
- **WHEN** `"[[javascript&#58;alert(1)|x]]"` and `"[[Note: x]]"` are rendered with default options
- **THEN** the outputs are `<p><a href="" data-wikilink="true">x</a></p>` and `<p><a href="Note:%20x" data-wikilink="true">Note: x</a></p>`

### Requirement: Task list checkboxes are disabled by default
The renderer SHALL emit task list checkboxes with `disabled=""` unless
`tasklist_checkable` is set; `tasklist_attr_order` SHALL choose whether
`disabled` or `checked` comes first.

### Requirement: Document layout
The renderer SHALL join rendered top-level blocks with a single `\n`, with no
leading or trailing newline, and SHALL append the footnote section after them
when any footnote is referenced.

### Requirement: Shortcodes render their glyph
The renderer SHALL write a `Shortcode` as the escaped text of its
`Shortcode::glyph()`, with no wrapper element, and SHALL read it as that glyph
in an image's alt text.

### Requirement: Wiki embeds are marked
The renderer SHALL write a `WikiLink` marked as an embed as the same anchor it
writes for a wiki link, with a `data-wikilink-embed="true"` attribute after
`data-wikilink="true"`, and SHALL NOT resolve what the target is. It SHALL
write a wiki link's `decoded_target()` and `decoded_label()`, the target and
label with their escapes and character references decoded, the target encoded
as a link destination is.

### Requirement: Tags render as their text
The renderer SHALL write a `Tag` as the text `#` followed by its name,
escaped as text, with no element around it.
