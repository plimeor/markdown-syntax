//! Block structure, read in one pass over the lines with one stack of open
//! blocks, as CommonMark's block algorithm (and commonmark.js) reads it.
//!
//! Each line is first matched against the open containers from the outside
//! in, each one consuming its marker or indentation. What is left then opens
//! new blocks, continues the open paragraph lazily when the innermost open
//! block is a paragraph the line did not reach, or is added to the open leaf
//! block. A block that a line does not continue is closed with every block
//! inside it. The crate's extension containers (container directives,
//! footnote definitions, and HTML containers) sit on the same stack as block quotes and list items.
//!
//! Inline content is parsed after every line is read, once all link reference
//! definitions are known.

use alloc::boxed::Box;
use core::mem;

use super::*;
use crate::memo::BracketMemo;

/// Reads the blocks of the document split into `lines`.
pub(super) fn parse_document(
    lines: &[Line<'_>],
    known: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Block> {
    let mut parser = BlockParser::new(lines);
    for index in 0..lines.len() {
        parser.read_line(index);
    }
    while parser.stack.len() > 1 {
        parser.close_top();
    }
    let document = parser.stack.pop().expect("the document stays open");
    let mut definitions = Vec::new();
    for child in &document.children {
        collect_definitions(&child.block, &mut definitions);
    }
    // Sorted and deduplicated so `definition_exists` can binary-search.
    definitions.sort_unstable();
    definitions.dedup();
    let mut found = parser.diagnostics;
    let finish = Finish {
        definitions: Definitions {
            own: &definitions,
            known,
        },
    };
    let blocks = document
        .children
        .into_iter()
        .map(|child| finish.block(child.block, &mut found))
        .collect();
    // Block diagnostics are found in the block pass and inline ones after it;
    // both are reported in source order.
    found.sort_by_key(|diagnostic| diagnostic.span.map(|span| span.start));
    diagnostics.extend(found);
    blocks
}

/// Where the parser is in the line it reads: the byte and source column
/// reached, whether the tab at that byte is split, and where the next char
/// other than a space or tab is.
#[derive(Clone, Copy)]
struct Cursor<'a> {
    line: Line<'a>,
    offset: usize,
    column: usize,
    /// The tab at `offset` is split: `column` is inside it.
    partial: bool,
    next_nonspace: usize,
    next_nonspace_column: usize,
    /// The columns from `column` to `next_nonspace_column`.
    indent: usize,
    /// Nothing but spaces and tabs is left.
    blank: bool,
}

impl<'a> Cursor<'a> {
    fn new(line: Line<'a>) -> Self {
        let mut cursor = Cursor {
            line,
            offset: 0,
            column: line.column,
            partial: false,
            next_nonspace: 0,
            next_nonspace_column: line.column,
            indent: 0,
            blank: true,
        };
        cursor.find_next_nonspace();
        cursor
    }

    fn find_next_nonspace(&mut self) {
        let bytes = self.line.text.as_bytes();
        let mut index = self.offset;
        let mut column = self.column;
        while let Some(&byte) = bytes.get(index) {
            match byte {
                b' ' => column += 1,
                b'\t' => column += 4 - column % 4,
                _ => break,
            }
            index += 1;
        }
        self.blank = index == bytes.len();
        self.next_nonspace = index;
        self.next_nonspace_column = column;
        self.indent = column - self.column;
    }

    fn indented(&self) -> bool {
        self.indent >= 4
    }

    fn nonspace_byte(&self) -> Option<u8> {
        self.line.text.as_bytes().get(self.next_nonspace).copied()
    }

    /// The line from its next char other than a space or tab.
    fn nonspace_rest(&self) -> &'a str {
        &self.line.text[self.next_nonspace..]
    }

    /// The line from the byte reached.
    fn rest(&self) -> &'a str {
        &self.line.text[self.offset..]
    }

    fn advance_next_nonspace(&mut self) {
        self.offset = self.next_nonspace;
        self.column = self.next_nonspace_column;
        self.partial = false;
    }

    /// Moves `count` chars on, or `count` columns when `columns` is set, in
    /// which case a tab wider than what is left of `count` is split.
    fn advance_offset(&mut self, mut count: usize, columns: bool) {
        let text = self.line.text;
        while count > 0 {
            let Some(char) = text[self.offset..].chars().next() else {
                break;
            };
            if char == '\t' {
                let to_tab = 4 - self.column % 4;
                if columns {
                    self.partial = to_tab > count;
                    let advance = to_tab.min(count);
                    self.column += advance;
                    if !self.partial {
                        self.offset += 1;
                    }
                    count -= advance;
                } else {
                    self.partial = false;
                    self.column += to_tab;
                    self.offset += 1;
                    count -= 1;
                }
            } else {
                self.partial = false;
                self.offset += char.len_utf8();
                self.column += 1;
                count -= 1;
            }
        }
    }

    /// Moves to byte `offset` of the line, past chars that are not split.
    fn advance_to(&mut self, offset: usize) {
        if self.partial {
            self.advance_offset(1, false);
        }
        self.column = super::advance_columns(self.column, &self.line.text[self.offset..offset]);
        self.offset = offset;
        self.partial = false;
    }

    fn skip_to_end(&mut self) {
        self.advance_to(self.line.text.len());
        self.find_next_nonspace();
    }

    /// The columns a split tab at `offset` still spans.
    fn split(&self) -> usize {
        if self.partial {
            4 - self.column % 4
        } else {
            0
        }
    }

    /// The input position of the byte reached.
    fn position(&self) -> usize {
        self.line.source_start(self.offset)
    }

    fn content(&self) -> Content<'a> {
        Content {
            line: self.line,
            offset: self.offset,
            split: self.split(),
        }
    }

    /// The line from `offset` on, as a line of its own.
    fn view(&self, offset: usize) -> Line<'a> {
        view(&self.line, offset)
    }

    /// The line from the column reached, as a line of its own: a tab split
    /// there spans only the columns left of it.
    fn rest_view(&self) -> Line<'a> {
        let mut line = view(&self.line, self.offset);
        line.column = self.column;
        line
    }
}

/// `line` from byte `offset` of its text on.
fn view<'a>(line: &Line<'a>, offset: usize) -> Line<'a> {
    Line {
        text: &line.text[offset..],
        eol: line.eol,
        start: line.source_start(offset),
        end: line.end,
        end_with_eol: line.end_with_eol,
        segments: line.segments,
        text_offset: line.text_offset + offset,
        column: line.column_at(offset),
    }
}

/// A line's content from where the block parser left it.
#[derive(Clone, Copy)]
struct Content<'a> {
    line: Line<'a>,
    offset: usize,
    /// The columns of a split tab at `offset` that the content starts with,
    /// as spaces.
    split: usize,
}

impl<'a> Content<'a> {
    /// The content, without the columns of a split tab.
    fn text(&self) -> &'a str {
        &self.line.text[self.offset + usize::from(self.split > 0)..]
    }

    fn push_into(&self, value: &mut String) {
        value.extend(core::iter::repeat_n(' ', self.split));
        value.push_str(self.text());
    }

    fn whole(&self) -> Cow<'a, str> {
        if self.split == 0 {
            Cow::Borrowed(self.text())
        } else {
            let mut value = String::new();
            self.push_into(&mut value);
            Cow::Owned(value)
        }
    }

    fn view(&self) -> Line<'a> {
        view(&self.line, self.offset)
    }
}

/// One line of an open paragraph.
#[derive(Clone, Copy)]
struct ParagraphLine<'a> {
    /// The line's text from its first char other than a space or tab.
    content: Content<'a>,
    /// The input position the containers around the paragraph leave the
    /// line at, before the paragraph's own indentation.
    start: usize,
    /// The columns of that indentation.
    indent: usize,
    index: usize,
}

impl<'a> ParagraphLine<'a> {
    fn text(&self) -> &'a str {
        self.content.text()
    }
}

/// An open block.
struct Frame<'a> {
    kind: Kind<'a>,
    first_line: usize,
    /// The last line the block holds, as list tightness counts lines.
    last_line: usize,
    /// The input positions of the block's span so far.
    start: usize,
    end: usize,
    /// The containers this block sits in that count toward
    /// `MAX_BLOCK_NESTING`, itself included.
    depth: usize,
    children: Vec<Child<'a>>,
    /// The lines ahead that reach this container, for blocks that need to see
    /// their end before they open.
    lookahead: Option<Box<Lookahead<'a>>>,
}

enum Kind<'a> {
    Document,
    BlockQuote {
        alert: Option<(AlertKind, Option<String>)>,
        /// The alert's marker line was the last line read: as the first
        /// line of a paragraph, it takes a lazy line.
        marker_open: bool,
    },
    List {
        ordered: bool,
        start: Option<u64>,
        delimiter: ListDelimiter,
    },
    Item {
        /// The columns, from where the item's containers leave its lines, that
        /// a line continuing the item is indented.
        indent: usize,
        checked: Option<bool>,
    },
    ContainerDirective(Box<OpenDirective>),
    FootnoteDefinition {
        label: String,
    },
    HtmlContainer {
        opening: HtmlTag,
        close_line: usize,
        /// The first content line may still be a `<summary>`.
        summary_pending: bool,
    },
    Paragraph(ParagraphState<'a>),
    Code(CodeState),
    HtmlBlock {
        kind: HtmlBlockKind,
        value: String,
        lines: usize,
    },
    Table(TableState),
    /// A block read whole when it opened, which takes its lines up to `until`.
    Swallow {
        until: usize,
        block: Option<Block>,
    },
}

struct OpenDirective {
    name: String,
    label: Option<DerivedText>,
    attributes: Vec<DirectiveAttribute>,
    fence: usize,
    opener: Span,
    closed: bool,
}

#[derive(Default)]
struct ParagraphState<'a> {
    lines: Vec<ParagraphLine<'a>>,
    /// The first of the trailing lines that reached the paragraph lazily.
    lazy_from: Option<usize>,
}

struct CodeState {
    kind: CodeKind,
    value: String,
    /// Indented code: the value's length after its last line that is not
    /// blank, that line, and where it ends.
    content_len: usize,
    content_line: usize,
    content_end: usize,
}

enum CodeKind {
    Fenced {
        marker: FenceMarker,
        length: usize,
        indent: usize,
        info: Option<String>,
    },
    Math {
        length: usize,
        indent: usize,
    },
    Indented,
}

struct TableState {
    alignments: Vec<TableAlignment>,
    rows: Vec<(Span, Vec<TableCellSource>)>,
}

/// The lines ahead that reach a container, each from where the container
/// leaves it, with the lookups made over them.
struct Lookahead<'a> {
    first: usize,
    lines: Vec<Line<'a>>,
    closes: BracketMemo,
}

/// A closed block whose inline content is not parsed yet, and the lines it
/// holds.
struct Child<'a> {
    block: Pending<'a>,
    first_line: usize,
    last_line: usize,
    end: usize,
}

enum Pending<'a> {
    Done(Block),
    Paragraph {
        span: Span,
        lines: Vec<ParagraphLine<'a>>,
    },
    Heading {
        span: Span,
        depth: u8,
        kind: HeadingKind,
        text: DerivedText,
    },
    Table {
        span: Span,
        alignments: Vec<TableAlignment>,
        rows: Vec<(Span, Vec<TableCellSource>)>,
    },
    BlockQuote {
        span: Span,
        alert: Option<(AlertKind, Option<String>)>,
        children: Vec<Child<'a>>,
    },
    List {
        span: Span,
        ordered: bool,
        start: Option<u64>,
        delimiter: ListDelimiter,
        tight: bool,
        items: Vec<Child<'a>>,
    },
    Item {
        span: Span,
        checked: Option<bool>,
        children: Vec<Child<'a>>,
    },
    FootnoteDefinition {
        span: Span,
        label: String,
        children: Vec<Child<'a>>,
    },
    HtmlContainer {
        span: Span,
        opening: HtmlTag,
        closing: HtmlTag,
        children: Vec<Child<'a>>,
    },
    Summary {
        span: Span,
        opening: HtmlTag,
        closing: HtmlTag,
        text: DerivedText,
    },
    LeafDirective {
        span: Span,
        name: String,
        label: Option<DerivedText>,
        attributes: Vec<DirectiveAttribute>,
    },
    ContainerDirective {
        span: Span,
        name: String,
        label: Option<DerivedText>,
        attributes: Vec<DirectiveAttribute>,
        children: Vec<Child<'a>>,
    },
}

impl Frame<'_> {
    fn is_leaf(&self) -> bool {
        matches!(
            self.kind,
            Kind::Paragraph(_)
                | Kind::Code(_)
                | Kind::HtmlBlock { .. }
                | Kind::Table(_)
                | Kind::Swallow { .. }
        )
    }

    fn is_paragraph(&self) -> bool {
        matches!(self.kind, Kind::Paragraph(_))
    }

    /// A paragraph or table, which other blocks can interrupt and which reads
    /// text lines.
    fn holds_text(&self) -> bool {
        matches!(self.kind, Kind::Paragraph(_) | Kind::Table(_))
    }

    /// Whether the block can hold blocks other than list items.
    fn holds_blocks(&self) -> bool {
        matches!(
            self.kind,
            Kind::Document
                | Kind::BlockQuote { .. }
                | Kind::Item { .. }
                | Kind::ContainerDirective(_)
                | Kind::FootnoteDefinition { .. }
                | Kind::HtmlContainer { .. }
        )
    }

    fn can_contain(&self, kind: &Kind<'_>) -> bool {
        match self.kind {
            Kind::List { .. } => matches!(kind, Kind::Item { .. }),
            Kind::Document
            | Kind::BlockQuote { .. }
            | Kind::Item { .. }
            | Kind::ContainerDirective(_)
            | Kind::FootnoteDefinition { .. }
            | Kind::HtmlContainer { .. } => !matches!(kind, Kind::Item { .. }),
            _ => false,
        }
    }

    /// Takes the line `index` ending at `end`.
    fn extend(&mut self, index: usize, end: usize) {
        self.last_line = index;
        self.end = self.end.max(end);
    }
}

/// What a block start did with the line.
enum Started {
    /// Nothing started.
    None,
    /// A container opened; the rest of the line may open more.
    Container,
    /// A leaf block opened, which takes the rest of the line.
    Leaf,
    /// The line is read whole.
    Line,
}

/// What an open leaf block did with a line.
enum LeafStep {
    Matched,
    Failed,
    /// The line ended the block, which took it.
    Done,
}

struct BlockParser<'a> {
    lines: &'a [Line<'a>],
    stack: Vec<Frame<'a>>,
    diagnostics: Vec<Diagnostic>,
    /// The open blocks the current line continued: `stack[..matched]`.
    matched: usize,
    /// Every open block the line did not continue is closed.
    all_closed: bool,
    /// The line follows an alert's marker line, which it continues as a
    /// paragraph's first line, so no indented code interrupts it.
    after_alert_marker: bool,
    /// The lookaheads of closed containers, with what their open blocks
    /// read of a line. A container that
    /// opens later under blocks that read lines alike takes one over, so
    /// containers opening one after another do not each read the lines
    /// ahead again.
    retired: Vec<(Vec<ReachKey>, Box<Lookahead<'a>>)>,
}

/// What `reach` reads of an open block: its kind, and the column or line it
/// depends on.
type ReachKey = (u8, usize);

fn reach_key(kind: &Kind<'_>) -> ReachKey {
    match kind {
        Kind::BlockQuote { .. } => (1, 0),
        Kind::Item { indent, .. } => (2, *indent),
        Kind::ContainerDirective(directive) => (3, directive.fence),
        Kind::FootnoteDefinition { .. } => (4, 0),
        Kind::HtmlContainer { close_line, .. } => (5, *close_line),
        _ => (0, 0),
    }
}

impl<'a> BlockParser<'a> {
    fn new(lines: &'a [Line<'a>]) -> Self {
        let start = lines.first().map_or(0, |line| line.start);
        BlockParser {
            lines,
            stack: alloc::vec![Frame {
                kind: Kind::Document,
                first_line: 0,
                last_line: 0,
                start,
                end: start,
                depth: 0,
                children: Vec::new(),
                lookahead: None,
            }],
            diagnostics: Vec::new(),
            matched: 1,
            all_closed: true,
            after_alert_marker: false,
            retired: Vec::new(),
        }
    }

    fn read_line(&mut self, index: usize) {
        let line = self.lines[index];
        let mut cursor = Cursor::new(line);
        // An alert's marker line ends like a paragraph's first line.
        let alert_open = match self.stack.last_mut().map(|top| &mut top.kind) {
            Some(Kind::BlockQuote { marker_open, .. }) => mem::take(marker_open),
            _ => false,
        };
        self.after_alert_marker = alert_open;
        let leaf = self.stack.last().is_some_and(Frame::is_leaf);
        let containers = self.stack.len() - usize::from(leaf);

        // The open containers, from the outside in.
        let mut matched = 1;
        let mut extended = Vec::new();
        let mut directives = Vec::new();
        while matched < containers {
            cursor.find_next_nonspace();
            let blank = cursor.blank;
            let has_children =
                !self.stack[matched].children.is_empty() || matched + 1 < self.stack.len();
            let continues =
                match continuation(&self.stack[matched].kind, &mut cursor, has_children, index) {
                    Continuation::Continues => true,
                    Continuation::Ends => false,
                    Continuation::Directive => {
                        directives.push((matched, cursor));
                        true
                    }
                    Continuation::HtmlClose => {
                        self.extend_frames(&extended, index, line.end_with_eol);
                        self.close_html_container(matched, &cursor, index);
                        return;
                    }
                };
            if !continues {
                break;
            }
            // A list's span is its items'.
            if !blank && !matches!(self.stack[matched].kind, Kind::List { .. }) {
                extended.push(matched);
            }
            matched += 1;
        }

        // A closing fence closes the innermost container directive it can
        // close, before any block inside it reads the line.
        for &(frame, mut state) in directives.iter().rev() {
            state.find_next_nonspace();
            let Kind::ContainerDirective(directive) = &self.stack[frame].kind else {
                continue;
            };
            if !state.indented()
                && directive_container_closing_fence(state.nonspace_rest(), directive.fence)
                    .is_some()
            {
                let outer: Vec<usize> = extended.iter().copied().filter(|f| *f < frame).collect();
                self.extend_frames(&outer, index, line.end_with_eol);
                while self.stack.len() > frame + 1 {
                    self.close_top();
                }
                let top = self.stack.last_mut().expect("the directive is open");
                if let Kind::ContainerDirective(directive) = &mut top.kind {
                    directive.closed = true;
                }
                top.extend(index, line.end_with_eol);
                self.close_top();
                return;
            }
        }
        self.extend_frames(&extended, index, line.end_with_eol);

        // A block read whole takes its lines lazily too.
        if matched < containers {
            if let Some(Kind::Swallow { until, .. }) = self.stack.last().map(|top| &top.kind) {
                if index <= *until && !cursor.blank {
                    let until = *until;
                    for frame in &mut self.stack {
                        frame.extend(index, line.end_with_eol);
                    }
                    if index == until {
                        self.close_top();
                    }
                    return;
                }
            }
        }

        // The open leaf block.
        let mut all_closed = matched == containers;
        if all_closed && leaf {
            cursor.find_next_nonspace();
            match self.continue_leaf(&mut cursor, index) {
                LeafStep::Matched => matched += 1,
                LeafStep::Failed => all_closed = false,
                LeafStep::Done => return,
            }
        }
        self.matched = matched;
        self.all_closed = all_closed;

        // New blocks.
        let mut container = matched - 1;
        let mut takes_line = self.stack[container].is_leaf() && !self.stack[container].holds_text();
        let mut block_start = cursor.position();
        let mut indent = 0;
        let mut fence_like = false;
        while !takes_line {
            block_start = cursor.position();
            cursor.find_next_nonspace();
            indent = cursor.indent;
            match self.start(&mut cursor, container, index) {
                Started::None => {
                    fence_like =
                        !cursor.indented() && matches!(cursor.nonspace_byte(), Some(b'`' | b'~'));
                    cursor.advance_next_nonspace();
                    break;
                }
                Started::Container => container = self.stack.len() - 1,
                Started::Leaf => {
                    container = self.stack.len() - 1;
                    takes_line = true;
                }
                Started::Line => return,
            }
        }

        // A lazy continuation line: a paragraph is open where the line did not
        // reach. A fence-like line ends a block quote's paragraph instead
        // (GH-19).
        if !self.all_closed
            && !cursor.blank
            && (alert_open || self.stack.last().is_some_and(Frame::is_paragraph))
            && !(fence_like
                && self.stack[self.matched..]
                    .iter()
                    .any(|frame| matches!(frame.kind, Kind::BlockQuote { .. })))
        {
            self.add_lazy_line(&cursor, index, block_start, indent);
            return;
        }

        self.close_unmatched();
        let in_container = self.stack.len() > 2;
        let top = self.stack.len() - 1;
        let frame = &mut self.stack[top];
        match &mut frame.kind {
            Kind::Paragraph(paragraph) => {
                paragraph.lines.push(ParagraphLine {
                    content: cursor.content(),
                    start: block_start,
                    indent,
                    index,
                });
                paragraph.lazy_from = None;
                frame.last_line = index;
                frame.end = line.end;
            }
            Kind::Code(code) => {
                let content = cursor.content();
                content.push_into(&mut code.value);
                // A last line without a line ending still ends one, empty or
                // not, as every line of the value does.
                if line.eol.is_empty() {
                    code.value.push_str(value_line_ending(&code.value));
                } else {
                    code.value.push_str(code_line_ending(&line, in_container));
                }
                if let CodeKind::Indented = code.kind {
                    if !is_blank(&content.whole()) {
                        code.content_len = code.value.len();
                        code.content_line = index;
                        code.content_end = line.end_with_eol;
                    }
                }
                frame.extend(index, line.end_with_eol);
            }
            Kind::HtmlBlock { kind, value, lines } => {
                let content = cursor.content().whole();
                if *lines > 0 {
                    value.push('\n');
                }
                value.push_str(&content);
                *lines += 1;
                let ends = match kind {
                    HtmlBlockKind::RawTag => ["script", "pre", "style", "textarea"]
                        .iter()
                        .any(|tag| line_contains_raw_closing_tag(&content, tag)),
                    HtmlBlockKind::Until(end) => content.contains(*end),
                    HtmlBlockKind::BlockTag | HtmlBlockKind::UntilBlank => false,
                };
                frame.extend(index, line.end_with_eol);
                if ends {
                    self.close_top();
                }
            }
            Kind::Table(table) => {
                let row = cursor.rest();
                let row_line = cursor.rest_view();
                let mut cells = table_row_cells(&row_line, row);
                cells.truncate(table.alignments.len());
                while cells.len() < table.alignments.len() {
                    cells.push(TableCellSource {
                        text: DerivedText::default(),
                        escaped_pipes: Vec::new(),
                        span: Span::new(line.end, line.end),
                    });
                }
                table.rows.push((Span::new(block_start, line.end), cells));
                frame.extend(index, line.end_with_eol);
            }
            Kind::Swallow { until, .. } => {
                if *until == index {
                    self.close_top();
                }
            }
            _ => {
                if !cursor.blank {
                    let content = cursor.content();
                    self.add_frame(
                        Kind::Paragraph(ParagraphState {
                            lines: alloc::vec![ParagraphLine {
                                content,
                                start: block_start,
                                indent,
                                index,
                            }],
                            ..ParagraphState::default()
                        }),
                        block_start,
                        index,
                        line.end,
                    );
                }
            }
        }
    }

    fn extend_frames(&mut self, frames: &[usize], index: usize, end: usize) {
        for &frame in frames {
            self.stack[frame].extend(index, end);
        }
    }

    fn add_lazy_line(
        &mut self,
        cursor: &Cursor<'a>,
        index: usize,
        block_start: usize,
        indent: usize,
    ) {
        let line = cursor.line;
        let top = self.stack.len() - 1;
        for frame in &mut self.stack[..top] {
            frame.extend(index, line.end_with_eol);
        }
        let frame = &mut self.stack[top];
        if !frame.is_paragraph() {
            // The first line of an alert's paragraph.
            frame.extend(index, line.end_with_eol);
            let content = cursor.content();
            self.add_frame(
                Kind::Paragraph(ParagraphState {
                    lines: alloc::vec![ParagraphLine {
                        content,
                        start: block_start,
                        indent,
                        index,
                    }],
                    lazy_from: Some(0),
                }),
                block_start,
                index,
                line.end,
            );
            return;
        }
        frame.last_line = index;
        frame.end = line.end;
        if let Kind::Paragraph(paragraph) = &mut frame.kind {
            if paragraph.lazy_from.is_none() {
                paragraph.lazy_from = Some(paragraph.lines.len());
            }
            paragraph.lines.push(ParagraphLine {
                content: cursor.content(),
                start: block_start,
                indent,
                index,
            });
        }
    }

    /// Continues the open leaf block with the line, which reached it.
    fn continue_leaf(&mut self, cursor: &mut Cursor<'a>, index: usize) -> LeafStep {
        let line = cursor.line;
        let top = self.stack.len() - 1;
        let frame = &mut self.stack[top];
        match &mut frame.kind {
            Kind::Paragraph(_) | Kind::Table(_) => {
                if cursor.blank {
                    LeafStep::Failed
                } else {
                    LeafStep::Matched
                }
            }
            Kind::Code(code) => {
                let (closes, indent) = match &code.kind {
                    CodeKind::Fenced {
                        marker,
                        length,
                        indent,
                        ..
                    } => (
                        !cursor.indented() && fence_close(cursor.nonspace_rest(), *marker, *length),
                        *indent,
                    ),
                    CodeKind::Math { length, indent } => (
                        !cursor.indented()
                            && math_block_fence_closes(cursor.nonspace_rest(), *length),
                        *indent,
                    ),
                    CodeKind::Indented => {
                        return if cursor.indented() {
                            cursor.advance_offset(4, true);
                            LeafStep::Matched
                        } else if cursor.blank {
                            cursor.advance_next_nonspace();
                            LeafStep::Matched
                        } else {
                            LeafStep::Failed
                        };
                    }
                };
                if closes {
                    frame.extend(index, line.end_with_eol);
                    self.close_top();
                    return LeafStep::Done;
                }
                // The opening fence's indentation is removed from each line.
                let mut left = indent;
                while left > 0
                    && matches!(line.text.as_bytes().get(cursor.offset), Some(b' ' | b'\t'))
                {
                    cursor.advance_offset(1, true);
                    left -= 1;
                }
                LeafStep::Matched
            }
            Kind::HtmlBlock { kind, .. } => {
                if cursor.blank
                    && matches!(kind, HtmlBlockKind::BlockTag | HtmlBlockKind::UntilBlank)
                {
                    LeafStep::Failed
                } else {
                    LeafStep::Matched
                }
            }
            Kind::Swallow { until, .. } => {
                if index <= *until {
                    LeafStep::Matched
                } else {
                    LeafStep::Failed
                }
            }
            _ => unreachable!("a container is no leaf"),
        }
    }

    /// Closes the blocks the line did not continue.
    fn close_unmatched(&mut self) {
        if !self.all_closed {
            while self.stack.len() > self.matched {
                self.close_top();
            }
            self.all_closed = true;
        }
    }

    /// Opens a block in the innermost open block that can hold it.
    fn add_frame(&mut self, kind: Kind<'a>, start: usize, index: usize, end: usize) {
        while !self.stack.last().is_some_and(|top| top.can_contain(&kind)) {
            self.close_top();
        }
        let parent = self.stack.last().expect("the document stays open");
        let nests = matches!(
            kind,
            Kind::BlockQuote { .. }
                | Kind::Item { .. }
                | Kind::ContainerDirective(_)
                | Kind::FootnoteDefinition { .. }
                | Kind::HtmlContainer { .. }
        );
        let depth = parent.depth + usize::from(nests);
        self.stack.push(Frame {
            kind,
            first_line: index,
            last_line: index,
            start,
            end,
            depth,
            children: Vec::new(),
            lookahead: None,
        });
    }

    /// Adds a closed block to the innermost open block that can hold it.
    fn add_block(&mut self, block: Pending<'a>, index: usize, end: usize) {
        while !self.stack.last().is_some_and(Frame::holds_blocks) {
            self.close_top();
        }
        let top = self.stack.last_mut().expect("the document stays open");
        top.children.push(Child {
            block,
            first_line: index,
            last_line: index,
            end,
        });
    }

    /// The nesting depth a container opened in `container` reaches.
    fn nesting_allows(&self, container: usize) -> bool {
        self.stack[container].depth < MAX_BLOCK_NESTING
    }

    /// Tries each block start on the rest of the line, in order.
    fn start(&mut self, cursor: &mut Cursor<'a>, container: usize, index: usize) -> Started {
        let line = cursor.line;
        let start = cursor.position();
        let interrupting = self.stack[container].is_paragraph();
        let tip_holds_text = self.stack.last().is_some_and(Frame::holds_text);
        let nesting = self.nesting_allows(container);
        let indented = cursor.indented();

        // The first content line of an HTML container may be its summary.
        let summary_pending = match &mut self.stack[container].kind {
            Kind::HtmlContainer {
                summary_pending, ..
            } if !cursor.blank => mem::take(summary_pending),
            _ => false,
        };
        if summary_pending && !indented {
            let source = cursor.view(cursor.next_nonspace);
            if let Some(summary) = summary_parts(&source, source.text, 0) {
                self.close_unmatched();
                let top = self.stack.last_mut().expect("the container is open");
                top.children.push(Child {
                    block: summary,
                    first_line: index,
                    last_line: index,
                    end: line.end,
                });
                top.extend(index, line.end_with_eol);
                return Started::Line;
            }
        }

        if index == 0 && self.stack.len() == 1 {
            if let Some(started) = self.start_frontmatter(index) {
                return started;
            }
        }

        if cursor.blank {
            return Started::None;
        }
        let byte = cursor.nonspace_byte();

        // Block quote.
        if !indented && byte == Some(b'>') && nesting {
            cursor.advance_next_nonspace();
            cursor.advance_offset(1, false);
            if matches!(line.text.as_bytes().get(cursor.offset), Some(b' ' | b'\t')) {
                cursor.advance_offset(1, true);
            }
            self.close_unmatched();
            let alert = if !cursor.partial {
                parse_alert_marker(cursor.rest())
            } else {
                None
            };
            let alerted = alert.is_some();
            self.add_frame(
                Kind::BlockQuote {
                    alert,
                    marker_open: alerted,
                },
                start,
                index,
                line.end_with_eol,
            );
            if alerted {
                cursor.skip_to_end();
            }
            return Started::Container;
        }

        // ATX heading.
        if !indented && byte == Some(b'#') {
            if let Some((depth, content)) = atx_heading(cursor.nonspace_rest()) {
                let source = cursor.view(cursor.next_nonspace);
                let mut text = DerivedText::default();
                text.append(&source, content);
                self.close_unmatched();
                self.add_block(
                    Pending::Heading {
                        span: Span::new(start, line.end),
                        depth,
                        kind: HeadingKind::Atx,
                        text,
                    },
                    index,
                    line.end,
                );
                return Started::Line;
            }
        }

        // Fenced code.
        if !indented && matches!(byte, Some(b'`' | b'~')) {
            if let Some((marker, length)) = fence_start(cursor.nonspace_rest()) {
                let info = cursor.nonspace_rest()[length..].trim_matches([' ', '\t']);
                let info = (!info.is_empty()).then(|| decode_escapes_and_references(info));
                let indent = cursor.indent;
                self.close_unmatched();
                self.add_frame(
                    Kind::Code(CodeState {
                        kind: CodeKind::Fenced {
                            marker,
                            length,
                            indent,
                            info,
                        },
                        value: String::new(),
                        content_len: 0,
                        content_line: index,
                        content_end: line.end_with_eol,
                    }),
                    start,
                    index,
                    line.end_with_eol,
                );
                return Started::Line;
            }
        }

        // Math block.
        if !indented && byte == Some(b'$') {
            if let Some(length) = math_block_fence_length(cursor.nonspace_rest()) {
                let indent = cursor.indent;
                self.close_unmatched();
                self.add_frame(
                    Kind::Code(CodeState {
                        kind: CodeKind::Math { length, indent },
                        value: String::new(),
                        content_len: 0,
                        content_line: index,
                        content_end: line.end_with_eol,
                    }),
                    start,
                    index,
                    line.end_with_eol,
                );
                return Started::Line;
            }
        }

        // Container directive.
        if !indented && byte == Some(b':') {
            if let Some((fence, rest)) = directive_container_opener_prefix(cursor.nonspace_rest()) {
                match parse_directive_opener(rest, |_, _, _| true) {
                    Ok(opener) if nesting => {
                        let source = cursor.view(cursor.next_nonspace);
                        let label = opener.label.map(|label| {
                            let mut text = DerivedText::default();
                            text.append(&source, label);
                            text
                        });
                        let at = cursor.next_nonspace + fence;
                        opener.report_dropped(&mut self.diagnostics, |start, end| {
                            Span::new(line.source_start(at + start), line.source_end(at + end))
                        });
                        let DirectiveOpener {
                            name, attributes, ..
                        } = opener;
                        self.close_unmatched();
                        self.add_frame(
                            Kind::ContainerDirective(Box::new(OpenDirective {
                                name,
                                label,
                                attributes,
                                fence,
                                opener: Span::new(start, line.end),
                                closed: false,
                            })),
                            start,
                            index,
                            line.end_with_eol,
                        );
                        cursor.skip_to_end();
                        return Started::Container;
                    }
                    Ok(_) => {}
                    // A bad name or an unclosed `[` / `{`: nothing is refused
                    // for what follows the opener.
                    Err(_) => {
                        if !tip_holds_text {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticSeverity::Error,
                                DiagnosticCode::InvalidDirectiveName,
                                Span::new(start, line.end),
                                "container directive must have a valid name",
                            ));
                        }
                    }
                }
            }
        }

        // HTML container.
        if !indented && nesting && byte == Some(b'<') {
            if let Some(started) = self.start_html_container(cursor, container, index) {
                return started;
            }
        }

        // HTML block.
        if !indented && byte == Some(b'<') {
            if let Some(kind) = html_block_start(cursor.nonspace_rest()) {
                if !interrupting || html_block_may_interrupt(kind) {
                    self.close_unmatched();
                    self.add_frame(
                        Kind::HtmlBlock {
                            kind,
                            value: String::new(),
                            lines: 0,
                        },
                        start,
                        index,
                        line.end_with_eol,
                    );
                    return Started::Leaf;
                }
            }
        }

        // Setext heading.
        if interrupting && !indented && matches!(byte, Some(b'=' | b'-')) {
            if let Some(depth) = setext_underline_depth(cursor.nonspace_rest()) {
                if let Some(started) = self.start_setext(depth, index) {
                    return started;
                }
            }
        }

        // Table.
        if interrupting && !indented && matches!(byte, Some(b'|' | b':' | b'-')) {
            if let Some(started) = self.start_table(cursor, index) {
                return started;
            }
        }

        // Thematic break.
        if !indented && matches!(byte, Some(b'*' | b'-' | b'_')) {
            if let Some(Block::ThematicBreak(ThematicBreak { marker, .. })) =
                parse_thematic_break(Line::detached(cursor.nonspace_rest()))
            {
                self.close_unmatched();
                self.add_block(
                    Pending::Done(Block::ThematicBreak(ThematicBreak {
                        meta: NodeMeta::new(Some(Span::new(start, line.end))),
                        marker,
                    })),
                    index,
                    line.end,
                );
                return Started::Line;
            }
        }

        // Footnote definition.
        if !indented && nesting && byte == Some(b'[') {
            let text = cursor.nonspace_rest();
            if text.starts_with("[^") {
                if let Some(close) = find_footnote_definition_label_end(text) {
                    let label = &text[2..close];
                    if is_footnote_label(label) {
                        let label = String::from(label);
                        self.close_unmatched();
                        self.add_frame(
                            Kind::FootnoteDefinition { label },
                            start,
                            index,
                            line.end_with_eol,
                        );
                        cursor.advance_to(cursor.next_nonspace + close + 2);
                        cursor.find_next_nonspace();
                        cursor.advance_next_nonspace();
                        return Started::Container;
                    }
                }
            }
        }

        // List item.
        if (!indented || matches!(self.stack[container].kind, Kind::List { .. })) && nesting {
            if let Some(marker) = list_marker(cursor, interrupting) {
                self.close_unmatched();
                let matches_list = matches!(
                    self.stack.last().map(|top| &top.kind),
                    Some(Kind::List { ordered, delimiter, .. })
                        if *ordered == marker.ordered && *delimiter == marker.delimiter
                );
                if !matches_list {
                    self.add_frame(
                        Kind::List {
                            ordered: marker.ordered,
                            start: marker.start,
                            delimiter: marker.delimiter,
                        },
                        start,
                        index,
                        line.end_with_eol,
                    );
                }
                self.add_frame(
                    Kind::Item {
                        indent: marker.indent,
                        checked: None,
                    },
                    start,
                    index,
                    line.end_with_eol,
                );
                return Started::Container;
            }
        }

        // Leaf directive.
        if !indented && byte == Some(b':') {
            let text = cursor.nonspace_rest();
            if text.starts_with("::") && !text.starts_with(":::") {
                // Only whitespace may follow a leaf directive's opener.
                match parse_directive_opener(&text[2..], |_, _, rest| is_blank(rest)) {
                    Ok(opener) => {
                        let source = cursor.view(cursor.next_nonspace);
                        let label = opener.label.map(|label| {
                            let mut derived = DerivedText::default();
                            derived.append(&source, label);
                            derived
                        });
                        let at = cursor.next_nonspace + 2;
                        opener.report_dropped(&mut self.diagnostics, |start, end| {
                            Span::new(line.source_start(at + start), line.source_end(at + end))
                        });
                        let DirectiveOpener {
                            name, attributes, ..
                        } = opener;
                        self.close_unmatched();
                        self.add_block(
                            Pending::LeafDirective {
                                span: Span::new(start, line.end),
                                name,
                                label,
                                attributes,
                            },
                            index,
                            line.end,
                        );
                        return Started::Line;
                    }
                    Err(Refused::Follow) => {}
                    Err(Refused::Name | Refused::Unclosed) => {
                        if !tip_holds_text {
                            self.diagnostics.push(Diagnostic::new(
                                DiagnosticSeverity::Error,
                                DiagnosticCode::InvalidDirectiveName,
                                Span::new(start, line.end),
                                "leaf directive must have a valid name",
                            ));
                        }
                    }
                }
            }
        }

        // Indented code.
        if indented
            && !self.after_alert_marker
            && !self.stack.last().is_some_and(Frame::is_paragraph)
        {
            cursor.advance_offset(4, true);
            self.close_unmatched();
            self.add_frame(
                Kind::Code(CodeState {
                    kind: CodeKind::Indented,
                    value: String::new(),
                    content_len: 0,
                    content_line: index,
                    content_end: line.end_with_eol,
                }),
                start,
                index,
                line.end_with_eol,
            );
            return Started::Leaf;
        }

        Started::None
    }

    fn start_frontmatter(&mut self, index: usize) -> Option<Started> {
        let lines = self.lines;
        let kind = frontmatter_fence_kind(lines[index].text)?;
        let close = (index + 1..lines.len())
            .find(|&cursor| frontmatter_fence_kind(lines[cursor].text) == Some(kind))?;
        let mut value = String::new();
        for line in &lines[index + 1..close] {
            push_line(&mut value, line.text);
        }
        let block = Block::Frontmatter(Frontmatter {
            meta: NodeMeta::new(Some(Span::new(
                lines[index].start,
                lines[close].end_with_eol,
            ))),
            kind,
            value,
        });
        let end = lines[close].end_with_eol;
        self.add_frame(
            Kind::Swallow {
                until: close,
                block: Some(block),
            },
            lines[index].start,
            index,
            end,
        );
        self.stack.last_mut().expect("just opened").last_line = close;
        Some(Started::Leaf)
    }

    fn start_html_container(
        &mut self,
        cursor: &mut Cursor<'a>,
        container: usize,
        index: usize,
    ) -> Option<Started> {
        let line = cursor.line;
        let source = cursor.view(cursor.next_nonspace);
        let (opening, summary) = details_opening(&source)?;
        let first = cursor.rest_view();
        let lookahead = self.lookahead(container, index, first);
        let from = index - lookahead.first;
        let Lookahead { lines, closes, .. } = lookahead;
        let close = closes.resolve(lines.len() + 1, from + 1, |cursor| {
            html_container_close_step(lines, cursor, "details")
        })?;
        let close_line = lookahead.first + close;
        let start = opening.meta.span.map_or(line.start, |span| span.start);
        self.close_unmatched();
        self.add_frame(
            Kind::HtmlContainer {
                opening,
                close_line,
                summary_pending: summary.is_none(),
            },
            start,
            index,
            line.end_with_eol,
        );
        if let Some(summary) = summary {
            self.stack
                .last_mut()
                .expect("just opened")
                .children
                .push(Child {
                    block: summary,
                    first_line: index,
                    last_line: index,
                    end: line.end,
                });
        }
        cursor.skip_to_end();
        Some(Started::Container)
    }

    fn close_html_container(&mut self, frame: usize, cursor: &Cursor<'a>, index: usize) {
        while self.stack.len() > frame + 1 {
            self.close_top();
        }
        let line = cursor.line;
        let top = self.stack.pop().expect("the container is open");
        let Kind::HtmlContainer { opening, .. } = top.kind else {
            unreachable!("an HTML container closes here");
        };
        let closing =
            parse_html_container_tag_line(cursor.rest_view(), "details", HtmlContainerTag::Closing)
                .unwrap_or_else(|| closing_details_tag(line.end_with_eol));
        let span = Span::new(top.start, line.end_with_eol);
        let parent = self.stack.last_mut().expect("the document stays open");
        parent.children.push(Child {
            block: Pending::HtmlContainer {
                span,
                opening,
                closing,
                children: top.children,
            },
            first_line: top.first_line,
            last_line: index,
            end: line.end_with_eol,
        });
    }

    /// The lines from `index` on that reach the container `container` opens
    /// a block in, each from where the containers leave it: `first` is the
    /// current line. A line that a container does not continue ends them;
    /// lazy lines continue only paragraphs, so none is counted.
    fn lookahead(&mut self, container: usize, index: usize, first: Line<'a>) -> &mut Lookahead<'a> {
        let frame = if self.stack[container].is_leaf() {
            container - 1
        } else {
            container
        };
        let cached = |lookahead: &Option<Box<Lookahead<'a>>>| {
            lookahead.as_ref().is_some_and(|lookahead| {
                lookahead.first <= index && index < lookahead.first + lookahead.lines.len()
            })
        };
        if !cached(&self.stack[frame].lookahead) {
            let keys: Vec<ReachKey> = self.stack[1..=frame]
                .iter()
                .map(|open| reach_key(&open.kind))
                .collect();
            let reusable = self.retired.iter().position(|(retired_keys, lookahead)| {
                *retired_keys == keys
                    && index >= lookahead.first
                    && lookahead
                        .lines
                        .get(index - lookahead.first)
                        .is_some_and(|line| {
                            core::ptr::eq(line.text, first.text) && line.column == first.column
                        })
            });
            if let Some(position) = reusable {
                let (_, lookahead) = self.retired.swap_remove(position);
                self.stack[frame].lookahead = Some(lookahead);
            }
        }
        if !cached(&self.stack[frame].lookahead) {
            let mut lines = alloc::vec![first];
            for next in index + 1..self.lines.len() {
                match self.reach(frame, next) {
                    Some(line) => lines.push(line),
                    None => break,
                }
            }
            self.stack[frame].lookahead = Some(Box::new(Lookahead {
                first: index,
                lines,
                closes: BracketMemo::default(),
            }));
        }
        self.stack[frame]
            .lookahead
            .as_deref_mut()
            .expect("the lookahead is built")
    }

    /// Line `index` from where the containers `stack[..=frame]` leave it, when
    /// they all continue on it.
    fn reach(&self, frame: usize, index: usize) -> Option<Line<'a>> {
        let mut cursor = Cursor::new(self.lines[index]);
        for open in &self.stack[1..=frame] {
            cursor.find_next_nonspace();
            // A line past the one being read follows content in each item.
            let continues = match continuation(&open.kind, &mut cursor, true, index) {
                Continuation::Continues => true,
                Continuation::Ends => false,
                Continuation::Directive => {
                    let Kind::ContainerDirective(directive) = &open.kind else {
                        unreachable!("a directive continues as one");
                    };
                    if !cursor.indented()
                        && directive_container_closing_fence(
                            cursor.nonspace_rest(),
                            directive.fence,
                        )
                        .is_some()
                    {
                        return None;
                    }
                    true
                }
                Continuation::HtmlClose => return None,
            };
            if !continues {
                return None;
            }
        }
        Some(cursor.rest_view())
    }

    fn start_setext(&mut self, depth: u8, index: usize) -> Option<Started> {
        let line = self.lines[index];
        self.take_paragraph_definitions();
        let top = self.stack.last()?;
        let Kind::Paragraph(paragraph) = &top.kind else {
            return None;
        };
        if paragraph.lines.is_empty() {
            return None;
        }
        let frame = self.stack.pop().expect("the paragraph is open");
        let Kind::Paragraph(paragraph) = frame.kind else {
            unreachable!("the paragraph is open");
        };
        let mut text = DerivedText::default();
        for paragraph_line in &paragraph.lines {
            push_paragraph_line(&mut text, paragraph_line);
        }
        // CommonMark strips the final whitespace of a heading's content.
        text.trim_final_whitespace();
        let start = paragraph.lines[0].start;
        let parent = self.stack.last_mut().expect("the document stays open");
        parent.children.push(Child {
            block: Pending::Heading {
                span: Span::new(start, line.end),
                depth,
                kind: HeadingKind::Setext,
                text,
            },
            first_line: paragraph.lines[0].index,
            last_line: index,
            end: line.end,
        });
        Some(Started::Line)
    }

    /// Moves the link reference definitions the open paragraph starts with
    /// into its parent, ahead of it.
    fn take_paragraph_definitions(&mut self) {
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        let Kind::Paragraph(paragraph) = &mut top.kind else {
            return;
        };
        let (definitions, used) = take_definitions(&paragraph.lines);
        if used == 0 {
            return;
        }
        paragraph.lines.drain(..used);
        if let Some(lazy_from) = &mut paragraph.lazy_from {
            *lazy_from = lazy_from.saturating_sub(used);
        }
        if let Some(first) = paragraph.lines.first() {
            top.start = first.start;
            top.first_line = first.index;
        }
        let parent = self.stack.len() - 2;
        self.stack[parent].children.extend(definitions);
    }

    fn start_table(&mut self, cursor: &Cursor<'a>, index: usize) -> Option<Started> {
        let delimiter = cursor.nonspace_rest();
        // A delimiter row of dashes alone is a setext underline, which wins,
        // and a list marker opens a list.
        if setext_underline_depth(delimiter).is_some() || opens_list_item(delimiter) {
            return None;
        }
        let Kind::Paragraph(paragraph) = &self.stack.last()?.kind else {
            return None;
        };
        let header = *paragraph.lines.last()?;
        // A header row indented four columns or more is a paragraph's
        // continuation text, as markdown-it and micromark read it.
        if header.indent > 3 {
            return None;
        }
        if !table_has_separator(header.text(), delimiter) {
            return None;
        }
        let alignments = parse_table_delimiter(delimiter)?;
        if table_row_cell_ranges(header.text()).len() != alignments.len() {
            return None;
        }
        // Definitions the paragraph starts with are not its rows. They are
        // taken only once the rows would form a table, so a paragraph's
        // lines are not read again for every line that cannot start one.
        self.take_paragraph_definitions();
        let Kind::Paragraph(paragraph) = &self.stack.last()?.kind else {
            return None;
        };
        if paragraph.lines.last().map(|line| line.index) != Some(header.index) {
            return None;
        }
        let line = cursor.line;
        let header_line = header.content.view();
        let cells = table_row_cells(&header_line, header_line.text);
        let header_row = (Span::new(header.start, header.content.line.end), cells);

        // The header row leaves the paragraph, which closes with what is left.
        let top = self.stack.last_mut().expect("the paragraph is open");
        if let Kind::Paragraph(paragraph) = &mut top.kind {
            paragraph.lines.pop();
            if let Some(last) = paragraph.lines.last() {
                top.last_line = last.index;
                top.end = last.content.line.end;
            }
        }
        self.close_top();
        self.add_frame(
            Kind::Table(TableState {
                alignments,
                rows: alloc::vec![header_row],
            }),
            header.start,
            header.index,
            line.end_with_eol,
        );
        self.stack.last_mut().expect("just opened").last_line = index;
        Some(Started::Line)
    }

    /// Closes the innermost open block.
    fn close_top(&mut self) {
        let mut frame = self.stack.pop().expect("the document stays open");
        if let Some(lookahead) = frame.lookahead.take() {
            let mut keys: Vec<ReachKey> = self.stack[1..]
                .iter()
                .map(|open| reach_key(&open.kind))
                .collect();
            keys.push(reach_key(&frame.kind));
            self.retired.clear();
            self.retired.push((keys, lookahead));
        }
        let fresh_item = matches!(
            self.stack.last().map(|top| &top.kind),
            Some(Kind::Item { .. })
        ) && self.stack.last().is_some_and(|top| top.children.is_empty());
        let mut checked = None;
        let children = self.finish_frame(frame, fresh_item, &mut checked);
        let parent = self.stack.last_mut().expect("the document stays open");
        if let (Some(value), Kind::Item { checked, .. }) = (checked, &mut parent.kind) {
            *checked = Some(value);
        }
        parent.children.extend(children);
    }

    fn finish_frame(
        &mut self,
        frame: Frame<'a>,
        fresh_item: bool,
        checked: &mut Option<bool>,
    ) -> Vec<Child<'a>> {
        let Frame {
            kind,
            first_line,
            last_line,
            start,
            end,
            children,
            ..
        } = frame;
        let children_end = children.last().map_or(end, |child| child.end.max(end));
        let children_last_line = children.last().map_or(first_line, |child| child.last_line);
        let block = match kind {
            Kind::Document => unreachable!("the document stays open"),
            Kind::Paragraph(paragraph) => {
                let mut lines = paragraph.lines;
                let (mut found, used) = take_definitions(&lines);
                lines.drain(..used);
                if lines.is_empty() {
                    return found;
                }
                if fresh_item {
                    *checked = take_task_marker(&mut lines);
                }
                let first = &lines[0];
                let start = if checked.is_some() {
                    first.content.line.source_start(first.content.offset)
                } else {
                    first.start
                };
                let last = &lines[lines.len() - 1];
                let (first_line, last_line, end) = (first.index, last.index, last.content.line.end);
                found.push(Child {
                    block: Pending::Paragraph {
                        span: Span::new(start, end),
                        lines,
                    },
                    first_line,
                    last_line,
                    end,
                });
                return found;
            }
            Kind::Code(code) => {
                let CodeState {
                    kind,
                    mut value,
                    content_len,
                    content_line,
                    content_end,
                } = code;
                let (kind, info, end, last_line) = match kind {
                    CodeKind::Fenced {
                        marker,
                        length,
                        info,
                        ..
                    } => (
                        Some(CodeBlockKind::Fenced { marker, length }),
                        info,
                        end,
                        last_line,
                    ),
                    CodeKind::Math { .. } => (None, None, end, last_line),
                    CodeKind::Indented => {
                        // Blank lines after the last line are not the block's.
                        value.truncate(content_len);
                        (
                            Some(CodeBlockKind::Indented),
                            None,
                            content_end,
                            content_line,
                        )
                    }
                };
                end_last_line(&mut value);
                let meta = NodeMeta::new(Some(Span::new(start, end)));
                let block = match kind {
                    Some(kind) => Block::CodeBlock(CodeBlock {
                        meta,
                        kind,
                        info,
                        value,
                    }),
                    None => Block::MathBlock(MathBlock { meta, value }),
                };
                return alloc::vec![Child {
                    block: Pending::Done(block),
                    first_line,
                    last_line,
                    end,
                }];
            }
            Kind::HtmlBlock { value, .. } => Pending::Done(Block::HtmlBlock(HtmlBlock {
                meta: NodeMeta::new(Some(Span::new(start, end))),
                value,
            })),
            Kind::Table(table) => Pending::Table {
                span: Span::new(start, end),
                alignments: table.alignments,
                rows: table.rows,
            },
            Kind::Swallow { block, .. } => Pending::Done(block.expect("a swallowed block")),
            Kind::BlockQuote { alert, .. } => {
                return alloc::vec![Child {
                    block: Pending::BlockQuote {
                        span: Span::new(start, children_end),
                        alert,
                        children,
                    },
                    first_line,
                    last_line,
                    end: children_end,
                }];
            }
            Kind::List {
                ordered,
                start: number,
                delimiter,
            } => {
                let tight = list_is_tight(&children);
                let (first_line, last_line) = (
                    children.first().map_or(first_line, |item| item.first_line),
                    children_last_line,
                );
                let span_start = match children.first() {
                    Some(Child {
                        block: Pending::Item { span, .. },
                        ..
                    }) => span.start,
                    _ => start,
                };
                return alloc::vec![Child {
                    block: Pending::List {
                        span: Span::new(span_start, children_end),
                        ordered,
                        start: number,
                        delimiter,
                        tight,
                        items: children,
                    },
                    first_line,
                    last_line,
                    end: children_end,
                }];
            }
            Kind::Item { checked, .. } => {
                return alloc::vec![Child {
                    block: Pending::Item {
                        span: Span::new(start, children_end),
                        checked,
                        children,
                    },
                    first_line,
                    last_line: children_last_line,
                    end: children_end,
                }];
            }
            Kind::FootnoteDefinition { label } => Pending::FootnoteDefinition {
                span: Span::new(start, children_end),
                label,
                children,
            },
            Kind::HtmlContainer { opening, .. } => Pending::HtmlContainer {
                span: Span::new(start, children_end),
                closing: closing_details_tag(children_end),
                opening,
                children,
            },
            Kind::ContainerDirective(directive) => {
                let OpenDirective {
                    name,
                    label,
                    attributes,
                    opener,
                    closed,
                    ..
                } = *directive;
                if !closed {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticSeverity::Error,
                        DiagnosticCode::UnclosedDirectiveContainer,
                        opener,
                        "container directive is missing a closing fence",
                    ));
                }
                Pending::ContainerDirective {
                    span: Span::new(start, children_end),
                    name,
                    label,
                    attributes,
                    children,
                }
            }
        };
        alloc::vec![Child {
            block,
            first_line,
            last_line: last_line.max(children_last_line),
            end: children_end,
        }]
    }
}

fn quote_continues(cursor: &mut Cursor<'_>) -> bool {
    if cursor.indented() || cursor.nonspace_byte() != Some(b'>') {
        return false;
    }
    cursor.advance_next_nonspace();
    cursor.advance_offset(1, false);
    if matches!(
        cursor.line.text.as_bytes().get(cursor.offset),
        Some(b' ' | b'\t')
    ) {
        cursor.advance_offset(1, true);
    }
    true
}

/// A list item's continuation: a blank line when the item holds a block, or
/// a line indented to the item's content.
/// What an open container does with a line.
enum Continuation {
    /// The container continues, its markers read.
    Continues,
    /// The container does not continue.
    Ends,
    /// A container directive, which continues unless its closing fence is on
    /// the line; the caller checks that once the containers inside it read.
    Directive,
    /// An HTML container whose closing line this is.
    HtmlClose,
}

/// What the open container `kind` does with line `index`, reading its markers
/// from `cursor`, which is at the line's first nonspace char; `has_children`
/// is whether a list item holds content already.
fn continuation(
    kind: &Kind<'_>,
    cursor: &mut Cursor<'_>,
    has_children: bool,
    index: usize,
) -> Continuation {
    let continues = match kind {
        Kind::Document | Kind::List { .. } => true,
        Kind::BlockQuote { .. } => quote_continues(cursor),
        Kind::Item { indent, .. } => item_continues(cursor, *indent, has_children),
        Kind::ContainerDirective(_) => return Continuation::Directive,
        Kind::FootnoteDefinition { .. } => indent_continues(cursor),
        Kind::HtmlContainer { close_line, .. } => {
            if *close_line == index {
                return Continuation::HtmlClose;
            }
            true
        }
        _ => unreachable!("leaf blocks are innermost"),
    };
    if continues {
        Continuation::Continues
    } else {
        Continuation::Ends
    }
}

fn item_continues(cursor: &mut Cursor<'_>, indent: usize, has_children: bool) -> bool {
    if cursor.blank {
        if !has_children {
            return false;
        }
        cursor.advance_next_nonspace();
    } else if cursor.indent >= indent {
        cursor.advance_offset(indent, true);
    } else {
        return false;
    }
    true
}

/// The continuation of a footnote definition: a blank line or a line
/// indented four columns.
fn indent_continues(cursor: &mut Cursor<'_>) -> bool {
    if cursor.blank {
        cursor.advance_next_nonspace();
    } else if cursor.indented() {
        cursor.advance_offset(4, true);
    } else {
        return false;
    }
    true
}

struct ListMarker {
    ordered: bool,
    start: Option<u64>,
    delimiter: ListDelimiter,
    /// The columns a line continuing the item is indented.
    indent: usize,
}

/// The list item marker the cursor is at, which it moves past along with the
/// spaces that make the item's content indentation. A marker interrupting a
/// paragraph needs content and, when ordered, the number 1.
fn list_marker(cursor: &mut Cursor<'_>, interrupting: bool) -> Option<ListMarker> {
    if cursor.indented() {
        return None;
    }
    let rest = cursor.nonspace_rest();
    let (delimiter, width) = list_marker_head(rest)?;
    let ordered = matches!(delimiter, ListDelimiter::Period | ListDelimiter::Paren);
    let start = if ordered {
        Some(rest[..width - 1].parse().ok()?)
    } else {
        None
    };
    if interrupting && !marker_may_interrupt(start, is_blank(&rest[width..])) {
        return None;
    }
    let marker_offset = cursor.indent;
    cursor.advance_next_nonspace();
    cursor.advance_offset(width, true);
    let spaces_column = cursor.column;
    let spaces_offset = cursor.offset;
    loop {
        cursor.advance_offset(1, true);
        let next = cursor.line.text.as_bytes().get(cursor.offset);
        if cursor.column - spaces_column >= 5 || !matches!(next, Some(b' ' | b'\t')) {
            break;
        }
    }
    let blank_item = cursor.offset >= cursor.line.text.len();
    let spaces = cursor.column - spaces_column;
    let padding = if !(1..5).contains(&spaces) || blank_item {
        // Content indented five columns or more is indented code; the item's
        // content starts one column after its marker.
        cursor.column = spaces_column;
        cursor.offset = spaces_offset;
        cursor.partial = false;
        if matches!(
            cursor.line.text.as_bytes().get(cursor.offset),
            Some(b' ' | b'\t')
        ) {
            cursor.advance_offset(1, true);
        }
        width + 1
    } else {
        width + spaces
    };
    Some(ListMarker {
        ordered,
        start,
        delimiter,
        indent: marker_offset + padding,
    })
}

/// Whether a list item marker numbered `number` (`None` for a bullet), with
/// `blank` saying whether nothing follows it on its line, may interrupt a
/// paragraph: it needs content and, when ordered, the number 1.
fn marker_may_interrupt(number: Option<u64>, blank: bool) -> bool {
    !blank && number.is_none_or(|number| number == 1)
}

/// Whether an HTML block of `kind` may interrupt a paragraph: every kind but
/// a lone tag's, which ends at a blank line.
fn html_block_may_interrupt(kind: HtmlBlockKind) -> bool {
    kind != HtmlBlockKind::UntilBlank
}

/// Whether `block`, written on the line right after a paragraph's, starts
/// there instead of continuing the paragraph, by the checks `start` makes on
/// a line that would interrupt one: paragraph text continues the paragraph,
/// a definition starts only a paragraph, indented code and a frontmatter
/// fence cannot start in one (`---` underlines it), a list needs its first
/// item's marker to interrupt, and an HTML block its first line. Whether a
/// heading is written as a setext heading, whose text line would continue
/// the paragraph, is the serializer's choice and is not judged here.
pub(crate) fn interrupts_paragraph(block: &Block) -> bool {
    match block {
        Block::Paragraph(_) | Block::Definition(_) | Block::Frontmatter(_) => false,
        Block::CodeBlock(node) => node.kind != CodeBlockKind::Indented,
        Block::List(list) => {
            let number = list.ordered.then(|| list.start.unwrap_or(1));
            let blank = list
                .children
                .first()
                .is_none_or(|item| item.children.is_empty());
            marker_may_interrupt(number, blank)
        }
        Block::HtmlBlock(node) => {
            let first_line = node.value.split(['\n', '\r']).next().unwrap_or_default();
            // The columns a leading tab reaches depend on where the block is
            // written, so the indentation is not judged.
            html_block_start(trim_ascii_start(first_line)).is_some_and(html_block_may_interrupt)
        }
        _ => true,
    }
}

/// The delimiter and width of the list item marker `rest` opens with, when
/// a space, a tab, or the line's end follows it.
pub(super) fn list_marker_head(rest: &str) -> Option<(ListDelimiter, usize)> {
    let bytes = rest.as_bytes();
    let (delimiter, width) = match *bytes.first()? {
        b'-' => (ListDelimiter::Dash, 1),
        b'*' => (ListDelimiter::Asterisk, 1),
        b'+' => (ListDelimiter::Plus, 1),
        byte if byte.is_ascii_digit() => {
            let digits = bytes
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits > 9 {
                return None;
            }
            let delimiter = match bytes.get(digits)? {
                b'.' => ListDelimiter::Period,
                b')' => ListDelimiter::Paren,
                _ => return None,
            };
            (delimiter, digits + 1)
        }
        _ => return None,
    };
    matches!(bytes.get(width), None | Some(b' ' | b'\t')).then_some((delimiter, width))
}

/// The depth and content of the ATX heading `text` opens.
fn atx_heading(text: &str) -> Option<(u8, &str)> {
    let depth = text.bytes().take_while(|byte| *byte == b'#').count();
    if depth == 0 || depth > 6 {
        return None;
    }
    if !matches!(text.as_bytes().get(depth), None | Some(b' ' | b'\t')) {
        return None;
    }
    let content = trim_closing_hashes(trim_ascii_start(&text[depth..]));
    Some((depth as u8, content))
}

/// The `<details>` opening tag `line` holds, and the `<summary>` element after
/// it on the line, if any.
fn details_opening<'a>(line: &Line<'a>) -> Option<(HtmlTag, Option<Pending<'a>>)> {
    let text = line.text;
    let (open_end, name) = parse_html_tag(text, 0)?;
    if !name.eq_ignore_ascii_case("details")
        || html_tag_is_closing(text, 0)
        || html_tag_is_self_closing(&text[..open_end])
    {
        return None;
    }
    let rest_start = open_end + leading_ascii_whitespace_len(&text[open_end..]);
    let summary = if text[rest_start..].is_empty() {
        None
    } else {
        Some(summary_parts(line, &text[rest_start..], rest_start)?)
    };
    Some((
        HtmlTag {
            meta: NodeMeta::new(Some(Span::new(
                line.source_start(0),
                line.source_end(open_end),
            ))),
            name: "details".into(),
            raw: text[..open_end].into(),
        },
        summary,
    ))
}

/// A `<summary>…</summary>` element in `source`, the text of `line` from byte
/// `offset` on.
fn summary_parts<'a>(line: &Line<'a>, source: &'a str, offset: usize) -> Option<Pending<'a>> {
    let (open_end, open_name) = parse_html_tag(source, 0)?;
    if !open_name.eq_ignore_ascii_case("summary")
        || html_tag_is_closing(source, 0)
        || html_tag_is_self_closing(&source[..open_end])
    {
        return None;
    }
    let close_start = source[open_end..]
        .find("</")
        .map(|found| open_end + found)?;
    let (close_end, close_name) = parse_html_tag(source, close_start)?;
    if !close_name.eq_ignore_ascii_case("summary")
        || !html_tag_is_closing(source, close_start)
        || !is_blank(&source[close_end..])
    {
        return None;
    }
    let span = |start: usize, end: usize| {
        Span::new(
            line.source_start(offset + start),
            line.source_end(offset + end),
        )
    };
    let mut text = DerivedText::default();
    text.append(line, &source[open_end..close_start]);
    Some(Pending::Summary {
        span: span(0, source.len()),
        opening: HtmlTag {
            meta: NodeMeta::new(Some(span(0, open_end))),
            name: "summary".into(),
            raw: source[..open_end].into(),
        },
        closing: HtmlTag {
            meta: NodeMeta::new(Some(span(close_start, close_end))),
            name: "summary".into(),
            raw: source[close_start..close_end].into(),
        },
        text,
    })
}

/// The closing tag of an HTML container closed without its closing line.
fn closing_details_tag(at: usize) -> HtmlTag {
    HtmlTag {
        meta: NodeMeta::new(Some(Span::new(at, at))),
        name: "details".into(),
        raw: "</details>".into(),
    }
}

/// The line ending a code or math block's value keeps for `line`: the
/// source's at the top level, and `\n` inside a container.
fn code_line_ending<'a>(line: &Line<'a>, in_container: bool) -> &'a str {
    if line.eol.is_empty() || !in_container {
        line.eol
    } else {
        "\n"
    }
}

fn push_paragraph_line(text: &mut DerivedText, line: &ParagraphLine<'_>) {
    text.push_line(&line.content.line, line.text());
}

/// The link reference definitions a paragraph's lines start with, and how
/// many lines they take.
fn take_definitions<'a>(lines: &[ParagraphLine<'a>]) -> (Vec<Child<'a>>, usize) {
    if !lines
        .first()
        .is_some_and(|line| line.text().starts_with('['))
    {
        return (Vec::new(), 0);
    }
    // Each line is read from its first char other than a space or tab, as
    // a paragraph's lines are.
    let views: Vec<Line<'a>> = lines
        .iter()
        .map(|line| view(&line.content.line, line.content.offset))
        .collect();
    let mut found = Vec::new();
    let mut index = 0;
    while index < views.len() && lines[index].text().starts_with('[') {
        let Some((mut definition, next)) = parse_definition(&views, index) else {
            break;
        };
        let end = lines[next - 1].content.line.end_with_eol;
        definition.meta = NodeMeta::new(Some(Span::new(lines[index].start, end)));
        found.push(Child {
            block: Pending::Done(Block::Definition(definition)),
            first_line: lines[index].index,
            last_line: lines[next - 1].index,
            end,
        });
        index = next;
    }
    (found, index)
}

/// Takes the task checkbox a list item's first paragraph starts with, which
/// is part of the item's marker: `[ ]`, `[x]`, or `[X]` before a space or tab
/// or the line's end, or `[` ending the line before `]` or `x]`. The item
/// needs content after it.
fn take_task_marker(lines: &mut Vec<ParagraphLine<'_>>) -> Option<bool> {
    let first = lines[0].text();
    if let Some(checked) = task_marker_checked(first) {
        let after = &first[3..];
        if after.starts_with([' ', '\t']) && !is_blank(&after[1..]) {
            lines[0].content.offset += 4;
            return Some(checked);
        }
        if is_blank(after) && lines.len() > 1 {
            lines.remove(0);
            return Some(checked);
        }
        return None;
    }
    if first.trim_end_matches([' ', '\t']) == "[" && lines.len() > 1 {
        let second = lines[1].text();
        let (checked, consumed) = match second.as_bytes() {
            [b']', b' ' | b'\t', ..] => (false, 2),
            [b'x' | b'X', b']', b' ' | b'\t', ..] => (true, 3),
            _ => return None,
        };
        if is_blank(&second[consumed..]) && lines.len() < 3 {
            return None;
        }
        lines.remove(0);
        lines[0].content.offset += consumed;
        return Some(checked);
    }
    None
}

/// Whether a list is tight: no blank line between two of its items, or
/// between two blocks of one item.
fn list_is_tight(items: &[Child<'_>]) -> bool {
    items.iter().enumerate().all(|(position, item)| {
        let next_follows = items
            .get(position + 1)
            .is_none_or(|next| item.last_line + 1 == next.first_line);
        let children_follow = match &item.block {
            Pending::Item { children, .. } => children
                .windows(2)
                .all(|pair| pair[0].last_line + 1 == pair[1].first_line),
            _ => true,
        };
        next_follows && children_follow
    })
}

fn collect_definitions(block: &Pending<'_>, definitions: &mut Vec<String>) {
    let children = |children: &[Child<'_>], definitions: &mut Vec<String>| {
        for child in children {
            collect_definitions(&child.block, definitions);
        }
    };
    match block {
        Pending::Done(Block::Definition(definition)) => {
            definitions.push(definition.identifier.clone());
        }
        Pending::BlockQuote {
            children: inner, ..
        }
        | Pending::Item {
            children: inner, ..
        }
        | Pending::FootnoteDefinition {
            children: inner, ..
        }
        | Pending::HtmlContainer {
            children: inner, ..
        }
        | Pending::ContainerDirective {
            children: inner, ..
        } => children(inner, definitions),
        Pending::List { items, .. } => children(items, definitions),
        _ => {}
    }
}

/// Parses the inline content of closed blocks.
struct Finish<'o> {
    definitions: Definitions<'o>,
}

impl Finish<'_> {
    fn inlines(&self, text: &DerivedText, diagnostics: &mut Vec<Diagnostic>) -> Vec<Inline> {
        parse_inlines(&text.text, text.map(), Some(self.definitions), diagnostics)
    }

    fn blocks(&self, children: Vec<Child<'_>>, diagnostics: &mut Vec<Diagnostic>) -> Vec<Block> {
        children
            .into_iter()
            .map(|child| self.block(child.block, diagnostics))
            .collect()
    }

    fn block(&self, block: Pending<'_>, diagnostics: &mut Vec<Diagnostic>) -> Block {
        let meta = |span: Span| NodeMeta::new(Some(span));
        match block {
            Pending::Done(block) => block,
            Pending::Paragraph { span, lines } => {
                let mut text = DerivedText::default();
                for line in &lines {
                    push_paragraph_line(&mut text, line);
                }
                // CommonMark strips the final whitespace of a paragraph.
                text.trim_final_whitespace();
                Block::Paragraph(Paragraph {
                    meta: meta(span),
                    children: self.inlines(&text, diagnostics),
                })
            }
            Pending::Heading {
                span,
                depth,
                kind,
                text,
            } => Block::Heading(Heading {
                meta: meta(span),
                depth,
                kind,
                children: self.inlines(&text, &mut Vec::new()),
            }),
            Pending::Table {
                span,
                alignments,
                rows,
            } => Block::Table(Table {
                meta: meta(span),
                alignments,
                rows: rows
                    .into_iter()
                    .map(|(span, cells)| TableRow {
                        meta: meta(span),
                        cells: cells
                            .into_iter()
                            .map(|cell| TableCell {
                                meta: meta(cell.span),
                                children: parse_cell_inlines(
                                    &cell.text.text,
                                    cell.text.map(),
                                    cell.escaped_pipes,
                                    Some(self.definitions),
                                    diagnostics,
                                ),
                            })
                            .collect(),
                    })
                    .collect(),
            }),
            Pending::BlockQuote {
                span,
                alert,
                children,
            } => {
                let children = self.blocks(children, diagnostics);
                match alert {
                    Some((kind, title)) => Block::Alert(Alert {
                        meta: meta(span),
                        kind,
                        title,
                        children,
                    }),
                    None => Block::BlockQuote(BlockQuote {
                        meta: meta(span),
                        children,
                    }),
                }
            }
            Pending::List {
                span,
                ordered,
                start,
                delimiter,
                tight,
                items,
            } => Block::List(List {
                meta: meta(span),
                ordered,
                start,
                delimiter,
                tight,
                children: items
                    .into_iter()
                    .filter_map(|item| match item.block {
                        Pending::Item {
                            span,
                            checked,
                            children,
                        } => Some(ListItem {
                            meta: meta(span),
                            checked,
                            children: self.blocks(children, diagnostics),
                        }),
                        _ => None,
                    })
                    .collect(),
            }),
            Pending::Item { .. } => unreachable!("items only sit in their lists"),
            Pending::FootnoteDefinition {
                span,
                label,
                children,
            } => Block::FootnoteDefinition(FootnoteDefinition {
                meta: meta(span),
                identifier: normalize_label(&label),
                label,
                children: self.blocks(children, diagnostics),
            }),
            Pending::HtmlContainer {
                span,
                opening,
                closing,
                children,
            } => Block::HtmlContainer(HtmlContainer {
                meta: meta(span),
                opening,
                content: HtmlContainerContent::Blocks(self.blocks(children, diagnostics)),
                closing,
            }),
            Pending::Summary {
                span,
                opening,
                closing,
                text,
            } => Block::HtmlContainer(HtmlContainer {
                meta: meta(span),
                opening,
                content: HtmlContainerContent::Inlines(self.inlines(&text, diagnostics)),
                closing,
            }),
            Pending::LeafDirective {
                span,
                name,
                label,
                attributes,
            } => Block::LeafDirective(LeafDirective {
                meta: meta(span),
                name,
                label: label
                    .map(|label| self.inlines(&label, diagnostics))
                    .unwrap_or_default(),
                attributes,
            }),
            Pending::ContainerDirective {
                span,
                name,
                label,
                attributes,
                children,
            } => {
                let label = label
                    .map(|label| self.inlines(&label, diagnostics))
                    .unwrap_or_default();
                Block::ContainerDirective(ContainerDirective {
                    meta: meta(span),
                    name,
                    label,
                    attributes,
                    children: self.blocks(children, diagnostics),
                })
            }
        }
    }
}
