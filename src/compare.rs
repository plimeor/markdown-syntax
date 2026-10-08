//! The tree comparison round-trip checks use. Two trees are the same when
//! they differ only in spans, in text one holds as an `Escape` or a
//! `CharacterReference` and the other as plain text, in a heading's soft
//! break that one holds where the other holds a space, and in where adjacent
//! `Text` nodes split. The matches are exhaustive so a new node kind cannot
//! skip the normalization.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;

/// `inlines` normalized for comparison: spans cleared, escapes and character
/// references read as text, and adjacent text merged.
#[cfg(test)]
pub(crate) fn normalized_inlines(inlines: &[Inline]) -> Vec<Inline> {
    let mut inlines = inlines.to_vec();
    normalize_inlines(&mut inlines, false);
    inlines
}

/// `blocks` normalized for comparison: spans cleared, escapes and character
/// references read as text, a heading's soft breaks read as spaces, and
/// adjacent text merged.
pub fn normalized_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    normalize_blocks(&mut blocks);
    blocks
}

fn clear(meta: &mut NodeMeta) {
    meta.span = None;
}

fn normalize_blocks(blocks: &mut [Block]) {
    let inlines = |inlines: &mut Vec<Inline>| normalize_inlines(inlines, false);
    for block in blocks {
        match block {
            Block::Paragraph(node) => {
                clear(&mut node.meta);
                inlines(&mut node.children);
            }
            Block::Heading(node) => {
                clear(&mut node.meta);
                normalize_inlines(&mut node.children, true);
            }
            Block::ThematicBreak(node) => clear(&mut node.meta),
            Block::BlockQuote(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children);
            }
            Block::Alert(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children);
            }
            Block::List(node) => {
                clear(&mut node.meta);
                for item in &mut node.children {
                    clear(&mut item.meta);
                    normalize_blocks(&mut item.children);
                }
            }
            Block::CodeBlock(node) => clear(&mut node.meta),
            Block::HtmlBlock(node) => clear(&mut node.meta),
            Block::HtmlContainer(node) => {
                clear(&mut node.meta);
                clear(&mut node.opening.meta);
                clear(&mut node.closing.meta);
                match &mut node.content {
                    HtmlContainerContent::Blocks(children) => normalize_blocks(children),
                    HtmlContainerContent::Inlines(children) => inlines(children),
                }
            }
            Block::Definition(node) => clear(&mut node.meta),
            Block::FootnoteDefinition(node) => {
                clear(&mut node.meta);
                normalize_blocks(&mut node.children);
            }
            Block::Table(node) => {
                clear(&mut node.meta);
                for row in &mut node.rows {
                    clear(&mut row.meta);
                    for cell in &mut row.cells {
                        clear(&mut cell.meta);
                        inlines(&mut cell.children);
                    }
                }
            }
            Block::MathBlock(node) => clear(&mut node.meta),
            Block::Frontmatter(node) => clear(&mut node.meta),
            Block::LeafDirective(node) => {
                clear(&mut node.meta);
                inlines(&mut node.label);
            }
            Block::ContainerDirective(node) => {
                clear(&mut node.meta);
                inlines(&mut node.label);
                normalize_blocks(&mut node.children);
            }
        }
    }
}

/// Normalizes `inlines`; in a heading, `heading` reads a soft break as a
/// space.
fn normalize_inlines(inlines: &mut Vec<Inline>, heading: bool) {
    let mut normalized = Vec::with_capacity(inlines.len());
    for mut inline in inlines.drain(..) {
        let text = match &mut inline {
            Inline::Text(node) => Some(core::mem::take(&mut node.value)),
            Inline::Escape(node) => Some(String::from(node.value)),
            Inline::CharacterReference(node) => Some(
                node.value()
                    .unwrap_or_else(|| core::mem::take(&mut node.reference)),
            ),
            Inline::SoftBreak(_) if heading => Some(String::from(" ")),
            Inline::SoftBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::LineBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::Emphasis(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::Strong(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::Delete(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::Mark(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::InlineFootnote(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::Link(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::Image(node) => children(&mut node.meta, &mut node.alt, heading),
            Inline::LinkReference(node) => children(&mut node.meta, &mut node.children, heading),
            Inline::ImageReference(node) => children(&mut node.meta, &mut node.alt, heading),
            Inline::TextDirective(node) => children(&mut node.meta, &mut node.label, heading),
            Inline::Shortcode(node) => leaf(&mut node.meta),
            Inline::Code(node) => leaf(&mut node.meta),
            Inline::Autolink(node) => leaf(&mut node.meta),
            Inline::Html(node) => leaf(&mut node.meta),
            Inline::Math(node) => leaf(&mut node.meta),
            Inline::FootnoteReference(node) => leaf(&mut node.meta),
            Inline::WikiLink(node) => leaf(&mut node.meta),
        };
        match text {
            Some(value) => match normalized.last_mut() {
                Some(Inline::Text(last)) => last.value.push_str(&value),
                _ => normalized.push(Inline::Text(Text {
                    meta: NodeMeta::default(),
                    value,
                })),
            },
            None => normalized.push(inline),
        }
    }
    *inlines = normalized;
}

fn children(meta: &mut NodeMeta, children: &mut Vec<Inline>, heading: bool) -> Option<String> {
    clear(meta);
    normalize_inlines(children, heading);
    None
}

fn leaf(meta: &mut NodeMeta) -> Option<String> {
    clear(meta);
    None
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    fn text(value: &str) -> Inline {
        Text::from(value).into()
    }

    #[test]
    fn escapes_and_references_read_as_text() {
        let escaped = vec![
            Inline::Escape(Escape {
                meta: NodeMeta::new(Some(crate::Span::new(0, 2))),
                value: '*',
            }),
            text("a"),
            Inline::CharacterReference(CharacterReference::new("&#42;")),
        ];
        assert_eq!(normalized_inlines(&escaped), vec![text("*a*")]);
    }

    #[test]
    fn adjacent_text_merges_inside_containers() {
        let split = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::new(Some(crate::Span::new(0, 4))),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![text("a"), text("b")],
        })];
        let whole = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            delimiter: EmphasisDelimiter::Asterisk,
            children: vec![text("ab")],
        })];
        assert_eq!(normalized_inlines(&split), whole);
    }

    #[test]
    fn heading_soft_breaks_read_as_spaces() {
        let parsed = crate::parse("a\nb\n===").document.children;
        let mut heading = Heading::new(1, [Text::from("a b")]);
        heading.kind = HeadingKind::Setext;
        assert_eq!(
            normalized_blocks(&parsed),
            normalized_blocks(&[heading.into()])
        );
    }

    #[test]
    fn block_spans_are_cleared() {
        let parsed = crate::parse("> a\\*b").document.children;
        let built: Block = BlockQuote {
            meta: NodeMeta::default(),
            children: vec![Paragraph::new([Text::from("a*b")]).into()],
        }
        .into();
        assert_eq!(normalized_blocks(&parsed), normalized_blocks(&[built]));
    }
}
