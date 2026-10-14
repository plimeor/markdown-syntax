//! Writing a node partly from the source (`per-node-reuse` design): each node
//! matched to the base node at its span, through its parent, is written
//! bottom-up as its source slice when it corresponds to it, with its own
//! syntax copied around children written by the same rules when only its
//! children changed, and rendered otherwise.

use alloc::{format, string::String, vec::Vec};
use core::mem::discriminant;

use super::{
    strip_line_ending, write_blocks, write_delimiter_row, write_inlines, write_item, write_row,
    Context, Correspondence, Join, SerializeError, SerializeOptions,
};
use crate::ast::*;
use crate::compare::normalized_inlines;
use crate::parse::Definitions;
use crate::span::Span;

/// How a node is written.
pub(super) enum Out {
    /// As the source text of its span.
    Kept(Span),
    /// As these lines.
    Lines(Vec<Line>),
    /// As canonical output.
    Render,
}

/// One written line: its text, its line ending (empty on the last line), and
/// whether it already holds every enclosing container's prefix, as a line
/// copied from the source does, or takes them as content.
#[derive(Clone, Debug)]
pub(super) struct Line {
    pub(super) body: String,
    pub(super) ending: String,
    pub(super) full: bool,
}

/// `lines` as text.
pub(super) fn join(lines: &[Line]) -> String {
    let mut output = String::new();
    for line in lines {
        output.push_str(&line.body);
        output.push_str(&line.ending);
    }
    output
}

/// The lines of `text`, split at `\n`, `\r\n`, and `\r` as the canonical
/// writer's prefixing splits them: the first takes `first_full`, every later
/// one `rest_full`. A final line ending ends the last line.
fn split(text: &str, first_full: bool, rest_full: bool) -> Vec<Line> {
    let bytes = text.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut cursor = 0;
    while cursor < text.len() {
        let end = match bytes[cursor] {
            b'\n' => Some(cursor + 1),
            b'\r' if bytes.get(cursor + 1) == Some(&b'\n') => Some(cursor + 2),
            b'\r' => Some(cursor + 1),
            _ => None,
        };
        match end {
            Some(end) => {
                lines.push(Line {
                    body: text[start..cursor].into(),
                    ending: text[cursor..end].into(),
                    full: if lines.is_empty() {
                        first_full
                    } else {
                        rest_full
                    },
                });
                cursor = end;
                start = end;
            }
            None => cursor += 1,
        }
    }
    if start < text.len() || lines.is_empty() {
        lines.push(Line {
            body: text[start..].into(),
            ending: String::new(),
            full: if lines.is_empty() {
                first_full
            } else {
                rest_full
            },
        });
    }
    lines
}

/// Rendered text as content lines.
fn content(text: &str) -> Vec<Line> {
    split(text, false, false)
}

/// One empty content line.
fn empty() -> Line {
    Line {
        body: String::new(),
        ending: String::new(),
        full: false,
    }
}

/// Appends `more` to `lines`, ending the last line with `\n` where it has no
/// ending yet.
fn extend(lines: &mut Vec<Line>, more: Vec<Line>) {
    if let Some(last) = lines.last_mut() {
        if last.ending.is_empty() {
            last.ending.push('\n');
        }
    }
    lines.extend(more);
}

/// Prefixes each content line of `lines` with `prefix`, or with `empty`
/// when it is empty; a full line stays as it is, and every line becomes
/// content of the container around it, apart from full ones.
fn prefixed(lines: Vec<Line>, prefix: &str, empty_prefix: &str) -> Vec<Line> {
    lines
        .into_iter()
        .map(|mut line| {
            if !line.full {
                let head = if line.body.is_empty() {
                    empty_prefix
                } else {
                    prefix
                };
                line.body.insert_str(0, head);
            }
            line
        })
        .collect()
}

/// The base node among `base` at `span` of the same kind as `node`.
fn matching<'b, T>(
    span: Option<Span>,
    node: &T,
    base: &'b [T],
    span_of: impl Fn(&T) -> Option<Span>,
) -> Option<(usize, &'b T)> {
    let span = span?;
    let first = base.partition_point(|other| span_of(other).map_or(0, |at| at.start) < span.start);
    base[first..]
        .iter()
        .enumerate()
        .take_while(|(_, other)| span_of(other).is_some_and(|at| at.start == span.start))
        .find(|(_, other)| {
            span_of(other) == Some(span) && discriminant(*other) == discriminant(node)
        })
        .map(|(index, other)| (first + index, other))
}

/// Writes nodes against the base document, the parse of `source`.
pub(super) struct Reuse<'a, 'c> {
    pub(super) source: &'a str,
    pub(super) options: &'a SerializeOptions,
    pub(super) correspondence: &'c mut Correspondence<'a>,
    /// The definitions of the document being written.
    pub(super) definitions: &'a Definitions,
    /// The definitions of the base document.
    pub(super) base_definitions: &'a Definitions,
}

/// Children written against the base's children: whether they are the base
/// children, all kept, and otherwise their lines.
enum Children {
    Kept,
    Lines(Vec<Line>),
    Render,
}

impl Reuse<'_, '_> {
    /// Where the text of the node at `span` ends, before its line ending.
    fn body_end(&self, span: Span) -> usize {
        span.start + strip_line_ending(&self.source[span.start..span.end]).len()
    }

    /// The source lines of the node at `span`: its first line as content,
    /// every later one full.
    fn slice(&self, span: Span) -> Vec<Line> {
        split(&self.source[span.start..self.body_end(span)], false, true)
    }

    /// The end of the source line `at` is on, before its line ending.
    fn line_end(&self, at: usize) -> usize {
        self.source[at..]
            .find(['\n', '\r'])
            .map_or(self.source.len(), |end| at + end)
    }

    /// The last line of the node at `span`, from its first char that is not a
    /// space, a tab, or `>`: own syntax no container prefix opens with.
    fn last_line(&self, span: Span) -> Option<&str> {
        let body = &self.source[span.start..self.body_end(span)];
        let start = body.rfind(['\n', '\r'])? + 1;
        let line = body[start..].trim_start_matches([' ', '\t', '>']);
        (!line.is_empty()).then_some(line)
    }

    /// Whether a leaf corresponds to its base leaf: its inlines equal the
    /// base's, or the text it keeps is the base leaf's inline source.
    fn leaf_holds(
        &mut self,
        span: Option<Span>,
        kept: Option<&str>,
        inlines: &[Inline],
        base: &[Inline],
    ) -> bool {
        match kept {
            None => inlines == base,
            Some(text) => span
                .and_then(|span| self.correspondence.source_of(span))
                .is_some_and(|source| source.text == text),
        }
    }

    /// The content of a leaf at `span` whose base leaf holds `base`: the text
    /// it keeps, or else its inlines written inline by inline against the
    /// base leaf's inline source, when they read back as themselves.
    fn leaf_text(
        &mut self,
        span: Span,
        kept: Option<&str>,
        inlines: &[Inline],
        base: &[Inline],
        heading: bool,
    ) -> Result<Option<String>, SerializeError> {
        if let Some(text) = kept {
            return Ok(Some(text.into()));
        }
        let Some(source) = self.correspondence.source_of(span).cloned() else {
            return Ok(None);
        };
        let read = crate::parse_inline(&source.text, self.base_definitions).inlines;
        let context = if heading {
            Context::HEADING
        } else {
            Context::BLOCK
        };
        let mut output = String::new();
        if !write_inline_run(inlines, base, &read, &source.text, context, &mut output)? {
            return Ok(None);
        }
        let reads = crate::parse_inline(&output, self.definitions).inlines;
        Ok(
            (normalized_inlines(&reads, heading) == normalized_inlines(inlines, heading))
                .then_some(output),
        )
    }

    /// Writes `block`, matched to `base` (same kind, same span).
    pub(super) fn block(&mut self, block: &Block, base: &Block) -> Result<Out, SerializeError> {
        let span = block.span().expect("a matched block has a span");
        Ok(match (block, base) {
            (Block::Paragraph(node), Block::Paragraph(base)) => {
                let kept = node.source.as_deref();
                if self.leaf_holds(Some(span), kept, &node.children, &base.children) {
                    Out::Kept(span)
                } else {
                    match self.leaf_text(span, kept, &node.children, &base.children, false)? {
                        Some(text) => Out::Lines(content(&text)),
                        None => Out::Render,
                    }
                }
            }
            (Block::Heading(node), Block::Heading(base)) => self.heading(node, base, span)?,
            (Block::BlockQuote(node), Block::BlockQuote(base)) => {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines) => Out::Lines(prefixed(lines, "> ", ">")),
                    Children::Render => Out::Render,
                }
            }
            (Block::Alert(node), Block::Alert(base))
                if node.kind == base.kind && node.title == base.title =>
            {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines) => {
                        let first = &self.source[span.start..self.line_end(span.start)];
                        let mut written = content(first);
                        extend(&mut written, prefixed(lines, "> ", ">"));
                        Out::Lines(written)
                    }
                    Children::Render => Out::Render,
                }
            }
            (Block::List(node), Block::List(base))
                if node.ordered == base.ordered
                    && node.delimiter == base.delimiter
                    && node.tight == base.tight =>
            {
                self.list(node, base, span)?
            }
            (Block::FootnoteDefinition(node), Block::FootnoteDefinition(base))
                if node.label == base.label && node.identifier == base.identifier =>
            {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines) => {
                        let start = first_block(&base.children)
                            .and_then(Block::span)
                            .map_or(usize::MAX, |at| at.start);
                        if start > self.line_end(span.start) {
                            return Ok(Out::Render);
                        }
                        let marker = &self.source[span.start..start];
                        self.marked(marker, lines, "    ")
                    }
                    Children::Render => Out::Render,
                }
            }
            (Block::ContainerDirective(node), Block::ContainerDirective(base))
                if node.name == base.name && node.attributes == base.attributes =>
            {
                let kept = node.label_source.as_deref();
                if !self.leaf_holds(Some(span), kept, &node.label, &base.label) {
                    return Ok(Out::Render);
                }
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines) => {
                        let Some(closing) = self.last_line(span) else {
                            return Ok(Out::Render);
                        };
                        let closing = String::from(closing);
                        let opening = &self.source[span.start..self.line_end(span.start)];
                        let mut written = content(opening);
                        extend(&mut written, lines);
                        extend(&mut written, content(&closing));
                        Out::Lines(written)
                    }
                    Children::Render => Out::Render,
                }
            }
            (Block::Table(node), Block::Table(base)) if node.alignments == base.alignments => {
                self.table(node, base, span)?
            }
            _ => {
                if self.correspondence.holds(block, base) {
                    Out::Kept(span)
                } else {
                    Out::Render
                }
            }
        })
    }

    fn heading(
        &mut self,
        node: &Heading,
        base: &Heading,
        span: Span,
    ) -> Result<Out, SerializeError> {
        if node.depth != base.depth || node.kind != base.kind {
            return Ok(Out::Render);
        }
        let kept = node.source.as_deref();
        if self.leaf_holds(Some(span), kept, &node.children, &base.children) {
            return Ok(Out::Kept(span));
        }
        let Some((from, to)) = self
            .correspondence
            .source_of(span)
            .filter(|source| !source.text.is_empty())
            .map(|source| (source.to_source(0), source.to_source(source.text.len())))
        else {
            return Ok(Out::Render);
        };
        let Some(text) = self.leaf_text(span, kept, &node.children, &base.children, true)? else {
            return Ok(Out::Render);
        };
        let body_end = self.body_end(span);
        Ok(match node.kind {
            HeadingKind::Atx => {
                if text.contains(['\n', '\r']) || from < span.start || to > body_end {
                    return Ok(Out::Render);
                }
                let line = format!(
                    "{}{text}{}",
                    &self.source[span.start..from],
                    &self.source[to..body_end]
                );
                Out::Lines(content(&line))
            }
            HeadingKind::Setext => {
                let Some(underline) = self.last_line(span) else {
                    return Ok(Out::Render);
                };
                let underline = String::from(underline);
                let mut written = content(&text);
                extend(&mut written, content(&underline));
                Out::Lines(written)
            }
        })
    }

    /// A container whose first line opens with `marker`, before the first
    /// line of its children, and whose later content lines take `indent`.
    fn marked(&self, marker: &str, lines: Vec<Line>, indent: &str) -> Out {
        let mut lines = lines.into_iter();
        let Some(mut first) = lines.next() else {
            return Out::Render;
        };
        if first.full {
            return Out::Render;
        }
        first.body.insert_str(0, marker);
        let mut written = alloc::vec![first];
        written.extend(prefixed(lines.collect(), indent, ""));
        Out::Lines(written)
    }

    /// Writes sibling blocks against the base's siblings.
    fn children(
        &mut self,
        children: &[Block],
        base: &[Block],
        join: Join,
    ) -> Result<Children, SerializeError> {
        let span_of = |block: &Block| block.span();
        let matched: Vec<Option<(usize, &Block)>> = children
            .iter()
            .map(|child| matching(child.span(), child, base, span_of))
            .collect();
        let mut outs = Vec::with_capacity(children.len());
        for (child, found) in children.iter().zip(&matched) {
            outs.push(match (child, found) {
                (Block::BlankLine(_), _) => Out::Render,
                (_, Some((_, base))) => self.block(child, base)?,
                (_, None) => Out::Render,
            });
        }
        let all_kept = children.len() == base.len()
            && matched
                .iter()
                .enumerate()
                .all(|(index, found)| found.is_some_and(|(at, _)| at == index))
            && children.iter().zip(&outs).all(|(child, out)| {
                matches!(child, Block::BlankLine(_)) || matches!(out, Out::Kept(_))
            });
        if all_kept {
            return Ok(Children::Kept);
        }
        if children
            .iter()
            .all(|child| matches!(child, Block::BlankLine(_)))
        {
            return Ok(Children::Render);
        }
        let mut lines: Vec<Line> = Vec::new();
        // The last block written: its index among the children, and its base
        // index when it was written from the source.
        let mut previous: Option<(usize, Option<usize>)> = None;
        let mut blank_lines = 0;
        for (index, (child, out)) in children.iter().zip(outs).enumerate() {
            if let Block::BlankLine(_) = child {
                blank_lines += 1;
                continue;
            }
            let at = matched[index]
                .map(|(at, _)| at)
                .filter(|_| !matches!(out, Out::Render));
            let mut written = match out {
                Out::Kept(span) => self.slice(span),
                Out::Lines(lines) => lines,
                Out::Render => content(&write_blocks(
                    core::slice::from_ref(child),
                    self.options,
                    Join::Gap,
                    false,
                    None,
                )?),
            };
            match previous {
                None => {
                    for _ in 0..blank_lines {
                        lines.push(empty());
                    }
                    extend(&mut lines, written);
                }
                Some((before, before_at)) => {
                    let gap = match (before_at, at) {
                        (Some(from), Some(to))
                            if to.checked_sub(from) == Some(index - before)
                                && (before + 1..index).all(|between| {
                                    matched[between]
                                        .is_some_and(|(at, _)| at == from + between - before)
                                }) =>
                        {
                            let start = self.body_end(base[from].span().expect("matched"));
                            let end = base[to].span().expect("matched").start;
                            Some(&self.source[start..end])
                        }
                        _ => None,
                    };
                    match gap.filter(|gap| gap.starts_with(['\n', '\r'])) {
                        Some(gap) => {
                            let mut pieces = split(gap, true, true);
                            // The gap's first piece is the line ending of the
                            // line before it, and its last the prefix of the
                            // next block's first line.
                            let ending = pieces.remove(0);
                            let last = lines.last_mut().expect("a block was written");
                            last.body.push_str(&ending.body);
                            last.ending = ending.ending;
                            let prefix = match pieces.last() {
                                Some(line) if line.ending.is_empty() => {
                                    pieces.pop().map(|line| line.body)
                                }
                                _ => None,
                            };
                            lines.extend(pieces);
                            let first = written.first_mut().expect("a block has a line");
                            if let Some(prefix) = prefix {
                                first.body.insert_str(0, &prefix);
                            }
                            first.full = true;
                            lines.extend(written);
                        }
                        None => {
                            if join == Join::Gap {
                                extend(&mut lines, alloc::vec![empty()]);
                            }
                            let mut blank = Vec::new();
                            for _ in 0..blank_lines {
                                blank.push(empty());
                            }
                            if !blank.is_empty() {
                                extend(&mut lines, blank);
                            }
                            extend(&mut lines, written);
                        }
                    }
                }
            }
            blank_lines = 0;
            previous = Some((index, at));
        }
        if blank_lines > 0 {
            let mut blank = Vec::new();
            for _ in 0..blank_lines {
                blank.push(empty());
            }
            extend(&mut lines, blank);
        }
        Ok(Children::Lines(lines))
    }

    fn list(&mut self, node: &List, base: &List, span: Span) -> Result<Out, SerializeError> {
        let span_of = |item: &ListItem| item.meta.span;
        let mut lines: Vec<Line> = Vec::new();
        let mut all_kept = node.children.len() == base.children.len();
        let mut previous_number: Option<u64> = None;
        // The base index of the item written before, when it was written from
        // the source.
        let mut previous_at: Option<usize> = None;
        let marker_char = super::written_marker(node);
        for (index, item) in node.children.iter().enumerate() {
            let found = matching(item.meta.span, item, &base.children, span_of);
            let out = match found {
                Some((_, base_item)) => self.item(node, item, base_item, &mut previous_number)?,
                None => Out::Render,
            };
            all_kept &= found.is_some_and(|(at, _)| at == index) && matches!(out, Out::Kept(_));
            let at = found
                .map(|(at, _)| at)
                .filter(|_| !matches!(out, Out::Render));
            let written = match out {
                Out::Kept(span) => {
                    if let Some(number) = item.number {
                        previous_number = Some(number);
                    }
                    self.slice(span)
                }
                Out::Lines(lines) => lines,
                Out::Render => content(&write_item(
                    node,
                    item,
                    self.options,
                    marker_char,
                    &mut previous_number,
                )?),
            };
            if index == 0 {
                lines = written;
            } else {
                let gap = match (previous_at, at) {
                    (Some(from), Some(to)) if to == from + 1 => {
                        let start = self.body_end(base.children[from].meta.span.expect("matched"));
                        let end = base.children[to].meta.span.expect("matched").start;
                        Some(&self.source[start..end]).filter(|gap| gap.starts_with(['\n', '\r']))
                    }
                    _ => None,
                };
                match gap {
                    Some(gap) => {
                        let mut pieces = split(gap, true, true);
                        let ending = pieces.remove(0);
                        let last = lines.last_mut().expect("an item was written");
                        last.body.push_str(&ending.body);
                        last.ending = ending.ending;
                        let prefix = match pieces.last() {
                            Some(line) if line.ending.is_empty() => {
                                pieces.pop().map(|line| line.body)
                            }
                            _ => None,
                        };
                        lines.extend(pieces);
                        let mut written = written;
                        let first = written.first_mut().expect("an item has a line");
                        if let Some(prefix) = prefix {
                            first.body.insert_str(0, &prefix);
                        }
                        first.full = true;
                        lines.extend(written);
                    }
                    None => {
                        if !node.tight {
                            extend(&mut lines, alloc::vec![empty()]);
                        }
                        extend(&mut lines, written);
                    }
                }
            }
            previous_at = at;
        }
        if all_kept {
            return Ok(Out::Kept(span));
        }
        Ok(Out::Lines(lines))
    }

    fn item(
        &mut self,
        list: &List,
        item: &ListItem,
        base: &ListItem,
        previous_number: &mut Option<u64>,
    ) -> Result<Out, SerializeError> {
        let span = item.meta.span.expect("a matched item has a span");
        let own = item.checked == base.checked && item.number == base.number;
        let join = if list.tight { Join::Line } else { Join::Gap };
        let children = self.children(&item.children, &base.children, join)?;
        if own && matches!(children, Children::Kept) {
            return Ok(Out::Kept(span));
        }
        let Some(first) = first_block(&base.children) else {
            return Ok(Out::Render);
        };
        if item.checked.is_some() && !matches!(base.children.first(), Some(Block::Paragraph(_))) {
            return Ok(Out::Render);
        }
        let first_start = first.span().map_or(usize::MAX, |at| at.start);
        let line_end = self.line_end(span.start);
        let same_line = first_start <= line_end;
        let marker = if same_line {
            &self.source[span.start..first_start]
        } else {
            self.source[span.start..line_end].trim_end()
        };
        if marker.contains('\t') || !marker.is_ascii() {
            return Ok(Out::Render);
        }
        let column = if same_line {
            marker.len()
        } else {
            marker.len() + 1
        };
        let marker = if own {
            if let Some(number) = item.number {
                *previous_number = Some(number);
            }
            String::from(marker)
        } else {
            let number = list
                .ordered
                .then(|| super::item_number(item, previous_number));
            match changed_marker(marker, number, item.checked, base.checked) {
                Some(marker) if same_line || item.checked == base.checked => marker,
                _ => return Ok(Out::Render),
            }
        };
        let lines = match children {
            Children::Kept => {
                let start = first_start;
                let end = self.body_end(span);
                if start > end {
                    return Ok(Out::Render);
                }
                split(&self.source[start..end], false, true)
            }
            Children::Lines(lines) => lines,
            Children::Render => return Ok(Out::Render),
        };
        let indent = " ".repeat(column);
        Ok(if same_line {
            self.marked(&marker, lines, &indent)
        } else {
            let mut written = content(&marker);
            extend(&mut written, prefixed(lines, &indent, ""));
            Out::Lines(written)
        })
    }

    fn table(&mut self, node: &Table, base: &Table, span: Span) -> Result<Out, SerializeError> {
        let span_of = |row: &TableRow| row.meta.span;
        // The delimiter row: the line after the header row.
        let delimiter = base.rows.first().and_then(|header| {
            let after = self.body_end(header.meta.span?);
            let rest = &self.source[after..];
            let start = after + rest.len() - rest.trim_start_matches(['\r', '\n']).len();
            let line =
                self.source[start..self.line_end(start)].trim_start_matches([' ', '\t', '>']);
            (start > after && !line.is_empty()).then(|| String::from(line))
        });
        let mut all_kept = node.rows.len() == base.rows.len();
        let mut lines = Vec::new();
        for (index, row) in node.rows.iter().enumerate() {
            let found = matching(row.meta.span, row, &base.rows, span_of);
            let holds = match found {
                Some((_, base_row)) => {
                    row.cells.len() == base_row.cells.len()
                        && row
                            .cells
                            .iter()
                            .zip(&base_row.cells)
                            .all(|(cell, base_cell)| {
                                cell.meta.span == base_cell.meta.span
                                    && self.leaf_holds(
                                        cell.meta.span,
                                        cell.source.as_deref(),
                                        &cell.children,
                                        &base_cell.children,
                                    )
                            })
                }
                None => false,
            };
            all_kept &= holds && found.is_some_and(|(at, _)| at == index);
            let written = match (holds, row.meta.span) {
                (true, Some(span)) => content(&self.source[span.start..self.body_end(span)]),
                _ => content(&write_row(row)?),
            };
            extend(&mut lines, written);
            if index == 0 {
                let delimiter = delimiter
                    .clone()
                    .unwrap_or_else(|| write_delimiter_row(&node.alignments));
                extend(&mut lines, content(&delimiter));
            }
        }
        if all_kept {
            return Ok(Out::Kept(span));
        }
        Ok(Out::Lines(lines))
    }
}

/// `marker`, an item's marker as the source writes it, with `number` in
/// place of its number and its checkbox ticked as `checked`, keeping its
/// width by the spaces after the number; `None` where it cannot.
fn changed_marker(
    marker: &str,
    number: Option<u64>,
    checked: Option<bool>,
    base_checked: Option<bool>,
) -> Option<String> {
    let mut written = String::from(marker);
    if let Some(number) = number {
        let digits = written.len()
            - written
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .len();
        let number = format!("{number}");
        // The delimiter, then the spaces that make up the difference.
        let spaces =
            written[digits + 1..].len() - written[digits + 1..].trim_start_matches(' ').len();
        let width = digits + 1 + spaces;
        let gap = width
            .checked_sub(number.len() + 1)
            .filter(|gap| *gap >= 1)?;
        let rest = String::from(&written[width..]);
        written = format!(
            "{number}{}{}{rest}",
            &written[digits..digits + 1],
            " ".repeat(gap)
        );
    }
    if checked != base_checked {
        let (Some(checked), Some(_)) = (checked, base_checked) else {
            return None;
        };
        let open = written.find('[')?;
        written.replace_range(open + 1..open + 2, if checked { "x" } else { " " });
    }
    Some(written)
}

/// The first block of `blocks` other than a blank line.
fn first_block(blocks: &[Block]) -> Option<&Block> {
    blocks
        .iter()
        .find(|block| !matches!(block, Block::BlankLine(_)))
}

/// Writes `inlines`, whose base siblings are `base` and, read from `text`,
/// `read`, copying from `text` each one that equals its base inline and the
/// own syntax of each one whose own fields do, and the text between two
/// copied ones adjacent in the base. Returns whether `base` and `read` had
/// the same shape.
fn write_inline_run(
    inlines: &[Inline],
    base: &[Inline],
    read: &[Inline],
    text: &str,
    context: Context,
    output: &mut String,
) -> Result<bool, SerializeError> {
    if base.len() != read.len() {
        return Ok(false);
    }
    let span_of = |inline: &Inline| inline.span();
    // The base index and text span of the inline written before, when it was
    // copied.
    let mut previous: Option<(usize, Span)> = None;
    for inline in inlines {
        let found = matching(inline.span(), inline, base, span_of);
        let mut copied = None;
        if let Some((at, base_inline)) = found {
            let Some(at_text) = read[at].span() else {
                return Ok(false);
            };
            if discriminant(&read[at]) != discriminant(base_inline) {
                return Ok(false);
            }
            if let Some((before, before_text)) = previous {
                if before + 1 == at && before_text.end <= at_text.start {
                    output.push_str(&text[before_text.end..at_text.start]);
                }
            }
            if inline == base_inline {
                output.push_str(&text[at_text.start..at_text.end]);
                copied = Some((at, at_text));
            } else if own_fields_equal(inline, base_inline)
                && !inline.children().is_empty()
                && !base_inline.children().is_empty()
            {
                let children = read[at].children();
                let (Some(first), Some(last)) = (
                    children.first().and_then(Inline::span),
                    children.last().and_then(Inline::span),
                ) else {
                    return Ok(false);
                };
                if !(at_text.start <= first.start && last.end <= at_text.end) {
                    return Ok(false);
                }
                output.push_str(&text[at_text.start..first.start]);
                if !write_inline_run(
                    inline.children(),
                    base_inline.children(),
                    children,
                    text,
                    context,
                    output,
                )? {
                    return Ok(false);
                }
                output.push_str(&text[last.end..at_text.end]);
                copied = Some((at, at_text));
            }
        }
        if copied.is_none() {
            output.push_str(&write_inlines(core::slice::from_ref(inline), context)?);
        }
        previous = copied;
    }
    Ok(true)
}

/// Whether two inlines of one kind hold the same fields, children aside.
fn own_fields_equal(one: &Inline, other: &Inline) -> bool {
    match (one, other) {
        (Inline::Emphasis(a), Inline::Emphasis(b)) => a.delimiter == b.delimiter,
        (Inline::Strong(a), Inline::Strong(b)) => a.delimiter == b.delimiter,
        (Inline::Delete(_), Inline::Delete(_))
        | (Inline::Mark(_), Inline::Mark(_))
        | (Inline::InlineFootnote(_), Inline::InlineFootnote(_)) => true,
        (Inline::Link(a), Inline::Link(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
        }
        (Inline::Image(a), Inline::Image(b)) => {
            a.destination == b.destination
                && a.destination_kind == b.destination_kind
                && a.title == b.title
        }
        (Inline::LinkReference(a), Inline::LinkReference(b)) => {
            a.identifier == b.identifier && a.label == b.label && a.kind == b.kind
        }
        (Inline::ImageReference(a), Inline::ImageReference(b)) => {
            a.identifier == b.identifier && a.label == b.label && a.kind == b.kind
        }
        (Inline::TextDirective(a), Inline::TextDirective(b)) => {
            a.name == b.name && a.attributes == b.attributes
        }
        // The other kinds hold no children: a change is a change of their
        // own fields.
        _ => false,
    }
}
