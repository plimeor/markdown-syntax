//! Footnote document pre-pass + section emission (GFM shape only).
//!
//! Before rendering the body, we walk the whole document to learn:
//!   * the first-reference order of footnote ids (regular `[^id]` and
//!     synthesized inline `__inline_{N}`), which gives each its 1-based number;
//!   * how many times each id is referenced (drives the backref count and the
//!     per-reference `id="fnref-{id}{-k}"` suffix);
//!   * each referenced id's definition body (regular def children, or the
//!     harvested children of an inline footnote);
//!   * which ids are actually referenced (unreferenced defs are dropped).
//!
//! The body renderer consumes this read-only context; only the references it
//! actually emits advance the per-id reference counter, so the suffixes match.

use core::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::format;
use std::string::String;
use std::vec::Vec;

use markdown_syntax::ast::{Block, Inline};

use super::escape::{attr_escape_gfm, encode_href};

/// Read-only footnote state shared across the body render.
pub struct FootnoteContext {
    /// Distinct ids in first-reference order; 1-based index = display number.
    order: Vec<String>,
    /// Id → display number (1-based).
    numbers: BTreeMap<String, usize>,
    /// Id → footnote body blocks (def children or harvested inline children).
    defs: BTreeMap<String, Vec<Block>>,
    /// Id → total number of references (for backref anchors).
    ref_totals: BTreeMap<String, usize>,
    /// Ids actually referenced (only these are emitted).
    referenced: BTreeSet<String>,
    /// Folded id → first definition's ORIGINAL-case label, used for the
    /// displayed `fn-`/`fnref-` ids (matching still uses the folded id).
    display_labels: BTreeMap<String, String>,
    /// Running per-id reference counter consumed during the body render,
    /// interior-mutable so the read-only `&Ctx` thread can advance it.
    seen_during_render: BTreeMap<String, Cell<usize>>,
    /// Running inline-footnote counter consumed during the body render so the
    /// N-th `^[..]` maps to the same `__inline_{N}` minted in the pre-pass.
    inline_emitted: Cell<usize>,
}

impl FootnoteContext {
    /// True when at least one footnote was referenced (gates section output).
    pub fn has_any(&self) -> bool {
        !self.referenced.is_empty()
    }

    /// 1-based display number for an id (0 if unknown, which never happens for
    /// a referenced id).
    pub fn number(&self, id: &str) -> usize {
        self.numbers.get(id).copied().unwrap_or(0)
    }

    /// True when `id` resolves to a definition (regular or harvested inline).
    /// Undefined references are never registered, so this is `numbers`-backed.
    pub fn is_defined(&self, id: &str) -> bool {
        self.numbers.contains_key(id)
    }

    /// The DISPLAY id for `id`: the first definition's original-case label when
    /// known, else the id itself (covers `__inline_{N}`).
    fn display_id<'a>(&'a self, id: &'a str) -> &'a str {
        self.display_labels
            .get(id)
            .map(String::as_str)
            .unwrap_or(id)
    }
}

/// Pre-pass: build the footnote context from the whole document.
pub fn build(blocks: &[Block]) -> FootnoteContext {
    let mut order: Vec<String> = Vec::new();
    let mut numbers: BTreeMap<String, usize> = BTreeMap::new();
    let mut ref_totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    let mut defs: BTreeMap<String, Vec<Block>> = BTreeMap::new();
    let mut display_labels: BTreeMap<String, String> = BTreeMap::new();

    // First collect all regular definitions (anywhere in the tree).
    collect_defs(blocks, &mut defs, &mut display_labels);

    // Then walk in document order to learn reference order + inline bodies.
    let mut inline_counter = 0usize;
    walk_refs(
        blocks,
        &mut order,
        &mut numbers,
        &mut ref_totals,
        &mut referenced,
        &mut defs,
        &mut inline_counter,
    );

    let seen_during_render = order.iter().map(|id| (id.clone(), Cell::new(0))).collect();

    FootnoteContext {
        order,
        numbers,
        defs,
        ref_totals,
        referenced,
        display_labels,
        seen_during_render,
        inline_emitted: Cell::new(0),
    }
}

fn collect_defs(
    blocks: &[Block],
    defs: &mut BTreeMap<String, Vec<Block>>,
    display_labels: &mut BTreeMap<String, String>,
) {
    for block in blocks {
        match block {
            Block::FootnoteDefinition(fd) => {
                defs.entry(fd.identifier.clone())
                    .or_insert_with(|| fd.children.clone());
                // First definition for a folded id wins the displayed label.
                display_labels
                    .entry(fd.identifier.clone())
                    .or_insert_with(|| fd.label.clone());
            }
            Block::BlockQuote(bq) => collect_defs(&bq.children, defs, display_labels),
            Block::Alert(a) => collect_defs(&a.children, defs, display_labels),
            Block::List(list) => {
                for item in &list.children {
                    collect_defs(&item.children, defs, display_labels);
                }
            }
            Block::DescriptionList(dl) => {
                for item in &dl.children {
                    for details in &item.details {
                        collect_defs(&details.children, defs, display_labels);
                    }
                }
            }
            Block::ContainerDirective(dir) => collect_defs(&dir.children, defs, display_labels),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_refs(
    blocks: &[Block],
    order: &mut Vec<String>,
    numbers: &mut BTreeMap<String, usize>,
    ref_totals: &mut BTreeMap<String, usize>,
    referenced: &mut BTreeSet<String>,
    defs: &mut BTreeMap<String, Vec<Block>>,
    inline_counter: &mut usize,
) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => walk_inline_refs(
                &p.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Block::Heading(h) => walk_inline_refs(
                &h.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Block::BlockQuote(bq) => walk_refs(
                &bq.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Block::Alert(a) => walk_refs(
                &a.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Block::List(list) => {
                for item in &list.children {
                    walk_refs(
                        &item.children,
                        order,
                        numbers,
                        ref_totals,
                        referenced,
                        defs,
                        inline_counter,
                    );
                }
            }
            Block::DescriptionList(dl) => {
                for item in &dl.children {
                    walk_inline_refs(
                        &item.term,
                        order,
                        numbers,
                        ref_totals,
                        referenced,
                        defs,
                        inline_counter,
                    );
                    for details in &item.details {
                        walk_refs(
                            &details.children,
                            order,
                            numbers,
                            ref_totals,
                            referenced,
                            defs,
                            inline_counter,
                        );
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        walk_inline_refs(
                            &cell.children,
                            order,
                            numbers,
                            ref_totals,
                            referenced,
                            defs,
                            inline_counter,
                        );
                    }
                }
            }
            Block::FootnoteDefinition(fd) => walk_refs(
                &fd.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Block::ContainerDirective(dir) => {
                walk_inline_refs(
                    &dir.label,
                    order,
                    numbers,
                    ref_totals,
                    referenced,
                    defs,
                    inline_counter,
                );
                walk_refs(
                    &dir.children,
                    order,
                    numbers,
                    ref_totals,
                    referenced,
                    defs,
                    inline_counter,
                );
            }
            Block::LeafDirective(dir) => walk_inline_refs(
                &dir.label,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_inline_refs(
    inlines: &[Inline],
    order: &mut Vec<String>,
    numbers: &mut BTreeMap<String, usize>,
    ref_totals: &mut BTreeMap<String, usize>,
    referenced: &mut BTreeSet<String>,
    defs: &mut BTreeMap<String, Vec<Block>>,
    inline_counter: &mut usize,
) {
    for inline in inlines {
        match inline {
            Inline::FootnoteReference(fr) => {
                // Only register a reference that resolves to a definition; an
                // undefined `[^x]` is rendered as literal text, not numbered.
                if defs.contains_key(&fr.identifier) {
                    register_ref(&fr.identifier, order, numbers, ref_totals, referenced);
                }
            }
            Inline::InlineFootnote(node) => {
                *inline_counter += 1;
                let id = format!("__inline_{}", inline_counter);
                defs.insert(id.clone(), block_wrap_inline(&node.children));
                register_ref(&id, order, numbers, ref_totals, referenced);
                // Inline-footnote children may themselves contain footnotes.
                walk_inline_refs(
                    &node.children,
                    order,
                    numbers,
                    ref_totals,
                    referenced,
                    defs,
                    inline_counter,
                );
            }
            Inline::Emphasis(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Strong(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Underline(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Delete(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Insert(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Mark(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Subscript(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Superscript(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Spoiler(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::Link(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::LinkReference(n) => recurse_inline(
                &n.children,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            Inline::TextDirective(d) => recurse_inline(
                &d.label,
                order,
                numbers,
                ref_totals,
                referenced,
                defs,
                inline_counter,
            ),
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn recurse_inline(
    inlines: &[Inline],
    order: &mut Vec<String>,
    numbers: &mut BTreeMap<String, usize>,
    ref_totals: &mut BTreeMap<String, usize>,
    referenced: &mut BTreeSet<String>,
    defs: &mut BTreeMap<String, Vec<Block>>,
    inline_counter: &mut usize,
) {
    walk_inline_refs(
        inlines,
        order,
        numbers,
        ref_totals,
        referenced,
        defs,
        inline_counter,
    );
}

fn register_ref(
    id: &str,
    order: &mut Vec<String>,
    numbers: &mut BTreeMap<String, usize>,
    ref_totals: &mut BTreeMap<String, usize>,
    referenced: &mut BTreeSet<String>,
) {
    if !numbers.contains_key(id) {
        order.push(String::from(id));
        numbers.insert(String::from(id), order.len());
    }
    *ref_totals.entry(String::from(id)).or_insert(0) += 1;
    referenced.insert(String::from(id));
}

fn block_wrap_inline(children: &[Inline]) -> Vec<Block> {
    use markdown_syntax::ast::{NodeMeta, Paragraph};
    std::vec![Block::Paragraph(Paragraph {
        meta: NodeMeta::default(),
        children: children.to_vec(),
    })]
}

/// Per-reference render hook: advance the running counter for `id` and return
/// the `(number, fnref_id_with_suffix)` pair for the `<sup>` marker.
pub fn reference_marker(ctx: &FootnoteContext, id: &str) -> (usize, String) {
    let number = ctx.number(id);
    let count = ctx
        .seen_during_render
        .get(id)
        .map(|c| {
            let v = c.get() + 1;
            c.set(v);
            v
        })
        .unwrap_or(1);
    let enc = footnote_id_encode(ctx.display_id(id));
    let fnref = if count == 1 {
        format!("fnref-{}", enc)
    } else {
        format!("fnref-{}-{}", enc, count)
    };
    (number, fnref)
}

/// The houdini+attr-encoded DISPLAY id for `id` (preserved-case label), used by
/// the `#fn-` href of a reference marker.
pub fn reference_fn_target(ctx: &FootnoteContext, id: &str) -> String {
    footnote_id_encode(ctx.display_id(id))
}

/// Resolve the id minted for the next inline footnote encountered during the
/// body render (matches the pre-pass `__inline_{N}` numbering).
pub fn next_inline_id(ctx: &FootnoteContext) -> String {
    let n = ctx.inline_emitted.get() + 1;
    ctx.inline_emitted.set(n);
    format!("__inline_{}", n)
}

/// GFM footnote id encoding: houdini href-encode, then attr-escape (so a
/// literal `'`→`&#x27;`). `__inline_{N}` is ASCII so this is a no-op there.
pub fn footnote_id_encode(id: &str) -> String {
    attr_escape_gfm(&encode_href(id))
}

/// Emit the document-end footnote section, or `""` when nothing referenced.
/// `render_blocks` renders a footnote body's blocks the same way the body
/// renderer renders any block list (joined with `\n`).
pub fn emit_footnote_section<F>(ctx: &FootnoteContext, render_blocks: F) -> String
where
    F: Fn(&[Block]) -> String,
{
    if !ctx.has_any() {
        return String::new();
    }
    let mut out = String::from("<section class=\"footnotes\" data-footnotes>\n<ol>\n");
    for id in &ctx.order {
        if !ctx.referenced.contains(id) {
            continue;
        }
        let number = ctx.number(id);
        let enc = footnote_id_encode(ctx.display_id(id));
        out.push_str(&format!("<li id=\"fn-{}\">\n", enc));

        let empty: Vec<Block> = Vec::new();
        let body = ctx.defs.get(id).unwrap_or(&empty);
        let rendered = render_blocks(body);
        let total = ctx.ref_totals.get(id).copied().unwrap_or(1);
        let backrefs = build_backrefs(&enc, number, total);

        out.push_str(&place_backrefs(&rendered, &backrefs));
        out.push('\n');
        out.push_str("</li>\n");
    }
    out.push_str("</ol>\n</section>");
    out
}

/// Build the space-joined backref anchors for a footnote referenced `total`
/// times.
fn build_backrefs(enc: &str, number: usize, total: usize) -> String {
    let mut anchors: Vec<String> = Vec::new();
    let total = total.max(1);
    for k in 1..=total {
        if k == 1 {
            anchors.push(format!(
                "<a href=\"#fnref-{enc}\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"{number}\" aria-label=\"Back to reference {number}\">\u{21a9}</a>",
            ));
        } else {
            anchors.push(format!(
                "<a href=\"#fnref-{enc}-{k}\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"{number}-{k}\" aria-label=\"Back to reference {number}-{k}\">\u{21a9}<sup class=\"footnote-ref\">{k}</sup></a>",
            ));
        }
    }
    anchors.join(" ")
}

/// Place the backref anchors: inside the last paragraph child when the body's
/// last block rendered as a `<p>…</p>` (append ` {backrefs}` before `</p>`);
/// otherwise on their own trailing line after the last block.
fn place_backrefs(rendered: &str, backrefs: &str) -> String {
    if backrefs.is_empty() {
        return String::from(rendered);
    }
    if rendered.is_empty() {
        return String::from(backrefs);
    }
    if rendered.ends_with("</p>") {
        let head = &rendered[..rendered.len() - "</p>".len()];
        return format!("{head} {backrefs}</p>");
    }
    format!("{rendered}\n{backrefs}")
}
