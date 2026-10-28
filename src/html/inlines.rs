//! Every `Inline` arm.

use alloc::string::String;
use core::fmt::Write as _;

use crate::ast::{DirectiveAttribute, Inline, MathInlineKind};

use super::escape::{attr_escape, encode_href, escape_text, filter_img_protocol, filter_protocol};
use super::footnotes;
use super::refs::escaped_alt;
use super::{Ctx, SafeRawHtmlForm};

/// Render an inline slice by appending each node's HTML to `out`.
pub fn render_inlines(children: &[Inline], ctx: &Ctx, out: &mut String) {
    for child in children {
        render_inline(child, ctx, out);
    }
}

/// Render a single inline node into `out`. Every `Inline` arm is handled
/// explicitly — there is no catch-all.
pub fn render_inline(inline: &Inline, ctx: &Ctx, out: &mut String) {
    match inline {
        // 1. Text — escape the parsed text value.
        Inline::Text(t) => escape_text(out, &t.value),

        // 2. Escape — the single escaped char, text-escaped.
        Inline::Escape(e) => {
            let mut buf = [0u8; 4];
            escape_text(out, e.value.encode_utf8(&mut buf));
        }

        // 3. CharacterReference — the decoded scalar(s), re-escaped.
        Inline::CharacterReference(c) => escape_text(out, &c.value().unwrap_or_default()),

        // 4. Emphasis.
        Inline::Emphasis(n) => wrap_inlines(out, "<em>", &n.children, "</em>", ctx),

        // 5. Strong.
        Inline::Strong(n) => wrap_inlines(out, "<strong>", &n.children, "</strong>", ctx),

        // 6. Delete.
        Inline::Delete(n) => wrap_inlines(out, "<del>", &n.children, "</del>", ctx),

        // 7. Mark (GFM `==x==`).
        Inline::Mark(n) => wrap_inlines(out, "<mark>", &n.children, "</mark>", ctx),

        // 8. Shortcode — its gemoji glyph, text-escaped, no wrapper; a
        // name gemoji does not hold stays as written.
        Inline::Shortcode(s) => match s.glyph() {
            Some(glyph) => escape_text(out, glyph),
            None => {
                out.push(':');
                escape_text(out, &s.name);
                out.push(':');
            }
        },

        // Tag — its text, `#` and the name, with no element around it.
        Inline::Tag(t) => {
            out.push('#');
            escape_text(out, &t.name);
        }

        // 9. Code — `value` already code-span-normalized; text-escape only.
        Inline::Code(c) => {
            out.push_str("<code>");
            escape_text(out, &c.value);
            out.push_str("</code>");
        }

        // 10. Link.
        Inline::Link(n) => {
            out.push_str("<a href=\"");
            encode_href(
                out,
                filter_protocol(
                    &n.destination,
                    ctx.allow_dangerous_protocol,
                    ctx.gfm_url_denylist(),
                ),
            );
            out.push('"');
            title_attr(out, n.title.as_ref().map(|title| title.value.as_str()));
            out.push('>');
            render_inlines(&n.children, ctx, out);
            out.push_str("</a>");
        }

        // 10a. Autolink — its derived destination, its text as the link text.
        Inline::Autolink(n) => {
            out.push_str("<a href=\"");
            encode_href(
                out,
                filter_protocol(
                    &n.destination().unwrap_or_default(),
                    ctx.allow_dangerous_protocol,
                    ctx.gfm_url_denylist(),
                ),
            );
            out.push_str("\">");
            escape_text(out, &n.text);
            out.push_str("</a>");
        }

        // 11. Image.
        Inline::Image(n) => {
            out.push_str("<img src=\"");
            encode_href(
                out,
                filter_img_protocol(
                    &n.destination,
                    ctx.allow_dangerous_protocol,
                    ctx.allow_any_img_src,
                ),
            );
            out.push_str("\" alt=\"");
            escaped_alt(out, &n.alt);
            out.push('"');
            title_attr(out, n.title.as_ref().map(|title| title.value.as_str()));
            out.push_str(" />");
        }

        // 12. LinkReference — resolve against the definition map.
        Inline::LinkReference(n) => match ctx.defs.resolve(&n.identifier) {
            Some(def) => {
                out.push_str("<a href=\"");
                encode_href(
                    out,
                    filter_protocol(
                        &def.destination,
                        ctx.allow_dangerous_protocol,
                        ctx.gfm_url_denylist(),
                    ),
                );
                out.push('"');
                title_attr(out, def.title.as_ref().map(|title| title.value.as_str()));
                out.push('>');
                render_inlines(&n.children, ctx, out);
                out.push_str("</a>");
            }
            None => link_reference_fallback(n, ctx, out),
        },

        // 13. ImageReference — resolve against the definition map.
        Inline::ImageReference(n) => match ctx.defs.resolve(&n.identifier) {
            Some(def) => {
                out.push_str("<img src=\"");
                encode_href(
                    out,
                    filter_img_protocol(
                        &def.destination,
                        ctx.allow_dangerous_protocol,
                        ctx.allow_any_img_src,
                    ),
                );
                out.push_str("\" alt=\"");
                escaped_alt(out, &n.alt);
                out.push('"');
                title_attr(out, def.title.as_ref().map(|title| title.value.as_str()));
                out.push_str(" />");
            }
            None => image_reference_fallback(n, out),
        },

        // 14. Html — verbatim under danger (with tagfilter), else text-escape.
        Inline::Html(h) => render_raw_html(&h.value, ctx, out),

        // 15. SoftBreak.
        Inline::SoftBreak(_) => out.push('\n'),

        // 16. LineBreak — both kinds identical.
        Inline::LineBreak(_) => out.push_str("<br />\n"),

        // 17. Math (GFM form). A 2+-dollar fence is display, a 1-dollar fence is
        //     inline, and `$`…`$` code-math is an inline `<code>`.
        Inline::Math(m) => {
            let (open, close) = match m.kind {
                MathInlineKind::Code => ("<code data-math-style=\"inline\">", "</code>"),
                MathInlineKind::Dollar { dollars } if dollars >= 2 => {
                    ("<span data-math-style=\"display\">", "</span>")
                }
                MathInlineKind::Dollar { .. } => ("<span data-math-style=\"inline\">", "</span>"),
            };
            out.push_str(open);
            escape_text(out, &m.value);
            out.push_str(close);
        }

        // 18. FootnoteReference (GFM shape). An undefined reference renders
        //     as its literal `[^label]` source text.
        Inline::FootnoteReference(fr) => {
            if ctx.footnotes.is_defined(&fr.identifier) {
                footnote_marker(&fr.identifier, ctx, out);
            } else {
                out.push_str("[^");
                escape_text(out, &fr.label);
                out.push(']');
            }
        }

        // 19. InlineFootnote — renders like a footnote reference; its body was
        //     harvested into the doc-end section during the pre-pass.
        Inline::InlineFootnote(_) => {
            let id = footnotes::next_inline_id(ctx.footnotes);
            footnote_marker(&id, ctx, out);
        }

        // 20. WikiLink — GFM shape. A note name like `Note: x` reads as a
        //     scheme, so the target takes the scheme denylist, never the
        //     allowlist: only a dangerous scheme is blanked.
        Inline::WikiLink(w) => {
            out.push_str("<a href=\"");
            encode_href(
                out,
                filter_protocol(&w.decoded_target(), ctx.allow_dangerous_protocol, true),
            );
            out.push_str("\" data-wikilink=\"true\"");
            if w.embed {
                out.push_str(" data-wikilink-embed=\"true\"");
            }
            out.push('>');
            escape_text(out, &w.decoded_label());
            out.push_str("</a>");
        }

        // 21. TextDirective [CONV] — classed span carrying name + attrs.
        Inline::TextDirective(d) => {
            out.push_str("<span class=\"directive directive-text\" data-directive-name=\"");
            attr_escape(out, &d.name);
            out.push('"');
            directive_attrs(&d.attributes, out);
            out.push('>');
            render_inlines(&d.label, ctx, out);
            out.push_str("</span>");
        }
    }
}

/// `{open}{children}{close}`.
fn wrap_inlines(out: &mut String, open: &str, children: &[Inline], close: &str, ctx: &Ctx) {
    out.push_str(open);
    render_inlines(children, ctx, out);
    out.push_str(close);
}

/// ` title="…"` when the title is `Some` (including empty `""` → drop, since
/// the parser only produces `Some("")` for an explicit empty title which the
/// oracles still drop). CommonMark/GFM both drop an empty title.
fn title_attr(out: &mut String, title: Option<&str>) {
    match title {
        Some(t) if !t.is_empty() => {
            out.push_str(" title=\"");
            attr_escape(out, t);
            out.push('"');
        }
        _ => {}
    }
}

/// The GFM footnote reference marker `<sup class="footnote-ref">…`. The
/// `#fn-`/`fnref-` ids use the first definition's preserved-case label.
fn footnote_marker(id: &str, ctx: &Ctx, out: &mut String) {
    let (number, fnref) = footnotes::reference_marker(ctx.footnotes, id);
    let enc = footnotes::reference_fn_target(ctx.footnotes, id);
    // Writing to a `String` cannot fail.
    let _ = write!(
        out,
        "<sup class=\"footnote-ref\"><a href=\"#fn-{enc}\" id=\"{fnref}\" data-footnote-ref>{number}</a></sup>",
    );
}

pub(super) fn render_raw_html(value: &str, ctx: &Ctx, out: &mut String) {
    if !ctx.allow_dangerous_html {
        safe_raw_html(value, ctx, out);
    } else if ctx.gfm_tagfilter {
        apply_tagfilter(value, out);
    } else {
        out.push_str(value);
    }
}

/// Shared directive data-* attribute serializer.
pub(super) fn directive_attrs(attributes: &[DirectiveAttribute], out: &mut String) {
    for attr in attributes {
        let value = attr.value.as_deref().unwrap_or("");
        out.push_str(" data-");
        attr_escape(out, &attr.name);
        out.push_str("=\"");
        attr_escape(out, value);
        out.push('"');
    }
}

/// Safe-mode raw HTML: the gfm suite replaces it with a fixed placeholder; the
/// commonmark suite text-escapes it (oracle `html_flow` case 1 →
/// `&lt;!-- asd --&gt;`). Shared by the inline and block raw-HTML renderers.
pub(super) fn safe_raw_html(value: &str, ctx: &Ctx, out: &mut String) {
    match ctx.safe_raw_html_form {
        SafeRawHtmlForm::OmitPlaceholder => out.push_str(RAW_HTML_OMITTED),
        SafeRawHtmlForm::EscapeText => escape_text(out, value),
    }
}

/// The GFM safe-mode placeholder emitted in place of raw HTML.
pub(super) const RAW_HTML_OMITTED: &str = "<!-- raw HTML omitted -->";

/// GFM tagfilter: rewrite the leading `<` of a blocklisted open/close tag to
/// `&lt;`. Applied only when danger is on. Text between rewritten `<`s is
/// copied whole.
pub fn apply_tagfilter(value: &str, out: &mut String) {
    const BLOCKED: [&str; 9] = [
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];
    let mut start = 0;
    for (i, &b) in value.as_bytes().iter().enumerate() {
        if b != b'<' {
            continue;
        }
        let after = &value[i + 1..];
        let tag_body = after.strip_prefix('/').unwrap_or(after);
        // A head that matches a name is ASCII, so the name ends on a char
        // boundary of `tag_body`.
        let matched = BLOCKED.iter().find(|tag| {
            tag_body
                .as_bytes()
                .get(..tag.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(tag.as_bytes()))
                && tag_terminates(&tag_body[tag.len()..])
        });
        if matched.is_some() {
            out.push_str(&value[start..i]);
            out.push_str("&lt;");
            start = i + 1;
        }
    }
    out.push_str(&value[start..]);
}

/// True when the char after a blocklisted tag name terminates the tag name
/// (so `<scriptx>` is not filtered but `<script>`/`<script `/`<script/` are).
fn tag_terminates(rest: &str) -> bool {
    match rest.chars().next() {
        None => true,
        Some(c) => c == '>' || c == '/' || c.is_whitespace(),
    }
}

/// Literal-source fallback for an unresolved link reference: re-emit the
/// bracketed source text so the surrounding output is not silently dropped.
fn link_reference_fallback(n: &crate::ast::LinkReference, ctx: &Ctx, out: &mut String) {
    use crate::ast::ReferenceKind;
    out.push('[');
    render_inlines(&n.children, ctx, out);
    match n.kind {
        ReferenceKind::Shortcut => out.push(']'),
        ReferenceKind::Collapsed => out.push_str("][]"),
        ReferenceKind::Full => {
            out.push_str("][");
            escape_text(out, &n.label);
            out.push(']');
        }
    }
}

/// Literal-source fallback for an unresolved image reference.
fn image_reference_fallback(n: &crate::ast::ImageReference, out: &mut String) {
    use crate::ast::ReferenceKind;
    out.push_str("![");
    escaped_alt(out, &n.alt);
    match n.kind {
        ReferenceKind::Shortcut => out.push(']'),
        ReferenceKind::Collapsed => out.push_str("][]"),
        ReferenceKind::Full => {
            out.push_str("][");
            escape_text(out, &n.label);
            out.push(']');
        }
    }
}
