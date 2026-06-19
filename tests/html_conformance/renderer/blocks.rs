//! All 19 `Block` arms plus the nested `ListItem` / `DescriptionItem` /
//! `DescriptionDetails` helpers they dispatch to.

use std::format;
use std::string::String;
use std::vec::Vec;

use markdown_syntax::ast::{
    Alert, AlertKind, Block, BlockQuote, CodeBlock, ContainerDirective, DescriptionList, Heading,
    LeafDirective, List, ListItem, MathBlock, Paragraph,
};

use crate::types::Category;

use super::escape::{attr_escape, escape_text};
use super::inlines::{apply_tagfilter, render_inlines, safe_raw_html};
use super::refs::flatten_alt;
use super::tables::render_table;
use super::Ctx;

/// Render a block list, dropping empty-string results (Definition, Frontmatter,
/// MDX*, source-position FootnoteDefinitions, output-less directives) and
/// joining survivors with a single `\n`. No leading/trailing newline.
pub fn render_blocks_joined(blocks: &[Block], ctx: &Ctx) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in blocks {
        let rendered = render_block(block, ctx);
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    parts.join("\n")
}

/// Top-level `Block` dispatch — every one of the 19 arms is handled; there is
/// no catch-all.
pub fn render_block(block: &Block, ctx: &Ctx) -> String {
    match block {
        // 1. Paragraph (loose context; tight suppression is applied by the
        //    list/description helpers that call render_paragraph directly).
        Block::Paragraph(p) => render_paragraph(p, ctx),

        // 2. Heading — Atx and Setext render identically.
        Block::Heading(h) => render_heading(h, ctx),

        // 3. ThematicBreak — all markers identical.
        Block::ThematicBreak(_) => String::from("<hr />"),

        // 4. BlockQuote.
        Block::BlockQuote(bq) => render_blockquote(bq, ctx),

        // 5. Alert (GFM alert extension).
        Block::Alert(a) => render_alert(a, ctx),

        // 6. List.
        Block::List(list) => render_list(list, ctx),

        // 7. DescriptionList (GFM extension).
        Block::DescriptionList(dl) => render_description_list(dl, ctx),

        // 8. CodeBlock.
        Block::CodeBlock(cb) => render_code_block(cb),

        // 9. HtmlBlock.
        Block::HtmlBlock(hb) => render_html_block(&hb.value, ctx),

        // 10. Definition — emits nothing (feeds reference resolution).
        Block::Definition(_) => String::new(),

        // 11. FootnoteDefinition — hoisted to the doc-end section; nothing here.
        Block::FootnoteDefinition(_) => String::new(),

        // 12. Table (GFM).
        Block::Table(t) => render_table(t, ctx),

        // 13. MathBlock — GFM display wrapper.
        Block::MathBlock(mb) => render_math_block(mb, ctx),

        // 14. Frontmatter — no HTML.
        Block::Frontmatter(_) => String::new(),

        // 15. MdxEsm — no HTML.
        Block::MdxEsm(_) => String::new(),

        // 16. MdxExpression (flow) — no HTML.
        Block::MdxExpression(_) => String::new(),

        // 17. MdxJsx (flow) — no HTML (node carries no children).
        Block::MdxJsx(_) => String::new(),

        // 18. LeafDirective [CONV].
        Block::LeafDirective(d) => render_leaf_directive(d, ctx),

        // 19. ContainerDirective.
        Block::ContainerDirective(d) => render_container_directive(d, ctx),
    }
}

fn render_paragraph(p: &Paragraph, ctx: &Ctx) -> String {
    format!("<p>{}</p>", render_inlines(&p.children, ctx))
}

fn render_heading(h: &Heading, ctx: &Ctx) -> String {
    let depth = h.depth.clamp(1, 6);
    format!("<h{depth}>{}</h{depth}>", render_inlines(&h.children, ctx))
}

fn render_blockquote(bq: &BlockQuote, ctx: &Ctx) -> String {
    let inner = render_blocks_joined(&bq.children, ctx);
    if inner.is_empty() {
        String::from("<blockquote>\n</blockquote>")
    } else {
        format!("<blockquote>\n{inner}\n</blockquote>")
    }
}

fn alert_default_title(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "Note",
        AlertKind::Tip => "Tip",
        AlertKind::Important => "Important",
        AlertKind::Warning => "Warning",
        AlertKind::Caution => "Caution",
    }
}

fn alert_class_suffix(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "note",
        AlertKind::Tip => "tip",
        AlertKind::Important => "important",
        AlertKind::Warning => "warning",
        AlertKind::Caution => "caution",
    }
}

fn render_alert(a: &Alert, ctx: &Ctx) -> String {
    let suffix = alert_class_suffix(a.kind);
    let title = match a.title.as_deref() {
        Some(t) => escape_text(t),
        None => String::from(alert_default_title(a.kind)),
    };
    let inner = render_blocks_joined(&a.children, ctx);
    let body = if inner.is_empty() {
        String::new()
    } else {
        format!("\n{inner}")
    };
    format!(
        "<div class=\"markdown-alert markdown-alert-{suffix}\">\n<p class=\"markdown-alert-title\">{title}</p>{body}\n</div>",
    )
}

/// Ordered-list `start` attr: emitted unless `start` is `Some(1)` or `None`.
fn ordered_start_attr(list: &List) -> String {
    match list.start {
        Some(n) if n != 1 => format!(" start=\"{n}\""),
        _ => String::new(),
    }
}

fn render_list(list: &List, ctx: &Ctx) -> String {
    let items = render_list_items(list, ctx);
    if list.ordered {
        format!("<ol{}>\n{items}\n</ol>", ordered_start_attr(list))
    } else {
        format!("<ul>\n{items}\n</ul>")
    }
}

fn render_list_items(list: &List, ctx: &Ctx) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(list.children.len());
    for item in &list.children {
        parts.push(render_list_item(item, list.tight, ctx));
    }
    parts.join("\n")
}

fn render_list_item(item: &ListItem, tight: bool, ctx: &Ctx) -> String {
    let checkbox = item
        .checked
        .map(|checked| task_checkbox(checked, ctx.tasklist_checkable, ctx.category));

    if tight {
        render_tight_item(item, checkbox, ctx)
    } else {
        render_loose_item(item, checkbox, ctx)
    }
}

/// Tight `<li>`: child paragraphs contribute bare inline content; sibling
/// blocks keep their own tags; all joined with `\n`.
///
/// Following the CommonMark/GFM conventions, a block-level `<li>` child sits on its
/// own line: when the content BEGINS with a block (first emitted child is not
/// an unwrapped paragraph) a `\n` is prepended, and when it ENDS with a block a
/// `\n` is appended — yielding `<li>foo\n<ul>…</ul>\n</li>` and
/// `<li>\n<pre>…</pre>\n</li>`.
fn render_tight_item(item: &ListItem, checkbox: Option<String>, ctx: &Ctx) -> String {
    // Each emitted part tracks whether it came from an unwrapped paragraph.
    let mut parts: Vec<(String, bool)> = Vec::new();
    let mut checkbox = checkbox;
    for child in &item.children {
        let (rendered, is_paragraph) = match child {
            Block::Paragraph(p) => {
                let mut inner = render_inlines(&p.children, ctx);
                if let Some(cb) = checkbox.take() {
                    inner = format!("{cb} {inner}");
                }
                (inner, true)
            }
            other => (render_block(other, ctx), false),
        };
        if !rendered.is_empty() {
            parts.push((rendered, is_paragraph));
        }
    }
    // A checkbox with no paragraph still leads the content (as bare text).
    if let Some(cb) = checkbox.take() {
        parts.insert(0, (cb, true));
    }

    if parts.is_empty() {
        return String::from("<li></li>");
    }

    let begins_with_block = !parts.first().map(|(_, p)| *p).unwrap_or(true);
    let ends_with_block = !parts.last().map(|(_, p)| *p).unwrap_or(true);
    let body = parts
        .into_iter()
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join("\n");
    let lead = if begins_with_block { "\n" } else { "" };
    let trail = if ends_with_block { "\n" } else { "" };
    format!("<li>{lead}{body}{trail}</li>")
}

/// Loose `<li>`: every child rendered fully (paragraphs stay `<p>`), joined
/// with `\n`, on its own lines.
fn render_loose_item(item: &ListItem, checkbox: Option<String>, ctx: &Ctx) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut checkbox = checkbox;
    for child in &item.children {
        let rendered = match child {
            Block::Paragraph(p) if checkbox.is_some() => {
                let cb = checkbox.take().unwrap();
                format!("<p>{cb} {}</p>", render_inlines(&p.children, ctx))
            }
            other => render_block(other, ctx),
        };
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    if let Some(cb) = checkbox.take() {
        parts.insert(0, cb);
    }
    let inner = parts.join("\n");
    if inner.is_empty() {
        String::from("<li>\n</li>")
    } else {
        format!("<li>\n{inner}\n</li>")
    }
}

/// GFM task-list checkbox `<input>`; `checkable` (non-default) drops the
/// `disabled=""`. Always followed by one literal space before the content.
///
/// Attribute order is CATEGORY-DIVERGENT between the two suite layers: the gfm
/// suite emits `checked=""` before `disabled=""`; the commonmark suite emits
/// `disabled=""` before `checked=""`.
fn task_checkbox(checked: bool, checkable: bool, category: Category) -> String {
    let checked_attr = if checked { " checked=\"\"" } else { "" };
    if checkable {
        return format!("<input type=\"checkbox\"{checked_attr} />");
    }
    match category {
        Category::Gfm => format!("<input type=\"checkbox\"{checked_attr} disabled=\"\" />"),
        Category::CommonMark => format!("<input type=\"checkbox\" disabled=\"\"{checked_attr} />"),
    }
}

fn render_description_list(dl: &DescriptionList, ctx: &Ctx) -> String {
    let mut parts: Vec<String> = Vec::new();
    for item in &dl.children {
        parts.push(format!("<dt>{}</dt>", render_inlines(&item.term, ctx)));
        for details in &item.details {
            parts.push(render_description_details(&details.children, dl.tight, ctx));
        }
    }
    format!("<dl>\n{}\n</dl>", parts.join("\n"))
}

/// `<dd>`: tight unwraps a sole paragraph child to bare inline; loose keeps
/// full block rendering. Mirrors the list tightness rule.
fn render_description_details(children: &[Block], tight: bool, ctx: &Ctx) -> String {
    if tight {
        let mut parts: Vec<String> = Vec::new();
        for child in children {
            let rendered = match child {
                Block::Paragraph(p) => render_inlines(&p.children, ctx),
                other => render_block(other, ctx),
            };
            if !rendered.is_empty() {
                parts.push(rendered);
            }
        }
        format!("<dd>{}</dd>", parts.join("\n"))
    } else {
        let inner = render_blocks_joined(children, ctx);
        if inner.is_empty() {
            String::from("<dd>\n</dd>")
        } else {
            format!("<dd>\n{inner}\n</dd>")
        }
    }
}

/// `code_body`: text-escape the value and add the implicit final newline. The
/// AST convention is `value = content lines joined by '\n', no trailing
/// newline`, so the block's terminating newline is always implicit and must be
/// re-added here. A trailing blank content line therefore appears in the value
/// as a trailing `\n` and still gets its own newline (e.g. `b\n\n` → `b\n\n\n`).
/// An empty value stays empty (zero content lines).
fn code_body(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        let mut s = escape_text(value);
        s.push('\n');
        s
    }
}

/// First whitespace-delimited token of the info string, text-escaped; `None`
/// when info is absent or empty.
fn language_class(info: Option<&str>) -> Option<String> {
    let info = info?;
    let token = info.split(|c: char| c.is_ascii_whitespace()).next()?;
    if token.is_empty() {
        None
    } else {
        Some(escape_text(token))
    }
}

/// Raw (un-escaped) first whitespace-delimited token of the info string.
fn info_first_token(info: Option<&str>) -> Option<&str> {
    let token = info?.split(|c: char| c.is_ascii_whitespace()).next()?;
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn render_code_block(cb: &CodeBlock) -> String {
    let body = code_body(&cb.value);
    match language_class(cb.info.as_deref()) {
        Some(lang) => {
            // GFM math: a ```math fence carries the display marker.
            let math_attr = if info_first_token(cb.info.as_deref()) == Some("math") {
                " data-math-style=\"display\""
            } else {
                ""
            };
            format!("<pre><code class=\"language-{lang}\"{math_attr}>{body}</code></pre>")
        }
        None => format!("<pre><code>{body}</code></pre>"),
    }
}

fn render_math_block(mb: &MathBlock, _ctx: &Ctx) -> String {
    let body = code_body(&mb.value);
    format!("<pre><code class=\"language-math\" data-math-style=\"display\">{body}</code></pre>")
}

fn render_html_block(value: &str, ctx: &Ctx) -> String {
    if ctx.allow_dangerous_html {
        if ctx.gfm_tagfilter {
            return apply_tagfilter(value);
        }
        return String::from(value);
    }
    // Safe mode: GFM emits the `<!-- raw HTML omitted -->` placeholder;
    // CommonMark text-escapes the raw block.
    safe_raw_html(value, ctx)
}

/// LeafDirective [CONV]: a self-describing classed `<div>` carrying the name,
/// data-* attributes, and the inline label.
fn render_leaf_directive(d: &LeafDirective, ctx: &Ctx) -> String {
    let attrs = directive_attrs(&d.attributes);
    format!(
        "<div class=\"directive directive-leaf\" data-directive-name=\"{}\"{attrs}>{}</div>",
        attr_escape(&d.name),
        render_inlines(&d.label, ctx)
    )
}

/// ContainerDirective: oracle-backed `<div class="name">…</div>` core plus the
/// convention data-* attribute and label additions.
fn render_container_directive(d: &ContainerDirective, ctx: &Ctx) -> String {
    let mut attrs = String::new();
    for attr in &d.attributes {
        let value = attr.value.as_deref().unwrap_or("");
        attrs.push_str(&format!(
            " data-{}=\"{}\"",
            attr_escape(&attr.name),
            attr_escape(value)
        ));
    }
    if !d.label.is_empty() {
        attrs.push_str(&format!(
            " data-directive-label=\"{}\"",
            attr_escape(&flatten_alt(&d.label))
        ));
    }
    let inner = render_blocks_joined(&d.children, ctx);
    let body = if inner.is_empty() {
        String::new()
    } else {
        format!("\n{inner}")
    };
    format!(
        "<div class=\"{}\"{attrs}>{body}\n</div>",
        attr_escape(&d.name)
    )
}

/// Shared directive data-* attribute serializer (LeafDirective / TextDirective
/// use the same ` data-{name}="{value}"` form).
fn directive_attrs(attributes: &[markdown_syntax::ast::DirectiveAttribute]) -> String {
    let mut out = String::new();
    for attr in attributes {
        let value = attr.value.as_deref().unwrap_or("");
        out.push_str(&format!(
            " data-{}=\"{}\"",
            attr_escape(&attr.name),
            attr_escape(value)
        ));
    }
    out
}
