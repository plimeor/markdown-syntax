//! Link-definition map, reference resolution, and image-alt flattening.

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;
use alloc::string::String;

use crate::ast::{Block, Definition, HtmlContainerContent, Inline};

use super::escape::escape_text;

/// Resolved link-reference definitions keyed by the CommonMark-normalized
/// label. The first definition for a given normalized label wins. A
/// definition whose identifier is already normalized is keyed by it.
pub struct DefMap<'a> {
    entries: BTreeMap<Cow<'a, str>, &'a Definition>,
}

impl<'a> DefMap<'a> {
    /// Build the definition map by walking the whole document tree (defs may
    /// live inside block quotes, list items, etc.).
    pub fn build(blocks: &'a [Block]) -> Self {
        let mut entries = BTreeMap::new();
        collect_defs(blocks, &mut entries);
        Self { entries }
    }

    /// Resolve a reference identifier to its definition, applying CommonMark
    /// label normalization. An identifier that is already its own normalized
    /// form is looked up as is.
    pub fn resolve(&self, identifier: &str) -> Option<&'a Definition> {
        if is_normalized_ascii(identifier) {
            self.entries.get(identifier).copied()
        } else {
            self.entries
                .get(normalize_label(identifier).as_str())
                .copied()
        }
    }
}

fn collect_defs<'a>(blocks: &'a [Block], out: &mut BTreeMap<Cow<'a, str>, &'a Definition>) {
    for block in blocks {
        match block {
            Block::Definition(def) => {
                let key = if is_normalized_ascii(&def.identifier) {
                    Cow::Borrowed(def.identifier.as_str())
                } else {
                    Cow::Owned(normalize_label(&def.identifier))
                };
                out.entry(key).or_insert(def);
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
/// a single space, then Unicode case-fold via uppercase→lowercase. On an ASCII
/// label that fold is ASCII lowercasing.
pub fn normalize_label(label: &str) -> String {
    let mut collapsed = collapse_whitespace(label);
    if label.is_ascii() {
        collapsed.make_ascii_lowercase();
        collapsed
    } else {
        casefold(&collapsed)
    }
}

/// `label` trimmed, with each internal whitespace run collapsed to a single
/// space.
fn collapse_whitespace(label: &str) -> String {
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
    collapsed
}

/// The Unicode case-fold label matching uses: uppercase, then lowercase.
fn casefold(text: &str) -> String {
    text.to_uppercase().to_lowercase()
}

/// True when `label` is ASCII and [`normalize_label`] returns it unchanged:
/// no uppercase letter, no whitespace but single spaces between other chars.
fn is_normalized_ascii(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.first() == Some(&b' ') || bytes.last() == Some(&b' ') {
        return false;
    }
    let mut prev_space = false;
    for &b in bytes {
        if !b.is_ascii() || b.is_ascii_uppercase() {
            return false;
        }
        if b == b' ' {
            if prev_space {
                return false;
            }
            prev_space = true;
        } else if (b as char).is_whitespace() {
            return false;
        } else {
            prev_space = false;
        }
    }
    true
}

/// Append the HTML-escaped plain-text reduction of an inline subtree, used for
/// image alt text and the directive label-as-text context. Markup tags are
/// stripped; only textual content (and decoded char refs) survive, each piece
/// escaped with [`escape_text`] as it is appended.
pub fn escaped_alt(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => escape_text(out, &t.value),
            Inline::Escape(e) => {
                let mut buf = [0u8; 4];
                escape_text(out, e.value.encode_utf8(&mut buf));
            }
            Inline::CharacterReference(c) => escape_text(out, &c.value().unwrap_or_default()),
            Inline::Code(c) => escape_text(out, &c.value),
            Inline::Math(m) => escape_text(out, &m.value),
            Inline::Emphasis(n) => escaped_alt(out, &n.children),
            Inline::Strong(n) => escaped_alt(out, &n.children),
            Inline::Delete(n) => escaped_alt(out, &n.children),
            Inline::Mark(n) => escaped_alt(out, &n.children),
            Inline::Link(n) => escaped_alt(out, &n.children),
            Inline::Autolink(n) => escape_text(out, &n.text),
            Inline::LinkReference(n) => escaped_alt(out, &n.children),
            Inline::Image(n) => escaped_alt(out, &n.alt),
            Inline::ImageReference(n) => escaped_alt(out, &n.alt),
            Inline::InlineFootnote(n) => escaped_alt(out, &n.children),
            Inline::SoftBreak(_) => out.push('\n'),
            Inline::LineBreak(_) => out.push('\n'),
            Inline::Html(h) => escape_text(out, &h.value),
            Inline::WikiLink(w) => escape_text(out, &w.decoded_label()),
            Inline::Shortcode(s) => match s.glyph() {
                Some(glyph) => escape_text(out, glyph),
                None => {
                    out.push(':');
                    escape_text(out, &s.name);
                    out.push(':');
                }
            },
            Inline::Tag(t) => {
                out.push('#');
                escape_text(out, &t.name);
            }
            Inline::FootnoteReference(_) => {}
            Inline::TextDirective(d) => escaped_alt(out, &d.label),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::String;

    use super::{casefold, collapse_whitespace, is_normalized_ascii, normalize_label};
    use crate::test_support::labels;

    /// What `normalize_label` defines: the Unicode case-fold of the collapsed
    /// label, whatever the label holds.
    fn normalized(label: &str) -> String {
        casefold(&collapse_whitespace(label))
    }

    #[test]
    fn ascii_labels_normalize_as_the_casefold_does() {
        for label in labels() {
            assert_eq!(normalize_label(&label), normalized(&label), "{label:?}");
        }
    }

    #[test]
    fn normalized_ascii_labels_are_the_ones_normalization_keeps() {
        for label in labels() {
            assert_eq!(
                is_normalized_ascii(&label),
                label.is_ascii() && normalized(&label) == label,
                "{label:?}"
            );
        }
    }
}
