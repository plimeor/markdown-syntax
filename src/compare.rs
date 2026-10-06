//! The tree comparison serialization reads back with. Two trees are the same
//! when they differ only in spans, in text one holds as an `Escape` or a
//! `CharacterReference` and the other as plain text, and in where adjacent
//! `Text` nodes split: the parser cannot tell an author's escape from one the
//! serializer writes, so an escaped text char always reads back as an
//! `Escape`. The matches are exhaustive so a new node kind cannot skip the
//! normalization.

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;

/// `inlines` normalized for comparison: spans cleared, escapes and character
/// references read as text, and adjacent text merged.
pub(crate) fn normalized_inlines(inlines: &[Inline]) -> Vec<Inline> {
    let mut inlines = inlines.to_vec();
    normalize_inlines(&mut inlines);
    inlines
}

/// `blocks` normalized for comparison, as [`normalized_inlines`] does for
/// inline content.
#[cfg(test)]
pub(crate) fn normalized_blocks(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    normalize_blocks(&mut blocks);
    blocks
}

fn clear(meta: &mut NodeMeta) {
    meta.span = None;
}

#[cfg(test)]
fn normalize_blocks(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(node) => {
                clear(&mut node.meta);
                normalize_inlines(&mut node.children);
            }
            Block::Heading(node) => {
                clear(&mut node.meta);
                normalize_inlines(&mut node.children);
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
            Block::DescriptionList(node) => {
                clear(&mut node.meta);
                for item in &mut node.children {
                    clear(&mut item.meta);
                    normalize_inlines(&mut item.term);
                    for details in &mut item.details {
                        clear(&mut details.meta);
                        normalize_blocks(&mut details.children);
                    }
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
                    HtmlContainerContent::Inlines(children) => normalize_inlines(children),
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
                        normalize_inlines(&mut cell.children);
                    }
                }
            }
            Block::MathBlock(node) => clear(&mut node.meta),
            Block::Frontmatter(node) => clear(&mut node.meta),
            Block::MdxEsm(node) => clear(&mut node.meta),
            Block::MdxExpression(node) => clear(&mut node.meta),
            Block::MdxJsx(node) => clear(&mut node.meta),
            Block::LeafDirective(node) => {
                clear(&mut node.meta);
                normalize_inlines(&mut node.label);
            }
            Block::ContainerDirective(node) => {
                clear(&mut node.meta);
                normalize_inlines(&mut node.label);
                normalize_blocks(&mut node.children);
            }
        }
    }
}

fn normalize_inlines(inlines: &mut Vec<Inline>) {
    let mut normalized = Vec::with_capacity(inlines.len());
    for mut inline in inlines.drain(..) {
        let text = match &mut inline {
            Inline::Text(node) => Some(core::mem::take(&mut node.value)),
            Inline::Escape(node) => Some(String::from(node.value)),
            Inline::CharacterReference(node) => Some(core::mem::take(&mut node.value)),
            Inline::SoftBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::LineBreak(node) => {
                clear(&mut node.meta);
                None
            }
            Inline::Emphasis(node) => children(&mut node.meta, &mut node.children),
            Inline::Strong(node) => children(&mut node.meta, &mut node.children),
            Inline::Underline(node) => children(&mut node.meta, &mut node.children),
            Inline::Delete(node) => children(&mut node.meta, &mut node.children),
            Inline::Insert(node) => children(&mut node.meta, &mut node.children),
            Inline::Mark(node) => children(&mut node.meta, &mut node.children),
            Inline::Subscript(node) => children(&mut node.meta, &mut node.children),
            Inline::Superscript(node) => children(&mut node.meta, &mut node.children),
            Inline::Spoiler(node) => children(&mut node.meta, &mut node.children),
            Inline::InlineFootnote(node) => children(&mut node.meta, &mut node.children),
            Inline::Link(node) => children(&mut node.meta, &mut node.children),
            Inline::Image(node) => children(&mut node.meta, &mut node.alt),
            Inline::LinkReference(node) => children(&mut node.meta, &mut node.children),
            Inline::ImageReference(node) => children(&mut node.meta, &mut node.alt),
            Inline::TextDirective(node) => children(&mut node.meta, &mut node.label),
            Inline::Shortcode(node) => leaf(&mut node.meta),
            Inline::Code(node) => leaf(&mut node.meta),
            Inline::Html(node) => leaf(&mut node.meta),
            Inline::Math(node) => leaf(&mut node.meta),
            Inline::FootnoteReference(node) => leaf(&mut node.meta),
            Inline::WikiLink(node) => leaf(&mut node.meta),
            Inline::MdxExpression(node) => leaf(&mut node.meta),
            Inline::MdxJsx(node) => leaf(&mut node.meta),
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

fn children(meta: &mut NodeMeta, children: &mut Vec<Inline>) -> Option<String> {
    clear(meta);
    normalize_inlines(children);
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
            Inline::CharacterReference(CharacterReference {
                meta: NodeMeta::default(),
                reference: "#42;".into(),
                value: "*".into(),
            }),
        ];
        assert_eq!(normalized_inlines(&escaped), vec![text("*a*")]);
    }

    #[test]
    fn adjacent_text_merges_inside_containers() {
        let split = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::new(Some(crate::Span::new(0, 4))),
            children: vec![text("a"), text("b")],
        })];
        let whole = vec![Inline::Emphasis(Emphasis {
            meta: NodeMeta::default(),
            children: vec![text("ab")],
        })];
        assert_eq!(normalized_inlines(&split), whole);
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
