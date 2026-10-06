//! Block structure, read in one pass over the lines with one stack of open
//! blocks, as CommonMark's block algorithm (and commonmark.js) reads it.
//!
//! Each line is first matched against the open containers from the outside
//! in, each one consuming its marker or indentation. What is left then opens
//! new blocks, continues the open paragraph lazily when the innermost open
//! block is a paragraph the line did not reach, or is added to the open leaf
//! block. A block that a line does not continue is closed with every block
//! inside it. The crate's extension containers (container directives,
//! footnote definitions, HTML containers, and description details) sit on the
//! same stack as block quotes and list items.
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
    options: &SyntaxOptions,
    known: &[String],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Block> {
    let mut parser = BlockParser::new(lines, options);
    for index in 0..lines.len() {
        parser.read_line(index);
    }
    while parser.stack.len() > 1 {
        parser.close_top();
    }
    let document = parser.stack.pop().expect("the document stays open");
    let mut definitions = known.to_vec();
    for child in &document.children {
        collect_definitions(&child.block, &mut definitions);
    }
    // Sorted and deduplicated so `definition_exists` can binary-search.
    definitions.sort_unstable();
    definitions.dedup();
    let mut found = parser.diagnostics;
    let finish = Finish {
        options,
        definitions: &definitions,
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
    /// The byte the containers around the paragraph leave the line at, before
    /// the paragraph's own indentation, and its input position.
    from: usize,
    start: usize,
    /// The columns of that indentation.
    indent: usize,
    /// The line reached the paragraph as a lazy continuation line.
    lazy: bool,
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
    /// their end before they open: without lazy lines, and with them.
    lookahead: Option<Box<Lookahead<'a>>>,
    lazy_lookahead: Option<Box<Lookahead<'a>>>,
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
    DescriptionList(Box<OpenDescriptionList<'a>>),
    Details,
    Paragraph(ParagraphState<'a>),
    Code(CodeState),
    HtmlBlock {
        kind: HtmlBlockKind,
        value: String,
        lines: usize,
    },
    Table(TableState),
    MdxEsm {
        value: String,
        state: MdxEsmState,
        lines: usize,
    },
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

struct OpenDescriptionList<'a> {
    /// Items of a list this one continues after blank lines.
    prior: Vec<PendingDescriptionItem<'a>>,
    prior_tight: bool,
    items: Vec<OpenDescriptionItem>,
}

struct OpenDescriptionItem {
    term: DerivedText,
    start: usize,
    term_last_line: usize,
    /// The first of the list's children that are this item's details.
    details_from: usize,
}

#[derive(Default)]
struct ParagraphState<'a> {
    lines: Vec<ParagraphLine<'a>>,
    /// The first of the trailing lines that reached the paragraph lazily.
    lazy_from: Option<usize>,
    /// The fewest open blocks those lines continued.
    lazy_depth: usize,
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
    mdx: MdxFlowScan,
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
    DescriptionList {
        span: Span,
        tight: bool,
        items: Vec<PendingDescriptionItem<'a>>,
    },
    Details {
        span: Span,
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

/// A description item whose details are closed.
struct PendingDescriptionItem<'a> {
    span: Span,
    term: DerivedText,
    details: Vec<Child<'a>>,
}

impl Frame<'_> {
    fn is_leaf(&self) -> bool {
        matches!(
            self.kind,
            Kind::Paragraph(_)
                | Kind::Code(_)
                | Kind::HtmlBlock { .. }
                | Kind::Table(_)
                | Kind::MdxEsm { .. }
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

    /// Whether the block can hold blocks other than list items and details.
    fn holds_blocks(&self) -> bool {
        matches!(
            self.kind,
            Kind::Document
                | Kind::BlockQuote { .. }
                | Kind::Item { .. }
                | Kind::ContainerDirective(_)
                | Kind::FootnoteDefinition { .. }
                | Kind::HtmlContainer { .. }
                | Kind::Details
        )
    }

    fn can_contain(&self, kind: &Kind<'_>) -> bool {
        match self.kind {
            Kind::List { .. } => matches!(kind, Kind::Item { .. }),
            Kind::DescriptionList(_) => matches!(kind, Kind::Details),
            Kind::Document
            | Kind::BlockQuote { .. }
            | Kind::Item { .. }
            | Kind::ContainerDirective(_)
            | Kind::FootnoteDefinition { .. }
            | Kind::HtmlContainer { .. }
            | Kind::Details => !matches!(kind, Kind::Item { .. } | Kind::Details),
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

struct BlockParser<'a, 'o> {
    lines: &'a [Line<'a>],
    options: &'o SyntaxOptions,
    stack: Vec<Frame<'a>>,
    diagnostics: Vec<Diagnostic>,
    /// The open blocks the current line continued: `stack[..matched]`.
    matched: usize,
    /// Every open block the line did not continue is closed.
    all_closed: bool,
}

impl<'a, 'o> BlockParser<'a, 'o> {
    fn new(lines: &'a [Line<'a>], options: &'o SyntaxOptions) -> Self {
        let start = lines.first().map_or(0, |line| line.start);
        BlockParser {
            lines,
            options,
            stack: alloc::vec![Frame {
                kind: Kind::Document,
                first_line: 0,
                last_line: 0,
                start,
                end: start,
                depth: 0,
                children: Vec::new(),
                lookahead: None,
                lazy_lookahead: None,
            }],
            diagnostics: Vec::new(),
            matched: 1,
            all_closed: true,
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
            let continues = match &self.stack[matched].kind {
                Kind::Document | Kind::List { .. } | Kind::DescriptionList(_) => true,
                Kind::BlockQuote { .. } => quote_continues(&mut cursor),
                Kind::Item { indent, .. } => item_continues(&mut cursor, *indent, has_children),
                Kind::ContainerDirective(_) => {
                    directives.push((matched, cursor));
                    true
                }
                Kind::FootnoteDefinition { .. } | Kind::Details => indent_continues(&mut cursor),
                Kind::HtmlContainer { close_line, .. } => {
                    if *close_line == index {
                        self.extend_frames(&extended, index, line.end_with_eol);
                        self.close_html_container(matched, &cursor, index);
                        return;
                    }
                    true
                }
                _ => unreachable!("leaf blocks are innermost"),
            };
            if !continues {
                break;
            }
            // A list's span is its items'.
            if !blank
                && !matches!(
                    self.stack[matched].kind,
                    Kind::List { .. } | Kind::DescriptionList(_)
                )
            {
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
        let mut block_from = cursor.offset;
        let mut indent = 0;
        let mut fence_like = false;
        while !takes_line {
            block_start = cursor.position();
            block_from = cursor.offset;
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
            self.add_lazy_line(&cursor, index, (block_from, block_start), indent);
            return;
        }

        self.close_unmatched();
        let in_container = self.stack.len() > 2;
        let top = self.stack.len() - 1;
        let options = self.options;
        let frame = &mut self.stack[top];
        match &mut frame.kind {
            Kind::Paragraph(paragraph) => {
                paragraph.lines.push(ParagraphLine {
                    content: cursor.content(),
                    from: block_from,
                    start: block_start,
                    indent,
                    lazy: false,
                    index,
                });
                paragraph.lazy_from = None;
                frame.last_line = index;
                frame.end = line.end;
            }
            Kind::Code(code) => {
                let content = cursor.content();
                content.push_into(&mut code.value);
                code.value.push_str(code_line_ending(&line, in_container));
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
                let row_line = cursor.view(cursor.offset);
                let mut cells = table_row_cells(&row_line, row, options.constructs.spoiler);
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
            Kind::MdxEsm {
                value,
                state,
                lines,
            } => {
                let text = cursor.rest();
                if *lines > 0 {
                    value.push('\n');
                }
                value.push_str(text);
                update_mdx_esm_state(text, state);
                *lines += 1;
                frame.last_line = index;
                frame.end = line.end;
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
                                from: block_from,
                                start: block_start,
                                indent,
                                lazy: false,
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
        (block_from, block_start): (usize, usize),
        indent: usize,
    ) {
        let line = cursor.line;
        let matched = self.matched;
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
                        from: block_from,
                        start: block_start,
                        indent,
                        lazy: true,
                        index,
                    }],
                    lazy_from: Some(0),
                    lazy_depth: matched,
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
            match paragraph.lazy_from {
                Some(_) => paragraph.lazy_depth = paragraph.lazy_depth.min(matched),
                None => {
                    paragraph.lazy_from = Some(paragraph.lines.len());
                    paragraph.lazy_depth = matched;
                }
            }
            paragraph.lines.push(ParagraphLine {
                content: cursor.content(),
                from: block_from,
                start: block_start,
                indent,
                lazy: true,
                index,
            });
        }
    }

    /// Continues the open leaf block with the line, which reached it.
    fn continue_leaf(&mut self, cursor: &mut Cursor<'a>, index: usize) -> LeafStep {
        let line = cursor.line;
        let indented_code = self.options.constructs.indented_code;
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
                        (!cursor.indented() || !indented_code)
                            && fence_close(cursor.nonspace_rest(), *marker, *length),
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
            Kind::MdxEsm { state, .. } => {
                if is_mdx_esm_continuation(cursor.rest(), state) {
                    LeafStep::Matched
                } else {
                    LeafStep::Failed
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
                | Kind::Details
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
            lazy_lookahead: None,
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
        let options = self.options;
        let constructs = &options.constructs;
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

        if index == 0 && self.stack.len() == 1 && constructs.frontmatter {
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
            let alert = if constructs.gfm_alert && !cursor.partial {
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
        if (!indented || !constructs.indented_code) && matches!(byte, Some(b'`' | b'~')) {
            if let Some((marker, length)) = fence_start(cursor.nonspace_rest()) {
                let info = cursor.nonspace_rest()[length..].trim_matches([' ', '\t']);
                let info = (!info.is_empty()).then(|| unescape_string(info));
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
        if constructs.math_block && !indented && byte == Some(b'$') {
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
        if constructs.directive_container && !indented && byte == Some(b':') {
            if let Some((fence, rest)) = directive_container_opener_prefix(cursor.nonspace_rest()) {
                match parse_directive_opener(rest) {
                    Some(opener) if nesting => {
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
                    Some(_) => {}
                    None => {
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
        if constructs.html_container && !indented && nesting && byte == Some(b'<') {
            if let Some(started) = self.start_html_container(cursor, container, index) {
                return started;
            }
        }

        // HTML block.
        if constructs.html_block && !indented && byte == Some(b'<') {
            if let Some(kind) = html_block_start(cursor.nonspace_rest()) {
                if kind != HtmlBlockKind::UntilBlank || !interrupting {
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

        // MDX flow constructs, which interrupt no paragraph.
        if !tip_holds_text {
            if let Some(started) = self.start_mdx(cursor, container, index) {
                return started;
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
        if interrupting
            && constructs.gfm_table
            && (!indented || !constructs.indented_code)
            && matches!(byte, Some(b'|' | b':' | b'-'))
        {
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
        if constructs.footnote_definition && !indented && nesting && byte == Some(b'[') {
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
        if constructs.directive_leaf && !indented && byte == Some(b':') {
            let text = cursor.nonspace_rest();
            if text.starts_with("::") && !text.starts_with(":::") {
                match parse_directive_opener(&text[2..]) {
                    Some(opener) => {
                        if is_blank(&text[2 + opener.consumed..]) {
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
                    }
                    None => {
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

        // Description details.
        if constructs.description_list && cursor.indent <= 2 && matches!(byte, Some(b':' | b'~')) {
            if let Some(started) = self.start_details(cursor, container, index) {
                return started;
            }
        }

        // Indented code.
        if indented
            && constructs.indented_code
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
        let first = cursor.view(cursor.offset);
        let lookahead = self.lookahead(container, index, first, false);
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
        let closing = parse_html_container_tag_line(
            cursor.view(cursor.offset),
            "details",
            HtmlContainerTag::Closing,
        )
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

    fn start_mdx(
        &mut self,
        cursor: &mut Cursor<'a>,
        container: usize,
        index: usize,
    ) -> Option<Started> {
        let options = self.options;
        let constructs = &options.constructs;
        let line = cursor.line;
        let start = cursor.position();
        if constructs.mdx_esm && is_mdx_esm_start(cursor.rest()) {
            self.close_unmatched();
            self.add_frame(
                Kind::MdxEsm {
                    value: String::new(),
                    state: MdxEsmState::default(),
                    lines: 0,
                },
                start,
                index,
                line.end,
            );
            return Some(Started::Leaf);
        }
        let byte = cursor.nonspace_byte();
        let open_byte = cursor.next_nonspace - cursor.offset;
        if constructs.mdx_expression_block && byte == Some(b'{') {
            let first = cursor.view(cursor.offset);
            let lookahead = self.lookahead(container, index, first, true);
            let from = index - lookahead.first;
            let Lookahead {
                lines, mdx, first, ..
            } = lookahead;
            let first = *first;
            match mdx.expression_close(lines, from, open_byte) {
                Some((close, close_byte)) => {
                    let block = Block::MdxExpression(MdxExpression {
                        meta: NodeMeta::new(Some(Span::new(start, lines[close].end))),
                        value: collect_mdx_expression_value(
                            lines, from, open_byte, close, close_byte,
                        ),
                    });
                    let end = lines[close].end;
                    return Some(self.swallow(block, start, index, first + close, end));
                }
                None => {
                    let end = lines
                        .last()
                        .map_or(line.end_with_eol, |last| last.end_with_eol);
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticSeverity::Error,
                        DiagnosticCode::InvalidMdx,
                        Span::new(line.source_start(cursor.next_nonspace), end),
                        "MDX expression block is missing a closing brace",
                    ));
                }
            }
        }
        if constructs.mdx_jsx_block && byte == Some(b'<') {
            let first = cursor.view(cursor.offset);
            let lookahead = self.lookahead(container, index, first, true);
            let from = index - lookahead.first;
            let Lookahead {
                lines, mdx, first, ..
            } = lookahead;
            let first = *first;
            if let Some(close) = mdx.jsx_close_line(lines, from, open_byte) {
                let block = Block::MdxJsx(MdxJsx {
                    meta: NodeMeta::new(Some(Span::new(start, lines[close].end))),
                    value: collect_line_range(lines, from, close),
                });
                let end = lines[close].end;
                return Some(self.swallow(block, start, index, first + close, end));
            }
            if let Some(root) = mdx_jsx_tag_start(lines[from].text, open_byte) {
                if !root.closing && mdx.jsx_tag_self_closing(lines, from, open_byte) == Some(false)
                {
                    let end = lines
                        .last()
                        .map_or(line.end_with_eol, |last| last.end_with_eol);
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticSeverity::Error,
                        DiagnosticCode::InvalidMdx,
                        Span::new(line.source_start(cursor.next_nonspace), end),
                        "MDX JSX block is missing a closing tag",
                    ));
                }
            }
        }
        None
    }

    /// Opens a block read whole, which takes its lines through `until`.
    fn swallow(
        &mut self,
        block: Block,
        start: usize,
        index: usize,
        until: usize,
        end: usize,
    ) -> Started {
        self.close_unmatched();
        self.add_frame(
            Kind::Swallow {
                until,
                block: Some(block),
            },
            start,
            index,
            end,
        );
        let top = self.stack.last_mut().expect("just opened");
        top.last_line = until;
        Started::Leaf
    }

    /// The lines from `index` on that reach the container `container` opens
    /// a block in, each from where the containers leave it: `first` is the
    /// current line. A line that a container does not continue ends them;
    /// lazy lines continue only paragraphs, so none is counted.
    ///
    /// With `lazy`, a line that is not blank also counts where a block quote,
    /// list item, footnote definition, or description details does not
    /// continue on it, as it would continue a paragraph: flow MDX reads such
    /// lines as its own.
    fn lookahead(
        &mut self,
        container: usize,
        index: usize,
        first: Line<'a>,
        lazy: bool,
    ) -> &mut Lookahead<'a> {
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
        let open = &self.stack[frame];
        let valid = if lazy {
            cached(&open.lazy_lookahead)
        } else {
            cached(&open.lookahead)
        };
        if !valid {
            let mut lines = alloc::vec![first];
            for next in index + 1..self.lines.len() {
                match self.reach(frame, next, lazy) {
                    Some(line) => lines.push(line),
                    None => break,
                }
            }
            let built = Some(Box::new(Lookahead {
                first: index,
                lines,
                mdx: MdxFlowScan::default(),
                closes: BracketMemo::default(),
            }));
            if lazy {
                self.stack[frame].lazy_lookahead = built;
            } else {
                self.stack[frame].lookahead = built;
            }
        }
        let open = &mut self.stack[frame];
        if lazy {
            open.lazy_lookahead.as_deref_mut()
        } else {
            open.lookahead.as_deref_mut()
        }
        .expect("the lookahead is built")
    }

    /// Line `index` from where the containers `stack[..=frame]` leave it, when
    /// they all continue on it, or, with `lazy`, when it can be a lazy line.
    fn reach(&self, frame: usize, index: usize, lazy: bool) -> Option<Line<'a>> {
        let mut cursor = Cursor::new(self.lines[index]);
        for open in &self.stack[1..=frame] {
            cursor.find_next_nonspace();
            let continues = match &open.kind {
                Kind::BlockQuote { .. } => quote_continues(&mut cursor),
                Kind::Item { indent, .. } => item_continues(&mut cursor, *indent, true),
                Kind::ContainerDirective(directive) => {
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
                Kind::FootnoteDefinition { .. } | Kind::Details => indent_continues(&mut cursor),
                Kind::HtmlContainer { close_line, .. } => {
                    if *close_line == index {
                        return None;
                    }
                    true
                }
                _ => true,
            };
            if !continues {
                return (lazy && !cursor.blank).then(|| cursor.view(cursor.offset));
            }
        }
        Some(cursor.view(cursor.offset))
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
        let options = self.options;
        let spoiler = options.constructs.spoiler;
        let delimiter = cursor.nonspace_rest();
        // A delimiter row of dashes alone is a setext underline, which wins,
        // and a list marker opens a list.
        if setext_underline_depth(delimiter).is_some() || opens_list_item(delimiter) {
            return None;
        }
        // Definitions the paragraph starts with are not its rows.
        self.take_paragraph_definitions();
        let top = self.stack.last()?;
        let Kind::Paragraph(paragraph) = &top.kind else {
            return None;
        };
        let header = *paragraph.lines.last()?;
        // A header row indented four columns or more is a paragraph's
        // continuation text, as markdown-it and micromark read it.
        if header.indent > 3 && options.constructs.indented_code {
            return None;
        }
        if !table_has_separator(header.text(), delimiter, spoiler) {
            return None;
        }
        let alignments = parse_table_delimiter(delimiter, spoiler)?;
        if split_table_row(header.text(), spoiler).len() != alignments.len() {
            return None;
        }
        let line = cursor.line;
        let header_line = header.content.view();
        let cells = table_row_cells(&header_line, header_line.text, spoiler);
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

    fn start_details(
        &mut self,
        cursor: &mut Cursor<'a>,
        container: usize,
        index: usize,
    ) -> Option<Started> {
        let text = cursor.nonspace_rest();
        if !matches!(text.as_bytes().get(1), None | Some(b' ' | b'\t')) {
            return None;
        }
        // Details hold content: on the marker's line, or on an indented line
        // after it.
        if is_blank(&text[1..]) && !self.indented_line_follows(container, index) {
            return None;
        }
        let line = cursor.line;
        let start = cursor.position();

        if matches!(self.stack[container].kind, Kind::DescriptionList(_)) {
            // The details before did not continue on this line. Lines that
            // reached their paragraph lazily right before it are the next
            // item's term.
            if !self.nesting_allows(container) {
                return None;
            }
            let tail = self.lazy_term_tail(container + 1);
            self.close_unmatched();
            if let Some(term) = tail {
                self.open_description_item(container, &term, index);
            }
        } else {
            let (term, before) = if self.stack[container].is_paragraph() {
                // The open paragraph is the term.
                self.take_paragraph_definitions();
                let Kind::Paragraph(paragraph) = &self.stack[container].kind else {
                    return None;
                };
                if paragraph.lines.is_empty() || !paragraph.lines.iter().all(is_term_line) {
                    return None;
                }
                if !self.nesting_allows(container - 1) {
                    return None;
                }
                let frame = self.stack.pop().expect("the paragraph is open");
                let Kind::Paragraph(paragraph) = frame.kind else {
                    unreachable!("the paragraph is open");
                };
                (paragraph.lines, container - 1)
            } else if container == self.stack.len() - 1 {
                // A paragraph that blank lines closed is the term.
                let Some(Child {
                    block: Pending::Paragraph { lines, .. },
                    ..
                }) = self.stack[container].children.last()
                else {
                    return None;
                };
                if !lines.iter().all(is_term_line) || !self.nesting_allows(container) {
                    return None;
                }
                let Some(Child {
                    block: Pending::Paragraph { lines, .. },
                    ..
                }) = self.stack[container].children.pop()
                else {
                    unreachable!("the paragraph was checked");
                };
                (lines, container)
            } else {
                return None;
            };
            // A list right before, past blank lines, continues.
            let list = match self.stack[before].children.last() {
                Some(Child {
                    block: Pending::DescriptionList { .. },
                    ..
                }) => {
                    let Some(Child {
                        block: Pending::DescriptionList { span, tight, items },
                        first_line,
                        ..
                    }) = self.stack[before].children.pop()
                    else {
                        unreachable!("the list was checked");
                    };
                    Some((span.start, first_line, tight, items))
                }
                _ => None,
            };
            let (list_start, list_line, prior_tight, prior) =
                list.unwrap_or_else(|| (term[0].start, term[0].index, true, Vec::new()));
            self.add_frame(
                Kind::DescriptionList(Box::new(OpenDescriptionList {
                    prior,
                    prior_tight,
                    items: Vec::new(),
                })),
                list_start,
                list_line,
                line.end_with_eol,
            );
            let list = self.stack.len() - 1;
            self.open_description_item(list, &term, index);
        }

        let list = self.stack.len() - 1;
        if !matches!(self.stack[list].kind, Kind::DescriptionList(_)) {
            return None;
        }
        self.add_frame(Kind::Details, start, index, line.end_with_eol);
        cursor.advance_next_nonspace();
        cursor.advance_offset(1, false);
        cursor.find_next_nonspace();
        cursor.advance_next_nonspace();
        Some(Started::Container)
    }

    /// Whether the next line that reaches the containers around `container`,
    /// past blank lines, is indented four columns.
    fn indented_line_follows(&self, container: usize, index: usize) -> bool {
        let frame = if self.stack[container].is_leaf() {
            container - 1
        } else {
            container
        };
        for next in index + 1..self.lines.len() {
            let Some(line) = self.reach(frame, next, false) else {
                return false;
            };
            let cursor = Cursor::new(line);
            if !cursor.blank {
                return cursor.indented();
            }
        }
        false
    }

    /// The lines at the end of the innermost open paragraph that reached it
    /// lazily past the open block `stack[block]`, removed from it, when they
    /// can be a description term.
    fn lazy_term_tail(&mut self, block: usize) -> Option<Vec<ParagraphLine<'a>>> {
        let top = self.stack.last_mut()?;
        let Kind::Paragraph(paragraph) = &mut top.kind else {
            return None;
        };
        let from = paragraph.lazy_from?;
        if paragraph.lazy_depth > block || from == 0 {
            return None;
        }
        let tail = &paragraph.lines[from..];
        if !tail
            .iter()
            .all(|line| line.indent <= 3 && !is_description_marker(line.text()))
        {
            return None;
        }
        let tail = paragraph.lines.split_off(from);
        paragraph.lazy_from = None;
        let last = *paragraph.lines.last().expect("the paragraph keeps a line");
        top.last_line = last.index;
        top.end = last.content.line.end;
        // The blocks the tail reached lazily end where the paragraph now does.
        let count = self.stack.len();
        for frame in &mut self.stack[block..count - 1] {
            frame.last_line = last.index;
            frame.end = last.content.line.end_with_eol;
        }
        Some(tail)
    }

    fn open_description_item(&mut self, list: usize, term: &[ParagraphLine<'a>], _index: usize) {
        let mut text = DerivedText::default();
        for term_line in term {
            let content = term_line.text().trim_end_matches([' ', '\t']);
            text.push_line(&term_line.content.line, content);
        }
        let frame = &mut self.stack[list];
        let details_from = frame.children.len();
        if let Kind::DescriptionList(open) = &mut frame.kind {
            open.items.push(OpenDescriptionItem {
                term: text,
                start: term[0].start,
                term_last_line: term[term.len() - 1].index,
                details_from,
            });
        }
    }

    /// Closes the innermost open block.
    fn close_top(&mut self) {
        let frame = self.stack.pop().expect("the document stays open");
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
                if fresh_item && self.options.constructs.gfm_task_list_item {
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
            Kind::MdxEsm { value, state, .. } => {
                if state_has_open_mdx_esm_construct(&state) {
                    self.diagnostics.push(Diagnostic::new(
                        DiagnosticSeverity::Error,
                        DiagnosticCode::InvalidMdx,
                        Span::new(start, self.lines[last_line].end_with_eol),
                        "MDX ESM block is missing a closing delimiter",
                    ));
                }
                Pending::Done(Block::MdxEsm(MdxEsm {
                    meta: NodeMeta::new(Some(Span::new(start, end))),
                    value,
                }))
            }
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
            Kind::Details => {
                return alloc::vec![Child {
                    block: Pending::Details {
                        span: Span::new(start, children_end),
                        children,
                    },
                    first_line,
                    last_line: children_last_line,
                    end: children_end,
                }];
            }
            Kind::DescriptionList(open) => {
                let OpenDescriptionList {
                    mut prior,
                    prior_tight,
                    items,
                } = *open;
                let mut tight = prior_tight;
                let mut details = children;
                let mut closed = Vec::with_capacity(items.len());
                for item in items.into_iter().rev() {
                    let held = details.split_off(item.details_from.min(details.len()));
                    let end = held.last().map_or(item.start, |child| child.end);
                    if let Some(first) = held.first() {
                        tight &= item.term_last_line + 1 == first.first_line;
                    }
                    tight &= held
                        .windows(2)
                        .all(|pair| pair[0].last_line + 1 == pair[1].first_line);
                    for child in &held {
                        if let Pending::Details { children, .. } = &child.block {
                            tight &= children
                                .windows(2)
                                .all(|pair| pair[0].last_line + 1 == pair[1].first_line);
                        }
                    }
                    closed.push(PendingDescriptionItem {
                        span: Span::new(item.start, end),
                        term: item.term,
                        details: held,
                    });
                }
                closed.reverse();
                let last_line = closed
                    .last()
                    .and_then(|item| item.details.last())
                    .map_or(last_line, |child| child.last_line);
                let end = closed.last().map_or(end, |item| item.span.end);
                prior.extend(closed);
                return alloc::vec![Child {
                    block: Pending::DescriptionList {
                        span: Span::new(start, end),
                        tight,
                        items: prior,
                    },
                    first_line,
                    last_line,
                    end,
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

/// The continuation of a footnote definition or description details: a
/// blank line or a line indented four columns.
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
    let bytes = rest.as_bytes();
    let (ordered, start, delimiter, width) = match *bytes.first()? {
        b'-' => (false, None, ListDelimiter::Dash, 1),
        b'*' => (false, None, ListDelimiter::Asterisk, 1),
        b'+' => (false, None, ListDelimiter::Plus, 1),
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
            let number: u64 = rest[..digits].parse().ok()?;
            if interrupting && number != 1 {
                return None;
            }
            (true, Some(number), delimiter, digits + 1)
        }
        _ => return None,
    };
    if !matches!(bytes.get(width), None | Some(b' ' | b'\t')) {
        return None;
    }
    if interrupting && is_blank(&rest[width..]) {
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

/// Whether a paragraph line can be a line of a description term.
fn is_term_line(line: &ParagraphLine<'_>) -> bool {
    !line.lazy && line.indent <= 3 && !(line.indent <= 2 && is_description_marker(line.text()))
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
    // Continuation lines keep the indentation a label or title holds.
    let views: Vec<Line<'a>> = lines
        .iter()
        .map(|line| view(&line.content.line, line.from.min(line.content.offset)))
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
        | Pending::Details {
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
        Pending::DescriptionList { items, .. } => {
            for item in items {
                children(&item.details, definitions);
            }
        }
        _ => {}
    }
}

/// Parses the inline content of closed blocks.
struct Finish<'o> {
    options: &'o SyntaxOptions,
    definitions: &'o [String],
}

impl Finish<'_> {
    fn inlines(&self, text: &DerivedText, diagnostics: &mut Vec<Diagnostic>) -> Vec<Inline> {
        parse_inlines(
            &text.text,
            text.map(),
            self.options,
            Some(self.definitions),
            diagnostics,
        )
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
                                    self.options,
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
            Pending::Item { span, children, .. } | Pending::Details { span, children } => {
                // Items and details only sit in their lists.
                Block::BlockQuote(BlockQuote {
                    meta: meta(span),
                    children: self.blocks(children, diagnostics),
                })
            }
            Pending::DescriptionList { span, tight, items } => {
                Block::DescriptionList(DescriptionList {
                    meta: meta(span),
                    tight,
                    children: items
                        .into_iter()
                        .map(|item| DescriptionItem {
                            meta: meta(item.span),
                            term: self.inlines(&item.term, diagnostics),
                            details: item
                                .details
                                .into_iter()
                                .filter_map(|child| match child.block {
                                    Pending::Details { span, children } => {
                                        Some(DescriptionDetails {
                                            meta: meta(span),
                                            children: self.blocks(children, diagnostics),
                                        })
                                    }
                                    _ => None,
                                })
                                .collect(),
                        })
                        .collect(),
                })
            }
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
