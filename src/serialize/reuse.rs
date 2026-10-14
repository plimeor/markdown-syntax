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

/// Appends `count` empty content lines to `lines`.
fn push_empty(lines: &mut Vec<Line>, count: usize) {
    for _ in 0..count {
        extend(lines, alloc::vec![empty()]);
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

/// The lines of `head`, the source before a container's first block from
/// the container's start, and the piece of it that opens the block's first
/// line: content when that line is the container's first, else full.
fn head_and_prefix(head: &str) -> (Vec<Line>, Line) {
    let mut lines = split(head, false, true);
    let prefix = match lines.last() {
        Some(last) if last.ending.is_empty() => lines.pop().expect("a last line"),
        _ => Line {
            body: String::new(),
            ending: String::new(),
            full: true,
        },
    };
    (lines, prefix)
}

/// Appends `written`, a block's lines, to `lines` after `gap`, the source
/// between the block before it and this one: the gap's first piece ends the
/// last line, its whole lines follow, and its last piece is the prefix of
/// `written`'s first line, which it makes full.
fn splice(lines: &mut Vec<Line>, gap: &str, mut written: Vec<Line>) {
    let mut pieces = split(gap, true, true);
    let ending = pieces.remove(0);
    let last = lines.last_mut().expect("a block was written");
    last.body.push_str(&ending.body);
    last.ending = ending.ending;
    let prefix = match pieces.last() {
        Some(line) if line.ending.is_empty() => pieces.pop().map(|line| line.body),
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

/// Whether `gap`, the source between two siblings, holds a line of nothing
/// but container prefixes: a blank line where the siblings stand.
pub(super) fn holds_blank_line(gap: &str) -> bool {
    split(gap, false, false)
        .iter()
        .skip(1)
        .filter(|line| !line.ending.is_empty())
        .any(|line| {
            line.body
                .chars()
                .all(|char| matches!(char, ' ' | '\t' | '>'))
        })
}

/// Whether the first of `lines`, a node written against the base node at
/// `span`, is that node's first source line.
pub(super) fn first_line_holds(source: &str, lines: &[Line], span: Span) -> bool {
    let end = source[span.start..]
        .find(['\n', '\r'])
        .map_or(source.len(), |end| span.start + end);
    lines
        .first()
        .is_some_and(|first| !first.full && first.body == source[span.start..end])
}

/// Whether the last of `lines`, a node written against the base node at
/// `span` whose text ends at `body_end`, is that node's last source line;
/// `top` where content lines take no prefix.
pub(super) fn last_line_holds(
    source: &str,
    lines: &[Line],
    span: Span,
    body_end: usize,
    top: bool,
) -> bool {
    let line_start = source[..body_end]
        .rfind(['\n', '\r'])
        .map_or(0, |at| at + 1);
    let Some(last) = lines.last() else {
        return false;
    };
    if lines.len() == 1 && !last.full {
        line_start <= span.start && last.body == source[span.start..body_end]
    } else {
        (last.full || top) && last.body == source[line_start..body_end]
    }
}

/// Whether `gap`, the source between two siblings written from the source
/// and adjacent there, reads the same between them as written: it holds a
/// blank line, or one of them is a blank line, or the lines around it are
/// the source's. A leaf's own edited lines read the same before any sibling;
/// after a container, whose last line a leaf could continue, or before one,
/// only its source lines do. `left` and `right` are each a block, its base
/// span, and its lines when it was not kept whole; `top` where content lines
/// take no prefix.
pub(super) fn keeps_gap(
    source: &str,
    gap: &str,
    left: (&Block, Span, Option<&[Line]>),
    right: (&Block, Span, Option<&[Line]>),
    top: bool,
) -> bool {
    let (left_block, left_span, left_lines) = left;
    let (right_block, right_span, right_lines) = right;
    let left_end =
        left_span.start + strip_line_ending(&source[left_span.start..left_span.end]).len();
    let containers = holds_blocks(left_block) || holds_blocks(right_block);
    holds_blank_line(gap)
        || matches!(left_block, Block::BlankLine(_))
        || matches!(right_block, Block::BlankLine(_))
        || (!holds_blocks(left_block)
            || left_lines
                .is_none_or(|lines| last_line_holds(source, lines, left_span, left_end, top)))
            && (!containers
                || right_lines.is_none_or(|lines| first_line_holds(source, lines, right_span)))
}

/// Whether `block` holds blocks, whose last line can change what the line
/// after it reads as.
pub(super) fn holds_blocks(block: &Block) -> bool {
    matches!(
        block,
        Block::BlockQuote(_)
            | Block::Alert(_)
            | Block::List(_)
            | Block::FootnoteDefinition(_)
            | Block::ContainerDirective(_)
            | Block::HtmlContainer(_)
    )
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
    /// Their lines, and whether a blank line separates two of them or
    /// follows the last.
    Lines(Vec<Line>, bool),
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
        self.source_lines(span.start, self.body_end(span))
    }

    /// The source lines from `start` to `end`, the first as content and
    /// every later one full; text that ends with a line ending, as a node
    /// ending with a blank line does, ends with an empty line.
    fn source_lines(&self, start: usize, end: usize) -> Vec<Line> {
        let mut lines = split(&self.source[start..end], false, true);
        if lines.last().is_some_and(|line| !line.ending.is_empty()) {
            lines.push(Line {
                body: String::new(),
                ending: String::new(),
                full: true,
            });
        }
        lines
    }

    /// Whether the source of `base`, a sequence of sibling blocks, holds a
    /// blank line between its last block and the blank lines after it, as
    /// a list item's does, whose source holds the separator to the next
    /// item.
    fn separates_trailing_blank_lines(&self, base: &[Block]) -> bool {
        let Some(last) = base
            .iter()
            .rposition(|block| !matches!(block, Block::BlankLine(_)))
        else {
            return false;
        };
        let (Some(block), Some(blank)) =
            (base[last].span(), base.get(last + 1).and_then(Block::span))
        else {
            return false;
        };
        holds_blank_line(&self.source[self.body_end(block)..blank.start])
    }

    /// Whether a blank line in the source separates two of `base`, sibling
    /// blocks, or follows the last: what makes the list holding them as an
    /// item's children loose.
    fn separated(&self, base: &[Block]) -> bool {
        let mut previous: Option<Span> = None;
        for block in base {
            let Some(span) = block.span() else {
                return true;
            };
            if let Block::BlankLine(_) = block {
                if previous.is_some() {
                    return true;
                }
                continue;
            }
            if let Some(previous) = previous {
                if holds_blank_line(&self.source[self.body_end(previous)..span.start]) {
                    return true;
                }
            }
            previous = Some(span);
        }
        false
    }

    /// The source text of the node at `span`, with its line ending, and
    /// every line after the first from its first char that is not a space,
    /// a tab, or `>`: the node as it reads without its containers.
    fn unprefixed(&self, span: Span) -> String {
        let mut text = String::new();
        for (index, line) in split(&self.source[span.start..span.end], false, true)
            .iter()
            .enumerate()
        {
            let body = if index == 0 {
                line.body.as_str()
            } else {
                line.body.trim_start_matches([' ', '\t', '>'])
            };
            text.push_str(body);
            text.push_str(&line.ending);
        }
        text
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
        // Lines the source indented, or wrote as lazy continuation lines,
        // start at the container's column here, so no line may open a block,
        // nor a later one open with spaces or tabs, which the block parse
        // strips.
        if (!heading || output.contains(['\n', '\r']))
            && (split(&output, false, false)
                .iter()
                .skip(1)
                .any(|line| line.body.starts_with([' ', '\t']))
                || !matches!(
                    crate::parse(&output).document.children.as_slice(),
                    [Block::Paragraph(_)]
                ))
        {
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
        let base_block = base;
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
            (Block::Heading(node), Block::Heading(base))
                if own_block_fields_equal(block, base_block) =>
            {
                self.heading(node, base, span)?
            }
            (Block::BlockQuote(node), Block::BlockQuote(base)) => {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines, _) => self.quoted(span, &base.children, lines),
                    Children::Render => Out::Render,
                }
            }
            (Block::Alert(node), Block::Alert(base))
                if own_block_fields_equal(block, base_block) =>
            {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines, _) => {
                        let first = &self.source[span.start..self.line_end(span.start)];
                        let mut written = content(first);
                        extend(&mut written, prefixed(lines, "> ", ">"));
                        Out::Lines(written)
                    }
                    Children::Render => Out::Render,
                }
            }
            (Block::List(node), Block::List(base)) if own_block_fields_equal(block, base_block) => {
                self.list(node, base, span)?
            }
            (Block::FootnoteDefinition(node), Block::FootnoteDefinition(base))
                if own_block_fields_equal(block, base_block) =>
            {
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines, _) => {
                        let start = first_block(&base.children)
                            .and_then(Block::span)
                            .map_or(usize::MAX, |at| at.start);
                        let line_end = self.line_end(span.start);
                        if start <= line_end {
                            let marker = &self.source[span.start..start];
                            self.marked(marker, lines, "    ")
                        } else {
                            // Blocks that start on a later line keep it, and
                            // the lines before it.
                            let (mut written, prefix) =
                                head_and_prefix(&self.source[span.start..start]);
                            let mut lines = prefixed(lines, "    ", "");
                            match lines.first_mut() {
                                Some(first) if !first.full && first.body.starts_with("    ") => {
                                    first.body.replace_range(..4, &prefix.body);
                                    first.full = true;
                                }
                                _ => return Ok(Out::Render),
                            }
                            written.extend(lines);
                            Out::Lines(written)
                        }
                    }
                    Children::Render => Out::Render,
                }
            }
            (Block::ContainerDirective(node), Block::ContainerDirective(base))
                if own_block_fields_equal(block, base_block) =>
            {
                let kept = node.label_source.as_deref();
                if !self.leaf_holds(Some(span), kept, &node.label, &base.label) {
                    return Ok(Out::Render);
                }
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept => Out::Kept(span),
                    Children::Lines(lines, _) => {
                        // An unclosed directive's last line is its last
                        // child's.
                        let children_end = base
                            .children
                            .last()
                            .and_then(Block::span)
                            .map_or(span.start, |at| self.body_end(at));
                        let Some(closing) = self.last_line(span).filter(|_| {
                            self.source[children_end..self.body_end(span)].contains(['\n', '\r'])
                        }) else {
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
            (Block::HtmlContainer(node), Block::HtmlContainer(base))
                if own_block_fields_equal(block, base_block) =>
            {
                match (&node.content, &base.content) {
                    (
                        HtmlContainerContent::Blocks(children),
                        HtmlContainerContent::Blocks(base),
                    ) if !base.is_empty() => match self.children(children, base, Join::Gap)? {
                        Children::Kept => Out::Kept(span),
                        Children::Lines(lines, _) => self.enclosed(span, base, lines),
                        Children::Render => Out::Render,
                    },
                    _ if self.correspondence.holds(block, base_block) => Out::Kept(span),
                    _ => Out::Render,
                }
            }
            (Block::Table(node), Block::Table(base))
                if own_block_fields_equal(block, base_block) =>
            {
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

    /// An HTML container at `span` whose base holds the blocks `base` and
    /// whose children were written as `lines`: the source before its first
    /// block, the opening tag with what follows it, and after its last, the
    /// closing tag with what precedes it, around them.
    fn enclosed(&self, span: Span, base: &[Block], mut lines: Vec<Line>) -> Out {
        let (Some(first), Some(last)) = (
            base.first().and_then(Block::span),
            base.last().and_then(Block::span),
        ) else {
            return Out::Render;
        };
        let (head, tail) = (
            &self.source[span.start..first.start],
            &self.source[self.body_end(last)..self.body_end(span)],
        );
        // The head runs into the first block's line, so it stays only before
        // that line as the source holds it.
        if !tail.contains(['\n', '\r'])
            || matches!(base[0], Block::BlankLine(_))
            || !first_line_holds(self.source, &lines, first)
        {
            return Out::Render;
        }
        let (mut written, prefix) = head_and_prefix(head);
        let first = &mut lines[0];
        first.body.insert_str(0, &prefix.body);
        first.full = prefix.full;
        written.extend(lines);
        let mut closing = split(tail, true, true);
        let ending = closing.remove(0);
        let last = written.last_mut().expect("a block has a line");
        last.body.push_str(&ending.body);
        last.ending = ending.ending;
        written.extend(closing);
        Out::Lines(written)
    }

    /// A block quote at `span` whose base holds `base` and whose children
    /// were written as `lines`: its first line opens with the source's own
    /// marker, the `>` and the space after it, where the base's first block
    /// starts on that line; every other content line takes the canonical
    /// prefix.
    fn quoted(&self, span: Span, base: &[Block], lines: Vec<Line>) -> Out {
        let marker = base
            .first()
            .filter(|first| !matches!(first, Block::BlankLine(_)))
            .and_then(Block::span)
            .filter(|first| first.start <= self.line_end(span.start))
            .map(|first| &self.source[span.start..first.start])
            .filter(|marker| !marker.contains('\t'));
        let mut lines = lines;
        let opened = match (marker, lines.first_mut()) {
            (Some(marker), Some(first)) if !first.full && !first.body.is_empty() => {
                first.body.insert_str(0, marker);
                first.full = true;
                true
            }
            _ => false,
        };
        let mut lines = prefixed(lines, "> ", ">");
        if opened {
            lines[0].full = false;
        }
        Out::Lines(lines)
    }

    /// A container whose first line opens with `marker`, before the first
    /// line of its children, and whose later content lines take `indent`.
    fn marked(&self, marker: &str, lines: Vec<Line>, indent: &str) -> Out {
        let mut lines = lines.into_iter();
        let Some(mut first) = lines.next() else {
            return Out::Render;
        };
        // A first line that opens with indentation or is blank would read
        // differently after the marker than as the canonical writer writes
        // it, as indented code does.
        if first.full || first.body.is_empty() || first.body.starts_with([' ', '\t']) {
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
        // A kept block before a sibling other than its successor in the
        // source stays kept only where a blank line and a paragraph after it
        // would not extend it, as they extend an unclosed fence.
        for index in 0..children.len() {
            let (Out::Kept(span), Some((at, _))) = (&outs[index], matched[index]) else {
                continue;
            };
            let successor = index + 1 == children.len()
                || matched[index + 1].is_some_and(|(next, _)| next == at + 1)
                    && (matches!(children[index + 1], Block::BlankLine(_))
                        || !matches!(outs[index + 1], Out::Render));
            if !successor && !super::closed_by_a_blank_line(&self.unprefixed(*span)) {
                outs[index] = Out::Render;
            }
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
        // The marker of the list written last, with only blank lines after it.
        let mut previous_list: Option<char> = None;
        let mut blank_lines = 0;
        let mut separated = false;
        for (index, (child, out)) in children.iter().zip(outs).enumerate() {
            if let Block::BlankLine(_) = child {
                blank_lines += 1;
                continue;
            }
            let mut at = matched[index]
                .map(|(at, _)| at)
                .filter(|_| !matches!(out, Out::Render));
            let mut written = match out {
                Out::Kept(span) => Some(self.slice(span)),
                Out::Lines(lines) => Some(lines),
                Out::Render => None,
            };
            // The source between this block and the one written before it,
            // kept where they are adjacent in the source, every block
            // between them is a blank line written from there, and the
            // gap reads the same between them as written.
            let gap = match (previous, at, &written) {
                (Some((before, Some(from))), Some(to), Some(written))
                    if to.checked_sub(from) == Some(index - before)
                        && (before + 1..index).all(|between| {
                            matched[between].is_some_and(|(at, _)| at == from + between - before)
                        }) =>
                {
                    let start = self.body_end(base[from].span().expect("matched"));
                    let end = base[to].span().expect("matched").start;
                    Some(&self.source[start..end]).filter(|gap| {
                        gap.starts_with(['\n', '\r'])
                            && keeps_gap(
                                self.source,
                                gap,
                                (
                                    &children[before],
                                    base[from].span().expect("matched"),
                                    Some(&lines),
                                ),
                                (child, base[to].span().expect("matched"), Some(written)),
                                false,
                            )
                    })
                }
                _ => None,
            };
            // A block that does not follow its predecessor in the source
            // reads as there only where nothing before it takes it in.
            if gap.is_none() {
                let last = previous.map(|(before, _)| &children[before]);
                if written.as_ref().is_some_and(|written| {
                    super::moves_into_previous(child, &written[0].body, last, previous_list)
                }) {
                    written = None;
                    at = None;
                }
            }
            let written = match written {
                Some(written) => written,
                None => content(&write_blocks(
                    core::slice::from_ref(child),
                    self.options,
                    Join::Gap,
                    false,
                    previous_list,
                )?),
            };
            previous_list = match child {
                Block::List(list) if at.is_some() => Some(super::written_marker(list)),
                Block::List(list) => Some(super::list_marker(list, self.options, previous_list)),
                _ => None,
            };
            match previous {
                None => {
                    push_empty(&mut lines, blank_lines);
                    extend(&mut lines, written);
                }
                Some(_) => {
                    separated |= gap.map_or(join == Join::Gap || blank_lines > 0, holds_blank_line);
                    match gap {
                        Some(gap) => splice(&mut lines, gap, written),
                        None => {
                            let separator = usize::from(join == Join::Gap);
                            push_empty(&mut lines, separator + blank_lines);
                            extend(&mut lines, written);
                        }
                    }
                }
            }
            blank_lines = 0;
            previous = Some((index, at));
        }
        if blank_lines > 0 {
            separated = true;
            let separator = usize::from(self.separates_trailing_blank_lines(base));
            push_empty(&mut lines, separator + blank_lines);
        }
        Ok(Children::Lines(lines, separated))
    }

    fn list(&mut self, node: &List, base: &List, span: Span) -> Result<Out, SerializeError> {
        let span_of = |item: &ListItem| item.meta.span;
        let mut lines: Vec<Line> = Vec::new();
        let mut all_kept = node.children.len() == base.children.len();
        let mut previous_number: Option<u64> = None;
        // The base index of the item written before, when it was written from
        // the source.
        let mut previous_at: Option<usize> = None;
        let mut previous_holds_separator = false;
        // Whether a blank line separates two items or two blocks of an item,
        // which a loose list needs to read back as loose.
        let mut separated = false;
        let marker_char = super::written_marker(node);
        for (index, item) in node.children.iter().enumerate() {
            let found = matching(item.meta.span, item, &base.children, span_of);
            let (out, item_separated) = match found {
                Some((_, base_item)) => self.item(node, item, base_item, &mut previous_number)?,
                None => (Out::Render, false),
            };
            separated |= match out {
                Out::Kept(_) => item_separated,
                Out::Lines(_) => item_separated,
                // Written canonically, a loose list's item separates its
                // blocks with blank lines.
                Out::Render => {
                    item.children
                        .iter()
                        .filter(|child| !matches!(child, Block::BlankLine(_)))
                        .count()
                        > 1
                        || item
                            .children
                            .iter()
                            .any(|child| matches!(child, Block::BlankLine(_)))
                }
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
                separated |= gap.map_or(!node.tight, holds_blank_line);
                match gap {
                    Some(gap) => splice(&mut lines, gap, written),
                    None => {
                        if !node.tight && !previous_holds_separator {
                            extend(&mut lines, alloc::vec![empty()]);
                        }
                        extend(&mut lines, written);
                    }
                }
            }
            previous_at = at;
            // An item written from the source that ends with blank lines
            // holds the separator to the next item before them.
            previous_holds_separator = found.is_some_and(|(_, base_item)| {
                at.is_some()
                    && matches!(item.children.last(), Some(Block::BlankLine(_)))
                    && self.separates_trailing_blank_lines(&base_item.children)
            });
        }
        if all_kept {
            return Ok(Out::Kept(span));
        }
        if !node.tight && !separated {
            return Ok(Out::Render);
        }
        Ok(Out::Lines(lines))
    }

    /// Writes `item` against `base`, with whether a blank line separates
    /// two of its blocks or follows the last.
    fn item(
        &mut self,
        list: &List,
        item: &ListItem,
        base: &ListItem,
        previous_number: &mut Option<u64>,
    ) -> Result<(Out, bool), SerializeError> {
        let span = item.meta.span.expect("a matched item has a span");
        let own = own_item_fields_equal(item, base);
        let join = if list.tight { Join::Line } else { Join::Gap };
        let children = self.children(&item.children, &base.children, join)?;
        if own && matches!(children, Children::Kept) {
            return Ok((Out::Kept(span), self.separated(&base.children)));
        }
        let Some(first) = first_block(&base.children) else {
            return Ok((Out::Render, false));
        };
        if item.checked.is_some() && !matches!(base.children.first(), Some(Block::Paragraph(_))) {
            return Ok((Out::Render, false));
        }
        let first_start = first.span().map_or(usize::MAX, |at| at.start);
        let line_end = self.line_end(span.start);
        let same_line = first_start <= line_end;
        // A checkbox split across lines is not a marker to copy.
        if base.checked.is_some() && !same_line {
            return Ok((Out::Render, false));
        }
        let marker = if same_line {
            &self.source[span.start..first_start]
        } else {
            self.source[span.start..line_end].trim_end()
        };
        if marker.contains('\t') || !marker.is_ascii() {
            return Ok((Out::Render, false));
        }
        // A task item's content starts at its checkbox, which its first
        // paragraph's text leaves out.
        let column = match marker.find('[').filter(|_| base.checked.is_some()) {
            Some(checkbox) => checkbox,
            None if same_line => marker.len(),
            None => marker.len() + 1,
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
                _ => return Ok((Out::Render, false)),
            }
        };
        let (lines, separated) = match children {
            Children::Kept => {
                let start = first_start;
                let end = self.body_end(span);
                if start > end {
                    return Ok((Out::Render, false));
                }
                (
                    self.source_lines(start, end),
                    self.separated(&base.children),
                )
            }
            Children::Lines(lines, separated) => (lines, separated),
            // An item left without blocks keeps its marker, and with it the
            // column the next item's indentation is measured against.
            Children::Render if item.children.is_empty() => {
                return Ok((Out::Lines(content(marker.trim_end())), false));
            }
            Children::Render => return Ok((Out::Render, false)),
        };
        let indent = " ".repeat(column);
        // A break's own `-` or `*` after the marker would open a list.
        let out = if matches!(first_block(&item.children), Some(Block::ThematicBreak(_))) {
            Out::Render
        } else if same_line {
            self.marked(&marker, lines, &indent)
        } else {
            let mut written = content(&marker);
            extend(&mut written, prefixed(lines, &indent, ""));
            Out::Lines(written)
        };
        Ok((out, separated))
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
    // The marker's indentation stays, and the number after it changes.
    let indent = marker.len() - marker.trim_start_matches(' ').len();
    let mut written = String::from(&marker[indent..]);
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
        if written.get(open + 2..open + 3) != Some("]") {
            return None;
        }
        written.replace_range(open + 1..open + 2, if checked { "x" } else { " " });
    }
    written.insert_str(0, &marker[..indent]);
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

/// Whether two inlines hold the same own fields, children aside: false for
/// two kinds, and for the kinds that hold no children, whose change is a
/// change of their own fields. Each kind is taken apart whole, so that a new
/// field is compared here.
fn own_fields_equal(one: &Inline, other: &Inline) -> bool {
    match one {
        Inline::Emphasis(Emphasis {
            meta: _,
            delimiter,
            children: _,
        }) => matches!(other, Inline::Emphasis(other) if other.delimiter == *delimiter),
        Inline::Strong(Strong {
            meta: _,
            delimiter,
            children: _,
        }) => matches!(other, Inline::Strong(other) if other.delimiter == *delimiter),
        Inline::Delete(Delete {
            meta: _,
            children: _,
        }) => matches!(other, Inline::Delete(_)),
        Inline::Mark(Mark {
            meta: _,
            children: _,
        }) => matches!(other, Inline::Mark(_)),
        Inline::InlineFootnote(InlineFootnote {
            meta: _,
            children: _,
        }) => matches!(other, Inline::InlineFootnote(_)),
        Inline::Link(Link {
            meta: _,
            destination,
            destination_kind,
            title,
            children: _,
        }) => matches!(other, Inline::Link(other)
            if other.destination == *destination
                && other.destination_kind == *destination_kind
                && other.title == *title),
        Inline::Image(Image {
            meta: _,
            destination,
            destination_kind,
            title,
            alt: _,
        }) => matches!(other, Inline::Image(other)
            if other.destination == *destination
                && other.destination_kind == *destination_kind
                && other.title == *title),
        Inline::LinkReference(LinkReference {
            meta: _,
            identifier,
            label,
            kind,
            children: _,
        }) => matches!(other, Inline::LinkReference(other)
            if other.identifier == *identifier && other.label == *label && other.kind == *kind),
        Inline::ImageReference(ImageReference {
            meta: _,
            identifier,
            label,
            kind,
            alt: _,
        }) => matches!(other, Inline::ImageReference(other)
            if other.identifier == *identifier && other.label == *label && other.kind == *kind),
        Inline::TextDirective(TextDirective {
            meta: _,
            name,
            label: _,
            attributes,
        }) => matches!(other, Inline::TextDirective(other)
            if other.name == *name && other.attributes == *attributes),
        Inline::Text(_)
        | Inline::Escape(_)
        | Inline::CharacterReference(_)
        | Inline::Shortcode(_)
        | Inline::Code(_)
        | Inline::Autolink(_)
        | Inline::Html(_)
        | Inline::SoftBreak(_)
        | Inline::LineBreak(_)
        | Inline::Math(_)
        | Inline::FootnoteReference(_)
        | Inline::WikiLink(_)
        | Inline::Tag(_) => false,
    }
}

/// Whether two blocks hold the same own fields, children and a leaf's
/// inlines and kept text aside: false for two kinds, and for the kinds whose
/// own syntax is not copied, which are kept whole or rendered. Each kind is
/// taken apart whole, so that a new field is compared here.
fn own_block_fields_equal(one: &Block, other: &Block) -> bool {
    match one {
        Block::Paragraph(Paragraph {
            meta: _,
            children: _,
            source: _,
        }) => matches!(other, Block::Paragraph(_)),
        Block::Heading(Heading {
            meta: _,
            depth,
            kind,
            children: _,
            source: _,
        }) => {
            matches!(other, Block::Heading(other) if other.depth == *depth && other.kind == *kind)
        }
        Block::BlockQuote(BlockQuote {
            meta: _,
            children: _,
        }) => matches!(other, Block::BlockQuote(_)),
        Block::Alert(Alert {
            meta: _,
            kind,
            title,
            children: _,
        }) => matches!(other, Block::Alert(other) if other.kind == *kind && other.title == *title),
        Block::List(List {
            meta: _,
            ordered,
            delimiter,
            tight,
            children: _,
        }) => matches!(other, Block::List(other)
            if other.ordered == *ordered && other.delimiter == *delimiter && other.tight == *tight),
        Block::FootnoteDefinition(FootnoteDefinition {
            meta: _,
            label,
            identifier,
            children: _,
        }) => matches!(other, Block::FootnoteDefinition(other)
            if other.label == *label && other.identifier == *identifier),
        Block::ContainerDirective(ContainerDirective {
            meta: _,
            name,
            label: _,
            attributes,
            children: _,
            label_source: _,
        }) => matches!(other, Block::ContainerDirective(other)
            if other.name == *name && other.attributes == *attributes),
        Block::Table(Table {
            meta: _,
            alignments,
            rows: _,
        }) => matches!(other, Block::Table(other) if other.alignments == *alignments),
        Block::HtmlContainer(HtmlContainer {
            meta: _,
            opening,
            content,
            closing,
        }) => matches!(other, Block::HtmlContainer(other)
            if tag_fields_equal(&other.opening, opening)
                && tag_fields_equal(&other.closing, closing)
                && discriminant(&other.content) == discriminant(content)),
        Block::ThematicBreak(_)
        | Block::CodeBlock(_)
        | Block::HtmlBlock(_)
        | Block::Definition(_)
        | Block::MathBlock(_)
        | Block::Frontmatter(_)
        | Block::LeafDirective(_)
        | Block::BlankLine(_) => false,
    }
}

/// Whether two tags of an HTML container hold the same fields, their spans
/// aside.
fn tag_fields_equal(one: &HtmlTag, other: &HtmlTag) -> bool {
    let HtmlTag { meta: _, name, raw } = one;
    other.name == *name && other.raw == *raw
}

/// Whether two list items hold the same own fields, children aside.
fn own_item_fields_equal(one: &ListItem, other: &ListItem) -> bool {
    let ListItem {
        meta: _,
        checked,
        number,
        children: _,
    } = one;
    other.checked == *checked && other.number == *number
}
