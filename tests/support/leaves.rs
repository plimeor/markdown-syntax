//! The leaf-block property of "Inline source of a leaf block"
//! (`docs/specs/public-api.md`): every paragraph and heading, at every depth,
//! parsed again with `parse_inline` from its inline source and its document's
//! definitions, yields the inlines the document holds once their spans are
//! mapped through the inline source.

use markdown_syntax::__private::inline_spans_to_source;
use markdown_syntax::prelude::*;

/// Each paragraph and heading of `blocks`, at every depth, with its path.
fn leaves<'a>(
    blocks: &'a [Block],
    path: &mut Vec<usize>,
    found: &mut Vec<(NodePath, &'a [Inline])>,
) {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        match block {
            Block::Paragraph(node) => found.push((NodePath(path.clone()), &node.children)),
            Block::Heading(node) => found.push((NodePath(path.clone()), &node.children)),
            Block::BlockQuote(node) => leaves(&node.children, path, found),
            Block::Alert(node) => leaves(&node.children, path, found),
            Block::FootnoteDefinition(node) => leaves(&node.children, path, found),
            Block::ContainerDirective(node) => leaves(&node.children, path, found),
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(children) = &node.content {
                    leaves(children, path, found);
                }
            }
            Block::List(node) => {
                for (item, list_item) in node.children.iter().enumerate() {
                    path.push(item);
                    leaves(&list_item.children, path, found);
                    path.pop();
                }
            }
            _ => {}
        }
        path.pop();
    }
}

/// How many leaves `input` holds, each of which reproduces its inlines, or
/// the first that does not.
pub(crate) fn leaves_reproduce(input: &str) -> Result<usize, String> {
    let document = parse(input).document;
    let definitions = Definitions::of(&document);
    let mut found = Vec::new();
    leaves(&document.children, &mut Vec::new(), &mut found);
    for (path, inlines) in &found {
        let Some(source) = document.inline_source(path) else {
            return Err(format!("{input:?} {path:?}: no inline source"));
        };
        let mut reparsed = parse_inline(&source.text, &definitions).inlines;
        inline_spans_to_source(&source, &mut reparsed);
        if reparsed != *inlines {
            return Err(format!(
                "{input:?} {path:?}: {:?} reads as {reparsed:?}, not {inlines:?}",
                source.text
            ));
        }
    }
    Ok(found.len())
}
