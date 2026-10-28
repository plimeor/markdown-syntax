//! All 17 `Block` arms plus the nested `ListItem` helpers they dispatch to.
//!
//! Every renderer appends to one output buffer. A block that renders to
//! nothing is recognized by the buffer not growing, so a separator written
//! ahead of it is truncated away again.

use alloc::string::String;
use core::fmt::Write as _;

use crate::ast::{
    Alert, AlertKind, Block, BlockQuote, CodeBlock, ContainerDirective, Heading, HtmlContainer,
    HtmlContainerContent, Inline, LeafDirective, List, ListItem, MathBlock, Paragraph,
};

use super::escape::{attr_escape, escape_text};
use super::inlines::{directive_attrs, render_inlines, render_raw_html};
use super::refs::escaped_alt;
use super::tables::render_table;
use super::{Ctx, TasklistAttrOrder};

/// Render a block list into `out`, dropping empty results (Definition,
/// Frontmatter, source-position FootnoteDefinitions, output-less directives)
/// and joining survivors with a single `\n`. No leading/trailing newline.
pub fn render_blocks_joined(blocks: &[Block], ctx: &Ctx, out: &mut String) {
    let mut any = false;
    for block in blocks {
        let mark = out.len();
        if any {
            out.push('\n');
        }
        let start = out.len();
        render_block(block, ctx, out);
        if out.len() == start {
            out.truncate(mark);
        } else {
            any = true;
        }
    }
}

/// `render_blocks_joined` preceded by a `\n` when it renders anything.
fn render_blocks_after_newline(blocks: &[Block], ctx: &Ctx, out: &mut String) {
    let mark = out.len();
    out.push('\n');
    render_blocks_joined(blocks, ctx, out);
    if out.len() == mark + 1 {
        out.truncate(mark);
    }
}

/// Top-level `Block` dispatch — every one of the 17 arms is handled; there is
/// no catch-all.
pub fn render_block(block: &Block, ctx: &Ctx, out: &mut String) {
    match block {
        // 1. Paragraph (loose context; tight suppression is applied by the
        //    list helpers that call render_paragraph directly).
        Block::Paragraph(p) => render_paragraph(p, ctx, out),

        // 2. Heading.
        Block::Heading(h) => render_heading(h, ctx, out),

        // 3. ThematicBreak — all markers identical.
        Block::ThematicBreak(_) => out.push_str("<hr />"),

        // 4. BlockQuote.
        Block::BlockQuote(bq) => render_blockquote(bq, ctx, out),

        // 5. Alert (GFM alert extension).
        Block::Alert(a) => render_alert(a, ctx, out),

        // 6. List.
        Block::List(list) => render_list(list, ctx, out),

        // 7. CodeBlock.
        Block::CodeBlock(cb) => render_code_block(cb, out),

        // 8. HtmlBlock.
        Block::HtmlBlock(hb) => render_raw_html(&hb.value, ctx, out),

        // 9. HtmlContainer.
        Block::HtmlContainer(container) => render_html_container(container, ctx, out),

        // 10. Definition — emits nothing (feeds reference resolution).
        Block::Definition(_) => {}

        // 11. FootnoteDefinition — hoisted to the doc-end section; nothing here.
        Block::FootnoteDefinition(_) => {}

        // 12. Table (GFM).
        Block::Table(t) => render_table(t, ctx, out),

        // 13. MathBlock — GFM display wrapper.
        Block::MathBlock(mb) => render_math_block(mb, ctx, out),

        // 14. Frontmatter — no HTML.
        Block::Frontmatter(_) => {}

        // 15. LeafDirective [CONV].
        Block::LeafDirective(d) => render_leaf_directive(d, ctx, out),

        // 16. ContainerDirective.
        Block::ContainerDirective(d) => render_container_directive(d, ctx, out),

        // 17. BlankLine — no HTML.
        Block::BlankLine(_) => {}
    }
}

fn render_html_container(container: &HtmlContainer, ctx: &Ctx, out: &mut String) {
    render_raw_html(&container.opening.raw, ctx, out);
    match &container.content {
        HtmlContainerContent::Blocks(children) => {
            render_blocks_after_newline(children, ctx, out);
            out.push('\n');
        }
        HtmlContainerContent::Inlines(children) => render_inlines(children, ctx, out),
    }
    render_raw_html(&container.closing.raw, ctx, out);
}

fn render_paragraph(p: &Paragraph, ctx: &Ctx, out: &mut String) {
    render_paragraph_of(&p.children, ctx, out);
}

/// A paragraph of `inlines`, as a paragraph block or an inline footnote's
/// body renders.
pub(super) fn render_paragraph_of(inlines: &[Inline], ctx: &Ctx, out: &mut String) {
    out.push_str("<p>");
    render_inlines(inlines, ctx, out);
    out.push_str("</p>");
}

fn render_heading(h: &Heading, ctx: &Ctx, out: &mut String) {
    let depth = h.depth.clamp(1, 6);
    let digit = char::from(b'0' + depth);
    out.push_str("<h");
    out.push(digit);
    out.push('>');
    render_inlines(&h.children, ctx, out);
    out.push_str("</h");
    out.push(digit);
    out.push('>');
}

fn render_blockquote(bq: &BlockQuote, ctx: &Ctx, out: &mut String) {
    out.push_str("<blockquote>");
    render_blocks_after_newline(&bq.children, ctx, out);
    out.push_str("\n</blockquote>");
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

fn render_alert(a: &Alert, ctx: &Ctx, out: &mut String) {
    out.push_str("<div class=\"markdown-alert markdown-alert-");
    out.push_str(alert_class_suffix(a.kind));
    out.push_str("\">\n<p class=\"markdown-alert-title\">");
    match a.title.as_deref() {
        Some(t) => escape_text(out, t),
        None => out.push_str(alert_default_title(a.kind)),
    }
    out.push_str("</p>");
    render_blocks_after_newline(&a.children, ctx, out);
    out.push_str("\n</div>");
}

/// Ordered-list `start` attr: the first item's number, emitted unless it is
/// `Some(1)` or `None`.
fn ordered_start_attr(list: &List, out: &mut String) {
    match list.children.first().and_then(|item| item.number) {
        Some(n) if n != 1 => {
            // Writing to a `String` cannot fail.
            let _ = write!(out, " start=\"{n}\"");
        }
        _ => {}
    }
}

fn render_list(list: &List, ctx: &Ctx, out: &mut String) {
    if list.ordered {
        out.push_str("<ol");
        ordered_start_attr(list, out);
        out.push_str(">\n");
    } else {
        out.push_str("<ul>\n");
    }
    for (i, item) in list.children.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_list_item(item, list.tight, ctx, out);
    }
    out.push_str(if list.ordered { "\n</ol>" } else { "\n</ul>" });
}

fn render_list_item(item: &ListItem, tight: bool, ctx: &Ctx, out: &mut String) {
    let checkbox = item
        .checked
        .map(|checked| task_checkbox(checked, ctx.tasklist_checkable, ctx.tasklist_attr_order));

    if tight {
        render_tight_item(item, checkbox, ctx, out);
    } else {
        render_loose_item(item, checkbox, ctx, out);
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
///
/// The checkbox leads the first paragraph's content; with no paragraph it
/// leads the item's content as bare text.
fn render_tight_item(item: &ListItem, checkbox: Option<&'static str>, ctx: &Ctx, out: &mut String) {
    out.push_str("<li>");
    let mut checkbox = checkbox;
    // Whether anything was emitted yet, and whether the last emitted part is
    // a block rather than an unwrapped paragraph.
    let mut any = false;
    let mut ends_with_block = false;
    if let Some(cb) = take_unplaced_checkbox(item, &mut checkbox) {
        out.push_str(cb);
        any = true;
    }
    for child in &item.children {
        let mark = out.len();
        let is_paragraph = matches!(child, Block::Paragraph(_));
        // A part after another sits on its own line, and so does a leading
        // block: the `\n` before a block is written whether or not it is the
        // first part.
        if any || !is_paragraph {
            out.push('\n');
        }
        let content = out.len();
        match child {
            Block::Paragraph(p) => {
                if let Some(cb) = checkbox.take() {
                    out.push_str(cb);
                    out.push(' ');
                }
                render_inlines(&p.children, ctx, out);
            }
            other => render_block(other, ctx, out),
        }
        if out.len() == content {
            out.truncate(mark);
        } else {
            any = true;
            ends_with_block = !is_paragraph;
        }
    }
    if ends_with_block {
        out.push('\n');
    }
    out.push_str("</li>");
}

/// Loose `<li>`: every child rendered fully (paragraphs stay `<p>`), joined
/// with `\n`, on its own lines. The checkbox leads the first paragraph's
/// content; with no paragraph it is the first line.
fn render_loose_item(item: &ListItem, checkbox: Option<&'static str>, ctx: &Ctx, out: &mut String) {
    out.push_str("<li>\n");
    let mut checkbox = checkbox;
    let mut any = false;
    if let Some(cb) = take_unplaced_checkbox(item, &mut checkbox) {
        out.push_str(cb);
        any = true;
    }
    for child in &item.children {
        let mark = out.len();
        if any {
            out.push('\n');
        }
        let start = out.len();
        match child {
            Block::Paragraph(p) if checkbox.is_some() => {
                out.push_str("<p>");
                out.push_str(checkbox.take().unwrap());
                out.push(' ');
                render_inlines(&p.children, ctx, out);
                out.push_str("</p>");
            }
            other => render_block(other, ctx, out),
        }
        if out.len() == start {
            out.truncate(mark);
        } else {
            any = true;
        }
    }
    if any {
        out.push('\n');
    }
    out.push_str("</li>");
}

/// Take the checkbox when the item has no paragraph for it to lead.
fn take_unplaced_checkbox(
    item: &ListItem,
    checkbox: &mut Option<&'static str>,
) -> Option<&'static str> {
    let has_paragraph = || {
        item.children
            .iter()
            .any(|child| matches!(child, Block::Paragraph(_)))
    };
    if checkbox.is_some() && !has_paragraph() {
        checkbox.take()
    } else {
        None
    }
}

/// GFM task-list checkbox `<input>`; `checkable` (non-default) drops the
/// `disabled=""`. Always followed by one literal space before the content.
///
/// Attribute order is CATEGORY-DIVERGENT between the two suite layers: the gfm
/// suite emits `checked=""` before `disabled=""`; the commonmark suite emits
/// `disabled=""` before `checked=""`.
fn task_checkbox(checked: bool, checkable: bool, attr_order: TasklistAttrOrder) -> &'static str {
    match (checkable, checked, attr_order) {
        (true, true, _) => "<input type=\"checkbox\" checked=\"\" />",
        (true, false, _) => "<input type=\"checkbox\" />",
        (false, true, TasklistAttrOrder::CheckedFirst) => {
            "<input type=\"checkbox\" checked=\"\" disabled=\"\" />"
        }
        (false, true, TasklistAttrOrder::DisabledFirst) => {
            "<input type=\"checkbox\" disabled=\"\" checked=\"\" />"
        }
        (false, false, _) => "<input type=\"checkbox\" disabled=\"\" />",
    }
}

/// `code_body`: text-escape the value and add an implicit final line ending only
/// when the AST value does not already carry one.
fn code_body(value: &str, out: &mut String) {
    if !value.is_empty() {
        escape_text(out, value);
        if !value.ends_with('\n') && !value.ends_with('\r') {
            out.push_str(preferred_code_line_ending(value));
        }
    }
}

fn preferred_code_line_ending(value: &str) -> &str {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => return "\r\n",
            b'\r' => return "\r",
            b'\n' => return "\n",
            _ => cursor += 1,
        }
    }
    "\n"
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

fn render_code_block(cb: &CodeBlock, out: &mut String) {
    match info_first_token(cb.info.as_deref()) {
        Some(token) => {
            out.push_str("<pre><code class=\"language-");
            escape_text(out, token);
            out.push('"');
            // GFM math: a ```math fence carries the display marker.
            if token == "math" {
                out.push_str(" data-math-style=\"display\"");
            }
            out.push('>');
        }
        None => out.push_str("<pre><code>"),
    }
    code_body(&cb.value, out);
    out.push_str("</code></pre>");
}

fn render_math_block(mb: &MathBlock, _ctx: &Ctx, out: &mut String) {
    out.push_str("<pre><code class=\"language-math\" data-math-style=\"display\">");
    code_body(&mb.value, out);
    out.push_str("</code></pre>");
}

/// LeafDirective [CONV]: a self-describing classed `<div>` carrying the name,
/// data-* attributes, and the inline label.
fn render_leaf_directive(d: &LeafDirective, ctx: &Ctx, out: &mut String) {
    out.push_str("<div class=\"directive directive-leaf\" data-directive-name=\"");
    attr_escape(out, &d.name);
    out.push('"');
    directive_attrs(&d.attributes, out);
    out.push('>');
    render_inlines(&d.label, ctx, out);
    out.push_str("</div>");
}

/// ContainerDirective: oracle-backed `<div class="name">…</div>` core plus the
/// convention data-* attribute and label additions.
fn render_container_directive(d: &ContainerDirective, ctx: &Ctx, out: &mut String) {
    out.push_str("<div class=\"");
    attr_escape(out, &d.name);
    out.push('"');
    directive_attrs(&d.attributes, out);
    if !d.label.is_empty() {
        out.push_str(" data-directive-label=\"");
        escaped_alt(out, &d.label);
        out.push('"');
    }
    out.push('>');
    render_blocks_after_newline(&d.children, ctx, out);
    out.push_str("\n</div>");
}
