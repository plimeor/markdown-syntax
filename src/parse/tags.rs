//! The tag pass: finds `#name` in the inlines of one leaf after delimiter
//! pairing, since whether a `*` is a delimiter decides whether the text on
//! both sides of it is read as one.
//!
//! The pass reads the inlines as one run of characters: text as written, a
//! line break as `\n`, the content of emphasis-like spans in place of the
//! span, the content of a link, image, inline footnote, or text directive
//! between two opaque characters, and any other inline as one opaque
//! character. A `#` opens a tag when the character before it is whitespace
//! or the start of the run, and a Unicode letter follows it. A tag lies in
//! one text node: it ends at the first character it cannot hold or where the
//! text ends.

use alloc::{string::String, vec::Vec};

use crate::ast::{Inline, NodeMeta, Tag, Text};
use crate::span::Span;
use crate::unicode_letters::{is_letter, is_tag_name_char};

/// Stands in for an inline whose content is not read: not whitespace, so a
/// `#` after it opens no tag.
const OPAQUE: char = '\u{FFFC}';

/// The pieces split text nodes become while a leaf's inlines are walked,
/// kept from one leaf to the next.
#[derive(Default)]
pub(super) struct TagBuffers {
    /// The pieces of each split text node, in order.
    pieces: Vec<Inline>,
    /// Each split text node: its index in its list, how many pieces it
    /// splits into, and where the stripped spaces and tabs it leaves in no
    /// piece start.
    splits: Vec<(usize, usize, Option<usize>)>,
}

impl TagBuffers {
    /// Whether no split is pending: true between leaves.
    pub(super) fn is_empty(&self) -> bool {
        self.pieces.is_empty() && self.splits.is_empty()
    }
}

/// Splits the text nodes of `inlines`, the inlines of one leaf read from
/// `input`, around each tag they hold. Spans are in `input`'s coordinates.
pub(super) fn split_tags(input: &str, inlines: &mut Vec<Inline>, buffers: &mut TagBuffers) {
    if !input.contains('#') {
        return;
    }
    let mut before = None;
    walk(input, inlines, &mut before, buffers);
}

/// `before` is the last character read, or `None` at the start of the run.
fn walk(
    input: &str,
    inlines: &mut Vec<Inline>,
    before: &mut Option<char>,
    buffers: &mut TagBuffers,
) {
    // Each split text node and its pieces, applied in one rebuild of the
    // list, so a leaf with many tags costs no more than its length. A list
    // nested in this one takes its own splits off the buffers before this
    // one goes on.
    let (first_split, first_piece) = (buffers.splits.len(), buffers.pieces.len());
    for (index, inline) in inlines.iter_mut().enumerate() {
        match inline {
            Inline::Text(text) => {
                let pieces = buffers.pieces.len();
                let unheld = split_text(input, text, before, &mut buffers.pieces);
                if buffers.pieces.len() > pieces {
                    buffers
                        .splits
                        .push((index, buffers.pieces.len() - pieces, unheld));
                }
            }
            Inline::SoftBreak(_) | Inline::LineBreak(_) => *before = Some('\n'),
            Inline::Emphasis(node) => walk(input, &mut node.children, before, buffers),
            Inline::Strong(node) => walk(input, &mut node.children, before, buffers),
            Inline::Delete(node) => walk(input, &mut node.children, before, buffers),
            Inline::Mark(node) => walk(input, &mut node.children, before, buffers),
            Inline::Link(node) => opaque_edges(input, &mut node.children, before, buffers),
            Inline::LinkReference(node) => opaque_edges(input, &mut node.children, before, buffers),
            Inline::Image(node) => opaque_edges(input, &mut node.alt, before, buffers),
            Inline::ImageReference(node) => opaque_edges(input, &mut node.alt, before, buffers),
            Inline::InlineFootnote(node) => {
                opaque_edges(input, &mut node.children, before, buffers);
            }
            Inline::TextDirective(node) => opaque_edges(input, &mut node.label, before, buffers),
            Inline::Escape(_)
            | Inline::CharacterReference(_)
            | Inline::Shortcode(_)
            | Inline::Code(_)
            | Inline::Autolink(_)
            | Inline::Html(_)
            | Inline::Math(_)
            | Inline::FootnoteReference(_)
            | Inline::WikiLink(_)
            | Inline::Tag(_) => *before = Some(OPAQUE),
        }
    }
    if buffers.splits.len() == first_split {
        return;
    }
    let count = inlines.len()
        + buffers.splits[first_split..]
            .iter()
            .map(|(_, pieces, _)| pieces - 1)
            .sum::<usize>();
    let nodes = core::mem::replace(inlines, Vec::with_capacity(count));
    let mut pieces = buffers.pieces.drain(first_piece..);
    let mut splits = buffers.splits.drain(first_split..).peekable();
    // Where the stripped spaces and tabs a split left in no piece start; the
    // soft break after them reads them, as it does when no text holds them.
    let mut unheld = None;
    for (index, mut inline) in nodes.into_iter().enumerate() {
        if let Some(start) = unheld.take() {
            match &mut inline {
                Inline::SoftBreak(node) => {
                    node.meta.span = node.meta.span.map(|span| Span::new(start, span.end));
                }
                _ => debug_assert!(false, "stripped spaces end at a soft break"),
            }
        }
        match splits.next_if(|(at, _, _)| *at == index) {
            Some((_, count, start)) => {
                inlines.extend(pieces.by_ref().take(count));
                unheld = start;
            }
            None => inlines.push(inline),
        }
    }
}

fn opaque_edges(
    input: &str,
    inlines: &mut Vec<Inline>,
    before: &mut Option<char>,
    buffers: &mut TagBuffers,
) {
    *before = Some(OPAQUE);
    walk(input, inlines, before, buffers);
    *before = Some(OPAQUE);
}

/// Pushes onto `pieces` the text and tag nodes `text` splits into, or
/// nothing when it holds no tag, and returns where the bytes after its last
/// tag start when they are only the stripped spaces and tabs its span holds
/// past its value; `before` moves past its last character either way.
fn split_text(
    input: &str,
    text: &Text,
    before: &mut Option<char>,
    pieces: &mut Vec<Inline>,
) -> Option<usize> {
    let value = text.value.as_str();
    let bytes = value.as_bytes();
    let first_piece = pieces.len();
    let mut offsets = None;
    // The value offset and the input offset where the next text piece starts.
    let mut piece_start = 0;
    let mut cursor = text.meta.span.map(|span| span.start);
    let mut hash = crate::swar::find_any(bytes, 0, [b'#']);
    while hash < bytes.len() {
        let previous = match hash {
            0 => *before,
            _ => value[..hash].chars().next_back(),
        };
        let opens = previous.is_none_or(char::is_whitespace)
            && value[hash + 1..].chars().next().is_some_and(is_letter);
        if !opens {
            hash = crate::swar::find_any(bytes, hash + 1, [b'#']);
            continue;
        }
        let offsets = offsets.get_or_insert_with(|| Offsets::new(input, text));
        let mut end = value.len();
        for (offset, next) in value[hash + 1..].char_indices() {
            let next_index = hash + 1 + offset;
            if !is_tag_name_char(next) || !offsets.adjacent(next_index) {
                end = next_index;
                break;
            }
        }
        let tag = offsets.span(hash, end);
        if piece_start < hash {
            pieces.push(Inline::Text(Text {
                meta: NodeMeta::new(cursor.zip(tag).map(|(at, tag)| Span::new(at, tag.start))),
                value: String::from(&value[piece_start..hash]),
            }));
        }
        pieces.push(Inline::Tag(Tag {
            meta: NodeMeta::new(tag),
            name: String::from(&value[hash + 1..end]),
        }));
        piece_start = end;
        cursor = tag.map(|tag| tag.end);
        hash = crate::swar::find_any(bytes, end, [b'#']);
    }
    if let Some(last) = value.chars().next_back() {
        *before = Some(last);
    }
    if pieces.len() == first_piece {
        return None;
    }
    if piece_start == value.len() {
        return cursor
            .zip(text.meta.span)
            .and_then(|(at, span)| (at < span.end).then_some(at));
    }
    pieces.push(Inline::Text(Text {
        meta: NodeMeta::new(
            cursor
                .zip(text.meta.span)
                .map(|(at, span)| Span::new(at, span.end)),
        ),
        value: String::from(&value[piece_start..]),
    }));
    None
}

/// Where the bytes of a text node's value sit in the input. A value is its
/// span's slice, except that trailing spaces and tabs before a line break
/// are left out of it, and an angle-bracket autolink that link text demotes
/// to text leaves out its brackets: the value is the slice with some `<`,
/// `>`, space, and tab chars removed, none of which a tag holds.
struct Offsets {
    start: Option<usize>,
    /// The input offset of each byte of the value, when the value is not a
    /// prefix of its slice.
    map: Option<Vec<usize>>,
}

impl Offsets {
    fn new(input: &str, text: &Text) -> Self {
        let span = text.meta.span;
        let map = span.and_then(|span| {
            let slice = &input[span.start..span.end];
            if slice.starts_with(text.value.as_str()) {
                return None;
            }
            let mut map = Vec::with_capacity(text.value.len());
            let mut written = slice.char_indices();
            for char in text.value.chars() {
                let found = written.find(|&(_, read)| read == char);
                debug_assert!(
                    found.is_some(),
                    "a text value is its slice with chars removed: {:?} in {slice:?}",
                    text.value
                );
                let at = found.map_or(slice.len(), |(at, _)| at);
                map.extend((0..char.len_utf8()).map(|byte| span.start + at + byte));
            }
            Some(map)
        });
        Self {
            start: span.map(|span| span.start),
            map,
        }
    }

    /// Whether the byte at `offset` follows the byte before it in the input.
    fn adjacent(&self, offset: usize) -> bool {
        self.map
            .as_ref()
            .is_none_or(|map| map[offset] == map[offset - 1] + 1)
    }

    /// The input span of the value's bytes `from..to`, which are adjacent.
    fn span(&self, from: usize, to: usize) -> Option<Span> {
        let start = self.start?;
        Some(match &self.map {
            None => Span::new(start + from, start + to),
            Some(map) => Span::new(map[from], map[to - 1] + 1),
        })
    }
}
