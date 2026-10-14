//! The leaves of a tree: paragraphs, headings, table cells, and the labels of
//! leaf and container directives, the blocks whose content is inline. Each
//! may keep the text it was built from (`with_source`).

use alloc::string::String;
use alloc::vec::Vec;

use crate::ast::*;
use crate::span::Span;

/// One leaf: its span (a label's is its directive's), its inlines, and the
/// text it keeps.
pub(crate) struct LeafMut<'a> {
    pub(crate) span: Option<Span>,
    pub(crate) inlines: &'a mut Vec<Inline>,
    pub(crate) source: &'a mut Option<String>,
}

/// Calls `visit` with the path and span of every leaf in `blocks`, in
/// document order, where `path` is the path of their container.
pub(crate) fn visit(
    blocks: &[Block],
    path: &mut Vec<usize>,
    visit_leaf: &mut impl FnMut(&[usize], Option<Span>),
) {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        match block {
            Block::Paragraph(node) => visit_leaf(path, node.meta.span),
            Block::Heading(node) => visit_leaf(path, node.meta.span),
            Block::LeafDirective(node) => visit_leaf(path, node.meta.span),
            Block::ContainerDirective(node) => {
                visit_leaf(path, node.meta.span);
                visit(&node.children, path, visit_leaf);
            }
            Block::Table(node) => {
                for (row, cells) in node.rows.iter().enumerate() {
                    for (cell, node) in cells.cells.iter().enumerate() {
                        path.extend([row, cell]);
                        visit_leaf(path, node.meta.span);
                        path.truncate(path.len() - 2);
                    }
                }
            }
            Block::BlockQuote(node) => visit(&node.children, path, visit_leaf),
            Block::Alert(node) => visit(&node.children, path, visit_leaf),
            Block::FootnoteDefinition(node) => visit(&node.children, path, visit_leaf),
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(children) = &node.content {
                    visit(children, path, visit_leaf);
                }
            }
            Block::List(node) => {
                for (item, node) in node.children.iter().enumerate() {
                    path.push(item);
                    visit(&node.children, path, visit_leaf);
                    path.pop();
                }
            }
            _ => {}
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
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(children) = &mut node.content {
                    visit_mut(children, visit_leaf);
                }
            }
            Block::List(node) => {
                for item in &mut node.children {
                    visit_mut(&mut item.children, visit_leaf);
                }
            }
            _ => {}
        }
    }
}

/// Whether a leaf in `blocks` keeps the text it was built from.
pub(crate) fn any_kept(blocks: &[Block]) -> bool {
    blocks.iter().any(|block| match block {
        Block::Paragraph(node) => node.source.is_some(),
        Block::Heading(node) => node.source.is_some(),
        Block::LeafDirective(node) => node.label_source.is_some(),
        Block::ContainerDirective(node) => node.label_source.is_some() || any_kept(&node.children),
        Block::Table(node) => node
            .rows
            .iter()
            .flat_map(|row| &row.cells)
            .any(|cell| cell.source.is_some()),
        Block::BlockQuote(node) => any_kept(&node.children),
        Block::Alert(node) => any_kept(&node.children),
        Block::FootnoteDefinition(node) => any_kept(&node.children),
        Block::HtmlContainer(node) => match &node.content {
            HtmlContainerContent::Blocks(children) => any_kept(children),
            HtmlContainerContent::Inlines(_) => false,
        },
        Block::List(node) => node.children.iter().any(|item| any_kept(&item.children)),
        _ => false,
    })
}
