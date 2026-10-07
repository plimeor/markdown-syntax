//! Link-definition map, reference resolution, and image-alt flattening.

use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::ast::{Block, Definition, HtmlContainerContent, Inline};

use super::escape::escape_text;

/// Resolved link-reference definitions keyed by the CommonMark-normalized
/// label. The first definition for a given normalized label wins.
pub struct DefMap {
    entries: BTreeMap<String, Definition>,
}

impl DefMap {
    /// Build the definition map by walking the whole document tree (defs may
    /// live inside block quotes, list items, etc.).
    pub fn build(blocks: &[Block]) -> Self {
        let mut entries = BTreeMap::new();
        collect_defs(blocks, &mut entries);
        Self { entries }
    }

    /// Resolve a reference identifier to its definition, applying CommonMark
    /// label normalization.
    pub fn resolve(&self, identifier: &str) -> Option<&Definition> {
        self.entries.get(&normalize_label(identifier))
    }
}

fn collect_defs(blocks: &[Block], out: &mut BTreeMap<String, Definition>) {
    for block in blocks {
        match block {
            Block::Definition(def) => {
                let key = normalize_label(&def.identifier);
                out.entry(key).or_insert_with(|| def.clone());
            }
            Block::BlockQuote(bq) => collect_defs(&bq.children, out),
            Block::Alert(alert) => collect_defs(&alert.children, out),
            Block::List(list) => {
                for item in &list.children {
                    collect_defs(&item.children, out);
                }
            }
            Block::FootnoteDefinition(fd) => collect_defs(&fd.children, out),
            Block::HtmlContainer(container) => {
                if let HtmlContainerContent::Blocks(children) = &container.content {
                    collect_defs(children, out);
                }
            }
            Block::ContainerDirective(dir) => collect_defs(&dir.children, out),
            _ => {}
        }
    }
}

/// CommonMark label normalization: trim, collapse internal whitespace runs to
/// a single space, then Unicode case-fold via uppercase→lowercase.
pub fn normalize_label(label: &str) -> String {
    let mut collapsed = String::with_capacity(label.len());
    let mut in_ws = false;
    let mut started = false;
    for ch in label.chars() {
        if ch.is_whitespace() {
            in_ws = true;
            continue;
        }
        if in_ws && started {
            collapsed.push(' ');
        }
        in_ws = false;
        started = true;
        collapsed.push(ch);
    }
    collapsed.to_uppercase().to_lowercase()
}

/// Plain-text reduction of an inline subtree, used for image alt text and the
/// directive/wikilink label-as-text contexts. Markup tags are stripped; only
/// textual content (and decoded char refs) survive. The result is NOT yet
/// HTML-escaped — the caller applies [`escape_text`].
pub fn flatten_alt(inlines: &[Inline]) -> String {
    let mut out = String::new();
    flatten_into(inlines, &mut out);
    out
}

fn flatten_into(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&t.value),
            Inline::Escape(e) => out.push(e.value),
            Inline::CharacterReference(c) => out.push_str(&c.value),
            Inline::Code(c) => out.push_str(&c.value),
            Inline::Math(m) => out.push_str(&m.value),
            Inline::Emphasis(n) => flatten_into(&n.children, out),
            Inline::Strong(n) => flatten_into(&n.children, out),
            Inline::Delete(n) => flatten_into(&n.children, out),
            Inline::Mark(n) => flatten_into(&n.children, out),
            Inline::Link(n) => flatten_into(&n.children, out),
            Inline::LinkReference(n) => flatten_into(&n.children, out),
            Inline::Image(n) => flatten_into(&n.alt, out),
            Inline::ImageReference(n) => flatten_into(&n.alt, out),
            Inline::InlineFootnote(n) => flatten_into(&n.children, out),
            Inline::SoftBreak(_) => out.push('\n'),
            Inline::LineBreak(_) => out.push('\n'),
            Inline::Html(h) => out.push_str(&h.value),
            Inline::WikiLink(w) => out.push_str(&w.decoded_label()),
            Inline::Shortcode(s) => match s.glyph() {
                Some(glyph) => out.push_str(glyph),
                None => {
                    out.push(':');
                    out.push_str(&s.name);
                    out.push(':');
                }
            },
            Inline::FootnoteReference(_) => {}
            Inline::TextDirective(d) => flatten_into(&d.label, out),
        }
    }
}

/// Escape the flattened alt text in one step.
pub fn escaped_alt(alt: &[Inline]) -> String {
    escape_text(&flatten_alt(alt))
}
