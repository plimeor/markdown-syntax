//! Where the text the parser reads came from in the original input.
//!
//! Containers and inline parsing read derived strings: lines with container
//! markers and indentation removed, tabs split into spaces, lines joined with
//! `\n`, and table cells with `\|` read as `|`. A [`SourceMap`] pairs runs of a
//! derived string with the source bytes they were read from, always in
//! original-input coordinates, so a position in any derived string translates
//! to a span of the input without walking the nesting that produced it.

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
    /// The source column each pushed line starts at.
    columns: Vec<usize>,
}

impl DerivedText {
    pub(super) fn map(&self) -> &SourceMap {
        &self.map
    }

    pub(super) fn into_map(self) -> SourceMap {
        self.map
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

    /// Appends `derived`, which `line` read from its text: either a slice of
    /// `line.text`, or the text from byte `from` of `line.text` on with its
    /// leading whitespace expanded (a tab split into spaces), joined to the
    /// previous line.
    pub(super) fn push_line(&mut self, line: &Line<'_>, derived: &str, from: usize) {
        self.join();
        self.columns.push(derived_column(line, derived, from));
        self.append(line, derived, from);
        self.pending_eol = Some(line.eol_source());
    }

    /// The lines of the text, each starting at the source column it was read
    /// from.
    pub(super) fn lines(&self) -> Vec<Line<'_>> {
        let mut lines = super::collect_lines(&self.text, &self.map);
        for (line, column) in lines.iter_mut().zip(&self.columns) {
            line.column = *column;
        }
        lines
    }

    /// Appends `derived` to the current line without a joiner.
    pub(super) fn append(&mut self, line: &Line<'_>, derived: &str, from: usize) {
        let at = self.text.len();
        match slice_offset(line.text, derived) {
            Some(offset) => line.copy_into(&mut self.map, at, offset, offset + derived.len()),
            None => {
                // `derived` expands the whitespace at the start of
                // `line.text[from..]`; the rest is verbatim.
                let raw = &line.text[from..];
                let suffix = common_suffix_len(raw, derived);
                let head = derived.len() - suffix;
                let raw_head_end = from + raw.len() - suffix;
                let source_start = line.source_start(from);
                let source_end = line.source_end(raw_head_end);
                self.map.push(at, head, source_start, source_end);
                line.copy_into(&mut self.map, at + head, raw_head_end, line.text.len());
            }
        }
        self.text.push_str(derived);
    }

    /// Appends `line.text` with `inserted` placed before byte `offset`, joined
    /// to the previous line. The inserted bytes have no source of their own.
    pub(super) fn push_line_with_insertion(
        &mut self,
        line: &Line<'_>,
        offset: usize,
        inserted: &str,
    ) {
        self.join();
        self.columns.push(line.column);
        let at = self.text.len();
        line.copy_into(&mut self.map, at, 0, offset);
        let source = line.source_start(offset);
        self.map.push(at + offset, inserted.len(), source, source);
        line.copy_into(
            &mut self.map,
            at + offset + inserted.len(),
            offset,
            line.text.len(),
        );
        self.text.push_str(&line.text[..offset]);
        self.text.push_str(inserted);
        self.text.push_str(&line.text[offset..]);
        self.pending_eol = Some(line.eol_source());
    }

    /// Ends the last pushed line with `\n`, mapped to the line ending it was
    /// read with, as the lines before it are joined.
    pub(super) fn push_pending_eol(&mut self) {
        self.join();
        self.pending_eol = None;
    }

    /// Drops the spaces and tabs that end the text, with the map runs they
    /// covered.
    pub(super) fn trim_final_whitespace(&mut self) {
        let len = self.text.trim_end_matches([' ', '\t']).len();
        self.text.truncate(len);
        self.map.truncate(len);
    }

    /// Appends text the parser adds that the source does not hold.
    pub(super) fn push_synthetic(&mut self, text: &str) {
        let at = end_of(self.map.segments(), self.text.len());
        self.map.push(self.text.len(), text.len(), at, at);
        self.text.push_str(text);
    }
}

/// The source column `derived` starts at, which `line` read from byte `from`
/// of its text (see [`DerivedText::push_line`]): the column of its offset
/// when it is a slice of the text, or else the column its verbatim tail starts
/// at less the spaces its expanded head writes.
pub(super) fn derived_column(line: &Line<'_>, derived: &str, from: usize) -> usize {
    if let Some(offset) = slice_offset(line.text, derived) {
        return line.column_at(offset);
    }
    let raw = &line.text[from..];
    let suffix = common_suffix_len(raw, derived);
    let head = derived.len() - suffix;
    line.column_at(from + raw.len() - suffix)
        .saturating_sub(head)
}

/// The offset of `slice` inside `text` when it is a borrowed sub-slice of it.
fn slice_offset(text: &str, slice: &str) -> Option<usize> {
    let start = text.as_ptr() as usize;
    let offset = (slice.as_ptr() as usize).checked_sub(start)?;
    (offset + slice.len() <= text.len()).then_some(offset)
}

fn common_suffix_len(a: &str, b: &str) -> usize {
    a.bytes()
        .rev()
        .zip(b.bytes().rev())
        .take_while(|(x, y)| x == y)
        .count()
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
                Inline::Autolink(node) => self.meta(&mut node.meta),
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
