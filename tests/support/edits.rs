//! Simulated edits: a block at every depth of a document, with its first
//! leaf or one own field edited, deleted, or followed by a new paragraph,
//! written with the source (`per-node-reuse`, task 2.5). Shared by the
//! corpus in `fixtures.rs` and the seeded generators in
//! `serialize_roundtrip_fuzz.rs`.

#![allow(dead_code)]

use markdown_syntax::prelude::*;
use markdown_syntax::{Document, Span};

use super::leaves;
use super::normalize;

/// Appends " edited" to the first `Text` of the block's first leaf, or a line
/// to a code block's value; false for a block holding neither.
pub(crate) fn edit_block(block: &mut Block) -> bool {
    fn edit_inlines(inlines: &mut [Inline]) -> bool {
        inlines.iter_mut().any(|inline| match inline {
            Inline::Text(text) => {
                text.value.push_str(" edited");
                true
            }
            _ => false,
        })
    }
    fn edit_blocks(blocks: &mut [Block]) -> bool {
        blocks.iter_mut().any(edit_block)
    }
    match block {
        Block::Paragraph(node) => edit_inlines(&mut node.children),
        Block::Heading(node) => edit_inlines(&mut node.children),
        Block::BlockQuote(node) => edit_blocks(&mut node.children),
        Block::Alert(node) => edit_blocks(&mut node.children),
        Block::List(node) => node
            .children
            .iter_mut()
            .any(|item| edit_blocks(&mut item.children)),
        Block::Table(node) => node.rows.iter_mut().any(|row| {
            row.cells
                .iter_mut()
                .any(|cell| edit_inlines(&mut cell.children))
        }),
        Block::CodeBlock(node) => {
            node.value.push_str("edited\n");
            true
        }
        Block::Definition(node) => {
            node.destination.push_str("/edited");
            true
        }
        _ => false,
    }
}

/// The span of the leaf whose `Text` `edit_block` edits in `block`: a
/// paragraph, a heading, or a table cell.
fn edited_leaf(block: &Block) -> Option<Span> {
    fn holds_text(inlines: &[Inline]) -> bool {
        inlines
            .iter()
            .any(|inline| matches!(inline, Inline::Text(_)))
    }
    fn in_blocks(blocks: &[Block]) -> Option<Span> {
        blocks.iter().find_map(edited_leaf)
    }
    match block {
        Block::Paragraph(node) => holds_text(&node.children).then_some(node.meta.span?),
        Block::Heading(node) => holds_text(&node.children).then_some(node.meta.span?),
        Block::BlockQuote(node) => in_blocks(&node.children),
        Block::Alert(node) => in_blocks(&node.children),
        Block::List(node) => node
            .children
            .iter()
            .find_map(|item| in_blocks(&item.children)),
        Block::Table(node) => node.rows.iter().find_map(|row| {
            row.cells
                .iter()
                .find(|cell| holds_text(&cell.children))
                .and_then(|cell| cell.meta.span)
        }),
        _ => None,
    }
}

/// Whether the reused block at `index` of `blocks` is written again after a
/// block written canonically, as its indentation could carry it into that
/// block: any indentation after a list or footnote definition, and code
/// indentation on a block other than indented code. The writer's own rule is
/// `moves_into_previous` in `src/serialize.rs`; a change to one is a change
/// to both.
pub(crate) fn rewritten_after_an_edit(input: &str, blocks: &[Block], index: usize) -> bool {
    let Some(block) = blocks.get(index) else {
        return false;
    };
    let rest = &input[block.span().expect("a parsed block has a span").start..];
    let indent = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let columns = rest[..indent].chars().fold(0, |column, char| match char {
        '\t' => column + 4 - column % 4,
        _ => column + 1,
    });
    // Blank lines between them do not end a list or a footnote definition.
    let previous = blocks[..index]
        .iter()
        .rfind(|block| !matches!(block, Block::BlankLine(_)));
    match previous {
        Some(Block::List(_) | Block::FootnoteDefinition(_)) => indent > 0,
        _ => columns >= 4 && !matches!(block, Block::CodeBlock(_)),
    }
}

/// Whether the block parsed from `text`, its source with its line ending,
/// keeps that text when a blank line and a paragraph follow it, as a closed
/// fence does and an unclosed one does not: a copy of the writer's
/// `closed_by_a_blank_line` in `src/serialize.rs`, which the tests cannot
/// call.
pub(crate) fn closed_by_a_blank_line(text: &str) -> bool {
    let gap = if text.ends_with('\n') {
        "\n"
    } else if text.ends_with('\r') {
        "\r"
    } else {
        "\n\n"
    };
    let probe = format!("{text}{gap}x");
    let children = parse(&probe).document.children;
    let Some((Block::Paragraph(last), rest)) = children.split_last() else {
        return false;
    };
    last.meta
        .span
        .is_some_and(|span| span.start + 1 == probe.len())
        && rest.iter().all(|block| {
            matches!(block, Block::BlankLine(_))
                || block.span().is_some_and(|span| span.end <= text.len())
        })
}

/// Whether `markdown` parses as `document` does, as tree comparison reads it.
pub(crate) fn reads_as(markdown: &str, document: &Document) -> bool {
    normalize::normalized(&parse(markdown).document.children)
        == normalize::normalized(&document.children)
}

/// One step of a path to a sequence of sibling blocks: a block's children,
/// or the children of an item of the list at that index.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Step {
    Child(usize),
    Item(usize, usize),
}

/// The path of every sequence of sibling blocks in `blocks`, at every depth,
/// with the index of each block in it other than a blank line.
fn sibling_paths(blocks: &[Block], path: &mut Vec<Step>, found: &mut Vec<(Vec<Step>, usize)>) {
    for (index, block) in blocks.iter().enumerate() {
        if !matches!(block, Block::BlankLine(_)) {
            found.push((path.clone(), index));
        }
        let mut inside = |children: &[Block], step: Step, found: &mut Vec<(Vec<Step>, usize)>| {
            path.push(step);
            sibling_paths(children, path, found);
            path.pop();
        };
        match block {
            Block::BlockQuote(node) => inside(&node.children, Step::Child(index), found),
            Block::Alert(node) => inside(&node.children, Step::Child(index), found),
            Block::FootnoteDefinition(node) => inside(&node.children, Step::Child(index), found),
            Block::ContainerDirective(node) => inside(&node.children, Step::Child(index), found),
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(children) = &node.content {
                    inside(children, Step::Child(index), found);
                }
            }
            Block::List(node) => {
                for (item, list_item) in node.children.iter().enumerate() {
                    inside(&list_item.children, Step::Item(index, item), found);
                }
            }
            _ => {}
        }
    }
}

/// The sibling blocks at `path` in `blocks`.
fn siblings_at<'a>(blocks: &'a mut Vec<Block>, path: &[Step]) -> &'a mut Vec<Block> {
    let Some((step, rest)) = path.split_first() else {
        return blocks;
    };
    let children = match (*step, blocks) {
        (Step::Child(index), blocks) => match &mut blocks[index] {
            Block::BlockQuote(node) => &mut node.children,
            Block::Alert(node) => &mut node.children,
            Block::FootnoteDefinition(node) => &mut node.children,
            Block::ContainerDirective(node) => &mut node.children,
            Block::HtmlContainer(node) => match &mut node.content {
                HtmlContainerContent::Blocks(children) => children,
                HtmlContainerContent::Inlines(_) => unreachable!(),
            },
            _ => unreachable!(),
        },
        (Step::Item(index, item), blocks) => match &mut blocks[index] {
            Block::List(node) => &mut node.children[item].children,
            _ => unreachable!(),
        },
    };
    siblings_at(children, rest)
}

/// Changes one own field of `block`: a list's first item's checkbox or
/// number, a heading's depth, a code block's value; false for other blocks.
fn edit_own_field(block: &mut Block) -> bool {
    match block {
        Block::List(node) => {
            let item = &mut node.children[0];
            if let Some(checked) = item.checked {
                item.checked = Some(!checked);
                true
            } else if let Some(number) = item.number {
                item.number = Some(number + 1);
                true
            } else {
                false
            }
        }
        Block::Heading(node) if node.depth < 2 => {
            node.depth += 1;
            true
        }
        Block::CodeBlock(node) => {
            node.value.push_str("edited\n");
            true
        }
        _ => false,
    }
}

/// Removes a list's first item or a table's last body row, where another
/// stays, and adds an item or a row after them; false for other blocks.
fn edit_children(block: &mut Block) -> bool {
    match block {
        Block::List(node) => {
            if node.children.len() > 1 {
                node.children.remove(0);
            }
            node.children
                .push(ListItem::new([Paragraph::new([Text::from("new")])]));
            true
        }
        Block::Table(node) if !node.rows.is_empty() => {
            if node.rows.len() > 2 {
                node.rows.pop();
            }
            let cells = node.rows[0]
                .cells
                .iter()
                .map(|_| TableCell::new([Text::from("new")]))
                .collect();
            node.rows.push(TableRow {
                meta: Default::default(),
                cells,
            });
            true
        }
        _ => false,
    }
}

/// The next alignment of each of a table's columns, `None`, `Left`,
/// `Center`, `Right`, and `None` again; false for other blocks.
fn realign(block: &mut Block) -> bool {
    let Block::Table(node) = block else {
        return false;
    };
    for alignment in &mut node.alignments {
        *alignment = match alignment {
            TableAlignment::None => TableAlignment::Left,
            TableAlignment::Left => TableAlignment::Center,
            TableAlignment::Center => TableAlignment::Right,
            TableAlignment::Right => TableAlignment::None,
        };
    }
    true
}

/// Adds a column after a table's last, or removes its last where another
/// stays: a cell `new` in each row, or each row's last cell where it has
/// one in that column; false for other blocks.
fn edit_columns(block: &mut Block, add: bool) -> bool {
    let Block::Table(node) = block else {
        return false;
    };
    if add {
        node.alignments.push(TableAlignment::None);
        for row in &mut node.rows {
            let count = node.alignments.len() - 1;
            row.cells.truncate(count);
            row.cells
                .resize_with(count, || TableCell::new(Vec::<Inline>::new()));
            row.cells.push(TableCell::new([Text::from("new")]));
        }
        return true;
    }
    if node.alignments.len() < 2 {
        return false;
    }
    node.alignments.pop();
    for row in &mut node.rows {
        row.cells.truncate(node.alignments.len());
    }
    true
}

/// Gives the first cell of a table that is empty in the source the text
/// `new`, and returns its path below the table (row, cell); `None` for
/// other blocks and a table without one.
fn fill_empty_cell(block: &mut Block) -> Option<(usize, usize)> {
    let Block::Table(node) = block else {
        return None;
    };
    let (row, cell) = node.rows.iter().enumerate().find_map(|(index, row)| {
        row.cells
            .iter()
            .position(|cell| cell.children.is_empty() && cell.meta.span.is_some())
            .map(|cell| (index, cell))
    })?;
    node.rows[row].cells[cell].children = vec![Text::from("new").into()];
    Some((row, cell))
}

/// Where a new block goes before the block at `index` of `siblings`: before
/// the blank lines that precede it too, so that a blank line opening a
/// container comes to follow a block.
fn before(siblings: &[Block], index: usize) -> usize {
    siblings[..index]
        .iter()
        .rposition(|block| !matches!(block, Block::BlankLine(_)))
        .map_or(0, |at| at + 1)
}

/// `input` with the empty cell at `span`, the first empty cell of its row,
/// given the text `new`, as an edit of that cell alone writes it: the
/// padding between its pipes replaced by the text with a space on either
/// side, or, for a cell the source row lacks, which the parser reads as
/// empty at the row's end, the text after a pipe, and before one where the
/// row ends with a pipe.
fn filled(input: &str, span: Span) -> String {
    let (before, after) = input.split_at(span.start);
    if after.starts_with('|') {
        let pipe = before.rfind('|').expect("a cell after a pipe");
        return format!("{} new {after}", &before[..=pipe]);
    }
    if before.trim_end_matches([' ', '\t']).ends_with('|') {
        format!("{before} new |{after}")
    } else {
        format!("{before} | new{after}")
    }
}

/// Whether `written` holds the lines of `input`, all but one of them as
/// they are.
fn one_line_differs(input: &str, written: &str) -> bool {
    let (input, written): (Vec<_>, Vec<_>) =
        (input.split('\n').collect(), written.split('\n').collect());
    input.len() == written.len()
        && input
            .iter()
            .zip(&written)
            .filter(|(line, written)| line != written)
            .count()
            == 1
}

/// Removes the first inline of a paragraph or heading holding more than
/// one, and adds a link after the rest; false for other blocks.
fn edit_inlines(block: &mut Block) -> bool {
    let children = match block {
        Block::Paragraph(node) => &mut node.children,
        Block::Heading(node) => &mut node.children,
        _ => return false,
    };
    if children.len() > 1 {
        children.remove(0);
    }
    children.push(Text::from(" ").into());
    children.push(Link::new("/new", [Text::from("new")]).into());
    true
}

/// Applies the simulated edits to `input`, pushing each edit whose written
/// text does not read back as the edited tree where its canonical output
/// does, needs the read-back check to write a block canonically, or moves
/// the source of the top-level blocks around an edit inside one; pushes to
/// `leaf_lines`, when given, where each edit of a parsed leaf that changes a
/// line other than that leaf's own was made; returns how many edits were
/// checked. `at` names the input in failures.
pub(crate) fn simulated_edits(
    at: &str,
    input: &str,
    mut leaf_lines: Option<&mut Vec<String>>,
    failures: &mut Vec<String>,
) -> usize {
    let mut edits = 0;
    let parsed = parse(input).document;
    let rebuilt = leaves::rebuilt_from_leaf_sources(&parsed);
    let mut paths = Vec::new();
    sibling_paths(&parsed.children, &mut Vec::new(), &mut paths);
    for (document, path, index) in [&parsed, &rebuilt].into_iter().flat_map(|document| {
        paths
            .iter()
            .map(move |(path, index)| (document, path, *index))
    }) {
        for edit in [
            "leaf",
            "own",
            "delete",
            "insert",
            "before",
            "break",
            "child",
            "inline",
            "task",
            "wide",
            "align",
            "add column",
            "drop column",
            "cell",
        ] {
            let mut edited = document.clone();
            let siblings = siblings_at(&mut edited.children, path);
            // The span of the empty cell the edit fills.
            let mut filled_cell = None;
            let applied = match edit {
                "leaf" => {
                    matches!(
                        siblings[index],
                        Block::Paragraph(_) | Block::Heading(_) | Block::Table(_)
                    ) && edit_block(&mut siblings[index])
                }
                "own" => edit_own_field(&mut siblings[index]),
                "delete" => {
                    siblings.remove(index);
                    true
                }
                "insert" => {
                    siblings.insert(index + 1, Paragraph::new([Text::from("new")]).into());
                    true
                }
                "break" => {
                    siblings.insert(
                        index + 1,
                        Block::ThematicBreak(ThematicBreak {
                            meta: Default::default(),
                            marker: ThematicBreakMarker::Asterisk,
                        }),
                    );
                    true
                }
                "before" => {
                    let at = before(siblings, index);
                    siblings.insert(at, Paragraph::new([Text::from("new")]).into());
                    true
                }
                "align" => realign(&mut siblings[index]),
                "add column" => edit_columns(&mut siblings[index], true),
                "drop column" => edit_columns(&mut siblings[index], false),
                "cell" => match (fill_empty_cell(&mut siblings[index]), &siblings[index]) {
                    (Some((row, cell)), Block::Table(node)) => {
                        filled_cell = node.rows[row].cells[cell].meta.span;
                        true
                    }
                    _ => false,
                },
                "child" => edit_children(&mut siblings[index]),
                "inline" => edit_inlines(&mut siblings[index]),
                "task" => match &mut siblings[index] {
                    Block::List(node) if node.children[0].checked.is_none() => {
                        node.children[0].checked = Some(false);
                        true
                    }
                    _ => false,
                },
                _ => match &mut siblings[index] {
                    Block::List(node) if node.ordered => {
                        let number = node.children[0].number.unwrap_or(1);
                        node.children[0].number = Some(number + 1000);
                        true
                    }
                    _ => false,
                },
            };
            if !applied {
                continue;
            }
            let Ok(canonical) = edited.clone().without_source().to_markdown() else {
                continue;
            };
            if !reads_as(&canonical, &edited) {
                continue;
            }
            edits += 1;
            let leaves = if std::ptr::eq(document, &rebuilt) {
                " (leaves as text)"
            } else {
                ""
            };
            let at = format!("{at}{leaves} {path:?} {index} {edit}");
            // A leaf edited in a parsed document changes its own lines
            // only.
            let outside_the_leaf = (leaf_lines.is_some() && edit == "leaf" && leaves.is_empty())
                .then(|| {
                    let mut original = document.clone();
                    let span = edited_leaf(&siblings_at(&mut original.children, path)[index])?;
                    let body = input[span.start..span.end].trim_end_matches(['\n', '\r']);
                    let start = input[..span.start]
                        .rfind(['\n', '\r'])
                        .map_or(0, |at| at + 1);
                    let end = span.start + body.len();
                    let end = input[end..]
                        .find(['\n', '\r'])
                        .map_or(input.len(), |at| end + at);
                    Some((&input[..start], &input[end..]))
                })
                .flatten();
            match edited.to_markdown() {
                Err(_) => failures.push(format!("{at}: refused with its source only")),
                Ok(markdown) if !reads_as(&markdown, &edited) => {
                    failures.push(format!("{at}: {markdown:?} does not read back"))
                }
                Ok(markdown) if markdown_syntax::__private::rewritten_blocks(&edited) > 0 => {
                    failures.push(format!("{at}: {markdown:?} rewritten by the check"))
                }
                Ok(markdown)
                    if outside_the_leaf.is_some_and(|(before, after)| {
                        !markdown.starts_with(before)
                            || !markdown.ends_with(after)
                            || markdown.len() < before.len() + after.len()
                    }) =>
                {
                    if let Some(leaf_lines) = leaf_lines.as_deref_mut() {
                        leaf_lines.push(at.clone());
                    }
                }
                // A table's new alignments change its delimiter row alone.
                Ok(markdown) if edit == "align" && !one_line_differs(input, &markdown) => failures
                    .push(format!(
                        "{at}: {markdown:?} rewrites more than the delimiter row"
                    )),
                Ok(markdown) if filled_cell.is_some_and(|span| markdown != filled(input, span)) => {
                    failures.push(format!("{at}: {markdown:?} pads the filled cell otherwise"))
                }
                Ok(markdown) => {
                    // An edit inside a top-level block keeps the source
                    // of the blocks before and after it.
                    let Some(Step::Child(top) | Step::Item(top, _)) = path.first() else {
                        continue;
                    };
                    let span = |index: usize| parsed.children[index].span().expect("parsed");
                    // A leading `---` break is written again where a later
                    // `---` line closes it as frontmatter.
                    let fence = |line: &str| line.trim_end_matches([' ', '\t']) == "---";
                    let leading_dash_break = matches!(parsed.children[0], Block::ThematicBreak(_))
                        && input.split(['\n', '\r']).next().is_some_and(fence)
                        && markdown.split(['\n', '\r']).skip(1).any(fence);
                    // A block before it that a blank line and a paragraph
                    // would extend, as an unclosed fence, is closed.
                    let previous = top.checked_sub(1).filter(|_| !leading_dash_break);
                    let previous = previous.and_then(|previous| {
                        let text = &input[span(previous).start..span(previous).end];
                        if closed_by_a_blank_line(text) {
                            Some(previous)
                        } else {
                            previous.checked_sub(1)
                        }
                    });
                    let before = match previous {
                        Some(previous) => &input[..span(previous).end],
                        None => "",
                    };
                    let before = before
                        .strip_suffix("\r\n")
                        .or_else(|| before.strip_suffix(['\n', '\r']))
                        .unwrap_or(before);
                    // Blank lines carry no indentation of their own.
                    let mut kept_from = top + 1;
                    let mut scan = top + 1;
                    while let Some(block) = edited.children.get(scan) {
                        if !matches!(block, Block::BlankLine(_)) {
                            if !rewritten_after_an_edit(input, &edited.children, scan) {
                                break;
                            }
                            kept_from = scan + 1;
                        }
                        scan += 1;
                    }
                    let after = parsed
                        .children
                        .get(kept_from)
                        .map_or("", |_| &input[span(kept_from).start..]);
                    if !markdown.starts_with(before) || !markdown.ends_with(after) {
                        failures.push(format!("{at}: {markdown:?} moves the source around it"))
                    }
                }
            }
        }
    }
    edits
}
