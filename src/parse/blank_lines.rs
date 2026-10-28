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

/// The blank lines that are `BlankLine` nodes between the top-level blocks
/// of the document `input`, which may open with a byte order mark, at
/// `spans`, in order, each with the index of the block it comes before
/// (`spans.len()` after the last). Only where the blocks are is read, so
/// their inline content need not be.
pub(super) fn between_top_level(input: &str, spans: &[Span]) -> Vec<(usize, Span)> {
    Lines::new(input).gaps(
        spans.iter().copied(),
        Span::new(0, input.len()),
        Edges::BOTH,
    )
}

/// Adds the `BlankLine` nodes inside `block`, a top-level block of the
/// document `input`, which may open with a byte order mark.
pub(super) fn add_inside(input: &str, block: &mut Block) {
    Lines::new(input).fill_block(block);
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

/// The lines of the input, read only inside the gap a question names, so the
/// pass reads each byte of a gap a bounded number of times.
struct Lines<'a> {
    input: &'a str,
}

impl<'a> Lines<'a> {
    fn new(input: &'a str) -> Self {
        Self { input }
    }

    /// Whether a line starts at `at`: the start of the input or the end of a
    /// line ending.
    fn starts_line(&self, at: usize) -> bool {
        let bytes = self.input.as_bytes();
        let Some(before) = at.checked_sub(1) else {
            return true;
        };
        match bytes.get(before) {
            Some(b'\n') => true,
            Some(b'\r') => bytes.get(at) != Some(&b'\n'),
            _ => false,
        }
    }

    /// The end, after its line ending, of the line `at` falls inside, when it
    /// ends at or before `to`. A last line without a line ending ends at the
    /// end of the input; there is no line at the end of the input.
    fn line_end(&self, at: usize, to: usize) -> Option<usize> {
        let bytes = self.input.as_bytes();
        let to = to.min(bytes.len());
        let ending = bytes[at.min(to)..to]
            .iter()
            .position(|byte| matches!(byte, b'\n' | b'\r'))
            .map(|offset| at + offset);
        let end = match ending {
            Some(index) if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') => {
                index + 2
            }
            Some(index) => index + 1,
            None if to == bytes.len() && at < to => to,
            None => return None,
        };
        (end <= to).then_some(end)
    }

    /// Calls `found` with each blank line that starts at or after `from`
    /// and ends, with its line ending, at or before `to`.
    fn blank_in(&self, from: usize, to: usize, mut found: impl FnMut(Span)) {
        let mut start = from;
        if !self.starts_line(from) {
            match self.line_end(from, to) {
                Some(end) => start = end,
                None => return,
            }
        }
        while let Some(end) = self.line_end(start, to) {
            let line = &self.input[start..end];
            // A byte order mark is not content.
            let line = match start {
                0 => line.strip_prefix('\u{feff}').unwrap_or(line),
                _ => line,
            };
            if !line.is_empty() && is_blank(line) {
                found(Span::new(start, end));
            }
            start = end;
        }
    }

    /// Calls `keep` with each blank line of the gap from `from` to `to`, after
    /// `head`, that is a node at an edge of kind `edge`.
    fn gap(
        &self,
        from: usize,
        to: usize,
        head: Option<Span>,
        edge: Edge,
        mut keep: impl FnMut(Span),
    ) {
        let mut skip = match edge {
            Edge::Each => 0,
            Edge::AfterOne => 1,
            Edge::None => return,
        };
        let mut found = |span| {
            if skip == 0 {
                keep(span);
            } else {
                skip -= 1;
            }
        };
        if let Some(head) = head {
            found(head);
        }
        self.blank_in(from, to, found);
    }

    /// The rest of the line `from` falls inside, from `from` with its line
    /// ending, when it ends at or before `to`; `None` when `from` starts a
    /// line.
    fn rest_of_line(&self, from: usize, to: usize) -> Option<Span> {
        if self.starts_line(from) {
            return None;
        }
        let end = self.line_end(from, to)?;
        Some(Span::new(from, end))
    }

    /// Adds the blank lines of `blocks`, the children of a container whose
    /// content lies in `region`, and of every container they hold.
    fn fill(&self, blocks: &mut Vec<Block>, region: Span, edges: Edges) {
        for block in blocks.iter_mut() {
            self.fill_block(block);
        }
        if !blocks.iter().all(|block| block.span().is_some()) {
            return;
        }
        let spans = blocks.iter().filter_map(Block::span);
        let blank = self.gaps(spans, region, edges);
        if blank.is_empty() {
            return;
        }
        let nodes = core::mem::replace(blocks, Vec::with_capacity(blocks.len() + blank.len()));
        let mut blank = blank.into_iter().peekable();
        for (index, block) in nodes.into_iter().enumerate() {
            while let Some((_, span)) = blank.next_if(|(before, _)| *before == index) {
                blocks.push(blank_line(span));
            }
            blocks.push(block);
        }
        blocks.extend(blank.map(|(_, span)| blank_line(span)));
    }

    /// The blank lines that are nodes in the gaps around the blocks at
    /// `spans`, the children of a container whose content lies in `region`,
    /// in order, each with the index of the block it comes before (the
    /// number of blocks after the last).
    fn gaps(
        &self,
        spans: impl Iterator<Item = Span> + Clone,
        region: Span,
        edges: Edges,
    ) -> Vec<(usize, Span)> {
        let mut blank = Vec::new();
        let mut from = region.start;
        // A container that opens inside a line (a block quote after a list
        // marker or another quote's `>`) holds the rest of that line as its
        // first line.
        let head = self
            .rest_of_line(
                from,
                spans.clone().next().map_or(region.end, |span| span.start),
            )
            .filter(|rest| is_blank(&self.input[rest.start..rest.end]));
        if let Some(rest) = head {
            from = rest.end;
        }
        let mut count = 0;
        for (index, span) in spans.enumerate() {
            let (head, edge) = match index {
                0 => (head, edges.leading),
                _ => (None, Edge::AfterOne),
            };
            self.gap(from, span.start, head, edge, |span| {
                blank.push((index, span))
            });
            from = span.end;
            count += 1;
        }
        let (head, edge) = if count == 0 {
            // A container of blank lines alone holds each of them.
            let edge = match edges.leading {
                Edge::None => Edge::None,
                _ => Edge::Each,
            };
            (head, edge)
        } else {
            (None, edges.trailing)
        };
        self.gap(from, region.end, head, edge, |span| {
            blank.push((count, span));
        });
        blank
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
                    let mut last = None;
                    self.gap(end, start, None, Edge::AfterOne, |span| {
                        earlier.children.push(blank_line(span));
                        last = Some(span);
                    });
                    // The item's span grows over the blank lines it holds.
                    if let (Some(span), Some(last)) = (&mut earlier.meta.span, last) {
                        span.end = last.end;
                    }
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

/// Whether a line holds only whitespace and container markers.
fn is_blank(line: &str) -> bool {
    line.bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'>' | b'\n' | b'\r'))
}

/// The `BlankLine` node of the blank line at `span`.
pub(super) fn blank_line(span: Span) -> Block {
    Block::BlankLine(BlankLine {
        meta: NodeMeta::new(Some(span)),
    })
}
