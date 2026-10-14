//! The blank-line pass: after the block parse, each blank line beyond the
//! one that separates two blocks becomes a `BlankLine` node.
//!
//! The pass reads the gaps between sibling spans. A line that starts in a
//! gap and ends in it holds no block, so it is blank once its containers'
//! `>` markers are set aside, or it is a container's own line (a fence, a
//! tag, an alert's marker), which holds other text. Gaps of different
//! containers do not overlap, so the pass is linear in the source.

use alloc::vec::Vec;

use crate::ast::{BlankLine, Block, HtmlContainerContent, NodeMeta};
use crate::span::Span;

/// Adds the `BlankLine` nodes of a document whose top-level blocks are
/// `blocks`, read from `input`, which may open with a byte order mark.
pub(super) fn add_blank_lines(input: &str, blocks: &mut Vec<Block>) {
    let lines = Lines::new(input);
    lines.fill(blocks, Span::new(0, input.len()), Edges::BOTH);
}

/// Which ends of a container count every blank line: the others leave the
/// first one as the separator the container's own syntax needs, or leave
/// them all to the container around it.
#[derive(Clone, Copy)]
struct Edges {
    leading: Edge,
    trailing: Edge,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Edge {
    /// Every blank line is a node.
    Each,
    /// The first blank line is the separator.
    AfterOne,
    /// The blank lines belong to the container around it.
    None,
}

impl Edges {
    const BOTH: Self = Self {
        leading: Edge::Each,
        trailing: Edge::Each,
    };
    /// An HTML container writes a blank line before its closing tag.
    const HTML: Self = Self {
        leading: Edge::Each,
        trailing: Edge::AfterOne,
    };
    /// A list item or footnote definition opens on its marker's line, and
    /// a blank line after it is its parent's.
    const NONE: Self = Self {
        leading: Edge::None,
        trailing: Edge::None,
    };
}

/// The start of each line of the input, and the end of its last line.
struct Lines<'a> {
    input: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    fn new(input: &'a str) -> Self {
        let bytes = input.as_bytes();
        let mut starts = alloc::vec![0];
        let mut index = 0;
        while index < bytes.len() {
            match bytes[index] {
                b'\n' => starts.push(index + 1),
                b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                    index += 1;
                    starts.push(index + 1);
                }
                b'\r' => starts.push(index + 1),
                _ => {}
            }
            index += 1;
        }
        // A last line without a line ending ends at the end of the input.
        if starts.last() != Some(&input.len()) {
            starts.push(input.len());
        }
        Self { input, starts }
    }

    /// The blank lines that start at or after `from` and end, with their
    /// line ending, at or before `to`.
    fn blank_in(&self, from: usize, to: usize) -> Vec<Span> {
        let first = self.starts.partition_point(|&start| start < from);
        let mut blank = Vec::new();
        for pair in self.starts[first..].windows(2) {
            let [start, end] = [pair[0], pair[1]];
            if end > to {
                break;
            }
            let line = &self.input[start..end];
            // A byte order mark is not content.
            let line = match start {
                0 => line.strip_prefix('\u{feff}').unwrap_or(line),
                _ => line,
            };
            if !line.is_empty() && is_blank(line) {
                blank.push(Span::new(start, end));
            }
        }
        blank
    }

    /// The rest of the line `from` falls inside, from `from` with its line
    /// ending, when it ends at or before `to`; `None` when `from` starts a
    /// line.
    fn rest_of_line(&self, from: usize, to: usize) -> Option<Span> {
        let next = self.starts.partition_point(|&start| start <= from);
        let end = *self.starts.get(next)?;
        let starts_a_line = next > 0 && self.starts[next - 1] == from;
        (!starts_a_line && end <= to && from < end).then(|| Span::new(from, end))
    }

    /// Adds the blank lines of `blocks`, the children of a container whose
    /// content lies in `region`, and of every container they hold.
    fn fill(&self, blocks: &mut Vec<Block>, region: Span, edges: Edges) {
        for block in blocks.iter_mut() {
            self.fill_block(block);
        }
        let spans: Vec<Span> = blocks.iter().filter_map(Block::span).collect();
        if spans.len() != blocks.len() {
            return;
        }
        let mut gaps: Vec<(usize, Vec<Span>)> = Vec::new();
        let mut from = region.start;
        // A container that opens inside a line (a block quote after a list
        // marker or another quote's `>`) holds the rest of that line as its
        // first line.
        let head = self
            .rest_of_line(from, spans.first().map_or(region.end, |span| span.start))
            .filter(|rest| is_blank(&self.input[rest.start..rest.end]));
        if let Some(rest) = head {
            from = rest.end;
        }
        for (index, span) in spans.iter().enumerate() {
            let mut blank = self.blank_in(from, span.start);
            if index == 0 {
                blank.splice(0..0, head);
            }
            let edge = if index == 0 {
                edges.leading
            } else {
                Edge::AfterOne
            };
            keep(&mut blank, edge);
            if !blank.is_empty() {
                gaps.push((index, blank));
            }
            from = span.end;
        }
        let mut blank = self.blank_in(from, region.end);
        if spans.is_empty() {
            blank.splice(0..0, head);
        }
        keep(
            &mut blank,
            if spans.is_empty() {
                // A container of blank lines alone holds each of them.
                match edges.leading {
                    Edge::None => Edge::None,
                    _ => Edge::Each,
                }
            } else {
                edges.trailing
            },
        );
        if !blank.is_empty() {
            gaps.push((spans.len(), blank));
        }
        if gaps.is_empty() {
            return;
        }
        let count = blocks.len() + gaps.iter().map(|(_, blank)| blank.len()).sum::<usize>();
        let nodes = core::mem::replace(blocks, Vec::with_capacity(count));
        let mut gaps = gaps.into_iter().peekable();
        for (index, block) in nodes.into_iter().enumerate() {
            if let Some((_, blank)) = gaps.next_if(|(at, _)| *at == index) {
                blocks.extend(blank.into_iter().map(blank_line));
            }
            blocks.push(block);
        }
        if let Some((_, blank)) = gaps.next() {
            blocks.extend(blank.into_iter().map(blank_line));
        }
    }

    fn fill_block(&self, block: &mut Block) {
        match block {
            Block::BlockQuote(node) => {
                let region = node.meta.span;
                self.fill_container(&mut node.children, region, Edges::BOTH);
            }
            Block::Alert(node) => {
                let region = node.meta.span;
                self.fill_container(&mut node.children, region, Edges::BOTH);
            }
            Block::ContainerDirective(node) => {
                let region = node.meta.span;
                self.fill_container(&mut node.children, region, Edges::BOTH);
            }
            Block::HtmlContainer(node) => {
                let region = node.meta.span;
                if let HtmlContainerContent::Blocks(children) = &mut node.content {
                    self.fill_container(children, region, Edges::HTML);
                }
            }
            Block::FootnoteDefinition(node) => {
                let region = node.meta.span;
                self.fill_container(&mut node.children, region, Edges::NONE);
            }
            Block::List(list) => {
                for item in &mut list.children {
                    let region = item.meta.span;
                    self.fill_container(&mut item.children, region, Edges::NONE);
                }
                // Blank lines between two items close the earlier one, whose
                // span then ends after them; an
                // item with no block holds none, as nothing can be written
                // between its marker and them.
                for index in 1..list.children.len() {
                    let (Some(end), Some(start)) = (
                        list.children[index - 1].meta.span.map(|span| span.end),
                        list.children[index].meta.span.map(|span| span.start),
                    ) else {
                        continue;
                    };
                    let earlier = &mut list.children[index - 1];
                    if earlier.children.is_empty() {
                        continue;
                    }
                    let mut blank = self.blank_in(end, start);
                    keep(&mut blank, Edge::AfterOne);
                    // The item's span grows over the blank lines it holds.
                    if let (Some(span), Some(last)) = (&mut earlier.meta.span, blank.last()) {
                        span.end = last.end;
                    }
                    earlier.children.extend(blank.into_iter().map(blank_line));
                }
            }
            _ => {}
        }
    }

    fn fill_container(&self, blocks: &mut Vec<Block>, region: Option<Span>, edges: Edges) {
        if let Some(region) = region {
            self.fill(blocks, region, edges);
        }
    }
}

/// Keeps the blank lines of a gap that are nodes at an edge of kind `edge`.
fn keep(blank: &mut Vec<Span>, edge: Edge) {
    match edge {
        Edge::Each => {}
        Edge::AfterOne => {
            if !blank.is_empty() {
                blank.drain(..1);
            }
        }
        Edge::None => blank.clear(),
    }
}

/// Whether a line holds only whitespace and container markers.
fn is_blank(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'>' | b'\n' | b'\r'))
}

fn blank_line(span: Span) -> Block {
    Block::BlankLine(BlankLine {
        meta: NodeMeta::new(Some(span)),
    })
}
