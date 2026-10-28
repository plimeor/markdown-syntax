//! The leaves of a tree: paragraphs, headings, table cells, and the labels of
//! leaf and container directives, the blocks whose content is inline. Each
//! may keep the text it was built from (`with_source`).

use alloc::string::String;
use alloc::vec::Vec;
use core::ops::ControlFlow;

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
    let _ = walk(blocks, path, &mut |path, leaf| {
        visit_leaf(path, leaf.span, leaf.source);
        ControlFlow::Continue(())
    });
}

/// The path of the first leaf in `blocks` that `matches` holds for, given
/// its kept text and its inlines; the leaves after it are not visited.
pub(crate) fn find(
    blocks: &[Block],
    matches: &mut impl FnMut(Option<&str>, &[Inline]) -> bool,
) -> Option<Vec<usize>> {
    let mut found = None;
    let _ = walk(blocks, &mut Vec::new(), &mut |path, leaf| {
        if matches(leaf.source, leaf.inlines) {
            found = Some(path.to_vec());
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    });
    found
}

/// One leaf as `walk` finds it.
struct Leaf<'a> {
    span: Option<Span>,
    inlines: &'a [Inline],
    source: Option<&'a str>,
}

impl<'a> Leaf<'a> {
    fn new(span: Option<Span>, inlines: &'a [Inline], source: &'a Option<String>) -> Self {
        Self {
            span,
            inlines,
            source: source.as_deref(),
        }
    }
}

/// Calls `visit_leaf` with the path of its container and every leaf in
/// `blocks`, in document order, until it breaks.
fn walk(
    blocks: &[Block],
    path: &mut Vec<usize>,
    visit_leaf: &mut impl FnMut(&[usize], Leaf<'_>) -> ControlFlow<()>,
) -> ControlFlow<()> {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        match block {
            Block::Paragraph(node) => visit_leaf(
                path,
                Leaf::new(node.meta.span, &node.children, &node.source),
            )?,
            Block::Heading(node) => visit_leaf(
                path,
                Leaf::new(node.meta.span, &node.children, &node.source),
            )?,
            Block::LeafDirective(node) => visit_leaf(
                path,
                Leaf::new(node.meta.span, &node.label, &node.label_source),
            )?,
            Block::ContainerDirective(node) => {
                visit_leaf(
                    path,
                    Leaf::new(node.meta.span, &node.label, &node.label_source),
                )?;
                walk(&node.children, path, visit_leaf)?;
            }
            Block::Table(node) => {
                for (row, cells) in node.rows.iter().enumerate() {
                    for (cell, node) in cells.cells.iter().enumerate() {
                        path.extend([row, cell]);
                        visit_leaf(
                            path,
                            Leaf::new(node.meta.span, &node.children, &node.source),
                        )?;
                        path.truncate(path.len() - 2);
                    }
                }
            }
            Block::BlockQuote(node) => walk(&node.children, path, visit_leaf)?,
            Block::Alert(node) => walk(&node.children, path, visit_leaf)?,
            Block::FootnoteDefinition(node) => walk(&node.children, path, visit_leaf)?,
            Block::HtmlContainer(node) => match &node.content {
                HtmlContainerContent::Blocks(children) => walk(children, path, visit_leaf)?,
                HtmlContainerContent::Inlines(_) => {}
            },
            Block::List(node) => {
                for (item, node) in node.children.iter().enumerate() {
                    path.push(item);
                    walk(&node.children, path, visit_leaf)?;
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
    ControlFlow::Continue(())
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
