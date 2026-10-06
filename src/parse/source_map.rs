//! Where the text the parser reads came from in the original input.
//!
//! Inline parsing reads derived strings: a block's lines from where its
//! containers leave them, joined with `\n`, and table cells with `\|` read as
//! `|`. A [`SourceMap`] pairs runs of a derived string with the source bytes
//! they were read from, always in original-input coordinates, so a position in
//! a derived string translates to a span of the input.

use alloc::{string::String, vec::Vec};

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
#[derive(Clone, Copy, Debug)]
pub(super) struct Segment {
    text_start: usize,
    text_end: usize,
    source_start: usize,
    source_end: usize,
}

impl Segment {
    pub(super) fn text_start(&self) -> usize {
        self.text_start
    }

    pub(super) fn text_end(&self) -> usize {
        self.text_end
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
#[derive(Clone, Debug, Default)]
pub(super) struct SourceMap {
    segments: Vec<Segment>,
}

impl SourceMap {
    /// A string read verbatim from `source_start` on.
    pub(super) fn verbatim(len: usize, source_start: usize) -> Self {
        // An empty string still keeps where it sits in the input.
        Self {
            segments: alloc::vec![Segment {
                text_start: 0,
                text_end: len,
                source_start,
                source_end: source_start + len,
            }],
        }
    }

    pub(super) fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// Records that text `text_start..text_start + text_len` was read from
    /// `source_start..source_end`. Runs are pushed in text order.
    fn push(&mut self, text_start: usize, text_len: usize, source_start: usize, source_end: usize) {
        if text_len == 0 {
            return;
        }
        let segment = Segment {
            text_start,
            text_end: text_start + text_len,
            source_start,
            source_end,
        };
        if let Some(last) = self.segments.last_mut() {
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
        self.segments.push(segment);
    }

    /// Forgets what the map holds for text from `len` on.
    fn truncate(&mut self, len: usize) {
        while self
            .segments
            .last()
            .is_some_and(|last| last.text_start >= len)
        {
            self.segments.pop();
        }
        if let Some(last) = self.segments.last_mut() {
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

/// A derived string built line by line, with its source map. Lines are joined
/// with `\n`; a joiner stands for the line ending of the line before it.
#[derive(Clone, Debug, Default)]
pub(super) struct DerivedText {
    pub(super) text: String,
    map: SourceMap,
    /// The source range of the line ending the next joiner stands for.
    pending_eol: Option<(usize, usize)>,
}

impl DerivedText {
    pub(super) fn map(&self) -> &SourceMap {
        &self.map
    }

    fn join(&mut self) {
        if !self.text.is_empty() || self.pending_eol.is_some() {
            let (start, end) = self.pending_eol.unwrap_or_else(|| {
                let at = end_of(self.map.segments(), self.text.len());
                (at, at)
            });
            self.map.push(self.text.len(), 1, start, end);
            self.text.push('\n');
        }
    }

    /// Appends `slice`, a sub-slice of `line.text`, joined to the previous
    /// line.
    pub(super) fn push_line(&mut self, line: &Line<'_>, slice: &str) {
        self.join();
        self.append(line, slice);
        self.pending_eol = Some(line.eol_source());
    }

    /// Appends `slice`, a sub-slice of `line.text`, to the current line. An
    /// empty string adds nothing, wherever it points; other text that is no
    /// sub-slice is kept, read from the whole line.
    pub(super) fn append(&mut self, line: &Line<'_>, slice: &str) {
        let Some(offset) = slice_offset(line.text, slice) else {
            debug_assert!(slice.is_empty(), "a slice of the line");
            if !slice.is_empty() {
                let (start, end) = (line.source_start(0), line.source_end(line.text.len()));
                self.append_replacing(slice, start, end);
            }
            return;
        };
        line.copy_into(&mut self.map, self.text.len(), offset, offset + slice.len());
        self.text.push_str(slice);
    }

    /// Drops the spaces and tabs that end the text, with the map runs they
    /// covered.
    pub(super) fn trim_final_whitespace(&mut self) {
        let len = self.text.trim_end_matches([' ', '\t']).len();
        self.text.truncate(len);
        self.map.truncate(len);
    }

    /// Appends `text`, read from the whole source range `source_start..
    /// source_end`, which it replaces.
    pub(super) fn append_replacing(&mut self, text: &str, source_start: usize, source_end: usize) {
        self.map
            .push(self.text.len(), text.len(), source_start, source_end);
        self.text.push_str(text);
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

/// Maps spans with a cursor that only moves forward: in preorder, node starts
/// never decrease, and each end is found by searching on from its start.
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
                Inline::Html(node) => self.meta(&mut node.meta),
                Inline::Math(node) => self.meta(&mut node.meta),
                Inline::FootnoteReference(node) => self.meta(&mut node.meta),
                Inline::WikiLink(node) => self.meta(&mut node.meta),
                Inline::MdxExpression(node) => self.meta(&mut node.meta),
                Inline::MdxJsx(node) => self.meta(&mut node.meta),
                Inline::Emphasis(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Strong(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Underline(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Delete(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Insert(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Mark(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Subscript(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Superscript(node) => self.container(&mut node.meta, &mut node.children),
                Inline::Spoiler(node) => self.container(&mut node.meta, &mut node.children),
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
