//! The test suite's copy of the serializer's tree comparison, which
//! `src/compare.rs` owns and the tests cannot reach: two trees are the same
//! when they differ only in spans, in text one holds as an `Escape` or a
//! `CharacterReference` and the other as plain text, and in where adjacent
//! `Text` nodes split. It mirrors that module's strict form, without the
//! layout normalization the serializer applies to what it chooses itself.

use markdown_syntax::*;

/// `document` normalized as [`normalized`] normalizes its blocks.
#[allow(dead_code)]
pub(crate) fn normalized_document(document: &Document) -> Document {
    Document {
        children: normalized(&document.children),
        ..document.clone()
    }
}

/// `blocks` with spans cleared, escapes and character references read as
/// text, and adjacent text merged.
pub(crate) fn normalized(blocks: &[Block]) -> Vec<Block> {
    let mut blocks = blocks.to_vec();
    normalize_blocks(&mut blocks);
    blocks
}

fn clear(meta: &mut NodeMeta) {
    meta.span = None;
}

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
