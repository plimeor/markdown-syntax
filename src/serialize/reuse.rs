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
    /// As these lines, a list whose last item is not the base's last item
    /// written from the source: indented source after it can read as that
    /// item's content at a column the source did not give it.
    OpenList(Vec<Line>),
    /// As canonical output.
    Render,
}

/// One written line: its text, its line ending (empty on the last line),
/// whether it already holds every enclosing container's prefix, as a line
/// copied from the source does, or takes them as content, and whether its
/// ending is the source's, which the options' line ending leaves as it is.
#[derive(Clone, Debug)]
pub(super) struct Line {
    pub(super) body: String,
    pub(super) ending: String,
    pub(super) full: bool,
    pub(super) copied_ending: bool,
}

/// `lines` as text, each ending the writer gave in `line_ending`.
pub(super) fn join(lines: &[Line], line_ending: &str) -> String {
    let mut output = String::new();
    for line in lines {
        output.push_str(&line.body);
        output.push_str(match line.ending.as_str() {
            "\n" if !line.copied_ending => line_ending,
            ending => ending,
        });
    }
    output
}

/// The lines of `text`, split at `\n`, `\r\n`, and `\r` as the canonical
/// writer's prefixing splits them: the first takes `first_full`, every later
/// one `rest_full`. A final line ending ends the last line. `copied` where
/// `text` is the source's.
fn split(text: &str, first_full: bool, rest_full: bool, copied: bool) -> Vec<Line> {
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
                    copied_ending: copied,
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
            copied_ending: copied,
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
    split(text, false, false, false)
}

/// One empty content line.
fn empty() -> Line {
    Line {
        body: String::new(),
        ending: String::new(),
        full: false,
        copied_ending: false,
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
            last.copied_ending = false;
        }
    }
    lines.extend(more);
}

/// The lines of `head`, the source before a container's first block from
/// the container's start, and the piece of it that opens the block's first
/// line: content when that line is the container's first, else full.
fn head_and_prefix(head: &str) -> (Vec<Line>, Line) {
    let mut lines = split(head, false, true, true);
    let prefix = match lines.last() {
        Some(last) if last.ending.is_empty() => lines.pop().expect("a last line"),
        _ => Line {
            body: String::new(),
            ending: String::new(),
            full: true,
            copied_ending: true,
        },
    };
    (lines, prefix)
}

/// Ends the last of `lines` with `ending`, the source's, where it has none
/// yet.
fn end_with_source_ending(lines: &mut [Line], ending: Option<String>) {
    if let (Some(last), Some(ending)) = (lines.last_mut(), ending) {
        if last.ending.is_empty() && !ending.is_empty() {
            last.ending = ending;
            last.copied_ending = true;
        }
    }
}

/// A rendered item's lines, with the empty line its last line ending closes
/// where it ends with blank lines.
fn item_lines(text: &str) -> Vec<Line> {
    let mut lines = content(text);
    if text.ends_with(['\n', '\r']) {
        lines.push(empty());
    }
    lines
}

/// Whether the list item read from `text`, its source without its
/// containers, keeps that text when a blank line and another item of a list
/// marked with `marker` follow it, as an item ending in an unclosed fence
/// does not.
fn closed_before_an_item(text: &str, marker: char) -> bool {
    let text = strip_line_ending(text);
    let marker = if matches!(marker, '.' | ')') {
        format!("1{marker} x")
    } else {
        format!("{marker} x")
    };
    let probe = format!("{text}\n\n{marker}");
    match crate::parse(&probe).document.children.as_slice() {
        [Block::List(list)] => {
            list.children.len() == 2
                && list.children[0]
                    .meta
                    .span
                    .is_some_and(|span| span.end <= text.len() + 1)
        }
        _ => false,
    }
}

/// Appends `written`, a block's lines, to `lines` after `gap`, the source
/// between the block before it and this one: the gap's first piece ends the
/// last line, its whole lines follow, and its last piece is the prefix of
/// `written`'s first line, which it makes full.
fn splice(lines: &mut Vec<Line>, gap: &str, mut written: Vec<Line>) {
    let mut pieces = split(gap, true, true, true);
    let ending = pieces.remove(0);
    let last = lines.last_mut().expect("a block was written");
    last.body.push_str(&ending.body);
    last.ending = ending.ending;
    last.copied_ending = true;
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

/// Prefixes each content line of `lines` with `prefix`, or with
/// `empty_prefix` when it is empty, and the first, when it is a content
/// line, with `opening` in their place: its text and whether that text holds
/// every enclosing container's prefix, as text copied from a later source
/// line does. A full line stays as it is, and every other line becomes
/// content of the container around it.
fn prefixed(
    lines: Vec<Line>,
    opening: Option<(&str, bool)>,
    prefix: &str,
    empty_prefix: &str,
) -> Vec<Line> {
    lines
        .into_iter()
        .enumerate()
        .map(|(index, mut line)| {
            if !line.full {
                match opening.filter(|_| index == 0) {
                    Some((head, full)) => {
                        line.body.insert_str(0, head);
                        line.full = full;
                    }
                    None if line.body.is_empty() => line.body.insert_str(0, empty_prefix),
                    None => line.body.insert_str(0, prefix),
                }
            }
            line
        })
        .collect()
}

/// Whether `line`, a quote's first content line, is a blank line that would
/// not open the quote: an empty one, which takes the bare `>` of a blank
/// line, or one copied with its prefixes that ends in a `>`.
fn opens_bare(line: &Line) -> bool {
    if line.full {
        line.body.ends_with('>')
            && line
                .body
                .chars()
                .all(|char| matches!(char, ' ' | '\t' | '>'))
    } else {
        line.body.is_empty()
    }
}

/// Whether `gap`, the source between two siblings, holds a line of nothing
/// but container prefixes: a blank line where the siblings stand.
pub(super) fn holds_blank_line(gap: &str) -> bool {
    split(gap, false, false, true)
        .iter()
        .skip(1)
        .filter(|line| !line.ending.is_empty())
        .any(|line| {
            line.body
                .chars()
                .all(|char| matches!(char, ' ' | '\t' | '>'))
        })
}

/// Whether `line`, a later line of an edited leaf's text, reads as the
/// leaf's line where the base leaf's line `base` stands, after that line's
/// source prefix: it is that line, or it opens no block wherever it stands,
/// as a lazy line, which the containers around it do not hold, must not, or
/// no line opens a block where `base` stands.
fn keeps_line_prefix(line: &str, base: &str) -> bool {
    line == base
        || matches!(
            crate::parse(line).document.children.as_slice(),
            [Block::Paragraph(_)]
        )
        || opens_no_block_at(base)
}

/// The lines of a leaf's inline source, which joins them with `\n`, each
/// with its offset in that source.
fn base_lines(text: &str) -> Vec<(usize, &str)> {
    text.split('\n')
        .scan(0, |start, line| {
            let at = *start;
            *start += line.len() + 1;
            Some((at, line))
        })
        .collect()
}

/// Whether `base`, a later line of a leaf's paragraph, stands where no line
/// opens a block: it interrupts a paragraph on the line after one, but not
/// there. A line that interrupts a paragraph does so as a lazy line too, so
/// what kept it in the paragraph is indentation of four columns or more
/// past its container's content, which keeps any text there; should the
/// parser ever keep such a line for another reason, the read-back check in
/// `Document::write` rewrites the block.
fn opens_no_block_at(base: &str) -> bool {
    let probe = format!("a\n{base}");
    match crate::parse(&probe).document.children.as_slice() {
        [Block::Paragraph(paragraph), _, ..] => paragraph
            .meta
            .span
            .is_some_and(|span| span.end <= "a\n".len()),
        _ => false,
    }
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
/// around a container, the lines around the gap read the same where they
/// are the source's, and otherwise where the last leaf before the gap
/// (`last_leaf`) does not take the line after it (`starts_after_leaf`).
/// `left` and `right` are each a block, its base span, and its lines when it
/// was not kept whole; `open` where `left` is an open list (`Out::OpenList`),
/// whose last item may take indented lines at a column the source did not
/// give it, so that only its source lines read the same; `top` where
/// content lines take no prefix. The probes run only at a boundary whose
/// lines around it changed, each parsing two lines under the markers of
/// the containers above them.
pub(super) fn keeps_gap(
    source: &str,
    gap: &str,
    left: (&Block, Span, Option<&[Line]>),
    right: (&Block, Span, Option<&[Line]>),
    open: bool,
    top: bool,
) -> bool {
    let (left_block, left_span, left_lines) = left;
    let (right_block, right_span, right_lines) = right;
    let left_end =
        left_span.start + strip_line_ending(&source[left_span.start..left_span.end]).len();
    let lines_hold = || {
        left_lines.is_none_or(|lines| last_line_holds(source, lines, left_span, left_end, top))
            && right_lines.is_none_or(|lines| first_line_holds(source, lines, right_span))
    };
    holds_blank_line(gap)
        || matches!(left_block, Block::BlankLine(_))
        || matches!(right_block, Block::BlankLine(_))
        || !holds_blocks(left_block) && !holds_blocks(right_block)
        || lines_hold()
        || !open && {
            let first = match right_lines {
                Some(lines) => lines
                    .first()
                    .filter(|line| !line.full)
                    .map(|line| line.body.as_str()),
                None => source[right_span.start..].split(['\n', '\r']).next(),
            };
            match last_leaf(source, left_block) {
                Some((leaf, markers)) => starts_after_leaf(leaf, &markers, right_block, first),
                None => true,
            }
        }
}

/// The last leaf of `block`, through the last child of each container, with
/// the markers that open the containers holding it, in order, as they
/// would be written on one line: `- ` for a list item (`item_marker`), `> `
/// for a block quote or an alert, `[^a]: ` for a footnote definition. `None` where
/// nothing can take the line after it: the last child of a container is no
/// leaf, a blank line, or an HTML container, which its closing tag ends. A
/// directive stands for its last leaf, whose lines no probe stands for, and
/// a list whose last item is empty for that item, its marker last.
fn last_leaf<'b>(source: &str, block: &'b Block) -> Option<(&'b Block, String)> {
    let (children, marker) = match block {
        Block::BlockQuote(node) => (&node.children, String::from("> ")),
        // Its marker line takes the line after it as a paragraph's.
        Block::Alert(node) if node.children.is_empty() => return Some((block, String::new())),
        Block::Alert(node) => (&node.children, String::from("> ")),
        Block::FootnoteDefinition(node) => (&node.children, String::from("[^a]: ")),
        Block::List(list) => {
            let item = list.children.last()?;
            // An empty item, written as its marker and a space, stands for
            // itself.
            if item.children.is_empty() {
                let marker = item.meta.span.map(|span| {
                    let text = source[span.start..span.end].trim_start();
                    &text[..text.find(char::is_whitespace).unwrap_or(text.len())]
                });
                return Some((block, format!("{} ", marker.unwrap_or("-"))));
            }
            (&item.children, item_marker(source, item))
        }
        Block::ContainerDirective(_) => return Some((block, String::new())),
        // Its closing tag ends it.
        Block::HtmlContainer(_) | Block::BlankLine(_) => return None,
        Block::Paragraph(_)
        | Block::Heading(_)
        | Block::ThematicBreak(_)
        | Block::CodeBlock(_)
        | Block::HtmlBlock(_)
        | Block::Definition(_)
        | Block::Table(_)
        | Block::MathBlock(_)
        | Block::Frontmatter(_)
        | Block::LeafDirective(_) => return Some((block, String::new())),
    };
    let (leaf, markers) = last_leaf(source, children.last()?)?;
    if matches!(leaf, Block::ContainerDirective(_)) {
        return Some((leaf, markers));
    }
    Some((leaf, format!("{marker}{markers}")))
}

/// A `-` marker as wide as `item`'s in `source`, up to the column its
/// content starts at on its marker's line, so that a line indented less
/// than that column stays out of it: four spaces or more after the marker
/// count as one, the rest opening indented code, and a checkbox is content.
/// A lone tab after the marker, which `item` copies, stays a tab, as wide
/// as the probe's column makes it. `- ` where the source does not tell,
/// which leaves fewer lines out.
fn item_marker(source: &str, item: &ListItem) -> String {
    let marker = item
        .meta
        .span
        .zip(item.children.first().and_then(Block::span))
        .filter(|(item, first)| item.start < first.start)
        .map(|(item, first)| &source[item.start..first.start])
        .filter(|marker| !marker.contains(['\n', '\r']))
        // A checkbox is content.
        .map(
            |marker| match marker.find('[').filter(|_| item.checked.is_some()) {
                Some(checkbox) => &marker[..checkbox],
                None => marker,
            },
        );
    if let Some(tab) = marker.and_then(|marker| marker.find('\t')) {
        let lone = marker.is_some_and(|marker| content_column(marker, false, true).is_some());
        return if lone {
            format!("-{}\t", " ".repeat(tab - 1))
        } else {
            String::from("- ")
        };
    }
    let width = marker
        .and_then(|marker| {
            let rest = marker.trim_start_matches(' ');
            let after = rest.trim_start_matches(|char: char| !char.is_ascii_whitespace());
            let spaces = after.len() - after.trim_start_matches(' ').len();
            let spaces = if spaces > 4 { 1 } else { spaces };
            (spaces > 0).then(|| marker.len() - after.len() + spaces)
        })
        .unwrap_or(2);
    format!("-{}", " ".repeat(width.max(2) - 1))
}

/// Whether `right`, written on the line after the leaf `left`, starts there
/// whatever its first line became. After a leaf that takes the lines
/// following it: where `markers`, the containers holding that leaf, are
/// none, a paragraph's or a definition's only where `right` interrupts a
/// paragraph as written, a table's only where its first line as written
/// (`first`) ends a table; where they are some, only where that line, as a
/// lazy line, ends the containers holding a paragraph, which no row
/// continues a table with. After an alert's marker line, which takes a lazy
/// line as its paragraph's, the same; after an empty item, which takes no
/// line, where the line does not continue its list with another item;
/// after a directive, whose lines no probe stands for, never; after any
/// other leaf always.
fn starts_after_leaf(left: &Block, markers: &str, right: &Block, first: Option<&str>) -> bool {
    // Whether `first`, after `before`, starts a block of its own after the
    // first block `before` opens, or after the first item of a list.
    let ends = |before: &str| {
        first.is_some_and(|first| {
            let probe = format!("{before}\n{first}");
            let block = crate::parse(&probe).document.children.into_iter().next();
            let span = match block {
                Some(Block::List(list)) => list.children[0].meta.span,
                block => block.and_then(|block| block.span()),
            };
            span.is_some_and(|span| span.end == before.len() + 1)
        })
    };
    match left {
        Block::ContainerDirective(_) => false,
        Block::Paragraph(_) | Block::Definition(_) if markers.is_empty() => {
            crate::parse::interrupts_paragraph(right)
        }
        Block::Paragraph(_) | Block::Definition(_) => ends(&format!("{markers}a")),
        Block::Alert(_) => ends(&format!("{markers}> [!NOTE]")),
        Block::Table(_) if markers.is_empty() => ends("| a |\n| - |"),
        // An empty item, its marker last in `markers`, takes no line, so
        // the line stays out of the list unless it is another item.
        Block::List(_) => ends(markers),
        // No lazy line continues a table.
        Block::Table(_) => true,
        // Each ends where its own syntax or line does.
        Block::Heading(_)
        | Block::ThematicBreak(_)
        | Block::CodeBlock(_)
        | Block::HtmlBlock(_)
        | Block::MathBlock(_)
        | Block::Frontmatter(_)
        | Block::LeafDirective(_) => true,
        // `last_leaf` returns no other block; one that holds blocks may
        // take the line.
        Block::BlockQuote(_)
        | Block::FootnoteDefinition(_)
        | Block::HtmlContainer(_)
        | Block::BlankLine(_) => false,
    }
}

/// Whether `block` holds blocks, whose last line can change what the line
/// after it reads as.
pub(super) fn holds_blocks(block: &Block) -> bool {
    match block {
        Block::BlockQuote(_)
        | Block::Alert(_)
        | Block::List(_)
        | Block::FootnoteDefinition(_)
        | Block::ContainerDirective(_)
        | Block::HtmlContainer(_) => true,
        Block::Paragraph(_)
        | Block::Heading(_)
        | Block::ThematicBreak(_)
        | Block::CodeBlock(_)
        | Block::HtmlBlock(_)
        | Block::Definition(_)
        | Block::Table(_)
        | Block::MathBlock(_)
        | Block::Frontmatter(_)
        | Block::LeafDirective(_)
        | Block::BlankLine(_) => false,
    }
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

/// What the siblings written so far tell about the place of the next one
/// written from the source: one rule (`super::moves_into_previous`) for the
/// top level and every container.
#[derive(Default)]
pub(super) struct Placement<'b> {
    /// Whether a sibling, a blank line included, was written.
    started: bool,
    /// The last sibling written other than a blank line.
    last: Option<&'b Block>,
    /// The marker of the list written last, with only blank lines after it.
    previous_list: Option<char>,
    /// Whether `last` was written as `Out::OpenList`.
    open_list: bool,
}

impl<'b> Placement<'b> {
    /// The marker of the list written last, with only blank lines after it.
    pub(super) fn previous_list(&self) -> Option<char> {
        self.previous_list
    }

    /// Whether `block`, written from the source with text that opens with
    /// `text`, reads as part of what is written before it. One that follows
    /// its predecessor in the source across a kept gap (`follows`) reads as
    /// there unless that predecessor is an open list, whose last item's
    /// column moved.
    pub(super) fn takes_in(&self, block: &Block, text: &str, follows: bool) -> bool {
        self.started
            && if follows {
                self.open_list && super::moves_into_previous(block, text, self.last, None)
            } else {
                super::moves_into_previous(block, text, self.last, self.previous_list)
            }
    }

    /// Records `block`, written after the siblings so far: the marker it was
    /// written with when it is a list, and whether it is an open list.
    pub(super) fn wrote(&mut self, block: &'b Block, marker: Option<char>, open_list: bool) {
        self.started = true;
        if matches!(block, Block::BlankLine(_)) {
            return;
        }
        self.last = Some(block);
        self.previous_list = marker;
        self.open_list = open_list;
    }
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
        let mut lines = split(&self.source[start..end], false, true, true);
        if lines.last().is_some_and(|line| !line.ending.is_empty()) {
            lines.push(Line {
                body: String::new(),
                ending: String::new(),
                full: true,
                copied_ending: true,
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
        for (index, line) in split(&self.source[span.start..span.end], false, true, true)
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

    /// The line ending in the source at `at`, the end of a line.
    fn ending_at(&self, at: usize) -> String {
        let rest = &self.source[at..];
        String::from(if rest.starts_with("\r\n") {
            "\r\n"
        } else if rest.starts_with(['\n', '\r']) {
            &rest[..1]
        } else {
            ""
        })
    }

    /// The line ending in the source after the last line of the node at
    /// `span`.
    fn ending_after(&self, span: Span) -> String {
        self.ending_at(self.line_end(self.body_end(span)))
    }

    /// The start of the source line `at` is on.
    fn line_start(&self, at: usize) -> usize {
        self.source[..at]
            .rfind(['\n', '\r'])
            .map_or(0, |end| end + 1)
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
        let read = crate::parse::parse_inline_tree(&source.text, self.base_definitions).inlines;
        let context = if heading {
            Context::HEADING
        } else {
            Context::BLOCK
        };
        let mut output = String::new();
        if !write_inline_run(inlines, base, &read, &source.text, context, &mut output)? {
            return Ok(None);
        }
        // The text reads as one paragraph, no later line opening with spaces
        // or tabs, which the block parse strips; a later line whose base
        // line stands where no line opens a block (`opens_no_block_at`)
        // counts as plain text, since `leaf_lines` writes it after that
        // line's source prefix (`keeps_line_prefix`).
        if !heading || output.contains(['\n', '\r']) {
            let base_lines = base_lines(&source.text);
            let lines = split(&output, false, false, false);
            let mut probe = String::new();
            for (index, line) in lines.iter().enumerate() {
                if index > 0 && line.body.starts_with([' ', '\t']) {
                    return Ok(None);
                }
                let held = index > 0
                    && base_lines
                        .get(index)
                        .is_some_and(|(_, base)| opens_no_block_at(base));
                probe.push_str(if held { "x" } else { &line.body });
                probe.push_str(&line.ending);
            }
            if !matches!(
                crate::parse(&probe).document.children.as_slice(),
                [Block::Paragraph(_)]
            ) {
                return Ok(None);
            }
        }
        let reads = crate::parse::parse_inline_tree(&output, self.definitions).inlines;
        Ok(
            (normalized_inlines(&reads, heading) == normalized_inlines(inlines, heading))
                .then_some(output),
        )
    }

    /// `text`, a leaf's content written for the leaf at `span`, as lines:
    /// with the source's indentation before its first line and, with `tail`,
    /// the whitespace after its last, bytes of the leaf's span outside its
    /// inline source that a line ending does not cross; and each later line
    /// the base leaf also has as that source line, from its start, with the
    /// source's line ending before it, where it reads as that line does
    /// there (`keeps_line_prefix`).
    fn leaf_lines(&mut self, span: Span, text: &str, tail: bool) -> Vec<Line> {
        let Some(source) = self.correspondence.source_of(span).cloned() else {
            return content(text);
        };
        let (from, to) = (source.to_source(0), source.to_source(source.text.len()));
        let body_end = self.body_end(span);
        let mut padded = String::new();
        if span.start <= from && !self.source[span.start..from].contains(['\n', '\r']) {
            padded.push_str(&self.source[span.start..from]);
        }
        padded.push_str(text);
        if tail && to <= body_end && !self.source[to..body_end].contains(['\n', '\r']) {
            padded.push_str(&self.source[to..body_end]);
        }
        let mut lines = content(&padded);
        let base_lines = base_lines(&source.text);
        for index in 1..lines.len().min(base_lines.len()) {
            let (offset, base_line) = base_lines[index];
            let at = source.to_source(offset);
            let line_start = self.line_start(at);
            let line = &mut lines[index];
            if !keeps_line_prefix(&line.body, base_line) {
                continue;
            }
            line.body.insert_str(0, &self.source[line_start..at]);
            line.full = true;
            let ending = self.source[..line_start]
                .strip_suffix("\r\n")
                .map_or_else(|| &self.source[line_start - 1..line_start], |_| "\r\n");
            let previous = &mut lines[index - 1];
            previous.ending = ending.into();
            previous.copied_ending = true;
        }
        lines
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
                        Some(text) => Out::Lines(self.leaf_lines(span, &text, true)),
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
                    Children::Lines(mut lines, _) => {
                        let first = &self.source[span.start..self.line_end(span.start)];
                        let mut written = content(first);
                        self.open_later_line(&mut lines, &base.children);
                        extend(&mut written, prefixed(lines, None, "> ", ">"));
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
                            if !lines
                                .first()
                                .is_some_and(|first| !first.full && !first.body.is_empty())
                            {
                                return Ok(Out::Render);
                            }
                            let (mut written, prefix) =
                                head_and_prefix(&self.source[span.start..start]);
                            written.extend(prefixed(lines, Some((&prefix.body, true)), "    ", ""));
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
                let line_end = self.line_end(span.start);
                let label_holds = self.leaf_holds(Some(span), kept, &node.label, &base.label);
                // The opening line, with an edited label written inline by
                // inline in its place.
                let opening = if label_holds {
                    String::from(&self.source[span.start..line_end])
                } else {
                    let Some((from, to)) = self
                        .correspondence
                        .source_of(span)
                        .map(|source| (source.to_source(0), source.to_source(source.text.len())))
                        .filter(|(from, to)| span.start <= *from && from <= to && *to <= line_end)
                    else {
                        return Ok(Out::Render);
                    };
                    match self.leaf_text(span, kept, &node.label, &base.label, true)? {
                        Some(text) if !text.contains(['\n', '\r']) => format!(
                            "{}{text}{}",
                            &self.source[span.start..from],
                            &self.source[to..line_end]
                        ),
                        _ => return Ok(Out::Render),
                    }
                };
                match self.children(&node.children, &base.children, Join::Gap)? {
                    Children::Kept if label_holds => Out::Kept(span),
                    Children::Kept => {
                        let mut rest = self.source_lines(line_end, self.body_end(span));
                        let mut written = content(&opening);
                        let ending = rest.remove(0);
                        written[0].ending = ending.ending;
                        written[0].copied_ending = true;
                        written.extend(rest);
                        Out::Lines(written)
                    }
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
                        let mut written = content(&opening);
                        let mut lines = lines;
                        self.open_later_line(&mut lines, &base.children);
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
        if text.contains(['\n', '\r']) || from < span.start || to > body_end {
            return Ok(Out::Render);
        }
        let line = format!(
            "{}{text}{}",
            &self.source[span.start..from],
            &self.source[to..body_end]
        );
        Ok(Out::Lines(content(&line)))
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
        let mut closing = split(tail, true, true, true);
        let ending = closing.remove(0);
        let last = written.last_mut().expect("a block has a line");
        last.body.push_str(&ending.body);
        last.ending = ending.ending;
        last.copied_ending = true;
        written.extend(closing);
        Out::Lines(written)
    }

    /// Gives the first of `lines`, the children of a container whose own
    /// first line holds none of them, written against `base`, the source
    /// prefix of the line the first of `base` starts on, where it reads as
    /// that line does there (`keeps_line_prefix`): the whole source line of
    /// a blank line.
    fn open_later_line(&self, lines: &mut [Line], base: &[Block]) {
        let (Some(line), Some(first)) = (lines.first_mut(), base.first()) else {
            return;
        };
        let Some(span) = first.span().filter(|_| !line.full) else {
            return;
        };
        let line_start = self.line_start(span.start);
        if let Block::BlankLine(_) = first {
            if line.body.is_empty() {
                line.body = self.source[line_start..self.body_end(span)].into();
                line.full = true;
            }
        } else if keeps_line_prefix(
            &line.body,
            &self.source[span.start..self.line_end(span.start)],
        ) {
            line.body
                .insert_str(0, &self.source[line_start..span.start]);
            line.full = true;
        }
    }

    /// A block quote at `span` whose base holds `base` and whose children
    /// were written as `lines`: its first line opens with the source's own
    /// marker, the `>` and the space after it, where the base's first block
    /// starts on that line; every other content line takes the canonical
    /// prefix.
    fn quoted(&self, span: Span, base: &[Block], mut lines: Vec<Line>) -> Out {
        let marker = base
            .first()
            .filter(|first| !matches!(first, Block::BlankLine(_)))
            .and_then(Block::span)
            .filter(|first| first.start <= self.line_end(span.start))
            .map(|first| &self.source[span.start..first.start])
            .filter(|marker| !marker.contains('\t'));
        let opening = match (base.first(), lines.first()) {
            // A blank first line is the quote's own first line.
            (Some(Block::BlankLine(blank)), Some(first))
                if first.body.is_empty() && !first.full =>
            {
                blank
                    .meta
                    .span
                    .map(|blank| (&self.source[span.start..self.body_end(blank)], false))
            }
            _ => marker
                .filter(|_| lines.first().is_some_and(|first| !first.body.is_empty()))
                .map(|marker| (marker, false)),
        };
        // A blank first line that is not the base's opens the quote, which
        // takes `> `: a bare `>`, the canonical prefix of a later blank
        // line and what a copied one may hold, continues a quote but opens
        // none.
        let opening = match lines.first_mut() {
            Some(first) if opening.is_none() && opens_bare(first) => {
                first.body.clear();
                first.full = false;
                Some(("> ", false))
            }
            _ => opening,
        };
        Out::Lines(prefixed(lines, opening, "> ", ">"))
    }

    /// A container whose first line opens with `marker`, before the first
    /// line of its children, and whose later content lines take `indent`.
    fn marked(&self, marker: &str, lines: Vec<Line>, indent: &str) -> Out {
        // A first line that opens with indentation or is blank would read
        // differently after the marker than as the canonical writer writes
        // it, as indented code does.
        match lines.first() {
            Some(first)
                if !first.full
                    && !first.body.is_empty()
                    && !first.body.starts_with([' ', '\t']) => {}
            _ => return Out::Render,
        }
        Out::Lines(prefixed(lines, Some((marker, false)), indent, ""))
    }

    /// Appends `count` empty content lines to `lines`, then a line for each
    /// of `blank_lines`: the source line of one written from the source,
    /// with the source's line ending before it, unless it opens the
    /// container, whose own syntax opens that line; an empty content line
    /// otherwise.
    fn push_blank_lines(&self, lines: &mut Vec<Line>, count: usize, blank_lines: &[Option<Span>]) {
        push_empty(lines, count);
        for blank in blank_lines {
            match blank {
                Some(span) if !lines.is_empty() => {
                    let start = span.start;
                    let ending = self.source[..start]
                        .strip_suffix("\r\n")
                        .map_or_else(|| &self.source[start - 1..start], |_| "\r\n");
                    let last = lines.last_mut().expect("a line was written");
                    last.ending = ending.into();
                    last.copied_ending = true;
                    lines.push(Line {
                        body: self.source[start..self.body_end(*span)].into(),
                        ending: String::new(),
                        full: true,
                        copied_ending: true,
                    });
                }
                _ => push_empty(lines, 1),
            }
        }
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
        // A block written from the source before a sibling other than its
        // successor there stays so only where a blank line and a paragraph
        // after it would not extend it, as they extend an unclosed fence.
        for index in 0..children.len() {
            let Some((at, _)) = matched[index] else {
                continue;
            };
            let successor = index + 1 == children.len()
                || matched[index + 1].is_some_and(|(next, _)| next == at + 1)
                    && (matches!(children[index + 1], Block::BlankLine(_))
                        || !matches!(outs[index + 1], Out::Render));
            let text = match &outs[index] {
                _ if successor => continue,
                Out::Kept(span) => self.unprefixed(*span),
                Out::Lines(lines) | Out::OpenList(lines) => {
                    let mut text = self::join(lines, "\n");
                    text.push('\n');
                    text
                }
                Out::Render => continue,
            };
            if !super::closed_by_a_blank_line(&text) {
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
        // Whether the block written last is an open list.
        let mut previous_open = false;
        let mut placement = Placement::default();
        // The blank lines written since the last other block: the base span
        // of each written from the source.
        let mut blank_lines: Vec<Option<Span>> = Vec::new();
        let mut separated = false;
        for (index, (child, out)) in children.iter().zip(outs).enumerate() {
            if let Block::BlankLine(_) = child {
                blank_lines.push(matched[index].and_then(|(_, base)| base.span()));
                placement.wrote(child, None, false);
                continue;
            }
            let mut at = matched[index]
                .map(|(at, _)| at)
                .filter(|_| !matches!(out, Out::Render));
            let open_list = matches!(out, Out::OpenList(_));
            let mut written = match out {
                Out::Kept(span) => Some(self.slice(span)),
                Out::Lines(lines) | Out::OpenList(lines) => Some(lines),
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
                                previous_open,
                                false,
                            )
                    })
                }
                _ => None,
            };
            if written
                .as_ref()
                .is_some_and(|written| placement.takes_in(child, &written[0].body, gap.is_some()))
            {
                written = None;
                at = None;
            }
            let written = match written {
                Some(written) => written,
                None => content(&write_blocks(
                    core::slice::from_ref(child),
                    self.options,
                    Join::Gap,
                    false,
                    placement.previous_list(),
                )?),
            };
            let marker = match child {
                Block::List(list) if at.is_some() => Some(super::written_marker(list)),
                Block::List(list) => Some(super::list_marker(
                    list,
                    self.options,
                    placement.previous_list(),
                )),
                _ => None,
            };
            placement.wrote(child, marker, open_list && at.is_some());
            previous_open = open_list && at.is_some();
            match previous {
                None => {
                    self.push_blank_lines(&mut lines, 0, &blank_lines);
                    extend(&mut lines, written);
                }
                Some(_) => {
                    separated |= gap.map_or(
                        join == Join::Gap || !blank_lines.is_empty(),
                        holds_blank_line,
                    );
                    match gap {
                        Some(gap) => splice(&mut lines, gap, written),
                        None => {
                            let separator = usize::from(join == Join::Gap);
                            self.push_blank_lines(&mut lines, separator, &blank_lines);
                            extend(&mut lines, written);
                        }
                    }
                }
            }
            blank_lines.clear();
            previous = Some((index, at));
        }
        if !blank_lines.is_empty() {
            separated = true;
            let separator = usize::from(self.separates_trailing_blank_lines(base));
            self.push_blank_lines(&mut lines, separator, &blank_lines);
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
        // Whether the item written last is the base's last item, written
        // from the source.
        let mut last_from_source = false;
        // Where the lines of the item written last start.
        let mut previous_start = 0;
        for (index, item) in node.children.iter().enumerate() {
            let found = matching(item.meta.span, item, &base.children, span_of);
            let (mut out, mut item_separated) = match found {
                Some((_, base_item)) => self.item(node, item, base_item, &mut previous_number)?,
                None => (Out::Render, false),
            };
            // An item indented in the source reads as the content of the item
            // before it, unless that is its predecessor there.
            let follows = found
                .zip(previous_at)
                .is_some_and(|((at, _), previous)| at == previous + 1);
            if index > 0
                && !follows
                && !matches!(out, Out::Render)
                && item
                    .meta
                    .span
                    .is_some_and(|span| self.source[span.start..].starts_with([' ', '\t']))
            {
                (out, item_separated) = (Out::Render, false);
            }
            separated |= match out {
                Out::Kept(_) => item_separated,
                Out::Lines(_) | Out::OpenList(_) => item_separated,
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
            last_from_source = at.is_some_and(|at| at + 1 == base.children.len());
            let written = match out {
                Out::Kept(span) => {
                    if let Some(number) = item.number {
                        previous_number = Some(number);
                    }
                    self.slice(span)
                }
                Out::Lines(lines) | Out::OpenList(lines) => lines,
                Out::Render => {
                    let text =
                        write_item(node, item, self.options, marker_char, &mut previous_number)?;
                    item_lines(&text)
                }
            };
            let start = lines.len();
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
                        // An item written from the source that a blank line
                        // and another item would extend, as an unclosed
                        // fence, is written canonically before a new one.
                        let previous = &node.children[index - 1];
                        let extended = previous_at.is_some()
                            && previous.meta.span.is_some_and(|span| {
                                !closed_before_an_item(&self.unprefixed(span), marker_char)
                            });
                        if extended {
                            lines.truncate(previous_start);
                            let mut number = None;
                            let text =
                                write_item(node, previous, self.options, marker_char, &mut number)?;
                            if previous_start == 0 {
                                lines = item_lines(&text);
                            } else {
                                extend(&mut lines, item_lines(&text));
                            }
                        }
                        if !node.tight && (extended || !previous_holds_separator) {
                            extend(&mut lines, alloc::vec![empty()]);
                        }
                        extend(&mut lines, written);
                    }
                }
            }
            previous_start = start;
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
        Ok(if last_from_source {
            Out::Lines(lines)
        } else {
            Out::OpenList(lines)
        })
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
        // A checkbox split across lines is not a marker to edit.
        if base.checked.is_some() && !same_line && !own {
            return Ok((Out::Render, false));
        }
        let marker = if same_line {
            &self.source[span.start..first_start]
        } else {
            self.source[span.start..line_end].trim_end()
        };
        if !marker.is_ascii() || !own && marker.contains('\t') {
            return Ok((Out::Render, false));
        }
        let Some(column) = content_column(marker, base.checked.is_some(), same_line) else {
            return Ok((Out::Render, false));
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
                return Ok((Out::Lines(content(&empty_item_marker(&marker))), false));
            }
            Children::Render => return Ok((Out::Render, false)),
        };
        let indent = " ".repeat(column);
        // A break's own `-` or `*` after the marker would open a list, and a
        // first line of only the bullet's char after it would read as a
        // break with the bullet.
        let bullet = marker.trim_start().chars().next().unwrap_or_default();
        let out = if matches!(first_block(&item.children), Some(Block::ThematicBreak(_)))
            || (same_line
                && !list.ordered
                && item.checked.is_none()
                && lines
                    .first()
                    .is_some_and(|first| super::reads_as_thematic_break(bullet, &first.body)))
        {
            Out::Render
        } else if same_line {
            self.marked(&marker, lines, &indent)
        } else if own {
            // An unchanged marker is its source up to the first child, whose
            // line keeps its source prefix. After a checkbox that line is a
            // lazy one, which only a paragraph's text continues.
            let source_line = &self.source[first_start..self.line_end(first_start)];
            if !lines.first().is_some_and(|first| {
                !first.full
                    && !first.body.is_empty()
                    && (base.checked.is_none()
                        || first.body == source_line
                        || matches!(
                            crate::parse(&first.body).document.children.as_slice(),
                            [Block::Paragraph(_)]
                        ))
            }) {
                return Ok((Out::Render, false));
            }
            let (mut written, prefix) = head_and_prefix(&self.source[span.start..first_start]);
            written.extend(prefixed(lines, Some((&prefix.body, true)), &indent, ""));
            Out::Lines(written)
        } else {
            let mut written = content(&marker);
            extend(&mut written, prefixed(lines, None, &indent, ""));
            Out::Lines(written)
        };
        Ok((out, separated))
    }

    /// A row at `span` whose base is `base`, with the same cells there:
    /// its source with each cell that changed written in its place, inline
    /// by inline, and encoded as cell source.
    fn row(
        &mut self,
        row: &TableRow,
        base: &TableRow,
        span: Span,
    ) -> Result<Option<String>, SerializeError> {
        if row.cells.len() != base.cells.len() {
            return Ok(None);
        }
        let body_end = self.body_end(span);
        let mut written = String::new();
        let mut at = span.start;
        for (cell, base_cell) in row.cells.iter().zip(&base.cells) {
            let Some(cell_span) = cell.meta.span.filter(|cell_span| {
                Some(*cell_span) == base_cell.meta.span
                    && at <= cell_span.start
                    && cell_span.end <= body_end
            }) else {
                return Ok(None);
            };
            written.push_str(&self.source[at..cell_span.start]);
            let kept = cell.source.as_deref();
            if self.leaf_holds(Some(cell_span), kept, &cell.children, &base_cell.children) {
                written.push_str(&self.source[cell_span.start..cell_span.end]);
            } else {
                let Some(text) =
                    self.leaf_text(cell_span, kept, &cell.children, &base_cell.children, true)?
                else {
                    return Ok(None);
                };
                if text.contains(['\n', '\r']) {
                    return Ok(None);
                }
                written.push_str(&super::encode_cell(&text));
            }
            at = cell_span.end;
        }
        written.push_str(&self.source[at..body_end]);
        Ok((!written.contains(['\n', '\r'])).then_some(written))
    }

    fn table(&mut self, node: &Table, base: &Table, span: Span) -> Result<Out, SerializeError> {
        let span_of = |row: &TableRow| row.meta.span;
        // The delimiter row: the line after the header row.
        let delimiter = base.rows.first().and_then(|header| {
            let after = self.body_end(header.meta.span?);
            let rest = &self.source[after..];
            let start = after + rest.len() - rest.trim_start_matches(['\r', '\n']).len();
            let end = self.line_end(start);
            let line = self.source[start..end].trim_start_matches([' ', '\t', '>']);
            (start > after && !line.is_empty())
                .then(|| (String::from(&self.source[start..end]), self.ending_at(end)))
        });
        let mut all_kept = node.rows.len() == base.rows.len();
        let mut lines = Vec::new();
        // The source's line ending after the line written last.
        let mut ending: Option<String> = None;
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
            let mut written = match (holds, row.meta.span, found) {
                (true, Some(span), _) => content(&self.source[span.start..self.body_end(span)]),
                (false, Some(span), Some((_, base_row))) => match self.row(row, base_row, span)? {
                    Some(text) => content(&text),
                    None => content(&write_row(row)?),
                },
                _ => content(&write_row(row)?),
            };
            // A later row keeps its source line's prefix where it reads as
            // that line does there.
            if let (true, Some((_, base_row))) = (index > 0, found) {
                let span = base_row.meta.span.expect("a matched row has a span");
                let line = &mut written[0];
                if keeps_line_prefix(&line.body, &self.source[span.start..self.body_end(span)]) {
                    let line_start = self.line_start(span.start);
                    line.body
                        .insert_str(0, &self.source[line_start..span.start]);
                    line.full = true;
                }
            }
            end_with_source_ending(&mut lines, ending.take());
            extend(&mut lines, written);
            ending = found.and(row.meta.span).map(|span| self.ending_after(span));
            if index == 0 {
                let written = match &delimiter {
                    Some((line, _)) => Line {
                        body: line.clone(),
                        ending: String::new(),
                        full: true,
                        copied_ending: false,
                    },
                    None => content(&write_delimiter_row(&node.alignments)).remove(0),
                };
                end_with_source_ending(&mut lines, ending.take());
                extend(&mut lines, alloc::vec![written]);
                ending = delimiter.as_ref().map(|(_, ending)| ending.clone());
            }
        }
        if all_kept {
            return Ok(Out::Kept(span));
        }
        Ok(Out::Lines(lines))
    }
}

/// The column, from an item's start, that the lines the writer indents in
/// it take: `marker` is the item's source on its first line, up to its first
/// child where that starts there, and a checkbox, `task`, is content. A
/// lone tab after the marker, whose width depends on the column it stands
/// at, counts as its widest, four columns, which leaves those lines in the
/// item, short of indented code, wherever it is written; `None` for any
/// other tab before the content.
fn content_column(marker: &str, task: bool, same_line: bool) -> Option<usize> {
    let rest = marker.trim_start_matches(' ');
    if rest.starts_with(|char: char| char.is_ascii_whitespace()) {
        return None;
    }
    let after = rest.trim_start_matches(|char: char| !char.is_ascii_whitespace());
    let width = marker.len() - after.len();
    let space = if task {
        &after[..after.find('[')?]
    } else if same_line {
        &after[..after.len() - after.trim_start_matches([' ', '\t']).len()]
    } else {
        return Some(width + 1);
    };
    match space {
        "\t" => Some(width + 4),
        space if space.contains('\t') => None,
        space => Some(width + space.len()),
    }
}

/// `marker`, an item's marker up to its first child, as an empty item's:
/// the marker and one space or tab after it, which opens the item.
fn empty_item_marker(marker: &str) -> String {
    let trimmed = marker.trim_end();
    let after = &marker[trimmed.len()..];
    let gap = after
        .chars()
        .next()
        .filter(|char| matches!(char, ' ' | '\t'))
        .unwrap_or(' ');
    format!("{trimmed}{gap}")
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
            children: _,
            source: _,
        }) => matches!(other, Block::Heading(other) if other.depth == *depth),
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
