//! The leaf property of "Inline source of a leaf block" and "Leaf sources"
//! (`docs/specs/public-api.md`): every paragraph, heading, table cell, and
//! directive label, at every depth, parsed again with `parse_inline` from its
//! inline source and its document's definitions, yields the inlines the
//! document holds once their spans are mapped through the inline source; and
//! `leaf_sources` returns the same paths and sources in one pass.

#![allow(dead_code)]

use markdown_syntax::__private::inline_spans_to_source;
use markdown_syntax::prelude::*;

/// A leaf of a document: its path, its inlines, and whether it must have an
/// inline source (a directive's empty label may have been written without
/// brackets, and so has none).
type Leaf<'a> = (NodePath, &'a [Inline], bool);

/// Each leaf of `blocks`, at every depth, with its path.
fn leaves<'a>(blocks: &'a [Block], path: &mut Vec<usize>, found: &mut Vec<Leaf<'a>>) {
    for (index, block) in blocks.iter().enumerate() {
        path.push(index);
        let here = NodePath(path.clone());
        match block {
            Block::Paragraph(node) => found.push((here, &node.children, true)),
            Block::Heading(node) => found.push((here, &node.children, true)),
            Block::LeafDirective(node) => found.push((here, &node.label, !node.label.is_empty())),
            Block::ContainerDirective(node) => {
                found.push((here, &node.label, !node.label.is_empty()));
                leaves(&node.children, path, found);
            }
            Block::Table(node) => {
                for (row, cells) in node.rows.iter().enumerate() {
                    for (cell, node) in cells.cells.iter().enumerate() {
                        let mut at = path.clone();
                        at.extend([row, cell]);
                        found.push((NodePath(at), &node.children, true));
                    }
                }
            }
            Block::BlockQuote(node) => leaves(&node.children, path, found),
            Block::Alert(node) => leaves(&node.children, path, found),
            Block::FootnoteDefinition(node) => leaves(&node.children, path, found),
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

/// Whether two inline sources hold the same text and map each of its offsets
/// to the same source offset.
fn same_source(one: &InlineSource, other: &InlineSource) -> bool {
    one.text == other.text
        && (0..=one.text.len())
            .filter(|&offset| one.text.is_char_boundary(offset))
            .all(|offset| one.to_source(offset) == other.to_source(offset))
}

/// How many leaves `input` holds, each of which reproduces its inlines and
/// appears in `leaf_sources` with the same source, or the first that does
/// not.
pub(crate) fn leaves_reproduce(input: &str) -> Result<usize, String> {
    let document = parse(input).document;
    let definitions = Definitions::of(&document);
    let mut found = Vec::new();
    leaves(&document.children, &mut Vec::new(), &mut found);
    let all = document.leaf_sources();
    let mut all = all.iter().peekable();
    let mut read = 0;
    for (path, inlines, required) in &found {
        let Some(source) = document.inline_source(path) else {
            if *required {
                return Err(format!("{input:?} {path:?}: no inline source"));
            }
            continue;
        };
        match all.next() {
            Some((at, other)) if at == path && same_source(&source, other) => {}
            other => {
                return Err(format!(
                    "{input:?} {path:?}: leaf_sources holds {other:?}, not {source:?}"
                ))
            }
        }
        let mut reparsed = parse_inline(&source.text, &definitions).inlines;
        inline_spans_to_source(&source, &mut reparsed);
        if reparsed != *inlines {
            return Err(format!(
                "{input:?} {path:?}: {:?} reads as {reparsed:?}, not {inlines:?}",
                source.text
            ));
        }
        read += 1;
    }
    if let Some(extra) = all.next() {
        return Err(format!("{input:?}: leaf_sources also holds {extra:?}"));
    }
    Ok(read)
}

/// `document` with every leaf `leaf_sources` names built again with
/// `with_source` from its inline source and the document's definitions,
/// keeping its span and other fields.
pub(crate) fn rebuilt_from_leaf_sources(document: &Document) -> Document {
    let definitions = Definitions::of(document);
    let mut rebuilt = document.clone();
    for (path, source) in document.leaf_sources() {
        rebuild_at(&mut rebuilt.children, &path.0, &source.text, &definitions);
    }
    rebuilt
}

/// Builds the leaf at `path` in `blocks` again from `text`.
pub(crate) fn rebuild_at(
    blocks: &mut [Block],
    path: &[usize],
    text: &str,
    definitions: &Definitions,
) {
    let (first, rest) = path.split_first().expect("a leaf's path");
    match (&mut blocks[*first], rest) {
        (Block::Paragraph(node), []) => {
            let meta = node.meta.clone();
            *node = Paragraph::with_source(text, definitions).0;
            node.meta = meta;
        }
        (Block::Heading(node), []) => {
            let meta = node.meta.clone();
            *node = Heading::with_source(node.depth, text, definitions).0;
            node.meta = meta;
        }
        (Block::LeafDirective(node), []) => {
            let mut built = LeafDirective::with_source(node.name.clone(), text, definitions).0;
            built.meta = node.meta.clone();
            built.attributes = node.attributes.clone();
            *node = built;
        }
        (Block::ContainerDirective(node), []) => {
            let children = std::mem::take(&mut node.children);
            let mut built =
                ContainerDirective::with_source(node.name.clone(), text, children, definitions).0;
            built.meta = node.meta.clone();
            built.attributes = node.attributes.clone();
            *node = built;
        }
        (Block::Table(node), [row, cell]) => {
            let cell = &mut node.rows[*row].cells[*cell];
            let meta = cell.meta.clone();
            *cell = TableCell::with_source(text, definitions).0;
            cell.meta = meta;
        }
        (Block::List(node), [item, rest @ ..]) => {
            rebuild_at(&mut node.children[*item].children, rest, text, definitions)
        }
        (Block::ContainerDirective(node), rest) => {
            rebuild_at(&mut node.children, rest, text, definitions)
        }
        (Block::BlockQuote(node), rest) => rebuild_at(&mut node.children, rest, text, definitions),
        (Block::Alert(node), rest) => rebuild_at(&mut node.children, rest, text, definitions),
        (Block::FootnoteDefinition(node), rest) => {
            rebuild_at(&mut node.children, rest, text, definitions)
        }
        (Block::HtmlContainer(node), rest) => match &mut node.content {
            HtmlContainerContent::Blocks(children) => rebuild_at(children, rest, text, definitions),
            HtmlContainerContent::Inlines(_) => panic!("{path:?} names no leaf"),
        },
        _ => panic!("{path:?} names no leaf"),
    }
}
