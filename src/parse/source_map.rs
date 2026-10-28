//! Where the text the parser reads came from in the original input.
//!
//! Inline parsing reads derived strings: a block's lines from where its
//! containers leave them, joined with `\n`, and table cells with `\|` read as
//! `|`. A [`SourceMap`] pairs runs of a derived string with the source bytes
//! they were read from, always in original-input coordinates, so a position in
//! a derived string translates to a span of the input.

use alloc::{borrow::Cow, string::String, vec::Vec};
use core::ops::Range;

use super::Line;
use crate::{
    ast::{Inline, NodeMeta},
    diagnostic::Diagnostic,
    span::Span,
};

/// A run of derived text and the source bytes it was read from. When the two
/// lengths are equal the run maps byte for byte; otherwise every text byte of
/// the run stands for the whole source range (the spaces split from a tab, the
/// `\n` that joins lines ending in `\r\n`, the `|` read from `\|`, or a byte the
/// parser inserted, whose source range is empty).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Segment {
    text_start: usize,
    text_end: usize,
    source_start: usize,
    source_end: usize,
}

impl Segment {
    pub(super) fn text_end(&self) -> usize {
        self.text_end
    }

    pub(super) fn source_start(&self) -> usize {
        self.source_start
    }

    fn verbatim(&self) -> bool {
        self.text_end - self.text_start == self.source_end - self.source_start
    }

    /// The source position where a node starting at `position` (within this
    /// segment's text) starts.
    fn start_at(&self, position: usize) -> usize {
        if self.verbatim() {
            self.source_start + (position - self.text_start)
        } else {
            self.source_start
        }
    }

    /// The source position where a node ending at `position` (within this
    /// segment's text) ends.
    fn end_at(&self, position: usize) -> usize {
        if self.verbatim() {
            self.source_start + (position - self.text_start)
        } else {
            self.source_end
        }
    }
}

/// The source start of a node starting at `position`: a position on a segment
/// boundary maps through the segment that begins there.
pub(super) fn start_of(segments: &[Segment], position: usize) -> usize {
    match segments.iter().find(|segment| position < segment.text_end) {
        Some(segment) => segment.start_at(position.max(segment.text_start)),
        None => segments
            .last()
            .map_or(0, |segment| segment.end_at(segment.text_end)),
    }
}

/// The source end of a node ending at `position`: a position on a segment
/// boundary maps through the segment that ends there.
pub(super) fn end_of(segments: &[Segment], position: usize) -> usize {
    match segments
        .iter()
        .rev()
        .find(|segment| position > segment.text_start)
    {
        Some(segment) => segment.end_at(position.min(segment.text_end)),
        None => segments
            .first()
            .map_or(0, |segment| segment.start_at(segment.text_start)),
    }
}

/// The segments of a derived string, in text order, covering it without gaps.
#[derive(Clone, Default)]
pub(crate) struct SourceMap {
    segments: Segments,
}

/// A map's segments. Most leaves read their text in one run, whose map then
/// holds its one segment without allocating.
#[derive(Clone, Default)]
enum Segments {
    #[default]
    None,
    One(Segment),
    Many(Vec<Segment>),
}

/// Two maps are equal where they hold the same runs, however they hold them.
impl PartialEq for SourceMap {
    fn eq(&self, other: &Self) -> bool {
        self.segments() == other.segments()
    }
}

impl core::fmt::Debug for SourceMap {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SourceMap")
            .field("segments", &self.segments())
            .finish()
    }
}

impl SourceMap {
    /// A string read verbatim from `source_start` on.
    pub(super) fn verbatim(len: usize, source_start: usize) -> Self {
        // An empty string still keeps where it sits in the input.
        Self {
            segments: Segments::One(Segment {
                text_start: 0,
                text_end: len,
                source_start,
                source_end: source_start + len,
            }),
        }
    }

    /// The runs of the `len` bytes of text the map maps, in text order: each
    /// a range of the text and the source range it was read from. They cover
    /// the text from 0 without gaps, each holding text unless it is the only
    /// one. A map without segments maps every position to itself, as one run
    /// read from `0..len` does.
    ///
    /// A run whose two ranges are as long maps byte for byte; in any other,
    /// a position in the run's text maps to its source range's start as a
    /// span's start and to its end as a span's end. A span's start maps
    /// through the run holding it, or to the last run's source end at the
    /// text's end; a span's end through the first run ending at or after it,
    /// or, at 0, to the first run's source start; and the end is kept from
    /// falling below the start (`translate_span`).
    pub(crate) fn runs(
        &self,
        len: usize,
    ) -> impl Iterator<Item = (Range<usize>, Range<usize>)> + '_ {
        let segments = self.segments();
        debug_assert!(
            segments.first().is_none_or(|first| first.text_start == 0)
                && segments.last().is_none_or(|last| last.text_end == len)
                && segments
                    .windows(2)
                    .all(|pair| pair[0].text_start < pair[0].text_end
                        && pair[0].text_end == pair[1].text_start
                        && pair[1].text_start < pair[1].text_end),
            "runs that cover the text"
        );
        let unmapped = segments.is_empty().then_some((0..len, 0..len));
        segments
            .iter()
            .map(|segment| {
                (
                    segment.text_start..segment.text_end,
                    segment.source_start..segment.source_end,
                )
            })
            .chain(unmapped)
    }

    pub(super) fn segments(&self) -> &[Segment] {
        match &self.segments {
            Segments::None => &[],
            Segments::One(segment) => core::slice::from_ref(segment),
            Segments::Many(segments) => segments,
        }
    }

    fn last_mut(&mut self) -> Option<&mut Segment> {
        match &mut self.segments {
            Segments::None => None,
            Segments::One(segment) => Some(segment),
            Segments::Many(segments) => segments.last_mut(),
        }
    }

    fn pop(&mut self) {
        match &mut self.segments {
            Segments::None => {}
            Segments::One(_) => self.segments = Segments::None,
            Segments::Many(segments) => {
                segments.pop();
            }
        }
    }

    /// Records that text `text_start..text_start + text_len` was read from
    /// `source_start..source_end`. Runs are pushed in text order, each where
    /// the text the map holds so far ends.
    pub(super) fn push(
        &mut self,
        text_start: usize,
        text_len: usize,
        source_start: usize,
        source_end: usize,
    ) {
        if text_len == 0 {
            return;
        }
        debug_assert_eq!(
            self.segments().last().map_or(0, |last| last.text_end),
            text_start,
            "runs that cover the text from 0 without gaps"
        );
        let segment = Segment {
            text_start,
            text_end: text_start + text_len,
            source_start,
            source_end,
        };
        if let Some(last) = self.last_mut() {
            if last.verbatim()
                && segment.verbatim()
                && last.text_end == segment.text_start
                && last.source_end == segment.source_start
            {
                last.text_end = segment.text_end;
                last.source_end = segment.source_end;
                return;
            }
        }
        match &mut self.segments {
            Segments::None => self.segments = Segments::One(segment),
            Segments::One(first) => {
                let mut segments = Vec::with_capacity(4);
                segments.extend([*first, segment]);
                self.segments = Segments::Many(segments);
            }
            Segments::Many(segments) => segments.push(segment),
        }
    }

    /// Forgets what the map holds for text from `len` on.
    fn truncate(&mut self, len: usize) {
        while self
            .segments()
            .last()
            .is_some_and(|last| last.text_start >= len)
        {
            self.pop();
        }
        if let Some(last) = self.last_mut() {
            if last.text_end > len {
                if last.verbatim() {
                    last.source_end -= last.text_end - len;
                }
                last.text_end = len;
            }
        }
    }

    /// Records that text from `text_start` on repeats what `segments` map for
    /// their text `from..to`.
    pub(super) fn copy(&mut self, text_start: usize, segments: &[Segment], from: usize, to: usize) {
        let mut covered = from;
        for segment in segments {
            if segment.text_end <= from || segment.text_start >= to {
                continue;
            }
            let start = segment.text_start.max(from);
            let end = segment.text_end.min(to);
            let (source_start, source_end) = if segment.verbatim() {
                (segment.start_at(start), segment.end_at(end))
            } else {
                (segment.source_start, segment.source_end)
            };
            self.push(
                text_start + (start - from),
                end - start,
                source_start,
                source_end,
            );
            covered = end;
        }
        if covered < to {
            // Text the segments do not cover (none is expected) stays anchored
            // at the end of what they do.
            let at = end_of(segments, covered);
            self.push(text_start + (covered - from), to - covered, at, at);
        }
    }
}

/// A derived string and its source map.
///
/// The text borrows the input its lines were split from while it is one run
/// of it, as a leaf's content most often is, and is copied once it is not.
#[derive(Clone, Debug)]
pub(super) struct DerivedText<'a> {
    text: Cow<'a, str>,
    map: SourceMap,
}

impl Default for DerivedText<'_> {
    fn default() -> Self {
        Self {
            text: Cow::Borrowed(""),
            map: SourceMap::default(),
        }
    }
}

impl<'a> DerivedText<'a> {
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) fn map(&self) -> &SourceMap {
        &self.map
    }

    pub(super) fn into_parts(self) -> (String, SourceMap) {
        (self.text.into_owned(), self.map)
    }

    /// Appends `slice`, a sub-slice of `line.text`. An empty string adds
    /// nothing, wherever it points; other text that is no sub-slice is kept,
    /// read from the whole line.
    pub(super) fn append(&mut self, line: &Line<'a>, slice: &'a str) {
        self.append_in("", 0, line, slice);
    }

    /// Appends `slice` as `append` does, borrowed from `input`, the string
    /// `line` was split from, when the text so far ends where it starts; a
    /// copy has room for `room` bytes.
    fn append_in(&mut self, input: &'a str, room: usize, line: &Line<'a>, slice: &'a str) {
        let Some(offset) = slice_offset(line.text, slice) else {
            debug_assert!(slice.is_empty(), "a slice of the line");
            if !slice.is_empty() {
                let (start, end) = (line.source_start(0), line.source_end(line.text.len()));
                self.append_replacing(slice, start, end);
            }
            return;
        };
        line.copy_into(&mut self.map, self.text.len(), offset, offset + slice.len());
        self.push(input, slice, room);
    }

    /// Drops the spaces and tabs that end the text, with the map runs they
    /// covered.
    pub(super) fn trim_final_whitespace(&mut self) {
        let len = self.text.trim_end_matches([' ', '\t']).len();
        match &mut self.text {
            Cow::Borrowed(text) => *text = &text[..len],
            Cow::Owned(text) => text.truncate(len),
        }
        self.map.truncate(len);
    }

    /// Appends `text`, read from the whole source range `source_start..
    /// source_end`, which it replaces.
    pub(super) fn append_replacing(&mut self, text: &str, source_start: usize, source_end: usize) {
        self.map
            .push(self.text.len(), text.len(), source_start, source_end);
        self.push_copied(text, 0);
    }

    /// Appends `piece`, borrowed from `input` when the text so far is a run
    /// of it that ends where `piece` starts. A copy has room for `room`
    /// bytes.
    fn push(&mut self, input: &'a str, piece: &'a str, room: usize) {
        if piece.is_empty() {
            return;
        }
        if let Cow::Borrowed(text) = self.text {
            if text.is_empty() {
                self.text = Cow::Borrowed(piece);
                return;
            }
            if let (Some(start), Some(at)) = (slice_offset(input, text), slice_offset(input, piece))
            {
                if start + text.len() == at {
                    self.text = Cow::Borrowed(&input[start..at + piece.len()]);
                    return;
                }
            }
        }
        self.push_copied(piece, room);
    }

    /// Appends a copy of `piece`; a first copy has room for `room` bytes.
    fn push_copied(&mut self, piece: &str, room: usize) {
        if let Cow::Borrowed(text) = self.text {
            let mut copy = String::with_capacity(room.max(text.len() + piece.len()));
            copy.push_str(text);
            self.text = Cow::Owned(copy);
        }
        if let Cow::Owned(text) = &mut self.text {
            text.push_str(piece);
        }
    }
}

/// The derived text of lines joined with `\n`, built a line at a time; a
/// joiner stands for the line ending of the line before it.
pub(super) struct JoinedLines<'a> {
    text: DerivedText<'a>,
    /// The source range of the line ending the next joiner stands for, and
    /// that line ending.
    pending_eol: Option<(usize, usize, &'a str)>,
    /// The string the lines were split from.
    input: &'a str,
    /// The bytes the text is expected to reach, for its copy.
    room: usize,
}

impl<'a> JoinedLines<'a> {
    /// No lines yet, of lines split from `input` whose text is expected to
    /// reach `len` bytes.
    pub(super) fn new(input: &'a str, len: usize) -> Self {
        Self {
            text: DerivedText::default(),
            pending_eol: None,
            input,
            room: len,
        }
    }

    /// Appends `slice`, a sub-slice of `line.text`, joined to the previous
    /// line.
    pub(super) fn push_line(&mut self, line: &Line<'a>, slice: &'a str) {
        let text = &mut self.text;
        if !text.text.is_empty() || self.pending_eol.is_some() {
            let (start, end, eol) = self.pending_eol.unwrap_or_else(|| {
                let at = end_of(text.map.segments(), text.text.len());
                (at, at, "")
            });
            text.map.push(text.text.len(), 1, start, end);
            // A `\n` ending the line before is the joiner as the input holds
            // it.
            text.push(self.input, if eol == "\n" { eol } else { "\n" }, self.room);
        }
        text.append_in(self.input, self.room, line, slice);
        let (start, end) = line.eol_source();
        self.pending_eol = Some((start, end, line.eol));
    }

    /// The text of the lines.
    pub(super) fn finish(self) -> DerivedText<'a> {
        self.text
    }
}

/// The offset of `slice` inside `text` when it is a borrowed sub-slice of it.
fn slice_offset(text: &str, slice: &str) -> Option<usize> {
    let start = text.as_ptr() as usize;
    let offset = (slice.as_ptr() as usize).checked_sub(start)?;
    (offset + slice.len() <= text.len()).then_some(offset)
}

/// Translates spans that an inline parse produced in its input's coordinates
/// into original-input spans, with one walk over the nodes it produced and the
/// diagnostics it pushed.
pub(super) fn translate_inlines(
    map: &SourceMap,
    inlines: &mut [Inline],
    diagnostics: &mut [Diagnostic],
) {
    let mut translator = Translator {
        segments: map.segments(),
        cursor: 0,
    };
    translator.inlines(inlines);
    // Diagnostics arrive in scan order, so their starts mostly increase too.
    translator.cursor = 0;
    for diagnostic in diagnostics {
        if let Some(span) = diagnostic.span {
            diagnostic.span = Some(translator.span(span));
        }
    }
}

/// Maps `tokens`, in order and tiling the text `map` maps, to the source, as
/// `translate_inlines` maps a node's span.
pub(super) fn translate_tokens(map: &SourceMap, tokens: &mut [crate::parse::InlineToken]) {
    let mut translator = Translator {
        segments: map.segments(),
        cursor: 0,
    };
    for token in tokens {
        token.span = translator.span(token.span);
    }
}

/// The source span of `span`, a span in the text `map` maps, as
/// `translate_inlines` maps a node's.
pub(super) fn translate_span(map: &SourceMap, span: Span) -> Span {
    let segments = map.segments();
    if let [_] = segments {
        // A text read in one run, as most leaves are: where `Translator::span`
        // lands on its only segment, without its searches.
        let start = start_of(segments, span.start);
        let end = end_of(segments, span.end);
        return Span::new(start, end.max(start));
    }
    let cursor = segments
        .partition_point(|segment| segment.text_end <= span.start)
        .min(segments.len().saturating_sub(1));
    Translator { segments, cursor }.span(span)
}

/// Maps spans with a cursor that only moves forward: in preorder, node starts
/// never decrease, and each end is found by searching on from its start.
///
/// The WebAssembly binding tabulates how a leaf's positions map from its
/// `SourceMap::runs`, as they describe the mapping here.
struct Translator<'a> {
    segments: &'a [Segment],
    cursor: usize,
}

impl Translator<'_> {
    fn span(&mut self, span: Span) -> Span {
        if self.segments.is_empty() {
            return span;
        }
        if span.start < self.segments[self.cursor].text_start {
            // Out of order: find the segment again.
            self.cursor = self
                .segments
                .partition_point(|segment| segment.text_end <= span.start)
                .min(self.segments.len() - 1);
        }
        while self.cursor + 1 < self.segments.len()
            && span.start >= self.segments[self.cursor].text_end
        {
            self.cursor += 1;
        }
        let start = start_of(&self.segments[self.cursor..], span.start);
        let last = self.end_segment(span.end);
        let end = end_of(&self.segments[self.cursor..=last], span.end);
        Span::new(start, end.max(start))
    }

    /// The first segment at or after the cursor that `end` falls within, found
    /// by galloping: diagnostics do not nest, and many run to the input's end,
    /// so walking every segment they cross would make the pass quadratic.
    fn end_segment(&self, end: usize) -> usize {
        let last = self.segments.len() - 1;
        if end > self.segments[last].text_start {
            return last;
        }
        let covers = |index: usize| end <= self.segments[index].text_end;
        let mut low = self.cursor;
        let mut step = 1;
        while !covers(low) {
            let next = (low + step).min(last);
            if covers(next) {
                let found =
                    self.segments[low + 1..=next].partition_point(|segment| end > segment.text_end);
                return low + 1 + found;
            }
            low = next;
            step *= 2;
        }
        low
    }

    fn meta(&mut self, meta: &mut NodeMeta) {
        if let Some(span) = meta.span {
            meta.span = Some(self.span(span));
        }
    }

    fn inlines(&mut self, inlines: &mut [Inline]) {
        for inline in inlines {
            match inline {
                Inline::Text(node) => self.meta(&mut node.meta),
                Inline::Escape(node) => self.meta(&mut node.meta),
                Inline::CharacterReference(node) => self.meta(&mut node.meta),
                Inline::SoftBreak(node) => self.meta(&mut node.meta),
                Inline::LineBreak(node) => self.meta(&mut node.meta),
                Inline::Shortcode(node) => self.meta(&mut node.meta),
                Inline::Code(node) => self.meta(&mut node.meta),
                Inline::Autolink(node) => self.meta(&mut node.meta),
                Inline::Html(node) => self.meta(&mut node.meta),
                Inline::Math(node) => self.meta(&mut node.meta),
                Inline::FootnoteReference(node) => self.meta(&mut node.meta),
                Inline::WikiLink(node) => self.meta(&mut node.meta),
                Inline::Tag(node) => self.meta(&mut node.meta),
                Inline::Emphasis(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Strong(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Delete(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Mark(node) => self.container(&mut node.meta, &mut node.children),
                Inline::InlineFootnote(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Link(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Image(node) => self.container(&mut node.meta, &mut node.alt),
                Inline::LinkReference(node) => self.container(&mut node.meta, &mut node.children),
                Inline::ImageReference(node) => self.container(&mut node.meta, &mut node.alt),
                Inline::TextDirective(node) => self.container(&mut node.meta, &mut node.label),
            }
        }
    }

    fn container(&mut self, meta: &mut NodeMeta, children: &mut [Inline]) {
        self.meta(meta);
        self.inlines(children);
    }
}
