//! The leaves of a tree: paragraphs, headings, table cells, and the labels of
//! leaf and container directives, the blocks whose content is inline. Each
//! may keep the text it was built from (`with_source`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;
use crate::parse::Definitions;
use crate::span::Span;

/// One leaf: its span (a label's is its directive's), its inlines, and the
/// text it keeps.
pub(crate) struct LeafMut<'a> {
    pub(crate) span: Option<Span>,
    pub(crate) inlines: &'a mut Vec<Inline>,
    pub(crate) source: &'a mut Option<String>,
}

/// Calls `visit_leaf` with the path, span, and kept text of every leaf in
/// `blocks`, in document order, where `path` is the path of their container.
pub(crate) fn visit(
    blocks: &[Block],
    path: &mut Vec<usize>,
    visit_leaf: &mut impl FnMut(&[usize], Option<Span>, Option<&str>),
) {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        match block {
            Block::Paragraph(node) => visit_leaf(path, node.meta.span, node.source.as_deref()),
            Block::Heading(node) => visit_leaf(path, node.meta.span, node.source.as_deref()),
            Block::LeafDirective(node) => {
                visit_leaf(path, node.meta.span, node.label_source.as_deref())
            }
            Block::ContainerDirective(node) => {
                visit_leaf(path, node.meta.span, node.label_source.as_deref());
                visit(&node.children, path, visit_leaf);
            }
            Block::Table(node) => {
                for (row, cells) in node.rows.iter().enumerate() {
                    for (cell, node) in cells.cells.iter().enumerate() {
                        path.extend([row, cell]);
                        visit_leaf(path, node.meta.span, node.source.as_deref());
                        path.truncate(path.len() - 2);
                    }
                }
            }
            Block::BlockQuote(node) => visit(&node.children, path, visit_leaf),
            Block::Alert(node) => visit(&node.children, path, visit_leaf),
            Block::FootnoteDefinition(node) => visit(&node.children, path, visit_leaf),
            Block::HtmlContainer(node) => match &node.content {
                HtmlContainerContent::Blocks(children) => visit(children, path, visit_leaf),
                HtmlContainerContent::Inlines(_) => {}
            },
            Block::List(node) => {
                for (item, node) in node.children.iter().enumerate() {
                    path.push(item);
                    visit(&node.children, path, visit_leaf);
                    path.pop();
                }
            }
            Block::ThematicBreak(_)
            | Block::CodeBlock(_)
            | Block::HtmlBlock(_)
            | Block::Definition(_)
            | Block::MathBlock(_)
            | Block::Frontmatter(_)
            | Block::BlankLine(_) => {}
        }
        path.pop();
    }
}

/// Calls `visit_leaf` with every leaf in `blocks`, in document order.
pub(crate) fn visit_mut(blocks: &mut [Block], visit_leaf: &mut impl FnMut(LeafMut<'_>)) {
    for block in blocks {
        match block {
            Block::Paragraph(node) => visit_leaf(LeafMut {
                span: node.meta.span,
                inlines: &mut node.children,
                source: &mut node.source,
            }),
            Block::Heading(node) => visit_leaf(LeafMut {
                span: node.meta.span,
                inlines: &mut node.children,
                source: &mut node.source,
            }),
            Block::LeafDirective(node) => visit_leaf(LeafMut {
                span: node.meta.span,
                inlines: &mut node.label,
                source: &mut node.label_source,
            }),
            Block::ContainerDirective(node) => {
                visit_leaf(LeafMut {
                    span: node.meta.span,
                    inlines: &mut node.label,
                    source: &mut node.label_source,
                });
                visit_mut(&mut node.children, visit_leaf);
            }
            Block::Table(node) => {
                for cell in node.rows.iter_mut().flat_map(|row| &mut row.cells) {
                    visit_leaf(LeafMut {
                        span: cell.meta.span,
                        inlines: &mut cell.children,
                        source: &mut cell.source,
                    });
                }
            }
            Block::BlockQuote(node) => visit_mut(&mut node.children, visit_leaf),
            Block::Alert(node) => visit_mut(&mut node.children, visit_leaf),
            Block::FootnoteDefinition(node) => visit_mut(&mut node.children, visit_leaf),
            Block::HtmlContainer(node) => match &mut node.content {
                HtmlContainerContent::Blocks(children) => visit_mut(children, visit_leaf),
                HtmlContainerContent::Inlines(_) => {}
            },
            Block::List(node) => {
                for item in &mut node.children {
                    visit_mut(&mut item.children, visit_leaf);
                }
            }
            Block::ThematicBreak(_)
            | Block::CodeBlock(_)
            | Block::HtmlBlock(_)
            | Block::Definition(_)
            | Block::MathBlock(_)
            | Block::Frontmatter(_)
            | Block::BlankLine(_) => {}
        }
    }
}

/// Whether a leaf in `blocks` keeps the text it was built from.
pub(crate) fn any_kept(blocks: &[Block]) -> bool {
    let mut kept = false;
    visit(blocks, &mut Vec::new(), &mut |_, _, source| {
        kept |= source.is_some()
    });
    kept
}

impl Document {
    /// Builds again, with this document's definitions, every leaf that keeps
    /// the text it was built from (`with_source`), as `with_source` builds it,
    /// keeping the leaf's span and its other fields. A leaf built with other
    /// definitions than the document's, or kept after the document's
    /// definitions changed, holds inlines its text no longer reads as, and
    /// the writer then writes it from its inlines; this makes it read as its
    /// text again.
    pub fn reread_kept_sources(&mut self) {
        let definitions = Definitions::of(self);
        visit_mut(&mut self.children, &mut |leaf| {
            if let Some(text) = leaf.source.as_deref() {
                *leaf.inlines = crate::parse::parse_inline_tree(text, &definitions).inlines;
            }
        });
    }
}
