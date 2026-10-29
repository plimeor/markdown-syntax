//! AST to Markdown. The verbs live on [`Document`]
//! ([`to_markdown`](Document::to_markdown) /
//! [`to_markdown_with`](Document::to_markdown_with)); [`SerializeOptions`] tunes
//! the output style. The document is validated first, so serialization can fail
//! with a [`SerializeError`].
//!
//! A document with its source is written partly from it (`reuse`, decision
//! 0020): each node that corresponds to the node its source parses to is
//! written as its source, an edited one with its own syntax copied around
//! what changed, and the rest is rendered. Rendering only renders: each node
//! is written in the spelling the AST records for it, or else by one fixed
//! rule, with text, escapes, and character references as recorded. The output
//! is parsed back only to check the blocks written partly from the source.

use alloc::{borrow::Cow, format, string::String, vec::Vec};

use core::cell::OnceCell;

use crate::{
    ast::*, diagnostic::Diagnostic, leaves, parse::Definitions, parse::InlineSource, span::Span,
    validate::validate_document,
};

mod reuse;

/// The newline style emitted by the serializer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// Unix `\n`.
    Lf,
    /// Windows `\r\n`.
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

/// The bullet char [`SerializeOptions::bullet`] writes unordered lists with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BulletMarker {
    /// `-`.
    Dash,
    /// `*`.
    Asterisk,
    /// `+`.
    Plus,
}

/// The delimiter [`SerializeOptions::ordered_delimiter`] writes ordered-list
/// markers with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderedDelimiter {
    /// `.`, as in `1.`.
    Period,
    /// `)`, as in `1)`.
    Paren,
}

/// Output-style options for serialization. Defaults: LF, trailing newline, and
/// the list markers and code fences the AST records.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SerializeOptions {
    /// Newline style to emit.
    pub line_ending: LineEnding,
    /// Whether to end the output with a trailing newline.
    pub final_newline: bool,
    /// The bullet for unordered lists: `None`, the default, keeps the marker
    /// each list records; `Some` writes every unordered list with it.
    pub bullet: Option<BulletMarker>,
    /// The delimiter for ordered-list markers: `None`, the default, keeps the
    /// delimiter each list records; `Some` writes every ordered list with it.
    pub ordered_delimiter: Option<OrderedDelimiter>,
    /// The fence char for fenced code blocks: `None`, the default, keeps the
    /// fence each block records; `Some` writes every fenced block with it,
    /// except that an info string holding a backtick always takes tildes.
    pub fence_marker: Option<FenceMarker>,
}

impl Default for SerializeOptions {
    fn default() -> Self {
        Self {
            line_ending: LineEnding::Lf,
            final_newline: true,
            bullet: None,
            ordered_delimiter: None,
            fence_marker: None,
        }
    }
}

/// Why serialization failed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SerializeError {
    /// The AST failed validation; carries the validation diagnostics.
    InvalidDocument(Vec<Diagnostic>),
}

/// What the caller of a write vouches for in the leaves that keep the text
/// they were built from (`with_source`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeptTexts {
    /// Nothing: each such leaf's text is read again, and a leaf whose
    /// inlines are not the ones its text reads as is written from them.
    Check,
    /// Each such leaf holds the inlines its text reads as with the
    /// document's definitions, as `reread_kept_sources` leaves them or
    /// `with_source` builds them given those definitions, unchanged since;
    /// no text is read again.
    Trusted,
}

impl Document {
    /// Write this document as Markdown with default options: the source of
    /// every unchanged node, the source of its own syntax around a node whose
    /// children changed, and canonical Markdown for the rest.
    pub fn to_markdown(&self) -> Result<String, SerializeError> {
        self.to_markdown_with(&SerializeOptions::default())
    }

    /// Write this document as Markdown with explicit options. A document
    /// with a source reuses it for each node that corresponds to the node
    /// the source parses to at the same span, and for the own syntax of one
    /// whose children changed; the options apply to the rest.
    pub fn to_markdown_with(&self, options: &SerializeOptions) -> Result<String, SerializeError> {
        Ok(self.write(options)?.0)
    }

    /// The text `to_markdown_with` writes, and how many top-level blocks it
    /// wrote canonically because, written partly from the source, they read
    /// back as something else.
    pub(crate) fn write(
        &self,
        options: &SerializeOptions,
    ) -> Result<(String, usize), SerializeError> {
        self.write_from(None, None, KeptTexts::Check, options)
    }

    /// The text `to_markdown_with` writes for this document's blocks with
    /// the source of `parsed`, a document `parse` returned that is unchanged
    /// since, which the write reads as the parse of that source in place of
    /// parsing it again; and with `sources`, when given, as the inline
    /// sources of its leaves in place of reading them again, which must be
    /// the ones read with that parse. Without a source, `parsed` gives
    /// canonical Markdown. `kept` is what the caller vouches for in the
    /// leaves that keep their text.
    pub(crate) fn write_against(
        &self,
        parsed: &Document,
        sources: Option<&KeptSources>,
        kept: KeptTexts,
        options: &SerializeOptions,
    ) -> Result<String, SerializeError> {
        debug_assert!(parsed
            .source()
            .is_none_or(|source| crate::parse(source).document == *parsed));
        debug_assert!(sources.is_none_or(|sources| *sources == KeptSources::of(parsed)));
        Ok(self.write_from(Some(parsed), sources, kept, options)?.0)
    }

    /// The text `to_markdown_with` writes for this document's blocks, and
    /// how many top-level blocks it wrote canonically, with the source of
    /// `parsed`, the document `parse` returns for that source, or, when
    /// `None`, with this document's own source, parsed here; and with
    /// `sources`, the inline sources of the leaves of `parsed` read with
    /// it, when given; and with what the caller vouches for in the leaves
    /// that keep their text (`kept`).
    fn write_from(
        &self,
        parsed: Option<&Document>,
        sources: Option<&KeptSources>,
        kept: KeptTexts,
        options: &SerializeOptions,
    ) -> Result<(String, usize), SerializeError> {
        let diagnostics = validate_document(self);
        if !diagnostics.is_empty() {
            return Err(SerializeError::InvalidDocument(diagnostics));
        }
        // A leaf keeps its text only while its inlines are the ones the text
        // reads as, which each text is read again for unless `kept` vouches
        // for it; the blocks are copied only to drop a text.
        let current;
        let definitions = DefinitionsOf::new(self);
        let mut misread_leaf = |text: Option<&str>, inlines: &[Inline]| {
            text.is_some_and(|text| {
                crate::parse::parse_inline_tree(text, definitions.get()).inlines != *inlines
            })
        };
        debug_assert!(
            kept == KeptTexts::Check || leaves::find(&self.children, &mut misread_leaf).is_none(),
            "the leaf at {:?} keeps a text that does not read as its inlines",
            leaves::find(&self.children, &mut misread_leaf).unwrap_or_default()
        );
        let blocks = if kept == KeptTexts::Check
            && leaves::find(&self.children, &mut misread_leaf).is_some()
        {
            let mut blocks = self.children.clone();
            leaves::visit_mut(&mut blocks, &mut |leaf| {
                if let Some(text) = leaf.source.as_deref() {
                    if crate::parse::parse_inline_tree(text, definitions.get()).inlines
                        != *leaf.inlines
                    {
                        *leaf.source = None;
                    }
                }
            });
            current = blocks;
            &current
        } else {
            &self.children
        };
        let parsed_here;
        let base = match parsed {
            Some(parsed) => Some(parsed),
            None => {
                parsed_here = self.source().map(crate::parse);
                parsed_here.as_ref().map(|output| &output.document)
            }
        };
        let Some((base, source)) = base.and_then(|base| Some((base, base.source()?))) else {
            return Ok((write_canonical(blocks, options)?, 0));
        };
        // A block written partly from the source stays so only where the
        // output reads back as it at its place.
        let mut correspondence = Correspondence {
            base,
            sources: sources.map(LeafSources::lent),
        };
        let base_definitions = DefinitionsOf::new(base);
        // A first block other than frontmatter that the output opens with
        // from the source stays so only while no later line closes its
        // first line as frontmatter, or its canonical output would be
        // closed as well; every write is checked for it.
        let opens_with_block = !matches!(blocks.first(), None | Some(Block::Frontmatter(_)));
        let mut write = |whole: &mut [bool]| {
            let mut write_once = |whole: &[bool]| {
                write_reusing(
                    blocks,
                    &mut correspondence,
                    &base_definitions,
                    source,
                    options,
                    &definitions,
                    whole,
                )
            };
            let written = write_once(whole)?;
            if !opens_with_block || whole[0] || !opens_frontmatter(&written.0) {
                return Ok(written);
            }
            whole[0] = true;
            let rewritten = write_once(whole)?;
            if opens_frontmatter(&rewritten.0) {
                whole[0] = false;
                return Ok(written);
            }
            Ok(rewritten)
        };
        let mut whole = alloc::vec![false; blocks.len()];
        let (mut output, partly) = write(&mut whole)?;
        if partly.is_empty() {
            return Ok((output, 0));
        }
        let misread = misread(blocks, &output, &partly);
        if misread.is_empty() {
            return Ok((output, 0));
        }
        for index in &misread {
            whole[*index] = true;
        }
        output = write(&mut whole)?.0;
        Ok((output, misread.len()))
    }
}

/// Whether `output` opens with frontmatter: its first line, after any byte
/// order mark, is a fence that a later line closes.
fn opens_frontmatter(output: &str) -> bool {
    let output = output.strip_prefix('\u{feff}').unwrap_or(output);
    let mut lines = output.split(['\n', '\r']);
    let opening = lines.next().and_then(crate::parse::frontmatter_fence_kind);
    opening.is_some() && lines.any(|line| crate::parse::frontmatter_fence_kind(line) == opening)
}

/// The canonical Markdown of a document's top-level blocks.
fn write_canonical(blocks: &[Block], options: &SerializeOptions) -> Result<String, SerializeError> {
    let mut output = write_blocks(blocks, options, Join::Gap, true, None)?;
    if options.line_ending == LineEnding::CrLf {
        output = lf_to_crlf(&output);
    }
    // Blocks end without a line ending, except an HTML block whose last
    // line is empty (the final newline ends that line too) and an indented
    // code block that keeps its value's `\r` or `\r\n` ending.
    // A last blank line always takes its line ending.
    let ends_with_blank_line = matches!(blocks.last(), Some(Block::BlankLine(_)));
    if options.final_newline
        && !blocks.is_empty()
        && (ends_with_blank_line || !ends_with_carriage_return_ending(&output))
    {
        output.push_str(options.line_ending.as_str());
    }
    Ok(output)
}

/// Writes `blocks` against the base of `correspondence`, the parse of
/// `source` whose definitions are `base_definitions`: each
/// block matched to the base block at its span as its source when it
/// corresponds to it, partly from the source when only what it holds
/// changed (`reuse`), and canonically otherwise; returns the output and the
/// indices of the blocks written partly from the source, which `whole`
/// writes canonically instead. Between two blocks written from the source
/// and adjacent there the source's own bytes are kept where they read the
/// same (`reuse::keeps_gap`), as are the bytes before the source's first
/// block and after its last when those blocks are written from it there;
/// every other boundary is a blank line.
///
/// A block is written canonically instead where its source would read
/// differently in its new place: one a blank line and a paragraph after it
/// would change, as they extend an unclosed fence, when it comes before a
/// block other than its successor in the source; one the block before it
/// would take in (`moves_into_previous`); one away from its predecessor
/// that followed a line holding text (`follows_a_text_line`), whose text
/// read alone is not one block of its kind where its canonical output is
/// (`misreads_unlike_its_rendering`); and a first block whose first line a
/// later line closes as frontmatter, which `whole` marks.
fn write_reusing<'a>(
    blocks: &[Block],
    correspondence: &mut Correspondence<'a>,
    base_definitions: &'a DefinitionsOf<'a>,
    source: &'a str,
    options: &'a SerializeOptions,
    definitions: &'a DefinitionsOf<'a>,
    whole: &[bool],
) -> Result<(String, Vec<usize>), SerializeError> {
    let base = &correspondence.base.children[..];
    let mut reuse = reuse::Reuse {
        source,
        options,
        correspondence,
        definitions,
        base_definitions,
    };
    let span_of = |block: &Block| block.span().expect("a reused block has a span");
    let mut outs = Vec::with_capacity(blocks.len());
    // Whether each block written partly from the source is an open list.
    let mut open_lists = alloc::vec![false; blocks.len()];
    let start_of = |base: &Block| base.span().map_or(usize::MAX, |span| span.start);
    // Blocks most often follow their base blocks' order, so the base block
    // after the one matched last is tried first. Base blocks are sorted by
    // start, so one whose neighbours start elsewhere is the only one that
    // starts there, the one the binary search finds.
    let mut next = 0;
    for (index, block) in blocks.iter().enumerate() {
        let found = block.span().and_then(|span| {
            let starts_at = |at: Option<usize>| {
                at.and_then(|at| base.get(at))
                    .is_some_and(|base| start_of(base) == span.start)
            };
            let at = if starts_at(Some(next))
                && !starts_at(next.checked_sub(1))
                && !starts_at(Some(next + 1))
            {
                next
            } else {
                base.binary_search_by_key(&span.start, start_of).ok()?
            };
            (base[at].span() == Some(span)
                && core::mem::discriminant(&base[at]) == core::mem::discriminant(block))
            .then_some(at)
        });
        if let Some(at) = found {
            next = at + 1;
        }
        outs.push(match found {
            // A block equal to its base block, spans included, is kept, as
            // `reuse.block` keeps it: its leaves hold their base inlines, and
            // the spans of the base's siblings stand apart, so each of its
            // nodes matches the base node in its place. Only a top-level
            // block is compared whole, so no node is compared more than
            // twice.
            Some(at) if *block == base[at] => (Some(at), None),
            Some(at) => match reuse.block(block, &base[at])? {
                reuse::Out::Kept(_) => (Some(at), None),
                reuse::Out::Lines(_) | reuse::Out::OpenList(_) if whole[index] => (None, None),
                reuse::Out::Lines(lines) => (None, Some((at, lines))),
                reuse::Out::OpenList(lines) => {
                    open_lists[index] = true;
                    (None, Some((at, lines)))
                }
                reuse::Out::Render => (None, None),
            },
            None => (None, None),
        });
    }
    let mut reused: Vec<Option<usize>> = outs.iter().map(|(at, _)| *at).collect();
    // The blocks written partly from the source, with their place in it.
    let mut partly: Vec<Option<(usize, Vec<reuse::Line>)>> =
        outs.into_iter().map(|(_, lines)| lines).collect();
    // Where each block written from the source, whole or partly, stands in
    // the source.
    let placed =
        |reused: &[Option<usize>], partly: &[Option<(usize, Vec<reuse::Line>)>], index: usize| {
            reused[index].or(partly[index].as_ref().map(|(at, _)| *at))
        };
    if blocks.len() == base.len()
        && reused
            .iter()
            .enumerate()
            .all(|(index, at)| *at == Some(index))
    {
        return Ok((source.into(), Vec::new()));
    }
    // Whether the boundary before each block holds the blank line that
    // separates two blocks; the source between two blocks adjacent in the
    // source is kept only where it still does or does not.
    let separated = separated_boundaries(blocks);
    let base_separated = separated_boundaries(base);
    // Whether the source between the block at `index`, placed at `at`, and
    // the one before it, placed at `at - 1`, is kept: the boundary holds a
    // separating blank line exactly when it did, and the source reads the
    // same between them (`reuse::keeps_gap`).
    let keeps_gap = |partly: &[Option<(usize, Vec<reuse::Line>)>], index: usize, at: usize| {
        if index == 0 || at == 0 || separated[index] != base_separated[at] {
            return false;
        }
        let left = base[at - 1].span().expect("a parsed block has a span");
        let right = base[at].span().expect("a parsed block has a span");
        let left_end = left.start + strip_line_ending(&source[left.start..left.end]).len();
        reuse::keeps_gap(
            source,
            &source[left_end..right.start],
            (
                &blocks[index - 1],
                left,
                partly[index - 1]
                    .as_ref()
                    .map(|(_, lines)| lines.as_slice()),
            ),
            (
                &blocks[index],
                right,
                partly[index].as_ref().map(|(_, lines)| lines.as_slice()),
            ),
            open_lists[index - 1],
            true,
        )
    };
    // Backward, so that each block knows whether its successor stays reused,
    // with the source between them.
    for index in (0..blocks.len().saturating_sub(1)).rev() {
        let Some(at) = placed(&reused, &partly, index) else {
            continue;
        };
        if placed(&reused, &partly, index + 1) == Some(at + 1)
            && keeps_gap(&partly, index + 1, at + 1)
        {
            continue;
        }
        match &partly[index] {
            Some((_, lines)) => {
                let mut text = reuse::join(lines, "\n");
                text.push('\n');
                if !closed_by_a_blank_line(&text) {
                    partly[index] = None;
                }
            }
            None => {
                let span = span_of(&blocks[index]);
                if !closed_by_a_blank_line(&source[span.start..span.end]) {
                    reused[index] = None;
                }
            }
        }
    }

    let mut output = String::with_capacity(source.len());
    // The block written before the current one: its index in the source's
    // parse, where its text ends there, and its line ending there when it was
    // reused, and its marker when it is a list.
    let mut previous_at: Option<usize> = None;
    let mut placement = reuse::Placement::default();
    let mut previous_ending = "";
    let mut previous_body_end = 0;
    // Whether the output ends with the source's own trailing bytes, which
    // take the place of the final newline.
    let mut ends_with_source_tail = false;
    // Whether every block since the last one other than a blank line follows
    // its predecessor in the source.
    let mut run_follows = true;
    for (index, block) in blocks.iter().enumerate() {
        // A list written partly from the source keeps its recorded marker,
        // which a list before it may already use.
        if let (Block::List(list), Some(_)) = (block, &partly[index]) {
            if placement.previous_list() == Some(written_marker(list)) {
                partly[index] = None;
            }
        }
        let follows_predecessor = matches!(
            (previous_at, placed(&reused, &partly, index)),
            (Some(previous), Some(at)) if previous + 1 == at && keeps_gap(&partly, index, at)
        );
        // Blank lines between them do not end a list or a footnote
        // definition before an indented line, so a block follows its
        // predecessor only where every block since the last other than a
        // blank line did.
        let follows_run = follows_predecessor && run_follows;
        run_follows = index == 0 || !matches!(block, Block::BlankLine(_)) || follows_run;
        // A block whose source the block before it could take in stays only
        // after its predecessor in the source, written as it was there.
        let taken_in = |text: &str| placement.takes_in(block, text, follows_run);
        // A block whose first line the source read where it opens no block,
        // after a line holding text, stays away from that line where its
        // text reads alone as a block of its kind, or its canonical output
        // does not either.
        let moved_out_of_context = |position: usize| {
            !follows_predecessor && follows_a_text_line(source, span_of(&base[position]).start)
        };
        if let Some((position, lines)) = &partly[index] {
            if taken_in(&lines[0].body)
                || (moved_out_of_context(*position)
                    && misreads_unlike_its_rendering(block, &reuse::join(lines, "\n"), options))
            {
                partly[index] = None;
            }
        }
        let mut at = reused[index];
        if let Some(position) = at {
            let span = span_of(block);
            let start = span.start;
            // A first block whose first line a later line would close as
            // frontmatter is written canonically (`Document::write`), which
            // `whole` marks, wherever it stood in the source.
            let rewritten = whole[index]
                || taken_in(&source[start..])
                || (moved_out_of_context(position)
                    && misreads_unlike_its_rendering(block, &source[start..span.end], options));
            if rewritten {
                at = None;
            }
        }
        if index > 0 {
            if follows_predecessor {
                let start = span_of(block).start;
                output.push_str(&source[previous_body_end..start]);
            } else if separated[index] {
                // A reused block keeps its own line ending; one more makes
                // the blank line.
                output.push_str(previous_ending);
                match previous_ending {
                    "" => push_gap(&mut output, options),
                    "\r" => output.push('\r'),
                    _ => output.push_str(options.line_ending.as_str()),
                }
            } else {
                // A blank line is a line of its own, without a separator.
                output.push_str(match previous_ending {
                    "" => options.line_ending.as_str(),
                    own => own,
                });
            }
        }
        previous_ending = "";
        match at {
            Some(position) => {
                let span = span_of(block);
                let body = strip_line_ending(&source[span.start..span.end]);
                let body_end = span.start + body.len();
                if index == 0 && position == 0 {
                    output.push_str(&source[..span.start]);
                }
                output.push_str(body);
                if index + 1 == blocks.len() && position + 1 == base.len() {
                    output.push_str(&source[body_end..]);
                    ends_with_source_tail = true;
                }
                previous_ending = &source[body_end..span.end];
                previous_body_end = body_end;
                let marker = match block {
                    Block::List(list) => Some(written_marker(list)),
                    _ => None,
                };
                placement.wrote(block, marker, false);
            }
            None if partly[index].is_some() => {
                let (position, lines) = partly[index].as_ref().expect("written partly");
                let span = base[*position].span().expect("a parsed block has a span");
                let body_end = span.start + strip_line_ending(&source[span.start..span.end]).len();
                if index == 0 && *position == 0 {
                    output.push_str(&source[..span.start]);
                }
                output.push_str(&reuse::join(lines, options.line_ending.as_str()));
                if index + 1 == blocks.len() && *position + 1 == base.len() {
                    output.push_str(&source[body_end..]);
                    ends_with_source_tail = true;
                }
                previous_body_end = body_end;
                let marker = match block {
                    Block::List(list) => Some(written_marker(list)),
                    _ => None,
                };
                placement.wrote(block, marker, open_lists[index]);
                at = Some(*position);
            }
            None => {
                let mut text = write_blocks(
                    core::slice::from_ref(block),
                    options,
                    Join::Gap,
                    index == 0,
                    placement.previous_list(),
                )?;
                if options.line_ending == LineEnding::CrLf {
                    text = lf_to_crlf(&text);
                }
                output.push_str(&text);
                let marker = match block {
                    Block::List(list) => {
                        Some(list_marker(list, options, placement.previous_list()))
                    }
                    _ => None,
                };
                placement.wrote(block, marker, false);
            }
        }
        previous_at = at;
    }
    let ends_with_blank_line = matches!(blocks.last(), Some(Block::BlankLine(_)));
    if !ends_with_source_tail
        && options.final_newline
        && !blocks.is_empty()
        && (ends_with_blank_line || !ends_with_carriage_return_ending(&output))
    {
        // A reused last block keeps its own line ending, which can be part
        // of its value, as in indented code.
        output.push_str(match previous_ending {
            "" => options.line_ending.as_str(),
            own => own,
        });
    }
    let partly = partly
        .iter()
        .enumerate()
        .filter_map(|(index, lines)| lines.as_ref().map(|_| index))
        .collect();
    Ok((output, partly))
}

/// The blocks among `partly`, indices of `blocks` written partly from the
/// source, that `output` reads back as something else in their place: each
/// one that differs, or every one when the output holds a different number
/// of top-level blocks. Only the blocks among `partly` are read whole.
fn misread(blocks: &[Block], output: &str, partly: &[usize]) -> Vec<usize> {
    let (count, read) = crate::parse::parse_top_level_at(output, partly);
    if count != blocks.len() || read.len() != partly.len() {
        return partly.to_vec();
    }
    partly
        .iter()
        .copied()
        .zip(read)
        .filter_map(|(index, read)| (!reads_as(read, &blocks[index])).then_some(index))
        .collect()
}

/// Whether `read`, a block of the parsed output, reads as `block` as
/// "Round-trip stability" compares them, an ordered item without a number
/// reading as the number it is written with.
fn reads_as(mut read: Block, block: &Block) -> bool {
    let read = core::slice::from_mut(&mut read);
    crate::compare::normalize_blocks(read);
    number_items(read);
    let mut block = crate::compare::normalized_blocks(core::slice::from_ref(block));
    number_items(&mut block);
    *read == *block
}

/// Gives each ordered item without a number the one it is written with.
fn number_items(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::List(list) => {
                let mut previous = None;
                for item in &mut list.children {
                    if list.ordered {
                        item.number = Some(item_number(item, &mut previous));
                    }
                    number_items(&mut item.children);
                }
            }
            Block::BlockQuote(node) => number_items(&mut node.children),
            Block::Alert(node) => number_items(&mut node.children),
            Block::FootnoteDefinition(node) => number_items(&mut node.children),
            Block::ContainerDirective(node) => number_items(&mut node.children),
            Block::HtmlContainer(node) => {
                if let HtmlContainerContent::Blocks(children) = &mut node.content {
                    number_items(children);
                }
            }
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
            | Block::BlankLine(_) => {}
        }
    }
}

/// The definitions of a document, read the first time they are asked for:
/// a write reads them only to read a leaf's text or inlines again.
struct DefinitionsOf<'a> {
    document: &'a Document,
    read: OnceCell<Definitions>,
}

impl<'a> DefinitionsOf<'a> {
    fn new(document: &'a Document) -> Self {
        Self {
            document,
            read: OnceCell::new(),
        }
    }

    fn get(&self) -> &Definitions {
        self.read.get_or_init(|| Definitions::of(self.document))
    }
}

/// Whether a block corresponds to a block of the document its source parses
/// to: their spans and own fields are equal and their children correspond,
/// where a leaf that keeps its text corresponds to the leaf at its span whose
/// inline source is that text.
struct Correspondence<'a> {
    base: &'a Document,
    /// The inline source of each leaf of `base` with its span, ordered by
    /// span: lent by a caller that read them with the parse of `base`, or
    /// read here once the first leaf that keeps its text needs it.
    sources: Option<LeafSources<'a>>,
}

impl<'a> Correspondence<'a> {
    /// The inline source of each leaf of the base.
    fn sources(&mut self) -> &mut LeafSources<'a> {
        let base_document = self.base;
        self.sources
            .get_or_insert_with(|| LeafSources::owned(KeptSources::of(base_document)))
    }

    /// The inline source of the base leaf at `span`.
    fn source_of(&mut self, span: Span) -> Option<&InlineSource> {
        self.sources().get(span)
    }

    fn holds(&mut self, block: &Block, base: &Block) -> bool {
        if !leaves::any_kept(core::slice::from_ref(block)) {
            return block == base;
        }
        let sources = self.sources();
        // Each leaf whose text is its base leaf's inline source takes the
        // inlines that text reads as there.
        let mut block = block.clone();
        leaves::visit_mut(core::slice::from_mut(&mut block), &mut |leaf| {
            let (Some(text), Some(span)) = (leaf.source.as_deref(), leaf.span) else {
                return;
            };
            if let Some(source) = sources.get(span).filter(|source| source.text == text) {
                crate::parse::inline_spans_to_source(source, leaf.inlines);
                *leaf.source = None;
            }
        });
        block == *base
    }
}

/// The inline source of each leaf of a document with its span, ordered by
/// span, leaves at one span in document order. Leaves share a span where a
/// table row is padded with empty cells after its last one: each padding
/// cell has the empty span at the row's end and an empty inline source.
#[derive(Debug)]
pub(crate) struct KeptSources {
    entries: Vec<(Span, InlineSource)>,
}

impl KeptSources {
    /// The sources of `entries`, leaves in document order with their spans.
    pub(crate) fn new(mut entries: Vec<(Span, InlineSource)>) -> Self {
        // Spans follow document order, so the entries are most often sorted
        // already and skip the sort's scratch buffer; a stable sort keeps the
        // order of leaves at one span otherwise.
        if !entries.is_sorted_by_key(|(span, _)| (span.start, span.end)) {
            entries.sort_by_key(|(span, _)| (span.start, span.end));
        }
        Self { entries }
    }

    /// The sources of the leaves of `document`, read from its source.
    fn of(document: &Document) -> Self {
        let mut entries = Vec::new();
        document.each_leaf_source(|_, span, source| entries.push((span, source)));
        Self::new(entries)
    }
}

/// Two sets of sources are equal where they hold the same leaves at the same
/// spans, each with the same text read from the same source runs.
impl PartialEq for KeptSources {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().zip(&other.entries).all(|(one, other)| {
                one.0 == other.0 && one.1.text == other.1.text && one.1.map() == other.1.map()
            })
    }
}

/// Kept sources as a write looks leaves up in them, owned or lent.
struct LeafSources<'a> {
    entries: Cow<'a, [(Span, InlineSource)]>,
    /// The entry after the one found last.
    next: usize,
}

impl<'a> LeafSources<'a> {
    fn owned(kept: KeptSources) -> Self {
        Self {
            entries: Cow::Owned(kept.entries),
            next: 0,
        }
    }

    fn lent(kept: &'a KeptSources) -> Self {
        Self {
            entries: Cow::Borrowed(&kept.entries),
            next: 0,
        }
    }

    /// The inline source of the leaf at `span`: the last of the leaves
    /// there, each of which takes the place of the one before it. Leaves are
    /// most often asked for in document order, so the entry after the one
    /// found last is tried first.
    fn get(&mut self, span: Span) -> Option<&InlineSource> {
        let entries = &self.entries;
        let next = self.next;
        let last_at = |at: usize| {
            entries[at].0 == span && entries.get(at + 1).is_none_or(|after| after.0 != span)
        };
        let at = if next < entries.len() && last_at(next) {
            next
        } else {
            let key = (span.start, span.end);
            let after = entries.partition_point(|(other, _)| (other.start, other.end) <= key);
            after.checked_sub(1).filter(|&at| entries[at].0 == span)?
        };
        self.next = at + 1;
        Some(&entries[at].1)
    }
}

/// For each block, whether the boundary before it holds the blank line that
/// separates two blocks: one written after a block other than a blank line,
/// before a block or before the blank lines that precede one.
fn separated_boundaries(blocks: &[Block]) -> Vec<bool> {
    let mut separated = alloc::vec![false; blocks.len()];
    let mut block_follows = false;
    for index in (1..blocks.len()).rev() {
        block_follows |= !matches!(blocks[index], Block::BlankLine(_));
        separated[index] = block_follows && !matches!(blocks[index - 1], Block::BlankLine(_));
    }
    separated
}

/// Whether a block written from the source, whose text opens with `text`,
/// reads differently where it does not follow its predecessor in the
/// source, after `last`, the last block written before it other than a blank
/// line (`None` for one written first), and `previous_list`, the marker of a
/// list written just before it:
/// indentation after a list or a footnote definition, which blank lines do
/// not end, continues them; four columns of it on a block other than
/// indented code read as indented code, as a paragraph opened by definitions
/// took them as a continuation; and a list after a list written with its
/// marker continues that list. A blank line reads as one wherever it stands.
/// `rewritten_after_an_edit` in `tests/support/edits.rs` states the same rule
/// for the simulated edits.
fn moves_into_previous(
    block: &Block,
    text: &str,
    last: Option<&Block>,
    previous_list: Option<char>,
) -> bool {
    match block {
        Block::BlankLine(_) => false,
        Block::List(list) if previous_list == Some(written_marker(list)) => true,
        _ if matches!(last, Some(Block::List(_) | Block::FootnoteDefinition(_))) => {
            text.starts_with([' ', '\t'])
        }
        Block::CodeBlock(_) => false,
        _ => opens_with_code_indent(text),
    }
}

/// Whether `text` opens with four columns of indentation, which starts
/// indented code where no paragraph continues.
fn opens_with_code_indent(text: &str) -> bool {
    let mut column = 0;
    for char in text.chars() {
        match char {
            ' ' => column += 1,
            '\t' => column += 4 - column % 4,
            _ => break,
        }
        if column >= 4 {
            return true;
        }
    }
    false
}

/// Whether the block parsed from `text`, its source with its line ending,
/// keeps that text when a blank line and a paragraph follow it, as a closed
/// fence does and an unclosed one does not. `tests/support/edits.rs` holds a
/// copy.
fn closed_by_a_blank_line(text: &str) -> bool {
    let gap = if text.ends_with('\n') {
        "\n"
    } else if text.ends_with('\r') {
        "\r"
    } else {
        "\n\n"
    };
    let probe = format!("{text}{gap}x");
    let children = crate::parse(&probe).document.children;
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

/// Whether the line holding `start` in `source` follows a line holding
/// text, which a block starting there continued where it cannot interrupt
/// a paragraph: a table's header row, a paragraph after a definition's.
fn follows_a_text_line(source: &str, start: usize) -> bool {
    let line_start = source[..start].rfind(['\n', '\r']).map_or(0, |at| at + 1);
    let before = strip_line_ending(&source[..line_start]);
    let previous = &before[before.rfind(['\n', '\r']).map_or(0, |at| at + 1)..];
    !crate::parse::is_blank(previous)
}

/// Whether `text`, the source of `block`, reads as one block of its kind
/// where it opens a block after a blank line, where no frontmatter opens: a
/// first line that opens another block there, as `2) a | b` opens a list
/// that cannot interrupt a paragraph, does not.
fn reads_alone_as(block: &Block, text: &str) -> bool {
    let probe = format!("\n{text}");
    matches!(
        crate::parse(&probe).document.children.as_slice(),
        [Block::BlankLine(_), only]
            if core::mem::discriminant(only) == core::mem::discriminant(block)
    )
}

/// Whether `text`, the source of `block`, does not read alone as one block
/// of its kind (`reads_alone_as`) while the block's canonical output does: a
/// paragraph's, written as its text, reads as its source.
fn misreads_unlike_its_rendering(block: &Block, text: &str, options: &SerializeOptions) -> bool {
    !reads_alone_as(block, text)
        && write_blocks(core::slice::from_ref(block), options, Join::Gap, true, None)
            .is_ok_and(|written| reads_alone_as(block, &written))
}

/// Pushes the gap between two top-level blocks in the options' line ending.
fn push_gap(output: &mut String, options: &SerializeOptions) {
    let from = output.len();
    push_block_gap(output);
    if options.line_ending == LineEnding::CrLf {
        let gap = lf_to_crlf(&output[from..]);
        output.truncate(from);
        output.push_str(&gap);
    }
}

/// `text` without one trailing `\n`, `\r\n`, or `\r`.
fn strip_line_ending(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}

/// Rewrites each bare `\n` as `\r\n`; existing `\r\n` and `\r` endings that
/// verbatim values carry stay as they are.
fn lf_to_crlf(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut previous = '\0';
    for ch in input.chars() {
        if ch == '\n' && previous != '\r' {
            output.push('\r');
        }
        output.push(ch);
        previous = ch;
    }
    output
}

fn ends_with_carriage_return_ending(input: &str) -> bool {
    input.ends_with('\r') || input.ends_with("\r\n")
}

/// Separates two blocks with one blank line. A block that already ends its
/// last line with `\r` or `\r\n` gets only the blank line, written so that it
/// cannot merge into that ending.
fn push_block_gap(output: &mut String) {
    if output.ends_with('\r') {
        output.push('\r');
    } else if output.ends_with("\r\n") {
        output.push('\n');
    } else {
        output.push_str("\n\n");
    }
}

/// How sibling blocks are separated.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Join {
    /// By a blank line.
    Gap,
    /// By a line ending alone, as the blocks of a tight list item are.
    Line,
}

/// Writes a sequence of sibling blocks. `document_start` is true only for the
/// top-level document body, where the first block sits at byte 0 and a
/// contiguous `---` would open frontmatter.
///
/// Two blocks are separated by `join`, and each `BlankLine` is one more empty
/// line: between two blocks after their separator, and before the first
/// block or after the last on its own.
fn write_blocks(
    blocks: &[Block],
    options: &SerializeOptions,
    join: Join,
    document_start: bool,
    previous_list: Option<char>,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    write_blocks_into(
        &mut output,
        blocks,
        options,
        join,
        document_start,
        previous_list,
    )?;
    Ok(output)
}

/// Writes what `write_blocks` returns into `output`, which is empty.
fn write_blocks_into(
    output: &mut String,
    blocks: &[Block],
    options: &SerializeOptions,
    join: Join,
    document_start: bool,
    previous_list: Option<char>,
) -> Result<(), SerializeError> {
    debug_assert!(output.is_empty());
    // The marker of the list written right before the current block; ordered
    // and unordered markers never share a char.
    let mut previous_list = previous_list;
    // Whether a block other than a blank line was written, and the blank
    // lines since the last one.
    let mut wrote_block = false;
    let mut blank_lines = 0;
    for (index, block) in blocks.iter().enumerate() {
        if let Block::BlankLine(_) = block {
            blank_lines += 1;
            continue;
        }
        if wrote_block {
            match join {
                Join::Gap => push_block_gap(output),
                Join::Line => output.push('\n'),
            }
        }
        output.extend(core::iter::repeat_n('\n', blank_lines));
        blank_lines = 0;
        match block {
            Block::List(list) => {
                let marker = list_marker(list, options, previous_list);
                previous_list = Some(marker);
                push_list(output, list, options, marker)?;
            }
            Block::ThematicBreak(node) => {
                previous_list = None;
                output.push_str(&write_thematic_break(node, document_start && index == 0));
            }
            _ => {
                previous_list = None;
                push_block(output, block, options)?;
            }
        }
        wrote_block = true;
    }
    // Trailing blank lines each end the line before them; with no block,
    // the first of them starts the output.
    let trailing = if wrote_block {
        blank_lines
    } else {
        blank_lines.saturating_sub(1)
    };
    output.extend(core::iter::repeat_n('\n', trailing));
    Ok(())
}

/// What `push_block` pushes for `block`.
fn write_block(block: &Block, options: &SerializeOptions) -> Result<String, SerializeError> {
    let mut output = String::new();
    push_block(&mut output, block, options)?;
    Ok(output)
}

/// Pushes `block` to `output` as canonical Markdown, and a leaf that keeps
/// its text as that text: `to_markdown_with` has cleared every kept text its
/// leaf's inlines no longer read as. A paragraph is written in place.
fn push_block(
    output: &mut String,
    block: &Block,
    options: &SerializeOptions,
) -> Result<(), SerializeError> {
    let written = match block {
        Block::Paragraph(node) => {
            return match &node.source {
                Some(text) => {
                    output.push_str(text);
                    Ok(())
                }
                None => push_inlines(output, &node.children, Context::BLOCK),
            };
        }
        Block::BlankLine(_) => Ok(String::new()),
        Block::Heading(node) => write_heading(node),
        Block::ThematicBreak(node) => Ok(write_thematic_break(node, false)),
        Block::BlockQuote(node) => {
            let inner = write_blocks(&node.children, options, Join::Gap, false, None)?;
            // No content, or one blank line, is one `> ` line, the space
            // opening the quote, as it does before a first blank line.
            Ok(if inner.is_empty() {
                "> ".into()
            } else {
                let mut written = quote_lines(&inner);
                if written.starts_with(">\n") || written.starts_with(">\r") {
                    written.insert(1, ' ');
                }
                written
            })
        }
        Block::Alert(node) => write_alert(node, options),
        Block::List(node) => write_list(node, options, list_marker(node, options, None)),
        Block::CodeBlock(node) => Ok(write_code_block(node, options)),
        // An HTML block's lines are joined with `\n`, so a value ending in one
        // ends with an empty line that belongs to the block (an unclosed
        // comment, say); it is written as it is.
        Block::HtmlBlock(node) => Ok(node.value.clone()),
        Block::HtmlContainer(node) => write_html_container(node, options),
        Block::Definition(node) => Ok(write_definition(node)),
        Block::FootnoteDefinition(node) => {
            let inner = write_blocks(&node.children, options, Join::Gap, false, None)?;
            // Content opening with a space or a tab (indented code, say)
            // starts on the line after the label, as in a list item, where
            // the label's line would take that whitespace as its separator.
            Ok(if inner.starts_with([' ', '\t']) {
                format!(
                    "[^{}]:\n{}",
                    node.label,
                    prefix_container_lines(&inner, "    ")
                )
            } else {
                format!("[^{}]: {}", node.label, indent_continuation(&inner))
            })
        }
        Block::Table(node) => write_table(node),
        Block::MathBlock(node) => {
            let fence = block_math_fence(&node.value);
            Ok(fenced_body(&fence, &node.value, &fence))
        }
        Block::Frontmatter(node) => {
            let fence = match node.kind {
                FrontmatterKind::Yaml => "---",
                FrontmatterKind::Toml => "+++",
            };
            // The value holds the lines between the fences joined with `\n`,
            // so a final `\n` is an empty last line.
            Ok(format!("{fence}\n{}\n{fence}", node.value))
        }
        Block::LeafDirective(node) => Ok(format!(
            "::{}{}{}",
            node.name,
            write_directive_label(&node.label, node.label_source.as_deref())?,
            write_attributes(&node.attributes)
        )),
        Block::ContainerDirective(node) => {
            let mut inner = write_blocks(&node.children, options, Join::Gap, false, None)?;
            let fence = directive_fence(&inner);
            // Content ends with a line ending before the closing fence; an
            // empty directive takes no blank line, which would loosen a list
            // holding it.
            if !node.children.is_empty() {
                inner.push('\n');
            }
            Ok(format!(
                "{fence}{}{}{}\n{inner}{fence}",
                node.name,
                write_directive_label(&node.label, node.label_source.as_deref())?,
                write_attributes(&node.attributes)
            ))
        }
    };
    output.push_str(&written?);
    Ok(())
}

/// A thematic break. A dash break is written spaced at the document start,
/// where a contiguous `---` would open frontmatter.
fn write_thematic_break(node: &ThematicBreak, at_document_start: bool) -> String {
    match node.marker {
        ThematicBreakMarker::Dash if at_document_start => "- - -".into(),
        ThematicBreakMarker::Dash => "---".into(),
        ThematicBreakMarker::Asterisk => "***".into(),
        ThematicBreakMarker::Underscore => "___".into(),
    }
}

/// An ATX heading. An empty one is its `#` run and a space, which opens it.
fn write_heading(node: &Heading) -> Result<String, SerializeError> {
    let mut output: String = core::iter::repeat_n('#', usize::from(node.depth)).collect();
    output.push(' ');
    let content = output.len();
    match one_line(node.source.as_deref(), true) {
        Some(text) => output.push_str(text),
        None => output.push_str(&write_inlines(&node.children, Context::HEADING)?),
    }
    // Content ending in what would read as the closing sequence keeps it by
    // a closing sequence of its own.
    if reads_as_closing_sequence(&output[content..]) {
        output.push_str(" #");
    }
    Ok(output)
}

/// Whether written ATX content ends in a `#` run that is all of it or follows
/// a space or a tab, which a reader strips as the closing sequence.
fn reads_as_closing_sequence(content: &str) -> bool {
    let before = content.trim_end_matches('#');
    before.len() < content.len() && (before.is_empty() || before.ends_with([' ', '\t']))
}

fn write_definition(node: &Definition) -> String {
    let mut output = format!("[{}]: ", node.label);
    push_destination(&mut output, &node.destination, node.destination_kind);
    if let Some(title) = &node.title {
        output.push(' ');
        push_title(&mut output, title);
    }
    output
}

fn write_html_container(
    node: &HtmlContainer,
    options: &SerializeOptions,
) -> Result<String, SerializeError> {
    match &node.content {
        HtmlContainerContent::Blocks(children) => {
            let inner = write_blocks(children, options, Join::Gap, false, None)?;
            // Content takes a blank line before the closing tag; blank lines
            // alone stand between the tags.
            let blank_lines = children
                .iter()
                .all(|block| matches!(block, Block::BlankLine(_)));
            Ok(if blank_lines {
                format!(
                    "{}\n{}{}",
                    node.opening.raw,
                    "\n".repeat(children.len()),
                    node.closing.raw
                )
            } else {
                format!("{}\n{}\n\n{}", node.opening.raw, inner, node.closing.raw)
            })
        }
        HtmlContainerContent::Inlines(children) => Ok(format!(
            "{}{}{}",
            node.opening.raw,
            write_inlines(children, Context::BLOCK)?,
            node.closing.raw
        )),
    }
}

fn write_alert(node: &Alert, options: &SerializeOptions) -> Result<String, SerializeError> {
    let mut output = String::from("> [!");
    output.push_str(alert_kind_name(node.kind));
    output.push(']');
    if let Some(title) = &node.title {
        if !title.is_empty() {
            output.push(' ');
            // A line ending would end the marker's line.
            output.push_str(&title.replace(['\n', '\r'], " "));
        }
    }
    let inner = write_blocks(&node.children, options, Join::Gap, false, None)?;
    if !inner.is_empty() {
        output.push('\n');
        output.push_str(&quote_lines(&inner));
    } else if !node.children.is_empty() {
        // One blank line.
        output.push_str("\n>");
    }
    Ok(output)
}

/// `inner` with every line in the quote: an empty last line, which a value
/// ending in a line ending holds, takes `>` too.
fn quote_lines(inner: &str) -> String {
    let mut output = prefix_container_lines(inner, "> ");
    if inner.ends_with(['\n', '\r']) {
        output.push('>');
    }
    output
}

fn alert_kind_name(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "NOTE",
        AlertKind::Tip => "TIP",
        AlertKind::Important => "IMPORTANT",
        AlertKind::Warning => "WARNING",
        AlertKind::Caution => "CAUTION",
    }
}

/// The marker char a list is written with: the one the options replace it
/// with, or else the one the AST records. A marker that the list written right
/// before it in the same container, `previous`, also uses yields to the next
/// one in the order `-`, `*`, `+`, or `.`, `)`; validation keeps two adjacent
/// recorded markers apart, so only a replaced marker can meet this.
fn list_marker(list: &List, options: &SerializeOptions, previous: Option<char>) -> char {
    let marker = if list.ordered {
        options
            .ordered_delimiter
            .map_or_else(|| written_marker(list), ordered_delimiter_char)
    } else {
        options
            .bullet
            .map_or_else(|| written_marker(list), bullet_char)
    };
    if previous == Some(marker) {
        let order: &[char] = if list.ordered {
            &['.', ')']
        } else {
            &['-', '*', '+']
        };
        let at = order.iter().position(|char| *char == marker).unwrap_or(0);
        return order[(at + 1) % order.len()];
    }
    marker
}

/// The marker char the AST records for a list: its bullet, or its ordered
/// delimiter. A delimiter of the other list kind is written `-` or `.`.
/// Serialization and validation's adjacent-list rule both read it.
pub(crate) fn written_marker(list: &List) -> char {
    match (list.ordered, list.delimiter) {
        (false, ListDelimiter::Asterisk) => '*',
        (false, ListDelimiter::Plus) => '+',
        (false, _) => '-',
        (true, ListDelimiter::Paren) => ')',
        (true, _) => '.',
    }
}

fn bullet_char(bullet: BulletMarker) -> char {
    match bullet {
        BulletMarker::Dash => '-',
        BulletMarker::Asterisk => '*',
        BulletMarker::Plus => '+',
    }
}

fn ordered_delimiter_char(delimiter: OrderedDelimiter) -> char {
    match delimiter {
        OrderedDelimiter::Period => '.',
        OrderedDelimiter::Paren => ')',
    }
}

fn write_list(
    node: &List,
    options: &SerializeOptions,
    marker_char: char,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    push_list(&mut output, node, options, marker_char)?;
    Ok(output)
}

/// Pushes `node` as `write_list` writes it.
fn push_list(
    output: &mut String,
    node: &List,
    options: &SerializeOptions,
    marker_char: char,
) -> Result<(), SerializeError> {
    // The number the previous item was written with.
    let mut previous: Option<u64> = None;
    let mut inner = String::new();
    for (index, item) in node.children.iter().enumerate() {
        if index > 0 {
            output.push_str(if node.tight { "\n" } else { "\n\n" });
        }
        push_item(
            output,
            node,
            item,
            options,
            marker_char,
            &mut previous,
            &mut inner,
        )?;
    }
    Ok(())
}

/// The number an ordered item is written with: its own, or else one more than
/// the previous item's, `previous`, which it then becomes.
fn item_number(item: &ListItem, previous: &mut Option<u64>) -> u64 {
    // An item without a number counts on from the previous one, and stops at
    // the largest number a marker can hold.
    let number = item.number.unwrap_or_else(|| {
        previous.map_or(1, |previous| {
            previous
                .saturating_add(1)
                .min(crate::parse::MAX_ORDERED_NUMBER)
        })
    });
    *previous = Some(number);
    number
}

/// Whether `block`, as the serializer writes it with default options, opens
/// with a backtick or a tilde after fewer than four spaces: a line the
/// parser does not take as a quote's paragraph's lazy line.
pub(crate) fn written_opens_fence_like(block: &Block) -> bool {
    opens_fence_like(&written_opening(block, &SerializeOptions::default()))
}

/// Whether `line` opens with a backtick or a tilde after fewer than four
/// spaces.
fn opens_fence_like(line: &str) -> bool {
    let content = line.trim_start_matches(' ');
    line.len() - content.len() < 4 && content.starts_with(['`', '~'])
}

/// One item of `list`, written with `marker_char`; `previous` is the number
/// the item before it was written with. Its content starts on its marker's
/// line, after one space: an item with no content is its marker and the
/// space, which opens it.
fn write_item(
    list: &List,
    item: &ListItem,
    options: &SerializeOptions,
    marker_char: char,
    previous: &mut Option<u64>,
) -> Result<String, SerializeError> {
    let mut output = String::new();
    push_item(
        &mut output,
        list,
        item,
        options,
        marker_char,
        previous,
        &mut String::new(),
    )?;
    Ok(output)
}

/// Pushes one item of `list` as `write_item` writes it, its blocks written
/// in `inner` before they are indented.
fn push_item(
    output: &mut String,
    list: &List,
    item: &ListItem,
    options: &SerializeOptions,
    marker_char: char,
    previous: &mut Option<u64>,
    inner: &mut String,
) -> Result<(), SerializeError> {
    let from = output.len();
    if list.ordered {
        let number = item_number(item, previous);
        // Writing to a `String` cannot fail.
        let _ = core::fmt::Write::write_fmt(output, format_args!("{number}{marker_char} "));
    } else {
        output.push(marker_char);
        output.push(' ');
    }
    let width = output.len() - from;
    inner.clear();
    write_item_blocks(inner, &item.children, options, list.tight, item.checked)?;
    push_indented(output, inner, width);
    Ok(())
}

/// Whether `item` of `list`, written with `marker_char`, reads back as
/// something else, `opening` being the opening (`opening`) of the first line
/// its blocks are written as: where that line would read as padding after
/// the marker, or, after a bullet, as one thematic break with it (`- --`, or
/// the `- - -` of three nested empty items). An item's content cannot start
/// below its marker, since a marker with nothing after it on its line opens
/// an empty item.
fn misreads_after_marker(list: &List, item: &ListItem, marker_char: char, opening: &str) -> bool {
    opens_with_padding(opening)
        || (!list.ordered
            && item.checked.is_none()
            && reads_as_thematic_break(marker_char, opening))
}

/// The chars that pad a line's content: spaces and tabs.
const PADDING: [char; 2] = [' ', '\t'];

/// The chars a thematic break that a bullet can open is made of.
const BREAK_CHARS: [char; 2] = ['-', '*'];

/// Whether `char` goes on an opening (`opening`): padding, or a char of a
/// thematic break, which is all `opens_with_padding`,
/// `reads_as_thematic_break`, and the fence check of
/// `written_opens_fence_like` read a line through.
fn continues_opening(char: char) -> bool {
    PADDING.contains(&char) || BREAK_CHARS.contains(&char)
}

/// Whether `marker_char`, a space, and `rest` read as a thematic break of
/// that char: three or more `-` or `*`, with only spaces and tabs between.
pub(super) fn reads_as_thematic_break(marker_char: char, rest: &str) -> bool {
    BREAK_CHARS.contains(&marker_char)
        && rest
            .chars()
            .all(|char| PADDING.contains(&char) || char == marker_char)
        && rest.chars().filter(|char| *char == marker_char).count() >= 2
}

/// The opening (`opening`) of the first line the serializer writes `item` of
/// `list` with, its marker `marker_char` and `options`' text included.
fn item_opening(
    list: &List,
    item: &ListItem,
    marker_char: char,
    options: &SerializeOptions,
) -> String {
    let marker = if list.ordered {
        format!("{}{marker_char}", item.number.unwrap_or(1))
    } else {
        String::from(marker_char)
    };
    format!("{marker} {}", item_content_opening(item, options))
}

/// The opening (`opening`) of the first line of `item`'s blocks as the
/// serializer writes them with `options`: a nested list's first item is
/// followed into, and a leaf block is written to tell.
fn item_content_opening(item: &ListItem, options: &SerializeOptions) -> String {
    if item.checked.is_some() {
        return String::from("[");
    }
    match item.children.first() {
        None => String::new(),
        Some(Block::List(nested)) => match nested.children.first() {
            Some(first) => item_opening(nested, first, list_marker(nested, options, None), options),
            None => String::new(),
        },
        Some(
            block @ (Block::Paragraph(_)
            | Block::Heading(_)
            | Block::HtmlBlock(_)
            | Block::CodeBlock(_)
            | Block::ThematicBreak(_)),
        ) => written_opening(block, options),
        // Every other block opens with syntax of its own.
        Some(_) => String::from(">"),
    }
}

/// The opening (`opening`) of the first line `push_block` writes `block`
/// as with `options`, written only as far as that where it can be: a
/// paragraph's inlines each add to what the ones before them wrote.
fn written_opening(block: &Block, options: &SerializeOptions) -> String {
    let written = match block {
        Block::Paragraph(node) => match &node.source {
            Some(text) => Ok(String::from(opening(text))),
            None => write_opening(&node.children, Context::BLOCK),
        },
        Block::HtmlBlock(node) => Ok(String::from(opening(&node.value))),
        _ => write_block(block, options).map(|written| String::from(opening(&written))),
    };
    written.unwrap_or_default()
}

/// The opening of `text`'s first line: up to its line ending, or through
/// its first char that does not continue an opening (`continues_opening`).
/// It tells what the whole line does about reading as padding or a thematic
/// break after a list marker, or as a fence after a quote's lazy line.
fn opening(text: &str) -> &str {
    match text
        .char_indices()
        .find(|(_, char)| !continues_opening(*char))
    {
        Some((at, '\n' | '\r')) => &text[..at],
        Some((at, char)) => &text[..at + char.len_utf8()],
        None => text,
    }
}

/// Spaces or a tab opening an item's written content would read as padding
/// after the marker. Four spaces or more open indented code, which the
/// marker line holds after one space.
fn opens_with_padding(inner: &str) -> bool {
    !inner.starts_with("    ") && inner.starts_with(PADDING)
}

/// Whether, with default options, the serializer writes `item` of `list`
/// so that it reads back as something else (see
/// [`misreads_after_marker`]). Only the opening of the item's first line is
/// written to tell.
pub(crate) fn item_misreads_after_marker(list: &List, item: &ListItem) -> bool {
    // An opening of one char that ends it, a checkbox's `[` or the char a
    // paragraph's first inline opens with, reads as neither.
    let ends_opening = item.checked.is_some()
        || match item.children.first() {
            Some(Block::Paragraph(node)) => {
                node.source.is_none() && node.children.first().and_then(opening_char).is_some()
            }
            _ => false,
        };
    if ends_opening {
        return false;
    }
    let options = SerializeOptions::default();
    let content = item_content_opening(item, &options);
    misreads_after_marker(list, item, written_marker(list), &content)
}

/// Writes an item's blocks into `output`, which is empty; a task item's,
/// `task` holding whether it is checked, open with its checkbox: at the
/// start of the first paragraph after the definitions the item starts with,
/// or else of the item.
fn write_item_blocks(
    output: &mut String,
    blocks: &[Block],
    options: &SerializeOptions,
    tight: bool,
    task: Option<bool>,
) -> Result<(), SerializeError> {
    let join = if tight { Join::Line } else { Join::Gap };
    let Some(checked) = task else {
        return write_blocks_into(output, blocks, options, join, false, None);
    };
    let checkbox = if checked { "[x]" } else { "[ ]" };
    let definitions = blocks
        .iter()
        .take_while(|block| matches!(block, Block::Definition(_)))
        .count();
    if matches!(blocks.get(definitions), Some(Block::Paragraph(_))) {
        write_blocks_into(output, &blocks[..definitions], options, join, false, None)?;
        let rest = write_blocks(&blocks[definitions..], options, join, false, None)?;
        if !output.is_empty() {
            match join {
                Join::Gap => push_block_gap(output),
                Join::Line => output.push('\n'),
            }
        }
        output.push_str(checkbox);
        output.push(' ');
        output.push_str(&rest);
        return Ok(());
    }
    let written = write_blocks(blocks, options, join, false, None)?;
    output.push_str(checkbox);
    if !written.is_empty() {
        output.push(' ');
        output.push_str(&written);
    }
    Ok(())
}

fn write_code_block(node: &CodeBlock, options: &SerializeOptions) -> String {
    match node.kind {
        CodeBlockKind::Indented => {
            // Each value line ends with a line ending; the block gap or the
            // final newline writes a last `\n`, so only `\r` and `\r\n` stay.
            let body = trim_trailing_newline(&node.value);
            let mut output = prefix_lines(body, "    ");
            let ending = &node.value[body.len()..];
            if matches!(ending, "\r" | "\r\n") {
                output.push_str(ending);
            }
            output
        }
        CodeBlockKind::Fenced { marker, length } => {
            let marker = code_block_fence_marker(node, marker, options);
            let (fence, indent) = code_block_fence(&node.value, marker, length.max(3));
            let mut opener = fence.clone();
            if let Some(info) = &node.info {
                let info = escape_code_info(info);
                // The info string follows the fence, apart from one that
                // opens with the fence's char, which would lengthen it.
                if fence.ends_with(|char| info.starts_with(char)) {
                    opener.push(' ');
                }
                opener.push_str(&info);
            }
            let body = fenced_body(&opener, &node.value, &fence);
            if indent == 0 {
                body
            } else {
                prefix_lines(&body, &" ".repeat(indent))
            }
        }
    }
}

/// A fenced block: `opener`, then `value` (whose lines each keep their line
/// ending, the last one optionally), then `closer`. An empty value writes no
/// line between the fences.
fn fenced_body(opener: &str, value: &str, closer: &str) -> String {
    let mut output = String::with_capacity(opener.len() + value.len() + closer.len() + 2);
    output.push_str(opener);
    output.push('\n');
    output.push_str(value);
    if !value.is_empty() && !ends_with_line_ending(value) {
        output.push('\n');
    }
    output.push_str(closer);
    output
}

/// The fence char a code block is written with: tildes when its info string
/// holds a backtick, which a backtick fence's info string cannot, or else the
/// char the AST records or the options replace it with.
fn code_block_fence_marker(
    node: &CodeBlock,
    marker: FenceMarker,
    options: &SerializeOptions,
) -> FenceMarker {
    if node.info.as_deref().is_some_and(|info| info.contains('`')) {
        return FenceMarker::Tilde;
    }
    options.fence_marker.unwrap_or(marker)
}

fn escape_code_info(input: &str) -> String {
    let mut output = String::new();
    // The parser trims the info string, so whitespace at either end is
    // written as a character reference.
    let inner_start = input.len() - input.trim_start_matches([' ', '\t']).len();
    let inner_end = input.trim_end_matches([' ', '\t']).len().max(inner_start);
    for (offset, char) in input.char_indices() {
        match char {
            ' ' | '\t' if offset < inner_start || offset >= inner_end => {
                output.push_str(&char_reference(char));
            }
            '\t' => output.push(char),
            char if char.is_control() => output.push_str(&char_reference(char)),
            '\\' | '&' => {
                output.push('\\');
                output.push(char);
            }
            _ => output.push(char),
        }
    }
    output
}

/// A hexadecimal character reference for `char`.
fn char_reference(char: char) -> String {
    format!("&#x{:X};", char as u32)
}

fn write_table(node: &Table) -> Result<String, SerializeError> {
    let mut output = String::new();
    let mut written = String::new();
    for (row_index, row) in node.rows.iter().enumerate() {
        if row_index > 0 {
            output.push('\n');
        }
        push_row(&mut output, row, &mut written)?;
        if row_index == 0 {
            output.push('\n');
            push_delimiter_row(&mut output, &node.alignments);
        }
    }
    Ok(output)
}

fn write_delimiter_row(alignments: &[TableAlignment]) -> String {
    let mut output = String::new();
    push_delimiter_row(&mut output, alignments);
    output
}

fn push_delimiter_row(output: &mut String, alignments: &[TableAlignment]) {
    output.push_str("| ");
    for (index, alignment) in alignments.iter().enumerate() {
        if index > 0 {
            output.push_str(" | ");
        }
        output.push_str(match alignment {
            TableAlignment::None => "---",
            TableAlignment::Left => ":---",
            TableAlignment::Center => ":---:",
            TableAlignment::Right => "---:",
        });
    }
    output.push_str(" |");
}

fn write_row(row: &TableRow) -> Result<String, SerializeError> {
    let mut output = String::new();
    push_row(&mut output, row, &mut String::new())?;
    Ok(output)
}

/// Pushes `row` as `write_row` writes it, each cell's inlines written in
/// `written` before they are encoded.
fn push_row(
    output: &mut String,
    row: &TableRow,
    written: &mut String,
) -> Result<(), SerializeError> {
    output.push_str("| ");
    for (index, cell) in row.cells.iter().enumerate() {
        if index > 0 {
            output.push_str(" | ");
        }
        match one_line(cell.source.as_deref(), true) {
            Some(text) => push_encoded_cell(output, text),
            None => {
                written.clear();
                push_inlines(written, &cell.children, Context::BLOCK)?;
                push_encoded_cell(output, written);
            }
        }
    }
    output.push_str(" |");
    Ok(())
}

/// A cell's inline content, written as anywhere else, encoded as cell
/// source. The parser splits a row at each `|` after no backslash or an even
/// run of them, and reads an odd run before a `|` with one backslash less, so
/// a `\` added before each `|` of the first kind reads back as written. A `|`
/// that the content already puts after an odd run, an escaped `|` in text,
/// reads with one backslash less, still as an escaped `|`; validation rejects
/// the cell values whose meaning that would change.
fn encode_cell(written: &str) -> String {
    let mut output = String::with_capacity(written.len());
    push_encoded_cell(&mut output, written);
    output
}

/// Pushes `written` encoded as cell source, as `encode_cell` returns it.
fn push_encoded_cell(output: &mut String, written: &str) {
    let mut backslashes = 0usize;
    let mut copied = 0;
    for (at, byte) in written.bytes().enumerate() {
        if byte == b'|' && backslashes % 2 == 0 {
            output.push_str(&written[copied..at]);
            output.push('\\');
            copied = at;
        }
        backslashes = if byte == b'\\' { backslashes + 1 } else { 0 };
    }
    output.push_str(&written[copied..]);
}

/// Where inline content is written, as the value encodings of its nodes read
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Context {
    /// In a heading, which a line ending would end.
    heading: bool,
}

impl Context {
    const BLOCK: Self = Self { heading: false };
    const HEADING: Self = Self { heading: true };
}

fn write_inlines(inlines: &[Inline], context: Context) -> Result<String, SerializeError> {
    let mut output = String::new();
    push_inlines(&mut output, inlines, context)?;
    Ok(output)
}

/// Pushes what `write_inlines` writes for `inlines` to `output`; what
/// `output` holds already is no part of the content.
fn push_inlines(
    output: &mut String,
    inlines: &[Inline],
    context: Context,
) -> Result<(), SerializeError> {
    let mut out = InlineOut {
        start: output.len(),
        scanned: output.len(),
        text: core::mem::take(output),
        open_runs: Vec::new(),
    };
    let written = inlines
        .iter()
        .try_for_each(|inline| write_inline(inline, context, &mut out));
    *output = out.text;
    written
}

/// The opening (`opening`) of what `write_inlines` writes for `inlines`:
/// each inline adds to what the ones before it wrote, so the writing stops
/// at the first char that ends the opening, and an inline that opens with
/// one is not written past it.
fn write_opening(inlines: &[Inline], context: Context) -> Result<String, SerializeError> {
    let mut output = InlineOut::default();
    for inline in inlines {
        if let Some(char) = opening_char(inline) {
            output.push(char);
            break;
        }
        let from = output.len();
        write_inline(inline, context, &mut output)?;
        if output[from..].contains(|char| !continues_opening(char)) {
            break;
        }
    }
    let end = opening(&output).len();
    output.truncate(end);
    Ok(output.text)
}

/// The char `write_inline` writes `inline` with first, where its kind and
/// fields fix it and it ends an opening (`opening`) other than at a line
/// ending.
fn opening_char(inline: &Inline) -> Option<char> {
    let char = match inline {
        Inline::Text(node) => node.value.chars().next()?,
        Inline::Escape(_) => '\\',
        Inline::CharacterReference(node) => node.reference.chars().next()?,
        Inline::Emphasis(Emphasis { delimiter, .. }) | Inline::Strong(Strong { delimiter, .. }) => {
            match delimiter {
                EmphasisDelimiter::Asterisk => '*',
                EmphasisDelimiter::Underscore => '_',
            }
        }
        Inline::Delete(_) => '~',
        Inline::Mark(_) => '=',
        Inline::InlineFootnote(_) => '^',
        Inline::Shortcode(_) | Inline::TextDirective(_) => ':',
        Inline::Tag(_) => '#',
        Inline::Code(_) => '`',
        Inline::Link(_) | Inline::LinkReference(_) | Inline::FootnoteReference(_) => '[',
        Inline::Image(_) | Inline::ImageReference(_) => '!',
        Inline::WikiLink(node) if node.embed => '!',
        Inline::WikiLink(_) => '[',
        Inline::Autolink(node) => match node.form {
            AutolinkForm::Angle => '<',
            AutolinkForm::Literal => node.text.chars().next()?,
        },
        Inline::Html(node) => node.value.chars().next()?,
        Inline::Math(_) | Inline::SoftBreak(_) | Inline::LineBreak(_) => return None,
    };
    (!continues_opening(char) && !matches!(char, '\n' | '\r')).then_some(char)
}

/// The Markdown written so far for the inline content of one block, cell, or
/// label, which the parser reads in one inline pass, and the backtick runs in
/// it that a code span's fence must not close.
#[derive(Default)]
struct InlineOut {
    /// The output the content is written to, which may hold text written
    /// before it.
    text: String,
    /// Where the content starts in `text`.
    start: usize,
    /// `open_runs[n]`: a backtick run of length `n` that opens no code span
    /// was written, outside every code span: a later fence of that length
    /// would close it.
    open_runs: Vec<bool>,
    /// How far `text` has been read for `open_runs`.
    scanned: usize,
}

impl InlineOut {
    /// Reads the text written since the last code span for backtick runs. A
    /// backslash escapes the first backtick of a run, which then opens with
    /// the rest of the run.
    fn scan_open_runs(&mut self) {
        let bytes = self.text.as_bytes();
        let mut from = self.scanned;
        loop {
            let start = crate::swar::find_any(bytes, from, [b'`']);
            if start == bytes.len() {
                break;
            }
            // Escaped where an odd number of backslashes, read since the
            // last run or the start of the scan, stand right before it.
            let backslashes = bytes[from..start]
                .iter()
                .rev()
                .take_while(|byte| **byte == b'\\')
                .count();
            let end = start
                + bytes[start..]
                    .iter()
                    .take_while(|byte| **byte == b'`')
                    .count();
            let opening = end - start - backslashes % 2;
            if opening > 0 {
                if self.open_runs.len() <= opening {
                    self.open_runs.resize(opening + 1, false);
                }
                self.open_runs[opening] = true;
            }
            from = end;
        }
        self.scanned = self.text.len();
    }

    fn is_open_run(&self, length: usize) -> bool {
        self.open_runs.get(length).copied().unwrap_or(false)
    }

    /// Writes, with `write`, content that the parser reads whole from its
    /// first char, so no backtick run in it opens a code span.
    fn push_opaque(&mut self, write: impl FnOnce(&mut String)) {
        self.scan_open_runs();
        write(&mut self.text);
        self.scanned = self.text.len();
    }
}

impl core::ops::Deref for InlineOut {
    type Target = String;

    fn deref(&self) -> &String {
        &self.text
    }
}

impl core::ops::DerefMut for InlineOut {
    fn deref_mut(&mut self) -> &mut String {
        &mut self.text
    }
}

fn write_inline(
    inline: &Inline,
    context: Context,
    out: &mut InlineOut,
) -> Result<(), SerializeError> {
    match inline {
        Inline::Text(node) => out.push_str(&node.value),
        Inline::Escape(node) => {
            out.push('\\');
            out.push(node.value);
        }
        Inline::CharacterReference(node) => out.push_str(&node.reference),
        Inline::Emphasis(node) => {
            let delimiter = emphasis_delimiter(node.delimiter);
            write_span(out, delimiter, &node.children, delimiter, context)?;
        }
        Inline::Strong(node) => {
            let delimiter = match node.delimiter {
                EmphasisDelimiter::Asterisk => "**",
                EmphasisDelimiter::Underscore => "__",
            };
            write_span(out, delimiter, &node.children, delimiter, context)?;
        }
        Inline::Delete(node) => write_span(out, "~~", &node.children, "~~", context)?,
        Inline::Mark(node) => write_span(out, "==", &node.children, "==", context)?,
        Inline::InlineFootnote(node) => write_span(out, "^[", &node.children, "]", context)?,
        Inline::Shortcode(node) => {
            out.push(':');
            out.push_str(&node.name);
            out.push(':');
        }
        Inline::Tag(node) => {
            out.push('#');
            out.push_str(&node.name);
        }
        Inline::Code(node) => write_code_span(node, out),
        Inline::Link(node) => {
            write_span(out, "[", &node.children, "](", context)?;
            out.push_opaque(|out| {
                write_resource(
                    out,
                    &node.destination,
                    node.destination_kind,
                    node.title.as_ref(),
                );
            });
        }
        Inline::Autolink(node) => out.push_opaque(|out| match node.form {
            AutolinkForm::Literal => out.push_str(&node.text),
            AutolinkForm::Angle => {
                out.push('<');
                out.push_str(&node.text);
                out.push('>');
            }
        }),
        Inline::Image(node) => {
            write_span(out, "![", &node.alt, "](", context)?;
            out.push_opaque(|out| {
                write_resource(
                    out,
                    &node.destination,
                    node.destination_kind,
                    node.title.as_ref(),
                );
            });
        }
        Inline::LinkReference(node) => {
            write_reference(out, "[", &node.children, node.kind, &node.label, context)?;
        }
        Inline::ImageReference(node) => {
            write_reference(out, "![", &node.alt, node.kind, &node.label, context)?;
        }
        Inline::Html(node) => out.push_opaque(|out| out.push_str(&node.value)),
        Inline::SoftBreak(_) if context.heading => out.push(' '),
        Inline::SoftBreak(_) => {
            // A backslash right before the line ending would make it a hard
            // break; one space between them, which the parser strips, keeps
            // the backslash text.
            if ends_with_unescaped_backslash(out) {
                out.push(' ');
            }
            out.push('\n');
        }
        Inline::LineBreak(node) => match node.kind {
            LineBreakKind::Backslash => out.push_str("\\\n"),
            LineBreakKind::Spaces => out.push_str("  \n"),
        },
        Inline::Math(node) => {
            let math = write_inline_math(node);
            out.push_opaque(|out| out.push_str(&math));
        }
        Inline::FootnoteReference(node) => {
            out.push_str("[^");
            out.push_str(&node.label);
            out.push(']');
        }
        Inline::WikiLink(node) => out.push_opaque(|out| {
            if node.embed {
                out.push('!');
            }
            out.push_str("[[");
            out.push_str(&node.target);
            if node.target != node.label {
                out.push('|');
                out.push_str(&node.label);
            }
            out.push_str("]]");
        }),
        Inline::TextDirective(node) => {
            // The label is read in an inline pass of its own.
            let label = if node.label.is_empty() {
                String::new()
            } else {
                format!("[{}]", write_inlines(&node.label, context)?)
            };
            out.push_opaque(|out| {
                out.push(':');
                out.push_str(&node.name);
                out.push_str(&label);
                out.push_str(&write_attributes(&node.attributes));
            });
        }
    }
    Ok(())
}

/// Whether `text` ends with an odd run of backslashes, whose last one no
/// backslash escapes.
fn ends_with_unescaped_backslash(text: &str) -> bool {
    text.bytes().rev().take_while(|byte| *byte == b'\\').count() % 2 == 1
}

fn emphasis_delimiter(delimiter: EmphasisDelimiter) -> &'static str {
    match delimiter {
        EmphasisDelimiter::Asterisk => "*",
        EmphasisDelimiter::Underscore => "_",
    }
}

/// Writes `open`, `children`, and `close`.
fn write_span(
    out: &mut InlineOut,
    open: &str,
    children: &[Inline],
    close: &str,
    context: Context,
) -> Result<(), SerializeError> {
    out.push_str(open);
    for child in children {
        write_inline(child, context, out)?;
    }
    out.push_str(close);
    Ok(())
}

/// A link's or image's `destination "title")`, after its `](`.
fn write_resource(
    out: &mut String,
    destination: &str,
    kind: LinkDestinationKind,
    title: Option<&Title>,
) {
    push_destination(out, destination, kind);
    if let Some(title) = title {
        out.push(' ');
        push_title(out, title);
    }
    out.push(')');
}

/// What follows a reference's text: nothing, `[]`, or `[label]`.
/// A reference link or image. A full reference writes its text and then its
/// label; a shortcut or collapsed one writes its label as written, which is
/// both its text and its key, so the text it reads back as matches the
/// definition it names.
fn write_reference(
    out: &mut InlineOut,
    open: &str,
    children: &[Inline],
    kind: ReferenceKind,
    label: &str,
    context: Context,
) -> Result<(), SerializeError> {
    match kind {
        ReferenceKind::Full => write_span(out, open, children, "]", context)?,
        ReferenceKind::Shortcut | ReferenceKind::Collapsed => {
            out.push_str(open);
            out.push_str(label);
            out.push(']');
        }
    }
    out.push_opaque(|out| write_reference_kind(out, kind, label));
    Ok(())
}

fn write_reference_kind(out: &mut String, kind: ReferenceKind, label: &str) {
    match kind {
        ReferenceKind::Shortcut => {}
        ReferenceKind::Collapsed => out.push_str("[]"),
        ReferenceKind::Full => {
            out.push('[');
            // A label is matched as written.
            out.push_str(label);
            out.push(']');
        }
    }
}

/// Writes a code span from its value: fenced by the shortest backtick run
/// that neither the value holds nor a run written before it in the same
/// inline content opens, and padded with a space at each end when the value
/// would otherwise lose an end to the fence or to the space stripping.
fn write_code_span(node: &CodeInline, out: &mut InlineOut) {
    let value = &node.value;
    out.scan_open_runs();
    let held = BacktickRuns::of(value);
    // An escaped backtick written just before joins the opening fence into
    // one longer run, which an earlier open run of that length would take as
    // its close.
    let content = &out[out.start..];
    let before = content.len() - content.trim_end_matches('`').len();
    let length = (1..)
        .find(|length| {
            !held.holds(*length)
                && !out.is_open_run(*length)
                && (before == 0 || !out.is_open_run(before + *length))
        })
        .unwrap_or(1);
    let fence = core::iter::repeat_n('`', length);
    out.extend(fence.clone());
    if code_span_needs_padding(value) {
        out.push(' ');
        out.push_str(value);
        out.push(' ');
    } else {
        out.push_str(value);
    }
    out.extend(fence);
    // The span's own runs are matched; what follows is read afresh.
    out.scanned = out.text.len();
}

/// The lengths of the maximal backtick runs a value holds.
struct BacktickRuns {
    /// Bit `n`: a run of `n` backticks, for `n` below 64.
    short: u64,
    /// `long[n]`: a run of `n` backticks, for `n` from 64 on.
    long: Vec<bool>,
}

impl BacktickRuns {
    fn of(value: &str) -> Self {
        let mut runs = Self {
            short: 0,
            long: Vec::new(),
        };
        let mut run = 0;
        for byte in value.bytes() {
            if byte == b'`' {
                run += 1;
            } else {
                runs.add(run);
                run = 0;
            }
        }
        runs.add(run);
        runs
    }

    fn add(&mut self, run: usize) {
        if run < 64 {
            self.short |= 1 << run;
        } else {
            if self.long.len() <= run {
                self.long.resize(run + 1, false);
            }
            self.long[run] = true;
        }
    }

    /// Whether the value holds a maximal run of `length` backticks.
    fn holds(&self, length: usize) -> bool {
        if length < 64 {
            self.short & (1 << length) != 0
        } else {
            self.long.get(length).copied().unwrap_or(false)
        }
    }
}

fn code_span_needs_padding(input: &str) -> bool {
    input.starts_with('`')
        || input.ends_with('`')
        || (input.starts_with(' ') && input.ends_with(' ') && input.chars().any(|char| char != ' '))
}

pub(crate) fn write_inline_math(node: &MathInline) -> String {
    match node.kind {
        MathInlineKind::Code => format!("$`{}`$", node.value),
        // Dollar math is written verbatim behind its exact-length fence.
        MathInlineKind::Dollar { dollars } => {
            let fence = "$".repeat(usize::from(dollars));
            format!("{fence}{}{fence}", node.value)
        }
    }
}

/// A directive's `[label]`, written as the text it keeps if any, or nothing
/// for an empty label.
fn write_directive_label(label: &[Inline], source: Option<&str>) -> Result<String, SerializeError> {
    if let Some(text) = one_line(source, true) {
        return Ok(if text.is_empty() {
            String::new()
        } else {
            format!("[{text}]")
        });
    }
    if label.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("[{}]", write_inlines(label, Context::BLOCK)?))
    }
}

/// The text a leaf keeps, where it can be written in place of its inlines:
/// on one line when `single` holds, as in a cell, a label, or an ATX heading,
/// which a line ending would end.
fn one_line(source: Option<&str>, single: bool) -> Option<&str> {
    source.filter(|text| !single || !text.contains(['\n', '\r']))
}

fn write_attributes(attributes: &[DirectiveAttribute]) -> String {
    if attributes.is_empty() {
        return String::new();
    }
    let mut output = String::from("{");
    for (index, attribute) in attributes.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        match (&*attribute.name, &attribute.value) {
            ("id", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('#');
                output.push_str(value);
            }
            ("class", Some(value)) if is_directive_shorthand_value(value) => {
                output.push('.');
                output.push_str(value);
            }
            (_, Some(value)) => {
                output.push_str(&attribute.name);
                output.push_str("=\"");
                push_title_text(&mut output, value, LinkTitleKind::DoubleQuote);
                output.push('"');
            }
            (_, None) => output.push_str(&attribute.name),
        }
    }
    output.push('}');
    output
}

fn is_directive_shorthand_value(input: &str) -> bool {
    !input.is_empty()
        && input
            .chars()
            .all(|char| char.is_ascii_alphanumeric() || matches!(char, '_' | '-'))
}

/// How `push_escaped` writes a char.
#[derive(Clone, Copy)]
enum Escape {
    /// As it is.
    Keep,
    /// After a backslash.
    Backslash,
    /// As a character reference (`char_reference`).
    Reference,
}

/// Pushes `input`, each char written as `escape` says, and each run of
/// chars it keeps as one piece.
fn push_escaped(output: &mut String, input: &str, escape: impl Fn(char) -> Escape) {
    output.reserve(input.len());
    let mut start = 0;
    for (at, char) in input.char_indices() {
        let escape = escape(char);
        if matches!(escape, Escape::Keep) {
            continue;
        }
        output.push_str(&input[start..at]);
        match escape {
            Escape::Keep => {}
            Escape::Backslash => {
                output.push('\\');
                output.push(char);
            }
            Escape::Reference => output.push_str(&char_reference(char)),
        }
        start = at + char.len_utf8();
    }
    output.push_str(&input[start..]);
}

fn push_destination(output: &mut String, input: &str, kind: LinkDestinationKind) {
    match kind {
        LinkDestinationKind::Omitted if input.is_empty() => {}
        LinkDestinationKind::Angle => {
            output.push('<');
            push_escaped(output, input, |char| match char {
                char if char.is_control() => Escape::Reference,
                '\\' | '<' | '>' => Escape::Backslash,
                _ => Escape::Keep,
            });
            output.push('>');
        }
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted if input.is_empty() => {
            output.push_str("<>");
        }
        LinkDestinationKind::Bare | LinkDestinationKind::Omitted => {
            push_escaped(output, input, |char| match char {
                // A space would end the destination, and `\ ` is no escape.
                char if char.is_control() || char == ' ' => Escape::Reference,
                '(' | ')' | '\\' | '<' | '>' | '&' => Escape::Backslash,
                _ => Escape::Keep,
            });
        }
    }
}

fn push_title(output: &mut String, title: &Title) {
    let (open, close) = match title.kind {
        LinkTitleKind::DoubleQuote => ('"', '"'),
        LinkTitleKind::SingleQuote => ('\'', '\''),
        LinkTitleKind::Paren => ('(', ')'),
    };
    output.push(open);
    push_title_text(output, &title.value, title.kind);
    output.push(close);
}

fn push_title_text(output: &mut String, input: &str, kind: LinkTitleKind) {
    push_escaped(output, input, |char| match char {
        char if char.is_control() => Escape::Reference,
        '\\' | '&' => Escape::Backslash,
        '"' if kind == LinkTitleKind::DoubleQuote => Escape::Backslash,
        '\'' if kind == LinkTitleKind::SingleQuote => Escape::Backslash,
        '(' | ')' if kind == LinkTitleKind::Paren => Escape::Backslash,
        _ => Escape::Keep,
    });
}

/// Prefixes every line of `input` with a container's `prefix`, keeping each
/// line's ending; an empty line takes the prefix without its trailing
/// whitespace, so that no blank line the writer writes ends in whitespace.
fn prefix_container_lines(input: &str, prefix: &str) -> String {
    prefix_lines_with(input, prefix, prefix.trim_end_matches([' ', '\t']))
}

/// Prefixes every line of `input` with `prefix`, keeping each line's ending.
fn prefix_lines(input: &str, prefix: &str) -> String {
    prefix_lines_with(input, prefix, prefix)
}

/// Prefixes every line of `input` with `prefix`, or an empty one with
/// `empty_prefix`, keeping each line's ending.
fn prefix_lines_with(input: &str, prefix: &str, empty_prefix: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut line_start = 0;
    loop {
        let ending = crate::swar::find_any(bytes, line_start, [b'\n', b'\r']);
        let Some(&byte) = bytes.get(ending) else {
            break;
        };
        let end = match byte {
            b'\r' if bytes.get(ending + 1) == Some(&b'\n') => ending + 2,
            _ => ending + 1,
        };
        output.push_str(if ending == line_start {
            empty_prefix
        } else {
            prefix
        });
        output.push_str(&input[line_start..end]);
        line_start = end;
    }
    if line_start < input.len() {
        output.push_str(prefix);
        output.push_str(&input[line_start..]);
    }
    output
}

fn indent_continuation(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    push_indented(&mut output, input, 4);
    output
}

/// Pushes `input` with every line after its first indented by `width`
/// spaces, as a container's continuation; an empty line stays empty. Each
/// line keeps its line ending, a final one included.
fn push_indented(output: &mut String, input: &str, width: usize) {
    output.reserve(input.len());
    let bytes = input.as_bytes();
    let mut line_start = 0;
    let push = |output: &mut String, line: &str, number: usize| {
        if number > 0 && !line.starts_with(['\n', '\r']) {
            output.extend(core::iter::repeat_n(' ', width));
        }
        output.push_str(line);
    };
    let mut number = 0;
    loop {
        let ending = crate::swar::find_any(bytes, line_start, [b'\n', b'\r']);
        let Some(&byte) = bytes.get(ending) else {
            break;
        };
        let end = match byte {
            b'\r' if bytes.get(ending + 1) == Some(&b'\n') => ending + 2,
            _ => ending + 1,
        };
        push(output, &input[line_start..end], number);
        number += 1;
        line_start = end;
    }
    if line_start < input.len() {
        push(output, &input[line_start..], number);
    }
}

fn trim_trailing_newline(input: &str) -> &str {
    input.trim_end_matches('\n').trim_end_matches('\r')
}

fn ends_with_line_ending(input: &str) -> bool {
    input.ends_with('\n') || input.ends_with('\r')
}

/// The fence for a code block's `value` and the columns the block is indented
/// by, so that no line of `value` closes it. A closing fence is up to three
/// spaces, at least as many marker chars, and nothing else but spaces and
/// tabs. The fence keeps `min_len` chars when indenting the block (which the
/// value's lines lose again) moves every closing-like line past three spaces;
/// otherwise it grows past the longest of them.
fn code_block_fence(value: &str, marker: FenceMarker, min_len: usize) -> (String, usize) {
    let char = match marker {
        FenceMarker::Backtick => '`',
        FenceMarker::Tilde => '~',
    };
    let closing_like = |line: &str, length: usize| {
        let indent = line.len() - line.trim_start_matches(' ').len();
        let rest = &line[indent..];
        let run = rest.len() - rest.trim_start_matches(char).len();
        (indent <= 3 && run >= length && rest[run..].trim_matches([' ', '\t']).is_empty())
            .then_some((indent, run))
    };
    let lines = || value.split(['\n', '\r']);
    let least_indent = lines()
        .filter_map(|line| closing_like(line, min_len))
        .map(|(indent, _)| indent)
        .min();
    match least_indent {
        None => (core::iter::repeat_n(char, min_len).collect(), 0),
        Some(indent) if indent > 0 => (core::iter::repeat_n(char, min_len).collect(), 4 - indent),
        Some(_) => {
            let mut length = min_len;
            for line in lines() {
                if let Some((_, run)) = closing_like(line, length) {
                    length = run + 1;
                }
            }
            (core::iter::repeat_n(char, length).collect(), 0)
        }
    }
}

fn block_math_fence(input: &str) -> String {
    let mut length = 2;
    for line in trim_trailing_newline(input).lines() {
        let trimmed = line.trim();
        if trimmed.len() >= 2 && trimmed.chars().all(|char| char == '$') {
            length = length.max(trimmed.len() + 1);
        }
    }
    "$".repeat(length)
}

fn directive_fence(inner: &str) -> String {
    let mut length = 3;
    for line in inner.lines() {
        if let Some(closing) = directive_closing_fence_len(line) {
            length = length.max(closing + 1);
        }
    }
    ":".repeat(length)
}

fn directive_closing_fence_len(line: &str) -> Option<usize> {
    let trimmed = trim_up_to_three_indent_columns(line)?;
    let length = trimmed
        .as_bytes()
        .iter()
        .take_while(|byte| **byte == b':')
        .count();
    if length >= 3 && trimmed[length..].trim().is_empty() {
        Some(length)
    } else {
        None
    }
}

fn trim_up_to_three_indent_columns(input: &str) -> Option<&str> {
    let mut columns = 0usize;
    let mut bytes = 0usize;
    for byte in input.as_bytes() {
        match *byte {
            b' ' => columns += 1,
            b'\t' => columns += 4 - (columns % 4),
            _ => break,
        }
        bytes += 1;
    }
    (columns <= 3).then_some(&input[bytes..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture_inputs, Rng};

    /// `LeafSources` finds the last leaf at a span, whether the entry after
    /// the one found last is that leaf or not, and finds nothing at a span no
    /// leaf has; `KeptSources` sorts leaves given out of order, keeping the
    /// order of leaves at one span.
    #[test]
    fn leaf_sources_find_the_last_leaf_at_a_span() {
        let [a, b, c, d]: [InlineSource; 4] = crate::parse("A\n\nB\n\nC\n\nD")
            .document
            .leaf_sources()
            .into_iter()
            .map(|(_, source)| source)
            .collect::<Vec<_>>()
            .try_into()
            .expect("four paragraphs");
        let entries = [
            (Span::new(0, 3), a),
            (Span::new(3, 3), b),
            (Span::new(3, 3), c),
            (Span::new(5, 9), d),
        ];
        let text = |found: Option<&InlineSource>| found.map(|source| source.text.clone());
        let some = |text: &str| Some(String::from(text));

        // In document order: the entry after the one found last is tried
        // first, and at (3, 3) it is a leaf that a later one replaces.
        let mut sources = LeafSources::owned(KeptSources::new(entries.to_vec()));
        assert_eq!(text(sources.get(Span::new(0, 3))), some("A"));
        assert_eq!(text(sources.get(Span::new(3, 3))), some("C"));
        assert_eq!(text(sources.get(Span::new(5, 9))), some("D"));
        // Out of order, after a later span.
        assert_eq!(text(sources.get(Span::new(0, 3))), some("A"));
        assert_eq!(text(sources.get(Span::new(5, 9))), some("D"));
        assert_eq!(text(sources.get(Span::new(3, 3))), some("C"));
        // No leaf there, between the spans, after them, and at a span that
        // shares a leaf's start.
        assert_eq!(text(sources.get(Span::new(4, 4))), None);
        assert_eq!(text(sources.get(Span::new(9, 9))), None);
        assert_eq!(text(sources.get(Span::new(3, 9))), None);
        assert_eq!(text(sources.get(Span::new(5, 9))), some("D"));

        // Given out of order, the leaves are sorted by span, the ones at
        // (3, 3) in the order given.
        let [a, b, c, d] = entries;
        let mut sources = LeafSources::owned(KeptSources::new(alloc::vec![d, b, a, c]));
        let order: Vec<_> = sources
            .entries
            .iter()
            .map(|(span, source)| (span.start, span.end, source.text.as_str()))
            .collect();
        assert_eq!(order, [(0, 3, "A"), (3, 3, "B"), (3, 3, "C"), (5, 9, "D")]);
        assert_eq!(text(sources.get(Span::new(3, 3))), some("C"));
        assert_eq!(text(sources.get(Span::new(0, 3))), some("A"));
    }

    /// `write_opening`, which stops at the first inline that ends the
    /// opening and reads some inlines' first char from their kind, writes
    /// the opening of what `write_inlines` writes: for every leaf of every
    /// fixture input and its CRLF variant, every suffix of the leaf's inlines,
    /// and the inlines of every inline container in it.
    #[test]
    fn written_openings_are_the_openings_of_the_written_inlines() {
        fn check(inlines: &[Inline], context: Context) {
            for start in 0..inlines.len() {
                let inlines = &inlines[start..];
                let whole =
                    write_inlines(inlines, context).map(|text| String::from(opening(&text)));
                assert_eq!(write_opening(inlines, context), whole, "{inlines:?}");
            }
            for inline in inlines {
                match inline {
                    Inline::Emphasis(node) => check(&node.children, context),
                    Inline::Strong(node) => check(&node.children, context),
                    Inline::Delete(node) => check(&node.children, context),
                    Inline::Mark(node) => check(&node.children, context),
                    Inline::Link(node) => check(&node.children, context),
                    Inline::LinkReference(node) => check(&node.children, context),
                    Inline::Image(node) => check(&node.alt, context),
                    Inline::ImageReference(node) => check(&node.alt, context),
                    Inline::InlineFootnote(node) => check(&node.children, context),
                    Inline::TextDirective(node) => check(&node.label, context),
                    _ => {}
                }
            }
        }
        let mut leaves = 0;
        for input in fixture_inputs() {
            for input in [input.replace('\n', "\r\n"), input] {
                let mut blocks = crate::parse(&input).document.children;
                leaves::visit_mut(&mut blocks, &mut |leaf| {
                    leaves += 1;
                    for context in [Context::BLOCK, Context::HEADING] {
                        check(leaf.inlines, context);
                    }
                });
            }
        }
        assert!(leaves > 10_000, "only {leaves} leaves");
    }

    /// Each predicate that reads a written line through its opening
    /// (`opening`) answers for the opening as for the whole first line, and
    /// does so after a list marker too, as an item's opening is read: over
    /// every line of up to four chars and seeded random lines of chars these
    /// predicates or an opening tell apart.
    #[test]
    fn line_predicates_read_the_opening_as_the_first_line() {
        const CHARS: [char; 14] = [
            ' ', '\t', '-', '*', '+', '`', '~', '_', '#', '1', 'a', '\n', '\r', 'é',
        ];
        type Predicate = fn(&str) -> bool;
        let predicates: [(&str, Predicate); 5] = [
            ("opens_with_padding", opens_with_padding),
            ("opens_fence_like", opens_fence_like),
            ("a `-` break", |line| reads_as_thematic_break('-', line)),
            ("a `*` break", |line| reads_as_thematic_break('*', line)),
            ("a `+` break", |line| reads_as_thematic_break('+', line)),
        ];
        let mut lines = Vec::from([String::new()]);
        let mut grown = Vec::from([String::new()]);
        for _ in 0..4 {
            grown = grown
                .iter()
                .flat_map(|line| CHARS.iter().map(move |char| format!("{line}{char}")))
                .collect();
            lines.extend(grown.iter().cloned());
        }
        let mut rng = Rng(0x0be1);
        for _ in 0..50_000 {
            lines.push(
                (0..rng.below(16))
                    .map(|_| CHARS[rng.below(CHARS.len())])
                    .collect(),
            );
        }
        for text in &lines {
            let first = text.split(['\n', '\r']).next().unwrap_or_default();
            for marker in ["", "- ", "* ", "+ ", "1. ", "- - "] {
                let opening = format!("{marker}{}", opening(text));
                let first = format!("{marker}{first}");
                for (name, predicate) in predicates {
                    assert_eq!(
                        predicate(&opening),
                        predicate(&first),
                        "{name} of {text:?} after {marker:?}"
                    );
                }
            }
        }
    }

    // serialization: Unchanged source is kept
    #[test]
    fn the_read_back_check_names_each_block_that_reads_differently() {
        let blocks = crate::parse("> x\n\n# y\n\nz").document.children;
        assert_eq!(misread(&blocks, "> x\n\n# y\n\nz", &[0, 2]), [0usize; 0]);
        // A block read differently in its place.
        assert_eq!(misread(&blocks, "> w\n\n# y\n\nz", &[0, 2]), [0]);
        assert_eq!(misread(&blocks, "> w\n\n# y\n\nz", &[2]), [0usize; 0]);
        // `z` read into the quote: the block count differs.
        assert_eq!(misread(&blocks, "> x\nz\n\n# y", &[0, 2]), [0, 2]);
    }
}
