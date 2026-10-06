//! Block layout from the read-back: the serializer writes the default layout,
//! parses the whole output, and where a written block reads back as another
//! one, applies a layout alternative to a node on the way to the difference
//! and writes the document again. An alternative that leaves the difference
//! where it was is withdrawn, and each is tried once, within a bounded
//! number of rounds.

use alloc::vec::Vec;
use core::mem::discriminant;

use crate::ast::*;

/// A layout other than the default one.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Alternative {
    /// A block quote opens with an empty quote line, so that its first line
    /// cannot read as an alert marker.
    QuoteOpensEmpty,
    /// A block quote or alert ends with an empty quote line, which ends its
    /// last paragraph before the block after it.
    QuoteEndsEmpty,
    /// A list item's first block starts on the line after its marker, so
    /// that its first line cannot move the item's content column.
    ItemOnNextLine,
    /// A list takes a marker other than the list after it takes, so that
    /// the two do not read as one list.
    ListApartFromNext,
    /// A list's markers are indented past the indentation of the block after
    /// it, whose lines would otherwise continue the list's last item.
    ListPastNext,
    /// A block is followed by the next one without a blank line, which a
    /// block it ends with, such as an unclosed HTML comment, would take; the
    /// last block of a list item is so followed by the next item.
    JoinNext,
    /// A dash thematic break is written spaced, `- - -`, so that it cannot
    /// read as the setext underline of the paragraph before it.
    BreakSpaced,
    /// A block quote's markers are written one column further in, ` > `, so
    /// that a tab opening a line inside it reaches its tab stop sooner.
    QuoteIndented,
}

/// A layout alternative for a node, by its address.
pub(super) type Choice = (usize, Alternative);

/// The layout alternatives applied so far, by node address.
#[derive(Clone, Debug, Default)]
pub(super) struct Layout {
    choices: Vec<(usize, Alternative)>,
}

impl Layout {
    pub(super) fn has<T>(&self, node: &T, alternative: Alternative) -> bool {
        self.choices
            .binary_search(&(address(node), alternative))
            .is_ok()
    }

    pub(super) fn holds(&self, choice: &Choice) -> bool {
        self.choices.binary_search(choice).is_ok()
    }

    pub(super) fn add(&mut self, choice: Choice) {
        if let Err(at) = self.choices.binary_search(&choice) {
            self.choices.insert(at, choice);
        }
    }

    pub(super) fn remove(&mut self, choice: &Choice) {
        if let Ok(at) = self.choices.binary_search(choice) {
            self.choices.remove(at);
        }
    }
}

fn address<T>(node: &T) -> usize {
    node as *const T as usize
}

/// A node of the written document on the way to where its reparse first
/// differs.
#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Block(&'a Block),
    Item(&'a ListItem),
}

/// One step of that way: the node, its position among its siblings, and
/// whether the reparse holds a node of its kind there.
#[derive(Clone, Copy)]
pub(super) struct Step<'a> {
    pub(super) node: Node<'a>,
    index: usize,
    siblings: usize,
    same_kind: bool,
    /// The written block after this one, among its siblings.
    next: Option<&'a Block>,
}

/// The way from the document to the deepest written node where the reparse
/// first differs, comparing the layout-normalized `ours` and `theirs`, whose
/// shape `written` shares with `ours`. Empty when they are the same.
pub(super) fn divergence<'a>(
    written: &'a [Block],
    ours: &[Block],
    theirs: &[Block],
) -> Vec<Step<'a>> {
    let mut path = Vec::new();
    diverge(written, ours, theirs, &mut path);
    path
}

fn diverge<'a>(written: &'a [Block], ours: &[Block], theirs: &[Block], path: &mut Vec<Step<'a>>) {
    for index in 0..ours.len().max(theirs.len()) {
        match (ours.get(index), theirs.get(index)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), b) => {
                let same_kind = b.is_some_and(|b| discriminant(a) == discriminant(b));
                path.push(Step {
                    node: Node::Block(&written[index]),
                    index,
                    siblings: ours.len(),
                    same_kind,
                    next: written.get(index + 1),
                });
                if let (true, Some(b)) = (same_kind, b) {
                    descend(&written[index], a, b, path);
                }
                return;
            }
            // The reparse holds a block more: the block before it split.
            (None, _) => {
                if let Some(last) = index.checked_sub(1) {
                    path.push(Step {
                        node: Node::Block(&written[last]),
                        index: last,
                        siblings: ours.len(),
                        same_kind: true,
                        next: written.get(index),
                    });
                }
                return;
            }
        }
    }
}

fn descend<'a>(written: &'a Block, ours: &Block, theirs: &Block, path: &mut Vec<Step<'a>>) {
    match (written, ours, theirs) {
        (Block::BlockQuote(w), Block::BlockQuote(a), Block::BlockQuote(b)) => {
            diverge(&w.children, &a.children, &b.children, path)
        }
        (Block::Alert(w), Block::Alert(a), Block::Alert(b)) => {
            diverge(&w.children, &a.children, &b.children, path)
        }
        (
            Block::FootnoteDefinition(w),
            Block::FootnoteDefinition(a),
            Block::FootnoteDefinition(b),
        ) => diverge(&w.children, &a.children, &b.children, path),
        (
            Block::ContainerDirective(w),
            Block::ContainerDirective(a),
            Block::ContainerDirective(b),
        ) => diverge(&w.children, &a.children, &b.children, path),
        (Block::List(w), Block::List(a), Block::List(b)) => {
            for index in 0..a.children.len().max(b.children.len()) {
                match (a.children.get(index), b.children.get(index)) {
                    (Some(x), Some(y)) if x == y => {}
                    (Some(x), y) => {
                        path.push(Step {
                            node: Node::Item(&w.children[index]),
                            index,
                            siblings: a.children.len(),
                            same_kind: y.is_some(),
                            next: None,
                        });
                        if let Some(y) = y {
                            diverge(&w.children[index].children, &x.children, &y.children, path);
                        }
                        return;
                    }
                    (None, _) => return,
                }
            }
        }
        _ => {}
    }
}

/// The layout alternatives that may fix the difference `path` leads to,
/// nearest the difference first.
pub(super) fn candidates(path: &[Step<'_>]) -> Vec<Choice> {
    let mut candidates = Vec::new();
    for (depth, step) in path.iter().enumerate().rev() {
        let parent = depth.checked_sub(1).map(|parent| &path[parent]);
        match step.node {
            // A quote read as another block, such as an alert.
            Node::Block(block @ Block::BlockQuote(_)) if !step.same_kind => {
                candidates.push((address(block), Alternative::QuoteOpensEmpty));
            }
            // A list read as another block, such as a thematic break.
            Node::Block(Block::List(list)) if !step.same_kind => {
                if let Some(item) = list.children.first() {
                    candidates.push((address(item), Alternative::ItemOnNextLine));
                }
            }
            // A paragraph read as a heading over the dash break after it.
            Node::Block(Block::Paragraph(_)) if !step.same_kind => {
                if let Some(
                    next @ Block::ThematicBreak(ThematicBreak {
                        marker: ThematicBreakMarker::Dash,
                        ..
                    }),
                ) = step.next
                {
                    candidates.push((address(next), Alternative::BreakSpaced));
                }
            }
            // An item read as another block, such as a thematic break, or
            // as no item.
            Node::Item(item) if !step.same_kind => {
                candidates.push((address(item), Alternative::ItemOnNextLine));
            }
            // A list read with what follows it: the items of the list after
            // it, or the lines of the block after it.
            Node::Block(block @ Block::List(_)) if step.index + 1 < step.siblings => {
                candidates.push((address(block), Alternative::ListApartFromNext));
                candidates.push((address(block), Alternative::ListPastNext));
            }
            _ => {}
        }
        let Some(parent) = parent else {
            continue;
        };
        match parent.node {
            // A block of an item whose content column its first line moved.
            Node::Item(item) => candidates.push((address(item), Alternative::ItemOnNextLine)),
            // The last block of a quote that a block follows, which may have
            // taken that block's first line.
            Node::Block(quote @ (Block::BlockQuote(_) | Block::Alert(_)))
                if step.index + 1 == step.siblings && parent.index + 1 < parent.siblings =>
            {
                candidates.push((address(quote), Alternative::QuoteEndsEmpty));
            }
            // The last item of a list that a block follows, which may have
            // taken that block's lines.
            Node::Block(list @ Block::List(_))
                if step.index + 1 == step.siblings && parent.index + 1 < parent.siblings =>
            {
                candidates.push((address(list), Alternative::ListPastNext));
            }
            // A block of a quote read as another block, such as indented
            // code where a tab took four columns.
            Node::Block(quote @ Block::BlockQuote(_)) if !step.same_kind => {
                candidates.push((address(quote), Alternative::QuoteIndented));
            }
            _ => {}
        }
    }
    // A block that took the blank line after it, nearest the difference
    // first, joins the next block, or the next item when it ends an item.
    for (depth, step) in path.iter().enumerate().rev() {
        if let Node::Block(block) = step.node {
            let ends_item = step.index + 1 == step.siblings
                && depth.checked_sub(1).is_some_and(|parent| {
                    matches!(path[parent].node, Node::Item(_))
                        && path[parent].index + 1 < path[parent].siblings
                });
            if step.index + 1 < step.siblings || ends_item {
                candidates.push((address(block), Alternative::JoinNext));
            }
        }
    }
    candidates
}

/// Where a difference is, as the address of the deepest node on its way.
pub(super) fn key(path: &[Step<'_>]) -> Option<usize> {
    path.last().map(|step| match step.node {
        Node::Block(block) => address(block),
        Node::Item(item) => address(item),
    })
}

/// The written block or item a difference that no alternative fixes is
/// blamed on: the deepest one on its way.
pub(super) fn blamed<'a>(path: &[Step<'a>]) -> Option<Node<'a>> {
    path.last().map(|step| step.node)
}

/// Pairs each node of `shape`, a list holding copies of `items` and written
/// as `list`, with the node it copies, by address: the list itself, each
/// item, and every block and item inside them.
pub(super) fn address_pairs_of_items(
    items: &[ListItem],
    shape: &[Block],
    list: &Block,
    pairs: &mut Vec<(usize, usize)>,
) {
    let [shape_block @ Block::List(shape_list)] = shape else {
        return;
    };
    pairs.push((address(shape_block), address(list)));
    for (copy, item) in shape_list.children.iter().zip(items) {
        pairs.push((address(copy), address(item)));
        pair_blocks(&copy.children, &item.children, pairs);
    }
}

fn pair_blocks(copies: &[Block], originals: &[Block], pairs: &mut Vec<(usize, usize)>) {
    for (copy, original) in copies.iter().zip(originals) {
        pairs.push((address(copy), address(original)));
        match (copy, original) {
            (Block::BlockQuote(a), Block::BlockQuote(b)) => {
                pair_blocks(&a.children, &b.children, pairs)
            }
            (Block::Alert(a), Block::Alert(b)) => pair_blocks(&a.children, &b.children, pairs),
            (Block::FootnoteDefinition(a), Block::FootnoteDefinition(b)) => {
                pair_blocks(&a.children, &b.children, pairs)
            }
            (Block::ContainerDirective(a), Block::ContainerDirective(b)) => {
                pair_blocks(&a.children, &b.children, pairs)
            }
            (Block::List(a), Block::List(b)) => {
                for (copy, item) in a.children.iter().zip(&b.children) {
                    pairs.push((address(copy), address(item)));
                    pair_blocks(&copy.children, &item.children, pairs);
                }
            }
            _ => {}
        }
    }
}
