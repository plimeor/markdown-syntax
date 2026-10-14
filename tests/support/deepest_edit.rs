//! An edit at the deepest level of a document, through the last block of
//! each container: every container above it is written partly from the
//! source (`per-node-reuse`).

#![allow(dead_code)]

use markdown_syntax::prelude::*;

/// Appends a word to the paragraph reached through the last block of each
/// container, or adds a paragraph where the last block is not one.
pub(crate) fn edit_deepest(blocks: &mut Vec<Block>) {
    let children = match blocks.last_mut() {
        Some(Block::BlockQuote(node)) => &mut node.children,
        Some(Block::Alert(node)) => &mut node.children,
        Some(Block::FootnoteDefinition(node)) => &mut node.children,
        Some(Block::ContainerDirective(node)) => &mut node.children,
        Some(Block::HtmlContainer(HtmlContainer {
            content: HtmlContainerContent::Blocks(children),
            ..
        })) => children,
        Some(Block::List(node)) => match node.children.last_mut() {
            Some(item) => &mut item.children,
            None => return,
        },
        Some(Block::Paragraph(paragraph)) => {
            paragraph.children.push(Text::from(" edited").into());
            return;
        }
        _ => {
            blocks.push(Paragraph::new([Text::from("edited")]).into());
            return;
        }
    };
    edit_deepest(children);
}
